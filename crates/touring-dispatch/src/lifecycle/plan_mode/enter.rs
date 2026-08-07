//! EnterPlanMode handler (D9).
//!
//! Contains `handle_enter_plan_mode` and its exclusive private helpers.
//! Co-located helpers shared with exit live in `hints.rs`.

use std::time::Duration;

use serde_json::Value;

use super::super::{plan_scaffold_for_subject, suggest_generator_for_task_subject};
use super::hints::{
    maybe_adr_hint_on_enter_plan, maybe_asyncapi_hint_on_enter_plan,
    maybe_benchmark_hint_on_enter_plan, maybe_changelog_hint_on_enter_plan,
    maybe_ci_workflow_hint_on_enter_plan, maybe_cli_handler_hint_on_enter_plan,
    maybe_derive_macro_hint_on_enter_plan, maybe_diary_entry_hint_on_enter_plan,
    maybe_dockerfile_hint_on_enter_plan, maybe_error_catalog_hint_on_enter_plan,
    maybe_ffi_binding_hint_on_enter_plan, maybe_fuzz_target_hint_on_enter_plan,
    maybe_hook_handler_hint_on_enter_plan, maybe_k8s_hint_on_enter_plan,
    maybe_man_page_hint_on_enter_plan, maybe_mcp_tool_hint_on_enter_plan,
    maybe_migration_hint_on_enter_plan, maybe_openapi_hint_on_enter_plan,
    maybe_plan_md_hint_on_enter_plan, maybe_protobuf_hint_on_enter_plan,
    maybe_python_script_hint_on_enter_plan, maybe_rust_module_hint_on_enter_plan,
    maybe_schema_hint_on_enter_plan, maybe_shell_completion_hint_on_enter_plan,
    maybe_skill_document_hint_on_enter_plan, maybe_task_scaffold_hint_on_enter_plan,
    maybe_terraform_hint_on_enter_plan, upsert_plan_session_to_tantivy,
};
use crate::runtime::HookRuntime;
use crate::shared::shadow_rollout::run_shadow_rollout;

pub(crate) fn handle_enter_plan_mode(rt: &mut HookRuntime, input: &Value) -> String {
    // P4 (2026-04-13): consume pending plan_mode suggestion when CC enters plan mode.
    // If the caller provides a `suggestion_ref`, mark it consumed so the rate-limit
    // gate allows fresh suggestions on the next session start.
    consume_plan_mode_suggestion_if_present(rt, input);

    let intent = input
        .get("description")
        .or_else(|| input.get("objective"))
        .or_else(|| input.get("intent"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut hints = Vec::new();

    // R39-S2: Recall past session memory for context task before plan creation.
    // Extraction happens inside the helper to avoid adding CC to this function.
    // `extend(Option)` uses Option's IntoIterator — zero CC addition.
    hints.extend(task_id_session_recall_hint(rt, input));

    if !intent.is_empty() {
        let truncated = &intent[..intent.len().min(100)];

        // R24-S2: Surface concrete GeneratorKind BEFORE the generic plan-suggest hint.
        // Keyword-matches the intent to the nearest GeneratorKind via SUBJECT_KEYWORD_MAP,
        // giving a ready-to-copy `touring generate render <Kind>` command with no ambiguity.
        let kind_hint = suggest_generator_for_task_subject(intent);
        if !kind_hint.is_empty() {
            hints.push(kind_hint.trim_start_matches(" | ").to_string());
        }

        hints.push(format!(
            "generator: run `touring generate plan-suggest --intent \"{truncated}\"` to scaffold code artifacts for this plan"
        ));

        // R13-S4: Auto-create decompose DAG entry directly — no manual step needed.
        // EnterPlanMode(intent) → cli_decompose_create → SQLite: plan session tracked from entry.
        let decompose_payload = serde_json::json!({
            "task_type": "plan_session",
            "description": &intent[..intent.len().min(200)],
        });
        let decompose_result = crate::cli_handlers::cli_decompose_create(rt, &decompose_payload);
        let plan_task_id = serde_json::from_str::<serde_json::Value>(&decompose_result)
            .ok()
            .and_then(|v| {
                v.get("task_id")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_owned())
            })
            .unwrap_or_default();
        if !plan_task_id.is_empty() {
            hints.push(format!(
                "decompose: plan session auto-registered as {plan_task_id} | verify: `touring decompose get {plan_task_id}`"
            ));

            // R18-S2: Index the plan session in Tantivy for future discovery.
            upsert_plan_session_to_tantivy(&rt.project_root, &plan_task_id, intent);
            // R30-S2: Persist plan_task_id to memory → ExitPlanMode recalls + closes DAG entry.
            let _ = crate::cli_handlers::cli_memory_store(
                rt,
                &serde_json::json!({
                    "key": "plan_session:current",
                    "value": &plan_task_id,
                    "tier": "semantic",
                    "entry_type": "lesson",
                }),
            );
            // R167: Auto-start a Touring session for the plan session DAG entry.
            // EnterPlanMode creates a decompose DAG task (R13-S4) but never starts a session.
            // TaskUpdate(in_progress) starts one via R38-S1 — plan sessions are first-class and
            // deserve the same lifecycle treatment. cli_session_start is idempotent (upsert).
            // Closes the EnterPlanMode → session lifecycle gap: plan sessions now have
            // session_start → session_checkpoint (on exit assess) → session_assess (R18-S3).
            {
                let _ = crate::cli_handlers::cli_session_start(
                    rt,
                    &serde_json::json!({
                        "session_id": &plan_task_id,
                        "task_type": "plan_session",
                        "objective": &intent[..intent.len().min(200)],
                    }),
                );
            }
            // R31-S1: Emit a ready-to-paste GeneratorPlan stub for immediate submission.
            // EnterPlanMode now surfaces the same concrete stub as TaskCreate (R30-S1).
            if let Some(scaffold) = plan_scaffold_for_subject(intent, &plan_task_id) {
                hints.push(scaffold);
            }
        } else {
            hints.push(format!(
                "decompose: run `touring decompose create intent \"{truncated}\"` to track this planning session in the DAG"
            ));
        }

        // R14-S2: Surface past memory patterns for the intent via cli_memory_recall.
        // Extracted to helper to keep handle_enter_plan_mode CC ≤ 15 (R24-S2 refactor).
        if let Some(recall_hint) = memory_recall_hint_for_intent(rt, truncated) {
            hints.push(recall_hint);
        }
        // R46-S1: Search plan registry for reusable GeneratorPlans before creating a new one.
        // `extend(Option)` — zero CC addition to handle_enter_plan_mode.
        hints.extend(maybe_plan_recall_hint_for_intent(truncated));
        // R48-S2: When intent involves architectural decisions, suggest ADR generator.
        // `extend(Option)` — zero CC addition to handle_enter_plan_mode.
        hints.extend(maybe_adr_hint_on_enter_plan(truncated));
        // R49-S3: When intent involves async/event/messaging patterns, suggest asyncapi_spec generator.
        // `extend(Option)` — zero CC addition to handle_enter_plan_mode.
        hints.extend(maybe_asyncapi_hint_on_enter_plan(truncated));
        // R51-S3: When intent involves error/exception handling, suggest error_catalog generator.
        // `extend(Option)` — zero CC addition to handle_enter_plan_mode.
        hints.extend(maybe_error_catalog_hint_on_enter_plan(truncated));
        // R54-S3: When intent involves DAG/decompose keywords, suggest task_scaffold generator.
        // `extend(Option)` — zero CC addition to handle_enter_plan_mode.
        hints.extend(maybe_task_scaffold_hint_on_enter_plan(truncated));
        // R63-S1: When intent involves CI/CD, suggest ci_workflow generator.
        hints.extend(maybe_ci_workflow_hint_on_enter_plan(truncated));
        // R63-S2: When intent involves containers/docker, suggest dockerfile generator.
        hints.extend(maybe_dockerfile_hint_on_enter_plan(truncated));
        // R63-S3: When intent involves IaC/terraform, suggest terraform_module generator.
        hints.extend(maybe_terraform_hint_on_enter_plan(truncated));
        // R96-S1..S3: rust_module/migration/protobuf — source/db/rpc intent hints.
        hints.extend(maybe_rust_module_hint_on_enter_plan(truncated));
        hints.extend(maybe_migration_hint_on_enter_plan(truncated));
        hints.extend(maybe_protobuf_hint_on_enter_plan(truncated));
        // R102-S1..S3: k8s_manifest/openapi/shell_completion — container/API/CLI intent hints.
        hints.extend(maybe_k8s_hint_on_enter_plan(truncated));
        hints.extend(maybe_openapi_hint_on_enter_plan(truncated));
        hints.extend(maybe_shell_completion_hint_on_enter_plan(truncated));
        // R105-S1..S3: man_page/changelog/skill_document — docs/release/skill intent hints.
        hints.extend(maybe_man_page_hint_on_enter_plan(truncated));
        hints.extend(maybe_changelog_hint_on_enter_plan(truncated));
        hints.extend(maybe_skill_document_hint_on_enter_plan(truncated));
        // R108-S1..S3: ffi_binding/python_script/benchmark — native/scripting/perf intent hints.
        hints.extend(maybe_ffi_binding_hint_on_enter_plan(truncated));
        hints.extend(maybe_python_script_hint_on_enter_plan(truncated));
        hints.extend(maybe_benchmark_hint_on_enter_plan(truncated));
        // R111-S1..S3: fuzz_target/derive_macro/diary_entry — fuzzing/macro/diary intent hints.
        hints.extend(maybe_fuzz_target_hint_on_enter_plan(truncated));
        hints.extend(maybe_derive_macro_hint_on_enter_plan(truncated));
        hints.extend(maybe_diary_entry_hint_on_enter_plan(truncated));
        // R115-S1..S3: cli_handler/mcp_tool/hook_handler — CLI/MCP/hook intent hints. EnterPlanMode 28/30.
        hints.extend(maybe_cli_handler_hint_on_enter_plan(truncated));
        hints.extend(maybe_mcp_tool_hint_on_enter_plan(truncated));
        hints.extend(maybe_hook_handler_hint_on_enter_plan(truncated));
        // R116-S1..S2: plan_md/schema — project-plan/data-schema intent hints. EnterPlanMode 30/30 COMPLETE.
        hints.extend(maybe_plan_md_hint_on_enter_plan(truncated));
        hints.extend(maybe_schema_hint_on_enter_plan(truncated));
        // R150: RL reward when entering plan mode with specific intent — closes EnterPlanMode → RL loop.
        // Rewards the "plan before code" pattern (CLAUDE.md principle #7) with +0.15 in the RL engine.
        // Only fires for non-empty intent — empty-intent entries (accidental mode switches) get no reward.
        // Reinforces structured planning over ad-hoc coding in the orchestration reward model.
        {
            let context = format!("enter_plan_mode:{}", &truncated[..truncated.len().min(40)]);
            let _ = crate::cli_handlers::cli_learning_reward(
                rt,
                &serde_json::json!({
                    "tool_name": "orchestrate",
                    "reward_value": 0.15,
                    "context": context,
                }),
            );
        }
    } else {
        hints.push(
            "generator: run `touring generate plan-suggest --intent \"<intent>\"` to scaffold artifacts before planning".to_string(),
        );
    }

    // R28-S3: Surface active DAG ready-subtasks before pushing wiring/memory hints.
    // If the Touring DAG already has work queued, prioritize it over creating a new plan.
    if let Some(dag_hint) = active_dag_ready_hint(rt) {
        hints.push(dag_hint);
    }

    // R35-S3: Surface top orphan symbols as concrete planning candidates.
    // `extend(Option)` uses Option's IntoIterator — zero CC addition vs if-let.
    hints.extend(top_orphans_for_plan_hint(rt));

    hints.push(
        "wiring: run `touring wiring orphans -j` to discover symbols needing consumers before you plan".to_string(),
    );
    hints.push(
        "memory: run `touring memory recall \"<topic>\"` to surface past patterns relevant to this plan".to_string(),
    );

    // R38-S3: Check evolution drift before planning — structural degradation needs resolution first.
    // `extend(Option)` uses Option's IntoIterator — zero CC addition.
    hints.extend(evolution_drift_hint_on_enter_plan(rt));
    // R42-S3: Emit AAAK diary write hint — records planning intent for cross-session recall.
    // `extend(Option)` pattern — zero CC addition to handle_enter_plan_mode.
    hints.extend(maybe_diary_write_on_plan(intent));

    // D4 — MCTS shadow rollout for CILA >= 3 (complex/architectural plans).
    // Non-blocking: join_timeout of 200ms. If the rollout doesn't finish in time,
    // we continue without the hint — plan-mode entry is never gated.
    hints.extend(mcts_shadow_rollout_hint(rt, input));

    hints.join(" | ")
}

/// Check for already-ready DAG subtasks before plan creation (R28-S3 helper, CC≤4).
///
/// Calls `cli_decompose_ready` (no filter) and returns a formatted advisory hint when
/// there are pending subtasks whose dependencies are all completed. Returns `None` when
/// the DAG is empty or no subtask is actionable — avoids noise on fresh sessions.
///
/// Wired into `handle_enter_plan_mode` so EnterPlanMode tells Claude Code "there's already
/// work queued in the DAG" before it auto-creates a new plan session that would duplicate effort.
pub(crate) fn active_dag_ready_hint(rt: &mut HookRuntime) -> Option<String> {
    let result = crate::cli_handlers::cli_decompose_ready(rt, &serde_json::json!({}));
    let v = serde_json::from_str::<serde_json::Value>(&result).ok()?;
    let count = v.get("ready_count").and_then(|c| c.as_u64()).unwrap_or(0);
    if count == 0 {
        return None;
    }
    Some(format!(
        "dag-sync: {count} subtask(s) already actionable — run `touring decompose ready` to see them before creating a new plan"
    ))
}

/// R39-S2: Recall past session memory for a context task before plan creation starts (CC≤3).
///
/// When EnterPlanMode input includes a `task_id`, retrieves any past session lessons stored
/// under `task:<id>` so Claude Code has relevant context before committing to a new plan.
/// Returns `None` when no `task_id` is present or no memory entry exists — avoids noise on
/// fresh sessions that have no prior history.
/// Takes `input: &Value` directly to keep extraction inside the helper (CC savings in caller).
pub(crate) fn task_id_session_recall_hint(rt: &mut HookRuntime, input: &Value) -> Option<String> {
    let task_id = input.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
    if task_id.is_empty() {
        return None;
    }
    let recall_payload = serde_json::json!({"query": format!("task:{task_id}"), "limit": 1});
    let recall = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
    if recall.contains("\"value\"") {
        Some(format!(
            "context-recall: past session found for task {task_id} — run \
            `touring memory recall \"task:{task_id}\"` before planning to avoid duplication"
        ))
    } else {
        None
    }
}

/// R35-S3: Surface top orphan pub symbols as concrete planning candidates (CC≤4).
///
/// Calls `cli_wiring_orphans` and extracts up to 3 top orphan names so Claude Code gets
/// concrete integration targets during EnterPlanMode instead of a generic reminder.
/// Returns `None` when the orphan list is empty or JSON parse fails.
pub(crate) fn top_orphans_for_plan_hint(rt: &mut HookRuntime) -> Option<String> {
    let orphans_json = crate::cli_handlers::cli_wiring_orphans(rt, &serde_json::json!({}));
    let v = serde_json::from_str::<serde_json::Value>(&orphans_json).ok()?;
    let orphans = v.get("orphans").and_then(|o| o.as_array())?;
    let top: Vec<&str> = orphans
        .iter()
        .take(3)
        .filter_map(|o| o.get("name").and_then(|n| n.as_str()))
        .collect();
    if top.is_empty() {
        return None;
    }
    Some(format!(
        "top-orphans: {} — run `touring wiring suggest <file>` to wire them into this plan",
        top.join(", ")
    ))
}

/// R38-S3: Emit an evolution drift advisory before plan creation begins (CC≤3).
///
/// Calls `cli_evolution_drift` and parses the `alert_level` field. When the system is at
/// `"structural"` degradation (3+ metrics degrading), returns a warning hint so Claude Code
/// knows to investigate technical debt before creating new plans. Returns `None` for `"none"`
/// or `"degraded"` alert levels — avoids noise on normal operation.
pub(crate) fn evolution_drift_hint_on_enter_plan(rt: &mut HookRuntime) -> Option<String> {
    let drift_json = crate::cli_handlers::cli_evolution_drift(rt, &serde_json::json!({}));
    let v = serde_json::from_str::<serde_json::Value>(&drift_json).ok()?;
    let alert_level = v
        .get("alert_level")
        .and_then(|a| a.as_str())
        .unwrap_or("none");
    if alert_level == "structural" {
        Some(
            "⚠ evolution-drift: STRUCTURAL degradation detected — \
            run `touring evolution drift -j` to review before planning new features"
                .to_string(),
        )
    } else {
        None
    }
}

/// R42-S3: Emit AAAK diary write hint for the planning intent on EnterPlanMode (CC≤2).
///
/// Records the planning session intent into the claude_code diary using AAAK format
/// so future sessions can recall what was planned without needing memory_recall context.
/// Uses `extend(Option)` wiring pattern in `handle_enter_plan_mode` — zero CC overhead.
/// Returns None when intent is empty to avoid noise on bare EnterPlanMode calls.
pub(crate) fn maybe_diary_write_on_plan(intent: &str) -> Option<String> {
    if intent.is_empty() {
        return None;
    }
    let truncated = &intent[..intent.len().min(60)];
    Some(format!(
        "diary: run `touring diary write claude_code \
        \"#[P:planning] #[R:0.0] #[L:{truncated}] #[W:none] #[E:none]\" --aaak` \
        to record this planning session"
    ))
}

/// Query memory for intent-relevant past patterns (R14-S2 helper, CC≤3).
///
/// Calls `cli_memory_recall` and returns a formatted hint when entries are found.
/// Returns `None` when recall is empty or JSON is malformed. Extracted from
/// `handle_enter_plan_mode` so its CC stays ≤ 15 after R24-S2 added kind_hint branch.
pub(crate) fn memory_recall_hint_for_intent(
    rt: &mut HookRuntime,
    truncated: &str,
) -> Option<String> {
    let recall_payload = serde_json::json!({"query": truncated, "limit": 3});
    let recall_result = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
    let v = serde_json::from_str::<serde_json::Value>(&recall_result).ok()?;
    let entries = v
        .get("entries")
        .and_then(|e| e.as_array())
        .map(|arr| arr.len())
        .unwrap_or(0);
    if entries == 0 {
        return None;
    }
    Some(format!(
        "recall: {entries} past pattern(s) found — run `touring memory recall \"{truncated}\"` for details"
    ))
}

/// R46-S1: Search plan registry for GeneratorPlans matching the current intent (CC=2).
///
/// Surfaces `touring generate plan-recall --query "<intent>"` in EnterPlanMode so Claude Code
/// can discover existing GeneratorPlans before creating a new one. This connects the
/// Claude Code planning phase directly to the touring-generator plan registry — avoiding
/// duplicate plan creation and surfacing reusable artifact templates.
/// Returns `None` when intent is empty or too short to produce a useful recall query.
pub(crate) fn maybe_plan_recall_hint_for_intent(intent: &str) -> Option<String> {
    if intent.len() < 3 {
        return None;
    }
    let query = &intent[..intent.len().min(60)];
    Some(format!(
        "plan-registry: run `touring generate plan-recall --query \"{query}\"` \
        to find reusable GeneratorPlans before creating a new one"
    ))
}

/// D4 — MCTS shadow rollout hint for complex plans (CC≤5).
///
/// Reads the session CILA level from the result cache. When `cila_level >= 3`,
/// collects pending decompose subtasks as the task list and spawns a shadow rollout
/// on a dedicated thread with a 12s budget. Joins with a 200ms timeout — if the
/// rollout doesn't finish in time, returns `None` (plan-mode entry never blocked).
///
/// Returns `None` when:
/// - `cila_level < 3` (simple tasks don't need deadlock analysis)
/// - No decompose subtasks are available to analyze
/// - The rollout thread doesn't complete within 200ms
/// - The rollout detects no deadlock
pub(crate) fn mcts_shadow_rollout_hint(rt: &mut HookRuntime, _input: &Value) -> Option<String> {
    // Read CILA level from session cache — same pattern as post_edit and pre_read.
    let cila_level: u8 = rt
        .ctx
        .result_cache
        .get_result("__meta__", "__session_cila_level__")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    if cila_level < 3 {
        return None;
    }

    // Collect pending ready subtasks as the task list for the rollout.
    let ready_json = crate::cli_handlers::cli_decompose_ready(rt, &serde_json::json!({}));
    let tasks: Vec<serde_json::Value> = serde_json::from_str::<serde_json::Value>(&ready_json)
        .ok()
        .and_then(|v| v.get("ready_subtasks").and_then(|s| s.as_array()).cloned())
        .unwrap_or_default();

    if tasks.is_empty() {
        return None;
    }

    // Spawn rollout on a dedicated thread with a 12s budget.
    // Use a channel to implement a 200ms join timeout — advisory only, never
    // blocks plan-mode entry. If the rollout doesn't finish in time, returns None.
    let (tx, rx) = std::sync::mpsc::channel();
    let tasks_clone = tasks.clone();
    std::thread::spawn(move || {
        let result = run_shadow_rollout(&tasks_clone, None, Duration::from_secs(12));
        // Ignore send error — receiver may have timed out and been dropped.
        let _ = tx.send(result);
    });

    let result = rx.recv_timeout(Duration::from_millis(200)).ok()??;

    tracing::info!(
        cila_level,
        tasks_count = tasks.len(),
        elapsed_ms = result.elapsed_ms,
        deadlock_detected = result.deadlock_detected,
        prefetch_paths = result.prefetch_paths.len(),
        "mcts shadow rollout completed"
    );

    // Suggestion 3 — Predictive file prefetch: publish predicted paths to the
    // session bus and forward them to the background prefetch actor. The actor
    // calls global_cache().get_or_create() on a blocking thread so that when
    // pre_read fires on the same files the parse result is already in RAM.
    if !result.prefetch_paths.is_empty() {
        rt.ctx
            .session_bus
            .borrow_mut()
            .signal_prefetch_files(result.prefetch_paths.clone());
        let project_root = rt.project_root.clone();
        let paths = rt.ctx.session_bus.borrow_mut().drain_prefetch_queue();
        for rel_path in &paths {
            let abs_path = project_root.join(rel_path);
            crate::shared::file_prefetch::try_enqueue_prefetch(abs_path);
        }
    }

    result.as_hint()
}

/// P4 (2026-04-13): mark a pending `plan_mode` suggestion as consumed when CC
/// explicitly enters plan mode (CC≤3).
///
/// Looks for `suggestion_ref` in the input payload. When present, calls
/// `cli_suggestion_mark_consumed` so the rate-limit gate resets and fresh
/// suggestions can be emitted on the next session start.
/// Returns silently on any error — never blocks EnterPlanMode.
fn consume_plan_mode_suggestion_if_present(rt: &mut HookRuntime, input: &Value) {
    let sugg_id = input
        .get("suggestion_ref")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    if let Some(id) = sugg_id {
        let _ = crate::cli_handlers::cli_suggestion_mark_consumed(
            rt,
            &serde_json::json!({
                "suggestion_id": id,
                "consumed_action": "plan_mode",
            }),
        );
    }
}
