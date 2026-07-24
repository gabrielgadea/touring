//! Typed errors for this crate.

use thiserror::Error;

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error enum.
#[derive(Debug, Error)]
pub enum Error {
    /// I/O failure.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parse / serialize failure.
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),

    /// Generic invariant violation.
    #[error("invariant: {0}")]
    Invariant(String),
}

impl Error {
    /// Build an [`Error::Invariant`] from any displayable value.
    pub fn invariant(msg: impl Into<String>) -> Self {
        Self::Invariant(msg.into())
    }
}
