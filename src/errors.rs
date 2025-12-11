use std::ops::Range;

use crate::Span;

#[derive(Debug)]
pub struct Error {
    pub span: Span,
    pub reason: String,
}

impl Error {
    pub fn display(self, src: &str) {
        ariadne::Report::build(ariadne::ReportKind::Error, Range::from(self.span))
            .with_message(self.reason)
            .with_label(ariadne::Label::new(Range::from(self.span)).with_message("here"))
            .finish()
            .eprint(ariadne::Source::from(src))
            .unwrap();
    }
}

macro_rules! error {
    ($span:expr, $($t:tt)*) => {
        Error {
            span: $span,
            reason: format!($($t)*),
        }
    };
}

pub(crate) use error;
