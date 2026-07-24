//! `task-sync-post-delete` hook handler.
//!
//! Mirrors Claude Code's TaskDelete to the Touring decompose DAG — marks the
//! task cancelled, injects an RL negative reward, stores a deletion lesson,
//! and emits a DiaryEntry hint so the postmortem rationale is captured.
//! Extracted from `lifecycle.rs` as part of FIX-3 D1.

use serde_json::Value;

use crate::runtime::HookRuntime;

/// task-sync-delete: PostToolUse(TaskDelete) — mirror task deletion to Touring decompose.
///
/// Fires after Claude Code's TaskDelete tool call. Emits a decompose update hint
/// to mark the task as cancelled and a memory-cleanup note for the knowledge graph.
pub(crate) fn handle_task_sync_post_delete(rt: &mut HookRuntime, input: &Value) -> String {
    let tool_input = input.get("tool_input").unwrap_or(input);
    let task_id = tool_input
        .get("task_id")
        .or_else(|| tool_input.get("taskId"))
        .or_else(|| input.get("task_id"))
        .or_else(|| input.get("taskId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // R13-S2: Call cli_decompose_update(cancelled) directly — no manual step needed.
    // TaskDelete (Claude Code) → cli_decompose_update(cancelled) → SQLite update.
    let update_payload = serde_json::json!({"task_id": task_id, "status": "cancelled"});
    let _ = crate::cli_handlers::cli_decompose_update(rt, &update_payload);

    // R25-S1: Auto-inject RL negative reward + store deletion lesson.
    // Mirrors R22-S3 (TaskStop) — closes the feedback loop for task deletions.
    // Deletions are negative signals: the task was created but discarded, losing work.
    let _ = crate::cli_handlers::cli_learning_reward(
        rt,
        &serde_json::json!({
            "tool": "orchestrate",
            "reward": -0.3_f64,
            "context": format!("task:{task_id}:deleted"),
        }),
    );
    let _ = crate::cli_handlers::cli_memory_store(
        rt,
        &serde_json::json!({
            "key": format!("task:{task_id}:deleted"),
            "value": format!("Task {task_id} deleted via Claude Code — investigate if intentional or premature"),
            "tier": "semantic",
            "entry_type": "lesson",
        }),
    );

    // R127: Suggest DiaryEntry on task deletion — captures the postmortem rationale.
    // A deleted task should produce an institutional memory entry explaining why it was removed,
    // so future planning avoids repeating the same dead-end approach.
    let truncated_id = &task_id[..task_id.len().min(40)];
    let diary_hint = format!(
        " | postmortem: run `touring generate render DiaryEntry \
        --vars '{{\"agent\":\"claude_code\",\"task_id\":\"{truncated_id}\",\"phase\":\"deleted\"}}'` \
        to record deletion rationale"
    );

    format!(
        "touring-sync: task {task_id} deleted — decompose cancelled (applied) | RL -0.3 injected | lesson stored{diary_hint} | \
        run `touring memory recall \"task:{task_id}\"` to review deletion history"
    )
}
