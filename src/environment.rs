#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
use crate::{Error, Span, Symbol, SymbolRef, Target, Type, TypeContext, TypeRef, error};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Environment(HashMap<Symbol, TypeRef>);

impl Environment {
    pub fn assign(
        &mut self,
        target: Target,
        ty: TypeRef,
        ctx: &mut TypeContext,
    ) -> Result<(), Error> {
        // TODO: generalisation
        match target {
            Target::Symbol(name) => self.assign_symbol(name, ty),
            Target::Ignore => {}
            Target::Unpack(targets) => {
                let component_ts = targets
                    .into_iter()
                    .map(|target| {
                        let component = ctx.fresh();
                        self.assign(target, component, ctx)?;
                        Ok(component)
                    })
                    .collect::<Result<_, _>>()?;
                ctx.unify_with_concrete(ty, Type::Tuple(component_ts))?;
            }
        }
        Ok(())
    }

    pub fn assign_symbol(&mut self, symbol: Symbol, ty: TypeRef) {
        self.0.insert(symbol, ty);
    }

    pub fn get(&self, name: &SymbolRef, span: Span) -> Result<TypeRef, Error> {
        self.0
            .get(name)
            .copied()
            .ok_or_else(|| error!("undefined reference to {name:?}").with_span(span))
    }
}
