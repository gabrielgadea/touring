//! Hook ↔ Decompose Bridge — Wires hook events to the decompose task system.
//!
//! ## Purpose
//! This module bridges the gap between hook events (task-created, post-tool-rl,
//! teammate-idle-gate, cognitive output, pre-compact, post-edit quality) and the
//! decompose task management system via `cli_decompose_*` handlers.
//!
//! ## Design Principles
//! 1. **Fire-and-forget**: All bridge calls are fallible — never block Claude Code
//! 2. **Exit 0 invariant**: Every function preserves the hook exit guarantee
//! 3. **No new dependencies**: Reuses existing `cli_decompose_*` handlers
//! 4. **HookRuntime as context**: All functions accept `&mut HookRuntime`
//!
//! ## Integration Points
//! - `task_created` hook → `bridge_task_created` → `cli_decompose_create`
//! - `post_tool_rl` (success) → `bridge_post_tool_success` → `cli_decompose_update`
//! - `teammate_idle_gate` → `bridge_idle_gate_queue_state` → `cli_decompose_get`
//! - `cognitive` (high complexity) → `bridge_cognitive_mcts_trigger` → MCTS planning
//! - `pre_compact` → `bridge_precompact_checkpoint` → decompose state checkpoint
//! - `post_edit` quality → `bridge_post_edit_quality` → decompose subtask quality score

use crate::cli_handlers::{cli_decompose_create, cli_decompose_update};
use crate::runtime::HookRuntime;
use rusqlite::params;
use serde_json::json;

/// Typed error returned by decompose-bridge functions that query SQLite directly.
///
/// All bridge functions are fire-and-forget — callers log at `tracing::debug!` level
/// and continue.  The `Display` impl (via `thiserror`) is the only surface callers
/// observe; no variant carries non-`String` state so the type is `Send + Sync`.
#[derive(Debug, thiserror::Error)]
pub enum DecomposeBridgeError {
    /// Preparing a SQLite statement failed.
    #[error("{0}")]
    SqlitePrepareFailed(String),
    /// Executing or querying a SQLite statement failed.
    #[error("{0}")]
    SqliteQueryFailed(String),
}

/// Complexity threshold above which cognitive engine triggers MCTS planning.
pub(crate) const MCTS_COMPLEXITY_THRESHOLD: f64 = 0.7;

/// Bridge: `task_created` hook event → create decompose task.
///
/// Called from `run_task_created` in team_hooks.rs after recording to knowledge DB.
/// This enables automatic decompose DAG creation from agent team task events.
///
/// # Arguments
/// * `runtime` — HookRuntime context (provides knowledge DB access)
/// * `task_id` — Unique task identifier from the team event
/// * `task_subject` — Human-readable task description
/// * `session_id` — Current session identifier
/// * `teammate_name` — Optional name of teammate who owns the task
/// * `team_name` — Optional team name
///
/// # Example
/// ```ignore
/// if let Err(e) = bridge_task_created(runtime, "task_123", "Implement feature X", "sess_abc", Some("alice"), Some("team_a")) {
///     tracing::debug!("task-created decompose bridge failed: {}", e);
/// }
/// ```
/// F0-pre (2026-07-20): deterministic mirror id for a CC-originated task.
///
/// CC task ids are small session-scoped ints ("1", "2", …); mirroring them
/// verbatim would collide with nothing but reads ambiguously next to the
/// nanos-generated `task_*` ids. The `cc_task_` prefix makes provenance
/// explicit and gives every caller in the sync chain (create, the scaffold
/// subtasks, update, complete) ONE addressing convention. Idempotent: an id
/// that already carries a touring/mirror prefix passes through unchanged.
#[must_use]
pub fn cc_mirror_task_id(cc_id: &str) -> String {
    if cc_id.starts_with("task_") || cc_id.starts_with("cc_task_") {
        cc_id.to_string()
    } else {
        format!("cc_task_{cc_id}")
    }
}

/// Mirror a CC-originated task into the decompose DAG (idempotent per mirror id).
///
/// Reached from BOTH `task-sync-create` (PostToolUse TaskCreate) and the
/// `task-created` lifecycle event; the mirror-exists check below makes the
/// second arrival a dedup no-op instead of a duplicate container (F0-pre).
pub fn bridge_task_created(
    runtime: &mut HookRuntime,
    task_id: &str,
    task_subject: &str,
    session_id: &str,
    teammate_name: Option<&str>,
    team_name: Option<&str>,
) -> Result<String, String> {
    // F0-pre guard: a caller that could not resolve a REAL task id must not
    // mint a mirror — "unknown" ids from payload-shape mismatches collapsed
    // into a shared junk container (`cc_task_unknown`, observed live 20/07).
    // The path that DOES carry the id (the TaskCreated event) will mirror it.
    if task_id.is_empty() || task_id == "unknown" {
        return Err("bridge_task_created: no real task_id — mirror skipped".to_string());
    }
    let mirror_id = cc_mirror_task_id(task_id);

    // F0-pre idempotency: the bridge is reached from BOTH task-sync-create
    // (PostToolUse) and the TaskCreated lifecycle event. A mirror that already
    // exists means the other path got here first — skip both the INSERT and the
    // duplicate `task_created` audit event.
    let already = runtime
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT 1 FROM task_decompositions WHERE task_id = ?1",
            [&mirror_id],
            |_| Ok(()),
        )
        .is_ok();
    if already {
        tracing::debug!(task_id = %mirror_id, "bridge_task_created: mirror exists — deduped");
        touring_foundation::gate_metrics::record_task_sync_deduped();
        return Ok(json!({"task_id": mirror_id, "deduped": true}).to_string());
    }
    touring_foundation::gate_metrics::record_task_sync_create();

    let payload = json!({
        "task_type": "hook_event",
        "description": task_subject,
        "task_id": mirror_id,
        "session_id": session_id,
        "teammate": teammate_name,
        "team": team_name,
    });

    let result = cli_decompose_create(runtime, &payload);
    if result.contains("\"persisted\":false") || result.contains("\"error\"") {
        tracing::debug!(
            task_id = %task_id,
            result = %result,
            "bridge_task_created: decompose create may have failed"
        );
    } else {
        tracing::debug!(
            task_id = %task_id,
            "bridge_task_created: decompose task created"
        );
    }

    // F0-pre: scaffold scout→implement→validate HERE, in the single place both
    // sync paths (PostToolUse + TaskCreated event) converge — previously the
    // scaffold lived only in the PostToolUse handler, so mirrors minted by the
    // event path had no subtasks. Reached only on first creation (the dedup
    // early-return above skips repeats), so the scaffold is naturally idempotent.
    let scaffold = [
        ("scout", "scout: research context, blast radius, and wiring before changes", None),
        ("implement", "implement: apply changes with VGP verification and speculative validation", Some("scout")),
        ("validate", "validate: cargo test + wiring orphans + memory store lesson", Some("implement")),
    ];
    for (stage, description, dep) in scaffold {
        let deps: Vec<String> = dep
            .map(|d| vec![format!("{mirror_id}::{d}")])
            .unwrap_or_default();
        let add_result = crate::cli_handlers::cli_decompose_add(
            runtime,
            &json!({
                "task_id": mirror_id,
                "subtask_id": format!("{mirror_id}::{stage}"),
                "description": description,
                "depends_on": deps,
            }),
        );
        if add_result.contains("\"error\"") {
            tracing::warn!(
                task_id = %mirror_id,
                stage = stage,
                result = %add_result,
                "bridge_task_created: scaffold subtask add failed"
            );
        }
    }
    Ok(result)
}

/// Bridge: `post_tool_rl` success path → mark decompose subtask complete.
///
/// Called after RL reward is computed for a successful tool execution.
/// Updates the corresponding decompose subtask status to "completed".
///
/// # Arguments
/// * `runtime` — HookRuntime context
/// * `subtask_id` — Decompose subtask ID to mark complete (None = skip)
/// * `tool_name` — Name of the tool that was executed successfully
///
/// # Notes
/// * Fire-and-forget: errors are traced at debug level, never block
/// * If `subtask_id` is `None`, the function returns early (no error)
pub fn bridge_post_tool_success(
    runtime: &mut HookRuntime,
    subtask_id: Option<&str>,
    tool_name: &str,
) -> Result<String, String> {
    let Some(subtask_id) = subtask_id else {
        return Ok("skipped: no subtask_id".to_string());
    };

    let payload = json!({
        "task_id": subtask_id.split("::").next().unwrap_or(subtask_id),
        "subtask_id": subtask_id,
        "status": "completed",
        "tool": tool_name,
    });

    let result = cli_decompose_update(runtime, &payload);
    if result.contains("\"updated\":false") {
        tracing::debug!(
            subtask_id = %subtask_id,
            "bridge_post_tool_success: subtask not found or already completed"
        );
    } else {
        tracing::debug!(
            subtask_id = %subtask_id,
            tool = %tool_name,
            "bridge_post_tool_success: subtask marked completed"
        );
    }
    Ok(result)
}

/// Bridge: `teammate_idle_gate` → query decompose queue for pending subtasks.
///
/// Called from `run_teammate_idle_gate` to inform idle-gate decision with
/// decompose queue state. Returns JSON with pending subtasks for the teammate.
///
/// # Arguments
/// * `runtime` — HookRuntime context
/// * `teammate_name` — Name of the teammate being checked for idle
///
/// # Returns
/// JSON string with `task` and `subtasks` array, or error string.
///
/// # Notes
/// * Returns `Ok` with empty `{"task": null, "subtasks": []}` if no active task
/// * Fire-and-forget pattern: errors traced at debug level
pub fn bridge_idle_gate_queue_state(
    runtime: &mut HookRuntime,
    teammate_name: &str,
) -> Result<String, DecomposeBridgeError> {
    // Query decompose for all pending subtasks to inform idle-gate decision.
    // This gives the idle-gate awareness of whether there are pending tasks
    // without requiring a teammate→task mapping (which would need separate storage).
    let db = &runtime.ctx.knowledge;

    let pending_subtasks: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, description, task_id, priority FROM decomposition_subtasks WHERE status = 'pending' ORDER BY priority DESC LIMIT 50",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(
                    error = %e,
                    teammate = %teammate_name,
                    "bridge_idle_gate_queue_state: prepare failed"
                );
                return Err(DecomposeBridgeError::SqlitePrepareFailed(format!(
                    "prepare failed: {}",
                    e
                )));
            }
        };

        stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "subtask_id": row.get::<_, String>(0)?,
                "description": row.get::<_, String>(1)?,
                "task_id": row.get::<_, String>(2)?,
                "priority": row.get::<_, i32>(3)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    let has_pending = !pending_subtasks.is_empty();

    tracing::debug!(
        teammate = %teammate_name,
        pending_count = pending_subtasks.len(),
        has_pending = has_pending,
        "bridge_idle_gate_queue_state: decompose queue state retrieved"
    );

    Ok(json!({
        "teammate": teammate_name,
        "has_pending_tasks": has_pending,
        "pending_count": pending_subtasks.len(),
        "pending_subtasks": pending_subtasks,
    })
    .to_string())
}

/// Bridge: cognitive engine high-complexity detection → MCTS planning trigger.
///
/// Called when cognitive engine outputs a complexity score above threshold.
/// Initiates MCTS search for alternate approaches when task complexity is high.
///
/// # Arguments
/// * `runtime` — HookRuntime context
/// * `complexity_score` — Complexity score from cognitive engine (0.0–1.0)
///
/// # Notes
/// Bridge: cognitive engine high-complexity detection → MCTS planning trigger.
///
/// Called when cognitive engine outputs a complexity score above threshold.
/// Initiates MCTS search for alternate approaches when task complexity is high.
///
/// # Arguments
/// * `runtime` — HookRuntime context
/// * `complexity_score` — Complexity score from cognitive engine (0.0–1.0)
///
/// # Notes
/// * Only triggers if `complexity_score > MCTS_COMPLEXITY_THRESHOLD` (0.7)
/// * Fire-and-forget: errors traced at debug level
/// * Uses the `cli_mcts_search` handler to execute MCTS planning
pub fn bridge_cognitive_mcts_trigger(
    runtime: &mut HookRuntime,
    complexity_score: f64,
) -> Result<String, DecomposeBridgeError> {
    if complexity_score < MCTS_COMPLEXITY_THRESHOLD {
        return Ok(format!(
            "skipped: complexity {} below threshold {}",
            complexity_score, MCTS_COMPLEXITY_THRESHOLD
        ));
    }

    tracing::info!(
        complexity = %complexity_score,
        "bridge_cognitive_mcts_trigger: high complexity, initiating MCTS search"
    );

    let payload = json!({
        "root_state": format!("complexity_{}", complexity_score),
        "complexity": complexity_score,
    });

    let result = crate::cli_handlers::cli_mcts_search(runtime, &payload);
    tracing::debug!(
        mcts_result = %result,
        "bridge_cognitive_mcts_trigger: MCTS search completed"
    );

    Ok(result)
}

/// Bridge: `pre_compact` hook → checkpoint decompose state.
///
/// Called from `post_compact_handler` before context compaction begins.
/// Persists decompose task state so it survives context trimming.
///
/// # Arguments
/// * `runtime` — HookRuntime context
///
/// # Notes
/// * Fire-and-forget: errors traced at debug level, never block compaction
/// * Checkpoint includes: WAL flush + one snapshot per active task
pub fn bridge_precompact_checkpoint(
    runtime: &mut HookRuntime,
) -> Result<String, DecomposeBridgeError> {
    let db = &runtime.ctx.knowledge;

    // S1.6: Ensure decomposition_snapshots table exists before writing
    crate::cli_handlers_decompose::ensure_decompose_tables(db);

    // WAL checkpoint — fire-and-forget, errors are non-fatal for compaction
    let wal_result = db
        .conn_ref()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
    if let Err(e) = wal_result {
        tracing::debug!("bridge_precompact_checkpoint: WAL checkpoint failed: {}", e);
    }

    // S1.6: Snapshot all active (non-archived) tasks
    let active_tasks: Vec<String> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT task_id FROM task_decompositions WHERE archived_at IS NULL AND status = 'active'",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("bridge_precompact_checkpoint: prepare active tasks failed: {}", e);
                return Ok(json!({"checkpoint": "partial", "component": "decompose"}).to_string());
            }
        };
        stmt.query_map([], |r| r.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let now = chrono::Utc::now().to_rfc3339();
    let mut snapshot_count = 0_usize;

    for task_id in &active_tasks {
        // Fetch subtasks snapshot
        let subtasks_snapshot: Vec<serde_json::Value> = {
            let mut stmt = match db.conn_ref().prepare(
                "SELECT subtask_id, status, priority, deadline, deadline_behavior, \
                 parallel_group, review_required, quality_score, attempts, error \
                 FROM decomposition_subtasks WHERE task_id = ?1",
            ) {
                Ok(s) => s,
                Err(_) => continue,
            };
            stmt.query_map(params![task_id], |r| {
                Ok(serde_json::json!({
                    "subtask_id": r.get::<_, String>(0)?,
                    "status": r.get::<_, String>(1)?,
                    "priority": r.get::<_, i32>(2)?,
                    "deadline": r.get::<_, Option<String>>(3)?,
                    "deadline_behavior": r.get::<_, Option<String>>(4)?,
                    "parallel_group": r.get::<_, Option<String>>(5)?,
                    "review_required": r.get::<_, i32>(6)?,
                    "quality_score": r.get::<_, Option<f64>>(7)?,
                    "attempts": r.get::<_, i32>(8)?,
                    "error": r.get::<_, Option<String>>(9)?
                }))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        };

        // Compute live metrics snapshot
        let total = subtasks_snapshot.len() as i64;
        let completed = subtasks_snapshot
            .iter()
            .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("completed"))
            .count() as i64;
        let failed = subtasks_snapshot
            .iter()
            .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("failed"))
            .count() as i64;
        let pending = total - completed - failed;
        let avg_quality: Option<f64> = {
            let sum: f64 = subtasks_snapshot
                .iter()
                .filter_map(|s| s.get("quality_score").and_then(|v| v.as_f64()))
                .sum();
            let cnt = subtasks_snapshot
                .iter()
                .filter(|s| s.get("quality_score").and_then(|v| v.as_f64()).is_some())
                .count() as f64;
            if cnt > 0.0 { Some(sum / cnt) } else { None }
        };
        let completion_pct = if total > 0 {
            (completed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let metrics_snapshot = serde_json::json!({
            "total_subtasks": total,
            "completed": completed,
            "failed": failed,
            "pending": pending,
            "avg_quality": avg_quality,
            "completion_pct": completion_pct,
            "snapshot_at": now
        });

        let snapshot_id = format!("{}_{}", task_id, chrono::Utc::now().timestamp());

        match db.conn_ref().execute(
            "INSERT OR REPLACE INTO decomposition_snapshots (snapshot_id, task_id, subtasks_snapshot, metrics_snapshot, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                snapshot_id,
                task_id,
                serde_json::to_string(&subtasks_snapshot).unwrap_or_default(),
                serde_json::to_string(&metrics_snapshot).unwrap_or_default(),
                now
            ],
        ) { Err(e) => {
            tracing::debug!("bridge_precompact_checkpoint: snapshot insert failed for {}: {}", task_id, e);
        } _ => {
            snapshot_count += 1;
        }}
    }

    tracing::debug!(
        active_tasks = active_tasks.len(),
        snapshots_written = snapshot_count,
        "bridge_precompact_checkpoint: decompose state checkpointed"
    );

    Ok(json!({
        "checkpoint": "success",
        "component": "decompose",
        "active_tasks": active_tasks.len(),
        "snapshots_written": snapshot_count
    })
    .to_string())
}

/// Bridge: `post_edit` HookQualityAssessment D9 evolution score → decompose subtask quality.
///
/// Called after post_edit quality assessment to record the quality score
/// on the corresponding decompose subtask for quality-aware completion scoring.
///
/// # Arguments
/// * `runtime` — HookRuntime context
/// * `quality_score` — D9 evolution score from HookQualityAssessment (0.0–1.0)
/// * `subtask_id` — Decompose subtask ID that was completed (None = skip)
///
/// # Notes
/// * D9 evolution tracks code quality improvement over time
/// * Fire-and-forget: quality score is advisory, never blocks
pub fn bridge_post_edit_quality(
    runtime: &mut HookRuntime,
    quality_score: f64,
    subtask_id: Option<&str>,
) -> Result<String, String> {
    let Some(subtask_id) = subtask_id else {
        return Ok("skipped: no subtask_id".to_string());
    };

    let payload = json!({
        "task_id": subtask_id.split("::").next().unwrap_or(subtask_id),
        "subtask_id": subtask_id,
        "status": "completed",
        "quality_score": quality_score,
        "quality_dimension": "D9_evolution",
    });

    let result = cli_decompose_update(runtime, &payload);
    if result.contains("\"updated\":false") {
        tracing::debug!(
            subtask_id = %subtask_id,
            quality = %quality_score,
            "bridge_post_edit_quality: subtask not found"
        );
    } else {
        tracing::debug!(
            subtask_id = %subtask_id,
            quality = %quality_score,
            "bridge_post_edit_quality: quality score recorded"
        );
    }
    Ok(result)
}

/// Bridge: `post_tool_failure` hook → decompose subtask failure + MCTS replan trigger.
///
/// Called from `post_tool_failure` when a tool execution fails.
/// Records the failure pattern to enable Markov chain tracking and
/// triggers MCTS replan if failure threshold is exceeded.
///
/// # Arguments
/// * `runtime` — HookRuntime context
/// * `failure_pattern` — Hash of tool_name + error pattern (for pattern detection)
/// * `file_path` — File where failure occurred
/// * `tool_name` — Name of the tool that failed
/// * `error_text` — Error message text
///
/// # Notes
/// * MCTS replan only fires at FAILURE_PATTERN_THRESHOLD=3 (3+ same pattern within 1h)
/// * Fire-and-forget: errors traced at debug level, never block
pub fn bridge_post_tool_failure(
    runtime: &mut HookRuntime,
    failure_pattern: &str,
    file_path: &str,
    tool_name: &str,
    _error_text: &str,
) -> Result<String, String> {
    // Track failure count for this pattern in knowledge DB
    // Uses dedicated failure_counts table (not FILE_ACCESS_LOG)
    let failure_key = format!("__failure_count__{}", failure_pattern);
    let new_count = runtime
        .ctx
        .knowledge
        .increment_failure_count(&failure_key)
        .unwrap_or(1);

    // Determine if we should mark the subtask as failed (3rd failure: count >= 3)
    let should_fail = new_count >= 3;

    // Update decompose subtask if we have a subtask_id
    if should_fail {
        let sid = format!("subtask_{}_{}", tool_name, file_path);
        let fail_payload = json!({
            "task_id": sid.split("::").next().unwrap_or(&sid),
            "subtask_id": sid,
            "status": "failed",
            "priority": 1, // P1 — high priority retry
            "quality_score": 0.0, // Failed = 0 quality
        });
        let _ = cli_decompose_update(runtime, &fail_payload);
    }

    // Trigger MCTS replan + subgoal materialization if failure threshold exceeded.
    // R7 (Pln3): materialize top-3 subgoals into the Pln2 bidirectional channel
    // so CC adopts them in the next session digest as replanning tasks.
    let mcts_triggered = if new_count >= 3 {
        let mat_payload = json!({
            "root_state": format!("failure_replan:{}", failure_pattern),
            "top_n": 3,
        });
        let mat_result = crate::mcts_materializer::materialize_from_payload(runtime, &mat_payload);
        tracing::debug!(
            pattern = %failure_pattern,
            count = new_count,
            materialization = %mat_result,
            "bridge_post_tool_failure: MCTS replan + subgoal materialization triggered"
        );
        true
    } else {
        false
    };

    tracing::debug!(
        pattern = %failure_pattern,
        tool = %tool_name,
        file = %file_path,
        count = new_count,
        mcts_triggered = mcts_triggered,
        marked_failed = should_fail,
        "bridge_post_tool_failure: failure recorded"
    );

    Ok(json!({
        "failure_pattern": failure_pattern,
        "failure_count": new_count,
        "mcts_triggered": mcts_triggered,
        "marked_failed": should_fail,
    })
    .to_string())
}
