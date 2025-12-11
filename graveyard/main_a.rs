use std::{collections::HashMap, sync::Arc};

use sexpression::{OwnedExpression as Expr, read};

fn main() {
    let expr = read(include_str!("../sgd.ast")).unwrap().to_owned();
    let mut env = Environment(HashMap::new());
    let result = eval_expr(&expr, &mut env);
    println!("result: {result:?}");
}

struct Nodes(Vec<Node>);

#[derive(Clone, Copy)]
struct NodeId(usize);


impl Nodes {
    fn add(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.0.len());
        self.0.push(node);
        id
    }

    fn add_rec(&mut self, node: impl FnOnce(NodeId) -> Node) -> NodeId {
        let id = NodeId(self.0.len());
        self.0.push(node(id));
        id
    }

    fn add_n_rec<const N: usize>(
        &mut self,
        nodes: impl FnOnce([NodeId; N]) -> [Node; N],
    ) -> [NodeId; N] {
        let ids = std::array::from_fn(|i| NodeId(self.0.len() + i));
        self.0.extend(nodes(ids).into_iter());
        ids
    }

    fn get(&self, id: NodeId) -> &Node {
        &self.0[id.0]
    }
}

enum Node {
    Load(String),

}

#[derive(Debug, Clone)]
struct Defn {
    kind: Kind,
    params: Vec<Param>,
    value: Thunk,
}

#[derive(Clone, Debug)]
struct Param {
    target: Target,
    kind: Kind,
}

#[derive(Clone, Debug)]
enum Target {
    Symbol(Symbol),
    Unpack(Vec<Target>),
    Ignore,
}

#[derive(Debug, Clone)]
enum PartialValue {
    Thunk(Thunk),
    Tuple(Vec<PartialValue>),
    Array(PartialArray),
}

#[derive(Debug, Clone)]
struct PartialArray {
    dims: Dimensions,
    values: Vec<PartialScalar>,
}

#[derive(Debug, Clone)]
enum PartialScalar {
    Thunk(Thunk),
    Scalar(Scalar),
}

#[derive(Debug, Clone)]
struct Thunk {
    env: Arc<Environment>,
    expr: Expr,
}

fn eval_expr(expr: &Expr, env: &mut Environment) -> Value {
    match expr {
        // FIXME: how to parse nats?
        Expr::Number(val) => Scalar::Real(*val as f32).into_value(),
        Expr::Bool(b) => Scalar::Bool(*b).into_value(),
        Expr::Str(_) => panic!("strings not currently implemented"),
        Expr::Symbol(sym) => eval_call(sym, vec![], env),
        Expr::List(exprs) => {
            let Some((Expr::Symbol(fn_name), arg_exprs)) = exprs.split_first() else {
                panic!("call expression must start with function name")
            };
            let arg_env = Arc::new(env.clone());
            let args = arg_exprs
                .into_iter()
                .map(|expr| Thunk {
                    env: arg_env.clone(),
                    expr: expr.clone(),
                })
                .collect::<Vec<_>>();
            eval_call(fn_name, args, env)
        }
        Expr::Null => Value::unit(),
    }
}

fn eval_call(fn_name: &SymbolRef, args: Vec<Thunk>, env: &mut Environment) -> Value {
    if let Some(builtin) = Builtin::from_symbol(fn_name) {
        builtin.call(args, env)
    } else {
        let defn = env
            .0
            .get(fn_name)
            .expect(&format!("undefined reference {fn_name}"));
        eval_defn(defn, args) // XXX: should it get a look at env?
    }
}

fn eval_defn(defn: &Defn, args: Vec<Thunk>) -> Value {
    assert!(
        args.len() == defn.params.len(),
        "wrong number of args passed"
    );
    let mut env = (*defn.value.env).clone();
    for (param, arg) in defn.params.iter().zip(args.into_iter()) {
        // TODO: typechecking, broadcasting
        let defn = Defn {
            kind: param.kind.clone(),
            params: vec![],
            value: arg,
        };
        env.assign(&param.target, defn);
    }
    eval_expr(&defn.value.expr, &mut env)
}

fn eval_thunk(thunk: &Thunk) -> Value {
    let mut env = (*thunk.env).clone();
    eval_expr(&thunk.expr, &mut env)
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

    fn kind(self) -> ScalarKind {
        match self {
            Self::Real(_) => ScalarKind::Real,
            Self::Natural(_) => ScalarKind::Natural,
            Self::Bool(_) => ScalarKind::Bool,
        }
    }
}

#[derive(Clone, Debug)]
enum Kind {
    Tuple(Vec<Kind>),
    Array(ArrayKind),
}

impl Kind {
    fn unit() -> Self {
        Self::Tuple(vec![])
    }
}

#[derive(Clone, Debug)]
struct ArrayKind {
    dims: Dimensions,
    scalar: ScalarKind,
}

#[derive(Clone, Debug)]
enum ScalarKind {
    Real,
    Natural,
    Bool,
}

#[derive(Debug, Clone)]
struct Environment(HashMap<Symbol, Defn>);

impl Environment {
    fn assign(&mut self, target: &Target, defn: Defn) {
        match target {
            Target::Symbol(name) => {
                self.0.insert(name.clone(), defn);
            }
            Target::Ignore => {}
            Target::Unpack(_) => unimplemented!("tuple unpacking"),
        }
    }
}

type Symbol = String;
type SymbolRef = str;
type Real = f32;
type Natural = u32;
type Dimensions = Vec<Natural>;

#[derive(Clone, Copy, Debug)]
enum Builtin {
    // control flow
    Let,
    For,
    Block,
    Tuple,
    // helpers
    Normal,
    Range,
    Zip,
    Sum,
    // IO
    Load,
    Print,
    // basic operators
    Matmul,
    Add,
    Sub,
    Mul,
    Lt,
    // not basic operators (??)
    Deriv,
    SubEq,
}

struct NodeRef {
    node: NodeId,
    tuple_index: Vec<usize>,
}

enum Node {
    ForNode,

}

impl Builtin {
    fn from_symbol(sym: &SymbolRef) -> Option<Builtin> {
        match sym {
            "let" => Some(Self::Let),
            "for" => Some(Self::For),
            "block" => Some(Self::Block),
            "," => Some(Self::Tuple),
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
            "%" => Some(Self::Deriv),
            "-=" => Some(Self::SubEq),
            _ => None,
        }
    }

    fn call(self, args: Vec<Thunk>, env: &mut Environment) -> Value {
        match self {
            Self::Let => {
                assert!(args.len() == 4);
                let target = parse_target(&args[0].expr);
                let defn = Defn {
                    kind: parse_kind(&args[2].expr),
                    params: parse_param_list(&args[1].expr),
                    value: args[3],
                };
                env.assign(&target, defn);
                Value::unit()
            }
            Self::For => {
                assert!(args.len() == 3);
                let target = parse_target(&args[0].expr);
                let iter = eval_expr(&args[1].expr, env);
                match iter {
                    Value::Array(Array { dims, values }) => {
                        assert!(!dims.is_empty(), "cannot iterate over scalar");
                        let scalar_kind = if values.is_empty() {
                            return Value::unit();
                        } else {
                            values[0].kind()
                        };
                        let child_size = (values.len() as u32) / dims[0];
                        let child_dims = dims[1..].to_vec();
                        for child_values in values.chunks(child_size as usize) {
                            let child = Array {
                                dims: child_dims,
                                values: child_values.to_vec(),
                            };
                            env.assign(
                                &target,
                                Defn {
                                    kind: Kind::Array(ArrayKind {
                                        dims: child_dims,
                                        scalar: scalar_kind,
                                    }),
                                    params: vec![],
                                    value: Value::Array(child),
                                },
                            );
                            eval_expr(&args[2].expr, env);
                        }
                    }
                    _ => unimplemented!("tuple iteration"),
                }
                Value::unit()
            }
            Self::Block => {
                if args.is_empty() {
                    Value::unit()
                } else {
                    for thunk in &args[..args.len() - 1] {
                        eval_expr(&thunk.expr, env);
                    }
                    eval_expr(&args.last().unwrap().expr, env)
                }
            }
            Self::Tuple => {}
        }
    }
}

fn parse_kind(expr: &Expr) -> Kind {
    // TODO: only infer if we get `_`, otherwise parse
    infer_kind()
}

/// Placeholder function for when we want to somehow to type inference
fn infer_kind() -> Kind {
    Kind::unit()
}

fn parse_param_list(expr: &Expr) -> Vec<Param> {
    if let Expr::List(params) = expr {
        params.iter().map(parse_param).collect()
    } else {
        panic!("param list must be a list")
    }
}

fn parse_param(expr: &Expr) -> Param {
    if let Expr::List(exprs) = expr
        && let [target_expr, kind_expr] = &exprs[..]
    {
        Param {
            target: parse_target(target_expr),
            kind: parse_kind(kind_expr),
        }
    } else {
        panic!("param declaration must be of the form (name type)")
    }
}

fn parse_target(expr: &Expr) -> Target {
    match expr {
        Expr::Symbol(sym) if sym == "_" => Target::Ignore,
        Expr::Symbol(sym) => Target::Symbol(sym.to_string()),
        Expr::List(exprs) => Target::Unpack(exprs.iter().map(parse_target).collect()),
        _ => panic!("bad target {expr:?}"),
    }
}
