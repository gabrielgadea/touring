//! `task-sync-post-stop` hook handler.
//!
//! Mirrors Claude Code's TaskStop to the Touring decompose DAG — cancels all
//! three lifecycle subtasks (::scout/::implement/::validate), injects a
//! negative RL reward, stores a stopped-task lesson, assesses the Touring
//! session for a final quality score, and emits capture/gotcha/drift hints.
//! Extracted from `lifecycle.rs` as part of FIX-3 D1.

use serde_json::Value;

use crate::runtime::HookRuntime;

/// task-sync-stop: PostToolUse(TaskStop) — mirror task stop to Touring decompose.
///
/// Fires after Claude Code's TaskStop tool call (cancels/stops a running task).
/// Emits a decompose update hint to mark the task as cancelled in the DAG, and
/// a negative RL reward hint so the failure is learned from.
pub(crate) fn handle_task_sync_post_stop(rt: &mut HookRuntime, input: &Value) -> String {
    let tool_input = input.get("tool_input").unwrap_or(input);
    let task_id = tool_input
        .get("task_id")
        .or_else(|| tool_input.get("taskId"))
        .or_else(|| input.get("task_id"))
        .or_else(|| input.get("taskId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // P2 (2026-04-13): consume pending stop suggestion if CC passes suggestion_ref.
    let suggestion_ref = tool_input
        .get("suggestion_ref")
        .or_else(|| input.get("suggestion_ref"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    if let Some(sugg_id) = suggestion_ref {
        let _ = crate::cli_handlers::cli_suggestion_mark_consumed(
            rt,
            &serde_json::json!({"suggestion_id": sugg_id, "consumed_action": "stop"}),
        );
    }

    // R13-S1: Call cli_decompose_update(cancelled) + run_task_completed(success=false) directly.
    // TaskStop (Claude Code) → cli_decompose_update → SQLite update + ACO negative pheromone.
    let update_payload = serde_json::json!({"task_id": task_id, "status": "cancelled"});
    let _ = crate::cli_handlers::cli_decompose_update(rt, &update_payload);

    // R159: Auto-advance ::scout + ::implement + ::validate subtasks to cancelled when CC task stops.
    // Symmetric complement to R158 (completed path): all 3 lifecycle subtasks must be terminal
    // for cli_decompose_finalize to archive the DAG and clear the backlog.
    // Without R159: subtasks remain pending/in_progress after cancellation — DAG stays open,
    // RL 1.0 never fires, and the backlog accumulates stale entries that block future finalize calls.
    // Uses priority=5 (same as R156/R158) to trigger the subtask update path in cli_decompose_update.
    {
        let scout_id = format!("{task_id}::scout");
        let impl_id = format!("{task_id}::implement");
        let validate_id = format!("{task_id}::validate");
        for subtask_id in [scout_id, impl_id, validate_id] {
            let _ = crate::cli_handlers::cli_decompose_update(
                rt,
                &serde_json::json!({
                    "task_id": task_id,
                    "subtask_id": subtask_id,
                    "status": "cancelled",
                    "priority": 5,
                }),
            );
        }
    }

    let merged = serde_json::json!({
        "task_id": task_id,
        "success": false,
        "session_id": format!("cc-{}", &task_id[..task_id.len().min(20)]),
        "result_summary": format!("TaskStop: {task_id} stopped/cancelled via Claude Code"),
    });
    if let Err(e) = crate::team_hooks::run_task_completed(rt, &merged) {
        tracing::debug!(error = %e, task_id, "task_sync_stop: run_task_completed failed");
    }

    // R22-S3: Auto-inject RL negative reward + store lesson — closes the stopped-task lifecycle loop.
    // TaskStop → cli_learning_reward(-0.3) + cli_memory_store(lesson) — no manual steps needed.
    let rl_payload = serde_json::json!({
        "tool": "orchestrate",
        "reward": -0.3_f64,
        "context": format!("task:{task_id}:stopped"),
    });
    let _ = crate::cli_handlers::cli_learning_reward(rt, &rl_payload);
    let mem_payload = serde_json::json!({
        "key": format!("task:{task_id}:stopped"),
        "value": format!("Task {task_id} was stopped/cancelled via Claude Code TaskStop — investigate root cause"),
        "tier": "semantic",
        "entry_type": "lesson",
    });
    let _ = crate::cli_handlers::cli_memory_store(rt, &mem_payload);
    tracing::debug!(task_id, "task_sync_stop: RL reward + lesson stored");

    // R37-S3: Assess the Touring session created for this task by R36-S1 (TaskCreated).
    // TaskStop → cli_session_assess(task_id) → quality score inline — closes the session lifecycle.
    let assess_hint = session_assess_on_task_stop(rt, task_id);

    // R127: Capture interrupted work as generator artifacts so no progress is lost.
    // DiaryEntry: records AAAK-format lesson from the cancelled task (partial progress + why it stopped).
    // IncrementalPatch: captures partial implementation as a reusable patch for future resumption.
    let truncated_id = &task_id[..task_id.len().min(40)];
    let capture_hint = format!(
        " | capture-partial: run `touring generate render DiaryEntry \
        --vars '{{\"agent\":\"claude_code\",\"task_id\":\"{truncated_id}\",\"phase\":\"stopped\"}}'` \
        to record partial progress \
        | run `touring generate render IncrementalPatch \
        --vars '{{\"file_path\":\"<partial_file>\",\"patch_lines\":[]}}'` \
        to save partial implementation"
    );
    // R140: Auto-register a gotcha when a task is stopped — prevents future tasks from repeating
    // the same cancellation pattern. Derives a pattern from task_id and adds to the gotcha DB.
    // TaskStop → gotcha-add → future TaskCreate gotcha-check surfaces this pitfall (R139 loop).
    // Severity "medium" — stopped tasks may be environmental, not always a code smell.
    let gotcha_add_hint = format!(
        " | gotcha-auto: run `touring gotcha add \"task-stopped:{truncated_id}\" \
        \"Task {truncated_id} was stopped — investigate root cause before restarting\" \
        --severity medium` to register pitfall pattern"
    );
    // R152: Surface evolution drift analysis when a task is stopped — closes TaskStop → drift loop.
    // TaskStop = cancellation signal. Repeated cancellations indicate systemic degradation:
    // declining success rates, growing edit retry counts, or structural drift in quality metrics.
    // `touring evolution drift -j` detects and self-corrects these patterns before they cascade.
    // Static hint (not gated on current alert_level) — any cancellation warrants drift check.
    let drift_hint = " | evolution-drift: run `touring evolution drift -j` to detect \
        systemic degradation patterns causing task cancellations";

    format!(
        "touring-sync: decompose {task_id} cancelled (applied) | RL -0.3 injected | lesson stored{assess_hint}{capture_hint}{gotcha_add_hint}{drift_hint} — run `touring memory recall \"task:{task_id}\"` to review"
    )
}

/// R37-S3: Assess the Touring session created for this task by R36-S1 (TaskCreated).
///
/// R36-S1 creates `cli_session_start(session_id=task_id)` for every TaskCreated event.
/// When the task is stopped via TaskStop, this helper assesses that session to surface
/// a final quality score inline. Returns empty string when the session has no quality_score
/// (e.g. session never started, or assess returns no metrics — non-blocking either way).
///
/// `pub(crate)` so inline tests in `lifecycle::tests` can exercise this helper
/// directly via `super::session_assess_on_task_stop` after the re-export in
/// `lifecycle.rs`.
pub(crate) fn session_assess_on_task_stop(rt: &mut HookRuntime, task_id: &str) -> String {
    let assess_payload = serde_json::json!({"session_id": task_id});
    let result = crate::cli_handlers::cli_session_assess(rt, &assess_payload);
    match serde_json::from_str::<serde_json::Value>(&result) {
        Ok(v) => {
            if let Some(score) = v.get("quality_score").and_then(|s| s.as_f64()) {
                format!(" | session-assessed: {task_id} quality={score:.2}")
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}
