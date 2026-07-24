//! Team Hooks — N1: Hooks as Gateway for Agent Teams ↔ ACO Integration.
//!
//! ## Purpose
//! These hooks bridge Agent Teams lifecycle events (teammate-idle, task-completed)
//! to the ACO wiring layer, enabling the Touring daemon to learn from team dynamics.
//!
//! ## Design Principles
//! 1. **Fire-and-forget**: All ACO wiring calls are fallible — never block Claude Code
//! 2. **Exit 0 invariant**: Every function preserves the hook exit guarantee
//! 3. **HookQualityAssessment**: Reuses existing 9D tracker for team quality scoring
//!
//! ## Events Wired
//! - `teammate-idle` → `deposit_teammate_idle()` → pheromone heat for teammate
//! - `task-completed` → `deposit_task_completion()` → pheromone heat for task
//! - `teammate-idle-gate` → anti-limbo gate with context injection + exit code
//! - `subagent-bootstrap` → minimal bootstrap context for SubagentStart

use crate::hook_decompose_bridge::bridge_idle_gate_queue_state;
use crate::runtime::HookRuntime;
use crate::schemas::validate_payload;
use serde_yaml;

/// Result from gate hooks that control teammate behavior.
/// `context`: additionalContext to inject (empty = none).
/// `exit_code`: 0 = allow idle/stop, 2 = block idle/stop.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TeamHookGateResult {
    /// `additionalContext` to inject into the gated event (empty = none).
    pub context: String,
    /// Hook exit code: 0 allows the idle/stop, 2 blocks it.
    pub exit_code: u8,
}

/// Result from subagent-stop gate hook.
/// `feedback`: message to show agent if blocked (empty = allow stop).
/// `exit_code`: 0 = allow stop, 2 = block stop.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SubagentStopResult {
    /// Message shown to the agent when the stop is blocked (empty = allow stop).
    pub feedback: String,
    /// Hook exit code: 0 allows the stop, 2 blocks it.
    pub exit_code: u8,
}

/// Max recovery attempts before circuit breaker allows idle.
const MAX_IDLE_RECOVERY: u32 = 5;

/// Anti-limbo gate for `TeammateIdle` events.
///
/// Reads idle count from knowledge DB, detects transcript state,
/// returns minimal context injection + exit code 2 to block idle.
/// Circuit breaker: after MAX_IDLE_RECOVERY attempts, returns exit 0.
///
/// This replaces the Python `teammate_anti_limbo.py` logic with a
/// < 1ms Rust path through the daemon.
#[tracing::instrument(skip(runtime, input), fields(hook = "teammate_idle_gate"))]
pub fn run_teammate_idle_gate(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> TeamHookGateResult {
    let teammate_name = input
        .get("teammate_name")
        .or_else(|| input.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let team_name = input
        .get("team_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let tasks_completed = input
        .get("tasks_completed")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    // Track idle count in knowledge DB using a stable key per teammate
    let idle_key = format!("__idle_count__{team_name}__{teammate_name}__");
    let idle_count = runtime.ctx.knowledge.access_count(&idle_key).unwrap_or(0) as u32;
    let _ = runtime.ctx.knowledge.record_access(
        &idle_key,
        input
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
    );

    // Circuit breaker: after MAX_IDLE_RECOVERY, allow idle
    if idle_count >= MAX_IDLE_RECOVERY {
        tracing::warn!(
            teammate = teammate_name,
            count = idle_count,
            "Circuit breaker — allowing idle after {MAX_IDLE_RECOVERY} attempts"
        );
        return TeamHookGateResult {
            context: String::new(),
            exit_code: 0,
        };
    }

    // If tasks were completed, teammate is productive — allow idle
    if tasks_completed > 0 {
        // Still wire to ACO for learning
        let _ = run_teammate_idle(runtime, input);
        return TeamHookGateResult {
            context: String::new(),
            exit_code: 0,
        };
    }

    // Wire limbo to ACO for learning
    let _ = run_teammate_idle(runtime, input);

    // HDG-4: Check decompose queue for pending subtasks for this teammate
    // If pending subtasks exist, inject them into the idle-gate context
    let decompose_context = match bridge_idle_gate_queue_state(runtime, teammate_name) {
        Ok(json_str) => {
            if let Ok(state) = serde_json::from_str::<serde_json::Value>(&json_str) {
                let has_pending = state
                    .get("subtasks")
                    .and_then(|s| s.as_array())
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false);
                if has_pending {
                    let subtasks = state
                        .get("subtasks")
                        .and_then(|s| s.as_array())
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    let pending_count = subtasks.len();
                    let pending_ids: Vec<&str> = subtasks
                        .iter()
                        .filter_map(|s| s.get("subtask_id").and_then(|v| v.as_str()))
                        .collect();
                    let context_hint = format!(
                        "Decompose queue: {} pending task(s) — call TaskList() to see: {}",
                        pending_count,
                        pending_ids.join(", ")
                    );
                    tracing::debug!(
                        teammate = teammate_name,
                        pending = pending_count,
                        "HDG-4: decompose queue state injected into idle gate"
                    );
                    context_hint
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        }
        Err(e) => {
            tracing::debug!(
                teammate = teammate_name,
                error = %e,
                "HDG-4: bridge_idle_gate_queue_state failed"
            );
            String::new()
        }
    };

    // Detect transcript state if available
    let transcript = input
        .get("transcript_summary")
        .or_else(|| input.get("transcript_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let context = build_minimal_context(teammate_name, transcript);

    // HDG-4: Merge decompose queue context with transcript-based context
    // If pending subtasks exist, prepend decompose info so model sees it first
    let final_context = if !decompose_context.is_empty() {
        if context.is_empty() {
            decompose_context
        } else {
            format!("{}\n{}", decompose_context, context)
        }
    } else {
        context
    };

    tracing::info!(
        teammate = teammate_name,
        idle_count = idle_count + 1,
        context_len = final_context.len(),
        "Anti-limbo gate — blocking idle with context injection"
    );

    TeamHookGateResult {
        context: final_context,
        exit_code: 2,
    }
}

/// Build minimal context based on transcript keywords.
/// Shorter context = higher probability the model follows it.
fn build_minimal_context(teammate_name: &str, transcript: &str) -> String {
    let lower = transcript.to_lowercase();

    if !lower.contains("toolsearch") && !lower.contains("tool_search") {
        return "Call ToolSearch(query=\"select:TaskUpdate,TaskList,TaskGet,SendMessage\") \
             NOW. Then call TaskList(). Do NOT output text first."
            .to_string();
    }

    if !lower.contains("tasklist") && !lower.contains("task_list") {
        return "Call TaskList() NOW to see your assigned tasks.".to_string();
    }

    if !lower.contains("taskupdate") && !lower.contains("task_update") {
        let mut msg = String::from(
            "Call TaskList(), find your task, then TaskUpdate(taskId=\"ID\", owner=\"",
        );
        msg.push_str(teammate_name);
        msg.push_str("\", status=\"in_progress\") and execute it.");
        return msg;
    }

    if !lower.contains("sendmessage") && !lower.contains("send_message") {
        return "SendMessage(to=\"lead\", summary=\"Done\", message=\"<result>\") \
                then TaskList() for more work."
            .to_string();
    }

    "TaskList() — check for remaining tasks.".to_string()
}

/// Minimal bootstrap context for SubagentStart events.
///
/// Returns a 3-line bootstrap that the Python shim injects as additionalContext.
/// Replaces the 60-line Python BOOTSTRAP_CONTEXT constant.
pub fn subagent_bootstrap_context() -> String {
    "Your first tool call MUST be: \
     ToolSearch(query=\"select:TaskUpdate,TaskList,TaskGet,SendMessage\")\n\
     Then call: TaskList()\n\
     Do NOT plan, think, or output text before calling these two tools."
        .to_string()
}

// Tools that indicate an Agent Teams session (not a pure TACO subagent).
const TEAM_INFRA_TOOLS: &[&str] = &[
    "TeamCreate",
    "TaskCreate",
    "TaskUpdate",
    "TaskList",
    "SendMessage",
];

/// Extract all tool_use content blocks from the message transcript.
fn extract_tool_calls(transcript: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut calls = Vec::new();
    for msg in transcript {
        if !msg.is_object() {
            continue;
        }
        let content = match msg.get("content") {
            Some(serde_json::Value::Array(arr)) => arr,
            _ => continue,
        };
        for block in content {
            if block.is_object()
                && block
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(|t| t == "tool_use")
                    .unwrap_or(false)
            {
                calls.push(block.clone());
            }
        }
    }
    calls
}

/// Return true if the session invoked Agent Teams infrastructure tools.
fn session_used_team_infra(tool_calls: &[serde_json::Value]) -> bool {
    tool_calls.iter().any(|call| {
        call.get("name")
            .and_then(|v| v.as_str())
            .map(|name| TEAM_INFRA_TOOLS.contains(&name))
            .unwrap_or(false)
    })
}

/// Return true if a TaskUpdate with status='completed' was called.
fn has_completed_task_update(tool_calls: &[serde_json::Value]) -> bool {
    tool_calls.iter().any(|call| {
        if call.get("name").and_then(|v| v.as_str()) != Some("TaskUpdate") {
            return false;
        }
        call.get("input")
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str())
            .map(|status| status == "completed")
            .unwrap_or(false)
    })
}

/// Extract the final assistant message text from the transcript.
/// The subagent's structured JSON output is in the last assistant message's text content.
fn extract_final_output(transcript: &[serde_json::Value]) -> Option<String> {
    // Walk transcript in reverse to find the last assistant message with text content
    for msg in transcript.iter().rev() {
        let role = msg.get("role")?.as_str()?;
        if role != "assistant" {
            continue;
        }
        let content = msg.get("content")?.as_array()?;
        for block in content.iter().rev() {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                let trimmed = text.trim();
                if trimmed.starts_with('{') || trimmed.starts_with("SKIP") {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

const PARCER_AGENTS_DIR: &str = "/home/gabrielgadea/.claude/agents";

/// Validate subagent output against its PARCER profile's `response.format.schema_ref`.
/// Returns `None` if valid, or an error message describing the validation failure.
/// Gracefully degrades: if the profile cannot be read, returns `None` (allow stop).
fn validate_parcer_output(agent_id: &str, output: &str) -> Option<String> {
    // Build the PARCER YAML path
    let yaml_path = format!("{}/touring-{}.parcer.yaml", PARCER_AGENTS_DIR, agent_id);
    let yaml_text = match std::fs::read_to_string(&yaml_path) {
        Ok(t) => t,
        Err(_) => return None, // Graceful degradation: no profile → allow stop
    };
    let profile: serde_yaml::Value = match serde_yaml::from_str::<serde_yaml::Value>(&yaml_text) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(agent_id, error = %e, "Malformed PARCER YAML — allowing stop");
            return None;
        }
    };

    // Extract `response.format.schema_ref` from the profile
    let schema_ref = profile
        .get("response")?
        .get("format")?
        .get("schema_ref")?
        .as_str()?;

    // Parse the output as JSON
    let output_val: serde_json::Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(e) => {
            return Some(format!(
                "PARCER validation FAILED for `{agent_id}` — output is not valid JSON: {}",
                e
            ));
        }
    };

    // Validate based on schema_ref type
    let problem = match schema_ref {
        "scouter-output.schema.json" => validate_scouter_output(&output_val),
        "architect-output.schema.json" => validate_architect_output(&output_val),
        "engineer-output.schema.json" => validate_engineer_output(&output_val),
        "auditor-output.schema.json" => validate_auditor_output(&output_val),
        "scriber-output.schema.json" => validate_scriber_output(&output_val),
        _ => {
            // Unknown schema — structural check only (required top-level fields)
            validate_generic_output(&output_val)
        }
    };

    problem.map(|p| format!("PARCER validation FAILED for `{agent_id}` — {}", p))
}

/// Generic structural validation: require `status` and `role` fields.
fn validate_generic_output(output: &serde_json::Value) -> Option<String> {
    if output.get("status").is_none() {
        return Some("missing required field: `status`".to_string());
    }
    None
}

/// Validate scouter output against PARCER contract.
fn validate_scouter_output(output: &serde_json::Value) -> Option<String> {
    if output.get("status").is_none() {
        return Some("missing required field: `status`".to_string());
    }
    if output.get("findings").is_none() {
        return Some("missing required field: `findings`".to_string());
    }
    None
}

/// Validate architect output against PARCER contract.
fn validate_architect_output(output: &serde_json::Value) -> Option<String> {
    if output.get("status").is_none() {
        return Some("missing required field: `status`".to_string());
    }
    if output.get("context_snapshot").is_none() {
        return Some("missing required field: `context_snapshot`".to_string());
    }
    let confidence = output
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if confidence < 0.5 {
        return Some(format!("confidence {} is below minimum 0.5", confidence));
    }
    None
}

/// Validate engineer output against PARCER contract.
fn validate_engineer_output(output: &serde_json::Value) -> Option<String> {
    if output.get("status").is_none() {
        return Some("missing required field: `status`".to_string());
    }
    let composite = output
        .get("composite_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if composite < 1.0 {
        return Some(format!(
            "composite_score {} is below required 1.0 — PARCER gate: output REJECTED",
            composite
        ));
    }
    None
}

/// Validate auditor output against PARCER contract.
fn validate_auditor_output(output: &serde_json::Value) -> Option<String> {
    if output.get("status").is_none() {
        return Some("missing required field: `status`".to_string());
    }
    let confidence = output
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    if confidence < 0.8 {
        return Some(format!(
            "confidence {} is below minimum 0.8 — PARCER gate: output REJECTED",
            confidence
        ));
    }
    if output.get("e2e_proof").is_none() {
        return Some("missing required field: `e2e_proof`".to_string());
    }
    None
}

/// Validate scriber output against PARCER contract.
fn validate_scriber_output(output: &serde_json::Value) -> Option<String> {
    if output.get("status").is_none() {
        return Some("missing required field: `status`".to_string());
    }
    if output.get("documentation_created").is_none() {
        return Some("missing required field: `documentation_created`".to_string());
    }
    None
}

/// N1: SubagentStop gate — prevents incomplete Agent Teams teammates from stopping.
///
/// Detection strategy (transcript-based, NOT text matching):
/// 1. Parse actual tool_use blocks from the transcript
/// 2. If session used team infrastructure (TeamCreate/TaskCreate present):
///    - Require at least one TaskUpdate with status="completed" before allowing stop
/// 3. If no team infrastructure detected: allow stop unconditionally
///    (pure subagent paradigm — orchestrator manages results via return values)
///
/// v3 flaw (Python): text-matched "taskupdate"/"sendmessage" in last_assistant_message.
/// This caused false positives: explaining why NOT to call those tools triggered release.
/// v4 fix: inspect structured tool_use objects in transcript, not free text.
///
/// Input JSON shape:
/// ```json
/// {
///   "session_id": "...",
///   "hook_event_name": "SubagentStop",
///   "transcript": [{"role": "...", "content": [{"type": "tool_use", "name": "...", "input": {...}}]}],
///   "stop_hook_active": false
/// }
/// ```
///
/// Exit codes:
/// - 0 = allow stop
/// - 2 = block stop; stderr feedback to agent
#[tracing::instrument(skip(runtime, input), fields(hook = "subagent_stop_gate"))]
pub fn run_subagent_stop_gate(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> SubagentStopResult {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Record in knowledge DB (fire-and-forget)
    let _ = runtime
        .ctx
        .knowledge
        .record_access("__subagent_stop__", session_id);

    // Circuit breaker: already blocked once this stop cycle → allow stop
    if input
        .get("stop_hook_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return SubagentStopResult {
            feedback: String::new(),
            exit_code: 0,
        };
    }

    // Inspect actual tool calls — fail open if transcript unavailable
    let transcript = match input.get("transcript") {
        Some(serde_json::Value::Array(arr)) => arr.as_slice(),
        _ => &[],
    };

    // D3.4 PARCER format validation — validate subagent output against PARCER profile
    let agent_id = input
        .get("agent_id")
        .or_else(|| input.get("teammate_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("engineer");
    if let Some(output) = extract_final_output(transcript) {
        if let Some(problem) = validate_parcer_output(agent_id, &output) {
            tracing::info!(
                session_id,
                agent_id,
                problem,
                "PARCER validation failed — blocking subagent stop"
            );
            return SubagentStopResult {
                feedback: problem,
                exit_code: 2,
            };
        }
    }

    let tool_calls = extract_tool_calls(transcript);

    // Pure subagents and TACO orchestrators never use team infrastructure.
    // → Always allow stop unconditionally (TACO v5.0 pure subagent paradigm)
    if !session_used_team_infra(&tool_calls) {
        tracing::debug!(session_id, "Pure subagent — allowing stop");
        return SubagentStopResult {
            feedback: String::new(),
            exit_code: 0,
        };
    }

    // This is an Agent Teams teammate: require a completed TaskUpdate
    if has_completed_task_update(&tool_calls) {
        tracing::debug!(
            session_id,
            "Agent Teams teammate with completed tasks — allowing stop"
        );
        return SubagentStopResult {
            feedback: String::new(),
            exit_code: 0,
        };
    }

    // Teammate used team infra but never marked a task completed → block
    tracing::info!(
        session_id,
        "Agent Teams teammate — blocking stop (no TaskUpdate completed)"
    );
    SubagentStopResult {
        feedback: "Before stopping: TaskUpdate(taskId=\"ID\", status=\"completed\") \
                   then SendMessage(to=\"lead\", summary=\"Done\", message=\"result\")"
            .to_string(),
        exit_code: 2,
    }
}

/// N1: Handle `task-created` hook — records task creation for ACO learning.
///
/// Input JSON shape (Claude Code official format):
/// ```json
/// {
///   "session_id": "...",
///   "hook_event_name": "TaskCreated",
///   "task_id": "task-001",
///   "task_subject": "Engineer: implement cache core",
///   "task_description": "...",
///   "teammate_name": "engineer-1",
///   "team_name": "taco-abc123"
/// }
/// ```
///
/// Always returns Ok(()) — never blocks task creation (exit 0 invariant).
/// Records in knowledge DB for team analytics and ACO signal routing.
#[tracing::instrument(skip(runtime, input), fields(hook = "task_created"))]
pub fn run_task_created(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    // D9: Validate payload — skip silently on failure (non-blocking, fire-and-forget).
    let validated = match validate_payload::<crate::schemas::TaskCreatedPayload>(input) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };

    // Use validated fields; fall back to raw input for fields not in schema.
    let task_id = validated.task_id.as_str();
    let task_subject = validated.description.as_deref().unwrap_or_else(|| {
        input
            .get("task_subject")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    });

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

    // Record task creation in knowledge DB
    let _ = runtime
        .ctx
        .knowledge
        .record_access("__task_created__", session_id);

    // Record as bash outcome for task history and ACO signal routing
    let cmd = format!(
        "task_created:{}:{}",
        task_id,
        &task_subject[..task_subject.len().min(100)]
    );
    let _ = runtime
        .ctx
        .knowledge
        .record_bash_outcome(&crate::knowledge::BashOutcome {
            command: cmd,
            command_short: "task_creation".to_string(),
            exit_code: 0,
            success: true,
            error_pattern: None,
            file_context: None,
            command_hash: String::new(),
            executed_at: String::new(),
        });

    tracing::debug!(
        task_id = task_id,
        subject = task_subject,
        teammate = teammate_name,
        team = team_name,
        "Task created — recorded in knowledge DB"
    );

    // Bridge to decompose task system (fire-and-forget)
    // HDG-2: team_hooks run_task_created → bridge_task_created → cli_decompose_create
    if let Err(e) = crate::hook_decompose_bridge::bridge_task_created(
        runtime,
        task_id,
        task_subject,
        session_id,
        Some(teammate_name),
        Some(team_name),
    ) {
        tracing::debug!(error = %e, "bridge_task_created failed");
    }

    Ok(())
}

/// N1: Handle `teammate-idle` hook — deposits teammate productivity pheromone
/// and detects limbo patterns for ACO learning.
///
/// Input JSON shape:
/// ```json
/// {
///   "session_id": "...",
///   "teammate_name": "engineer-1",
///   "tasks_completed": 3,
///   "blocked_tasks_count": 0,
///   "has_uncompleted_tasks": false,
///   "message": "went idle after completing 3 tasks"
/// }
/// ```
///
/// Limbo detection: if `tasks_completed == 0` AND (`has_uncompleted_tasks == true`
/// OR `blocked_tasks_count > 0`), the teammate entered limbo. ACO receives
/// negative pheromone to learn which DAG/teammate configurations cause this.
///
/// Fire-and-forget: ACO wiring failures are silently swallowed, exit 0 always.
#[tracing::instrument(skip(runtime, input), fields(hook = "teammate_idle"))]
pub fn run_teammate_idle(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    let teammate_name = input
        .get("teammate_name")
        .or_else(|| input.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let tasks_completed = input
        .get("tasks_completed")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let blocked_tasks_count = input
        .get("blocked_tasks_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let has_uncompleted_tasks = input
        .get("has_uncompleted_tasks")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Record lifecycle event in knowledge DB (existing behavior)
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let _ = runtime
        .ctx
        .knowledge
        .record_access("__teammate_idle__", session_id);

    // N1: Wire to ACO — deposit teammate heat
    if let Ok(wiring) = runtime.aco_wiring.lock() {
        wiring.deposit_teammate_idle(teammate_name, tasks_completed);

        // Limbo detection: idle with no completed work but pending/blocked tasks
        let is_limbo = tasks_completed == 0 && (has_uncompleted_tasks || blocked_tasks_count > 0);

        if is_limbo {
            // Infer uncompleted count: if has_uncompleted_tasks is true but no
            // explicit count, assume at least 1 uncompleted task.
            let uncompleted_count = if has_uncompleted_tasks && blocked_tasks_count == 0 {
                1
            } else {
                0
            };

            wiring.deposit_teammate_limbo(teammate_name, blocked_tasks_count, uncompleted_count);

            tracing::warn!(
                teammate = teammate_name,
                blocked = blocked_tasks_count,
                uncompleted = has_uncompleted_tasks,
                "Limbo pattern detected — ACO pheromone deposited"
            );
        }
    }

    tracing::debug!(
        teammate = teammate_name,
        tasks = tasks_completed,
        blocked = blocked_tasks_count,
        uncompleted = has_uncompleted_tasks,
        "Teammate idle recorded"
    );

    Ok(())
}

/// N1: Handle `task-completed` hook — deposits task success pheromone.
///
/// Input JSON shape:
/// ```json
/// {
///   "session_id": "...",
///   "task_id": "task-42",
///   "task_subject": "Engineer: implement cache core",
///   "success": true,
///   "duration_ms": 45230,
///   "result_summary": "cache implemented, all tests pass"
/// }
/// ```
///
/// Fire-and-forget: ACO wiring failures are silently swallowed, exit 0 always.
#[tracing::instrument(skip(runtime, input), fields(hook = "task_completed"))]
pub fn run_task_completed(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    // D9: Validate payload — skip on failure (non-blocking).
    if validate_payload::<crate::schemas::TaskCompletedPayload>(input).is_err() {
        return Ok(());
    }
    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let success = input
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Record lifecycle event in knowledge DB (existing behavior)
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let _ = runtime
        .ctx
        .knowledge
        .record_access("__task_completed__", session_id);

    // Log summary if available
    if let Some(summary) = input
        .get("result_summary")
        .or_else(|| input.get("message"))
        .and_then(|v| v.as_str())
    {
        if !summary.is_empty() {
            let _ = runtime
                .ctx
                .knowledge
                .record_bash_outcome(&crate::knowledge::BashOutcome {
                    command: format!(
                        "task_completion:{}:{}",
                        task_id,
                        &summary[..summary.len().min(200)]
                    ),
                    command_short: "task_completion".to_string(),
                    exit_code: if success { 0 } else { 1 },
                    success,
                    error_pattern: None,
                    file_context: None,
                    command_hash: String::new(),
                    executed_at: String::new(),
                });
        }
    }

    // N1: Wire to ACO — deposit task heat
    if let Ok(wiring) = runtime.aco_wiring.lock() {
        wiring.deposit_task_completion(task_id, success);
    }

    tracing::debug!(
        task_id = task_id,
        success = success,
        "Task completed recorded"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().unwrap();
        let mut rt = HookRuntime::new(tmp.path()).unwrap();
        rt.reset_quality_tracking("team-test-session");
        (tmp, rt)
    }

    // ── task-created tests ────────────────────────────────────────────────

    #[test]
    fn test_task_created_records_in_knowledge_db() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "task_id": "task-001",
            "task_subject": "Engineer: implement cache core",
            "task_description": "Implement LRU cache",
            "teammate_name": "engineer-1",
            "team_name": "taco-abc123"
        });
        let result = run_task_created(&mut rt, &input);
        assert!(result.is_ok());

        // Verify knowledge DB was updated
        let count = rt.ctx.knowledge.access_count("__task_created__").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_task_created_unknown_fields_default_gracefully() {
        let (_tmp, mut rt) = setup();
        // Minimal input — all fields optional
        let input = serde_json::json!({ "session_id": "s1" });
        let result = run_task_created(&mut rt, &input);
        assert!(
            result.is_ok(),
            "Missing fields must not panic — fire-and-forget"
        );
    }

    #[test]
    fn test_task_created_multiple_tasks_accumulate() {
        let (_tmp, mut rt) = setup();
        for i in 0..3u32 {
            let input = serde_json::json!({
                "session_id": "s1",
                "task_id": format!("task-{i:03}"),
                "task_subject": format!("Task {i}"),
                "teammate_name": "orchestrator",
                "team_name": "taco-test"
            });
            let _ = run_task_created(&mut rt, &input);
        }
        let count = rt.ctx.knowledge.access_count("__task_created__").unwrap();
        assert_eq!(count, 3, "Three task creations should be recorded");
    }

    #[test]
    fn test_task_created_long_subject_truncated_safely() {
        let (_tmp, mut rt) = setup();
        let long_subject = "x".repeat(500);
        let input = serde_json::json!({
            "session_id": "s1",
            "task_id": "task-long",
            "task_subject": long_subject,
            "team_name": "taco-test"
        });
        let result = run_task_created(&mut rt, &input);
        assert!(
            result.is_ok(),
            "Long subject must not panic (truncated to 100 chars)"
        );
    }

    // ── teammate-idle tests ───────────────────────────────────────────────

    #[test]
    fn test_teammate_idle_records_and_wires() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "engineer-1",
            "tasks_completed": 3
        });
        let result = run_teammate_idle(&mut rt, &input);
        assert!(result.is_ok());

        // Verify knowledge DB was updated
        let count = rt.ctx.knowledge.access_count("__teammate_idle__").unwrap();
        assert_eq!(count, 1);

        // Verify ACO wiring received the deposit
        let wiring = rt.aco_wiring.lock().unwrap();
        let heat = wiring.teammate_heat("engineer-1");
        assert!((heat - 1.5).abs() < 1e-9, "3 tasks * 0.5 = 1.5");
    }

    #[test]
    fn test_teammate_idle_unknown_name_defaults() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({ "session_id": "s1" });
        let result = run_teammate_idle(&mut rt, &input);
        assert!(result.is_ok());
        // Should not panic with unknown teammate
    }

    #[test]
    fn test_task_completed_success_wires_positive() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "task_id": "task-42",
            "success": true,
            "result_summary": "all tests pass"
        });
        let result = run_task_completed(&mut rt, &input);
        assert!(result.is_ok());

        // Verify knowledge DB was updated
        let count = rt.ctx.knowledge.access_count("__task_completed__").unwrap();
        assert_eq!(count, 1);

        // Verify ACO wiring received positive deposit
        let wiring = rt.aco_wiring.lock().unwrap();
        let heat = wiring.task_heat("task-42");
        assert!(heat > 0.0, "successful task should have positive heat");
    }

    #[test]
    fn test_task_completed_failure_wires_negative() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "task_id": "task-fail",
            "success": false,
            "result_summary": "tests failed"
        });
        let result = run_task_completed(&mut rt, &input);
        assert!(result.is_ok());

        let wiring = rt.aco_wiring.lock().unwrap();
        let heat = wiring.task_heat("task-fail");
        assert!(heat < 0.0, "failed task should have negative heat");
    }

    #[test]
    fn test_task_completed_unknown_id_defaults() {
        let (_tmp, mut rt) = setup();
        // No task_id, no success field
        let input = serde_json::json!({ "session_id": "s1" });
        let result = run_task_completed(&mut rt, &input);
        assert!(result.is_ok()); // Must not panic — fire-and-forget
    }

    #[test]
    fn test_task_completed_multiple_deposits_accumulate() {
        let (_tmp, mut rt) = setup();
        for _i in 0..5 {
            let input = serde_json::json!({
                "session_id": "s1",
                "task_id": "task-repeated",
                "success": true
            });
            let _ = run_task_completed(&mut rt, &input);
        }
        let wiring = rt.aco_wiring.lock().unwrap();
        let heat = wiring.task_heat("task-repeated");
        assert!((heat - 5.0).abs() < 1e-9, "5 successes = 5.0 heat");
    }

    #[test]
    fn test_teammate_idle_limbo_with_blocked_tasks() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "engineer-blocked",
            "tasks_completed": 0,
            "blocked_tasks_count": 3,
            "has_uncompleted_tasks": false
        });
        let result = run_teammate_idle(&mut rt, &input);
        assert!(result.is_ok());

        let wiring = rt.aco_wiring.lock().unwrap();
        // Teammate heat should be 0 (no tasks completed)
        let heat = wiring.teammate_heat("engineer-blocked");
        assert!((heat - 0.0).abs() < 1e-9, "0 tasks = 0 heat");
        // Limbo heat should be negative (3 blocked * -0.3 = -0.9)
        let limbo = wiring.limbo_heat("engineer-blocked");
        assert!(
            (limbo - (-0.9)).abs() < 1e-9,
            "3 blocked * -0.3 = -0.9, got {limbo}"
        );
    }

    #[test]
    fn test_teammate_idle_limbo_with_uncompleted_tasks() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "engineer-stuck",
            "tasks_completed": 0,
            "has_uncompleted_tasks": true
        });
        let result = run_teammate_idle(&mut rt, &input);
        assert!(result.is_ok());

        let wiring = rt.aco_wiring.lock().unwrap();
        // Limbo heat: 1 uncompleted * -1.0 = -1.0
        let limbo = wiring.limbo_heat("engineer-stuck");
        assert!(
            (limbo - (-1.0)).abs() < 1e-9,
            "1 uncompleted * -1.0 = -1.0, got {limbo}"
        );
    }

    #[test]
    fn test_teammate_idle_limbo_mixed_blocked_and_uncompleted() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "engineer-mixed",
            "tasks_completed": 0,
            "blocked_tasks_count": 2,
            "has_uncompleted_tasks": true
        });
        let result = run_teammate_idle(&mut rt, &input);
        assert!(result.is_ok());

        let wiring = rt.aco_wiring.lock().unwrap();
        // has_uncompleted_tasks=true BUT blocked_tasks_count>0, so uncompleted_count=0
        // Limbo: 0 uncompleted * -1.0 + 2 blocked * -0.3 = -0.6
        let limbo = wiring.limbo_heat("engineer-mixed");
        assert!(
            (limbo - (-0.6)).abs() < 1e-9,
            "2 blocked * -0.3 = -0.6, got {limbo}"
        );
    }

    #[test]
    fn test_teammate_idle_no_limbo_when_tasks_completed() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "engineer-productive",
            "tasks_completed": 5,
            "blocked_tasks_count": 2,
            "has_uncompleted_tasks": true
        });
        let result = run_teammate_idle(&mut rt, &input);
        assert!(result.is_ok());

        let wiring = rt.aco_wiring.lock().unwrap();
        // Productive: 5 tasks * 0.5 = 2.5
        let heat = wiring.teammate_heat("engineer-productive");
        assert!((heat - 2.5).abs() < 1e-9);
        // NO limbo — tasks_completed > 0
        let limbo = wiring.limbo_heat("engineer-productive");
        assert!((limbo - 0.0).abs() < 1e-9, "no limbo when tasks completed");
    }

    #[test]
    fn test_teammate_idle_limbo_accumulates() {
        let (_tmp, mut rt) = setup();
        // Simulate 3 consecutive limbo events
        for _ in 0..3 {
            let input = serde_json::json!({
                "session_id": "s1",
                "teammate_name": "chronic-limbo",
                "tasks_completed": 0,
                "has_uncompleted_tasks": true
            });
            let _ = run_teammate_idle(&mut rt, &input);
        }

        let wiring = rt.aco_wiring.lock().unwrap();
        // 3 limbos * -1.0 = -3.0
        let limbo = wiring.limbo_heat("chronic-limbo");
        assert!(
            (limbo - (-3.0)).abs() < 1e-9,
            "3 limbos = -3.0, got {limbo}"
        );
    }

    #[test]
    fn test_fire_and_forget_aco_failure_does_not_propagate() {
        let (_tmp, mut rt) = setup();
        // Runtime has no quality_assessment, but ACO wiring should still work
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "ghost",
            "tasks_completed": 0
        });
        let result = run_teammate_idle(&mut rt, &input);
        assert!(
            result.is_ok(),
            "ACO wiring failure must not affect hook result"
        );
    }

    // ── teammate-idle-gate tests (anti-limbo) ─────────────────────────

    #[test]
    fn test_idle_gate_blocks_idle_with_no_tasks() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "stuck-agent",
            "team_name": "taco-test",
            "tasks_completed": 0,
            "has_uncompleted_tasks": true
        });
        let result = run_teammate_idle_gate(&mut rt, &input);
        assert_eq!(result.exit_code, 2, "Should block idle");
        assert!(!result.context.is_empty(), "Should inject context");
        assert!(
            result.context.contains("ToolSearch"),
            "Context should mention ToolSearch: {}",
            result.context
        );
    }

    #[test]
    fn test_idle_gate_allows_idle_when_productive() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "productive-agent",
            "team_name": "taco-test",
            "tasks_completed": 3
        });
        let result = run_teammate_idle_gate(&mut rt, &input);
        assert_eq!(
            result.exit_code, 0,
            "Should allow idle for productive teammate"
        );
    }

    #[test]
    fn test_idle_gate_circuit_breaker() {
        let (_tmp, mut rt) = setup();
        // Simulate MAX_IDLE_RECOVERY idle events
        for _ in 0..MAX_IDLE_RECOVERY {
            let input = serde_json::json!({
                "session_id": "s1",
                "teammate_name": "chronic",
                "team_name": "taco-test",
                "tasks_completed": 0,
                "has_uncompleted_tasks": true
            });
            let result = run_teammate_idle_gate(&mut rt, &input);
            assert_eq!(result.exit_code, 2, "Should block before circuit breaker");
        }
        // Next attempt should allow idle (circuit breaker)
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "chronic",
            "team_name": "taco-test",
            "tasks_completed": 0,
            "has_uncompleted_tasks": true
        });
        let result = run_teammate_idle_gate(&mut rt, &input);
        assert_eq!(result.exit_code, 0, "Circuit breaker should allow idle");
    }

    #[test]
    fn test_idle_gate_context_detects_toolsearch_in_transcript() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "agent-with-tools",
            "team_name": "taco-test",
            "tasks_completed": 0,
            "transcript_summary": "Called ToolSearch and got results"
        });
        let result = run_teammate_idle_gate(&mut rt, &input);
        assert_eq!(result.exit_code, 2);
        assert!(
            result.context.contains("TaskList"),
            "Should suggest TaskList after ToolSearch: {}",
            result.context
        );
        assert!(
            !result.context.contains("ToolSearch"),
            "Should NOT suggest ToolSearch again"
        );
    }

    #[test]
    fn test_idle_gate_serializes_to_json() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "teammate_name": "json-test",
            "team_name": "taco-test",
            "tasks_completed": 0
        });
        let result = run_teammate_idle_gate(&mut rt, &input);
        let json = serde_json::to_string(&result).expect("TeamHookGateResult must be serializable");
        assert!(json.contains("\"exit_code\":2"));
        assert!(json.contains("\"context\":"));
    }

    // ── subagent-bootstrap tests ──────────────────────────────────────

    #[test]
    fn test_bootstrap_context_is_minimal() {
        let ctx = subagent_bootstrap_context();
        let lines: Vec<&str> = ctx.lines().collect();
        assert!(
            lines.len() <= 4,
            "Bootstrap must be <= 4 lines, got {}",
            lines.len()
        );
        assert!(ctx.contains("ToolSearch"), "Must mention ToolSearch");
        assert!(ctx.contains("TaskList"), "Must mention TaskList");
        assert!(
            ctx.contains("Do NOT"),
            "Must include anti-planning directive"
        );
    }

    // ── subagent-stop gate tests ─────────────────────────────────────

    #[test]
    fn test_subagent_stop_pure_subagent_allows_stop() {
        let (_tmp, mut rt) = setup();
        // Transcript with no team infrastructure tools → pure subagent
        let input = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "SubagentStop",
            "transcript": [
                {"role": "assistant", "content": [{"type": "tool_use", "name": "Read", "input": {"file_path": "foo.rs"}}]}
            ]
        });
        let result = run_subagent_stop_gate(&mut rt, &input);
        assert_eq!(result.exit_code, 0, "Pure subagent must be allowed to stop");
        assert!(result.feedback.is_empty());
    }

    #[test]
    fn test_subagent_stop_agent_teams_without_completed_task_blocks() {
        let (_tmp, mut rt) = setup();
        // Agent Teams teammate that used SendMessage but no TaskUpdate completed
        let input = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "SubagentStop",
            "transcript": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "name": "TaskCreate", "input": {"task_id": "t1", "task_subject": "Do X"}},
                    {"type": "tool_use", "name": "SendMessage", "input": {"to": "lead", "message": "Done?"}}
                ]}
            ]
        });
        let result = run_subagent_stop_gate(&mut rt, &input);
        assert_eq!(
            result.exit_code, 2,
            "Must block stop without completed TaskUpdate"
        );
        assert!(result.feedback.contains("TaskUpdate"));
    }

    #[test]
    fn test_subagent_stop_agent_teams_with_completed_task_allows_stop() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "SubagentStop",
            "transcript": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "name": "TaskCreate", "input": {"task_id": "t1", "task_subject": "Do X"}},
                    {"type": "tool_use", "name": "TaskUpdate", "input": {"taskId": "t1", "status": "completed"}}
                ]}
            ]
        });
        let result = run_subagent_stop_gate(&mut rt, &input);
        assert_eq!(
            result.exit_code, 0,
            "Must allow stop after completed TaskUpdate"
        );
        assert!(result.feedback.is_empty());
    }

    #[test]
    fn test_subagent_stop_circuit_breaker_allows_stop() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "SubagentStop",
            "stop_hook_active": true,
            "transcript": []
        });
        let result = run_subagent_stop_gate(&mut rt, &input);
        assert_eq!(result.exit_code, 0, "Circuit breaker must allow stop");
        assert!(result.feedback.is_empty());
    }

    #[test]
    fn test_subagent_stop_no_transcript_allows_stop() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "SubagentStop"
        });
        let result = run_subagent_stop_gate(&mut rt, &input);
        assert_eq!(result.exit_code, 0, "No transcript → fail open, allow stop");
        assert!(result.feedback.is_empty());
    }

    #[test]
    fn test_subagent_stop_result_serializes_to_json() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({"session_id": "s1"});
        let result = run_subagent_stop_gate(&mut rt, &input);
        let json = serde_json::to_string(&result).expect("SubagentStopResult must be serializable");
        assert!(json.contains("\"exit_code\":0"));
        assert!(json.contains("\"feedback\":"));
    }

    #[test]
    fn test_subagent_stop_blocks_without_completed_task_update() {
        let (_tmp, mut rt) = setup();
        // TaskUpdate exists but with status="in_progress", not "completed"
        let input = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "SubagentStop",
            "transcript": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "name": "TaskUpdate", "input": {"taskId": "t1", "status": "in_progress"}}
                ]}
            ]
        });
        let result = run_subagent_stop_gate(&mut rt, &input);
        assert_eq!(
            result.exit_code, 2,
            "in_progress is not completed — must block"
        );
    }
}
