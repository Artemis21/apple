#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
use crate::{
    Error, PolyType, ResultExt, Span, Symbol, SymbolRef, Target, Type, TypeContext, TypeRef, cause,
    error,
};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Environment(HashMap<Symbol, PolyType>);

impl Environment {
    pub fn assign(
        &mut self,
        target: Target,
        ty: TypeRef,
        ctx: &mut TypeContext,
    ) -> Result<(), Error> {
        match target {
            Target::Symbol(name) => self.assign_symbol(name, ty, ctx),
            Target::Ignore => {}
            Target::Unpack(targets, span) => {
                let component_ts = targets
                    .into_iter()
                    .map(|target| {
                        let component = ctx.fresh();
                        self.assign(target, component, ctx)?;
                        Ok(component)
                    })
                    .collect::<Result<_, _>>()?;
                ctx.unify_with_concrete(ty, Type::Tuple(component_ts))
                    .error_cause(cause!(
                        Some(span),
                        "unpacked type must be correct-sized tuple"
                    ))?;
            }
        }
        Ok(())
    }

    pub fn assign_symbol(&mut self, symbol: Symbol, ty: TypeRef, ctx: &TypeContext) {
        let poly_ty = ctx.generalise(ty, self.0.values());
        self.0.insert(symbol, poly_ty);
    }

    pub fn get(
        &self,
        name: &SymbolRef,
        span: Span,
        ctx: &mut TypeContext,
    ) -> Result<TypeRef, Error> {
        self.0
            .get(name)
            .map(|pt| ctx.specialise(pt))
            .ok_or_else(|| error!("undefined reference to {name:?}").with_span(span))
    }

    pub fn debug_dump(&self, ctx: &mut TypeContext) {
        for (name, type_) in &self.0 {
            let monotype = ctx.specialise(type_);
            println!("{name}: {}", ctx.display(monotype));
        }
    }
}
