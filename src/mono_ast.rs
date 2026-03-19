use crate::{DefnId, Span, TypeRef};

#[derive(Debug)]
pub struct TExpr {
    pub type_: TypeRef,
    pub expr: Box<Expr>,
    pub span: Span,
}

#[derive(Debug)]
pub enum Expr {
    Call(Call),
    LetClosure(DefnId, Closure),
    LetLocal(Target, TExpr),
    Reference(Reference),
    For(For),
    If(If),
    Block(Vec<TExpr>),
    Tuple(Vec<TExpr>),
    LiteralReal(f32),
    LiteralNatural(u32),
}

#[derive(Debug)]
pub struct Closure {
    pub type_: TypeRef,
    pub captures: Vec<Reference>,
    pub params: Vec<Target>,
    pub body: TExpr,
    pub quantified: Vec<TypeRef>,
    /// Each element of `instances` has the same length as `quantified`, forming assignments
    /// from quantified to concrete types.
    pub instances: Vec<Vec<TypeRef>>,
}

#[derive(Debug)]
pub struct Call {
    pub callee: TExpr,
    pub args: Vec<TExpr>,
}

impl From<Call> for Box<Expr> {
    fn from(call: Call) -> Self {
        Self::new(Expr::Call(call))
    }
}

#[derive(Debug)]
pub enum Reference {
    Local(DefnId),
    Closure(DefnId, Vec<TypeRef>), // indicates one of the instances
}

impl Reference {
    pub const fn defn_id(&self) -> DefnId {
        match self {
            Self::Local(defn) | Self::Closure(defn, _) => *defn,
        }
    }
}

#[derive(Debug)]
pub struct For {
    pub target: Target,
    pub elem_ty: TypeRef,
    pub iter: TExpr,
    pub body: TExpr,
}

impl From<For> for Box<Expr> {
    fn from(for_: For) -> Self {
        Self::new(Expr::For(for_))
    }
}

#[derive(Debug)]
pub struct If {
    pub cond: TExpr,
    pub then: TExpr,
    pub else_: TExpr,
}

impl From<If> for Box<Expr> {
    fn from(if_: If) -> Self {
        Self::new(Expr::If(if_))
    }
}

#[derive(Debug, Clone)]
pub enum Target<Sym = DefnId> {
    Ignore,
    Symbol(Sym),
    Unpack(Vec<Self>, Span),
}
