//! `Suggester` trait + `PendingSuggestion` type + `run_suggester` driver.
//!
//! Each concrete suggester implements [`Suggester`] and is driven by
//! [`run_suggester`], which handles de-duplication and persistence so the
//! implementor only needs to return a `Vec<PendingSuggestion>`.

use crate::runtime::HookRuntime;

/// A suggestion emitted by a [`Suggester`] — ready for storage.
///
/// Created by `detect()` and consumed by `run_suggester` → `storage::insert_suggestion`.
#[derive(Debug, Clone)]
pub struct PendingSuggestion {
    /// The parent task this suggestion targets.
    pub target_task_id: String,
    /// The specific subtask, if applicable. `None` for task-level suggestions.
    pub target_subtask_id: Option<String>,
    /// Human-readable reason explaining why the suggestion was emitted.
    pub reason: String,
    /// Structured evidence for the Claude Code consumer (JSON object).
    pub evidence: serde_json::Value,
}

/// Trait for action-suggestion detectors.
///
/// Each implementor observes a slice of Touring state and emits
/// [`PendingSuggestion`]s for a specific action type.
///
/// # Contract
/// - `detect` MUST be pure (no writes to the DB).
/// - `detect` MUST NOT panic; return `vec![]` on any error.
/// - Storage and de-duplication are handled by [`run_suggester`].
pub trait Suggester {
    /// The constant action type emitted by this detector.
    ///
    /// Must be one of the values recognised by Claude Code:
    /// `"update"` | `"stop"` | `"plan_mode"` | etc.
    fn action_type(&self) -> &'static str;

    /// Run detection logic and return pending suggestions (not yet persisted).
    ///
    /// Implementations should query the DB read-only and return a `Vec` that
    /// may be empty. `run_suggester` will filter duplicates before persisting.
    fn detect(&self, rt: &HookRuntime) -> Vec<PendingSuggestion>;
}

/// Run a suggester end-to-end: detect → de-dup against existing pending rows →
/// persist via storage helper → return count of new rows inserted.
///
/// De-duplication key: `(action_type, target_task_id, target_subtask_id)`.
/// If an unconsumed row already exists for that key, the suggestion is skipped
/// (rate-limit: at most one unconsumed suggestion per key at a time).
pub fn run_suggester<S: Suggester>(suggester: &S, rt: &HookRuntime) -> usize {
    let pending = suggester.detect(rt);
    let mut inserted = 0usize;
    for s in pending {
        if crate::bidirectional::storage::has_pending_suggestion(
            rt,
            suggester.action_type(),
            &s.target_task_id,
            s.target_subtask_id.as_deref(),
        ) {
            continue; // rate-limit: skip if already pending
        }
        if crate::bidirectional::storage::insert_suggestion(rt, suggester.action_type(), &s) {
            inserted += 1;
        }
    }
    inserted
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_runtime() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().expect("tempdir");
        let rt = HookRuntime::new(tmp.path()).expect("runtime");
        (tmp, rt)
    }

    fn ensure_suggestions_table(conn: &rusqlite::Connection) {
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cc_action_suggestions (
                suggestion_id TEXT PRIMARY KEY,
                action_type TEXT NOT NULL,
                target_task_id TEXT NOT NULL,
                target_subtask_id TEXT,
                reason TEXT NOT NULL,
                evidence_json TEXT NOT NULL DEFAULT '{}',
                suggested_at TEXT NOT NULL,
                consumed INTEGER NOT NULL DEFAULT 0,
                consumed_at TEXT,
                consumed_action TEXT
            );",
        );
    }

    /// A minimal suggester that always emits one fixed suggestion.
    struct AlwaysSuggester {
        task_id: String,
        subtask_id: Option<String>,
    }

    impl Suggester for AlwaysSuggester {
        fn action_type(&self) -> &'static str {
            "test_action"
        }

        fn detect(&self, _rt: &HookRuntime) -> Vec<PendingSuggestion> {
            vec![PendingSuggestion {
                target_task_id: self.task_id.clone(),
                target_subtask_id: self.subtask_id.clone(),
                reason: "test reason".into(),
                evidence: serde_json::json!({"test": true}),
            }]
        }
    }

    /// A suggester that emits nothing.
    struct NopSuggester;

    impl Suggester for NopSuggester {
        fn action_type(&self) -> &'static str {
            "nop"
        }

        fn detect(&self, _rt: &HookRuntime) -> Vec<PendingSuggestion> {
            vec![]
        }
    }

    #[test]
    fn test_nop_suggester_returns_zero() {
        let (_tmp, rt) = make_runtime();
        assert_eq!(run_suggester(&NopSuggester, &rt), 0);
    }

    #[test]
    fn test_always_suggester_inserts_one() {
        let (_tmp, rt) = make_runtime();
        ensure_suggestions_table(rt.ctx.knowledge.conn_ref());

        let s = AlwaysSuggester {
            task_id: "task_t1".into(),
            subtask_id: Some("sub_s1".into()),
        };
        assert_eq!(run_suggester(&s, &rt), 1);
    }

    #[test]
    fn test_dedup_second_call_returns_zero() {
        let (_tmp, rt) = make_runtime();
        ensure_suggestions_table(rt.ctx.knowledge.conn_ref());

        let s = AlwaysSuggester {
            task_id: "task_dup".into(),
            subtask_id: None,
        };
        assert_eq!(run_suggester(&s, &rt), 1);
        // Second run: unconsumed row exists — de-duped, no new insert.
        assert_eq!(run_suggester(&s, &rt), 0);
    }

    #[test]
    fn test_run_suggester_missing_table_returns_zero() {
        // Without calling ensure_suggestions_table, the table doesn't exist.
        // run_suggester must return 0 gracefully.
        let (_tmp, rt) = make_runtime();
        let s = AlwaysSuggester {
            task_id: "task_notbl".into(),
            subtask_id: None,
        };
        // storage::has_pending_suggestion returns false on error,
        // storage::insert_suggestion returns false on error → inserted = 0.
        assert_eq!(run_suggester(&s, &rt), 0);
    }
}
