use std::ops::Range;

use ariadne::{Label, Report, ReportKind, Source};

use crate::Span;

#[derive(Debug)]
pub struct Error {
    pub message: String,
    pub span: Option<Span>,
    pub causes: Vec<ErrorCause>,
}

#[derive(Debug)]
pub struct ErrorCause {
    pub message: String,
    pub span: Option<Span>,
}

impl Error {
    pub fn display(self, src: &str) {
        let span = self
            .span
            .or_else(|| self.causes.iter().find_map(|cause| cause.span))
            .unwrap_or_default();
        let mut report =
            Report::build(ReportKind::Error, Range::from(span)).with_message(self.message);
        if let Some(span) = self.span {
            report =
                report.with_label(Label::new(Range::from(span)).with_message("error occured here"));
        }
        for cause in self.causes {
            report = if let Some(span) = cause.span {
                report.with_label(Label::new(Range::from(span)).with_message(cause.message))
            } else {
                report.with_note(cause.message)
            };
        }
        report.finish().eprint(Source::from(src)).unwrap();
    }

    pub const fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_cause(mut self, cause: ErrorCause) -> Self {
        self.causes.push(cause);
        self
    }
}

pub trait ResultExt {
    //fn error_span(self, span: Span) -> Self;
    fn error_cause(self, cause: ErrorCause) -> Self;
}

impl<T> ResultExt for Result<T, Error> {
    /*fn error_span(self, span: Span) -> Self {
        self.map_err(|e| e.with_span(span))
    }*/

    fn error_cause(self, cause: ErrorCause) -> Self {
        self.map_err(|e| e.with_cause(cause))
    }
}

macro_rules! error {
    ($($t:tt)*) => {
        crate::Error {
            span: None,
            message: format!($($t)*),
            causes: vec![],
        }
    };
}

macro_rules! cause {
    ($span:expr, $($t:tt)*) => {
        crate::ErrorCause {
            span: $span,
            message: format!($($t)*)
        }
    };
}

pub(crate) use cause;
pub(crate) use error;
