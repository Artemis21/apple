#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
use std::fmt::Display;

use crate::{
    Environment, Error, Expr, ResultExt, SExpr, Span, Symbol, SymbolRef, TExpr, Target, Type,
    TypeContext, cause, error, type_expr,
};

#[derive(Clone, Copy, Debug)]
pub enum Keyword {
    Let,
    Fn,
    For,
    If,
    Block,
    Tuple,
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
            Self::Let => typeck_let(span, args, env, ctx),
            Self::Fn => typeck_fn(span, args, env, ctx),
            Self::For => typeck_for(span, args, env, ctx),
            Self::If => typeck_if(span, args, env, ctx),
            Self::Block => typeck_block(span, args, env, ctx),
            Self::Tuple => typeck_tuple(span, args, env, ctx),
        }
    }
}

fn typeck_let(
    span: Span,
    args: &[(SExpr, Span)],
    env: &mut Environment,
    ctx: &mut TypeContext,
) -> Result<TExpr, Error> {
    if args.len() != 3 {
        return Err(error!("let takes 3 args: (let target type value)").with_span(span));
    }
    let sym_target = parse_target(&args[0])?;
    let value = type_expr(&args[2], env, ctx)?;
    let target = env.unpack_generalise_define(sym_target, value.type_, ctx)?;
    Ok(TExpr {
        type_: ctx.const_type(Type::unit()),
        expr: Box::new(Expr::Define(target, value)),
        span,
    })
}

fn typeck_fn(
    span: Span,
    args: &[(SExpr, Span)],
    env: &mut Environment,
    ctx: &mut TypeContext,
) -> Result<TExpr, Error> {
    if args.len() != 4 {
        return Err(error!("fn takes 4 args: (fn name params type value)").with_span(span));
    }
    let (name, _) = parse_symbol(&args[0])?;
    let sym_params = parse_param_list(&args[1])?;
    env.push();
    let (params, param_tys) = sym_params
        .iter()
        .map(|target| env.fresh_unpack_define(target.clone(), ctx))
        .unzip();
    let body = type_expr(&args[3], env, ctx)?;
    let captures = env.pop();
    let func_ty = ctx.const_type(Type::Function(param_tys, body.type_));
    let fn_id = env.define_symbol(name, ctx.generalise(func_ty, env));
    let lambda = TExpr {
        type_: func_ty,
        expr: Box::new(Expr::Lambda {
            params,
            captures,
            body,
        }),
        span,
    };
    Ok(TExpr {
        type_: ctx.const_type(Type::unit()),
        expr: Box::new(Expr::Define(Target::Symbol(fn_id), lambda)),
        span,
    })
}

fn typeck_for(
    span: Span,
    args: &[(SExpr, Span)],
    env: &mut Environment,
    ctx: &mut TypeContext,
) -> Result<TExpr, Error> {
    if args.len() != 3 {
        return Err(error!("for takes 3 args: (for target iter body)").with_span(span));
    }
    let sym_target = parse_target(&args[0])?;
    let iter = type_expr(&args[1], env, ctx)?;
    // TODO: iteration over non-arrays? scoping?
    let (target, elem_ty) = env.fresh_unpack_define(sym_target, ctx);
    ctx.unify_with_concrete(iter.type_, Type::Array(elem_ty))
        .error_cause(cause!(Some(span), "iteration must be over an array"))?;
    let body = type_expr(&args[2], env, ctx)?;
    Ok(TExpr {
        type_: ctx.const_type(Type::unit()),
        expr: Box::new(Expr::For {
            target,
            elem_ty,
            iter,
            body,
        }),
        span,
    })
}

fn typeck_if(
    span: Span,
    args: &[(SExpr, Span)],
    env: &mut Environment,
    ctx: &mut TypeContext,
) -> Result<TExpr, Error> {
    if args.len() != 3 {
        return Err(error!("if takes 3 args: (if cond then else)").with_span(span));
    }
    let cond = type_expr(&args[0], env, ctx)?;
    ctx.unify_with_concrete(cond.type_, Type::Bool)
        .error_cause(cause!(Some(span), "if condition must be of type bool"))?;
    let then = type_expr(&args[1], env, ctx)?;
    let else_ = type_expr(&args[2], env, ctx)?;
    ctx.unify(then.type_, else_.type_)
        .error_cause(cause!(Some(span), "if branches must be of the same type"))?;
    Ok(TExpr {
        type_: then.type_,
        expr: Box::new(Expr::If { cond, then, else_ }),
        span,
    })
}

fn typeck_block(
    span: Span,
    args: &[(SExpr, Span)],
    env: &mut Environment,
    ctx: &mut TypeContext,
) -> Result<TExpr, Error> {
    // TODO: scoping. clone env?
    let lines = args
        .iter()
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

fn typeck_tuple(
    span: Span,
    args: &[(SExpr, Span)],
    env: &mut Environment,
    ctx: &mut TypeContext,
) -> Result<TExpr, Error> {
    let components = args
        .iter()
        .map(|arg| type_expr(arg, env, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TExpr {
        type_: ctx.const_type(Type::Tuple(components.iter().map(|c| c.type_).collect())),
        expr: Box::new(Expr::Tuple(components)),
        span,
    })
}

fn parse_param_list((expr, span): &(SExpr, Span)) -> Result<Vec<Target<Symbol>>, Error> {
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

fn parse_target((expr, span): &(SExpr, Span)) -> Result<Target<Symbol>, Error> {
    let target = match expr {
        SExpr::Symbol(sym) if sym == "_" => Target::Ignore,
        SExpr::Symbol(sym) => Target::Symbol(sym.clone()),
        SExpr::List(exprs) => Target::Unpack(
            exprs.iter().map(parse_target).collect::<Result<_, _>>()?,
            *span,
        ),
        _ => return Err(error!("bad target (must be symbol or list)").with_span(*span)),
    };
    Ok(target)
}

fn parse_symbol((expr, span): &(SExpr, Span)) -> Result<(Symbol, Span), Error> {
    if let SExpr::Symbol(sym) = expr {
        Ok((sym.clone(), *span))
    } else {
        Err(error!("expected symbol").with_span(*span))
    }
}
