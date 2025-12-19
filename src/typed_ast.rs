use ariadne::{Label, Report, ReportKind, Source};

use crate::{Span, TypeRef, types::TypeContext};

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
    Unpack(Vec<Self>, Span),
}

type Symbol = String;

impl TExpr {
    pub fn debug_show_types(&self, src: &str, ctx: &mut TypeContext) {
        let mut labels = vec![];
        self.debug_get_labels(&mut labels, ctx);
        Report::build(ReportKind::Advice, 0..src.len())
            .with_labels(labels)
            .finish()
            .eprint(Source::from(src))
            .unwrap();
    }

    fn debug_get_labels(&self, labels: &mut Vec<Label>, ctx: &mut TypeContext) {
        labels.push(
            Label::new(self.span.into()).with_message(format!("{}", ctx.display(self.type_))),
        );
        match self.expr.as_ref() {
            Expr::Call(f, args) => {
                f.debug_get_labels(labels, ctx);
                for arg in args {
                    arg.debug_get_labels(labels, ctx);
                }
            }
            Expr::Reference(_) | Expr::LiteralReal(_) | Expr::LiteralNatural(_) => {}
            Expr::Define(_, val) | Expr::Assign(_, val) | Expr::Lambda(_, val) => {
                val.debug_get_labels(labels, ctx);
            }
            Expr::For(_, iter, body) => {
                iter.debug_get_labels(labels, ctx);
                body.debug_get_labels(labels, ctx);
            }
            Expr::If(cond, then, else_) => {
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
