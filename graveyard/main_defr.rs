mod sexpr;
use sexpr::SExpr;
use std::collections::HashMap;

fn main() {
    let expr = sexpr::read(include_str!("../test.ast"));
    let mut env = Environment(HashMap::new());
    let result = eval_expr(&expr, &mut env);
    println!("result: {result:?}");
}

#[derive(Debug, Clone)]
enum Defn {
    Imm(Value),
    Defr(DefrDefn),
}

#[derive(Debug, Clone)]
struct DefrDefn {
    params: Vec<Target>,
    expr: SExpr,
    env: Environment,
}

#[derive(Clone, Debug)]
enum Target {
    Symbol(Symbol),
    Unpack(Vec<Target>),
    Ignore,
}

#[derive(Debug, Clone)]
enum Value {
    Tuple(Vec<Value>),
    Array(Array),
}

impl Value {
    fn unit() -> Self {
        Self::Tuple(vec![])
    }
}

/// invariant: len(values) == product(dims)
/// invariant: all values should be the same type
#[derive(Debug, Clone)]
struct Array {
    dims: Dimensions,
    values: Vec<Scalar>,
}

#[derive(Debug, Copy, Clone)]
enum Scalar {
    Real(Real),
    Natural(Natural),
    Bool(bool),
}

impl Scalar {
    fn into_value(self) -> Value {
        Value::Array(Array {
            dims: vec![],
            values: vec![self],
        })
    }
}

#[derive(Debug, Clone)]
struct Environment(HashMap<Symbol, (Defn, Unpacking)>);

#[derive(Debug, Clone, Default)]
struct Unpacking(Vec<usize>);

impl Unpacking {
    fn of(&self, mut val: Value) -> Value {
        for i in &self.0 {
            if let Value::Tuple(mut vals) = val {
                // FIXME: also panic if we try to unpack too few?
                assert!(*i < vals.len(), "tried to unpack too many values");
                val = vals.swap_remove(*i);
            } else {
                panic!("tried to unpack non-tuple")
            }
        }
        val
    }
}

impl Environment {
    fn assign_unpacked(&mut self, target: &Target, defn: Defn, unpacking: Unpacking) {
        match target {
            Target::Symbol(name) => {
                self.0.insert(name.clone(), (defn, unpacking));
            }
            Target::Ignore => {}
            Target::Unpack(targets) => {
                for (i, target) in targets.into_iter().enumerate() {
                    let mut unpacking = unpacking.clone();
                    unpacking.0.push(i);
                    self.assign_unpacked(target, defn.clone(), unpacking);
                }
            }
        }
    }

    fn assign(&mut self, target: &Target, defn: Defn) {
        self.assign_unpacked(target, defn, Unpacking::default())
    }
}

type Symbol = String;
type SymbolRef = str;
type Real = f32;
type Natural = u32;
type Dimensions = Vec<Natural>;

fn eval_expr(expr: &SExpr, env: &mut Environment) -> Value {
    match expr {
        SExpr::Real(real) => Scalar::Real(*real).into_value(),
        SExpr::Natural(nat) => Scalar::Natural(*nat).into_value(),
        SExpr::Symbol(sym) => eval_call(sym, &[], env),
        SExpr::List(exprs) => {
            let Some((SExpr::Symbol(fn_name), args)) = exprs.split_first() else {
                panic!("call expression must start with function name")
            };
            eval_call(fn_name, args, env)
        }
    }
}

fn eval_call(fn_name: &SymbolRef, arg_exprs: &[SExpr], env: &mut Environment) -> Value {
    if let Some(builtin) = Builtin::from_symbol(fn_name) {
        builtin.call(arg_exprs, env)
    } else {
        let args = arg_exprs
            .into_iter()
            .map(|expr| DefrDefn {
                params: vec![],
                expr: expr.clone(),
                env: env.clone(),
            })
            .map(Defn::Defr)
            .collect::<Vec<_>>();
        let (defn, unpacking) = env
            .0
            .get(fn_name)
            .expect(&format!("undefined reference {fn_name:?}"));
        let whole_val = eval_defn(defn, args);
        unpacking.of(whole_val)
    }
}

fn eval_defn(defn: &Defn, args: Vec<Defn>) -> Value {
    match defn {
        Defn::Imm(v) => {
            assert!(args.is_empty(), "expected no arguments");
            v.clone()
        }
        Defn::Defr(DefrDefn { params, expr, env }) => {
            assert!(args.len() == params.len(), "wrong number of args passed");
            let mut env = env.clone();
            for (target, arg) in params.iter().zip(args.into_iter()) {
                // TODO: broadcasting
                env.assign(target, arg);
            }
            eval_expr(&expr, &mut env)
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Builtin {
    Let,
    For,
    If,
    Block,
    Tuple,
    SubEq,
    Basic(BuiltinFn),
}

impl Builtin {
    fn from_symbol(sym: &SymbolRef) -> Option<Builtin> {
        match sym {
            "let" => Some(Self::Let),
            "for" => Some(Self::For),
            "if" => Some(Self::If),
            "block" => Some(Self::Block),
            "," => Some(Self::Tuple),
            "-=" => Some(Self::SubEq),
            _ => BuiltinFn::from_symbol(sym).map(Self::Basic),
        }
    }

    fn call(self, args: &[SExpr], env: &mut Environment) -> Value {
        println!("builtin {self:?} called on args {args:?}");
        match self {
            Self::Let => {
                assert!(args.len() == 4); // (let target params _kind value)
                let target = parse_target(&args[0]);
                let defr = DefrDefn {
                    params: parse_param_list(&args[1]),
                    expr: args[3].clone(),
                    env: env.clone(),
                };
                env.assign(&target, Defn::Defr(defr));
                Value::unit()
            }
            Self::For => {
                assert!(args.len() == 3); // (for target iter body)
                let target = parse_target(&args[0]);
                let parent = eval_expr(&args[1], env);
                for child in iter_value(parent) {
                    env.assign(&target, Defn::Imm(child));
                    eval_expr(&args[2], env);
                }
                Value::unit()
            }
            Self::If => {
                assert!(args.len() == 3); // (if cond then else)
                let cond = eval_expr(&args[0], env);
                if let Value::Array(arr) = cond
                    && arr.dims.is_empty()
                    && let Scalar::Bool(b) = arr.values[0]
                {
                    let expr_idx = if b { 1 } else { 2 };
                    eval_expr(&args[expr_idx], env)
                } else {
                    panic!("if condition must be a single boolean")
                }
            }
            Self::Block => {
                if args.is_empty() {
                    Value::unit()
                } else {
                    for expr in &args[..args.len() - 1] {
                        eval_expr(expr, env);
                    }
                    eval_expr(args.last().unwrap(), env)
                }
            }
            Self::Tuple => Value::Tuple(args.into_iter().map(|e| eval_expr(e, env)).collect()),
            Self::SubEq => {
                assert!(args.len() == 2); // (-= name value)
                let name = parse_symbol(&args[0]);
                let lhs = eval_call(&name, &[], env);
                let rhs = eval_expr(&args[1], env);
                let result = BuiltinFn::Sub.call(vec![lhs, rhs]);
                env.assign(&Target::Symbol(name), Defn::Imm(result));
                Value::unit()
            }
            Self::Basic(f) => f.call(args.into_iter().map(|e| eval_expr(e, env)).collect()),
        }
    }
}

fn iter_value(parent: Value) -> Vec<Value> {
    match parent {
        Value::Array(Array { dims, values }) => {
            assert!(!dims.is_empty(), "cannot iterate over scalar");
            let child_size = (values.len() as u32) / dims[0];
            let child_dims = dims[1..].to_vec();
            values
                .chunks(child_size as usize)
                .map(|child_values| {
                    Value::Array(Array {
                        dims: child_dims.clone(),
                        values: child_values.to_vec(),
                    })
                })
                .collect()
        }
        Value::Tuple(vals) => {
            assert!(!vals.is_empty(), "cannot iterate over unit");
            let iters: Vec<_> = vals.into_iter().map(iter_value).collect();
            let len = iters[0].len();
            assert!(
                iters.iter().all(|iter| iter.len() == len),
                "when iterating over a tuple, all components must have the same length"
            );
            (0..len)
                .map(|i| Value::Tuple(iters.iter().map(|iter| iter[i].clone()).collect()))
                .collect()
        }
    }
}

fn parse_param_list(expr: &SExpr) -> Vec<Target> {
    let SExpr::List(params) = expr else {
        panic!("param list must be a list")
    };
    params
        .iter()
        .map(|e| {
            if let SExpr::List(param_args) = e
                && param_args.len() == 2
            {
                parse_target(&param_args[0])
            } else {
                panic!("param must be of the form (target type)")
            }
        })
        .collect()
}

fn parse_target(expr: &SExpr) -> Target {
    match expr {
        SExpr::Symbol(sym) if sym == "_" => Target::Ignore,
        SExpr::Symbol(sym) => Target::Symbol(sym.to_string()),
        SExpr::List(exprs) => Target::Unpack(exprs.iter().map(parse_target).collect()),
        _ => panic!("bad target {expr:?}"),
    }
}

fn parse_symbol(expr: &SExpr) -> Symbol {
    if let SExpr::Symbol(sym) = expr {
        sym.to_string()
    } else {
        panic!("expected symbol, got {expr:?}")
    }
}

#[derive(Clone, Copy, Debug)]
enum BuiltinFn {
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

impl BuiltinFn {
    fn from_symbol(sym: &SymbolRef) -> Option<Self> {
        match sym {
            "normal" => Some(Self::Normal),
            ".." => Some(Self::Range),
            "sum" => Some(Self::Sum),
            "load" => Some(Self::Load),
            "print" => Some(Self::Print),
            "@" => Some(Self::Matmul),
            "+" => Some(Self::Add),
            "-" => Some(Self::Sub),
            "*" => Some(Self::Mul),
            "<" => Some(Self::Lt),
            _ => None,
        }
    }

    fn call(self, args: Vec<Value>) -> Value {
        match (self, &args[..]) {
            (Self::Normal, [Value::Array(mean), Value::Array(stddev)]) => {
                use rand_distr::Distribution;
                let r = rand_distr::Normal::new(get_real(mean), get_real(stddev))
                    .unwrap()
                    .sample(&mut rand::rng());
                Scalar::Real(r).into_value()
            }
            (Self::Range, [Value::Array(from), Value::Array(to)]) => {
                let values: Vec<_> = (get_nat(from)..get_nat(to)).map(Scalar::Natural).collect();
                Value::Array(Array {
                    dims: vec![values.len() as u32],
                    values,
                })
            }
            (Self::Sum, [Value::Array(arr)]) => {
                todo!()
            }
            (Self::Load, []) => {
                // FIXME: implement somehow?
                let x = Array {
                    dims: vec![1, 1],
                    values: vec![Scalar::Real(5.0)],
                };
                Value::Tuple(vec![Value::Array(x.clone()), Value::Array(x)])
            }
            (Self::Print, vals) => {
                println!("OUT: {vals:?}");
                Value::unit()
            }
            (Self::Matmul, [Value::Array(lhs), Value::Array(rhs)]) => {
                todo!()
            }
            (Self::Add, [Value::Array(lhs), Value::Array(rhs)]) => {
                Scalar::Real(get_real(lhs) + get_real(rhs)).into_value()
            }
            (Self::Sub, [Value::Array(lhs), Value::Array(rhs)]) => {
                Scalar::Real(get_real(lhs) - get_real(rhs)).into_value()
            }
            (Self::Mul, [Value::Array(lhs), Value::Array(rhs)]) => {
                Scalar::Real(get_real(lhs) * get_real(rhs)).into_value()
            }
            (Self::Lt, [Value::Array(lhs), Value::Array(rhs)]) => {
                Scalar::Bool(get_real(lhs) < get_real(rhs)).into_value()
            }
            _ => {
                panic!("bad invocation of builtin {self:?}: {args:?}")
            }
        }
    }
}

fn get_real(arr: &Array) -> Real {
    match get_scalar(arr) {
        Scalar::Real(real) => real,
        Scalar::Natural(nat) => nat as f32,
        _ => panic!("expected scalar real, got {arr:?}"),
    }
}

fn get_nat(arr: &Array) -> Natural {
    if let Scalar::Natural(nat) = get_scalar(arr) {
        nat
    } else {
        panic!("expected scalar nat, got {arr:?}")
    }
}

fn get_scalar(arr: &Array) -> Scalar {
    assert!(arr.dims.is_empty(), "expected a scalar");
    arr.values[0].clone()
}
