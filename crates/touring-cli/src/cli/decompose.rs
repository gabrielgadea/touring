//! CLI decompose/DAG handlers (`cli_decompose_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Task/subtask CRUD + DAG validation (cycle detection) + status/finalize/ready
//! over the decompose tables. `cli_decompose_finalize`/`cli_decompose_ready`
//! are thin wrappers delegating to `crate::cli_handlers_decompose::*` (fully
//! qualified). The shared `ensure_decompose_tables` helper stays in
//! cli_handlers.rs.

use crate::cli_handlers::ensure_decompose_tables;
use crate::runtime::HookRuntime;
use crate::schemas::validate_payload;
use rusqlite::params;

// ── F1-ROUTING (2026-07-24) — task-home store resolution ─────────────────────
//
// Root cause of the "lost DAG" incidents (2x on 2026-07-24): decompose stores
// are per-project, but a task LIVES in the store where it was created. A
// command issued from a different cwd read/wrote the CURRENT project's store:
// `get` answered "not found" for a healthy task and `update` touched 0 rows
// while still answering success (orphan write). Forensics proved NO data was
// ever lost — both "lost" DAGs sat intact (one in the global store, one in the
// project store). These helpers route task-addressed reads AND writes to the
// store that actually holds the task: the current project first, then the
// global store (`~/.claude/touring/knowledge.db`).
//
// Scope note: `finalize`/`ready` delegate to `cli_handlers_decompose` (own db
// access) and stay local-store; routing them is follow-up work — the two
// operations broken in the incidents (`get`, `update`) plus `add`/`validate`
// are covered here.

/// Does this store hold the task container?
fn task_in_db(db: &crate::knowledge::FileKnowledgeDB, task_id: &str) -> bool {
    db.conn_ref()
        .query_row(
            "SELECT 1 FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |_| Ok(()),
        )
        .is_ok()
}

/// The store a task-addressed operation must run against.
enum TaskStore<'a> {
    Local(&'a crate::knowledge::FileKnowledgeDB),
    Global(crate::knowledge::FileKnowledgeDB),
}

impl TaskStore<'_> {
    fn db(&self) -> &crate::knowledge::FileKnowledgeDB {
        match self {
            TaskStore::Local(db) => db,
            TaskStore::Global(db) => db,
        }
    }
}

/// Locate the store holding `task_id`: local project store first, then the
/// global store. `None` means the task exists in NEITHER — writers must fail
/// loud instead of orphan-writing.
fn locate_task_store<'a>(
    local: &'a crate::knowledge::FileKnowledgeDB,
    task_id: &str,
) -> Option<TaskStore<'a>> {
    if task_in_db(local, task_id) {
        return Some(TaskStore::Local(local));
    }
    let home = std::env::var("HOME").ok()?;
    let global_path = std::path::Path::new(&home)
        .join(".claude")
        .join("touring")
        .join("knowledge.db");
    if !global_path.exists() {
        return None;
    }
    let global = crate::knowledge::FileKnowledgeDB::new(&global_path).ok()?;
    task_in_db(&global, task_id).then_some(TaskStore::Global(global))
}

/// Creates a new decomposition task (DAG root) from a task type and description, returning its id as JSON.
pub fn cli_decompose_create(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_type = payload
        .get("task_type")
        .and_then(|v| v.as_str())
        .unwrap_or("general");
    let description = payload
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let origin = payload
        .get("origin")
        .and_then(|v| v.as_str())
        .unwrap_or("claude-code");
    let mirrored_to_cc: i64 = if origin == "claude-code" { 1 } else { 0 };
    let cila_level: i64 = payload
        .get("cila_level")
        .and_then(|v| v.as_i64())
        .unwrap_or(3)
        .clamp(0, 6);
    let priority_token = payload
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("normal")
        .to_string();
    // F0-pre (2026-07-20): honor an explicit `task_id` from the payload. Always
    // generating a fresh nanos id here was the double-create engine: the two
    // task-sync callers (PostToolUse + TaskCreated event) each minted their own
    // container for the same CC task, and TaskUpdate — which addresses rows by
    // the CC id — never matched any container, so status NEVER propagated.
    // With the id honored, `INSERT OR IGNORE` makes the second caller a no-op
    // and updates find their row. Explicit ids also serve the ADW runner.
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| {
            format!(
                "task_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            )
        });
    let now = chrono::Utc::now().to_rfc3339();
    let cila_provided = payload.get("cila_level").is_some();
    let (bandit_split_factor, bandit_subtasks) = if !cila_provided {
        let factor = rt.select_task_split(0, "general", cila_level.min(4) as u8);
        let count = factor.subtask_count() as i64;
        (format!("{factor:?}"), count)
    } else {
        (String::from("explicit"), cila_level)
    };
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let result = db
        .conn_ref()
        .execute(
            "INSERT OR IGNORE INTO task_decompositions \
         (task_id, task_type, description, status, created_at, updated_at, origin, mirrored_to_cc, cila_level) \
         VALUES (?1, ?2, ?3, 'created', ?4, ?4, ?5, ?6, ?7)",
            params![
                task_id, task_type, description, now, origin, mirrored_to_cc, cila_level
            ],
        );
    if let Err(e) = &result {
        tracing::debug!("decompose create INSERT failed: {}", e);
    }
    crate::cli_handlers_decompose::log_event(
        db,
        &task_id,
        None,
        "task_created",
        &serde_json::json!(
            { "task_type" : task_type, "description" : description, "origin" : origin,
            "cila_level" : cila_level, "bandit_split_factor" : bandit_split_factor,
            "bandit_subtasks" : bandit_subtasks }
        ),
    );
    serde_json::json!(
        { "task_id" : task_id, "task_type" : task_type, "description" : description,
        "status" : "created", "created_at" : now, "origin" : origin, "mirrored_to_cc" :
        mirrored_to_cc == 1, "cila_level" : cila_level, "priority" : priority_token,
        "persisted" : result.is_ok(), "bandit_split_factor" : bandit_split_factor,
        "bandit_subtasks" : bandit_subtasks, }
    )
    .to_string()
}
/// Mark an existing decompose task as mirrored to Claude Code.
///
/// Called by `task-sync-post-create` hook when CC creates a TaskCreate with
/// `external_ref` pointing to a Touring-originated task. This closes the
/// bidirectional loop without duplicating DAG entries.
pub fn cli_decompose_mark_mirrored(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() {
        return serde_json::json!({ "marked" : false, "reason" : "missing task_id" }).to_string();
    }
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let result = db.conn_ref().execute(
        "UPDATE task_decompositions SET mirrored_to_cc = 1, updated_at = ?2 WHERE task_id = ?1",
        params![task_id, chrono::Utc::now().to_rfc3339()],
    );
    let updated = result.unwrap_or(0);
    serde_json::json!(
        { "marked" : updated > 0, "task_id" : task_id, "rows_updated" : updated, }
    )
    .to_string()
}
/// Adds a subtask to an existing task's DAG, recording its dependencies on other subtasks.
pub fn cli_decompose_add(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    ensure_decompose_tables(&rt.ctx.knowledge);
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subtask_id = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let description = payload
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let depends_on: Vec<String> = payload
        .get("depends_on")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let priority_token = payload
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("normal");
    let priority_int = crate::cli_handlers_decompose::parse_priority_token(priority_token);
    let now = chrono::Utc::now().to_rfc3339();
    let local = &rt.ctx.knowledge;
    // F1-ROUTING: subtasks attach to the task's home store; a task in NO
    // store is a loud error (the old code inserted orphan subtask rows into
    // whatever project the caller happened to be in).
    let store = match locate_task_store(local, task_id) {
        Some(s) => s,
        None => {
            return serde_json::json!({
                "task_id": task_id,
                "persisted": false,
                "error": "task not found in any decompose store (project or global)",
                "hint": "create the task first (decompose create) or run from the project it was created in",
            })
            .to_string();
        }
    };
    let db = store.db();
    let deps_json = serde_json::to_string(&depends_on).unwrap_or_else(|_| "[]".to_string());
    let scoped_id = if subtask_id.contains("::") {
        subtask_id.to_string()
    } else {
        format!("{}::{}", task_id, subtask_id)
    };
    let deadline = payload.get("deadline").and_then(|v| v.as_str());
    let deadline_behavior = payload
        .get("deadline_behavior")
        .and_then(|v| v.as_str())
        .unwrap_or("Fail");
    let parallel_group = payload.get("parallel_group").and_then(|v| v.as_str());
    let result = db
        .conn_ref()
        .execute(
            "INSERT OR REPLACE INTO decomposition_subtasks (subtask_id, task_id, description, depends_on, priority, status, deadline, deadline_behavior, parallel_group, review_required, complexity_hint, retry_policy, attempts, quality_score, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, 0, NULL, NULL, 0, NULL, ?9, ?10)",
            params![
                scoped_id, task_id, description, deps_json, priority_int, deadline,
                deadline_behavior, parallel_group, now, now
            ],
        );
    if let Err(e) = &result {
        tracing::debug!("decompose add INSERT failed: {}", e);
    }
    crate::cli_handlers_decompose::log_event(
        db,
        task_id,
        Some(&scoped_id),
        "subtask_added",
        &serde_json::json!(
            { "scoped_id" : scoped_id, "description" : description, "depends_on" :
            depends_on, "priority" : priority_int, "deadline" : deadline,
            "deadline_behavior" : deadline_behavior }
        ),
    );
    serde_json::json!(
        { "task_id" : task_id, "subtask_id" : subtask_id, "scoped_id" : scoped_id,
        "description" : description, "depends_on" : depends_on, "status" : "pending",
        "priority" : crate ::cli_handlers_decompose::priority_label(priority_int),
        "priority_int" : priority_int, "created_at" : now, "persisted" : result.is_ok(),
        "parallel_group" : parallel_group }
    )
    .to_string()
}
/// Retrieves a task and its full subtask DAG as JSON.
pub fn cli_decompose_get(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let local = &rt.ctx.knowledge;
    ensure_decompose_tables(local);
    // F1-ROUTING: read from the task's home store (local, then global).
    let store = locate_task_store(local, task_id);
    let db = match &store {
        Some(s) => s.db(),
        None => local, // falls through to the explicit not-found error below
    };
    let task: Option<serde_json::Value> = db
        .conn_ref()
        .query_row(
            "SELECT task_id, task_type, description, status, created_at, updated_at FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| {
                Ok(
                    serde_json::json!(
                        { "task_id" : r.get::< _, String > (0) ?, "task_type" : r.get::<
                        _, String > (1) ?, "description" : r.get::< _, String > (2) ?,
                        "status" : r.get::< _, String > (3) ?, "created_at" : r.get::< _,
                        String > (4) ?, "updated_at" : r.get::< _, String > (5) ? }
                    ),
                )
            },
        )
        .ok();
    let subtasks: Vec<serde_json::Value> = {
        let mut stmt = match db
            .conn_ref()
            .prepare(
                "SELECT subtask_id, description, depends_on, status, priority FROM decomposition_subtasks WHERE task_id = ?1",
            )
        {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("decompose get subtasks prepare failed: {}", e);
                return serde_json::json!({ "error" : format!("db error: {}", e) })
                    .to_string();
            }
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(2)?;
            let depends_on: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                deps_str
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            });
            Ok(serde_json::json!(
                { "subtask_id" : r.get::< _, String > (0) ?, "description" :
                r.get::< _, String > (1) ?, "depends_on" : depends_on,
                "status" : r.get::< _, String > (3) ?, "priority" : r.get::<
                _, i32 > (4) ? }
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };
    // F0-pre (2026-07-20): a missing task carries an explicit `error` — the bare
    // `{"task":null}` shape (no error key) made consumers classify "missing" as
    // "empty-and-fine" (loop-engineering stop-guard Bug B, 02/07) and gave no
    // clue that decompose stores are per-project (mis-sharded lookups).
    if task.is_none() {
        return serde_json::json!({
            "task": task,
            "subtasks": subtasks,
            "subtask_count": subtasks.len(),
            "error": "task not found in this project's decompose store",
            "hint": "decompose data is per-project — verify the cwd/project the task was created from",
        })
        .to_string();
    }
    serde_json::json!(
        { "task" : task, "subtasks" : subtasks, "subtask_count" : subtasks.len() }
    )
    .to_string()
}
/// Updates a subtask's mutable fields (status, title, dependencies) in place.
pub fn cli_decompose_update(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = payload.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let priority = payload
        .get("priority")
        .and_then(|v| v.as_i64())
        .map(|p| p as i32);
    let quality_score = payload.get("quality_score").and_then(|v| v.as_f64());
    let depends_on: Option<Vec<String>> = payload.get("depends_on").and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
    });
    let local = &rt.ctx.knowledge;
    ensure_decompose_tables(local);
    // F1-ROUTING: write to the task's home store; a task in NO store is a
    // loud error — the old code UPDATEd the current project's store and
    // answered success with 0 rows affected (the orphan-write incident).
    let store = match locate_task_store(local, task_id) {
        Some(s) => s,
        None => {
            return serde_json::json!({
                "task_id": task_id,
                "updated": false,
                "subtask_updated": false,
                "error": "task not found in any decompose store (project or global)",
                "hint": "decompose data is per-project; the task may have been created from another cwd — run from that project or recreate",
            })
            .to_string();
        }
    };
    let db = store.db();
    let now = chrono::Utc::now().to_rfc3339();
    let task_affected = if !status.is_empty() {
        db.conn_ref()
            .execute(
                "UPDATE task_decompositions SET status = ?1, updated_at = ?3 WHERE task_id = ?2",
                params![status, task_id, now],
            )
            .unwrap_or(0)
    } else {
        0
    };
    let has_subtask_id = payload.get("subtask_id").and_then(|v| v.as_str()).is_some();
    let subtask_affected = if has_subtask_id
        || priority.is_some()
        || quality_score.is_some()
        || depends_on.is_some()
    {
        let raw_subtask_id = payload
            .get("subtask_id")
            .and_then(|v| v.as_str())
            .unwrap_or(task_id);
        let subtask_id_owned = if raw_subtask_id.contains("::") {
            raw_subtask_id.to_string()
        } else {
            format!("{}::{}", task_id, raw_subtask_id)
        };
        let mut sets: Vec<String> = Vec::new();
        let mut sql_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if !status.is_empty() {
            sets.push(format!("status = ?{}", sql_params.len() + 1));
            sql_params.push(Box::new(status.to_string()));
        }
        if let Some(p) = priority {
            sets.push(format!("priority = ?{}", sql_params.len() + 1));
            sql_params.push(Box::new(p));
        }
        if let Some(q) = quality_score {
            sets.push(format!("quality_score = ?{}", sql_params.len() + 1));
            sql_params.push(Box::new(q));
        }
        if let Some(deps) = depends_on.as_ref() {
            let deps_json = serde_json::to_string(deps).unwrap_or_else(|_| "[]".to_string());
            sets.push(format!("depends_on = ?{}", sql_params.len() + 1));
            sql_params.push(Box::new(deps_json));
        }
        if sets.is_empty() {
            0
        } else {
            let sql = format!(
                "UPDATE decomposition_subtasks SET {} WHERE subtask_id = ?{}",
                sets.join(", "),
                sql_params.len() + 1
            );
            sql_params.push(Box::new(subtask_id_owned.clone()));
            let param_refs: Vec<&dyn rusqlite::ToSql> =
                sql_params.iter().map(|p| p.as_ref()).collect();
            db.conn_ref()
                .execute(&sql, param_refs.as_slice())
                .unwrap_or(0) as i64
        }
    } else {
        0
    };
    if subtask_affected > 0 {
        let raw_subtask_id = payload
            .get("subtask_id")
            .and_then(|v| v.as_str())
            .unwrap_or(task_id);
        let scoped_id = if raw_subtask_id.contains("::") {
            raw_subtask_id.to_string()
        } else {
            format!("{}::{}", task_id, raw_subtask_id)
        };
        let event_type = match status {
            "in_progress" => "subtask_started",
            "completed" => "subtask_completed",
            "failed" => "subtask_failed",
            _ => {
                return serde_json::json!(
                    { "task_id" : task_id, "status" : status, "updated" : task_affected >
                    0, "subtask_updated" : subtask_affected > 0, "priority" : priority,
                    "quality_score" : quality_score }
                )
                .to_string();
            }
        };
        crate::cli_handlers_decompose::log_event(
            db,
            task_id,
            Some(&scoped_id),
            event_type,
            &serde_json::json!(
                { "scoped_id" : scoped_id, "status" : status, "priority" : priority,
                "quality_score" : quality_score }
            ),
        );
    }
    serde_json::json!(
        { "task_id" : task_id, "status" : status, "updated" : task_affected > 0,
        "subtask_updated" : subtask_affected > 0, "priority" : priority, "quality_score"
        : quality_score }
    )
    .to_string()
}
/// Validates a task's DAG for structural integrity, detecting dependency cycles and dangling references.
pub fn cli_decompose_validate(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let local = &rt.ctx.knowledge;
    // F1-ROUTING: validate against the task's home store (local, then global).
    let store = locate_task_store(local, task_id);
    let db = match &store {
        Some(s) => s.db(),
        None => local,
    };
    ensure_decompose_tables(db);
    // F0-pre (2026-07-20): a nonexistent task must NEVER validate as true — the
    // old behavior ({"valid":true} on an empty subtask set for a missing task)
    // let convergence gates pass over vanished/mis-sharded DAGs (audit 20/07).
    let task_exists = db
        .conn_ref()
        .query_row(
            "SELECT 1 FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |_| Ok(()),
        )
        .is_ok();
    if !task_exists {
        return serde_json::json!({
            "valid": false,
            "task_id": task_id,
            "error": "task not found in this project's decompose store",
            "hint": "decompose data is per-project — verify the cwd/project the task was created from",
        })
        .to_string();
    }
    let subtasks: Vec<(String, String)> = {
        let mut stmt = match db
            .conn_ref()
            .prepare("SELECT subtask_id, depends_on FROM decomposition_subtasks WHERE task_id = ?1")
        {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("decompose validate prepare failed: {}", e);
                return serde_json::json!(
                    { "valid" : false, "error" : format!("db error: {}", e) }
                )
                .to_string();
            }
        };
        stmt.query_map(params![task_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };
    let mut graph: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (subtask_id, deps) in &subtasks {
        let dep_list: Vec<String> = serde_json::from_str(deps).unwrap_or_else(|_| {
            deps.split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        });
        graph.insert(subtask_id.clone(), dep_list);
    }
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut rec_stack: std::collections::HashSet<String> = std::collections::HashSet::new();
    fn has_cycle(
        node: &str,
        graph: &std::collections::HashMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        rec_stack: &mut std::collections::HashSet<String>,
    ) -> bool {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        if let Some(deps) = graph.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if has_cycle(dep, graph, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    return true;
                }
            }
        }
        rec_stack.remove(node);
        false
    }
    let mut has_cycles = false;
    for node in graph.keys() {
        if !visited.contains(node) && has_cycle(node, &graph, &mut visited, &mut rec_stack) {
            has_cycles = true;
            break;
        }
    }
    serde_json::json!(
        { "task_id" : task_id, "valid" : ! has_cycles, "has_cycles" : has_cycles,
        "subtask_count" : subtasks.len() }
    )
    .to_string()
}
/// Summarizes progress across all decomposition tasks (subtask counts by status) as JSON.
pub fn cli_decompose_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let total_tasks: i64 = db
        .conn_ref()
        .query_row("SELECT COUNT(*) FROM task_decompositions", [], |r| r.get(0))
        .unwrap_or(0);
    let total_subtasks: i64 = db
        .conn_ref()
        .query_row("SELECT COUNT(*) FROM decomposition_subtasks", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    serde_json::json!({ "total_tasks" : total_tasks, "total_subtasks" : total_subtasks })
        .to_string()
}
/// Finalize a task: verify all subtasks are terminal, compute per-status counts,
/// mark the task as archived, and inject an RL reward on success.
///
/// Returns JSON: `{ready, archived, completion_pct, total_subtasks, completed,
/// failed, skipped, cancelled, pending, in_progress, blocking, rl_reward_injected}`.
///
/// S1.5: review_required gate + S1.6: snapshots + S1.7: metrics + S1.1: deadline check
/// Delegated to cli_handlers_decompose::cli_decompose_finalize (new impl with all gaps fixed).
pub fn cli_decompose_finalize(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = match payload.get("task_id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return serde_json::json!({ "error" : "task_id required" }).to_string(),
    };
    let new_result = crate::cli_handlers_decompose::cli_decompose_finalize(rt, payload);
    if let Ok(result) = serde_json::from_str::<serde_json::Value>(&new_result) {
        if result.get("error").is_none() {
            let completion_pct = result
                .get("metrics")
                .and_then(|m| m.get("completion_pct"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let ctx = format!("decompose_finalize:task_id={task_id}:pct={completion_pct:.0}");
            // Sprint 2 PB (REGRA #19): reap the child to prevent <defunct>
            // zombies. The daemon runs indefinitely and never calls waitpid
            // implicitly — `Drop<Child>` only closes the fd, it does NOT wait.
            let _ = std::thread::spawn(move || {
                if let Ok(mut child) = std::process::Command::new("touring")
                    .args(["learning", "reward", "orchestrate", "1.0", &ctx])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    let _ = child.wait();
                }
            });
        }
    }
    new_result
}
/// Return subtasks ready to start: status = 'pending' and all dependency subtasks completed.
///
/// Optional `task_id` filter via payload; if empty, scans all tasks.
/// Returns JSON: `{ready_count, ready_subtasks: [{task_id, subtask_id, description}]}`.
pub fn cli_decompose_ready(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let by_priority = payload
        .get("by_priority")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let only_ready = payload
        .get("only_ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let delegate_payload = serde_json::json!(
        { "task_id" : task_id, "only_ready" : only_ready }
    );
    let result = crate::cli_handlers_decompose::cli_decompose_ready(rt, &delegate_payload);
    if by_priority {
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&result) {
            if let Some(arr) = val.get_mut("ready_subtasks").and_then(|v| v.as_array_mut()) {
                arr.sort_by(|a, b| {
                    let pa = a.get("priority").and_then(|v| v.as_i64()).unwrap_or(255);
                    let pb = b.get("priority").and_then(|v| v.as_i64()).unwrap_or(255);
                    pa.cmp(&pb)
                });
            }
            if let Some(arr) = val
                .get_mut("parallel_groups")
                .and_then(|v| v.as_array_mut())
            {
                for group in arr {
                    if let Some(members) = group.get_mut("members").and_then(|v| v.as_array_mut()) {
                        members.sort_by(|a, b| {
                            let pa = a.get("priority").and_then(|v| v.as_i64()).unwrap_or(255);
                            let pb = b.get("priority").and_then(|v| v.as_i64()).unwrap_or(255);
                            pa.cmp(&pb)
                        });
                    }
                }
            }
            val.as_object_mut()
                .map(|m| m.insert("sorted_by_priority".to_string(), serde_json::json!(true)));
            return val.to_string();
        }
    }
    let mut val = match serde_json::from_str::<serde_json::Value>(&result) {
        Ok(v) => v,
        Err(_) => return result,
    };
    val.as_object_mut()
        .map(|m| m.insert("sorted_by_priority".to_string(), serde_json::json!(false)));
    val.to_string()
}
/// Handle decompose-event subcommand from touring-hook binary.
///
/// Parses stdin JSON with event_type (TaskCreated | TaskCompleted),
/// session_id, and task_data. Maintains in-memory session→task mapping
/// via HookRuntime.decompose_event_state.
///
/// - TaskCreated: calls `touring decompose create <desc>` via Command,
///   stores session_id→task_id, returns {status: "ok", task_id, session_id}
/// - TaskCompleted: looks up task_id from session map, calls
///   `touring decompose add <task_id> <desc>`, removes from map,
///   returns {status: "ok"}
///
/// Fire-and-forget subprocess calls with 10s timeout.
/// Never fails — always returns ok with graceful degradation.
pub fn cli_decompose_event(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    if validate_payload::<crate::schemas::DecomposeEventPayload>(payload).is_err() {
        return serde_json::json!({ "status" : "ok" }).to_string();
    }
    let event_type = payload
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_data = payload
        .get("task_data")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    match event_type {
        "TaskCreated" => {
            let task_desc = task_data
                .get("description")
                .or_else(|| task_data.get("task_description"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let desc = task_desc.to_string();
            // Sprint 2 PB (REGRA #19): reap to prevent <defunct> zombies.
            let _ = std::thread::spawn(move || {
                if let Ok(mut child) = std::process::Command::new("touring")
                    .args(["decompose", "create", "general", &desc])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    let _ = child.wait();
                }
            });
            let session_prefix: String = session_id.chars().take(8).collect();
            let task_id = format!("decompose-session-{}", session_prefix);
            rt.decompose_event_state
                .insert(session_id.to_string(), task_id.clone());
            let generator_hint = if task_desc.len() > 3 {
                let hints =
                    touring_hooks_core::generator_hints::collect_subject_generator_hints(task_desc);
                if !hints.is_empty() {
                    format!("scaffold: {}", hints.join(" | "))
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            serde_json::json!(
                { "status" : "ok", "task_id" : task_id, "session_id" : session_id,
                "generator_hint" : generator_hint, }
            )
            .to_string()
        }
        "TaskCompleted" => {
            if let Some(task_id) = rt.decompose_event_state.remove(session_id) {
                let completion_desc = task_data
                    .get("description")
                    .or_else(|| task_data.get("result_summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed");
                let tid = task_id.clone();
                let desc = completion_desc.to_string();
                // Sprint 2 PB (REGRA #19): reap to prevent <defunct> zombies.
                let _ = std::thread::spawn(move || {
                    if let Ok(mut child) = std::process::Command::new("touring")
                        .args(["decompose", "add", &tid, &desc])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        let _ = child.wait();
                    }
                });
            }
            serde_json::json!({ "status" : "ok" }).to_string()
        }
        _ => serde_json::json!({ "status" : "ok", "skipped" : "unknown_event_type" }).to_string(),
    }
}
