//! SQLite storage helpers for `cc_action_suggestions`.
//!
//! These are thin wrappers used by [`super::suggester::run_suggester`] to
//! read/write the `cc_action_suggestions` table without duplicating SQL
//! across every concrete [`super::suggester::Suggester`] implementation.
//!
//! All functions degrade gracefully: any SQLite error returns
//! `false` / `0` — they never panic.

use rusqlite::params;

use crate::runtime::HookRuntime;

use super::suggester::PendingSuggestion;

/// Check whether a pending (consumed=0) suggestion already exists for the
/// given `(action_type, target_task_id, target_subtask_id)` combination.
///
/// Returns `false` on any DB error (conservative: allows insertion to proceed,
/// relying on the `INSERT OR IGNORE` primary-key constraint for final dedup).
pub fn has_pending_suggestion(
    rt: &HookRuntime,
    action_type: &str,
    target_task_id: &str,
    target_subtask_id: Option<&str>,
) -> bool {
    let conn = rt.ctx.knowledge.conn_ref();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cc_action_suggestions \
             WHERE action_type = ?1 AND target_task_id = ?2 \
               AND (target_subtask_id IS ?3 OR target_subtask_id = ?3) \
               AND consumed = 0",
            params![action_type, target_task_id, target_subtask_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    count > 0
}

/// Insert a new `cc_action_suggestions` row. Returns `true` if a new row was
/// inserted (i.e., `INSERT OR IGNORE` wrote exactly one row).
///
/// The `suggestion_id` is derived deterministically:
/// `sugg_<action_type>_<task_id>_<subtask_id|"_">` so that re-insertions for
/// the same key are silently ignored by the PRIMARY KEY constraint.
pub fn insert_suggestion(rt: &HookRuntime, action_type: &str, s: &PendingSuggestion) -> bool {
    let conn = rt.ctx.knowledge.conn_ref();
    // SQLite-compatible datetime format (space separator, no Z suffix) so
    // comparisons with datetime('now', ...) work correctly.
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let suggestion_id = format!(
        "sugg_{}_{}_{}",
        action_type,
        s.target_task_id,
        s.target_subtask_id.as_deref().unwrap_or("_")
    );
    let evidence_json = s.evidence.to_string();
    conn.execute(
        "INSERT OR IGNORE INTO cc_action_suggestions \
         (suggestion_id, action_type, target_task_id, target_subtask_id, \
          reason, evidence_json, suggested_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            suggestion_id,
            action_type,
            s.target_task_id,
            s.target_subtask_id,
            s.reason,
            evidence_json,
            now,
        ],
    )
    .map(|rows| rows > 0)
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn make_runtime() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().expect("tempdir");
        let rt = HookRuntime::new(tmp.path()).expect("runtime");
        (tmp, rt)
    }

    fn ensure_table(conn: &rusqlite::Connection) {
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

    fn make_sugg(task_id: &str, subtask_id: Option<&str>) -> PendingSuggestion {
        PendingSuggestion {
            target_task_id: task_id.to_string(),
            target_subtask_id: subtask_id.map(|s| s.to_string()),
            reason: "test reason".into(),
            evidence: json!({"x": 1}),
        }
    }

    #[test]
    fn test_insert_and_has_pending() {
        let (_tmp, rt) = make_runtime();
        ensure_table(rt.ctx.knowledge.conn_ref());

        let s = make_sugg("task_a", Some("sub_a"));
        assert!(!has_pending_suggestion(
            &rt,
            "update",
            "task_a",
            Some("sub_a")
        ));
        assert!(insert_suggestion(&rt, "update", &s));
        assert!(has_pending_suggestion(
            &rt,
            "update",
            "task_a",
            Some("sub_a")
        ));
    }

    #[test]
    fn test_insert_idempotent_primary_key() {
        let (_tmp, rt) = make_runtime();
        ensure_table(rt.ctx.knowledge.conn_ref());

        let s = make_sugg("task_b", None);
        assert!(insert_suggestion(&rt, "stop", &s));
        // Same suggestion_id → INSERT OR IGNORE → 0 rows → returns false.
        assert!(!insert_suggestion(&rt, "stop", &s));
    }

    #[test]
    fn test_has_pending_false_when_consumed() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        ensure_table(conn);

        let s = make_sugg("task_c", None);
        assert!(insert_suggestion(&rt, "update", &s));

        // Mark as consumed.
        let _ = conn.execute(
            "UPDATE cc_action_suggestions SET consumed=1 WHERE target_task_id='task_c'",
            [],
        );

        assert!(!has_pending_suggestion(&rt, "update", "task_c", None));
    }

    #[test]
    fn test_no_table_returns_false_gracefully() {
        let (_tmp, rt) = make_runtime();
        // Table not created — should not panic.
        assert!(!has_pending_suggestion(&rt, "update", "task_x", None));
        let s = make_sugg("task_x", None);
        assert!(!insert_suggestion(&rt, "update", &s));
    }

    #[test]
    fn test_subtask_none_deduplication() {
        let (_tmp, rt) = make_runtime();
        ensure_table(rt.ctx.knowledge.conn_ref());

        let s = make_sugg("task_d", None);
        assert!(insert_suggestion(&rt, "plan_mode", &s));
        assert!(has_pending_suggestion(&rt, "plan_mode", "task_d", None));
        // Different subtask (Some) must NOT be matched.
        assert!(!has_pending_suggestion(
            &rt,
            "plan_mode",
            "task_d",
            Some("sub_other")
        ));
    }
}
