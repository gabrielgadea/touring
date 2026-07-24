//! Refactor Preview — Thread-safe preview-then-apply workflow for refactoring.
//!
//! Generates rename edit lists, stores them with expiry (10 minutes),
//! and applies only validated, non-expired previews. Thread-safe via Mutex.
//! Inspired by code-review-graph's `refactor.py` pending refactors pattern.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Expiry time for pending refactors (seconds).
const REFACTOR_EXPIRY_SECS: u64 = 600; // 10 minutes

/// A single rename edit in a file.
#[derive(Debug, Clone, Serialize)]
pub struct RenameEdit {
    /// File where the edit applies.
    pub file_path: String,
    /// Line number of the occurrence to rename (1-based).
    pub line: u32,
    /// Identifier being renamed.
    pub old_name: String,
    /// Replacement identifier.
    pub new_name: String,
    /// "high" = exact match, "medium" = heuristic match
    pub confidence: String,
}

/// A pending refactor preview waiting to be applied.
#[derive(Debug, Clone, Serialize)]
pub struct RefactorPreview {
    /// Unique identifier of the pending refactor.
    pub refactor_id: String,
    /// Identifier being renamed.
    pub old_name: String,
    /// Replacement identifier.
    pub new_name: String,
    /// Edits that will be applied if the preview is accepted.
    pub edits: Vec<RenameEdit>,
    /// Unix timestamp (seconds) when the preview was created.
    pub created_at: u64,
    /// Current status of the preview (e.g. `pending`, `applied`).
    pub status: String,
}

/// Thread-safe store for pending refactor previews.
pub struct RefactorStore {
    pending: Mutex<HashMap<String, RefactorPreview>>,
}

impl Default for RefactorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RefactorStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Generate a rename preview and store it for later application.
    ///
    /// Returns the preview with a unique `refactor_id`.
    /// The preview expires after `REFACTOR_EXPIRY_SECS`.
    pub fn create_preview(
        &self,
        old_name: &str,
        new_name: &str,
        definition_file: &str,
        definition_line: u32,
        call_sites: &[(String, u32)], // (file_path, line)
    ) -> RefactorPreview {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let refactor_id = format!("ref_{}_{}", now_secs(), seq);

        let mut edits = vec![RenameEdit {
            file_path: definition_file.to_string(),
            line: definition_line,
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
            confidence: "high".to_string(),
        }];

        for (file, line) in call_sites {
            edits.push(RenameEdit {
                file_path: file.clone(),
                line: *line,
                old_name: old_name.to_string(),
                new_name: new_name.to_string(),
                confidence: "high".to_string(),
            });
        }

        let preview = RefactorPreview {
            refactor_id: refactor_id.clone(),
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
            edits,
            created_at: now_secs(),
            status: "pending".to_string(),
        };

        if let Ok(mut store) = self.pending.lock() {
            // Cleanup expired first
            let now = now_secs();
            store.retain(|_, v| now - v.created_at < REFACTOR_EXPIRY_SECS);
            store.insert(refactor_id, preview.clone());
        }

        preview
    }

    /// Retrieve a pending preview by ID (None if expired or not found).
    pub fn get_preview(&self, refactor_id: &str) -> Option<RefactorPreview> {
        let store = self.pending.lock().ok()?;
        let preview = store.get(refactor_id)?;
        let now = now_secs();
        if now - preview.created_at >= REFACTOR_EXPIRY_SECS {
            return None; // expired
        }
        Some(preview.clone())
    }

    /// Mark a refactor as applied (removes from pending).
    pub fn mark_applied(&self, refactor_id: &str) -> bool {
        match self.pending.lock() {
            Ok(mut store) => store.remove(refactor_id).is_some(),
            _ => false,
        }
    }

    /// Number of pending (non-expired) refactors.
    pub fn pending_count(&self) -> usize {
        match self.pending.lock() {
            Ok(store) => {
                let now = now_secs();
                store
                    .values()
                    .filter(|v| now - v.created_at < REFACTOR_EXPIRY_SECS)
                    .count()
            }
            _ => 0,
        }
    }

    /// List all pending refactor IDs.
    pub fn list_pending(&self) -> Vec<String> {
        match self.pending.lock() {
            Ok(store) => {
                let now = now_secs();
                store
                    .iter()
                    .filter(|(_, v)| now - v.created_at < REFACTOR_EXPIRY_SECS)
                    .map(|(k, _)| k.clone())
                    .collect()
            }
            _ => {
                vec![]
            }
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_preview() {
        let store = RefactorStore::new();
        let preview = store.create_preview(
            "old_func",
            "new_func",
            "src/main.rs",
            10,
            &[("src/lib.rs".into(), 25), ("src/test.rs".into(), 42)],
        );
        assert!(!preview.refactor_id.is_empty());
        assert_eq!(preview.edits.len(), 3); // 1 def + 2 calls
        assert_eq!(preview.old_name, "old_func");
        assert_eq!(preview.new_name, "new_func");

        // Retrieve
        let got = store.get_preview(&preview.refactor_id);
        assert!(got.is_some());
        assert_eq!(got.as_ref().unwrap().edits.len(), 3);
    }

    #[test]
    fn test_mark_applied_removes() {
        let store = RefactorStore::new();
        let preview = store.create_preview("a", "b", "f.rs", 1, &[]);
        assert_eq!(store.pending_count(), 1);

        let removed = store.mark_applied(&preview.refactor_id);
        assert!(removed);
        assert_eq!(store.pending_count(), 0);
        assert!(store.get_preview(&preview.refactor_id).is_none());
    }

    #[test]
    fn test_nonexistent_preview() {
        let store = RefactorStore::new();
        assert!(store.get_preview("nonexistent").is_none());
        assert!(!store.mark_applied("nonexistent"));
    }

    #[test]
    fn test_list_pending() {
        let store = RefactorStore::new();
        store.create_preview("a", "b", "f.rs", 1, &[]);
        store.create_preview("c", "d", "g.rs", 5, &[]);
        let ids = store.list_pending();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn test_default_trait() {
        let store = RefactorStore::default();
        assert_eq!(store.pending_count(), 0);
    }

    #[test]
    fn test_edit_confidence() {
        let store = RefactorStore::new();
        let preview = store.create_preview("x", "y", "a.rs", 1, &[("b.rs".into(), 5)]);
        assert!(preview.edits.iter().all(|e| e.confidence == "high"));
    }
}
