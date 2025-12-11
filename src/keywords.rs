#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
use crate::{
    Builtin, Environment, Error, Function, FunctionImpl, SExpr, Scalar, Span, Symbol, SymbolRef,
    Target, Value, error, eval_call, eval_expr,
};

#[derive(Clone, Copy, Debug)]
pub enum Keyword {
    Let,
    Fn,
    For,
    If,
    Block,
    Tuple,
    SubEq,
}

impl Keyword {
    pub fn from_symbol(sym: &SymbolRef) -> Option<Self> {
        match sym {
            "let" => Some(Self::Let),
            "fn" => Some(Self::Fn),
            "for" => Some(Self::For),
            "if" => Some(Self::If),
            "block" => Some(Self::Block),
            "," => Some(Self::Tuple),
            "-=" => Some(Self::SubEq),
            _ => None,
        }
    }

    pub fn call(
        self,
        args: &[(SExpr, Span)],
        span: Span,
        env: &mut Environment,
    ) -> Result<Value, Error> {
        match self {
            Self::Let => {
                if args.len() != 3 {
                    return Err(error!(span, "let takes 3 args: (let target type value)"));
                }
                let target = parse_target(&args[0])?;
                let value = eval_expr(&args[2], env)?;
                env.assign_let(target, value, span)?;
                Ok(Value::unit())
            }
            Self::Fn => {
                if args.len() != 4 {
                    return Err(error!(span, "fn takes 4 args: (fn name params type value)"));
                }
                let (name, _) = parse_symbol(&args[0])?;
                let func = Function {
                    params: parse_param_list(&args[1])?,
                    implementation: FunctionImpl::User(args[3].clone(), env.clone()),
                };
                env.assign_fn(name, func);
                Ok(Value::unit())
            }
            Self::For => {
                if args.len() != 3 {
                    return Err(error!(span, "for takes 3 args: (let target iter body)"));
                }
                let target = parse_target(&args[0])?;
                let parent = eval_expr(&args[1], env)?;
                for child in iter_value(parent, args[1].1)? {
                    env.assign_let(target.clone(), child, span)?;
                    eval_expr(&args[2], env)?;
                }
                Ok(Value::unit())
            }
            Self::If => {
                if args.len() != 3 {
                    return Err(error!(span, "if takes 3 args: (if cond then else)"));
                }
                let cond = eval_expr(&args[0], env)?;
                if let Value::Array(arr) = cond
                    && let Some(Scalar::Bool(b)) = arr.as_scalar()
                {
                    let expr_idx = if b { 1 } else { 2 };
                    eval_expr(&args[expr_idx], env)
                } else {
                    Err(error!(args[0].1, "if condition must be a single boolean"))
                }
            }
            Self::Block => {
                if args.is_empty() {
                    Ok(Value::unit())
                } else {
                    for expr in &args[..args.len() - 1] {
                        eval_expr(expr, env)?;
                    }
                    eval_expr(args.last().unwrap(), env)
                }
            }
            Self::Tuple => Ok(Value::Tuple(
                args.iter()
                    .map(|e| eval_expr(e, env))
                    .collect::<Result<_, _>>()?,
            )),
            Self::SubEq => {
                if args.len() != 2 {
                    return Err(error!(span, "-= takes 2 args: (-= name value)"));
                }
                let (name, name_span) = parse_symbol(&args[0])?;
                let lhs = eval_call((&name, name_span), name_span, &[], env)?;
                let rhs = eval_expr(&args[1], env)?;
                let result = Builtin::Sub.call(vec![(lhs, name_span), (rhs, args[1].1)], span)?;
                env.assign_let(Target::Symbol(name), result, span)?;
                Ok(Value::unit())
            }
        }
    }
}

fn iter_value(parent: Value, span: Span) -> Result<Vec<Value>, Error> {
    match parent {
        Value::Array(arr) => Ok(arr
            .as_view()
            .children()
            .ok_or_else(|| error!(span, "cannot iterate over a scalar"))?
            .map(|child| Value::Array(child.to_owned()))
            .collect()),
        Value::Tuple(vals) => {
            if vals.is_empty() {
                return Err(error!(span, "cannot iterate over unit"));
            }
            let iters: Vec<Vec<Value>> = vals
                .into_iter()
                .map(|v| iter_value(v, span))
                .collect::<Result<_, _>>()?;
            let len = iters[0].len();
            if iters.iter().any(|iter| iter.len() != len) {
                Err(error!(
                    span,
                    "cannot iterate over tuple with different length elements"
                ))
            } else {
                Ok((0..len)
                    .map(|i| Value::Tuple(iters.iter().map(|iter| iter[i].clone()).collect()))
                    .collect())
            }
        }
    }
}

fn parse_param_list((expr, span): &(SExpr, Span)) -> Result<Vec<Target>, Error> {
    let SExpr::List(params) = expr else {
        return Err(error!(*span, "param list must be a list"));
    };
    params
        .iter()
        .map(|(param_expr, param_span)| {
            if let SExpr::List(param_args) = param_expr
                && param_args.len() == 2
            {
                parse_target(&param_args[0])
            } else {
                Err(error!(
                    *param_span,
                    "param must be of the form (target type)"
                ))
            }
        })
        .collect()
}

fn parse_target((expr, span): &(SExpr, Span)) -> Result<Target, Error> {
    let target = match expr {
        SExpr::Symbol(sym) if sym == "_" => Target::Ignore,
        SExpr::Symbol(sym) => Target::Symbol(sym.to_string()),
        SExpr::List(exprs) => {
            Target::Unpack(exprs.iter().map(parse_target).collect::<Result<_, _>>()?)
        }
        _ => return Err(error!(*span, "bad target (must be symbol or list)")),
    };
    Ok(target)
}

fn parse_symbol((expr, span): &(SExpr, Span)) -> Result<(Symbol, Span), Error> {
    if let SExpr::Symbol(sym) = expr {
        Ok((sym.to_string(), *span))
    } else {
        Err(error!(*span, "expected symbol"))
    }
}
