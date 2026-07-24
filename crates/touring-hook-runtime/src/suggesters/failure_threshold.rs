//! Failure-threshold detector — emits `stop` action suggestions for Claude Code.
//!
//! Detects tasks with subtasks that have `status = 'failed'` OR `attempts > 3`,
//! as well as tasks whose global circuit breaker is open.
//!
//! When either condition is met for a task, a suggestion with `action_type='stop'`
//! is emitted into `cc_action_suggestions` so Claude Code can cancel the task via
//! `TaskStop(task_id=..., suggestion_ref=<id>)`.
//!
//! Implements [`crate::bidirectional::Suggester`] so the detection logic is
//! driven by [`crate::bidirectional::suggester::run_suggester`], which handles
//! de-duplication and persistence uniformly.
//!
//! The circuit-breaker sentinel is handled separately (outside the trait) because
//! it is task-independent and must be guarded by `#[cfg(not(test))]`.
//!
//! The public function `detect_and_suggest_failing_tasks` is preserved for
//! backward-compatibility with `instructions_loaded.rs`.
//!
//! Added: 2026-04-13 (Pln3 task_stop bidirectional — P2 failure threshold detector)
//! Refactored: 2026-04-13 (Pln3-P3 — migrated to Suggester trait)

use crate::bidirectional::{PendingSuggestion, Suggester};
use crate::runtime::HookRuntime;

/// Subtask attempt threshold — more than this many attempts triggers a stop suggestion.
const FAILURE_ATTEMPT_THRESHOLD: i64 = 3;

/// Maximum stop suggestions emitted per invocation (prevents flooding).
const MAX_SUGGESTIONS_PER_RUN: usize = 10;

/// A failing task candidate with enough context to emit a stop suggestion.
struct FailCandidate {
    task_id: String,
    subtask_id: String,
    reason_kind: &'static str, // "failed_status" | "high_attempts"
    attempts: i64,
    status: String,
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

/// Query subtasks that have `status = 'failed'` or `attempts > THRESHOLD`.
/// Only considers tasks whose parent task is still active (not archived/cancelled).
fn query_failing_candidates(conn: &rusqlite::Connection) -> Vec<FailCandidate> {
    let threshold = FAILURE_ATTEMPT_THRESHOLD;
    let mut stmt = match conn.prepare(
        "SELECT ds.subtask_id, ds.task_id, ds.status, ds.attempts \
         FROM decomposition_subtasks ds \
         JOIN task_decompositions td ON td.task_id = ds.task_id \
         WHERE td.status NOT IN ('archived', 'cancelled', 'completed') \
           AND ds.status NOT IN ('completed', 'cancelled') \
           AND (ds.status = 'failed' OR ds.attempts > ?1) \
         ORDER BY ds.attempts DESC \
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map(
        rusqlite::params![threshold, MAX_SUGGESTIONS_PER_RUN as i64],
        |row| {
            let status: String = row.get(2)?;
            let attempts: i64 = row.get(3)?;
            let reason_kind = if status == "failed" {
                "failed_status"
            } else {
                "high_attempts"
            };
            Ok(FailCandidate {
                subtask_id: row.get(0)?,
                task_id: row.get(1)?,
                reason_kind,
                attempts,
                status,
            })
        },
    )
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// Build the human-readable reason string for a `FailCandidate`.
fn build_reason(candidate: &FailCandidate) -> String {
    match candidate.reason_kind {
        "failed_status" => format!(
            "subtask {} reached status 'failed' (task {})",
            candidate.subtask_id, candidate.task_id,
        ),
        _ => format!(
            "subtask {} has {} attempts (>{} threshold, status '{}') for task {}",
            candidate.subtask_id,
            candidate.attempts,
            FAILURE_ATTEMPT_THRESHOLD,
            candidate.status,
            candidate.task_id,
        ),
    }
}

// ---------------------------------------------------------------------------
// Suggester implementation
// ---------------------------------------------------------------------------

/// Detector that finds tasks with failing subtasks (`status=failed` or
/// `attempts > 3`) and proposes `action_type = "stop"` suggestions.
pub struct FailureThresholdSuggester;

impl Suggester for FailureThresholdSuggester {
    fn action_type(&self) -> &'static str {
        "stop"
    }

    fn detect(&self, rt: &HookRuntime) -> Vec<PendingSuggestion> {
        let conn = rt.ctx.knowledge.conn_ref();
        if !tables_ready(conn) {
            return vec![];
        }
        query_failing_candidates(conn)
            .into_iter()
            .map(|c| {
                let reason = build_reason(&c);
                let evidence = serde_json::json!({
                    "subtask_id": c.subtask_id,
                    "status": c.status,
                    "attempts": c.attempts,
                    "threshold": FAILURE_ATTEMPT_THRESHOLD,
                    "reason_kind": c.reason_kind,
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
// Circuit-breaker sentinel (task-independent, test-guarded)
// ---------------------------------------------------------------------------

/// Emit a global sentinel stop suggestion when the circuit breaker is open.
///
/// This is intentionally separate from the `Suggester` trait because:
/// 1. It is not task-scoped — there is no `target_task_id` from a decompose row.
/// 2. It must be `#[cfg(not(test))]` to avoid global-static bleed in unit tests.
#[cfg(not(test))]
fn maybe_emit_circuit_open_sentinel(conn: &rusqlite::Connection) -> usize {
    if !crate::circuit_breaker::is_open() {
        return 0;
    }
    let sentinel_id = "sugg_fail__circuit_open__global";
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let inserted = conn
        .execute(
            "INSERT OR IGNORE INTO cc_action_suggestions \
             (suggestion_id, action_type, target_task_id, reason, evidence_json, suggested_at) \
             VALUES (?1, 'stop', '__circuit__', \
             'Global circuit breaker is open — systemic failure detected', '{}', ?2)",
            rusqlite::params![sentinel_id, now],
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    if inserted {
        tracing::debug!("failure_threshold: emitted circuit-open stop sentinel");
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Public backward-compatible wrapper
// ---------------------------------------------------------------------------

/// Detect tasks with failing subtasks (`status=failed` or `attempts > 3`) and emit
/// `action_type=stop` suggestions into `cc_action_suggestions`.
///
/// Also emits one global stop suggestion when the circuit breaker is open, so CC
/// has visibility into systemic failure state.
///
/// Returns the count of new suggestions emitted. Returns 0 on any error
/// (graceful degradation — never panics, never blocks session start).
///
/// This function is the stable public API consumed by `instructions_loaded.rs`.
/// Internally it delegates to [`crate::bidirectional::suggester::run_suggester`]
/// via [`FailureThresholdSuggester`].
pub fn detect_and_suggest_failing_tasks(runtime: &HookRuntime) -> usize {
    // `mut` is needed by the #[cfg(not(test))] block below; test builds don't reach that path.
    #[allow(unused_mut)]
    let mut emitted =
        crate::bidirectional::suggester::run_suggester(&FailureThresholdSuggester, runtime);

    // Circuit-breaker sentinel: emitted outside the trait (not test-safe as a
    // Suggester because circuit_breaker is a global static).
    #[cfg(not(test))]
    {
        let conn = runtime.ctx.knowledge.conn_ref();
        emitted += maybe_emit_circuit_open_sentinel(conn);
    }

    emitted
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

    /// Set up the `cc_action_suggestions` table and a parent task.
    fn setup_tables(conn: &rusqlite::Connection, task_id: &str) {
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
        let _ = conn.execute(
            "INSERT OR IGNORE INTO task_decompositions \
             (task_id, task_type, description, status, created_at, updated_at) \
             VALUES (?1, 'test', 'test task', 'active', datetime('now'), datetime('now'))",
            rusqlite::params![task_id],
        );
    }

    fn insert_subtask(
        conn: &rusqlite::Connection,
        subtask_id: &str,
        task_id: &str,
        status: &str,
        attempts: i64,
    ) {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO decomposition_subtasks \
             (subtask_id, task_id, description, status, attempts, created_at, updated_at) \
             VALUES (?1, ?2, 'desc', ?3, ?4, datetime('now'), datetime('now'))",
            rusqlite::params![subtask_id, task_id, status, attempts],
        );
    }

    #[test]
    fn test_no_failing_tasks_returns_zero() {
        let (_tmp, rt) = make_runtime();
        assert_eq!(detect_and_suggest_failing_tasks(&rt), 0);
    }

    #[test]
    fn test_failed_status_emits_stop_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        setup_tables(conn, "task_f1");
        insert_subtask(conn, "sub_f1", "task_f1", "failed", 1);

        assert_eq!(detect_and_suggest_failing_tasks(&rt), 1);

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cc_action_suggestions \
                 WHERE target_task_id='task_f1' AND action_type='stop' AND consumed=0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(n, 1);
    }

    #[test]
    fn test_high_attempts_emits_stop_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        setup_tables(conn, "task_a1");
        insert_subtask(conn, "sub_a1", "task_a1", "in_progress", 5);

        assert_eq!(detect_and_suggest_failing_tasks(&rt), 1);
    }

    #[test]
    fn test_low_attempts_no_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        setup_tables(conn, "task_a2");
        insert_subtask(conn, "sub_a2", "task_a2", "in_progress", 2);

        assert_eq!(detect_and_suggest_failing_tasks(&rt), 0);
    }

    #[test]
    fn test_rate_limit_no_duplicate_stop_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        setup_tables(conn, "task_r1");
        insert_subtask(conn, "sub_r1", "task_r1", "failed", 1);

        assert_eq!(detect_and_suggest_failing_tasks(&rt), 1);
        // Second call: unconsumed suggestion exists → no new emission.
        assert_eq!(detect_and_suggest_failing_tasks(&rt), 0);

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cc_action_suggestions WHERE target_task_id='task_r1'",
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
        setup_tables(conn, "task_c1");
        // Completed subtask with many attempts — should not trigger.
        insert_subtask(conn, "sub_c1", "task_c1", "completed", 10);

        assert_eq!(detect_and_suggest_failing_tasks(&rt), 0);
    }

    #[test]
    fn test_cancelled_parent_task_not_detected() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cc_action_suggestions (
                suggestion_id TEXT PRIMARY KEY, action_type TEXT NOT NULL,
                target_task_id TEXT NOT NULL, target_subtask_id TEXT,
                reason TEXT NOT NULL, evidence_json TEXT NOT NULL DEFAULT '{}',
                suggested_at TEXT NOT NULL, consumed INTEGER NOT NULL DEFAULT 0,
                consumed_at TEXT, consumed_action TEXT
            );",
        );
        let _ = conn.execute(
            "INSERT OR IGNORE INTO task_decompositions \
             (task_id, task_type, description, status, created_at, updated_at) \
             VALUES ('task_can', 'test', 'cancelled task', 'cancelled', datetime('now'), datetime('now'))",
            rusqlite::params![],
        );
        insert_subtask(conn, "sub_can", "task_can", "failed", 5);
        // Cancelled parent → no suggestion emitted.
        assert_eq!(detect_and_suggest_failing_tasks(&rt), 0);
    }

    #[test]
    fn test_exactly_threshold_attempts_not_detected() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        setup_tables(conn, "task_e1");
        // Exactly at threshold (3) — NOT over, so no suggestion.
        insert_subtask(conn, "sub_e1", "task_e1", "in_progress", 3);
        assert_eq!(detect_and_suggest_failing_tasks(&rt), 0);
    }

    // --- Trait-level tests ---

    #[test]
    fn test_suggester_action_type() {
        assert_eq!(FailureThresholdSuggester.action_type(), "stop");
    }

    #[test]
    fn test_suggester_detect_no_table_empty() {
        // Without cc_action_suggestions table, tables_ready returns false.
        // detect() must return empty vec without panic.
        let (_tmp, rt) = make_runtime();
        assert!(FailureThresholdSuggester.detect(&rt).is_empty());
    }
}
