use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Write},
    iter::zip,
};

use crate::{Environment, Error, error};

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
    pub const fn unit() -> Self {
        Self::Tuple(vec![])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeRef(usize);

impl Display for TypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "t{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct PolyType {
    quantified: HashSet<TypeRef>,
    pub term: TypeRef,
}

impl PolyType {
    pub fn unquantified(term: TypeRef) -> Self {
        Self {
            quantified: HashSet::new(),
            term,
        }
    }
}

#[derive(Clone)]
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
    pub const fn new() -> Self {
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

    pub const fn display(&self, type_: TypeRef) -> DisplayTypeRef<'_> {
        DisplayTypeRef { type_, ctx: self }
    }

    pub const fn display_concrete<'a>(&'a self, type_: &'a Type) -> DisplayType<'a> {
        DisplayType { type_, ctx: self }
    }

    pub const fn display_poly<'a>(&'a self, pt: &'a PolyType) -> DisplayPolytype<'a> {
        DisplayPolytype { pt, ctx: self }
    }

    pub fn get(&self, tr: TypeRef) -> Option<Type> {
        match self.resolve(tr) {
            ResolvedType::Free(_) => None,
            ResolvedType::Bound(ty) => Some(ty),
        }
    }

    pub fn generalise(&self, term: TypeRef, env: &Environment) -> PolyType {
        let bound_vars = env
            .live_types()
            .flat_map(|polytype| FreeVariablesIter {
                ctx: self,
                exclude: polytype.quantified.clone(),
                terms: vec![polytype.term],
            })
            .collect();
        let quantified = FreeVariablesIter {
            ctx: self,
            exclude: bound_vars,
            terms: vec![term],
        }
        .into_iter()
        .collect();
        PolyType { quantified, term }
    }

    pub fn specialise(&mut self, pt: &PolyType) -> TypeRef {
        let fresh = pt.quantified.iter().map(|qt| (*qt, self.fresh())).collect();
        self.instantiate_mapped(pt.term, &fresh)
    }

    fn instantiate_mapped(
        &mut self,
        type_: TypeRef,
        mapping: &HashMap<TypeRef, TypeRef>,
    ) -> TypeRef {
        if let Some(replacement) = mapping.get(&type_) {
            *replacement
        } else {
            match self.variables[type_.0].clone() {
                TypeVar::Substituted(var) => self.instantiate_mapped(var, mapping),
                TypeVar::Bound(Type::Function(params, ret)) => {
                    let params = params
                        .iter()
                        .map(|p| self.instantiate_mapped(*p, mapping))
                        .collect();
                    let ret = self.instantiate_mapped(ret, mapping);
                    self.const_type(Type::Function(params, ret))
                }
                TypeVar::Bound(Type::Tuple(components)) => {
                    let components = components
                        .iter()
                        .map(|c| self.instantiate_mapped(*c, mapping))
                        .collect();
                    self.const_type(Type::Tuple(components))
                }
                TypeVar::Bound(Type::Array(element)) => {
                    let element = self.instantiate_mapped(element, mapping);
                    self.const_type(Type::Array(element))
                }
                TypeVar::Free | TypeVar::Bound(Type::Bool | Type::Natural | Type::Real) => type_,
            }
        }
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
            (Type::Function(params1, ret1), Type::Function(params2, ret2))
                if params1.len() == params2.len() =>
            {
                for (t1, t2) in zip(params1, params2) {
                    self.unify(t1, t2)?;
                }
                self.unify(ret1, ret2)
            }
            (Type::Tuple(components1), Type::Tuple(components2))
                if components1.len() == components2.len() =>
            {
                for (t1, t2) in zip(components1, components2) {
                    self.unify(t1, t2)?;
                }
                Ok(())
            }
            (Type::Array(inner1), Type::Array(inner2)) => self.unify(inner1, inner2),
            (Type::Bool, Type::Bool)
            | (Type::Natural, Type::Natural)
            | (Type::Real, Type::Real) => Ok(()),
            (t1, t2) => Err(error!(
                "couldn't match type {} with {}",
                self.display_concrete(&t1),
                self.display_concrete(&t2)
            )),
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
                "infinite type caused by unifying {} and {}",
                self.display(free_var),
                self.display_concrete(&type_),
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

struct FreeVariablesIter<'a> {
    ctx: &'a TypeContext,
    exclude: HashSet<TypeRef>,
    terms: Vec<TypeRef>,
}

impl Iterator for FreeVariablesIter<'_> {
    type Item = TypeRef;

    fn next(&mut self) -> Option<Self::Item> {
        let term = self.terms.pop()?;
        if self.exclude.contains(&term) {
            return None;
        }
        self.exclude.insert(term); // don't re-visit
        match &self.ctx.variables[term.0] {
            TypeVar::Free => return Some(term),
            TypeVar::Bound(Type::Function(params, ret)) => {
                self.terms.extend(params);
                self.terms.push(*ret);
            }
            TypeVar::Bound(Type::Tuple(components)) => self.terms.extend(components),
            TypeVar::Bound(Type::Bool | Type::Natural | Type::Real) => {}
            TypeVar::Substituted(next_t) | TypeVar::Bound(Type::Array(next_t)) => {
                self.terms.push(*next_t);
            }
        }
        self.next()
    }
}

pub struct DisplayTypeRef<'a> {
    type_: TypeRef,
    ctx: &'a TypeContext,
}

impl Display for DisplayTypeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ctx.resolve(self.type_) {
            ResolvedType::Free(var) => var.fmt(f),
            ResolvedType::Bound(concrete) => self.ctx.display_concrete(&concrete).fmt(f),
        }
    }
}

pub struct DisplayType<'a> {
    type_: &'a Type,
    ctx: &'a TypeContext,
}

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.type_ {
            Type::Function(params, ret) => {
                f.write_char('(')?;
                if let Some((first, rest)) = params.split_first() {
                    self.ctx.display(*first).fmt(f)?;
                    for param in rest {
                        write!(f, ", {}", self.ctx.display(*param))?;
                    }
                }
                write!(f, ") -> {}", self.ctx.display(*ret))
            }
            Type::Tuple(components) => {
                f.write_char('(')?;
                if let Some((first, rest)) = components.split_first() {
                    self.ctx.display(*first).fmt(f)?;
                    for component in rest {
                        write!(f, ", {}", self.ctx.display(*component))?;
                    }
                }
                f.write_char(')')
            }
            Type::Array(element) => {
                write!(f, "[{}]", self.ctx.display(*element))
            }
            Type::Bool => f.write_str("bool"),
            Type::Natural => f.write_str("nat"),
            Type::Real => f.write_str("real"),
        }
    }
}

pub struct DisplayPolytype<'a> {
    pt: &'a PolyType,
    ctx: &'a TypeContext,
}

impl Display for DisplayPolytype<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.pt.quantified.is_empty() {
            f.write_char('<')?;
            let mut first = true;
            for qt in &self.pt.quantified {
                if !first {
                    f.write_str(", ")?;
                }
                qt.fmt(f)?;
                first = false;
            }
            f.write_str("> ")?;
        }
        self.ctx.display(self.pt.term).fmt(f)
    }
}
