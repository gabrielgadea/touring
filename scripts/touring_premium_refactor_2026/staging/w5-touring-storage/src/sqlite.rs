//! `sqlite` — SQLite backend (extracted from knowledge_db)
//!
//! W5 PLACEHOLDER. Real implementation extracted from existing crates in W5.2-W5.4.

use crate::{StorageError, StorageResult};

/// TODO: replace with extracted code from `touring-hooks::sqlite_*`.
pub struct SqliteBackend {}

impl SqliteBackend {
    /// Create a new backend (W5 placeholder).
    pub fn new() -> StorageResult<Self> {
        Ok(Self {})
    }
}

impl Default for SqliteBackend {
    fn default() -> Self {
        Self::new().expect("default backend")
    }
}
