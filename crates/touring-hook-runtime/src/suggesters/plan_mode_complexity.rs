//! PlanModeSuggester — detects CILA L4+ tasks at creation time (instructions_loaded)
//! and suggests that Claude Code enter plan mode before implementation.
//!
//! Uses the [`crate::bidirectional::Suggester`] trait: this is the **third consumer**
//! that validates the abstraction — zero boilerplate, only `action_type` and `detect`
//! need implementing; de-duplication and persistence are handled by `run_suggester`.
//!
//! Detection logic:
//! 1. Query `task_decompositions` created within the last 5 minutes with `status IN
//!    ('created', 'active')`.
//! 2. For each row, derive CILA level from the stored `cila_level` column if ≥ 4,
//!    or fall back to a keyword heuristic that scores the `description` text.
//! 3. Rows that exceed the threshold emit a `PendingSuggestion` with
//!    `action_type = "plan_mode"`.
//!
//! Added: 2026-04-13 (Pln3-P4 — enter_plan_mode bidirectional via Suggester trait)

use crate::bidirectional::{PendingSuggestion, Suggester};
use crate::runtime::HookRuntime;

/// CILA threshold: tasks at this level or above need structured planning.
const L4_THRESHOLD: i64 = 4;

/// Window for "just created" tasks (minutes).
const RECENT_WINDOW_MINUTES: i64 = 5;

/// Maximum suggestions per invocation (avoids flooding).
const MAX_SUGGESTIONS_PER_RUN: usize = 10;

/// Architectural complexity indicator keywords (case-insensitive prefix matches).
/// Two or more hits → classify as L4.
const COMPLEXITY_KEYWORDS: &[&str] = &[
    "architecture",
    "refactor major",
    "migration",
    "multi-service",
    "distributed",
    "schema migration",
    "breaking change",
    "multi-step",
    "cross-crate",
    "redesign",
    "paradigm shift",
    "orchestrat",
    "multi-phase",
    "multi-crate",
];

// ---------------------------------------------------------------------------
// Internal helpers (each CC ≤ 4)
// ---------------------------------------------------------------------------

/// Derive an effective CILA level for a task row.
///
/// Prefers the stored `cila_level` when it is already ≥ 4 (fast path).
/// Falls back to keyword heuristic when the stored level is below the threshold
/// — handles tasks created before the classifier was available or whose stored
/// level under-represents the actual complexity.
fn effective_cila_level(stored_level: i64, description: &str) -> i64 {
    if stored_level >= L4_THRESHOLD {
        return stored_level;
    }
    classify_complexity(description) as i64
}

/// Public wrapper for real-time CILA classification at task-creation time (Pln3 R1).
///
/// Returns the effective CILA level (2–6) for the given task subject text using
/// the keyword heuristic. Callers that already have a stored `cila_level` should
/// prefer `effective_cila_level` to respect the persisted value.
pub fn classify_complexity_for(text: &str) -> u8 {
    classify_complexity(text)
}

/// Keyword-based complexity classifier (fallback when stored cila_level < 4).
///
/// Scores the description against [`COMPLEXITY_KEYWORDS`].
/// Returns 4 when ≥ 2 keywords hit, 3 for 1 hit, 2 otherwise.
/// Mirrors the CILA L4/L3/L2 taxonomy without regex overhead.
fn classify_complexity(text: &str) -> u8 {
    let lower = text.to_lowercase();
    let hits = COMPLEXITY_KEYWORDS
        .iter()
        .filter(|kw| lower.contains(*kw))
        .count();
    match hits {
        0 => 2,
        1 => 3,
        _ => 4, // 2 or more indicators → L4
    }
}

/// Candidate row from the `task_decompositions` query.
struct ComplexTask {
    task_id: String,
    description: String,
    cila_level: i64,
}

/// Returns true when both required tables exist.
fn tables_ready(conn: &rusqlite::Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='table' AND name IN ('task_decompositions','cc_action_suggestions')",
        rusqlite::params![],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
        >= 2
}

/// Query tasks created in the last [`RECENT_WINDOW_MINUTES`] that are still
/// actionable (`status IN ('created', 'active')`).
fn query_recent_tasks(conn: &rusqlite::Connection) -> Vec<ComplexTask> {
    let window = format!("-{RECENT_WINDOW_MINUTES} minutes");
    let mut stmt = match conn.prepare(
        "SELECT task_id, description, cila_level \
         FROM task_decompositions \
         WHERE status IN ('created', 'active') \
           AND created_at > datetime('now', ?1) \
         ORDER BY created_at DESC \
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map(
        rusqlite::params![window, MAX_SUGGESTIONS_PER_RUN as i64],
        |row| {
            Ok(ComplexTask {
                task_id: row.get(0)?,
                description: row.get(1)?,
                cila_level: row.get(2)?,
            })
        },
    )
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// Build a `PendingSuggestion` from a complex task candidate.
fn to_pending_suggestion(task: &ComplexTask, effective_level: i64) -> PendingSuggestion {
    let reason = format!(
        "task {} classified as CILA L{} — \
         enter plan mode before implementation to reduce risk of scope drift",
        task.task_id, effective_level,
    );
    let evidence = serde_json::json!({
        "task_id": task.task_id,
        "cila_level": effective_level,
        "stored_cila_level": task.cila_level,
        "description_preview": &task.description[..task.description.len().min(80)],
    });
    PendingSuggestion {
        target_task_id: task.task_id.clone(),
        target_subtask_id: None, // task-level suggestion
        reason,
        evidence,
    }
}

// ---------------------------------------------------------------------------
// Suggester implementation
// ---------------------------------------------------------------------------

/// Detects recently-created CILA L4+ tasks and proposes entering plan mode.
pub struct PlanModeSuggester;

impl Suggester for PlanModeSuggester {
    fn action_type(&self) -> &'static str {
        "plan_mode"
    }

    fn detect(&self, rt: &HookRuntime) -> Vec<PendingSuggestion> {
        let conn = rt.ctx.knowledge.conn_ref();
        if !tables_ready(conn) {
            return vec![];
        }
        query_recent_tasks(conn)
            .iter()
            .filter_map(|task| {
                let level = effective_cila_level(task.cila_level, &task.description);
                if level >= L4_THRESHOLD {
                    Some(to_pending_suggestion(task, level))
                } else {
                    None
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Public backward-compatible wrapper (consumed by instructions_loaded.rs)
// ---------------------------------------------------------------------------

/// Detect recently-created CILA L4+ tasks and emit `plan_mode` suggestions.
///
/// Returns the count of new suggestions inserted. Returns 0 on any error
/// (graceful degradation — never panics, never blocks session start).
///
/// This is the **third consumer** of the [`crate::bidirectional::Suggester`] trait,
/// proving the abstraction eliminates boilerplate across all three detectors.
pub fn detect_and_suggest_plan_mode(runtime: &HookRuntime) -> usize {
    crate::bidirectional::suggester::run_suggester(&PlanModeSuggester, runtime)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_runtime() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().expect("tempdir");
        let rt = HookRuntime::new(tmp.path()).expect("runtime");
        (tmp, rt)
    }

    /// Create the `cc_action_suggestions` table (not in ensure_schema).
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

    /// Insert a task with explicit cila_level and created_at offset.
    fn insert_task(
        conn: &rusqlite::Connection,
        task_id: &str,
        task_type: &str,
        description: &str,
        cila_level: i64,
        minutes_ago: i64,
    ) {
        let created_at = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::minutes(minutes_ago))
            .unwrap_or_else(chrono::Utc::now)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO task_decompositions \
             (task_id, task_type, description, cila_level, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'created', ?5, ?5)",
            rusqlite::params![task_id, task_type, description, cila_level, created_at],
        );
    }

    // --- classify_complexity tests ---

    #[test]
    fn test_classify_complexity_no_keywords() {
        assert_eq!(classify_complexity("add a button"), 2);
    }

    #[test]
    fn test_classify_complexity_one_keyword() {
        assert_eq!(classify_complexity("fix migration script"), 3);
    }

    #[test]
    fn test_classify_complexity_two_keywords() {
        assert_eq!(
            classify_complexity("schema migration with cross-crate changes"),
            4
        );
    }

    #[test]
    fn test_classify_complexity_case_insensitive() {
        assert_eq!(classify_complexity("ARCHITECTURE redesign"), 4);
    }

    // --- effective_cila_level tests ---

    #[test]
    fn test_effective_level_prefers_stored_when_high() {
        assert_eq!(effective_cila_level(5, "simple task"), 5);
    }

    #[test]
    fn test_effective_level_falls_back_to_keyword() {
        // stored = 3, but description has 2 L4 keywords → effective = 4
        assert_eq!(
            effective_cila_level(3, "architecture redesign across crates"),
            4,
        );
    }

    #[test]
    fn test_effective_level_low_stored_low_keywords() {
        assert_eq!(effective_cila_level(2, "fix typo"), 2);
    }

    // --- PlanModeSuggester integration tests ---

    #[test]
    fn test_no_tasks_returns_zero() {
        let (_tmp, rt) = make_runtime();
        assert_eq!(detect_and_suggest_plan_mode(&rt), 0);
    }

    #[test]
    fn test_missing_suggestions_table_returns_zero() {
        // tables_ready check: cc_action_suggestions missing → 0
        let (_tmp, rt) = make_runtime();
        insert_task(
            rt.ctx.knowledge.conn_ref(),
            "task_x1",
            "plan_session",
            "architecture migration",
            4,
            1,
        );
        // No suggestions table → graceful zero
        assert_eq!(detect_and_suggest_plan_mode(&rt), 0);
    }

    #[test]
    fn test_old_task_not_detected() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        ensure_suggestions_table(conn);
        // 10 minutes ago = outside 5-minute window
        insert_task(
            conn,
            "task_old",
            "plan_session",
            "architecture migration overhaul",
            4,
            10,
        );
        assert_eq!(detect_and_suggest_plan_mode(&rt), 0);
    }

    #[test]
    fn test_recent_l4_stored_emits_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        ensure_suggestions_table(conn);
        insert_task(
            conn,
            "task_l4",
            "plan_session",
            "implement new L4 feature",
            4,
            1,
        );
        assert_eq!(detect_and_suggest_plan_mode(&rt), 1);

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cc_action_suggestions \
                 WHERE target_task_id='task_l4' AND action_type='plan_mode' AND consumed=0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(n, 1);
    }

    #[test]
    fn test_recent_l5_stored_emits_suggestion() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        ensure_suggestions_table(conn);
        insert_task(
            conn,
            "task_l5",
            "plan_session",
            "multi-agent orchestration",
            5,
            2,
        );
        assert_eq!(detect_and_suggest_plan_mode(&rt), 1);
    }

    #[test]
    fn test_recent_l3_not_emitted() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        ensure_suggestions_table(conn);
        insert_task(
            conn,
            "task_l3",
            "plan_session",
            "simple pipeline task",
            3,
            1,
        );
        assert_eq!(detect_and_suggest_plan_mode(&rt), 0);
    }

    #[test]
    fn test_keyword_fallback_upgrades_l3_to_l4() {
        // stored cila_level=3, but description has 2 keywords → effective L4
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        ensure_suggestions_table(conn);
        insert_task(
            conn,
            "task_kw",
            "plan_session",
            "cross-crate architecture refactor",
            3,
            1,
        );
        assert_eq!(detect_and_suggest_plan_mode(&rt), 1);
    }

    #[test]
    fn test_rate_limit_no_duplicate() {
        let (_tmp, rt) = make_runtime();
        let conn = rt.ctx.knowledge.conn_ref();
        ensure_suggestions_table(conn);
        insert_task(
            conn,
            "task_rl",
            "plan_session",
            "distributed architecture migration",
            4,
            1,
        );

        assert_eq!(detect_and_suggest_plan_mode(&rt), 1);
        // Second call: unconsumed row exists → rate-limited.
        assert_eq!(detect_and_suggest_plan_mode(&rt), 0);
    }

    #[test]
    fn test_action_type_is_plan_mode() {
        assert_eq!(PlanModeSuggester.action_type(), "plan_mode");
    }

    #[test]
    fn test_detect_empty_on_no_table() {
        let (_tmp, rt) = make_runtime();
        let s = PlanModeSuggester;
        // cc_action_suggestions doesn't exist → tables_ready = false → empty
        assert!(s.detect(&rt).is_empty());
    }
}
