use crate::{
    Error, PolyType, ResultExt, Span, Symbol, SymbolRef, Target, Type, TypeContext, TypeRef, cause,
    error,
};
use std::{
    collections::{HashMap, HashSet},
    iter::{once, zip},
};

#[derive(Debug, Default)]
pub struct Environment {
    lower_frames: Vec<Frame>,
    top_frame: Frame,
    pub definitions: Definitions,
}

/// An index into `Environment::definitions`, uniquely identifying a definition
/// (even in the presence of shadowing).
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct DefnId(usize);

#[derive(Debug, Default)]
pub struct Definitions(Vec<(Symbol, PolyType)>);

impl Definitions {
    pub fn get_type(&self, id: DefnId) -> &PolyType {
        &self.0[id.0].1
    }

    fn push(&mut self, symbol: Symbol, ty: PolyType) -> DefnId {
        let id = DefnId(self.0.len());
        self.0.push((symbol, ty));
        id
    }

    pub fn all_types(&self) -> impl Iterator<Item = &PolyType> {
        self.0.iter().map(|(_sym, ty)| ty)
    }
}

#[derive(Debug, Default)]
pub struct Frame {
    locals: HashMap<Symbol, DefnId>,
    captures: HashSet<DefnId>,
}

impl Environment {
    pub fn push(&mut self) {
        self.lower_frames.push(std::mem::take(&mut self.top_frame));
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
        let id = self.definitions.push(symbol.clone(), ty);
        self.top_frame.locals.insert(symbol, id);
        id
    }

    pub fn get(&mut self, name: &SymbolRef, span: Span) -> Result<(DefnId, &PolyType), Error> {
        let (depth, id) = self
            .frames()
            .enumerate()
            .find_map(|(depth, frame)| Some((depth, *frame.locals.get(name)?)))
            .ok_or_else(|| error!("undefined reference to {name:?}").with_span(span))?;
        if depth > 0 {
            for frame in self.mut_frames().take(depth - 1) {
                frame.captures.insert(id);
            }
        }
        Ok((id, self.definitions.get_type(id)))
    }

    #[allow(dead_code)]
    pub fn debug_dump(&self, ctx: &mut TypeContext) {
        for frame in self.frames() {
            for (name, id) in &frame.locals {
                let monotype = ctx.specialise(self.definitions.get_type(*id));
                println!("{name}: {}", ctx.display(monotype));
            }
        }
    }

    fn frames(&self) -> impl Iterator<Item = &Frame> {
        once(&self.top_frame).chain(self.lower_frames.iter())
    }

    fn mut_frames(&mut self) -> impl Iterator<Item = &mut Frame> {
        once(&mut self.top_frame).chain(self.lower_frames.iter_mut())
    }
}
