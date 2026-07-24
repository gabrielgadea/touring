//! Session-insight value types shared between the knowledge data layer and the
//! hooks session-insights layer.
//!
//! Relocated to the kernel (A5, 2026-06-15) so `FileKnowledgeDB`'s insight-returning
//! methods (`recent_error_patterns`, `top_edited_files` insight shaping) no longer
//! couple the data layer to `touring-hooks-core::session_insights` — which would form a
//! cycle once `FileKnowledgeDB` moves to `touring-storage`.

use serde::{Deserialize, Serialize};

/// An error pattern observed in edit history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPatternInsight {
    /// Normalized error pattern.
    pub pattern: String,
    /// Number of times the pattern occurred.
    pub occurrences: i64,
}

/// A frequently edited file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditedFileInsight {
    /// Path of the edited file.
    pub file_path: String,
    /// Number of edits applied to the file.
    pub edit_count: u32,
}
