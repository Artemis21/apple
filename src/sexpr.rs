use std::{
    fmt::{Display, Write},
    num::{ParseFloatError, ParseIntError},
};

use crate::{Error, error};
pub use logos::Span;
use logos::{Logos, SpannedIter};

pub fn read(src: &str) -> Result<(SExpr, Span), Error> {
    let mut lexer = Token::lexer(src).spanned();
    let mut out = vec![];
    match parse_sexprs_to(&mut lexer, &mut out)? {
        End::RParen(span) => Err(error!(span, "unexpected rparen")),
        End::Eof => Ok(out.remove(0)),
    }
}

#[must_use]
enum End {
    RParen(Span),
    Eof,
}

fn parse_sexprs_to(
    lex: &mut SpannedIter<'_, Token>,
    out: &mut Vec<(SExpr, Span)>,
) -> Result<End, Error> {
    while let Some((tok, span)) = lex.next() {
        let tok = tok.map_err(|lex_e| Error {
            reason: lex_e.0,
            span: span.clone(),
        })?;
        match tok {
            Token::LParen => {
                let mut els = vec![];
                let End::RParen(end_span) = parse_sexprs_to(lex, &mut els)? else {
                    return Err(error!(span, "unmatched lparen (reached EOF)"));
                };
                out.push((SExpr::List(els), (span.start)..(end_span.end)));
            }
            Token::RParen => {
                return Ok(End::RParen(span));
            }
            Token::Symbol(sym) => out.push((SExpr::Symbol(sym), span)),
            Token::Natural(nat) => out.push((SExpr::Natural(nat), span)),
            Token::Real(real) => out.push((SExpr::Real(real), span)),
        }
    }
    Ok(End::Eof)
}

#[derive(Clone, Debug)]
pub enum SExpr {
    List(Vec<(SExpr, Span)>),
    Symbol(String),
    Natural(u32),
    Real(f32),
}

impl Display for SExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::List(subexprs) => {
                f.write_char('(')?;
                if let Some((first, rest)) = subexprs.split_first() {
                    first.0.fmt(f)?;
                    for (subexpr, _) in rest {
                        write!(f, " {subexpr}")?;
                    }
                }
                Ok(())
            }
            Self::Symbol(s) => s.fmt(f),
            Self::Natural(n) => n.fmt(f),
            Self::Real(r) => r.fmt(f),
        }
    }
}

#[derive(Logos, Debug)]
#[logos(error(LexingError))]
#[logos(skip r"[ \n]+")]
enum Token {
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[regex(r"[^ \n()0-9][^ \n()]*", |lex| lex.slice().to_string())]
    Symbol(String),
    #[regex(r"[0-9]+", |lex| lex.slice().parse::<u32>())]
    Natural(u32),
    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse::<f32>())]
    Real(f32),
}

#[derive(Clone, PartialEq, Debug)]
struct LexingError(String);

impl Default for LexingError {
    fn default() -> Self {
        Self("unknown lexing error".to_string())
    }
}

impl From<ParseIntError> for LexingError {
    fn from(e: ParseIntError) -> Self {
        Self(format!("couldn't parse natural: {e}"))
    }
}

impl From<ParseFloatError> for LexingError {
    fn from(e: ParseFloatError) -> Self {
        Self(format!("couldn't parse real: {e}"))
    }
}
