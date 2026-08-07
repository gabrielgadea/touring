//! `task-sync-post-get` hook handler + 38 co-located hint helpers.
//!
//! Mirrors Claude Code's TaskGet to the Touring decompose DAG. Every helper
//! in this file is exclusive to `handle_task_sync_post_get` — all 30+
//! `maybe_*_hint_on_task_get` helpers, `finalize_hint_if_dag_complete`,
//! `maybe_validate_phase_hint`, `maybe_mcts_unblock_on_no_ready`,
//! `missing_dag_entry_creation_hint`, `maybe_implement_vgp_hint`,
//! `scout_tantivy_search_hint`, `dag_json_to_active_description`, and
//! `generator_for_active_subtask`.
//!
//! Co-location rationale: these helpers share the same "TaskGet intent →
//! generator kind hint" design and change together; moving them to
//! `lifecycle/shared.rs` would bloat the cross-cutting API surface.
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D4. All helpers are
//! `pub(crate)` so the inline tests in `lifecycle::tests` continue to reach
//! them via `super::<helper>` after `pub(crate) use task_get::*` re-export.
//!
//! Includes R169: RL -0.1 penalty + hint when DAG reveals a failed subtask.

use serde_json::Value;

use crate::runtime::HookRuntime;

// Pull in shared helpers used by this handler.
use super::{plan_scaffold_for_subject, suggest_generator_for_task_subject};

pub(crate) fn handle_task_sync_post_get(rt: &mut HookRuntime, input: &Value) -> String {
    let tool_input = input.get("tool_input").unwrap_or(input);
    let task_id = tool_input
        .get("task_id")
        .or_else(|| tool_input.get("taskId"))
        .or_else(|| input.get("task_id"))
        .or_else(|| input.get("taskId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // R13-S3: Query cli_decompose_get directly — inject live DAG state into Claude Code context.
    // TaskGet (Claude Code) → cli_decompose_get → SQLite → both CC task + Touring DAG state surfaced.
    let dag_payload = serde_json::json!({"task_id": task_id});
    let dag_state = crate::cli_handlers::cli_decompose_get(rt, &dag_payload);
    let dag_context = if dag_state.contains("\"status\"") {
        format!(" [live: {}]", &dag_state[..dag_state.len().min(180)])
    } else {
        String::new()
    };

    // R19-S2: Search Tantivy for all indexed docs associated with this task_id.
    // Surfaces task_dag subtasks, task_output captures, and plan_session docs in a single view.
    // Makes TaskGet a complete knowledge aggregator across CC + Touring + Tantivy.
    let tantivy_hint = search_tantivy_for_task(&rt.project_root, task_id);

    // R23-S1: Detect active (in_progress) subtask from live DAG and suggest matching generator.
    // TaskGet → dag_state JSON → first in_progress subtask description → keyword → GeneratorKind hint.
    let gen_hint = generator_for_active_subtask(&dag_state);

    // R26-S3: When all subtasks are completed, surface `decompose finalize` hint automatically.
    // Closes the DAG lifecycle: scout→implement→validate→finalize, without requiring manual CLI check.
    let finalize_hint = finalize_hint_if_dag_complete(&dag_state, task_id);

    // R32-S1: Emit a ready-to-paste GeneratorPlan stub for the active in_progress subtask.
    // TaskGet → dag_state JSON → first in_progress subtask description → plan stub.
    let scaffold_hint = plan_scaffold_for_active_subtask(&dag_state, task_id)
        .map(|s| format!(" | {s}"))
        .unwrap_or_default();

    // R33-S2: When no DAG entry exists for this CC task, surface a creation command.
    let no_dag_hint = missing_dag_entry_creation_hint(&dag_state, task_id);

    // R37-S1: When ::scout subtask is the next actionable step, emit Tantivy search hint.
    // TaskGet → dag_state → ::scout pending/in_progress → concrete tantivy + ast blast commands.
    let scout_hint = scout_tantivy_search_hint(&dag_state, task_id);
    // R40-S3: When ::implement subtask is pending (scout done), surface VGP verify before coding.
    // Closes the scout→implement gap: TaskGet shows implement ready → VGP verify hint inline.
    let vgp_hint = maybe_implement_vgp_hint(&dag_state, task_id);
    // R42-S1: When ::validate is pending and ::implement done, surface concrete validation commands.
    // Closes the scout→implement→validate chain: implement done → cargo test + wiring + decompose update.
    let validate_hint = maybe_validate_phase_hint(&dag_state, task_id);
    // R45-S2: When DAG is stuck (pending subtasks but none ready), suggest MCTS unblock path.
    // Closes the gap: TaskGet reveals blocked DAG → MCTS finds alternative execution route.
    let mcts_unblock = maybe_mcts_unblock_on_no_ready(&dag_state, task_id);
    // R61-S2: When DAG description contains DB migration keywords, surface migration template hint.
    // Closes TaskGet → generator pipeline for migration artifacts.
    let migration_hint = maybe_migration_hint_on_task_get(&dag_state).unwrap_or_default();
    // R71-S1..S3: TaskGet → changelog/k8s/consumer generator hints from DAG description keywords.
    let changelog_hint = maybe_changelog_hint_on_task_get(&dag_state).unwrap_or_default();
    let k8s_hint = maybe_k8s_hint_on_task_get(&dag_state).unwrap_or_default();
    let consumer_hint = maybe_consumer_hint_on_task_get(&dag_state).unwrap_or_default();
    // R76-S1..S3: TaskGet → rust_module/mcp_tool/schema generator hints from DAG description keywords.
    let rust_module_get_hint = maybe_rust_module_hint_on_task_get(&dag_state).unwrap_or_default();
    let mcp_tool_get_hint = maybe_mcp_tool_hint_on_task_get(&dag_state).unwrap_or_default();
    let schema_get_hint = maybe_schema_hint_on_task_get(&dag_state).unwrap_or_default();
    // R87-S1..S3: TaskGet → openapi/adr/changelog_entry generator hints from DAG description keywords.
    let openapi_get_hint = maybe_openapi_hint_on_task_get(&dag_state).unwrap_or_default();
    let adr_get_hint = maybe_adr_hint_on_task_get(&dag_state).unwrap_or_default();
    let changelog_entry_get_hint =
        maybe_changelog_entry_hint_on_task_get(&dag_state).unwrap_or_default();
    // R88-S1..S3: TaskGet → terraform/ci_workflow/dockerfile generator hints from DAG description keywords.
    let terraform_get_hint = maybe_terraform_hint_on_task_get(&dag_state).unwrap_or_default();
    let ci_workflow_get_hint = maybe_ci_workflow_hint_on_task_get(&dag_state).unwrap_or_default();
    let dockerfile_get_hint = maybe_dockerfile_hint_on_task_get(&dag_state).unwrap_or_default();
    // R89-S1..S3: TaskGet → benchmark/fuzz_target/derive_macro generator hints.
    let benchmark_get_hint = maybe_benchmark_hint_on_task_get(&dag_state).unwrap_or_default();
    let fuzz_target_get_hint = maybe_fuzz_target_hint_on_task_get(&dag_state).unwrap_or_default();
    let derive_macro_get_hint = maybe_derive_macro_hint_on_task_get(&dag_state).unwrap_or_default();
    // R90-S1..S3: TaskGet → cli_handler/hook_handler/plan_md generator hints.
    let cli_handler_get_hint = maybe_cli_handler_hint_on_task_get(&dag_state).unwrap_or_default();
    let hook_handler_get_hint = maybe_hook_handler_hint_on_task_get(&dag_state).unwrap_or_default();
    let plan_md_get_hint = maybe_plan_md_hint_on_task_get(&dag_state).unwrap_or_default();
    // R91-S1..S3: TaskGet → test/python_script/shell_completion generator hints.
    let test_get_hint = maybe_test_hint_on_task_get(&dag_state).unwrap_or_default();
    let python_script_get_hint =
        maybe_python_script_hint_on_task_get(&dag_state).unwrap_or_default();
    let shell_completion_get_hint =
        maybe_shell_completion_hint_on_task_get(&dag_state).unwrap_or_default();
    // R92-S1..S3: TaskGet → man_page/error_catalog/incremental_patch generator hints.
    let man_page_get_hint = maybe_man_page_hint_on_task_get(&dag_state).unwrap_or_default();
    let error_catalog_get_hint =
        maybe_error_catalog_hint_on_task_get(&dag_state).unwrap_or_default();
    let incremental_patch_get_hint =
        maybe_incremental_patch_hint_on_task_get(&dag_state).unwrap_or_default();
    // R93-S1..S3: TaskGet → skill_document/diary_entry/asyncapi_spec generator hints.
    let skill_document_get_hint =
        maybe_skill_document_hint_on_task_get(&dag_state).unwrap_or_default();
    let diary_entry_get_hint = maybe_diary_entry_hint_on_task_get(&dag_state).unwrap_or_default();
    let asyncapi_spec_get_hint =
        maybe_asyncapi_spec_hint_on_task_get(&dag_state).unwrap_or_default();
    // R94-S1..S3: TaskGet → ffi_binding/protobuf_schema/task_scaffold generator hints (30/30 COMPLETE).
    let ffi_binding_get_hint = maybe_ffi_binding_hint_on_task_get(&dag_state).unwrap_or_default();
    let protobuf_schema_get_hint =
        maybe_protobuf_schema_hint_on_task_get(&dag_state).unwrap_or_default();
    let task_scaffold_get_hint =
        maybe_task_scaffold_hint_on_task_get(&dag_state).unwrap_or_default();

    // R141: Persist task-specific DAG snapshot to memory for cross-session recall.
    // TaskGet(task_id) → dag_state → memory snapshot → `touring memory recall "dag_state:<task_id>"`.
    // Enables future sessions to see this task's last-known DAG state without re-querying.
    // Only stores when dag_state contains a real status field — avoids persisting empty/error responses.
    if dag_state.contains("\"status\"") {
        let dag_snippet = &dag_state[..dag_state.len().min(300)];
        let _ = crate::cli_handlers::cli_memory_store(
            rt,
            &serde_json::json!({
                "key": format!("dag_state:{task_id}"),
                "value": format!("DAG state for {task_id}: {dag_snippet}"),
                "tier": "semantic",
                "entry_type": "lesson",
            }),
        );
    }

    // R146: Surface wiring chains hint when DAG has subtasks — finds functional chains
    // between modules for this task. Closes TaskGet → wiring chains → integration analysis loop.
    // Only fires when dag_state contains subtasks array (avoids empty-DAG noise).
    // `touring wiring chains` reveals how modules are connected across the codebase,
    // guiding the engineer to the right files before implementing the task.
    let wiring_chains_hint = if dag_state.contains("\"subtasks\"") {
        let task_stem = &task_id[..task_id.len().min(30)];
        format!(
            " | chains: run `touring wiring chains` to map functional chains relevant to task {task_stem}"
        )
    } else {
        String::new()
    };

    // R149: Surface plan-recall hint when task is active (in_progress or pending).
    // TaskGet → DAG status → GeneratorPlan registry lookup — closes the loop where
    // a developer polls a live task but has no pointer to historical generator plans.
    // `touring generate plan-recall --query "task:<task_id>"` searches the plan registry
    // for past GeneratorPlans tagged to this exact task, enabling rapid plan replay or
    // incremental refinement without starting the planning pipeline from scratch.
    // Silent on terminal statuses (completed/cancelled/failed) to avoid stale-plan noise.
    let plan_recall_get_hint = if dag_state.contains("\"status\":\"in_progress\"")
        || dag_state.contains("\"status\":\"pending\"")
    {
        let task_stem = &task_id[..task_id.len().min(40)];
        format!(
            " | plan-recall: run `touring generate plan-recall --query \"task:{task_stem}\"` \
            to find historical GeneratorPlans for this task"
        )
    } else {
        String::new()
    };

    // R154: RL reward when TaskGet polls an active task — closes the monitoring → RL feedback loop.
    // Small +0.05 signal per active-task polling cycle: cumulative rewards incentivise
    // structured task monitoring over ad-hoc status queries. Gated on the same condition
    // as R149 (in_progress OR pending) to avoid rewarding terminal-state polling.
    if dag_state.contains("\"status\":\"in_progress\"")
        || dag_state.contains("\"status\":\"pending\"")
    {
        let context = format!(
            "task_get:active_monitoring:{}",
            &task_id[..task_id.len().min(30)]
        );
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "orchestrate",
                "reward_value": 0.05,
                "context": context,
            }),
        );
    }
    // R162: RL +0.2 when DAG fully complete detected on TaskGet — closes the completion → RL loop.
    // finalize_hint_if_dag_complete (R26-S3) returns non-empty when ALL subtasks are completed.
    // This larger reward (+0.2 vs R154's +0.05/active) reinforces that full DAG lifecycle completion
    // is a stronger positive signal than active-task monitoring — incentivizing the engine to guide
    // Claude Code toward closing complete scout→implement→validate→finalize cycles.
    // Mutual exclusion with R154: a completed DAG cannot simultaneously be in_progress/pending.
    if !finalize_hint.is_empty() {
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "orchestrate",
                "reward_value": 0.2,
                "context": format!("task_get:dag_complete:{}", &task_id[..task_id.len().min(30)]),
            }),
        );
    }
    // R169: RL -0.1 penalty when TaskGet reveals a failed DAG — closes TaskGet(failed) → RL loop.
    // R166 injects RL -0.1 at output time (TaskOutput failure signal). But when TaskGet polls
    // an already-failed task (e.g., after context compaction), there was no penalty signal.
    // The discovery of a failed task should reinforce investigation/recovery behavior.
    // Mutually exclusive with R154 (in_progress/pending) and R162 (all completed) by design.
    // Surface lesson recall so the engineer can find prior failure patterns for this task.
    let failed_dag_hint = if dag_state.contains("\"status\":\"failed\"") {
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "orchestrate",
                "reward_value": -0.1,
                "context": format!("task_get:failed_dag:{}", &task_id[..task_id.len().min(30)]),
            }),
        );
        format!(
            " | ✗ failed subtask detected — run `touring memory recall \"task:{task_id}:output-failed\"` \
            for past failure lessons | retry: update {task_id}::validate to pending and re-run"
        )
    } else {
        String::new()
    };

    format!(
        "touring-sync: run `touring decompose get {task_id}` for DAG state{dag_context}{tantivy_hint}{gen_hint}{finalize_hint}{scaffold_hint}{no_dag_hint}{scout_hint}{vgp_hint}{validate_hint}{mcts_unblock}{migration_hint}{changelog_hint}{k8s_hint}{consumer_hint}{rust_module_get_hint}{mcp_tool_get_hint}{schema_get_hint}{openapi_get_hint}{adr_get_hint}{changelog_entry_get_hint}{terraform_get_hint}{ci_workflow_get_hint}{dockerfile_get_hint}{benchmark_get_hint}{fuzz_target_get_hint}{derive_macro_get_hint}{cli_handler_get_hint}{hook_handler_get_hint}{plan_md_get_hint}{test_get_hint}{python_script_get_hint}{shell_completion_get_hint}{man_page_get_hint}{error_catalog_get_hint}{incremental_patch_get_hint}{skill_document_get_hint}{diary_entry_get_hint}{asyncapi_spec_get_hint}{ffi_binding_get_hint}{protobuf_schema_get_hint}{task_scaffold_get_hint}{wiring_chains_hint}{plan_recall_get_hint}{failed_dag_hint} | \
        run `touring wiring suggest <file>` for integration opportunities related to this task"
    )
}

/// R26-S3: Return a finalize hint when all DAG subtasks are completed (CC=4).
///
/// Parses the `cli_decompose_get` response JSON, counts subtasks by status,
/// and returns a `decompose finalize` command hint when every subtask has
/// `"status": "completed"` and at least one subtask exists.
/// Returns empty string when subtasks are still in progress or JSON is unavailable.
pub(crate) fn finalize_hint_if_dag_complete(dag_json: &str, task_id: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(dag_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let subtasks = match v.get("subtasks").and_then(|s| s.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return String::new(),
    };
    let all_completed = subtasks
        .iter()
        .all(|s| s.get("status").and_then(|st| st.as_str()) == Some("completed"));
    if all_completed {
        format!(
            " | DAG complete: run `touring decompose finalize {task_id}` to archive and inject RL reward"
        )
    } else {
        String::new()
    }
}

/// R42-S1: Predicate — true when subtask id ends with `::validate` and status is `pending`.
pub(crate) fn is_validate_pending(s: &serde_json::Value) -> bool {
    s.get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .ends_with("::validate")
        && s.get("status").and_then(|st| st.as_str()).unwrap_or("") == "pending"
}

/// R42-S1: Predicate — true when subtask id ends with `::implement` and status is `completed`.
pub(crate) fn is_implement_completed(s: &serde_json::Value) -> bool {
    s.get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .ends_with("::implement")
        && s.get("status").and_then(|st| st.as_str()).unwrap_or("") == "completed"
}

/// R42-S1: When ::validate is pending and ::implement is completed, surface concrete validation commands (CC≤5).
///
/// Closes the scout→implement→validate chain: after implement finishes, Claude Code sees
/// exactly what to run next — cargo test, wiring orphan check, then decompose update.
/// Predicates extracted to `is_validate_pending` and `is_implement_completed` to keep CC≤15.
/// Returns empty string when the condition is not met or DAG JSON is unavailable.
pub(crate) fn maybe_validate_phase_hint(dag_json: &str, task_id: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(dag_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let subtasks = match v.get("subtasks").and_then(|s| s.as_array()) {
        Some(arr) => arr,
        None => return String::new(),
    };
    if subtasks.iter().any(is_validate_pending) && subtasks.iter().any(is_implement_completed) {
        format!(
            " | validate-ready: run `cargo test -p <crate>` + `touring wiring orphans -j` then \
            `touring decompose update {task_id}::validate completed` when tests pass"
        )
    } else {
        String::new()
    }
}

/// R45-S2 predicate: true when subtask status is "pending" (CC=1).
pub(crate) fn is_subtask_pending(s: &serde_json::Value) -> bool {
    s.get("status").and_then(|st| st.as_str()) == Some("pending")
}

/// R45-S2 predicate: true when subtask is pending AND has no declared dependencies (CC=2).
///
/// Approximates "ready" status when the daemon does not provide an explicit ready_count.
pub(crate) fn is_pending_no_deps(s: &serde_json::Value) -> bool {
    is_subtask_pending(s)
        && s.get("depends_on")
            .and_then(|d| d.as_array())
            .map(|d| d.is_empty())
            .unwrap_or(true)
}

/// R45-S2: Suggest MCTS unblock when DAG has pending subtasks but none are ready (CC≤5).
///
/// Parses `dag_json` from `cli_decompose_get`. When at least one subtask is `pending`
/// but zero are actionable (all deps still unmet — the "stuck DAG" condition), emits
/// `touring mcts search "unblock <task_id>"` so Claude Code gets an alternative
/// execution path. Returns empty when dag is unavailable, all tasks complete, or
/// at least one ready subtask exists.
pub(crate) fn maybe_mcts_unblock_on_no_ready(dag_json: &str, task_id: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(dag_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let subtasks = match v.get("subtasks").and_then(|s| s.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return String::new(),
    };
    if !subtasks.iter().any(is_subtask_pending) {
        return String::new();
    }
    // Honour server-provided ready_count; fall back to counting no-dep pending subtasks.
    let ready_count = v
        .get("ready_count")
        .and_then(|n| n.as_u64())
        .unwrap_or_else(|| subtasks.iter().filter(|s| is_pending_no_deps(s)).count() as u64);
    if ready_count > 0 {
        return String::new();
    }
    format!(
        " | mcts-unblock: no ready subtasks — \
        run `touring mcts search \"unblock {task_id}\"` for alternative execution path"
    )
}

/// R61-S2: Emit migration scaffold hint when DAG JSON describes a DB migration task (CC=2).
///
/// Scans the raw `cli_decompose_get` response for migration-related keywords.
/// When found, surfaces `touring generate render migration` so the engineer can scaffold
/// the migration file directly from the TaskGet context.
/// Uses static keyword table + `find()` for zero if/else CC overhead.
pub(crate) fn maybe_migration_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "migration",
        "migrate",
        "schema change",
        "db migration",
        "database migration",
        "alter table",
        "create table",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | migration: run `touring generate render migration` to scaffold DB migration file"
            .to_owned()
    })
}

/// R71-S1: Suggest `changelog_entry` generator when DAG description mentions release/version patterns (CC=2).
///
/// When TaskGet's DAG content references changelog, release notes, version bump, or semver topics,
/// surfaces `touring generate render ChangelogEntry` so the engineer can scaffold a changelog
/// entry immediately from the task context. Closes the TaskGet → changelog generator loop.
pub(crate) fn maybe_changelog_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "changelog",
        "release notes",
        "version bump",
        "semver",
        "breaking change",
        "release candidate",
        "release entry",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | changelog: run `touring generate render ChangelogEntry` to scaffold a changelog entry for this release".to_owned()
    })
}

/// R71-S2: Suggest `k8s_manifest` generator when DAG description mentions Kubernetes patterns (CC=2).
///
/// When TaskGet's DAG content references kubernetes, k8s, helm, deployment, or pod topics,
/// surfaces `touring generate render K8sManifest` so the engineer can scaffold a manifest
/// immediately from the task context. Closes the TaskGet → k8s_manifest generator loop.
pub(crate) fn maybe_k8s_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "kubernetes",
        "k8s",
        "helm",
        "deployment",
        "pod",
        "container orchestration",
        "ingress",
        "statefulset",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | k8s: run `touring generate render K8sManifest` to scaffold a Kubernetes manifest for this task".to_owned()
    })
}

/// R71-S3: Suggest `consumer_generator` when DAG description mentions wiring/integration patterns (CC=2).
///
/// When TaskGet's DAG content references consumer wiring, orphan symbols, or integration gaps,
/// surfaces `touring generate render ConsumerGenerator` so the engineer can scaffold a consumer
/// immediately from the task context. Closes the TaskGet → consumer_generator loop.
pub(crate) fn maybe_consumer_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "consumer",
        "wire orphan",
        "orphan symbol",
        "wiring gap",
        "generate consumer",
        "integration bridge",
        "connect module",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | consumer: run `touring generate render ConsumerGenerator` to scaffold a consumer integration bridge".to_owned()
    })
}

/// R76-S1: hint for RustModule generator on TaskGet (CC=2).
///
/// Fires when the DAG JSON contains keywords suggesting a Rust module should be scaffolded.
pub(crate) fn maybe_rust_module_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "rust module",
        "new crate",
        "rust library",
        "rust struct",
        "impl block",
        "rust trait",
        "cargo module",
        "new module",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | rust-module: run `touring generate render RustModule` to scaffold a Rust module via touring-generator".to_owned()
    })
}

/// R76-S2: hint for McpTool generator on TaskGet (CC=2).
///
/// Fires when the DAG JSON contains keywords suggesting an MCP tool should be scaffolded.
pub(crate) fn maybe_mcp_tool_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "mcp tool",
        "mcp server",
        "tool handler",
        "rmcp",
        "model context protocol",
        "touring mcp",
        "new mcp",
        "expose tool",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | mcp-tool: run `touring generate render McpTool` to scaffold an MCP tool via touring-generator".to_owned()
    })
}

/// R76-S3: hint for Schema generator on TaskGet (CC=2).
///
/// Fires when the DAG JSON contains keywords suggesting a JSON schema or data schema
/// definition should be scaffolded.
pub(crate) fn maybe_schema_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "json schema",
        "data schema",
        "schema definition",
        "serde schema",
        "openrpc schema",
        "api schema",
        "type schema",
        "validation schema",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | schema: run `touring generate render Schema` to scaffold a schema definition via touring-generator".to_owned()
    })
}

/// R87-S1: When DAG description mentions OpenAPI/Swagger, suggest `openapi_spec` generator (CC=2).
///
/// Bridges TaskGet(openapi/swagger dag description) → touring-generator OpenApiSpec template.
pub(crate) fn maybe_openapi_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "openapi",
        "swagger",
        "rest api spec",
        "api specification",
        "oas3",
        "api contract",
        "rest specification",
        "http api spec",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | openapi: run `touring generate render OpenApiSpec` to scaffold an OpenAPI spec via touring-generator".to_owned()
    })
}

/// R87-S2: When DAG description mentions ADR/architecture decision, suggest `adr` generator (CC=2).
///
/// Bridges TaskGet(adr/decision dag description) → touring-generator Adr template.
pub(crate) fn maybe_adr_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "architecture decision",
        "adr",
        "design decision",
        "architectural record",
        "decision record",
        "madr",
        "adl",
        "system design decision",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | adr: run `touring generate render Adr` to scaffold an Architecture Decision Record via touring-generator".to_owned()
    })
}

/// R87-S3: When DAG description mentions changelog/release notes, suggest `changelog_entry` generator (CC=2).
///
/// Bridges TaskGet(changelog dag description) → touring-generator ChangelogEntry template.
pub(crate) fn maybe_changelog_entry_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "changelog entry",
        "release note",
        "version bump",
        "semver bump",
        "release changelog",
        "breaking change entry",
        "release entry",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | changelog-entry: run `touring generate render ChangelogEntry` to scaffold a changelog entry via touring-generator".to_owned()
    })
}

/// R88-S1: When DAG description mentions Terraform/IaC, suggest `terraform_module` generator (CC=2).
pub(crate) fn maybe_terraform_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "terraform",
        "infrastructure as code",
        "iac module",
        "tofu module",
        "tf module",
        "opentofu",
        "terraform resource",
        "infra provisioning",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | terraform: run `touring generate render TerraformModule` to scaffold a Terraform module via touring-generator".to_owned()
    })
}

/// R88-S2: When DAG description mentions CI/CD pipeline, suggest `ci_workflow` generator (CC=2).
pub(crate) fn maybe_ci_workflow_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "ci workflow",
        "github actions",
        "ci/cd",
        "pipeline workflow",
        "continuous integration",
        "ci pipeline",
        "cd pipeline",
        "workflow yaml",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | ci-workflow: run `touring generate render CiWorkflow` to scaffold a CI workflow via touring-generator".to_owned()
    })
}

/// R88-S3: When DAG description mentions Docker/containers, suggest `dockerfile` generator (CC=2).
pub(crate) fn maybe_dockerfile_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "dockerfile",
        "docker image",
        "container image",
        "containerize",
        "docker build",
        "docker container",
        "multi-stage build",
        "docker layer",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | dockerfile: run `touring generate render Dockerfile` to scaffold a Dockerfile via touring-generator".to_owned()
    })
}

/// R89-S1: When DAG description mentions benchmarks/perf, suggest `benchmark` generator (CC=2).
pub(crate) fn maybe_benchmark_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "benchmark",
        "criterion",
        "performance test",
        "latency benchmark",
        "throughput test",
        "microbenchmark",
        "perf regression",
        "bench target",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | benchmark: run `touring generate render Benchmark` to scaffold a criterion benchmark via touring-generator".to_owned()
    })
}

/// R89-S2: When DAG description mentions fuzzing, suggest `fuzz_target` generator (CC=2).
pub(crate) fn maybe_fuzz_target_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "fuzz target",
        "cargo fuzz",
        "fuzzing",
        "fuzz test",
        "afl fuzz",
        "libfuzzer",
        "property test fuzz",
        "fuzz corpus",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | fuzz-target: run `touring generate render FuzzTarget` to scaffold a fuzz target via touring-generator".to_owned()
    })
}

/// R89-S3: When DAG description mentions derive macros, suggest `derive_macro` generator (CC=2).
pub(crate) fn maybe_derive_macro_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "derive macro",
        "proc macro",
        "custom derive",
        "attribute macro",
        "derive trait",
        "procedural macro",
        "macro derive",
        "syn proc-macro",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | derive-macro: run `touring generate render DeriveMacro` to scaffold a derive macro via touring-generator".to_owned()
    })
}

/// R90-S1: When DAG description mentions CLI handlers, suggest `cli_handler` generator (CC=2).
pub(crate) fn maybe_cli_handler_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "cli handler",
        "command handler",
        "subcommand",
        "cli command",
        "clap handler",
        "argument parser",
        "cli routing",
        "command dispatch",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | cli-handler: run `touring generate render CliHandler` to scaffold a CLI handler via touring-generator".to_owned()
    })
}

/// R90-S2: When DAG description mentions hook handlers, suggest `hook_handler` generator (CC=2).
pub(crate) fn maybe_hook_handler_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "hook handler",
        "lifecycle hook",
        "pre-edit hook",
        "post-read hook",
        "claude hook",
        "hook event",
        "hook integration",
        "hook implementation",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | hook-handler: run `touring generate render HookHandler` to scaffold a hook handler via touring-generator".to_owned()
    })
}

/// R90-S3: When DAG description mentions planning docs, suggest `plan_md` generator (CC=2).
pub(crate) fn maybe_plan_md_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "plan.md",
        "markdown plan",
        "implementation plan",
        "feature plan",
        "task plan",
        "execution plan",
        "planning document",
        "spec document",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | plan-md: run `touring generate render PlanMd` to scaffold a Markdown plan via touring-generator".to_owned()
    })
}

/// R91-S1: When DAG description mentions tests/specs, suggest `test` generator (CC=2).
pub(crate) fn maybe_test_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "write tests",
        "unit test",
        "integration test",
        "test coverage",
        "add tests",
        "test suite",
        "e2e test",
        "test scaffold",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | test: run `touring generate render Test` to scaffold a test file via touring-generator".to_owned()
    })
}

/// R91-S2: When DAG description mentions Python scripts, suggest `python_script` generator (CC=2).
pub(crate) fn maybe_python_script_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "python script",
        "python module",
        "automation script",
        "python tool",
        "python automation",
        "write python",
        "python utility",
        "python helper",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | python-script: run `touring generate render PythonScript` to scaffold a Python script via touring-generator".to_owned()
    })
}

/// R91-S3: When DAG description mentions shell completions, suggest `shell_completion` generator (CC=2).
pub(crate) fn maybe_shell_completion_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "shell completion",
        "bash completion",
        "zsh completion",
        "tab completion",
        "autocomplete script",
        "fish completion",
        "cli autocomplete",
        "completions",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | shell-completion: run `touring generate render ShellCompletion` to scaffold shell completions via touring-generator".to_owned()
    })
}

/// R92-S1: When DAG description mentions man pages, suggest `man_page` generator (CC=2).
pub(crate) fn maybe_man_page_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "man page",
        "manpage",
        "linux man",
        "unix man",
        "manual page",
        "man section",
        "groff manual",
        "documentation page",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | man-page: run `touring generate render ManPage` to scaffold a man page via touring-generator".to_owned()
    })
}

/// R92-S2: When DAG description mentions error catalogs, suggest `error_catalog` generator (CC=2).
pub(crate) fn maybe_error_catalog_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "error catalog",
        "error codes",
        "error registry",
        "error taxonomy",
        "error catalogue",
        "error enum",
        "error definitions",
        "error reference",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | error-catalog: run `touring generate render ErrorCatalog` to scaffold an error catalog via touring-generator".to_owned()
    })
}

/// R92-S3: When DAG description mentions incremental patches, suggest `incremental_patch` generator (CC=2).
pub(crate) fn maybe_incremental_patch_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "incremental patch",
        "apply patch",
        "patch file",
        "schema patch",
        "code patch",
        "incremental update",
        "patch strategy",
        "delta patch",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | incremental-patch: run `touring generate render IncrementalPatch` to scaffold an incremental patch via touring-generator".to_owned()
    })
}

/// R93-S1: When DAG description mentions skill documents, suggest `skill_document` generator (CC=2).
pub(crate) fn maybe_skill_document_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "skill document",
        "skill file",
        "skill definition",
        "skill scaffold",
        "skill template",
        "agent skill",
        "claude skill",
        "skill spec",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | skill-document: run `touring generate render SkillDocument` to scaffold a skill document via touring-generator".to_owned()
    })
}

/// R93-S2: When DAG description mentions diary entries, suggest `diary_entry` generator (CC=2).
pub(crate) fn maybe_diary_entry_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "diary entry",
        "agent diary",
        "session diary",
        "diary log",
        "aaak entry",
        "agent memory diary",
        "diary write",
        "diary record",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | diary-entry: run `touring generate render DiaryEntry` to scaffold a diary entry via touring-generator".to_owned()
    })
}

/// R93-S3: When DAG description mentions AsyncAPI specs, suggest `asyncapi_spec` generator (CC=2).
pub(crate) fn maybe_asyncapi_spec_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "asyncapi",
        "async api",
        "event-driven api",
        "message broker spec",
        "pubsub spec",
        "kafka schema",
        "amqp spec",
        "event schema",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | asyncapi-spec: run `touring generate render AsyncApiSpec` to scaffold an AsyncAPI spec via touring-generator".to_owned()
    })
}

/// R94-S1: When DAG description mentions FFI bindings, suggest `ffi_binding` generator (CC=2).
pub(crate) fn maybe_ffi_binding_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "ffi binding",
        "foreign function",
        "c bindings",
        "unsafe extern",
        "bindgen",
        "ffi wrapper",
        "native binding",
        "c interop",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | ffi-binding: run `touring generate render FfiBinding` to scaffold an FFI binding via touring-generator".to_owned()
    })
}

/// R94-S2: When DAG description mentions protobuf schemas, suggest `protobuf_schema` generator (CC=2).
pub(crate) fn maybe_protobuf_schema_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "protobuf",
        "proto schema",
        "grpc proto",
        "protocol buffer",
        ".proto file",
        "proto definition",
        "grpc schema",
        "proto message",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | protobuf-schema: run `touring generate render ProtobufSchema` to scaffold a protobuf schema via touring-generator".to_owned()
    })
}

/// R94-S3: When DAG description mentions task scaffolds, suggest `task_scaffold` generator (CC=2).
pub(crate) fn maybe_task_scaffold_hint_on_task_get(dag_json: &str) -> Option<String> {
    const KEYWORDS: &[&str] = &[
        "task scaffold",
        "taco scaffold",
        "decompose scaffold",
        "dag scaffold",
        "task template",
        "subtask scaffold",
        "task framework",
        "task boilerplate",
    ];
    let lower = dag_json.to_lowercase();
    KEYWORDS.iter().find(|kw| lower.contains(*kw)).map(|_| {
        " | task-scaffold: run `touring generate render TaskScaffold` to scaffold a TACO task via touring-generator".to_owned()
    })
}

/// R33-S2: When a CC task has no Touring DAG entry, surface a creation hint (CC=2).
///
/// `handle_task_sync_post_get` calls `cli_decompose_get` and stores the result in `dag_state`.
/// If `dag_state` doesn't contain `"status"` the task has no DAG entry yet.
/// This helper returns a `touring decompose create` command so the engineer can wire the
/// CC task into the Touring decompose system in one copy-paste action.
///
/// Returns empty string when a DAG entry already exists (dag_state contains "status").
pub(crate) fn missing_dag_entry_creation_hint(dag_state: &str, task_id: &str) -> String {
    if dag_state.contains("\"status\"") {
        return String::new();
    }
    format!(
        " | no-dag: run `touring decompose create intent \"{task_id}\"` to wire this CC task into the Touring DAG"
    )
}

/// R32-S1: Emit a ready-to-paste GeneratorPlan stub for the active in_progress subtask (CC=4).
///
/// Parses the `cli_decompose_get` response JSON, finds the first subtask with
/// `"status": "in_progress"`, extracts its `description`, then delegates to
/// `plan_scaffold_for_subject`. Returns `None` when no active subtask exists,
/// when the description contains no recognizable keyword, or when JSON parsing fails.
pub(crate) fn plan_scaffold_for_active_subtask(dag_json: &str, task_id: &str) -> Option<String> {
    let v = serde_json::from_str::<serde_json::Value>(dag_json).ok()?;
    let subtasks = v.get("subtasks").and_then(|s| s.as_array())?;
    let active = subtasks
        .iter()
        .find(|s| s.get("status").and_then(|st| st.as_str()) == Some("in_progress"))?;
    let description = active
        .get("description")
        .and_then(|d| d.as_str())
        .filter(|d| !d.is_empty())?;
    plan_scaffold_for_subject(description, task_id)
}

/// R40-S3: Surface VGP verify hint when the ::implement subtask becomes pending (CC≤4).
///
/// When the Touring DAG shows `::implement` subtask as `pending` (meaning scout completed),
/// the next step is VGP verification before writing code. This hint closes the gap between
/// "scout done" and "start coding" by surfacing the exact VGP command to verify symbols first.
/// Returns empty string when ::implement is not pending or DAG JSON is unavailable.
pub(crate) fn maybe_implement_vgp_hint(dag_json: &str, task_id: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(dag_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let subtasks = match v.get("subtasks").and_then(|s| s.as_array()) {
        Some(arr) => arr,
        None => return String::new(),
    };
    let implement_pending = subtasks.iter().any(|s| {
        let id = s.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let status = s.get("status").and_then(|st| st.as_str()).unwrap_or("");
        id.ends_with("::implement") && status == "pending"
    });
    if implement_pending {
        format!(
            " | vgp-ready: `touring generate verify --symbol <Symbol>` + \
            `touring index find <Symbol>` before coding {task_id}::implement"
        )
    } else {
        String::new()
    }
}

/// R37-S1: Emit Tantivy-backed scout hint when the ::scout subtask is actionable (CC≤4).
///
/// Parses the decompose DAG JSON from `cli_decompose_get`, checks whether the subtask
/// whose `id` ends with `::scout` has status `"pending"` or `"in_progress"`, and surfaces
/// a concrete `touring tantivy search + touring ast blast` command pair for Claude Code.
/// Returns empty string when the scout subtask does not exist or has already completed.
pub(crate) fn scout_tantivy_search_hint(dag_json: &str, task_id: &str) -> String {
    let v = match serde_json::from_str::<serde_json::Value>(dag_json) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let subtasks = match v.get("subtasks").and_then(|s| s.as_array()) {
        Some(arr) => arr,
        None => return String::new(),
    };
    let scout_active = subtasks.iter().any(|s| {
        let id = s.get("id").and_then(|i| i.as_str()).unwrap_or("");
        let status = s.get("status").and_then(|st| st.as_str()).unwrap_or("");
        id.ends_with("::scout") && matches!(status, "pending" | "in_progress")
    });
    if scout_active {
        format!(
            " | scout-ready: run `touring tantivy search \"{task_id}\"` + \
            `touring ast blast <file>` to research context before implementation"
        )
    } else {
        String::new()
    }
}

/// Search Tantivy for documents related to a task_id (R19-S2).
///
/// Queries the `file_path` field for `task_dag:<task_id>`, `task_output:<task_id>`,
/// and `plan_session:<task_id>` to aggregate all indexed knowledge for the task.
/// Returns a formatted hint string; empty string when Tantivy is disabled or no hits.
/// A raiz do projeto acompanha a operação: o store de decompose já é
/// per-project (`locate_task_store`), então o espelho no Tantivy segue a
/// fonte da verdade em vez de cair no índice legado compartilhado.
pub(crate) fn search_tantivy_for_task(project_root: &std::path::Path, task_id: &str) -> String {
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::tantivy_for(Some(project_root)) {
            let query = format!("task:{task_id}");
            if let Ok(hits) = idx.search(&query, 10)
                && !hits.is_empty()
            {
                let mut kinds = std::collections::BTreeSet::new();
                for h in &hits {
                    kinds.insert(h.symbol_kind.as_str());
                }
                let kinds_str = kinds.into_iter().collect::<Vec<_>>().join(", ");
                return format!(
                    " | tantivy: {} doc(s) found ({kinds_str}) — `touring tantivy search \"{task_id}\"`",
                    hits.len()
                );
            }
        }
    }
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = task_id;
    String::new()
}

/// Extract the description of the first in-progress subtask from a decompose DAG JSON (R23-S1).
///
/// Parses `dag_json` (output of `cli_decompose_get`), finds the first subtask whose
/// `status` field is `"in_progress"`, and returns its `description`. Returns `None`
/// when the JSON has no `subtasks` array or none are currently active.
pub(crate) fn dag_json_to_active_description(dag_json: &str) -> Option<String> {
    let v = serde_json::from_str::<serde_json::Value>(dag_json).ok()?;
    let subtasks = v.get("subtasks").and_then(|s| s.as_array())?;
    subtasks
        .iter()
        .find(|s| s.get("status").and_then(|st| st.as_str()) == Some("in_progress"))
        .and_then(|s| s.get("description"))
        .and_then(|d| d.as_str())
        .map(str::to_owned)
}

/// Suggest a generator kind for the currently active (in_progress) DAG subtask (R23-S1).
///
/// Calls `dag_json_to_active_description` to extract the first in_progress subtask
/// description, then keyword-maps it to a `GeneratorKind` via `suggest_generator_for_task_subject`.
/// Returns empty string when no in_progress subtask is found or no keyword matches.
pub(crate) fn generator_for_active_subtask(dag_json: &str) -> String {
    match dag_json_to_active_description(dag_json) {
        Some(desc) => suggest_generator_for_task_subject(&desc),
        None => String::new(),
    }
}
