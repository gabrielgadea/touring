//! ExitPlanMode handler (D9).
//!
//! Contains `handle_exit_plan_mode` and its exclusive private helpers.
//! Co-located helpers shared with enter live in `hints.rs`.

use serde_json::Value;

use super::super::suggest_generator_for_task_subject;
use super::hints::{
    maybe_adr_hint_on_exit_plan, maybe_asyncapi_hint_on_exit_plan,
    maybe_benchmark_hint_on_exit_plan, maybe_changelog_hint_on_exit_plan,
    maybe_ci_workflow_hint_on_exit_plan, maybe_cli_handler_hint_on_exit_plan,
    maybe_consumer_generator_hint_on_exit_plan, maybe_derive_macro_hint_on_exit_plan,
    maybe_diary_entry_hint_on_exit_plan, maybe_dockerfile_hint_on_exit_plan,
    maybe_error_catalog_hint_on_exit_plan, maybe_ffi_binding_hint_on_exit_plan,
    maybe_fuzz_target_hint_on_exit_plan, maybe_hook_handler_hint_on_exit_plan,
    maybe_incremental_patch_hint_on_exit_plan, maybe_k8s_manifest_hint_on_exit_plan,
    maybe_man_page_hint_on_exit_plan, maybe_mcp_tool_hint_on_exit_plan,
    maybe_migration_hint_on_exit_plan, maybe_openapi_hint_on_exit_plan,
    maybe_plan_md_hint_on_exit_plan, maybe_protobuf_hint_on_exit_plan,
    maybe_python_script_hint_on_exit_plan, maybe_rust_module_hint_on_exit_plan,
    maybe_schema_hint_on_exit_plan, maybe_shell_completion_hint_on_exit_plan,
    maybe_skill_document_hint_on_exit_plan, maybe_task_scaffold_hint_on_exit_plan,
    maybe_terraform_hint_on_exit_plan, maybe_test_hint_on_exit_plan,
};
use crate::runtime::HookRuntime;

pub(crate) fn handle_exit_plan_mode(rt: &mut HookRuntime, input: &Value) -> String {
    let _ = rt
        .ctx
        .knowledge
        .record_access("__exit_plan_mode__", "lifecycle");

    // R12-S5: Query decompose ready subtasks directly — surface actionable work immediately.
    // ExitPlanMode (Claude Code) → cli_decompose_ready → SQLite → live ready subtasks in context.
    let ready_json = crate::cli_handlers::cli_decompose_ready(rt, &serde_json::json!({}));
    let ready_hint =
        if ready_json.contains("\"ready_count\":0") || !ready_json.contains("\"ready_count\"") {
            String::new()
        } else {
            format!(
                " | ready-subtasks: {}",
                &ready_json[..ready_json.len().min(150)]
            )
        };

    // R18-S3: Auto-assess the planning session to generate a quality score.
    // ExitPlanMode → cli_session_assess → RL reward injection + quality signal.
    // If a session_id was passed (e.g. the plan_task_id from EnterPlanMode), assess it directly.
    let assess_hint = assess_plan_session(rt, input);

    // R153: RL reward when plan session is successfully assessed — closes ExitPlanMode → RL loop.
    // Symmetric to R150 (EnterPlanMode +0.15): ExitPlanMode rewards +0.1 when assessment produces
    // a quality score, signaling that structured planning completed with measurable quality.
    if !assess_hint.is_empty() {
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "orchestrate",
                "reward_value": 0.1,
                "context": "exit_plan_mode:session_assessed",
            }),
        );
    }

    // R21-S1: Derive a concrete GeneratorKind hint from the plan intent.
    // When ExitPlanMode carries description/intent/objective, keyword-match to the nearest
    // GeneratorKind and emit a ready-to-copy `touring generate render <kind>` command.
    let intent = input
        .get("description")
        .or_else(|| input.get("intent"))
        .or_else(|| input.get("objective"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let generator_kind_hint = exit_plan_mode_generator_hint(intent);
    // R43-S2: Derive concrete plan filename from intent so the plan-submit command is copy-paste-ready.
    let plan_file = plan_file_from_intent(intent);
    let plan_file_arg = if plan_file.is_empty() {
        "<plan.json>".to_string()
    } else {
        plan_file
    };

    // R30-S2: Recall plan_task_id stored by EnterPlanMode and suggest closing the DAG entry.
    let plan_link = plan_session_link_hint(rt);
    // R35-S2: Surface plan-critique hint so Claude Code gets quality feedback before commit.
    let critique_hint = plan_critique_hint_on_exit(rt);
    // R37-S2: Auto-finalize the plan DAG — archives the task when all subtasks are complete.
    // ExitPlanMode → recall plan_session:current → cli_decompose_finalize → archived or pending hint.
    let finalize_plan = auto_finalize_plan_on_exit(rt);
    // R46-S2: Surface Tantivy symbol search so Claude Code finds existing symbols before generating.
    // Uses unwrap_or_default() — None maps to empty string, zero CC addition.
    let tantivy_exit_hint = maybe_tantivy_search_hint_on_exit(intent).unwrap_or_default();
    // R48-S3: Surface changelog_entry generator so planned changes are documented immediately.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let changelog_hint = maybe_changelog_hint_on_exit_plan(intent).unwrap_or_default();
    // R50-S3: When intent involves CLI/shell keywords, suggest shell_completion generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let shell_hint = maybe_shell_completion_hint_on_exit_plan(intent).unwrap_or_default();
    // R52-S3: When intent involves IaC/cloud keywords, suggest terraform_module generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let terraform_hint = maybe_terraform_hint_on_exit_plan(intent).unwrap_or_default();
    // R53-S3: When intent involves documentation keywords, suggest skill_document generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let skill_doc_hint = maybe_skill_document_hint_on_exit_plan(intent).unwrap_or_default();
    // R55-S3: When intent involves Touring hook keywords, suggest hook_handler generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let hook_handler_hint = maybe_hook_handler_hint_on_exit_plan(intent).unwrap_or_default();
    // R56-S3: When intent involves man page keywords, suggest man_page generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let man_page_hint = maybe_man_page_hint_on_exit_plan(intent).unwrap_or_default();
    // R57-S3: When intent involves async API keywords, suggest asyncapi_spec generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let asyncapi_exit_hint = maybe_asyncapi_hint_on_exit_plan(intent).unwrap_or_default();
    // R58-S3: When intent involves project planning keywords, suggest plan.md generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let plan_md_hint = maybe_plan_md_hint_on_exit_plan(intent).unwrap_or_default();
    // R60-S3: When intent involves error catalog design, suggest error_catalog generator.
    // Uses unwrap_or_default() — None maps to "" — zero CC addition.
    let error_catalog_exit_hint = maybe_error_catalog_hint_on_exit_plan(intent).unwrap_or_default();
    // R64-S1: When intent involves CI/CD automation, suggest ci_workflow generator.
    let ci_workflow_exit_hint = maybe_ci_workflow_hint_on_exit_plan(intent).unwrap_or_default();
    // R64-S2: When intent involves Docker/container builds, suggest dockerfile generator.
    let dockerfile_exit_hint = maybe_dockerfile_hint_on_exit_plan(intent).unwrap_or_default();
    // R64-S3: When intent involves Kubernetes orchestration, suggest k8s_manifest generator.
    let k8s_exit_hint = maybe_k8s_manifest_hint_on_exit_plan(intent).unwrap_or_default();
    // R97-S1..S3: rust_module/migration/protobuf — source/db/rpc planning exit hints.
    let rust_module_exit_hint = maybe_rust_module_hint_on_exit_plan(intent).unwrap_or_default();
    let migration_exit_hint = maybe_migration_hint_on_exit_plan(intent).unwrap_or_default();
    let protobuf_exit_hint = maybe_protobuf_hint_on_exit_plan(intent).unwrap_or_default();
    // R103-S1..S3: adr/task_scaffold/test — architecture/DAG/testing planning exit hints.
    let adr_exit_hint = maybe_adr_hint_on_exit_plan(intent).unwrap_or_default();
    let task_scaffold_exit_hint = maybe_task_scaffold_hint_on_exit_plan(intent).unwrap_or_default();
    let test_exit_hint = maybe_test_hint_on_exit_plan(intent).unwrap_or_default();
    // R106-S1..S3: openapi/consumer_generator/ffi_binding — API/event/native planning exit hints.
    let openapi_exit_hint = maybe_openapi_hint_on_exit_plan(intent).unwrap_or_default();
    let consumer_generator_exit_hint =
        maybe_consumer_generator_hint_on_exit_plan(intent).unwrap_or_default();
    let ffi_binding_exit_hint = maybe_ffi_binding_hint_on_exit_plan(intent).unwrap_or_default();
    // R109-S1..S3: python_script/benchmark/incremental_patch — scripting/perf/patch planning exit hints.
    let python_script_exit_hint = maybe_python_script_hint_on_exit_plan(intent).unwrap_or_default();
    let benchmark_exit_hint = maybe_benchmark_hint_on_exit_plan(intent).unwrap_or_default();
    let incremental_patch_exit_hint =
        maybe_incremental_patch_hint_on_exit_plan(intent).unwrap_or_default();
    // R112-S1..S3: cli_handler/mcp_tool/schema — CLI/MCP/data-schema planning exit hints. ExitPlanMode 27/30.
    let cli_handler_exit_hint = maybe_cli_handler_hint_on_exit_plan(intent).unwrap_or_default();
    let mcp_tool_exit_hint = maybe_mcp_tool_hint_on_exit_plan(intent).unwrap_or_default();
    let schema_exit_hint = maybe_schema_hint_on_exit_plan(intent).unwrap_or_default();
    // R117-S1..S3: diary_entry/fuzz_target/derive_macro — agent memory/fuzz/macro exit hints. ExitPlanMode 30/30 COMPLETE.
    let diary_entry_exit_hint = maybe_diary_entry_hint_on_exit_plan(intent).unwrap_or_default();
    let fuzz_target_exit_hint = maybe_fuzz_target_hint_on_exit_plan(intent).unwrap_or_default();
    let derive_macro_exit_hint = maybe_derive_macro_hint_on_exit_plan(intent).unwrap_or_default();

    // §4: Parallel subagent hint — when ready_count >= 2, suggest parallelizing subagents.
    // ExitPlanMode is the ideal moment to surface this: the plan is just finalized and the
    // ready subtasks represent independent work units the orchestrator can fan out.
    let parallel_hint = maybe_parallel_subagent_hint(&ready_json);

    // R142: Persist plan intent to memory for cross-session recall.
    // ExitPlanMode(intent) → memory store → `touring memory recall "last_plan_intent"`.
    // Enables future sessions to retrieve what was planned last without re-entering plan mode.
    // Complements R30-S2 (stores plan_task_id in EnterPlanMode) by storing the intent text itself.
    // Silent on empty intent — no stale entries from plan mode exits without context.
    if !intent.is_empty() {
        let intent_snippet = &intent[..intent.len().min(200)];
        let _ = crate::cli_handlers::cli_memory_store(
            rt,
            &serde_json::json!({
                "key": "last_plan_intent",
                "value": format!("Plan intent on exit: {intent_snippet}"),
                "tier": "semantic",
                "entry_type": "lesson",
            }),
        );
    }

    format!(
        "plan-mode exited: run `touring generate plan-submit --plan-file {plan_file_arg}` to commit planned artifacts{generator_kind_hint} | run `touring session checkpoint <id>` to persist planning state{ready_hint}{parallel_hint}{assess_hint}{plan_link}{critique_hint}{finalize_plan}{tantivy_exit_hint}{changelog_hint}{shell_hint}{terraform_hint}{skill_doc_hint}{hook_handler_hint}{man_page_hint}{asyncapi_exit_hint}{plan_md_hint}{error_catalog_exit_hint}{ci_workflow_exit_hint}{dockerfile_exit_hint}{k8s_exit_hint}{rust_module_exit_hint}{migration_exit_hint}{protobuf_exit_hint}{adr_exit_hint}{task_scaffold_exit_hint}{test_exit_hint}{openapi_exit_hint}{consumer_generator_exit_hint}{ffi_binding_exit_hint}{python_script_exit_hint}{benchmark_exit_hint}{incremental_patch_exit_hint}{cli_handler_exit_hint}{mcp_tool_exit_hint}{schema_exit_hint}{diary_entry_exit_hint}{fuzz_target_exit_hint}{derive_macro_exit_hint}"
    )
}

/// R21-S1: Derive a concrete GeneratorKind hint from ExitPlanMode intent (CC≤3).
///
/// Reuses `suggest_generator_for_task_subject` (R20-S3 keyword table) and reformats
/// the result for ExitPlanMode context — surfaces a parameterized `render` command
/// the user can copy-paste immediately after exiting plan mode.
/// Returns empty string if intent is empty or no keyword matches.
pub(crate) fn exit_plan_mode_generator_hint(intent: &str) -> String {
    if intent.is_empty() {
        return String::new();
    }
    let hint = suggest_generator_for_task_subject(intent);
    if hint.is_empty() {
        return String::new();
    }
    // hint format: " | generator: `touring generate render <Kind> ...` suggested"
    // Keep as-is — already fits the ExitPlanMode context string.
    hint
}

/// R46-S2: Surface Tantivy symbol search hint on ExitPlanMode (CC=2).
///
/// When ExitPlanMode carries an intent, surfaces `touring tantivy search "<keywords>"` so
/// Claude Code can discover existing symbols related to the planned work before committing
/// new artifacts. This bridges the ExitPlanMode event to the Tantivy BM25 index — ensuring
/// the generator knows what symbols already exist and can avoid reimplementing them.
/// Returns `None` when intent is empty or too short to produce a useful search query.
pub(crate) fn maybe_tantivy_search_hint_on_exit(intent: &str) -> Option<String> {
    if intent.len() < 3 {
        return None;
    }
    let keywords = &intent[..intent.len().min(50)];
    Some(format!(
        " | tantivy-search: run `touring tantivy search \"{keywords}\"` \
        to find existing symbols before committing new artifacts"
    ))
}

/// Assess the planning session on ExitPlanMode (R18-S3).
///
/// Calls `cli_session_assess` for the session referenced in the input (field `session_id`
/// or `task_id`). On success, surfaces the quality score inline. No-op if no session id
/// is present or if assessment returns no score field.
pub(crate) fn assess_plan_session(rt: &mut HookRuntime, input: &Value) -> String {
    // R168: Fall back to plan_session:current memory recall when no session_id in input.
    // ExitPlanMode inputs rarely carry session_id — they carry intent/description.
    // Without this fallback, assess_plan_session was always a no-op for plan sessions created
    // by EnterPlanMode (R13-S4), making R153's RL +0.1 reward unreachable.
    // Priority: input session_id > input task_id > memory-recalled plan_session:current.
    let explicit_id = input
        .get("session_id")
        .or_else(|| input.get("task_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session_id: String = if !explicit_id.is_empty() {
        explicit_id.to_owned()
    } else {
        // Recall plan_session:current stored by EnterPlanMode (R30-S2).
        let recall_payload = serde_json::json!({"query": "plan_session:current", "limit": 1});
        let recall = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
        assess_session_id_from_recall(&recall)
    };
    if session_id.is_empty() {
        return String::new();
    }
    let assess_payload = serde_json::json!({"session_id": &session_id});
    let result = crate::cli_handlers::cli_session_assess(rt, &assess_payload);
    assess_hint_from_session_result(&session_id, &result)
}

/// Pure logic: extract the session_id string from a `plan_session:current` memory recall JSON.
///
/// Separated from [`assess_plan_session`] so the parse step can be tested deterministically
/// without any I/O or daemon dependency. Returns empty string when the recall result is
/// empty, malformed JSON, or contains no `entries[0].value` field.
pub(crate) fn assess_session_id_from_recall(recall: &str) -> String {
    serde_json::from_str::<serde_json::Value>(recall)
        .ok()
        .and_then(|v| {
            v.get("entries")
                .and_then(|e| e.as_array())
                .and_then(|arr| arr.first())
                .and_then(|e| e.get("value"))
                .and_then(|val| val.as_str())
                .map(|s| s.to_owned())
        })
        .unwrap_or_default()
}

/// Pure logic: given a session_id and the JSON string returned by `cli_session_assess`,
/// produce the assess hint string or empty if no quality_score is present.
///
/// Separated from [`assess_plan_session`] so the formatting path can be tested
/// deterministically without any I/O or daemon dependency.
pub(crate) fn assess_hint_from_session_result(session_id: &str, result: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(result) {
        Ok(v) => {
            if let Some(score) = v.get("quality_score").and_then(|s| s.as_f64()) {
                format!(" | session-assessed: {session_id} quality={score:.2}")
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// R35-S2: Surface plan-critique hint when a plan session is active (CC≤3).
///
/// Recalls `plan_session:current` from memory (stored by EnterPlanMode via R30-S2).
/// When found, suggests `touring generate plan-critique --plan-file plan.json` so Claude Code
/// can get quality feedback before committing the plan. Returns empty string when no plan
/// task is active (fresh sessions or sessions without EnterPlanMode entry).
pub(crate) fn plan_critique_hint_on_exit(rt: &mut HookRuntime) -> String {
    let recall_payload = serde_json::json!({"query": "plan_session:current", "limit": 1});
    let recall = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
    plan_critique_hint_from_recall(&recall)
}

/// Pure logic: given the JSON string returned by a `plan_session:current` memory recall,
/// decide whether to surface the plan-critique hint.
///
/// Separated from [`plan_critique_hint_on_exit`] so the decision path can be tested
/// deterministically without any I/O or daemon dependency.
/// Returns empty string when the recall result does not contain a `"value"` field
/// (i.e. no plan session is currently active).
pub(crate) fn plan_critique_hint_from_recall(recall: &str) -> String {
    if !recall.contains("\"value\"") {
        return String::new();
    }
    " | plan-critique: run `touring generate plan-critique --plan-file plan.json` for quality feedback before committing".to_string()
}

/// R37-S2: Auto-finalize the plan DAG when ExitPlanMode fires and a session is active (CC≤5).
///
/// Recalls `plan_session:current` from memory (stored by EnterPlanMode via R30-S2), extracts
/// the task_id, then calls `cli_decompose_finalize`. When all subtasks are complete the DAG
/// is archived automatically — no manual `touring decompose finalize` needed. Returns empty
/// string when no plan session is active or finalize returns neither "archived" nor "ready:false".
pub(crate) fn auto_finalize_plan_on_exit(rt: &mut HookRuntime) -> String {
    let recall_payload = serde_json::json!({"query": "plan_session:current", "limit": 1});
    let recall = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
    let v = match serde_json::from_str::<serde_json::Value>(&recall) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let task_id = v
        .get("entries")
        .and_then(|e| e.as_array())
        .and_then(|arr| arr.first())
        .and_then(|e| e.get("value"))
        .and_then(|val| val.as_str())
        .unwrap_or("");
    if task_id.is_empty() {
        return String::new();
    }
    let finalize =
        crate::cli_handlers::cli_decompose_finalize(rt, &serde_json::json!({"task_id": task_id}));
    if finalize.contains("\"archived\":true") {
        format!(" | auto-finalized: DAG task {task_id} archived — all subtasks complete")
    } else if finalize.contains("\"ready\":false") {
        format!(
            " | dag-pending: task {task_id} has incomplete subtasks — run `touring decompose get {task_id}` to review"
        )
    } else {
        String::new()
    }
}

/// R30-S2: Recall the current plan session from memory and suggest DAG completion (CC≤4).
///
/// EnterPlanMode stores `plan_task_id` as `plan_session:current` in memory (R30-S2).
/// ExitPlanMode calls this helper to recall that ID and emit a `decompose update completed`
/// hint, closing the plan session in the Touring DAG without requiring manual CLI invocation.
/// Returns empty string when no plan session is stored or recall JSON is malformed.
pub(crate) fn plan_session_link_hint(rt: &mut HookRuntime) -> String {
    let recall_payload = serde_json::json!({"query": "plan_session:current", "limit": 1});
    let recall_result = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
    plan_session_link_hint_from_recall(&recall_result)
}

/// Pure logic: given the JSON string returned by a `plan_session:current` memory recall,
/// extract the stored plan_id and produce the DAG-completion hint string.
///
/// Separated from [`plan_session_link_hint`] so the parse-and-format path can be tested
/// deterministically without any I/O or daemon dependency.
/// Returns empty string when the recall result is empty, malformed JSON, or contains no
/// `entries[0].value` field.
pub(crate) fn plan_session_link_hint_from_recall(recall_result: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(recall_result) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let plan_id = v
        .get("entries")
        .and_then(|e| e.as_array())
        .and_then(|arr| arr.first())
        .and_then(|e| e.get("value"))
        .and_then(|val| val.as_str())
        .unwrap_or("");
    if plan_id.is_empty() {
        return String::new();
    }
    format!(
        " | plan-session: run `touring decompose update {plan_id} completed` to close DAG entry from EnterPlanMode"
    )
}

/// R43-S2: Derive a concrete plan filename from the ExitPlanMode intent (CC≤4).
///
/// Instead of the generic `<plan.json>` placeholder, derive a kebab-case filename
/// from the first 30 characters of the intent so Claude Code gets a copy-paste-ready
/// file name for `touring generate plan-submit --plan-file <derived-name>.json`.
///
/// Rules: alphanumeric chars are lowercased; non-alphanumeric become `-`; leading/trailing
/// dashes are stripped. Returns empty string when intent is blank or normalises to nothing.
pub(crate) fn plan_file_from_intent(intent: &str) -> String {
    if intent.is_empty() {
        return String::new();
    }
    let stem: String = intent
        .chars()
        .take(30)
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches('-').to_string();
    if stem.is_empty() {
        return String::new();
    }
    format!("plan-{stem}.json")
}

/// §4: Emit a `[TOURING PARALLEL]` hint when the decompose DAG has ≥ 2 ready subtasks.
///
/// ExitPlanMode is the ideal moment to surface this hint: the plan just closed and the
/// ready subtasks represent independent work units that can be fanned out to parallel
/// subagents, reducing wall-clock time. Returns empty string when ready_count < 2.
pub(crate) fn maybe_parallel_subagent_hint(ready_json: &str) -> String {
    let parsed = match serde_json::from_str::<serde_json::Value>(ready_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let ready_count = parsed
        .get("ready_count")
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    if ready_count < 2 {
        return String::new();
    }
    let ids: Vec<&str> = parsed
        .get("ready_subtasks")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .take(4)
                .filter_map(|s| s.get("subtask_id").and_then(|v| v.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let id_hint = if ids.is_empty() {
        String::new()
    } else {
        format!(" (subtasks: {})", ids.join(", "))
    };
    format!(
        " | [TOURING PARALLEL] {ready_count} independent subtasks ready{id_hint} \
        \u{2014} parallelize: spawn subagents per subtask to reduce wall-clock time"
    )
}
