#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
mod builtins;
mod environment;
mod errors;
mod keywords;
mod sexpr;
mod values;

use builtins::{Builtin, initial_env};
use environment::{Defn, Environment};
use errors::{Error, error};
use keywords::Keyword;
use sexpr::{SExpr, Span};
use values::{Array, Natural, Real, Scalar, Value};

type Symbol = String;
type SymbolRef = str;

#[derive(Debug, Clone)]
pub struct Function {
    params: Vec<Target>,
    implementation: FunctionImpl,
}

#[derive(Clone, Debug)]
pub enum Target {
    Symbol(Symbol),
    Unpack(Vec<Target>),
    Ignore,
}

#[derive(Debug, Clone)]
pub enum FunctionImpl {
    User((SExpr, Span), Environment),
    Builtin(Builtin),
}

fn main() {
    let src = include_str!("../samples/sgd.ast");
    if let Err(e) = read_eval_print(src) {
        e.display(src);
    }
}

fn read_eval_print(src: &str) -> Result<(), Error> {
    let expr = sexpr::read(src)?;
    let mut env = initial_env();
    let result = eval_expr(&expr, &mut env)?;
    println!("result: {result}");
    Ok(())
}

fn eval_expr((expr, span): &(SExpr, Span), env: &mut Environment) -> Result<Value, Error> {
    match expr {
        SExpr::Real(real) => Ok(Scalar::Real(*real).into_value()),
        SExpr::Natural(nat) => Ok(Scalar::Natural(*nat).into_value()),
        SExpr::Symbol(sym) => eval_call((sym, *span), *span, &[], env),
        SExpr::List(exprs) => {
            if let Some(((SExpr::Symbol(fn_name), fn_name_span), args)) = exprs.split_first() {
                eval_call((fn_name, *fn_name_span), *span, args, env)
            } else {
                Err(error!(
                    *span,
                    "call expression must start with function name"
                ))
            }
        }
    }
}

fn eval_call(
    (fn_name, fn_name_span): (&SymbolRef, Span),
    call_span: Span,
    arg_exprs: &[(SExpr, Span)],
    env: &mut Environment,
) -> Result<Value, Error> {
    if let Some(kw) = Keyword::from_symbol(fn_name) {
        kw.call(arg_exprs, call_span, env)
    } else {
        let args = arg_exprs
            .iter()
            .map(|expr| Ok((eval_expr(expr, env)?, expr.1)))
            .collect::<Result<_, _>>()?;
        eval_defn(env.get(fn_name, fn_name_span)?, call_span, args)
    }
}

fn eval_defn(defn: &Defn, call_span: Span, args: Vec<(Value, Span)>) -> Result<Value, Error> {
    match defn {
        Defn::Let(v) => {
            if args.is_empty() {
                Ok(v.clone())
            } else {
                Err(error!(call_span, "tried to call a non-function value {v}"))
            }
        }
        Defn::Fn(Function {
            params,
            implementation,
        }) => {
            if args.len() != params.len() {
                return Err(error!(
                    call_span,
                    "expected {} args, got {}",
                    params.len(),
                    args.len()
                ));
            }
            match implementation {
                FunctionImpl::Builtin(func) => func.call(args, call_span),
                FunctionImpl::User(body, env) => {
                    let mut env = env.clone();
                    for (target, (arg, arg_span)) in params.iter().zip(args.into_iter()) {
                        // TODO: broadcasting
                        env.assign_let(target.clone(), arg, arg_span)?;
                    }
                    eval_expr(body, &mut env)
                }
            }
        }
    }
}
