//! Workflow CLI handlers (`cli_workflow_*`) — extracted from decompose.rs (F-9).
//! Re-exported from `cli_handlers_decompose` so historic call paths resolve unchanged.

use crate::cli_handlers_decompose::ensure_decompose_tables;
use crate::knowledge::FileKnowledgeDB;
use crate::runtime::HookRuntime;
use rusqlite::params;

// ─────────────────────────────────────────────────────────────────────────────
// Feature C & B: Workflow analytics + execution tracking
// ─────────────────────────────────────────────────────────────────────────────

/// Return analytics for a specific task (Feature C).
pub fn cli_workflow_stats(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let total_subtasks: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let completed: i64 = db.conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'completed'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let failed: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'failed'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Aggregate from subtask_results
    let avg_duration_ms: Option<f64> = db
        .conn_ref()
        .query_row(
            "SELECT AVG(duration_ms) FROM subtask_results sr
             JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id
             WHERE st.task_id = ?1 AND sr.duration_ms IS NOT NULL",
            params![task_id],
            |r| r.get::<_, f64>(0),
        )
        .ok();

    let cache_hit_rate: Option<f64> = db
        .conn_ref()
        .query_row(
            "SELECT CAST(SUM(cache_hit) AS REAL) / COUNT(*) FROM subtask_results sr
             JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id
             WHERE st.task_id = ?1",
            params![task_id],
            |r| r.get::<_, f64>(0),
        )
        .ok();

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
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
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
        let total: i64 = db
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1",
                params![task_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let completed: i64 = db.conn_ref()
            .query_row("SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'completed'", params![task_id], |r| r.get(0))
            .unwrap_or(0);
        let avg_ms: Option<f64> = db.conn_ref()
            .query_row(
                "SELECT AVG(sr.duration_ms) FROM subtask_results sr JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id WHERE st.task_id = ?1 AND sr.duration_ms IS NOT NULL",
                params![task_id],
                |r| r.get::<_, f64>(0),
            )
            .ok();
        let cache_rate: Option<f64> = db.conn_ref()
            .query_row(
                "SELECT CAST(SUM(sr.cache_hit) AS REAL) / COUNT(*) FROM subtask_results sr JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id WHERE st.task_id = ?1",
                params![task_id],
                |r| r.get::<_, f64>(0),
            )
            .ok();
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
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Fetch task + subtasks
    let task: Option<serde_json::Value> = db
        .conn_ref()
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
        .ok();

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
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Get task info
    let task: Option<serde_json::Value> = db
        .conn_ref()
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
        .ok();

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
            let scoped_id = if raw_id.starts_with(&format!("{}::", task_id)) {
                raw_id.clone()
            } else {
                format!("{}::{}", task_id, raw_id)
            };
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
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

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
    let total: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let pending: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'pending'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let in_progress: i64 = db.conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'in_progress'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let completed: i64 = db.conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'completed'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let failed: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'failed'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let cancelled: i64 = db.conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 AND status = 'cancelled'",
            params![task_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Aggregate timing from subtask_results
    let total_duration_ms: Option<i64> = db
        .conn_ref()
        .query_row(
            "SELECT SUM(sr.duration_ms) FROM subtask_results sr
             JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id
             WHERE st.task_id = ?1 AND sr.duration_ms IS NOT NULL",
            params![task_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();

    let avg_duration_ms: Option<f64> = db
        .conn_ref()
        .query_row(
            "SELECT AVG(sr.duration_ms) FROM subtask_results sr
             JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id
             WHERE st.task_id = ?1 AND sr.duration_ms IS NOT NULL",
            params![task_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();

    let cache_hit_rate: Option<f64> = db
        .conn_ref()
        .query_row(
            "SELECT CAST(SUM(sr.cache_hit) AS REAL) / COUNT(*)
             FROM subtask_results sr
             JOIN decomposition_subtasks st ON st.subtask_id = sr.subtask_id
             WHERE st.task_id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();

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
            let scoped_id = if raw_id.starts_with(&format!("{}::", task_id)) {
                raw_id.clone()
            } else {
                format!("{}::{}", task_id, raw_id)
            };
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
