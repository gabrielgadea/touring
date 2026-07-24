//! `task-sync-post-list` hook handler + co-located helpers.
//!
//! Fires on PostToolUse(TaskList). Injects live Touring decompose DAG summary,
//! ready subtasks, MCTS ordering hints, generator scaffolding suggestions, and
//! RL reward feedback into Claude Code's context window.
//!
//! All helpers in this file are exclusive to `handle_task_sync_post_list` —
//! `in_progress_count_advisory`, `pending_tasks_mcts_hint`, `dag_cc_task_ratio_hint`,
//! `first_inprogress_task_lesson_hint`, `tantivy_search_for_inprogress_task`,
//! `maybe_all_completed_finalize_hint`, all 30 `maybe_*_hint_on_task_list` helpers,
//! `generator_for_first_inprogress_cc_task`, `build_ready_subtasks_hint_from_json`,
//! `code_symbols_for_active_tasks`, `search_symbols_for_first_ready_task`,
//! `generator_for_first_ready_subtask`, `upsert_task_completion_to_tantivy`,
//! and `upsert_file_changed_to_tantivy`.
//!
//! Co-location rationale: these helpers share the same "TaskList → generator hint"
//! design and change together; moving them to `lifecycle/shared.rs` would bloat
//! the cross-cutting API surface.
//!
//! All public helpers are `pub(crate)` so the inline tests in `lifecycle::tests`
//! continue to reach them via `super::<helper>` after
//! `pub(crate) use task_list::*` re-export.
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D7.

use serde_json::Value;

use crate::runtime::HookRuntime;
use crate::shared::{TaskRoutingDecision, extract_task_features};

// Pull in shared helpers used by this handler.
use super::{file_stem, suggest_generator_for_task_subject};

/// Extract the CC task list from a TaskList hook input.
///
/// Resolves `input["tool_result"]["tasks"]`, falling back to `input["result"]["tasks"]`
/// and finally `input["tasks"]` directly. Returns `None` when the payload is absent or
/// malformed. All TaskList hook helpers share this as the single extraction point.
fn extract_task_list(input: &Value) -> Option<&Vec<Value>> {
    let tool_result = input
        .get("tool_result")
        .or_else(|| input.get("result"))
        .unwrap_or(input);
    tool_result.get("tasks").and_then(|t| t.as_array())
}

/// Returns `true` when any task's `title` or `description` (lowercased) contains
/// at least one of `keywords`. Used by all 30 `maybe_*_hint_on_task_list` helpers
/// to replace the repeated inline keyword-scan body.
fn any_task_has_keyword(tasks: &[Value], keywords: &[&str]) -> bool {
    tasks.iter().any(|t| {
        let subject = t
            .get("title")
            .or_else(|| t.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        keywords.iter().any(|kw| subject.contains(*kw))
    })
}

/// task-sync-list: PostToolUse(TaskList) — append Touring decompose DAG summary + ready subtasks.
///
/// Fires after Claude Code's TaskList tool call. Injects live DAG task count and
/// ready subtasks (pending deps all completed) so Claude Code sees immediately
/// actionable work without requiring a separate decompose query.
pub(crate) fn handle_task_sync_post_list(rt: &mut HookRuntime, input: &Value) -> String {
    // R12-S4: Query decompose status directly — inject real DAG counts alongside hint.
    // TaskList (Claude Code) → cli_decompose_status → SQLite → live task count in context.
    let status_json = crate::cli_handlers::cli_decompose_status(rt, &serde_json::json!({}));
    let dag_context = if status_json.contains("\"task_count\"") {
        format!(" [live: {}]", &status_json[..status_json.len().min(120)])
    } else {
        String::new()
    };

    // R17-S1: Query ready subtasks and inject inline — Claude Code sees actionable work immediately.
    // TaskList (Claude Code) → cli_decompose_ready → SQLite → ready subtask list in context.
    // Skips the manual step of running `touring decompose ready` after every TaskList.
    // R21-S2: Reuse the same ready_json for BM25 symbol cross-reference (single DB call).
    let ready_json = crate::cli_handlers::cli_decompose_ready(rt, &serde_json::json!({}));
    let ready_hint = build_ready_subtasks_hint_from_json(&ready_json);
    let code_hint = code_symbols_for_active_tasks(&ready_json);
    // R25-S3: Surface generator kind for the first ready subtask so Claude Code
    // knows which artifact to scaffold without an extra round-trip.
    let gen_hint = generator_for_first_ready_subtask(&ready_json);
    // Fix-B1: gen_hint already starts with " | generator: ..." so use directly (no double-pipe).
    let gen_suffix = gen_hint;
    // R29-S3: Detect excessive CC task concurrency → advise focusing on Touring DAG ready subtasks.
    // When TaskList returns many in_progress tasks, context fragmentation hurts quality.
    let concurrency_hint = in_progress_count_advisory(input);
    // R31-S3: Surface generator kind for the first in_progress CC task subject.
    // Complements generator_for_first_ready_subtask (Touring DAG) with CC task perspective.
    let active_cc_gen = generator_for_first_inprogress_cc_task(input)
        .map(|h| format!(" | {h}"))
        .unwrap_or_default();
    // R34-S2: Compare CC task count vs Touring DAG task count — surface sync advisory.
    let ratio_hint = dag_cc_task_ratio_hint(input, &status_json);
    // R157: RL penalty when CC/Touring DAG desync detected — closes the advisory → RL feedback loop.
    // dag_cc_task_ratio_hint (R34-S2) fires when >2 CC tasks are untracked in Touring DAG.
    // Until R157, this was advisory-only (text hint). R157 adds a -0.05 RL signal so the engine
    // learns that CC/Touring desync is a negative pattern, incentivizing agents to keep task
    // systems in sync — directly addresses "integração e sincronia entre tarefas do claude code e touring".
    if !ratio_hint.is_empty() {
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "orchestrate",
                "reward_value": -0.05,
                "context": "task_list:dag_cc_desync",
            }),
        );
    }
    // R38-S2: Recall past lessons for the first in_progress CC task.
    // TaskList → first in_progress task_id → cli_memory_recall → lesson hint inline.
    // Bridges CC task history to Touring memory so lessons from past runs surface immediately.
    let lesson_hint = first_inprogress_task_lesson_hint(rt, input);
    // R136: Persist active task snapshot to memory for cross-session recall.
    // TaskList (CC) → compact snapshot → `touring memory recall "active_tasks_latest"`.
    // Enables future sessions to reconstruct task state without full DAG re-query.
    // Only stores when task_count > 0 — silent on empty DAG to avoid stale entries.
    {
        let snapshot_task_count = serde_json::from_str::<serde_json::Value>(&status_json)
            .ok()
            .and_then(|v| v.get("task_count").and_then(|n| n.as_u64()))
            .unwrap_or(0);
        let snapshot_ready_count = serde_json::from_str::<serde_json::Value>(&ready_json)
            .ok()
            .and_then(|v| v.get("ready_count").and_then(|n| n.as_u64()))
            .unwrap_or(0);
        if snapshot_task_count > 0 {
            let snapshot_value = format!(
                "task_count={snapshot_task_count} ready={snapshot_ready_count} | restore: `touring decompose status -j`"
            );
            let _ = crate::cli_handlers::cli_memory_store(
                rt,
                &serde_json::json!({
                    "key": "active_tasks_latest",
                    "value": snapshot_value,
                    "tier": "semantic",
                    "entry_type": "lesson",
                }),
            );
            // R148: RL reward when ready subtasks exist — closes the TaskList → RL feedback loop.
            // When TaskList reveals actionable work (ready_count > 0), that's a positive
            // orchestration signal: the planner is keeping the DAG unblocked. Reward +0.1
            // reinforces the pattern of maintaining a healthy ready queue.
            if snapshot_ready_count > 0 {
                let context = format!("task_list:ready:{snapshot_ready_count}");
                let _ = crate::cli_handlers::cli_learning_reward(
                    rt,
                    &serde_json::json!({
                        "tool_name": "orchestrate",
                        "reward_value": 0.1,
                        "context": context,
                    }),
                );
            }
        }
    }
    // R41-S2: When 3+ tasks are pending, surface MCTS search for optimal execution ordering.
    // Pending tasks lack execution signals — MCTS can find the optimal path through the backlog.
    let mcts_hint = pending_tasks_mcts_hint(input);
    // R44-S2: Surface Tantivy code symbol search for the first in_progress task subject.
    // Complements first_inprogress_task_lesson_hint (lessons) with live code symbol discovery.
    let tantivy_task_hint = tantivy_search_for_inprogress_task(input);
    // R47-S2: When all CC tasks are completed, suggest DAG finalization via touring decompose.
    // `unwrap_or_default()` — None maps to "" — zero CC addition to handle_task_sync_post_list.
    let finalize_hint = maybe_all_completed_finalize_hint(input).unwrap_or_default();
    // R66-S1: When tasks mention REST/OpenAPI keywords, surface openapi_spec generator.
    let openapi_hint = maybe_openapi_hint_on_task_list(input).unwrap_or_default();
    // R66-S2: When tasks mention architecture-decision keywords, surface adr generator.
    let adr_hint = maybe_adr_hint_on_task_list(input).unwrap_or_default();
    // R66-S3: When tasks mention error-catalog keywords, surface error_catalog generator.
    let error_catalog_hint = maybe_error_catalog_hint_on_task_list(input).unwrap_or_default();
    // R72-S1..S3: benchmark/terraform/ci_workflow generators from task subject patterns.
    let benchmark_list_hint = maybe_benchmark_hint_on_task_list(input).unwrap_or_default();
    let terraform_list_hint = maybe_terraform_hint_on_task_list(input).unwrap_or_default();
    let ci_list_hint = maybe_ci_workflow_hint_on_task_list(input).unwrap_or_default();
    // R78-S1..S3: TaskList → rust_module/protobuf/derive_macro generator hints.
    let rust_module_list_hint = maybe_rust_module_hint_on_task_list(input).unwrap_or_default();
    let protobuf_list_hint = maybe_protobuf_hint_on_task_list(input).unwrap_or_default();
    let derive_macro_list_hint = maybe_derive_macro_hint_on_task_list(input).unwrap_or_default();
    // R79-S1..S3: TaskList → fuzz_target/migration/schema generator hints.
    let fuzz_list_hint = maybe_fuzz_target_hint_on_task_list(input).unwrap_or_default();
    let migration_list_hint = maybe_migration_hint_on_task_list(input).unwrap_or_default();
    let schema_list_hint = maybe_schema_hint_on_task_list(input).unwrap_or_default();
    // R81-S1..S3: TaskList → changelog/dockerfile/k8s_manifest generator hints.
    let changelog_list_hint = maybe_changelog_hint_on_task_list(input).unwrap_or_default();
    let dockerfile_list_hint = maybe_dockerfile_hint_on_task_list(input).unwrap_or_default();
    let k8s_list_hint = maybe_k8s_hint_on_task_list(input).unwrap_or_default();
    // R82-S1..S3: TaskList → asyncapi/consumer/task_scaffold generator hints.
    let asyncapi_list_hint = maybe_asyncapi_hint_on_task_list(input).unwrap_or_default();
    let consumer_list_hint = maybe_consumer_hint_on_task_list(input).unwrap_or_default();
    let task_scaffold_list_hint = maybe_task_scaffold_hint_on_task_list(input).unwrap_or_default();
    // R83-S1..S3: TaskList → man_page/incremental_patch/skill_document generator hints.
    let man_page_list_hint = maybe_man_page_hint_on_task_list(input).unwrap_or_default();
    let incremental_patch_list_hint =
        maybe_incremental_patch_hint_on_task_list(input).unwrap_or_default();
    let skill_document_list_hint =
        maybe_skill_document_hint_on_task_list(input).unwrap_or_default();
    // R84-S1..S3: TaskList → cli_handler/mcp_tool/hook_handler generator hints.
    let cli_handler_list_hint = maybe_cli_handler_hint_on_task_list(input).unwrap_or_default();
    let mcp_tool_list_hint = maybe_mcp_tool_hint_on_task_list(input).unwrap_or_default();
    let hook_handler_list_hint = maybe_hook_handler_hint_on_task_list(input).unwrap_or_default();
    // R85-S1..S3: TaskList → plan_md/test/python_script generator hints.
    let plan_md_list_hint = maybe_plan_md_hint_on_task_list(input).unwrap_or_default();
    let test_list_hint = maybe_test_hint_on_task_list(input).unwrap_or_default();
    let python_script_list_hint = maybe_python_script_hint_on_task_list(input).unwrap_or_default();
    // R86-S1..S3: TaskList → ffi_binding/shell_completion/diary_entry generator hints (30/30 complete).
    let ffi_binding_list_hint = maybe_ffi_binding_hint_on_task_list(input).unwrap_or_default();
    let shell_completion_list_hint =
        maybe_shell_completion_hint_on_task_list(input).unwrap_or_default();
    let diary_entry_list_hint = maybe_diary_entry_hint_on_task_list(input).unwrap_or_default();
    // D3: LinUCB contextual bandit routing hint — emitted when EV of non-manual arm
    // exceeds manual edit by a confidence margin. Uses try_lock so it is always NOOP-safe.
    let linucb_router_hint = linucb_routing_hint(rt, input);
    // AcoPheromone §3: heat-based task priority — orders pending tasks by wiring entropy.
    // Low integration_score modules have high orphan density = high refactoring risk = prioritize.
    let heat_hint = heat_based_task_priority_hint(rt, input);
    format!(
        "touring-sync: run `touring decompose status -j` for DAG view{dag_context}{ready_hint}{code_hint}{gen_suffix}{active_cc_gen}{concurrency_hint}{ratio_hint}{lesson_hint}{mcts_hint}{tantivy_task_hint}{finalize_hint}{openapi_hint}{adr_hint}{error_catalog_hint}{benchmark_list_hint}{terraform_list_hint}{ci_list_hint}{rust_module_list_hint}{protobuf_list_hint}{derive_macro_list_hint}{fuzz_list_hint}{migration_list_hint}{schema_list_hint}{changelog_list_hint}{dockerfile_list_hint}{k8s_list_hint}{asyncapi_list_hint}{consumer_list_hint}{task_scaffold_list_hint}{man_page_list_hint}{incremental_patch_list_hint}{skill_document_list_hint}{cli_handler_list_hint}{mcp_tool_list_hint}{hook_handler_list_hint}{plan_md_list_hint}{test_list_hint}{python_script_list_hint}{ffi_binding_list_hint}{shell_completion_list_hint}{diary_entry_list_hint}{linucb_router_hint}{heat_hint} | orphan symbols: `touring wiring orphans -j` | scaffold: `touring generate plan-suggest --intent \"<intent>\"`"
    )
}

/// D3: LinUCB contextual bandit routing hint for the first pending task (CC≤5).
///
/// Emits a delegation hint when the LinUCB winning arm is not `ManualEdit` and
/// the confidence margin (best − second_best) exceeds 0.15. Always NOOP-safe.
pub(crate) fn linucb_routing_hint(rt: &mut HookRuntime, input: &Value) -> String {
    let task = match first_pending_task(input) {
        Some(t) => t,
        None => return String::new(),
    };
    let subject = task_subject(task);
    let (best_arm, best_score, margin) = linucb_score(rt, task);
    const CONFIDENCE_THRESHOLD: f64 = 0.15;
    if margin < CONFIDENCE_THRESHOLD {
        return String::new();
    }
    emit_routing_hint(best_arm, best_score, margin, subject)
}

/// Returns the first task with `status == "pending"` from the TaskList payload.
fn first_pending_task(input: &Value) -> Option<&Value> {
    let tasks = extract_task_list(input)?;
    tasks
        .iter()
        .find(|t| t.get("status").and_then(|s| s.as_str()) == Some("pending"))
}

/// Extracts the subject string from a task JSON object.
fn task_subject(task: &Value) -> &str {
    task.get("title")
        .or_else(|| task.get("description"))
        .or_else(|| task.get("subject"))
        .or_else(|| task.get("task_subject"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
}

/// Queries the LinUCB bandit and returns `(best_arm, best_score, confidence_margin)`.
fn linucb_score(rt: &mut HookRuntime, task: &Value) -> (usize, f64, f64) {
    use ndarray::Array1;
    let raw = extract_task_features(task);
    let features = Array1::from_vec(raw.to_vec());
    let bandit = rt.linucb_bandit();
    let (best_arm, best_score) = bandit.select_arm(&features);
    let second_best = bandit
        .arm_stats()
        .iter()
        .filter(|(i, _, _)| *i != best_arm)
        .map(|(_, _, avg)| *avg)
        .fold(f64::NEG_INFINITY, f64::max);
    let margin = best_score - second_best.max(0.0);
    (best_arm, best_score, margin)
}

/// Formats the routing hint string, or returns empty when `ManualEdit` wins.
fn emit_routing_hint(best_arm: usize, best_score: f64, margin: f64, subject: &str) -> String {
    let decision = TaskRoutingDecision::from_arm_id(best_arm);
    // Predictive Wave D3: classify the winning arm for observability.
    // ManualEdit → no hint; any other arm that produces a hint → generator family.
    match decision.hint(subject) {
        Some(hint) => {
            crate::shared::gate_metrics::record_linucb_route_generator();
            crate::shared::gate_metrics::record_linucb_route_hint();
            tracing::info!(
                task_subject = subject,
                arm = best_arm,
                ev = best_score,
                margin,
                "linucb-router: routing hint emitted"
            );
            format!(" | rl-route: {hint}")
        }
        None => {
            crate::shared::gate_metrics::record_linucb_route_manual();
            String::new()
        }
    }
}

/// R29-S3: Count in_progress CC tasks from the TaskList tool result and warn on excess (CC≤4).
///
/// PostToolUse(TaskList) delivers the CC task list in `input["tool_result"]`. When more than
/// 3 tasks are simultaneously in_progress, context fragmentation hurts output quality.
/// Advises focusing on the Touring DAG ready subtasks instead of starting more work.
/// Returns empty string when count ≤ 3 or when the task list payload is absent/malformed.
pub(crate) fn in_progress_count_advisory(input: &Value) -> String {
    let tasks = match extract_task_list(input) {
        Some(arr) => arr,
        None => return String::new(),
    };
    let in_progress_count = tasks
        .iter()
        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("in_progress"))
        .count();
    if in_progress_count <= 3 {
        return String::new();
    }
    format!(
        " | ⚠ {in_progress_count} tasks in_progress — consider focusing: \
        run `touring decompose ready` for the next DAG subtask to complete first"
    )
}

/// R41-S2: When TaskList shows 3+ pending tasks, surface touring mcts search for ordering (CC≤4).
///
/// Pending tasks lack execution signals — MCTS can evaluate multi-step paths to find
/// the optimal execution order. Fires when pending count >= 3 to avoid noise on small lists.
/// Complements `in_progress_count_advisory` (fires on excess in_progress) with the pending side.
pub(crate) fn pending_tasks_mcts_hint(input: &Value) -> String {
    let tasks = match extract_task_list(input) {
        Some(arr) => arr,
        None => return String::new(),
    };
    let pending_count = tasks
        .iter()
        .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("pending"))
        .count();
    if pending_count < 3 {
        return String::new();
    }
    format!(
        " | mcts-plan: {pending_count} pending tasks — run \
        `touring mcts search \"prioritize {pending_count} pending tasks\"` for optimal execution order"
    )
}

/// R34-S2: Compare CC task count vs Touring DAG task count and surface a sync advisory (CC=4).
///
/// When CC has significantly more tasks than the Touring DAG, many CC tasks are untracked.
/// This function parses:
/// - `input["tool_result"]["tasks"]` → CC task count
/// - `dag_status_json["task_count"]` → Touring DAG task count
///
/// If `cc_count > dag_count + 2`, surfaces a batch-create advisory to sync the gap.
/// Returns empty string when counts are balanced or data is unavailable.
pub(crate) fn dag_cc_task_ratio_hint(input: &Value, dag_status_json: &str) -> String {
    let cc_count = extract_task_list(input).map(|arr| arr.len()).unwrap_or(0);
    if cc_count == 0 {
        return String::new();
    }
    let dag_count = serde_json::from_str::<serde_json::Value>(dag_status_json)
        .ok()
        .and_then(|v| v.get("task_count").and_then(|c| c.as_u64()))
        .unwrap_or(0) as usize;
    let untracked = cc_count.saturating_sub(dag_count);
    if untracked <= 2 {
        return String::new();
    }
    format!(
        " | dag-gap: {untracked} CC task(s) untracked in Touring DAG — \
        run `touring decompose create intent \"<subject>\"` for each to sync"
    )
}

/// R38-S2: Recall past Touring lessons for the first in_progress CC task (CC≤4).
///
/// Parses `tool_result.tasks` from the PostToolUse(TaskList) input, finds the first task
/// with `status == "in_progress"`, extracts its `task_id`, then calls `cli_memory_recall`
/// for `"task:<task_id>"` to surface stored lessons, gotchas, or patterns from prior runs.
/// Returns empty string when no in_progress task exists or when memory has no relevant entry.
pub(crate) fn first_inprogress_task_lesson_hint(rt: &mut HookRuntime, input: &Value) -> String {
    let tasks = match extract_task_list(input) {
        Some(arr) => arr,
        None => return String::new(),
    };
    let task_id = tasks
        .iter()
        .find(|t| t.get("status").and_then(|s| s.as_str()) == Some("in_progress"))
        .and_then(|t| {
            t.get("task_id")
                .or_else(|| t.get("id"))
                .and_then(|id| id.as_str())
        })
        .unwrap_or("");
    if task_id.is_empty() {
        return String::new();
    }
    let recall_payload = serde_json::json!({"query": format!("task:{task_id}"), "limit": 1});
    let recall = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
    if recall.contains("\"value\"") {
        format!(
            " | lesson: past pattern found for {task_id} — run `touring memory recall \"task:{task_id}\"` to review"
        )
    } else {
        String::new()
    }
}

/// R44-S2: Emit `touring tantivy search` hint for the first in_progress CC task subject (CC≤4).
///
/// `first_inprogress_task_lesson_hint` already recalls *past lessons* for the in_progress task.
/// This helper closes the complementary gap: surface *current code symbols* related to the task
/// subject via BM25 Tantivy search so the engineer can find relevant implementations without
/// leaving the hook context. Returns empty string when no in_progress task has a subject.
pub(crate) fn tantivy_search_for_inprogress_task(input: &Value) -> String {
    let tasks = match extract_task_list(input) {
        Some(arr) => arr,
        None => return String::new(),
    };
    let subject = tasks
        .iter()
        .find(|t| t.get("status").and_then(|s| s.as_str()) == Some("in_progress"))
        .and_then(|t| {
            t.get("task_subject")
                .or_else(|| t.get("title"))
                .or_else(|| t.get("description"))
                .and_then(|s| s.as_str())
        })
        .unwrap_or("");
    if subject.is_empty() {
        return String::new();
    }
    let query = &subject[..subject.len().min(40)];
    format!(
        " | tantivy-symbols: run `touring tantivy search \"{query}\"` to find related code symbols for active task"
    )
}

/// R47-S2: Suggest `touring decompose finalize` when all CC tasks are completed (CC≤3).
///
/// Reads `tool_result.tasks` from the PostToolUse(TaskList) input. When every task
/// has status `completed` (and there is at least one task), surfaces the finalize command
/// so Claude Code archives the DAG without a manual step.
/// Closes the "all work done" → "archive the DAG" loop via touring-generator/decompose.
/// Returns `None` when tasks are missing, empty, or any task is not yet completed.
pub(crate) fn maybe_all_completed_finalize_hint(input: &Value) -> Option<String> {
    let tasks = extract_task_list(input)?;
    if tasks.is_empty() {
        return None;
    }
    let all_done = tasks
        .iter()
        .all(|t| t.get("status").and_then(|s| s.as_str()) == Some("completed"));
    if !all_done {
        return None;
    }
    Some(format!(
        " | all-done: {} task(s) completed — run `touring decompose finalize <task_id>` to archive the DAG",
        tasks.len()
    ))
}

/// R66-S1: When any task subject contains API/REST/OpenAPI keywords, suggest openapi_spec (CC≤2).
///
/// Scans all tasks in `tool_result.tasks` for REST API, OpenAPI, or Swagger keywords.
/// Returns a hint to run `touring generate render openapi_spec` when detected.
/// Bridges TaskList(API tasks) → openapi_spec.tera → touring-generator OpenAPI scaffold.
pub(crate) fn maybe_openapi_hint_on_task_list(input: &Value) -> Option<String> {
    const OPENAPI_KEYWORDS: &[&str] = &[
        "openapi",
        "swagger",
        "rest api",
        "restful",
        "api spec",
        "api contract",
        "api endpoints",
        "http api",
        "api design",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, OPENAPI_KEYWORDS) {
        return None;
    }
    Some(
        " | openapi: API tasks detected — run `touring generate render openapi_spec` \
        to scaffold an OpenAPI spec via touring-generator"
            .to_string(),
    )
}

/// R66-S2: When any task subject contains architecture-decision keywords, suggest adr (CC≤2).
///
/// Scans all tasks in `tool_result.tasks` for ADR, architecture-decision, design-doc keywords.
/// Returns a hint to run `touring generate render adr` when detected.
/// Bridges TaskList(architecture tasks) → adr.tera → touring-generator ADR scaffold.
pub(crate) fn maybe_adr_hint_on_task_list(input: &Value) -> Option<String> {
    const ADR_KEYWORDS: &[&str] = &[
        "architecture decision",
        "adr",
        "design decision",
        "design doc",
        "architectural record",
        "tech decision",
        "technical decision",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, ADR_KEYWORDS) {
        return None;
    }
    Some(
        " | adr: architecture-decision tasks detected — run `touring generate render adr` \
        to scaffold an Architecture Decision Record via touring-generator"
            .to_string(),
    )
}

/// R66-S3: When any task subject contains error-catalog keywords, suggest error_catalog (CC≤2).
///
/// Scans all tasks in `tool_result.tasks` for error-code, error-catalog, error-registry keywords.
/// Returns a hint to run `touring generate render error_catalog` when detected.
/// Bridges TaskList(error-handling tasks) → error_catalog.tera → touring-generator catalog scaffold.
pub(crate) fn maybe_error_catalog_hint_on_task_list(input: &Value) -> Option<String> {
    const ERROR_CATALOG_KEYWORDS: &[&str] = &[
        "error catalog",
        "error code",
        "error registry",
        "error types",
        "error handling",
        "thiserror",
        "error enum",
        "custom errors",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, ERROR_CATALOG_KEYWORDS) {
        return None;
    }
    Some(
        " | error-catalog: error-type tasks detected — run `touring generate render error_catalog` \
        to scaffold an error catalog via touring-generator"
            .to_string(),
    )
}

/// R72-S1: Suggest `benchmark` generator when any task subject contains performance-testing keywords (CC≤2).
///
/// Scans all tasks in `tool_result.tasks` for criterion, benchmark, profiling, hyperfine keywords.
/// Bridges TaskList(perf tasks) → benchmark.tera → touring-generator Benchmark scaffold.
pub(crate) fn maybe_benchmark_hint_on_task_list(input: &Value) -> Option<String> {
    const BENCHMARK_KEYWORDS: &[&str] = &[
        "benchmark",
        "criterion",
        "performance test",
        "profiling",
        "hyperfine",
        "load test",
        "latency test",
        "divan bench",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, BENCHMARK_KEYWORDS) {
        return None;
    }
    Some(
        " | benchmark: performance tasks detected — run `touring generate render Benchmark` \
        to scaffold a Criterion benchmark via touring-generator"
            .to_string(),
    )
}

/// R72-S2: Suggest `terraform_module` generator when any task subject contains IaC keywords (CC≤2).
///
/// Scans all tasks in `tool_result.tasks` for terraform, iac, infrastructure as code keywords.
/// Bridges TaskList(infra tasks) → terraform_module.tera → touring-generator TerraformModule scaffold.
pub(crate) fn maybe_terraform_hint_on_task_list(input: &Value) -> Option<String> {
    const TERRAFORM_KEYWORDS: &[&str] = &[
        "terraform",
        "iac",
        "infrastructure as code",
        "aws provider",
        "azure module",
        "gcp resource",
        "tofu",
        "opentofu",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, TERRAFORM_KEYWORDS) {
        return None;
    }
    Some(
        " | terraform: IaC tasks detected — run `touring generate render TerraformModule` \
        to scaffold a Terraform module via touring-generator"
            .to_string(),
    )
}

/// R72-S3: Suggest `ci_workflow` generator when any task subject contains CI/CD keywords (CC≤2).
///
/// Scans all tasks in `tool_result.tasks` for github actions, ci/cd, pipeline, workflow keywords.
/// Bridges TaskList(ci tasks) → ci_workflow.tera → touring-generator CiWorkflow scaffold.
pub(crate) fn maybe_ci_workflow_hint_on_task_list(input: &Value) -> Option<String> {
    const CI_KEYWORDS: &[&str] = &[
        "ci/cd",
        "github actions",
        "pipeline",
        "continuous integration",
        "deploy pipeline",
        "ci workflow",
        "gh actions",
        "gitlab ci",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, CI_KEYWORDS) {
        return None;
    }
    Some(
        " | ci-workflow: CI/CD tasks detected — run `touring generate render CiWorkflow` \
        to scaffold a GitHub Actions workflow via touring-generator"
            .to_string(),
    )
}

/// R78-S1: hint for RustModule generator on TaskList (CC=2).
///
/// Fires when any task title/description contains Rust module keywords.
pub(crate) fn maybe_rust_module_hint_on_task_list(input: &Value) -> Option<String> {
    const RUST_KEYWORDS: &[&str] = &[
        "rust module",
        "new crate",
        "rust struct",
        "impl block",
        "rust trait",
        "cargo module",
        "new module",
        "rust library",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, RUST_KEYWORDS) {
        return None;
    }
    Some(
        " | rust-module: Rust module tasks detected — run `touring generate render RustModule` \
        to scaffold a Rust module via touring-generator"
            .to_string(),
    )
}

/// R78-S2: hint for ProtobufSchema generator on TaskList (CC=2).
///
/// Fires when any task title/description contains protobuf/gRPC keywords.
pub(crate) fn maybe_protobuf_hint_on_task_list(input: &Value) -> Option<String> {
    const PROTO_KEYWORDS: &[&str] = &[
        "protobuf",
        "proto schema",
        "grpc",
        "proto file",
        "protocol buffer",
        ".proto",
        "grpc service",
        "proto message",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, PROTO_KEYWORDS) {
        return None;
    }
    Some(
        " | protobuf: gRPC/protobuf tasks detected — run `touring generate render ProtobufSchema` \
        to scaffold a Protobuf schema via touring-generator"
            .to_string(),
    )
}

/// R78-S3: hint for DeriveMacro generator on TaskList (CC=2).
///
/// Fires when any task title/description contains derive macro / proc-macro keywords.
pub(crate) fn maybe_derive_macro_hint_on_task_list(input: &Value) -> Option<String> {
    const DERIVE_KEYWORDS: &[&str] = &[
        "derive macro",
        "proc macro",
        "proc-macro",
        "custom derive",
        "attribute macro",
        "macro derive",
        "procedural macro",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, DERIVE_KEYWORDS) {
        return None;
    }
    Some(
        " | derive-macro: proc-macro tasks detected — run `touring generate render DeriveMacro` \
        to scaffold a derive macro via touring-generator"
            .to_string(),
    )
}

/// R79-S1: hint for FuzzTarget generator on TaskList (CC=2).
///
/// Fires when any task title/description contains fuzzing or property-testing keywords.
pub(crate) fn maybe_fuzz_target_hint_on_task_list(input: &Value) -> Option<String> {
    const FUZZ_KEYWORDS: &[&str] = &[
        "fuzz target",
        "fuzzing",
        "fuzz test",
        "cargo fuzz",
        "property test",
        "proptest",
        "arbitrary input",
        "libfuzzer",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, FUZZ_KEYWORDS) {
        return None;
    }
    Some(
        " | fuzz-target: fuzzing tasks detected — run `touring generate render FuzzTarget` \
        to scaffold a cargo-fuzz target via touring-generator"
            .to_string(),
    )
}

/// R79-S2: hint for Migration generator on TaskList (CC=2).
///
/// Fires when any task title/description contains DB migration keywords.
pub(crate) fn maybe_migration_hint_on_task_list(input: &Value) -> Option<String> {
    const MIGRATION_KEYWORDS: &[&str] = &[
        "db migration",
        "database migration",
        "schema migration",
        "alter table",
        "sqlx migrate",
        "diesel migration",
        "migration script",
        "add column",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, MIGRATION_KEYWORDS) {
        return None;
    }
    Some(
        " | migration: DB migration tasks detected — run `touring generate render Migration` \
        to scaffold a database migration via touring-generator"
            .to_string(),
    )
}

/// R79-S3: hint for Schema generator on TaskList (CC=2).
///
/// Fires when any task title/description contains JSON schema or data schema keywords.
pub(crate) fn maybe_schema_hint_on_task_list(input: &Value) -> Option<String> {
    const SCHEMA_KEYWORDS: &[&str] = &[
        "json schema",
        "data schema",
        "schema definition",
        "serde schema",
        "validation schema",
        "openrpc schema",
        "api schema",
        "type schema",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, SCHEMA_KEYWORDS) {
        return None;
    }
    Some(
        " | schema: data schema tasks detected — run `touring generate render Schema` \
        to scaffold a schema definition via touring-generator"
            .to_string(),
    )
}

/// R81-S1: hint for ChangelogEntry generator on TaskList (CC=2).
///
/// Fires when any task title/description contains release/changelog keywords.
pub(crate) fn maybe_changelog_hint_on_task_list(input: &Value) -> Option<String> {
    const CHANGELOG_KEYWORDS: &[&str] = &[
        "changelog",
        "release notes",
        "version bump",
        "semver",
        "breaking change",
        "release entry",
        "release candidate",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, CHANGELOG_KEYWORDS) {
        return None;
    }
    Some(
        " | changelog: release tasks detected — run `touring generate render ChangelogEntry` \
        to scaffold a changelog entry via touring-generator"
            .to_string(),
    )
}

/// R81-S2: hint for Dockerfile generator on TaskList (CC=2).
///
/// Fires when any task title/description contains container/Docker keywords.
pub(crate) fn maybe_dockerfile_hint_on_task_list(input: &Value) -> Option<String> {
    const DOCKER_KEYWORDS: &[&str] = &[
        "dockerfile",
        "docker image",
        "containerize",
        "docker build",
        "docker container",
        "container image",
        "docker layer",
        "multi-stage build",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, DOCKER_KEYWORDS) {
        return None;
    }
    Some(
        " | dockerfile: container tasks detected — run `touring generate render Dockerfile` \
        to scaffold a Dockerfile via touring-generator"
            .to_string(),
    )
}

/// R81-S3: hint for K8sManifest generator on TaskList (CC=2).
///
/// Fires when any task title/description contains Kubernetes keywords.
pub(crate) fn maybe_k8s_hint_on_task_list(input: &Value) -> Option<String> {
    const K8S_KEYWORDS: &[&str] = &[
        "kubernetes",
        "k8s",
        "kubectl",
        "helm chart",
        "pod deployment",
        "k8s service",
        "ingress controller",
        "namespace",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, K8S_KEYWORDS) {
        return None;
    }
    Some(
        " | k8s-manifest: Kubernetes tasks detected — run `touring generate render K8sManifest` \
        to scaffold a K8s manifest via touring-generator"
            .to_string(),
    )
}

/// R82-S1: hint for AsyncApiSpec generator on TaskList (CC=2).
///
/// Fires when any task title/description contains async API / event-driven keywords.
pub(crate) fn maybe_asyncapi_hint_on_task_list(input: &Value) -> Option<String> {
    const ASYNC_KEYWORDS: &[&str] = &[
        "asyncapi",
        "event-driven",
        "message broker",
        "pubsub",
        "amqp",
        "kafka",
        "nats",
        "rabbitmq",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, ASYNC_KEYWORDS) {
        return None;
    }
    Some(
        " | asyncapi: event-driven tasks detected — run `touring generate render AsyncApiSpec` \
        to scaffold an AsyncAPI spec via touring-generator"
            .to_string(),
    )
}

/// R82-S2: hint for ConsumerGenerator generator on TaskList (CC=2).
///
/// Fires when any task title/description contains orphan wiring / consumer bridge keywords.
pub(crate) fn maybe_consumer_hint_on_task_list(input: &Value) -> Option<String> {
    const CONSUMER_KEYWORDS: &[&str] = &[
        "consumer",
        "wire orphan",
        "orphan symbol",
        "wiring gap",
        "integration bridge",
        "connect module",
        "generate consumer",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, CONSUMER_KEYWORDS) {
        return None;
    }
    Some(
        " | consumer: wiring/consumer tasks detected — run `touring generate render ConsumerGenerator` \
        to scaffold a consumer integration bridge via touring-generator"
            .to_string(),
    )
}

/// R82-S3: hint for TaskScaffold generator on TaskList (CC=2).
///
/// Fires when any task title/description contains task DAG / TACO scaffold keywords.
pub(crate) fn maybe_task_scaffold_hint_on_task_list(input: &Value) -> Option<String> {
    const SCAFFOLD_KEYWORDS: &[&str] = &[
        "task scaffold",
        "taco task",
        "decompose task",
        "dag scaffold",
        "task dag",
        "new task dag",
        "scaffold task",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, SCAFFOLD_KEYWORDS) {
        return None;
    }
    Some(
        " | task-scaffold: task DAG tasks detected — run `touring generate render TaskScaffold` \
        to scaffold a TACO task DAG via touring-generator"
            .to_string(),
    )
}

/// R83-S1: hint for ManPage generator on TaskList (CC=2).
///
/// Fires when any task title/description contains man page / CLI documentation keywords.
pub(crate) fn maybe_man_page_hint_on_task_list(input: &Value) -> Option<String> {
    const MAN_KEYWORDS: &[&str] = &[
        "man page",
        "manual page",
        "man section",
        "manpage",
        "cli documentation",
        "command reference",
        "troff",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, MAN_KEYWORDS) {
        return None;
    }
    Some(
        " | man-page: documentation tasks detected — run `touring generate render ManPage` \
        to scaffold a man page via touring-generator"
            .to_string(),
    )
}

/// R83-S2: hint for IncrementalPatch generator on TaskList (CC=2).
///
/// Fires when any task title/description contains patch / diff / incremental update keywords.
pub(crate) fn maybe_incremental_patch_hint_on_task_list(input: &Value) -> Option<String> {
    const PATCH_KEYWORDS: &[&str] = &[
        "incremental patch",
        "patch file",
        "diff patch",
        "hotfix patch",
        "incremental update",
        "apply patch",
        "code patch",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, PATCH_KEYWORDS) {
        return None;
    }
    Some(
        " | incremental-patch: patch tasks detected — run `touring generate render IncrementalPatch` \
        to scaffold a patch file via touring-generator"
            .to_string(),
    )
}

/// R83-S3: hint for SkillDocument generator on TaskList (CC=2).
///
/// Fires when any task title/description contains Claude Code skill / skill.md keywords.
pub(crate) fn maybe_skill_document_hint_on_task_list(input: &Value) -> Option<String> {
    const SKILL_KEYWORDS: &[&str] = &[
        "skill document",
        "skill.md",
        "claude skill",
        "skill file",
        "code skill",
        "skill definition",
        "agent skill",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, SKILL_KEYWORDS) {
        return None;
    }
    Some(
        " | skill-document: skill tasks detected — run `touring generate render SkillDocument` \
        to scaffold a Claude Code skill document via touring-generator"
            .to_string(),
    )
}

/// R84-S1: When task list contains CLI handler tasks, suggest `cli_handler` generator (CC=2).
///
/// Bridges TaskList(cli/command tasks) → touring-generator CliHandler template.
/// Keywords: "cli handler", "command handler", "subcommand", "cli command", "clap handler", etc.
/// Closes the loop: TaskList(cli task) → cli_handler template → touring-generator commit.
pub(crate) fn maybe_cli_handler_hint_on_task_list(input: &Value) -> Option<String> {
    const CLI_KEYWORDS: &[&str] = &[
        "cli handler",
        "command handler",
        "subcommand",
        "cli command",
        "clap handler",
        "argument parser",
        "cli subcommand",
        "terminal command",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, CLI_KEYWORDS) {
        return None;
    }
    Some(
        " | cli-handler: CLI command tasks detected — run `touring generate render CliHandler` \
        to scaffold a CLI handler via touring-generator"
            .to_string(),
    )
}

/// R84-S2: When task list contains MCP tool tasks, suggest `mcp_tool` generator (CC=2).
///
/// Bridges TaskList(mcp/tool tasks) → touring-generator McpTool template.
/// Keywords: "mcp tool", "model context protocol", "mcp server", "tool handler", etc.
/// Closes the loop: TaskList(mcp task) → mcp_tool template → touring-generator commit.
pub(crate) fn maybe_mcp_tool_hint_on_task_list(input: &Value) -> Option<String> {
    const MCP_KEYWORDS: &[&str] = &[
        "mcp tool",
        "model context protocol",
        "mcp server",
        "tool handler",
        "mcp handler",
        "mcp endpoint",
        "mcp integration",
        "tool definition",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, MCP_KEYWORDS) {
        return None;
    }
    Some(
        " | mcp-tool: MCP tool tasks detected — run `touring generate render McpTool` \
        to scaffold an MCP tool via touring-generator"
            .to_string(),
    )
}

/// R84-S3: When task list contains hook handler tasks, suggest `hook_handler` generator (CC=2).
///
/// Bridges TaskList(hook tasks) → touring-generator HookHandler template.
/// Keywords: "hook handler", "lifecycle hook", "pre-edit hook", "post-read hook", etc.
/// Closes the loop: TaskList(hook task) → hook_handler template → touring-generator commit.
pub(crate) fn maybe_hook_handler_hint_on_task_list(input: &Value) -> Option<String> {
    const HOOK_KEYWORDS: &[&str] = &[
        "hook handler",
        "lifecycle hook",
        "pre-edit hook",
        "post-read hook",
        "claude hook",
        "hook event",
        "hook integration",
        "hook implementation",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, HOOK_KEYWORDS) {
        return None;
    }
    Some(
        " | hook-handler: hook tasks detected — run `touring generate render HookHandler` \
        to scaffold a hook handler via touring-generator"
            .to_string(),
    )
}

/// R85-S1: When task list contains plan/markdown tasks, suggest `plan_md` generator (CC=2).
///
/// Bridges TaskList(plan/spec tasks) → touring-generator PlanMd template.
/// Keywords: "plan.md", "markdown plan", "implementation plan", "feature plan", etc.
/// Closes the loop: TaskList(plan task) → plan_md template → touring-generator commit.
pub(crate) fn maybe_plan_md_hint_on_task_list(input: &Value) -> Option<String> {
    const PLAN_KEYWORDS: &[&str] = &[
        "plan.md",
        "markdown plan",
        "implementation plan",
        "feature plan",
        "task plan",
        "execution plan",
        "planning document",
        "spec document",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, PLAN_KEYWORDS) {
        return None;
    }
    Some(
        " | plan-md: planning tasks detected — run `touring generate render PlanMd` \
        to scaffold a Markdown plan document via touring-generator"
            .to_string(),
    )
}

/// R85-S2: When task list contains test/spec tasks, suggest `test` generator (CC=2).
///
/// Bridges TaskList(test tasks) → touring-generator Test template.
/// Keywords: "write tests", "unit test", "integration test", "test coverage", etc.
/// Closes the loop: TaskList(test task) → test template → touring-generator commit.
pub(crate) fn maybe_test_hint_on_task_list(input: &Value) -> Option<String> {
    const TEST_KEYWORDS: &[&str] = &[
        "write tests",
        "unit test",
        "integration test",
        "test coverage",
        "add tests",
        "test suite",
        "e2e test",
        "test scaffold",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, TEST_KEYWORDS) {
        return None;
    }
    Some(
        " | test: test tasks detected — run `touring generate render Test` \
        to scaffold a test file via touring-generator"
            .to_string(),
    )
}

/// R85-S3: When task list contains Python script tasks, suggest `python_script` generator (CC=2).
///
/// Bridges TaskList(python tasks) → touring-generator PythonScript template.
/// Keywords: "python script", "python module", "automation script", "python tool", etc.
/// Closes the loop: TaskList(python task) → python_script template → touring-generator commit.
pub(crate) fn maybe_python_script_hint_on_task_list(input: &Value) -> Option<String> {
    const PYTHON_KEYWORDS: &[&str] = &[
        "python script",
        "python module",
        "automation script",
        "python tool",
        "python automation",
        "write python",
        "python utility",
        "python helper",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, PYTHON_KEYWORDS) {
        return None;
    }
    Some(
        " | python-script: Python tasks detected — run `touring generate render PythonScript` \
        to scaffold a Python script via touring-generator"
            .to_string(),
    )
}

/// R86-S1: When task list contains FFI/binding tasks, suggest `ffi_binding` generator (CC=2).
///
/// Bridges TaskList(ffi/C interop tasks) → touring-generator FfiBinding template.
/// Keywords: "ffi binding", "c binding", "foreign function", "unsafe extern", etc.
/// Closes the loop: TaskList(ffi task) → ffi_binding template → touring-generator commit.
pub(crate) fn maybe_ffi_binding_hint_on_task_list(input: &Value) -> Option<String> {
    const FFI_KEYWORDS: &[&str] = &[
        "ffi binding",
        "c binding",
        "foreign function",
        "unsafe extern",
        "c interop",
        "ffi interface",
        "native binding",
        "cffi",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, FFI_KEYWORDS) {
        return None;
    }
    Some(
        " | ffi-binding: FFI tasks detected — run `touring generate render FfiBinding` \
        to scaffold a C/FFI binding via touring-generator"
            .to_string(),
    )
}

/// R86-S2: When task list contains shell completion tasks, suggest `shell_completion` generator (CC=2).
///
/// Bridges TaskList(completion tasks) → touring-generator ShellCompletion template.
/// Keywords: "shell completion", "bash completion", "zsh completion", "tab completion", etc.
/// Closes the loop: TaskList(completion task) → shell_completion template → touring-generator commit.
pub(crate) fn maybe_shell_completion_hint_on_task_list(input: &Value) -> Option<String> {
    const COMPLETION_KEYWORDS: &[&str] = &[
        "shell completion",
        "bash completion",
        "zsh completion",
        "tab completion",
        "autocomplete script",
        "fish completion",
        "cli autocomplete",
        "completions",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, COMPLETION_KEYWORDS) {
        return None;
    }
    Some(
        " | shell-completion: completion tasks detected — run `touring generate render ShellCompletion` \
        to scaffold shell completion scripts via touring-generator"
            .to_string(),
    )
}

/// R86-S3: When task list contains diary/journal tasks, suggest `diary_entry` generator (CC=2).
///
/// Bridges TaskList(diary/journal tasks) → touring-generator DiaryEntry template.
/// Keywords: "diary entry", "agent diary", "aaak entry", "write diary", etc.
/// Closes the loop: TaskList(diary task) → diary_entry template → touring-generator commit.
pub(crate) fn maybe_diary_entry_hint_on_task_list(input: &Value) -> Option<String> {
    const DIARY_KEYWORDS: &[&str] = &[
        "diary entry",
        "agent diary",
        "aaak entry",
        "write diary",
        "diary write",
        "touring diary",
        "diary log",
        "agent log entry",
    ];
    let tasks = extract_task_list(input)?;
    if !any_task_has_keyword(tasks, DIARY_KEYWORDS) {
        return None;
    }
    Some(
        " | diary-entry: diary tasks detected — run `touring generate render DiaryEntry` \
        to scaffold a diary entry via touring-generator"
            .to_string(),
    )
}

/// R31-S3: Suggest a GeneratorKind for the first in_progress CC task in the TaskList result (CC≤4).
///
/// Reads `tool_result.tasks` from the PostToolUse(TaskList) input, finds the first task
/// with `status == "in_progress"`, keyword-maps its subject to a GeneratorKind, and returns
/// a formatted generator hint. Complements `generator_for_first_ready_subtask` (which looks
/// at Touring DAG subtasks) by bridging the CC task perspective to the generator pipeline.
/// Returns `None` when no in_progress tasks exist or no keyword matches the subject.
pub(crate) fn generator_for_first_inprogress_cc_task(input: &Value) -> Option<String> {
    let tasks = extract_task_list(input)?;
    let first = tasks
        .iter()
        .find(|t| t.get("status").and_then(|s| s.as_str()) == Some("in_progress"))?;
    let subject = first
        .get("task_subject")
        .or_else(|| first.get("title"))
        .or_else(|| first.get("description"))
        .and_then(|s| s.as_str())
        .filter(|s| !s.is_empty())?;
    let hint = suggest_generator_for_task_subject(subject);
    if hint.is_empty() {
        return None;
    }
    Some(format!("active-cc-task{hint}"))
}

/// Format ready subtasks from pre-fetched `cli_decompose_ready` JSON response.
///
/// Accepts the raw JSON string to avoid a second DB round-trip — the caller owns
/// the single `cli_decompose_ready` call and passes the result to both this helper
/// and `code_symbols_for_active_tasks`. Returns empty string if no ready subtasks.
pub(crate) fn build_ready_subtasks_hint_from_json(ready_json: &str) -> String {
    if !ready_json.contains("\"ready_count\"") {
        return String::new();
    }
    let v = match serde_json::from_str::<serde_json::Value>(ready_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let count = v.get("ready_count").and_then(|c| c.as_u64()).unwrap_or(0);
    if count == 0 {
        return String::new();
    }
    // Collect first 3 subtask IDs for inline display
    let ids: Vec<String> = v
        .get("ready_subtasks")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .take(3)
                .filter_map(|s| {
                    s.get("subtask_id")
                        .and_then(|id| id.as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();

    if ids.is_empty() {
        format!(" | ready: {count} subtask(s) — run `touring decompose ready` to list")
    } else {
        format!(
            " | ready: {count} subtask(s) — start with: {}",
            ids.join(", ")
        )
    }
}

/// R21-S2: Cross-reference ready subtasks with BM25 symbol index (CC≤2).
///
/// Dispatches to `search_symbols_for_first_ready_task` when tantivy-fts is ON.
/// Returns empty string if Tantivy is unavailable, no ready subtasks, or no non-DAG hits.
pub(crate) fn code_symbols_for_active_tasks(ready_json: &str) -> String {
    #[cfg(feature = "tantivy-fts")]
    if let Some(hint) = search_symbols_for_first_ready_task(ready_json) {
        return hint;
    }
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = ready_json;
    String::new()
}

/// Inner BM25 search for the first ready subtask's linked code symbols (CC≤5).
///
/// Uses `?`-chain to propagate None at each fallible step — no panics, no unwraps.
/// Filters out DAG/task_output/task_completed docs; surfaces up to 2 code symbols.
#[cfg(feature = "tantivy-fts")]
pub(crate) fn search_symbols_for_first_ready_task(ready_json: &str) -> Option<String> {
    let v = serde_json::from_str::<serde_json::Value>(ready_json).ok()?;
    let first_id = v
        .get("ready_subtasks")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("subtask_id"))
        .and_then(|id| id.as_str())?;
    if first_id.is_empty() {
        return None;
    }
    let idx = crate::tantivy_index::global_tantivy()?;
    let hits = idx.search(first_id, 8).ok()?;
    let task_kinds = ["task_dag", "task_output", "task_completed"];
    let syms: Vec<&str> = hits
        .iter()
        .filter(|h| !task_kinds.contains(&h.symbol_kind.as_str()))
        .take(2)
        .map(|h| h.symbol_name.as_str())
        .collect();
    if syms.is_empty() {
        return None;
    }
    Some(format!(
        " | code-context: {} — `touring tantivy search \"{first_id}\"`",
        syms.join(", ")
    ))
}

/// R25-S3: Map the first ready subtask description to a generator kind hint (CC=3).
///
/// Extracts the `description` field of the first ready subtask from the
/// `cli_decompose_ready` JSON response, then delegates to
/// `suggest_generator_for_task_subject` to resolve the matching GeneratorKind.
/// Returns empty string when no ready subtasks exist or no keyword matches.
pub(crate) fn generator_for_first_ready_subtask(ready_json: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(ready_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let desc = v
        .get("ready_subtasks")
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or("");
    suggest_generator_for_task_subject(desc)
}

/// R21-S3: Upsert task completion marker to Tantivy FTS index (CC≤2).
///
/// Called from `handle_task_sync_post_update` on `status == "completed"`.
/// Makes completed tasks discoverable via `touring tantivy search "task_completed"`
/// or `touring tantivy search "<task_id>"`. Feature-gated: no-op when tantivy-fts OFF.
pub(crate) fn upsert_task_completion_to_tantivy(task_id: &str) {
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            let doc = crate::tantivy_index::SymbolDoc {
                symbol_name: format!("completed:{task_id}"),
                file_path: format!("task_lifecycle:{task_id}"),
                symbol_kind: "task_completed".to_string(),
                module_path: Some("task_completions".to_string()),
                docstring: Some(format!(
                    "Task {task_id} completed via Claude Code TaskUpdate"
                )),
                line_number: 0,
                language: "task".to_string(),
                visibility: None,
                crate_name: None,
                blake3_hash: None,
                import_count: None,
                export_count: None,
                cognitive_score: None,
                functional_signature: None,
                community_id: None,
            };
            let _ = idx.upsert_symbol(&doc);
            let _ = idx.commit();
            tracing::debug!(task_id, "task completion upserted to Tantivy");
        }
    }
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = task_id;
}

/// R22-S2: Upsert file_changed event into Tantivy FTS index (CC≤2).
///
/// Records each FileChanged event as a `file_changed` SymbolDoc so recent file
/// modifications become BM25-searchable via `touring tantivy search "file_changed"`
/// or `touring tantivy search "<stem>"`. Creates an audit trail of modifications.
/// Feature-gated: no-op when tantivy-fts OFF.
pub(super) fn upsert_file_changed_to_tantivy(rel_path: &str) {
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            let stem = file_stem(rel_path);
            let lang = rel_path.rsplit('.').next().unwrap_or("").to_string();
            let doc = crate::tantivy_index::SymbolDoc {
                symbol_name: format!("changed:{stem}"),
                file_path: rel_path.to_string(),
                symbol_kind: "file_changed".to_string(),
                module_path: Some("file_changes".to_string()),
                docstring: Some(format!("File changed: {rel_path}")),
                line_number: 0,
                language: lang,
                visibility: None,
                crate_name: None,
                blake3_hash: None,
                import_count: None,
                export_count: None,
                cognitive_score: None,
                functional_signature: None,
                community_id: None,
            };
            let _ = idx.upsert_symbol(&doc);
            let _ = idx.commit();
            tracing::debug!(rel_path, "file_changed upserted to Tantivy");
        }
    }
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = rel_path;
}

// ──────────────────────────────────────────────────────────────────────────────
// AcoPheromone §3: Heat-based task priority (wiring entropy proxy)
// ──────────────────────────────────────────────────────────────────────────────

/// Returns all tasks with `status == "pending"` from the TaskList payload.
fn all_pending_tasks(input: &Value) -> Vec<&Value> {
    let tool_result = input
        .get("tool_result")
        .or_else(|| input.get("result"))
        .unwrap_or(input);
    tool_result
        .get("tasks")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|t| t.get("status").and_then(|s| s.as_str()) == Some("pending"))
                .collect()
        })
        .unwrap_or_default()
}

/// Computes a heat score [0.0, 1.0] for a task based on wiring integration scores.
///
/// Matches task subject keywords against module file paths. Lower integration_score
/// = higher orphan density = higher refactoring risk = higher heat (priority).
/// Returns 0.75 (neutral) when no keyword matches a known module.
fn score_task_by_wiring_heat(subject: &str, modules_json: &str) -> f64 {
    let modules: Vec<serde_json::Value> = serde_json::from_str(modules_json).unwrap_or_default();
    if modules.is_empty() {
        return 0.75;
    }
    let keywords: Vec<String> = subject
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_lowercase())
        .collect();
    if keywords.is_empty() {
        return 0.75;
    }
    let mut best_score: f64 = 0.75;
    let mut matched = false;
    for module in &modules {
        let file_path = module
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let score = module
            .get("integration_score")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let file_lower = file_path.to_lowercase();
        if keywords.iter().any(|kw| file_lower.contains(kw.as_str()))
            && (!matched || score < best_score)
        {
            best_score = score;
            matched = true;
        }
    }
    best_score
}

/// Emits a `[TOURING HEAT]` hint ordering top pending tasks by wiring entropy.
///
/// Tasks whose subject keywords match low-integration modules get flagged with
/// higher heat — indicating more orphan symbols and refactoring risk. The hint
/// is suppressed when fewer than 2 tasks are pending or when score spread < 0.05.
pub(crate) fn heat_based_task_priority_hint(rt: &mut HookRuntime, input: &Value) -> String {
    let tasks = all_pending_tasks(input);
    if tasks.len() < 2 {
        return String::new();
    }
    let modules_json = crate::cli_handlers::cli_wiring_modules(rt, &serde_json::json!({}));
    let mut scored: Vec<(&Value, f64)> = tasks
        .iter()
        .take(5)
        .map(|t| {
            let subj = task_subject(t);
            let heat = score_task_by_wiring_heat(subj, &modules_json);
            (*t, heat)
        })
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let min_score = scored.first().map(|(_, s)| *s).unwrap_or(0.75);
    let max_score = scored.last().map(|(_, s)| *s).unwrap_or(0.75);
    if (max_score - min_score).abs() < 0.05 {
        return String::new();
    }
    let parts: Vec<String> = scored
        .iter()
        .take(3)
        .map(|(t, score)| {
            let subj = task_subject(t);
            let label = subj
                .split_whitespace()
                .take(4)
                .collect::<Vec<_>>()
                .join(" ");
            let risk = if *score < 0.40 {
                "high-orphan"
            } else if *score < 0.70 {
                "med"
            } else {
                "ok"
            };
            format!("\"{}\" [{} heat={:.2}]", label, risk, 1.0 - score)
        })
        .collect();
    format!(" | [TOURING HEAT] priority: {}", parts.join(" \u{2192} "))
}
