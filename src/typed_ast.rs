use std::collections::{HashMap, HashSet};

use ariadne::{Label, Report, ReportKind, Source};

use crate::{DefnId, Span, TypeContext, TypeRef};

#[derive(Debug)]
pub struct TExpr {
    pub type_: TypeRef,
    pub expr: Box<Expr>,
    pub span: Span,
}

#[derive(Debug)]
pub enum Expr {
    Call(Call),
    Reference(Reference),
    Define(Define),
    Lambda(Lambda),
    For(For),
    If(If),
    Block(Vec<TExpr>),
    Tuple(Vec<TExpr>),
    LiteralReal(f32),
    LiteralNatural(u32),
}

#[derive(Debug)]
pub struct Call {
    pub callee: TExpr,
    pub args: Vec<TExpr>,
}

impl From<Call> for Box<Expr> {
    fn from(call: Call) -> Self {
        Self::new(Expr::Call(call))
    }
}

#[derive(Debug)]
pub struct Reference {
    pub defn: DefnId,
    pub specialise: HashMap<TypeRef, TypeRef>, // from quantified to concrete types
}

impl From<Reference> for Box<Expr> {
    fn from(reference: Reference) -> Self {
        Self::new(Expr::Reference(reference))
    }
}

#[derive(Debug)]
pub struct Define {
    pub target: Target,
    pub body: TExpr,
    /// Invariant: if `generalise` is nonempty then `body` is side-effect free
    /// (so we can do monomorphisation).
    pub generalise: HashSet<TypeRef>, // quantified types
}

impl From<Define> for Box<Expr> {
    fn from(define: Define) -> Self {
        Self::new(Expr::Define(define))
    }
}

#[derive(Debug)]
pub struct Lambda {
    pub params: Vec<Target>,
    pub captures: Vec<DefnId>,
    pub body: TExpr,
}

impl From<Lambda> for Box<Expr> {
    fn from(lambda: Lambda) -> Self {
        Self::new(Expr::Lambda(lambda))
    }
}

#[derive(Debug)]
pub struct For {
    pub target: Target,
    pub elem_ty: TypeRef,
    pub iter: TExpr,
    pub body: TExpr,
}

impl From<For> for Box<Expr> {
    fn from(for_: For) -> Self {
        Self::new(Expr::For(for_))
    }
}

#[derive(Debug)]
pub struct If {
    pub cond: TExpr,
    pub then: TExpr,
    pub else_: TExpr,
}

impl From<If> for Box<Expr> {
    fn from(if_: If) -> Self {
        Self::new(Expr::If(if_))
    }
}

#[derive(Debug, Clone)]
pub enum Target<Sym = DefnId> {
    Ignore,
    Symbol(Sym),
    Unpack(Vec<Self>, Span),
}

impl TExpr {
    #[allow(dead_code)]
    pub fn debug_show_types(&self, src: &str, ctx: &mut TypeContext) {
        let mut labels = vec![];
        self.debug_get_labels(&mut labels, ctx);
        Report::build(ReportKind::Advice, 0..src.len())
            .with_labels(labels)
            .finish()
            .eprint(Source::from(src))
            .unwrap();
    }

    #[allow(dead_code)]
    fn debug_get_labels(&self, labels: &mut Vec<Label>, ctx: &mut TypeContext) {
        labels.push(
            Label::new(self.span.into()).with_message(format!("{}", ctx.display(self.type_))),
        );
        match self.expr.as_ref() {
            Expr::Call(Call { callee, args }) => {
                callee.debug_get_labels(labels, ctx);
                for arg in args {
                    arg.debug_get_labels(labels, ctx);
                }
            }
            Expr::Reference(_) | Expr::LiteralReal(_) | Expr::LiteralNatural(_) => {}
            Expr::Define(Define { body, .. }) | Expr::Lambda(Lambda { body, .. }) => {
                body.debug_get_labels(labels, ctx);
            }
            Expr::For(For { iter, body, .. }) => {
                iter.debug_get_labels(labels, ctx);
                body.debug_get_labels(labels, ctx);
            }
            Expr::If(If { cond, then, else_ }) => {
                cond.debug_get_labels(labels, ctx);
                then.debug_get_labels(labels, ctx);
                else_.debug_get_labels(labels, ctx);
            }
            Expr::Block(parts) | Expr::Tuple(parts) => {
                for part in parts {
                    part.debug_get_labels(labels, ctx);
                }
            }
        }
    }
}
