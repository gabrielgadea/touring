//! Typed errors for this crate.

use thiserror::Error;

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error enum (non-stage errors only).
#[derive(Debug, Error)]
pub enum Error {
    /// I/O failure.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parse / serialize failure.
    #[error("JSON: {0}")]
    Serialize(#[from] serde_json::Error),

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

/// Errors that can occur during stage execution.
#[derive(Debug, Error)]
pub enum StageError {
    /// An item was filtered out and did not proceed.
    #[error("item filtered")]
    Filtered,

    /// Fan-out produced no results.
    #[error("fan-out: no branches produced output")]
    FanOutEmpty,

    /// Fan-out produced more than one result (expected exactly one).
    #[error("fan-out: expected 1 result, got {0}")]
    FanOutMultiple(usize),

    /// Stage timed out.
    #[error("stage timed out")]
    Timeout,
}

impl StageError {
    /// Construct a [`StageError::Filtered`] for an item.
    pub fn filtered() -> Self {
        Self::Filtered
    }
}
