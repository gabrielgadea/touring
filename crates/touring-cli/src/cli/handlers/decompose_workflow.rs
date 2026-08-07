//! Workflow CLI handlers (`cli_workflow_*`) — extracted from decompose.rs (F-9).
//! Re-exported from `cli_handlers_decompose` so historic call paths resolve unchanged.

use crate::cli::params as payload_params;
use crate::cli_handlers_decompose::ensure_decompose_tables;
use crate::knowledge::FileKnowledgeDB;
use crate::runtime::HookRuntime;
use rusqlite::params;

// ─────────────────────────────────────────────────────────────────────────────
// Feature C & B: Workflow analytics + execution tracking
// ─────────────────────────────────────────────────────────────────────────────

/// `COUNT(*)` of `task_id`'s subtasks, optionally narrowed to one `status`.
///
/// `cli_workflow_stats` and `cli_workflow_status` between them spelled this
/// query out eight times, differing only in the status literal. An unusable
/// row counts as `0` — the behaviour every call site already had via
/// `.unwrap_or(0)`.
fn subtask_count(db: &FileKnowledgeDB, task_id: &str, status: Option<&str>) -> i64 {
    match status {
        Some(s) => db
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = ?2",
                params![task_id, s],
                |r| r.get(0),
            )
            .unwrap_or(0),
        None => db
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1",
                params![task_id],
                |r| r.get(0),
            )
            .unwrap_or(0),
    }
}

/// The `task_decompositions` row for `task_id` as JSON, or `None` when absent.
fn fetch_task_row(db: &FileKnowledgeDB, task_id: &str) -> Option<serde_json::Value> {
    db.conn_ref()
        .query_row(
            "SELECT task_id, description, status FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| {
                Ok(serde_json::json!({
                    "task_id": r.get::<_, String>(0)?,
                    "description": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?
                }))
            },
        )
        .ok()
}

/// Aggregate over every `subtask_results` row belonging to `task_id`.
///
/// Seven call sites across `cli_workflow_{stats,compare,status}` repeated this
/// same join and filter, differing only in the aggregate expression and an
/// optional extra predicate. A `NULL` aggregate (no matching rows) comes back
/// as `None`, which is what each site produced via `.ok()`/`.ok().flatten()`.
///
/// `select` and `extra_where` are interpolated into the SQL and must stay
/// caller-side literals — they are never derived from a payload.
fn results_aggregate<T: rusqlite::types::FromSql>(
    db: &FileKnowledgeDB,
    task_id: &str,
    select: &str,
    extra_where: &str,
) -> Option<T> {
    let sql = format!(
        "SELECT {select} FROM subtask_results sr \
         JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id \
         WHERE st.task_id = ?1{extra_where}"
    );
    db.conn_ref()
        .query_row(&sql, params![task_id], |r| r.get::<_, Option<T>>(0))
        .ok()
        .flatten()
}

/// The `duration_ms IS NOT NULL` guard the duration aggregates share.
const DURATION_NOT_NULL: &str = " AND sr.duration_ms IS NOT NULL";
/// Mean subtask duration, in milliseconds.
const AVG_DURATION: &str = "AVG(sr.duration_ms)";
/// Total subtask duration, in milliseconds.
const SUM_DURATION: &str = "SUM(sr.duration_ms)";
/// Fraction of subtask results that were cache hits.
const CACHE_HIT_RATE: &str = "CAST(SUM(sr.cache_hit) AS REAL) / COUNT(*)";

/// Normalise a stored subtask id to the scoped `task_id::subtask_id` form that
/// `cli_decompose_add` writes, leaving an already-scoped id untouched.
fn scoped_subtask_id(raw_id: &str, task_id: &str) -> String {
    if raw_id.starts_with(&format!("{task_id}::")) {
        raw_id.to_string()
    } else {
        format!("{task_id}::{raw_id}")
    }
}

/// Return analytics for a specific task (Feature C).
pub fn cli_workflow_stats(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload_params::str_or_empty(payload, "task_id");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let total_subtasks = subtask_count(db, task_id, None);
    let completed = subtask_count(db, task_id, Some("completed"));
    let failed = subtask_count(db, task_id, Some("failed"));

    // Aggregate from subtask_results
    let avg_duration_ms: Option<f64> =
        results_aggregate(db, task_id, AVG_DURATION, DURATION_NOT_NULL);

    let cache_hit_rate: Option<f64> = results_aggregate(db, task_id, CACHE_HIT_RATE, "");

    serde_json::json!({
        "task_id": task_id,
        "total_subtasks": total_subtasks,
        "completed": completed,
        "failed": failed,
        "pending": total_subtasks - completed - failed,
        "avg_duration_ms": avg_duration_ms,
        "cache_hit_rate": cache_hit_rate
    })
    .to_string()
}

/// Return the slowest subtasks for a task (Feature C).
pub fn cli_workflow_slowest(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload_params::str_or_empty(payload, "task_id");
    let top_n: usize = payload.get("top").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let mut stmt = match db.conn_ref().prepare(
        "SELECT sr.subtask_id, sr.duration_ms, sr.cache_hit, sr.completed_at, st.description
         FROM subtask_results sr
         JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id
         WHERE st.task_id = ?1 AND sr.duration_ms IS NOT NULL
         ORDER BY sr.duration_ms DESC
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
    };

    let rows: Vec<serde_json::Value> = stmt
        .query_map(params![task_id, top_n as i64], |r| {
            Ok(serde_json::json!({
                "subtask_id": r.get::<_, String>(0)?,
                "duration_ms": r.get::<_, Option<i64>>(1)?,
                "cache_hit": r.get::<_, Option<i32>>(2)? == Some(1),
                "completed_at": r.get::<_, Option<String>>(3)?,
                "description": r.get::<_, String>(4)?
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    serde_json::json!({
        "task_id": task_id,
        "slowest": rows
    })
    .to_string()
}

/// Compare execution metrics between two tasks (Feature C).
pub fn cli_workflow_compare(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_a = payload
        .get("task_id_a")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_b = payload
        .get("task_id_b")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    fn task_metrics(db: &FileKnowledgeDB, task_id: &str) -> serde_json::Value {
        let total = subtask_count(db, task_id, None);
        let completed = subtask_count(db, task_id, Some("completed"));
        let avg_ms: Option<f64> = results_aggregate(db, task_id, AVG_DURATION, DURATION_NOT_NULL);
        let cache_rate: Option<f64> = results_aggregate(db, task_id, CACHE_HIT_RATE, "");
        serde_json::json!({
            "task_id": task_id,
            "total": total,
            "completed": completed,
            "avg_duration_ms": avg_ms,
            "cache_hit_rate": cache_rate
        })
    }

    serde_json::json!({
        "task_a": task_metrics(db, task_a),
        "task_b": task_metrics(db, task_b)
    })
    .to_string()
}

/// Feature B: Execute a task workflow with streaming output.
/// Feature B3: Streaming events for workflow run.
///
/// Emits a sequence of events (task_start, subtask_start per item, task_complete)
/// that can be streamed to stdout. The `events` array in the response encodes these
/// events; a true SSE transport would emit one JSON line per event with flush().
fn build_workflow_events(task_id: &str, subtasks: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut events = Vec::with_capacity(2 + subtasks.len());

    // Event 1: task_start
    events.push(serde_json::json!({
        "event": "task_start",
        "task_id": task_id,
        "timestamp": now
    }));

    // Event 2+: subtask_start per subtask
    for st in subtasks {
        events.push(serde_json::json!({
            "event": "subtask_start",
            "subtask_id": st["subtask_id"],
            "description": st["description"],
            "depends_on": st["depends_on"],
            "timestamp": chrono::Utc::now().to_rfc3339()
        }));
    }

    // Final: task_complete
    events.push(serde_json::json!({
        "event": "task_complete",
        "task_id": task_id,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));

    events
}

/// Records and reports a workflow run for a task over the decompose tables as JSON.
pub fn cli_workflow_run(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload_params::str_or_empty(payload, "task_id");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Fetch task + subtasks
    let task = fetch_task_row(db, task_id);

    let subtasks: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, description, depends_on, status FROM decomposition_subtasks WHERE task_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db error"}).to_string(),
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(2)?;
            let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_else(|_| vec![]);
            Ok(serde_json::json!({
                "subtask_id": r.get::<_, String>(0)?,
                "description": r.get::<_, String>(1)?,
                "depends_on": deps,
                "status": r.get::<_, String>(3)?
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    // B3: Build streaming events array
    let events = build_workflow_events(task_id, &subtasks);

    // B6: ANSI-colored summary (emitted when color mode is active)
    let colored = payload
        .get("color")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let summary = if colored {
        let run = format!(
            "\x1b[1;32m▶\x1b[0m {} \x1b[90m(workflow run)\x1b[0m",
            task_id
        );
        serde_json::json!({ "colored": run, "raw": task_id })
    } else {
        serde_json::json!({ "raw": task_id })
    };

    serde_json::json!({
        "event": "workflow_start",
        "task": task,
        "subtasks": subtasks,
        "events": events,
        "summary": summary
    })
    .to_string()
}

/// Resumes a previously suspended workflow run for a task, returning the resumed state as JSON.
pub fn cli_workflow_resume(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload_params::str_or_empty(payload, "task_id");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Get task info
    let task = fetch_task_row(db, task_id);

    // Get all subtasks with their execution status
    let subtasks: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT st.subtask_id, st.description, st.depends_on, st.status,
                    sr.started_at, sr.completed_at, sr.duration_ms, sr.cache_hit, sr.error
             FROM decomposition_subtasks st
             LEFT JOIN subtask_results sr ON st.subtask_id = sr.subtask_id
             WHERE st.task_id = ?1
             ORDER BY st.priority ASC, st.created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db error"}).to_string(),
        };
        stmt.query_map(params![task_id], |r| {
            let status: String = r.get(3)?;
            let started_at: Option<String> = r.get(4).ok();
            let completed_at: Option<String> = r.get(5).ok();
            let duration_ms: Option<i64> = r.get(6).ok();
            let cache_hit: Option<i32> = r.get(7).ok();
            let error: Option<String> = r.get(8).ok();
            let deps_str: String = r.get(2)?;
            let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_else(|_| vec![]);
            // Use scoped subtask_id (task_id::subtask_id) to match cli_decompose_add's format
            let raw_id: String = r.get(0)?;
            let scoped_id = scoped_subtask_id(&raw_id, task_id);
            Ok(serde_json::json!({
                "subtask_id": scoped_id,
                "description": r.get::<_, String>(1)?,
                "depends_on": deps,
                "status": status,
                "started_at": started_at,
                "completed_at": completed_at,
                "duration_ms": duration_ms,
                "cache_hit": cache_hit.map(|c| c != 0),
                "error": error
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    // Find next subtask to execute (first pending or in_progress)
    let next_subtask = subtasks
        .iter()
        .find(|s| {
            let status = s.get("status").and_then(|v| v.as_str()).unwrap_or("");
            status == "pending" || status == "in_progress"
        })
        .cloned();

    // Calculate completion percentage
    let total = subtasks.len() as i64;
    let completed = subtasks
        .iter()
        .filter(|s| {
            let status = s.get("status").and_then(|v| v.as_str()).unwrap_or("");
            status == "completed"
        })
        .count() as i64;
    let completion_pct = if total > 0 {
        (completed as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    serde_json::json!({
        "event": "workflow_resume",
        "task_id": task_id,
        "task": task,
        "subtasks": subtasks,
        "next_subtask": next_subtask,
        "completion_pct": completion_pct,
        "completed_count": completed,
        "total_count": total
    })
    .to_string()
}

/// Feature B4: Return current task/subtask status for polling.
pub fn cli_workflow_status(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload_params::str_or_empty(payload, "task_id");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Get task-level status
    let task_row: Option<serde_json::Value> = db
        .conn_ref()
        .query_row(
            "SELECT task_id, description, status, quality_score, created_at, updated_at
             FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| {
                Ok(serde_json::json!({
                    "task_id": r.get::<_, String>(0)?,
                    "description": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?,
                    "quality_score": r.get::<_, Option<f64>>(3)?,
                    "created_at": r.get::<_, Option<String>>(4)?,
                    "updated_at": r.get::<_, Option<String>>(5)?
                }))
            },
        )
        .ok();

    // Get aggregated subtask counts
    let total = subtask_count(db, task_id, None);
    let pending = subtask_count(db, task_id, Some("pending"));
    let in_progress = subtask_count(db, task_id, Some("in_progress"));
    let completed = subtask_count(db, task_id, Some("completed"));
    let failed = subtask_count(db, task_id, Some("failed"));

    let cancelled: i64 = db.conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'cancelled'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Aggregate timing from subtask_results
    let total_duration_ms: Option<i64> =
        results_aggregate(db, task_id, SUM_DURATION, DURATION_NOT_NULL);

    let avg_duration_ms: Option<f64> =
        results_aggregate(db, task_id, AVG_DURATION, DURATION_NOT_NULL);

    let cache_hit_rate: Option<f64> = results_aggregate(db, task_id, CACHE_HIT_RATE, "");

    // Per-subtask status list (lightweight — no subtask_results JOIN)
    let subtask_statuses: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, description, status, depends_on, priority
             FROM decomposition_subtasks WHERE task_id = ?1
             ORDER BY priority ASC, created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db error"}).to_string(),
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str: String = r.get(3)?;
            let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_else(|_| vec![]);
            let raw_id: String = r.get(0)?;
            let scoped_id = scoped_subtask_id(&raw_id, task_id);
            Ok(serde_json::json!({
                "subtask_id": scoped_id,
                "description": r.get::<_, String>(1)?,
                "status": r.get::<_, String>(2)?,
                "depends_on": deps,
                "priority": r.get::<_, i32>(4)?
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    let completion_pct = if total > 0 {
        (completed as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    serde_json::json!({
        "event": "workflow_status",
        "task_id": task_id,
        "task": task_row,
        "summary": {
            "total_subtasks": total,
            "pending": pending,
            "in_progress": in_progress,
            "completed": completed,
            "failed": failed,
            "cancelled": cancelled,
            "completion_pct": completion_pct,
            "total_duration_ms": total_duration_ms,
            "avg_duration_ms": avg_duration_ms,
            "cache_hit_rate": cache_hit_rate
        },
        "subtasks": subtask_statuses
    })
    .to_string()
}
