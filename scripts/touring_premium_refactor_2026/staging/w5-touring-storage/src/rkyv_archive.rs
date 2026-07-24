//! `rkyv_archive` — rkyv zero-copy archive backend
//!
//! W5 PLACEHOLDER. Real implementation extracted from existing crates in W5.2-W5.4.

use crate::{StorageError, StorageResult};

/// TODO: replace with extracted code from `touring-hooks::rkyv_archive_*`.
pub struct RkyvArchiveBackend {}

impl RkyvArchiveBackend {
    /// Create a new backend (W5 placeholder).
    pub fn new() -> StorageResult<Self> {
        Ok(Self {})
    }
}

impl Default for RkyvArchiveBackend {
    fn default() -> Self {
        Self::new().expect("default backend")
    }
}
