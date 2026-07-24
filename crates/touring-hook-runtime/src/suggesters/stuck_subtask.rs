//! Stuck-subtask detector — emits `update` action suggestions for Claude Code.
//!
//! Detects subtasks that have been in `pending` or `ready` status for more than
//! 30 minutes and emits a suggestion into `cc_action_suggestions` so Claude Code
//! can act on them via `TaskUpdate(task_id=..., suggestion_ref=<id>)`.
//!
//! Rate-limiting: one unconsumed suggestion per subtask is allowed at a time.
//! If an unconsumed suggestion already exists for a subtask, no new one is emitted.
//!
//! Implements [`crate::bidirectional::Suggester`] so the detection logic is
//! driven by [`crate::bidirectional::suggester::run_suggester`], which handles
//! de-duplication and persistence uniformly.
//!
//! The public function `detect_and_suggest_stuck_subtasks` is preserved for
//! backward-compatibility with `instructions_loaded.rs`.
//!
//! Added: 2026-04-13 (Pln3 task_update bidirectional — P1 stuck subtask detector)
//! Refactored: 2026-04-13 (Pln3-P3 — migrated to Suggester trait)

use crate::bidirectional::{PendingSuggestion, Suggester};
use crate::runtime::HookRuntime;

/// Threshold after which a pending/ready subtask is considered stuck.
const STUCK_THRESHOLD_MINUTES: u64 = 30;

/// Maximum suggestions emitted per invocation (prevents flooding on large DAGs).
const MAX_SUGGESTIONS_PER_RUN: usize = 10;

/// Row returned by the stuck-subtask query.
struct StuckCandidate {
    subtask_id: String,
    task_id: String,
    status: String,
    updated_at: String,
    stuck_minutes: i64,
}

/// Returns true if both required tables exist in the database.
fn tables_ready(conn: &rusqlite::Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='table' AND name IN ('decomposition_subtasks','cc_action_suggestions')",
        rusqlite::params![],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
        >= 2
}

/// Query subtasks stuck beyond `STUCK_THRESHOLD_MINUTES`.
fn query_stuck_candidates(conn: &rusqlite::Connection) -> Vec<StuckCandidate> {
    let threshold_expr = format!("-{STUCK_THRESHOLD_MINUTES} minutes");
    let mut stmt = match conn.prepare(
        "SELECT ds.subtask_id, ds.task_id, ds.status, ds.updated_at, \
                CAST((julianday('now') - julianday(ds.updated_at)) * 1440 AS INTEGER) AS stuck_minutes \
         FROM decomposition_subtasks ds \
         WHERE ds.status IN ('pending', 'ready') \
           AND ds.updated_at < datetime('now', ?1) \
         ORDER BY ds.updated_at ASC \
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map(
        rusqlite::params![threshold_expr, MAX_SUGGESTIONS_PER_RUN as i64],
        |row| {
            Ok(StuckCandidate {
                subtask_id: row.get(0)?,
                task_id: row.get(1)?,
                status: row.get(2)?,
                updated_at: row.get(3)?,
                stuck_minutes: row.get(4)?,
            })
        },
    )
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Suggester implementation
// ---------------------------------------------------------------------------

/// Detector that finds subtasks stuck in `pending` or `ready` and proposes
/// `action_type = "update"` suggestions for Claude Code.
pub struct StuckSubtaskSuggester;

impl Suggester for StuckSubtaskSuggester {
    fn action_type(&self) -> &'static str {
        "update"
    }

    fn detect(&self, rt: &HookRuntime) -> Vec<PendingSuggestion> {
        let conn = rt.ctx.knowledge.conn_ref();
        if !tables_ready(conn) {
            return vec![];
        }
        query_stuck_candidates(conn)
            .into_iter()
            .map(|c| {
                let bucket = c.stuck_minutes / 10;
                // Include bucket in reason so the caller can construct a
                // deterministic suggestion_id at the storage layer if needed.
                let reason = format!(
                    "subtask {} stuck in '{}' for {} minutes (last updated: {}, bucket: {})",
                    c.subtask_id, c.status, c.stuck_minutes, c.updated_at, bucket
                );
                let evidence = serde_json::json!({
                    "stuck_minutes": c.stuck_minutes,
                    "status": c.status,
                    "updated_at": c.updated_at,
                    "threshold_minutes": STUCK_THRESHOLD_MINUTES,
                    "bucket": bucket,
                });
                PendingSuggestion {
                    target_task_id: c.task_id,
                    target_subtask_id: Some(c.subtask_id),
                    reason,
                    evidence,
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Public backward-compatible wrapper
// ---------------------------------------------------------------------------

/// Detect subtasks stuck in `pending` or `ready` for more than 30 minutes and
/// emit `action_type=update` suggestions into `cc_action_suggestions`.
///
/// Returns the count of new suggestions emitted. Returns 0 on any error
/// (graceful degradation — never panics, never blocks session start).
///
/// This function is the stable public API consumed by `instructions_loaded.rs`.
/// Internally it delegates to [`crate::bidirectional::suggester::run_suggester`]
/// via [`StuckSubtaskSuggester`].
pub fn detect_and_suggest_stuck_subtasks(runtime: &HookRuntime) -> usize {
    crate::bidirectional::suggester::run_suggester(&StuckSubtaskSuggester, runtime)
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

    /// Insert a stuck subtask into the test DB.
    ///
    /// `HookRuntime::new()` already creates `task_decompositions` and
    /// `decomposition_subtasks` via `ensure_schema`. We only need to create
    /// `cc_action_suggestions` (not in `ensure_schema`) and insert rows.
    fn insert_stuck_subtask(
        conn: &rusqlite::Connection,
        subtask_id: &str,
        task_id: &str,
        status: &str,
        minutes_ago: i64,
    ) {
        // cc_action_suggestions is created by ensure_decompose_tables (CLI handlers)
        // but not by ensure_schema, so we create it here for tests.
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
        // task_decompositions requires task_type (NOT NULL, no default in ensure_schema).
        let _ = conn.execute(
            "INSERT OR IGNORE INTO task_decompositions \
             (task_id, task_type, description, status, created_at, updated_at) \
             VALUES (?1, 'test', 'test task', 'active', datetime('now'), datetime('now'))",
            rusqlite::params![task_id],
        );
        // SQLite-compatible format (space separator) so datetime() comparisons work.
        let updated_at = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(minutes_ago))
            .unwrap_or_else(chrono::Utc::now)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO decomposition_subtasks \
             (subtask_id, task_id, description, status, created_at, updated_at) \
             VALUES (?1, ?2, 'test subtask', ?3, datetime('now'), ?4)",
            rusqlite::params![subtask_id, task_id, status, updated_at],
        );
    }

    #[test]
    fn test_no_stuck_subtasks_returns_zero() {
        let (_tmp, rt) = make_runtime();
        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 0);
    }

    #[test]
    fn test_fresh_subtask_not_stuck() {
        let (_tmp, rt) = make_runtime();
        insert_stuck_subtask(
            rt.ctx.knowledge.conn_ref(),
            "sub_fresh",
            "task_t1",
            "pending",
            5,
        );
        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 0);
    }

    #[test]
    fn test_stuck_pending_emits_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        insert_stuck_subtask(conn, "sub_stuck_p", "task_t2", "pending", 45);
        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 1);

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cc_action_suggestions \
                 WHERE target_subtask_id='sub_stuck_p' AND action_type='update' AND consumed=0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(n, 1);
    }

    #[test]
    fn test_stuck_ready_emits_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        insert_stuck_subtask(conn, "sub_stuck_r", "task_t3", "ready", 60);
        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 1);
    }

    #[test]
    fn test_rate_limit_no_duplicate_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        insert_stuck_subtask(conn, "sub_dup", "task_t4", "pending", 45);

        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 1);
        // Second call: unconsumed suggestion exists → no new emission.
        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 0);

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cc_action_suggestions WHERE target_subtask_id='sub_dup'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(n, 1);
    }

    #[test]
    fn test_completed_subtask_not_detected() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        insert_stuck_subtask(conn, "sub_done", "task_t5", "completed", 120);
        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 0);
    }

    #[test]
    fn test_in_progress_subtask_not_detected() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        insert_stuck_subtask(conn, "sub_wip", "task_t6", "in_progress", 90);
        assert_eq!(detect_and_suggest_stuck_subtasks(&rt), 0);
    }

    // --- Trait-level tests ---

    #[test]
    fn test_suggester_trait_detect_no_table_empty() {
        // Without the cc_action_suggestions table, tables_ready returns false.
        // detect() must return empty vec without panic.
        let (_tmp, rt) = make_runtime();
        let s = StuckSubtaskSuggester;
        assert!(s.detect(&rt).is_empty());
    }

    #[test]
    fn test_suggester_action_type() {
        assert_eq!(StuckSubtaskSuggester.action_type(), "update");
    }
}
