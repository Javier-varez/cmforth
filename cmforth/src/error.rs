use crate::types::Address;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Interpreter failure")]
    InterpreterFailure,
    #[error("Word not found")]
    WordNotFound,
    #[error("Stack overflow")]
    StackOverflow,
    #[error("Stack underflow")]
    StackUnderflow,
    #[error("Corrupt word definition at {0}")]
    CorruptWordDef(Address),
    #[error("Invalid Word. It is not UTF-8 encoded.")]
    InvalidWord,
    #[error("Invalid string. It is not UTF-8 encoded.")]
    InvalidString,
}
