//! Error model (SPEC §5.9): parse errors cite a 1-based line number
//! (normative); compile-time errors have no line number. Message texts
//! are informative.

use std::fmt;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Error {
    Score { line: usize, msg: String },
    Compile(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Score { line, msg } => write!(f, "score error, line {line}: {msg}"),
            Error::Compile(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {}
