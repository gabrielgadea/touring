//! Storage layer error types.

use thiserror::Error;

/// Storage-layer error.
#[derive(Debug, Error)]
pub enum StorageError {
    /// SQLite-specific error.
    #[error("sqlite error: {0}")]
    Sqlite(String),
    /// Tantivy-specific error.
    #[error("tantivy error: {0}")]
    Tantivy(String),
    /// rkyv archive error.
    #[error("rkyv error: {0}")]
    Rkyv(String),
    /// Checkpoint I/O error.
    #[error("checkpoint error: {0}")]
    Checkpoint(String),
    /// Generic I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;
