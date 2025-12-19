#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
use std::fmt::Display;

use crate::{
    Environment, Error, Expr, SExpr, Span, Symbol, SymbolRef, TExpr, Target, Type, TypeContext,
    error, type_expr,
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

impl Display for Keyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Let => "let",
            Self::Fn => "fn",
            Self::For => "for",
            Self::If => "if",
            Self::Block => "block",
            Self::Tuple => ",",
            Self::SubEq => "-=",
        };
        f.write_str(name)
    }
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

    pub fn typeck(
        self,
        span: Span,
        args: &[(SExpr, Span)],
        env: &mut Environment,
        ctx: &mut TypeContext,
    ) -> Result<TExpr, Error> {
        match self {
            Self::Let => {
                if args.len() != 3 {
                    return Err(error!("let takes 3 args: (let target type value)").with_span(span));
                }
                let target = parse_target(&args[0])?;
                let value = type_expr(&args[2], env, ctx)?;
                env.assign(target.clone(), value.type_, ctx)?;
                Ok(TExpr {
                    type_: ctx.const_type(Type::unit()),
                    expr: Box::new(Expr::Define(target, value)),
                    span,
                })
            }
            Self::Fn => {
                if args.len() != 4 {
                    return Err(
                        error!("fn takes 4 args: (fn name params type value)").with_span(span)
                    );
                }
                let (name, _) = parse_symbol(&args[0])?;
                let params = parse_param_list(&args[1])?;
                let mut func_env = env.clone();
                let param_tys = params
                    .iter()
                    .map(|target| {
                        let ty = ctx.fresh();
                        func_env.assign(target.clone(), ty, ctx)?;
                        Ok(ty)
                    })
                    .collect::<Result<_, _>>()?;
                let body = type_expr(&args[3], &mut func_env, ctx)?;
                let func_ty = ctx.const_type(Type::Function(param_tys, body.type_));
                env.assign_symbol(name.clone(), func_ty);
                let lambda = TExpr {
                    type_: func_ty,
                    expr: Box::new(Expr::Lambda(params, body)),
                    span,
                };
                Ok(TExpr {
                    type_: ctx.const_type(Type::unit()),
                    expr: Box::new(Expr::Define(Target::Symbol(name), lambda)),
                    span,
                })
            }
            Self::For => {
                if args.len() != 3 {
                    return Err(error!("for takes 3 args: (for target iter body)").with_span(span));
                }
                let target = parse_target(&args[0])?;
                let iter = type_expr(&args[1], env, ctx)?;
                let mut loop_env = env.clone();
                loop_env.assign(target.clone(), iter.type_, ctx)?;
                let body = type_expr(&args[2], &mut loop_env, ctx)?;
                Ok(TExpr {
                    type_: ctx.const_type(Type::unit()),
                    expr: Box::new(Expr::For(target, iter, body)),
                    span,
                })
            }
            Self::If => {
                if args.len() != 3 {
                    return Err(error!("if takes 3 args: (if cond then else)").with_span(span));
                }
                let cond = type_expr(&args[0], env, ctx)?;
                ctx.unify_with_concrete(cond.type_, Type::Bool)?;
                let then = type_expr(&args[1], env, ctx)?;
                let else_ = type_expr(&args[2], env, ctx)?;
                ctx.unify(then.type_, else_.type_)?;
                Ok(TExpr {
                    type_: then.type_,
                    expr: Box::new(Expr::If(cond, then, else_)),
                    span,
                })
            }
            Self::Block => {
                // TODO: scoping. clone env?
                let lines = args
                    .into_iter()
                    .map(|arg| type_expr(arg, env, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(TExpr {
                    type_: lines
                        .last()
                        .map_or_else(|| ctx.const_type(Type::unit()), |line| line.type_),
                    expr: Box::new(Expr::Block(lines)),
                    span,
                })
            }
            Self::Tuple => {
                let components = args
                    .into_iter()
                    .map(|arg| type_expr(arg, env, ctx))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(TExpr {
                    type_: ctx
                        .const_type(Type::Tuple(components.iter().map(|c| c.type_).collect())),
                    expr: Box::new(Expr::Tuple(components)),
                    span,
                })
            }
            Self::SubEq => {
                if args.len() != 2 {
                    return Err(error!("-= takes 2 args: (-= name value)").with_span(span));
                }
                todo!()
            }
        }
    }
}

fn parse_param_list((expr, span): &(SExpr, Span)) -> Result<Vec<Target>, Error> {
    let SExpr::List(params) = expr else {
        return Err(error!("param list must be a list").with_span(*span));
    };
    params
        .iter()
        .map(|(param_expr, _param_span)| {
            if let SExpr::List(param_args) = param_expr
                && param_args.len() == 2
            {
                parse_target(&param_args[0])
            } else {
                Err(error!("param must be of the form (target type)").with_span(*span))
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
        _ => return Err(error!("bad target (must be symbol or list)").with_span(*span)),
    };
    Ok(target)
}

fn parse_symbol((expr, span): &(SExpr, Span)) -> Result<(Symbol, Span), Error> {
    if let SExpr::Symbol(sym) = expr {
        Ok((sym.to_string(), *span))
    } else {
        Err(error!("expected symbol").with_span(*span))
    }
}
