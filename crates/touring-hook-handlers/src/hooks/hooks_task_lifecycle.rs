//! Hooks for task lifecycle events — task_metrics, task_completed, etc.
//!
//! This module contains hooks that record task execution metrics to the
//! knowledge DB for analytics and performance tracking.
//!
//! ## Handlers
//! - `run_task_metrics`: records task completion metrics to knowledge DB
//! - `run_task_escalation`: triggers when a task fails or blocks, enabling automatic escalation

use crate::knowledge::TaskMetrics;
use crate::runtime::HookResponse;
use crate::runtime::HookRuntime;
use serde_json::Value;

/// run_task_metrics: records task completion metrics to knowledge DB.
///
/// Input: { task_id, completion_time, subtask_count, success_rate, quality_score }
/// Output: HookResponse::Context with recorded metrics summary
///
/// # Errors
/// Silently returns HookResponse::Allow if knowledge DB write fails.
/// This is fire-and-forget for analytics — never blocks task execution.
#[tracing::instrument(skip(runtime, input), fields(hook = "run_task_metrics"))]
pub fn run_task_metrics(runtime: &HookRuntime, input: &Value) -> HookResponse {
    // Parse input fields with defaults for backward compatibility
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let completion_time = input
        .get("completion_time")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let subtask_count = input
        .get("subtask_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let success_rate = input
        .get("success_rate")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let quality_score = input
        .get("quality_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // Build metrics struct
    let metrics = TaskMetrics {
        task_id: task_id.to_string(),
        completion_time_secs: completion_time,
        subtask_count,
        success_rate,
        quality_score,
    };

    // Record to knowledge DB — fire-and-forget
    if let Err(e) = runtime.ctx.knowledge.record_task_metrics(&metrics) {
        tracing::debug!(error = %e, task_id = task_id, "run_task_metrics: failed to record");
        return HookResponse::Allow;
    }

    // Build context summary
    let summary = format!(
        "Task metrics recorded: {} (time={:.2}s, subtasks={}, success={:.1}%, quality={:.2})",
        task_id,
        completion_time,
        subtask_count,
        success_rate * 100.0,
        quality_score
    );

    tracing::debug!(
        task_id = task_id,
        completion_time = completion_time,
        subtask_count = subtask_count,
        success_rate = success_rate,
        quality_score = quality_score,
        "Task metrics recorded"
    );

    HookResponse::Context {
        context: summary,
        event_name: Some("run_task_metrics".to_string()),
    }
}

/// run_task_validation: validates a task DAG before creation/execution.
///
/// This PreToolUse hook validates task dependency cycles and structural
/// integrity before the task is created or executed. Returns validation
/// result as context for the orchestrator.
///
/// ## Input
/// ```json
/// {
///   "task_id": "task-42",
///   "subtask_count": 5
/// }
/// ```
///
/// ## Output
/// - `HookResponse::Context` with validation result (valid=true/false)
/// - Always returns Allow (exit 0 invariant) — validation failures do not block
#[tracing::instrument(skip(runtime, input), fields(hook = "run_task_validation"))]
pub fn run_task_validation(runtime: &mut HookRuntime, input: &Value) -> HookResponse {
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    tracing::debug!(task_id = %task_id, "Task validation requested");

    // Call decompose validate to check for DAG cycles
    let validation_result = crate::cli_handlers::cli_decompose_validate(runtime, input);

    // Parse the validation result
    let is_valid = validation_result.contains("\"valid\":true")
        || validation_result.contains("\"valid\" : true");

    let context = if is_valid {
        format!("Task {} validation passed: DAG is cycle-free", task_id)
    } else {
        format!("Task {} validation: {}", task_id, validation_result)
    };

    tracing::info!(
        task_id = %task_id,
        is_valid = is_valid,
        "Task validation completed"
    );

    HookResponse::context_with_event(context, "task-validation")
}

/// run_task_escalation: triggers when a task fails or blocks.
///
/// This is a PostToolUse hook that records task failure/block events,
/// logs them for team analytics, and prepares escalation context for
/// notification to the human orchestrator.
///
/// ## Input
/// ```json
/// {
///   "session_id": "...",
///   "task_id": "task-42",
///   "failure_reason": "unresolvable dependency conflict",
///   "blocked_since": 3600,
///   "teammate_name": "engineer-1",
///   "team_name": "taco-abc123"
/// }
/// ```
///
/// ## Output
/// - `HookResponse::Context` with escalation details always (exit 0 invariant)
#[tracing::instrument(skip(runtime, input), fields(hook = "task_escalation"))]
pub fn run_task_escalation(runtime: &mut HookRuntime, input: &Value) -> HookResponse {
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let failure_reason = input
        .get("failure_reason")
        .or_else(|| input.get("reason"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let blocked_since_secs = input.get("blocked_since").and_then(|v| v.as_u64());

    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let teammate_name = input
        .get("teammate_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let team_name = input
        .get("team_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Record escalation event in knowledge DB
    let escalation_key = format!("__task_escalation__{}", task_id);
    let _ = runtime
        .ctx
        .knowledge
        .record_access(&escalation_key, session_id);

    // Record as bash outcome for task history and ACO signal routing
    let reason_short = if failure_reason.len() > 100 {
        format!("{}...", &failure_reason[..100])
    } else {
        failure_reason.to_string()
    };
    let cmd = format!("task_escalation:{}:{}", task_id, reason_short);
    let _ = runtime
        .ctx
        .knowledge
        .record_bash_outcome(&crate::knowledge::BashOutcome {
            command: cmd,
            command_short: "task_escalation".to_string(),
            exit_code: 1,
            success: false,
            error_pattern: Some(failure_reason.to_string()),
            file_context: None,
            command_hash: String::new(),
            executed_at: String::new(),
        });

    tracing::warn!(
        task_id = %task_id,
        reason = %failure_reason,
        blocked_since_secs = ?blocked_since_secs,
        teammate = %teammate_name,
        team = %team_name,
        "Task escalation recorded — notifying orchestrator"
    );

    // Build context message for the orchestrator
    let blocked_msg = blocked_since_secs.map(|secs| {
        let mins = secs / 60;
        let hours = mins / 60;
        if hours > 0 {
            format!("Task blocked for {}h {}m", hours, mins % 60)
        } else if mins > 0 {
            format!("Task blocked for {}m", mins)
        } else {
            format!("Task blocked for {}s", secs)
        }
    });

    let context = format!(
        "ESCALATION: Task {} failed. Reason: {}. {}. Teammate: {} (team: {})",
        task_id,
        failure_reason,
        blocked_msg.unwrap_or_else(|| "Duration unknown".to_string()),
        teammate_name,
        team_name
    );

    tracing::info!(
        task_id = %task_id,
        context_len = context.len(),
        "Task escalation context prepared"
    );

    HookResponse::context_with_event(context, "task-escalation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().unwrap();
        let rt = HookRuntime::new(tmp.path()).unwrap();
        (tmp, rt)
    }

    #[test]
    fn test_run_task_metrics_records_to_knowledge_db() {
        let (_tmp, rt) = setup();
        let input = serde_json::json!({
            "task_id": "task-001",
            "completion_time": 12.5,
            "subtask_count": 5,
            "success_rate": 0.95,
            "quality_score": 0.88
        });

        let response = run_task_metrics(&rt, &input);

        // Should return Context with summary
        match response {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert!(context.contains("task-001"));
                assert!(context.contains("12.5"));
                assert_eq!(event_name, Some("run_task_metrics".to_string()));
            }
            other => panic!("Expected HookResponse::Context, got {:?}", other),
        }

        // Verify it was recorded in DB
        let stats = rt.ctx.knowledge.stats().unwrap();
        assert!(stats.task_metrics_count > 0);
    }

    #[test]
    fn test_run_task_metrics_missing_fields_use_defaults() {
        let (_tmp, rt) = setup();
        // Minimal input — all fields optional
        let input = serde_json::json!({
            "task_id": "task-002"
        });

        let response = run_task_metrics(&rt, &input);

        match response {
            HookResponse::Context { context, .. } => {
                assert!(context.contains("task-002"));
                assert!(context.contains("0")); // defaults for missing numeric fields
            }
            other => panic!("Expected HookResponse::Context, got {:?}", other),
        }
    }

    #[test]
    fn test_run_task_metrics_empty_input_graceful() {
        let (_tmp, rt) = setup();
        let input = serde_json::json!({});

        let response = run_task_metrics(&rt, &input);

        // Should still return Allow (or Context with defaults) — never panic
        match response {
            HookResponse::Context { context, .. } => {
                assert!(context.contains("unknown")); // default task_id
            }
            HookResponse::Allow => {}
            other => panic!("Unexpected response: {:?}", other),
        }
    }
}
