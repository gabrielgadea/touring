//! CLI workflow handlers (`cli_workflow_*`) — extracted from cli_handlers.rs (A-W2.P3).
//!
//! Workflow-stage telemetry queries over the decompose tables. The shared
//! `ensure_decompose_tables` helper stays in cli_handlers.rs.

use crate::cli_handlers::ensure_decompose_tables;
use crate::runtime::HookRuntime;

/// Reports workflow-stage run telemetry for a task over the decompose tables as JSON.
pub fn cli_workflow_run(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let resume = payload
        .get("resume")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    if task_id.is_empty() {
        return serde_json::json!({ "error" : "task_id required" }).to_string();
    }
    let subtasks: Vec<(String, String, String)> = {
        let mut stmt = match db
            .conn_ref()
            .prepare(
                "SELECT subtask_id, description, status FROM decomposition_subtasks WHERE task_id = ?1 ORDER BY created_at ASC",
            )
        {
            Ok(s) => s,
            Err(e) => return serde_json::json!({ "error" : format!("{e}") }).to_string(),
        };
        stmt.query_map(rusqlite::params![task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };
    serde_json::json!(
        { "task_id" : task_id, "resume" : resume, "subtasks" : subtasks, "total" :
        subtasks.len() }
    )
    .to_string()
}
/// Reports aggregate workflow-stage statistics over the decompose tables as JSON.
pub fn cli_workflow_stats(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    if task_id.is_empty() {
        return serde_json::json!({ "error" : "task_id required" }).to_string();
    }
    let stats: (i64, i64, i64, i64, i64) = {
        let mut stmt = match db
            .conn_ref()
            .prepare(
                "SELECT status, COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 GROUP BY status",
            )
        {
            Ok(s) => s,
            Err(_) => return serde_json::json!({ "error" : "query failed" }).to_string(),
        };
        let mut completed = 0i64;
        let mut failed = 0i64;
        let mut skipped = 0i64;
        let mut pending = 0i64;
        let mut in_progress = 0i64;
        let rows = stmt
            .query_map(rusqlite::params![task_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map(|iter| iter.filter_map(|r| r.ok()).collect::<Vec<_>>())
            .unwrap_or_default();
        for (status, count) in rows {
            match status.as_str() {
                "completed" => completed = count,
                "failed" => failed = count,
                "skipped" => skipped = count,
                "pending" => pending = count,
                "in_progress" => in_progress = count,
                _ => {}
            }
        }
        (completed, failed, skipped, pending, in_progress)
    };
    serde_json::json!(
        { "task_id" : task_id, "completed" : stats.0, "failed" : stats.1, "skipped" :
        stats.2, "pending" : stats.3, "in_progress" : stats.4 }
    )
    .to_string()
}
/// Reports the slowest workflow stages by recorded duration as JSON.
pub fn cli_workflow_slowest(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let top = payload.get("top").and_then(|v| v.as_i64()).unwrap_or(5) as usize;
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    if task_id.is_empty() {
        return serde_json::json!({ "error" : "task_id required" }).to_string();
    }
    let results: Vec<(String, i64)> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT r.subtask_id, r.duration_ms FROM subtask_results r
             JOIN decomposition_subtasks s ON s.subtask_id = r.subtask_id
             WHERE s.task_id = ?1 AND r.duration_ms IS NOT NULL
             ORDER BY r.duration_ms DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({ "error" : "query failed" }).to_string(),
        };
        stmt.query_map(rusqlite::params![task_id, top as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };
    serde_json::json!(
        { "task_id" : task_id, "slowest" : results, "count" : results.len() }
    )
    .to_string()
}
/// Compares workflow-stage telemetry across runs to surface regressions as JSON.
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
    if task_a.is_empty() || task_b.is_empty() {
        return serde_json::json!({ "error" : "both task_id_a and task_id_b required" })
            .to_string();
    }
    let fetch_stats = |tid: &str| -> serde_json::Value {
        let rows: Vec<(String, i64)> = {
            let mut stmt = match db
                .conn_ref()
                .prepare(
                    "SELECT status, COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 GROUP BY status",
                )
            {
                Ok(s) => s,
                Err(_) => return serde_json::json!({}),
            };
            stmt.query_map(rusqlite::params![tid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        };
        serde_json::json!({ "task_id" : tid, "status_breakdown" : rows })
    };
    serde_json::json!({ "task_a" : fetch_stats(task_a), "task_b" : fetch_stats(task_b) })
        .to_string()
}
