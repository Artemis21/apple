#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
mod builtins;
mod environment;
mod errors;
mod keywords;
mod sexpr;
mod typed_ast;
mod types;

use builtins::initial_env;
use environment::Environment;
use errors::{Error, ErrorCause, ResultExt, cause, error};
use keywords::Keyword;
use sexpr::{SExpr, Span};
use typed_ast::{Expr, TExpr, Target};
use types::{PolyType, Type, TypeContext, TypeRef};

type Symbol = String;
type SymbolRef = str;

fn main() {
    let src = include_str!("../samples/test.ast");
    if let Err(e) = read_eval_print(src) {
        e.display(src);
    }
}

fn read_eval_print(src: &str) -> Result<(), Error> {
    let expr = sexpr::read(src)?;
    let mut ctx = TypeContext::new();
    let mut env = initial_env(&mut ctx);
    let texpr = type_expr(&expr, &mut env, &mut ctx)?;
    env.debug_dump(&mut ctx);
    texpr.debug_show_types(src, &mut ctx);
    Ok(())
}

fn type_expr(
    &(ref expr, span): &(SExpr, Span),
    env: &mut Environment,
    ctx: &mut TypeContext,
) -> Result<TExpr, Error> {
    match expr {
        SExpr::Real(real) => Ok(TExpr {
            type_: ctx.const_type(Type::Real),
            expr: Box::new(Expr::LiteralReal(*real)),
            span,
        }),
        SExpr::Natural(nat) => Ok(TExpr {
            type_: ctx.const_type(Type::Natural),
            expr: Box::new(Expr::LiteralNatural(*nat)),
            span,
        }),
        SExpr::Symbol(sym) => Ok(TExpr {
            type_: env.get(sym, span, ctx)?,
            expr: Box::new(Expr::Reference(sym.clone())),
            span,
        }),
        SExpr::Keyword(kw) => Err(error!("keyword {kw} found out of context").with_span(span)),
        SExpr::List(exprs) => match exprs.split_first() {
            Some(((SExpr::Keyword(kw), _kw_span), args)) => kw.typeck(span, args, env, ctx),
            Some((func_e, arg_es)) => {
                let func = type_expr(func_e, env, ctx)?;
                let args = arg_es
                    .iter()
                    .map(|arg_e| type_expr(arg_e, env, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                let arg_tys = args.iter().map(|arg| arg.type_).collect();
                let result_ty = ctx.fresh();
                ctx.unify_with_concrete(func.type_, Type::Function(arg_tys, result_ty))
                    .error_cause(cause!(
                        Some(span),
                        "function arguments must match parameters"
                    ))?;
                Ok(TExpr {
                    type_: result_ty,
                    expr: Box::new(Expr::Call(func, args)),
                    span,
                })
            }
            None => Err(error!("empty list not permitted").with_span(span)),
        },
    }
}
