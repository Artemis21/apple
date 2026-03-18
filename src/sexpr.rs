use std::{
    fmt::{Display, Write},
    num::{ParseFloatError, ParseIntError},
    ops::Range,
};

use crate::{Error, Keyword, error};
use logos::{Logos, SpannedIter};

pub fn read(src: &str) -> Result<(SExpr, Span), Error> {
    let mut lexer = Token::lexer(src).spanned();
    let mut out = vec![];
    match parse_sexprs_to(&mut lexer, &mut out)? {
        End::RParen(span) => Err(error!("unexpected rparen").with_span(span)),
        End::Eof => Ok(out.remove(0)),
    }
}

/// Custom type mainly because we want it to implement Copy.
#[derive(Clone, Copy, Debug, Default)]
pub struct Span {
    start: usize,
    end: usize,
}

impl From<Range<usize>> for Span {
    fn from(Range { start, end }: Range<usize>) -> Self {
        Self { start, end }
    }
}

impl From<Span> for Range<usize> {
    fn from(Span { start, end }: Span) -> Self {
        start..end
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
        let mut span = Span::from(span);
        let tok = tok.map_err(|lex_e| error!("lex error: {}", lex_e.0).with_span(span))?;
        match tok {
            Token::LParen => {
                let mut els = vec![];
                let End::RParen(end_span) = parse_sexprs_to(lex, &mut els)? else {
                    return Err(error!("unmatched lparen (reached EOF)").with_span(span));
                };
                span.end = end_span.end;
                out.push((SExpr::List(els), span));
            }
            Token::RParen => {
                return Ok(End::RParen(span));
            }
            Token::Symbol(sym) => out.push((
                Keyword::from_symbol(&sym).map_or(SExpr::Symbol(sym), SExpr::Keyword),
                span,
            )),
            Token::Natural(nat) => out.push((SExpr::Natural(nat), span)),
            Token::Real(real) => out.push((SExpr::Real(real), span)),
        }
    }
    Ok(End::Eof)
}

#[derive(Clone, Debug)]
pub enum SExpr {
    List(Vec<(Self, Span)>),
    Symbol(String),
    Keyword(Keyword),
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
                f.write_char(')')?;
                Ok(())
            }
            Self::Symbol(s) => s.fmt(f),
            Self::Keyword(kw) => kw.fmt(f),
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
