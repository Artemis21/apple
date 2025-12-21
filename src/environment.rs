use crate::{
    Error, PolyType, ResultExt, Span, Symbol, SymbolRef, Target, Type, TypeContext, TypeRef, cause,
    error,
};
use std::{
    collections::{HashMap, HashSet},
    iter::zip,
};

#[derive(Debug, Default)]
pub struct Environment {
    lower_frames: Vec<Frame>,
    top_frame: Frame,
    symbols: Vec<Symbol>,
}

/// An index into Environment::symbols, uniquely identifying a definition (even
/// in the presence of shadowing).
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct DefnId(usize);

#[derive(Debug, Default)]
pub struct Frame {
    locals: HashMap<Symbol, (DefnId, PolyType)>,
    captures: HashSet<DefnId>,
}

impl Environment {
    pub fn push(&mut self) {
        self.lower_frames.push(std::mem::take(&mut self.top_frame))
    }

    pub fn pop(&mut self) -> Vec<DefnId> {
        std::mem::replace(
            &mut self.top_frame,
            self.lower_frames
                .pop()
                .expect("cannot pop, only one frame left"),
        )
        .captures
        .into_iter()
        .collect()
    }

    pub fn unpack_generalise_define(
        &mut self,
        target: Target<Symbol>,
        ty: TypeRef,
        ctx: &mut TypeContext,
    ) -> Result<Target<DefnId>, Error> {
        match target {
            Target::Symbol(name) => {
                let polyty = ctx.generalise(ty, self);
                Ok(Target::Symbol(self.define_symbol(name, polyty)))
            }
            Target::Ignore => Ok(Target::Ignore),
            Target::Unpack(targets, span) => {
                // NB: unify must happen before we generalise
                let component_ts: Vec<_> = targets.iter().map(|_| ctx.fresh()).collect();
                ctx.unify_with_concrete(ty, Type::Tuple(component_ts.clone()))
                    .error_cause(cause!(
                        Some(span),
                        "unpacked type must be correct-sized tuple"
                    ))?;
                Ok(Target::Unpack(
                    zip(targets, component_ts)
                        .map(|(tgt, cmpt)| self.unpack_generalise_define(tgt, cmpt, ctx))
                        .collect::<Result<_, _>>()?,
                    span,
                ))
            }
        }
    }

    pub fn fresh_unpack_define(
        &mut self,
        target: Target<Symbol>,
        ctx: &mut TypeContext,
    ) -> (Target<DefnId>, TypeRef) {
        let var = ctx.fresh();
        let id_target = match target {
            Target::Symbol(name) => {
                Target::Symbol(self.define_symbol(name, PolyType::unquantified(var)))
            }
            Target::Ignore => Target::Ignore,
            Target::Unpack(targets, span) => {
                let (id_targets, component_ts) = targets
                    .into_iter()
                    .map(|target| self.fresh_unpack_define(target, ctx))
                    .unzip();
                ctx.unify_with_concrete(var, Type::Tuple(component_ts))
                    .expect("unifying fresh var with fresh tuple shouldn't error");
                Target::Unpack(id_targets, span)
            }
        };
        (id_target, var)
    }

    pub fn define_symbol(&mut self, symbol: Symbol, ty: PolyType) -> DefnId {
        let id = DefnId(self.symbols.len());
        self.symbols.push(symbol.clone());
        self.top_frame.locals.insert(symbol, (id, ty));
        id
    }

    pub fn get(&mut self, name: &SymbolRef, span: Span) -> Result<(DefnId, PolyType), Error> {
        let (depth, (id, ty)) = self
            .frames()
            .enumerate()
            .find_map(|(depth, frame)| Some((depth, frame.locals.get(name)?.clone())))
            .ok_or_else(|| error!("undefined reference to {name:?}").with_span(span))?;
        if depth > 0 {
            for frame in self.mut_frames().take(depth - 1) {
                frame.captures.insert(id);
            }
        }
        Ok((id, ty))
    }

    pub fn all_types(&self) -> impl Iterator<Item = &PolyType> {
        self.frames().flat_map(|frame| frame.locals.values()).map(|(_, ty)| ty)
    }

    #[allow(dead_code)]
    pub fn debug_dump(&self, ctx: &mut TypeContext) {
        for frame in self.frames() {
            for (name, (_, type_)) in &frame.locals {
                let monotype = ctx.specialise(type_);
                println!("{name}: {}", ctx.display(monotype));
            }
        }
    }

    fn frames(&self) -> impl Iterator<Item = &Frame> {
        Some(&self.top_frame)
            .into_iter()
            .chain(self.lower_frames.iter())
    }

    fn mut_frames(&mut self) -> impl Iterator<Item = &mut Frame> {
        Some(&mut self.top_frame)
            .into_iter()
            .chain(self.lower_frames.iter_mut())
    }
}
