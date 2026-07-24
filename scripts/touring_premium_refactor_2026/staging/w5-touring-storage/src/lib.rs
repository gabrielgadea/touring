//! `touring-storage` — Layer 2 of new Touring architecture.
//!
//! Centralizes ALL persistence concerns previously scattered across
//! `touring-hooks`, `touring-server`, `touring-cortex`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "tantivy")]
pub mod tantivy;

#[cfg(feature = "rkyv")]
pub mod rkyv_archive;

pub mod checkpoint;
pub mod error;

pub use error::{StorageError, StorageResult};

/// Common trait for read-write storage backends.
pub trait Store {
    /// Backend identifier (for logging / tracing).
    fn backend(&self) -> &'static str;
    /// Number of records currently stored.
    fn len(&self) -> StorageResult<usize>;
    /// `true` if no records.
    fn is_empty(&self) -> StorageResult<bool> {
        self.len().map(|n| n == 0)
    }
}
