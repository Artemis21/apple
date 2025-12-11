#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
use std::fmt::{Display, Write};

#[derive(Debug, Clone)]
pub enum Value {
    Tuple(Vec<Value>),
    Array(Array),
}

impl Value {
    pub const fn unit() -> Self {
        Self::Tuple(vec![])
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tuple(vals) => {
                f.write_char('(')?;
                if let Some((first, rest)) = vals.split_first() {
                    first.fmt(f)?;
                    for val in rest {
                        write!(f, ", {val}")?;
                    }
                }
                f.write_char(')')
            }
            Self::Array(arr) => arr.fmt(f),
        }
    }
}

/// invariant: len(values) == product(dims)
/// invariant: all values should be the same type
#[derive(Debug, Clone)]
pub struct Array {
    dims: Vec<Natural>,
    values: Vec<Scalar>,
}

impl Array {
    pub fn from_vec(values: Vec<Scalar>) -> Self {
        Self {
            dims: vec![values.len() as u32],
            values,
        }
    }

    pub fn as_view(&self) -> ArrayView<'_> {
        ArrayView {
            dims: &self.dims,
            values: &self.values,
        }
    }

    pub fn as_scalar(&self) -> Option<Scalar> {
        if self.dims.is_empty() {
            Some(self.values[0])
        } else {
            None
        }
    }
}

impl Display for Array {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_view().fmt(f)
    }
}

pub struct ArrayView<'a> {
    dims: &'a [Natural],
    values: &'a [Scalar],
}

impl<'a> ArrayView<'a> {
    pub fn children(&self) -> Option<impl Iterator<Item = ArrayView<'a>>> {
        if self.dims.is_empty() {
            return None;
        }
        let child_size: Natural = self.dims[1..].iter().product();
        Some(
            self.values
                .chunks(child_size as usize)
                .map(|chunk| ArrayView {
                    dims: &self.dims[1..],
                    values: chunk,
                }),
        )
    }

    pub fn to_owned(&self) -> Array {
        Array {
            dims: self.dims.to_vec(),
            values: self.values.to_vec(),
        }
    }
}

impl Display for ArrayView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(children) = self.children() else {
            return self.values[0].fmt(f);
        };
        f.write_char('[')?;
        let mut is_first = true;
        for child in children {
            if !is_first {
                f.write_str(", ")?;
            }
            child.fmt(f)?;
            is_first = false;
        }
        f.write_char(']')
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Scalar {
    Real(Real),
    Natural(Natural),
    Bool(bool),
}

impl Scalar {
    pub fn into_value(self) -> Value {
        Value::Array(Array {
            dims: vec![],
            values: vec![self],
        })
    }
}

impl Display for Scalar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Real(r) => r.fmt(f),
            Self::Natural(n) => n.fmt(f),
            Self::Bool(b) => b.fmt(f),
        }
    }
}

pub type Real = f32;
pub type Natural = u32;
