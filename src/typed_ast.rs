use crate::{Span, TypeRef};

#[derive(Debug)]
pub struct TExpr {
    pub type_: TypeRef,
    pub expr: Box<Expr>,
    pub span: Span,
}

#[derive(Debug)]
#[allow(dead_code)] // TODO: actually evaluate/compile
pub enum Expr {
    Call(TExpr, Vec<TExpr>),
    Reference(Symbol),
    Define(Target, TExpr),
    Assign(Symbol, TExpr),
    Lambda(Vec<Target>, TExpr),
    For(Target, TExpr, TExpr),
    If(TExpr, TExpr, TExpr),
    Block(Vec<TExpr>),
    Tuple(Vec<TExpr>),
    LiteralReal(f32),
    LiteralNatural(u32),
}

#[derive(Debug, Clone)]
pub enum Target {
    Ignore,
    Symbol(Symbol),
    Unpack(Vec<Target>),
}

type Symbol = String;
