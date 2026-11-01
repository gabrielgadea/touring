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
/// Completed attempts over finished attempts. One `subtask_results` row is one attempt (a retry opens
/// a new row), and the `cache_hit` column is what the writer sets for an attempt that ended
/// `completed` — the name is historical, nothing is cached. It used to be published as
/// `cache_hit_rate` and divided by every row, open ones included (touring-a4, 04/10/2026).
const ATTEMPT_COMPLETION_RATE: &str = "CAST(SUM(sr.cache_hit) AS REAL) / COUNT(*)";
/// Only attempts that ended: an open row has no outcome yet, so a task whose every row is open
/// aggregates to NULL (`None`) instead of 0.
const FINISHED_ONLY: &str = " AND sr.completed_at IS NOT NULL";

/// What a `subtask_results` row says about its attempt: `completed`, `failed`, or nothing yet. Only
/// `completed` and `failed` close a row (`record_subtask_transition`), so a closed row whose
/// `cache_hit` is 0 is a failure.
fn attempt_outcome(completed_at: Option<&str>, cache_hit: Option<i32>) -> Option<&'static str> {
    completed_at.map(|_| {
        if cache_hit == Some(1) {
            "completed"
        } else {
            "failed"
        }
    })
}

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

    let attempt_completion_rate: Option<f64> =
        results_aggregate(db, task_id, ATTEMPT_COMPLETION_RATE, FINISHED_ONLY);

    serde_json::json!({
        "task_id": task_id,
        "total_subtasks": total_subtasks,
        "completed": completed,
        "failed": failed,
        "pending": total_subtasks - completed - failed,
        "avg_duration_ms": avg_duration_ms,
        "attempt_completion_rate": attempt_completion_rate
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
            let completed_at = r.get::<_, Option<String>>(3)?;
            // Read from `completed_at`, never inferred from the duration: a nullable column that only
            // the closer writes today is what `cache_hit` was (touring-a4, 04/10/2026).
            let outcome = attempt_outcome(completed_at.as_deref(), r.get::<_, Option<i32>>(2)?);
            Ok(serde_json::json!({
                "subtask_id": r.get::<_, String>(0)?,
                "duration_ms": r.get::<_, Option<i64>>(1)?,
                "attempt_outcome": outcome,
                "completed_at": completed_at,
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
        let attempt_rate: Option<f64> =
            results_aggregate(db, task_id, ATTEMPT_COMPLETION_RATE, FINISHED_ONLY);
        serde_json::json!({
            "task_id": task_id,
            "total": total,
            "completed": completed,
            "avg_duration_ms": avg_ms,
            "attempt_completion_rate": attempt_rate
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

/// Resumes a previously suspended workflow run for a task: one row per subtask (each with its LATEST
/// attempt), every attempt under `attempts`, and the next subtask to run, as JSON.
///
/// `subtasks` used to be the `LEFT JOIN` of subtasks and attempts — one row per ATTEMPT — so a
/// retried subtask counted twice and `completion_pct` disagreed with `cli_workflow_status` for the
/// same plan (cross-audit 04/10/2026, X2). The counts come from the subtasks; the attempts have
/// their own key.
pub fn cli_workflow_resume(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload_params::str_or_empty(payload, "task_id");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let task = fetch_task_row(db, task_id);
    let Ok(attempts) = resume_attempts(db, task_id) else {
        return serde_json::json!({"error": "db error"}).to_string();
    };
    let Ok(subtasks) = resume_subtasks(db, task_id, &attempts) else {
        return serde_json::json!({"error": "db error"}).to_string();
    };

    // The next subtask to run: the first one still pending or in progress.
    let next_subtask = subtasks
        .iter()
        .find(|s| matches!(s["status"].as_str(), Some("pending" | "in_progress")))
        .cloned();

    let total = subtasks.len() as i64;
    let completed = subtasks
        .iter()
        .filter(|s| s["status"] == "completed")
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
        "attempts": attempts,
        "next_subtask": next_subtask,
        "completion_pct": completion_pct,
        "completed_count": completed,
        "total_count": total
    })
    .to_string()
}

/// Every attempt of `task_id`'s subtasks, oldest first, each with its outcome (`null` while open).
fn resume_attempts(
    db: &FileKnowledgeDB,
    task_id: &str,
) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = db.conn_ref().prepare(
        "SELECT sr.subtask_id, sr.started_at, sr.completed_at, sr.duration_ms, sr.cache_hit, sr.error
         FROM subtask_results sr
         JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id
         WHERE st.task_id = ?1
         ORDER BY sr.started_at ASC, sr.rowid ASC",
    )?;
    let rows = stmt.query_map(params![task_id], |r| {
        let raw_id: String = r.get(0)?;
        let started_at: Option<String> = r.get(1).ok();
        let completed_at: Option<String> = r.get(2).ok();
        let duration_ms: Option<i64> = r.get(3).ok();
        let cache_hit: Option<i32> = r.get(4).ok();
        let error: Option<String> = r.get(5).ok();
        Ok(serde_json::json!({
            "subtask_id": scoped_subtask_id(&raw_id, task_id),
            "started_at": started_at,
            "completed_at": completed_at,
            "duration_ms": duration_ms,
            "attempt_outcome": attempt_outcome(completed_at.as_deref(), cache_hit),
            "error": error
        }))
    })?;
    rows.collect()
}

/// `task_id`'s subtasks, one row each, in queue order, carrying the LATEST of `attempts` (by start)
/// and how many attempts it had.
fn resume_subtasks(
    db: &FileKnowledgeDB,
    task_id: &str,
    attempts: &[serde_json::Value],
) -> rusqlite::Result<Vec<serde_json::Value>> {
    let mut stmt = db.conn_ref().prepare(
        "SELECT subtask_id, description, depends_on, status
         FROM decomposition_subtasks
         WHERE task_id = ?1
         ORDER BY priority ASC, created_at ASC",
    )?;
    let rows = stmt.query_map(params![task_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut subtasks = Vec::new();
    for row in rows {
        let (raw_id, description, deps_str, status) = row?;
        let subtask_id = scoped_subtask_id(&raw_id, task_id);
        let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
        let own: Vec<&serde_json::Value> = attempts
            .iter()
            .filter(|a| a["subtask_id"] == subtask_id.as_str())
            .collect();
        let latest = |key: &str| {
            own.last()
                .map_or(serde_json::Value::Null, |a| a[key].clone())
        };
        subtasks.push(serde_json::json!({
            "subtask_id": subtask_id,
            "description": description,
            "depends_on": deps,
            "status": status,
            "attempt_count": own.len(),
            "started_at": latest("started_at"),
            "completed_at": latest("completed_at"),
            "duration_ms": latest("duration_ms"),
            "attempt_outcome": latest("attempt_outcome"),
            "error": latest("error")
        }));
    }
    Ok(subtasks)
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

    let attempt_completion_rate: Option<f64> =
        results_aggregate(db, task_id, ATTEMPT_COMPLETION_RATE, FINISHED_ONLY);

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
            "attempt_completion_rate": attempt_completion_rate
        },
        "subtasks": subtask_statuses
    })
    .to_string()
}
