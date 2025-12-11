#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
use crate::{Error, Function, Span, Symbol, SymbolRef, Target, Value, error};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Environment(HashMap<Symbol, Defn>);

#[derive(Debug, Clone)]
pub enum Defn {
    Let(Value),
    Fn(Function),
}

impl Environment {
    pub fn assign_let(&mut self, target: Target, val: Value, span: Span) -> Result<(), Error> {
        match target {
            Target::Symbol(name) => {
                self.0.insert(name, Defn::Let(val));
            }
            Target::Ignore => {}
            Target::Unpack(targets) => {
                let Value::Tuple(vals) = val else {
                    return Err(error!(span, "tried to unpack non-tuple {val}"));
                };
                if vals.len() != targets.len() {
                    return Err(error!(
                        span,
                        "unpacking {} elements into {} targets",
                        vals.len(),
                        targets.len()
                    ));
                }
                for (subtarget, subvalue) in targets.into_iter().zip(vals) {
                    self.assign_let(subtarget, subvalue, span.clone())?;
                }
            }
        }
        Ok(())
    }

    pub fn assign_fn(&mut self, name: Symbol, func: Function) {
        self.0.insert(name, Defn::Fn(func));
    }

    pub fn get(&self, name: &SymbolRef, span: Span) -> Result<&Defn, Error> {
        self.0
            .get(name)
            .ok_or_else(|| error!(span, "undefined reference to {name:?}"))
    }
}
