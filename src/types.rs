use crate::{Error, error};

#[derive(Debug, Clone)]
pub enum Type {
    Function(Vec<TypeRef>, TypeRef),
    Tuple(Vec<TypeRef>),
    Array(TypeRef),
    Bool,
    Natural,
    Real,
}

impl Type {
    pub fn unit() -> Self {
        Self::Tuple(vec![])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeRef(usize);

enum TypeVar {
    Free,
    Bound(Type),
    Substituted(TypeRef),
}

enum ResolvedType {
    Free(TypeRef),
    Bound(Type),
}

pub struct TypeContext {
    variables: Vec<TypeVar>,
}

impl TypeContext {
    pub fn new() -> Self {
        Self { variables: vec![] }
    }

    pub fn fresh(&mut self) -> TypeRef {
        let idx = self.variables.len();
        self.variables.push(TypeVar::Free);
        TypeRef(idx)
    }

    pub fn const_type(&mut self, t: Type) -> TypeRef {
        let idx = self.variables.len();
        self.variables.push(TypeVar::Bound(t));
        TypeRef(idx)
    }

    pub fn unify(&mut self, t1: TypeRef, t2: TypeRef) -> Result<(), Error> {
        match (self.resolve(t1), self.resolve(t2)) {
            (ResolvedType::Bound(t1), ResolvedType::Bound(t2)) => self.unify_concrete(t1, t2),
            (ResolvedType::Free(v), t) | (t, ResolvedType::Free(v)) => self.substitute(v, t),
        }
    }

    pub fn unify_with_concrete(&mut self, t1: TypeRef, t2: Type) -> Result<(), Error> {
        match self.resolve(t1) {
            ResolvedType::Bound(t1) => self.unify_concrete(t1, t2),
            ResolvedType::Free(v) => self.substitute(v, ResolvedType::Bound(t2)),
        }
    }

    fn unify_concrete(&mut self, t1: Type, t2: Type) -> Result<(), Error> {
        match (t1, t2) {
            (Type::Function(params1, ret1), Type::Function(params2, ret2)) => {
                for (t1, t2) in params1.iter().zip(params2.iter()) {
                    self.unify(*t1, *t2)?;
                }
                self.unify(ret1, ret2)
            }
            (Type::Tuple(components1), Type::Tuple(components2)) => {
                for (t1, t2) in components1.iter().zip(components2.iter()) {
                    self.unify(*t1, *t2)?;
                }
                Ok(())
            }
            (Type::Array(inner1), Type::Array(inner2)) => self.unify(inner1, inner2),
            (Type::Bool, Type::Bool)
            | (Type::Natural, Type::Natural)
            | (Type::Real, Type::Real) => Ok(()),
            (t1, t2) => Err(error!("couldn't match type {t1:?} with {t2:?}")),
        }
    }

    fn substitute(&mut self, free_var: TypeRef, type_: ResolvedType) -> Result<(), Error> {
        match type_ {
            ResolvedType::Free(v2) if v2 == free_var => Ok(()),
            ResolvedType::Free(v2) => {
                self.variables[free_var.0] = TypeVar::Substituted(v2);
                Ok(())
            }
            ResolvedType::Bound(type_) if self.occurs_in_concrete(free_var, &type_) => Err(error!(
                "infinite type caused by unifying {free_var:?} and {type_:?}"
            )),
            ResolvedType::Bound(type_) => {
                self.variables[free_var.0] = TypeVar::Bound(type_);
                Ok(())
            }
        }
    }

    fn occurs_in_concrete(&self, free_var: TypeRef, type_: &Type) -> bool {
        match type_ {
            Type::Function(params, ret) => {
                params.iter().any(|t| self.occurs_in_ref(free_var, *t))
                    || self.occurs_in_ref(free_var, *ret)
            }
            Type::Tuple(ts) => ts.iter().any(|t| self.occurs_in_ref(free_var, *t)),
            Type::Array(t) => self.occurs_in_ref(free_var, *t),
            Type::Bool | Type::Natural | Type::Real => false,
        }
    }

    fn occurs_in_ref(&self, free_var: TypeRef, type_: TypeRef) -> bool {
        match self.resolve(type_) {
            ResolvedType::Free(variable) => free_var == variable,
            ResolvedType::Bound(type_) => self.occurs_in_concrete(free_var, &type_),
        }
    }

    fn resolve(&self, t_ref: TypeRef) -> ResolvedType {
        match &self.variables[t_ref.0] {
            TypeVar::Free => ResolvedType::Free(t_ref),
            TypeVar::Bound(t) => ResolvedType::Bound(t.clone()),
            TypeVar::Substituted(r) => self.resolve(*r),
        }
    }
}
