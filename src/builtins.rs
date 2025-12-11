use crate::{
    Array, Environment, Error, Function, FunctionImpl, Natural, Real, Scalar, Span, Target, Value,
    error,
};
use std::fmt::Display;

macro_rules! initial_env {
    ( $( $name:tt -> $variant:ident ( $( $param:expr ),* ) ,)* ) => {{
        let mut env = Environment::default();
        $(
            env.assign_fn($name.to_string(), Function {
                params: vec![ $( Target::Symbol($param.to_string()) ),* ],
                implementation: FunctionImpl::Builtin(Builtin::$variant),
            });
        )*
        env
    }};
}

pub fn initial_env() -> Environment {
    initial_env!(
        "normal" -> Normal("mean", "stddev"),
        ".." -> Range("from", "to"),
        "sum" -> Sum("array"),
        "load" -> Load(),
        "print" -> Print("any..."),
        "@" -> Matmul("lhs", "rhs"),
        "+" -> Add("lhs", "rhs"),
        "-" -> Sub("lhs", "rhs"),
        "*" -> Mul("lhs", "rhs"),
        "<" -> Lt("lhs", "rhs"),
    )
}

#[derive(Clone, Copy, Debug)]
pub enum Builtin {
    Normal,
    Range,
    Sum,
    Load,
    Print,
    Matmul,
    Add,
    Sub,
    Mul,
    Lt,
}

impl Builtin {
    pub fn call(self, args: Vec<(Value, Span)>, span: Span) -> Result<Value, Error> {
        match (self, &args[..]) {
            (Self::Normal, [mean, stddev]) => {
                use rand_distr::Distribution;
                let r = rand_distr::Normal::new(get_real(mean)?, get_real(stddev)?)
                    .unwrap()
                    .sample(&mut rand::rng());
                Ok(Scalar::Real(r).into_value())
            }
            (Self::Range, [from, to]) => Ok(Value::Array(Array::from_vec(
                (get_nat(from)?..get_nat(to)?)
                    .map(Scalar::Natural)
                    .collect(),
            ))),
            (Self::Sum, [_arr]) => {
                todo!()
            }
            (Self::Load, []) => {
                todo!()
            }
            (Self::Print, [(val, _)]) => {
                println!("OUT: {val}");
                Ok(Value::unit())
            }
            (Self::Matmul, [_lhs, _rhs]) => {
                todo!()
            }
            (Self::Add, [lhs, rhs]) => {
                Ok(Scalar::Real(get_real(lhs)? + get_real(rhs)?).into_value())
            }
            (Self::Sub, [lhs, rhs]) => {
                Ok(Scalar::Real(get_real(lhs)? - get_real(rhs)?).into_value())
            }
            (Self::Mul, [lhs, rhs]) => {
                Ok(Scalar::Real(get_real(lhs)? * get_real(rhs)?).into_value())
            }
            (Self::Lt, [lhs, rhs]) => {
                Ok(Scalar::Bool(get_real(lhs)? < get_real(rhs)?).into_value())
            }
            _ => Err(error!(span, "bad invocation of builtin {self}: {args:?}")),
        }
    }
}

impl Display for Builtin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Normal => "normal",
            Self::Range => "..",
            Self::Sum => "sum",
            Self::Load => "load",
            Self::Print => "print",
            Self::Matmul => "@",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Lt => "<",
        };
        f.write_str(name)
    }
}

fn get_real((val, span): &(Value, Span)) -> Result<Real, Error> {
    let Value::Array(arr) = val else {
        return Err(error!(span, "expected scalar real, got tuple"));
    };
    match arr.as_scalar() {
        Some(Scalar::Real(real)) => Ok(real),
        Some(Scalar::Natural(nat)) => Ok(nat as f32),
        _ => Err(error!(span, "expected scalar real, got {arr}")),
    }
}

fn get_nat((val, span): &(Value, Span)) -> Result<Natural, Error> {
    let Value::Array(arr) = val else {
        return Err(error!(span, "expected scalar nat, got tuple"));
    };
    if let Some(Scalar::Natural(nat)) = arr.as_scalar() {
        Ok(nat)
    } else {
        Err(error!(span, "expected scalar nat, got {arr}"))
    }
}
