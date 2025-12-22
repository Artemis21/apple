use crate::{Environment, Type, TypeContext, environment::DefnId, types::TypeRef};

pub fn initial_env(ctx: &mut TypeContext) -> (Environment, Vec<(Builtin, DefnId)>) {
    let mut env = Environment::default();
    let defns = BUILTINS
        .iter()
        .map(|b| {
            let monoty = b.type_(ctx);
            let polyty = ctx.generalise(monoty, &env);
            (*b, env.define_symbol(b.name().to_string(), polyty))
        })
        .collect();
    (env, defns)
}

const BUILTINS: [Builtin; 11] = [
    Builtin::Normal,
    Builtin::Range,
    Builtin::Sum,
    Builtin::Load,
    Builtin::Print,
    Builtin::ToReal,
    Builtin::Matmul,
    Builtin::Add,
    Builtin::Sub,
    Builtin::Mul,
    Builtin::Lt,
];

#[derive(Clone, Copy, Debug)]
pub enum Builtin {
    Normal,
    Range,
    Sum,
    Load,
    Print,
    ToReal,
    Matmul,
    Add,
    Sub,
    Mul,
    Lt,
}

impl Builtin {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Range => "..",
            Self::Sum => "sum",
            Self::Load => "load",
            Self::Print => "print",
            Self::ToReal => "to_real",
            Self::Matmul => "@",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Lt => "<",
        }
    }

    pub fn type_(self, ctx: &mut TypeContext) -> TypeRef {
        let real = ctx.const_type(Type::Real);
        let real_arr = ctx.const_type(Type::Array(real));
        let nat = ctx.const_type(Type::Natural);
        let (params, ret) = match self {
            Self::Normal | Self::Add | Self::Sub => (vec![real, real], real),
            Self::Mul => (vec![nat, nat], nat), // TODO: overloading
            Self::Range => (vec![nat, nat], ctx.const_type(Type::Array(nat))),
            Self::Sum => (vec![real_arr], real),
            Self::Load => (vec![], real_arr),
            Self::Print => (vec![real], ctx.const_type(Type::unit())),
            Self::ToReal => (vec![nat], real),
            Self::Matmul => (vec![real_arr, real_arr], real),
            Self::Lt => (vec![real, real], ctx.const_type(Type::Bool)),
        };
        ctx.const_type(Type::Function(params, ret))
    }
}
