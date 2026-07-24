use thiserror::Error;

/// Errors produced by polyglot search and rewrite operations.
#[derive(Debug, Error)]
pub enum Error {
    /// The requested language string is unknown or not supported by the polyglot stack.
    #[error("unknown or unsupported language: {0}")]
    UnknownLang(String),

    /// The supplied ast-grep pattern could not be parsed for the target language.
    #[error("invalid ast-grep pattern for {lang}: {reason}")]
    InvalidPattern {
        /// Name of the language the pattern was parsed against.
        lang: &'static str,
        /// Human-readable explanation of why the pattern is invalid.
        reason: String,
    },
}

/// Convenience result type for polyglot operations, defaulting the error to [`Error`](enum@Error).
pub type Result<T> = std::result::Result<T, Error>;
