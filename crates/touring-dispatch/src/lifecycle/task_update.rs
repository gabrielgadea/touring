//! `task-sync-post-update` hook handler.
//!
//! Mirrors Claude Code's TaskUpdate to the Touring decompose DAG. On status
//! transitions it:
//!   - Updates the DAG row (`cli_decompose_update`)
//!   - On `completed`: advances ::implement/::validate, auto-finalizes DAG,
//!     upserts Tantivy completion marker, checkpoints session, stores lesson,
//!     emits wiring-orphan/diary/plan-recall hints, records artifact memory,
//!     and surfaces subject-based plan-replay.
//!   - On `in_progress`: suggests GeneratorKind, starts session (R38-S1),
//!     advances ::scout subtask (R156), surfaces lesson recall + Tantivy
//!     code-intel, and collects per-kind generator hints.
//!   - On `blocked`: surfaces MCTS unblock hint (R61-S3).
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D3. Helpers reused across
//! other TaskSync handlers remain in `lifecycle.rs` and are accessed via
//! `super::<helper>` (all `pub(crate)`).

use serde_json::Value;

use crate::runtime::HookRuntime;

// suggest_generator_for_task_subject and upsert_task_completion_to_tantivy
// remain in super (shared.rs and task_list.rs respectively).
use super::{suggest_generator_for_task_subject, upsert_task_completion_to_tantivy};

/// Pln3: Mark a `cc_action_suggestions` row as consumed if `suggestion_ref` is
/// present in the tool input. Extracted to keep `handle_task_sync_post_update` CC-lean.
///
/// Accepts both `tool_input` (nested) and `input` (flat) to handle the two
/// payload shapes that hooks receive.
fn consume_suggestion_ref(rt: &mut HookRuntime, tool_input: &Value, input: &Value) {
    let sugg_id = tool_input
        .get("suggestion_ref")
        .or_else(|| input.get("suggestion_ref"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    if let Some(id) = sugg_id {
        let _ = crate::cli_handlers::cli_suggestion_mark_consumed(
            rt,
            &serde_json::json!({"suggestion_id": id, "consumed_action": "update"}),
        );
    }
}

/// task-sync-update: PostToolUse(TaskUpdate) — mirror status change to Touring decompose.
///
/// Fires after Claude Code's TaskUpdate tool call. Emits a CLI hint to mirror
/// the status change in the Touring decompose DAG so both systems stay in sync.
pub(crate) fn handle_task_sync_post_update(rt: &mut HookRuntime, input: &Value) -> String {
    let tool_input = input.get("tool_input").unwrap_or(input);
    // F0-pre (2026-07-20): CC tool schemas use camelCase (`taskId`) — reading
    // only snake_case routed every update to the "unknown" mirror (a no-op),
    // which is why status transitions never propagated (probe B2, live).
    let task_id = tool_input
        .get("task_id")
        .or_else(|| tool_input.get("taskId"))
        .or_else(|| input.get("task_id"))
        .or_else(|| input.get("taskId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let status = tool_input
        .get("status")
        .or_else(|| input.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("updated");

    let _ = rt
        .ctx
        .knowledge
        .record_access("__task_sync_update__", task_id);

    // Pln3 (2026-04-13): consume suggestion if caller passed suggestion_ref.
    consume_suggestion_ref(rt, tool_input, input);

    // R12-S2: Call cli_decompose_update directly for all statuses — real sync bridge.
    // F0-pre (2026-07-20): address the MIRROR row (`cc_task_<id>`) — updates
    // previously targeted the raw CC id, which never matched any container, so
    // status transitions silently propagated to nothing.
    let mirror_id = crate::hook_decompose_bridge::cc_mirror_task_id(task_id);
    let update_payload = serde_json::json!({
        "task_id": &mirror_id,
        "status": status,
    });
    let update_result = crate::cli_handlers::cli_decompose_update(rt, &update_payload);
    if update_result.contains("\"updated\":true") {
        touring_foundation::gate_metrics::record_task_sync_update_propagated();
    }

    // R135: Extract task subject before branch — used in completed branch for artifact memory
    // and subject-based plan-replay hint (mirrors R130 + R134 in hook_registry::task-completed).
    let subject = tool_input
        .get("task_subject")
        .or_else(|| input.get("task_subject"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if status == "completed" {
        let merged = serde_json::json!({
            "task_id": task_id,
            "success": true,
            "session_id": format!("cc-{}", &task_id[..task_id.len().min(20)]),
            "result_summary": format!("TaskUpdate: {task_id} marked completed via Claude Code"),
        });
        if let Err(e) = crate::team_hooks::run_task_completed(rt, &merged) {
            tracing::debug!(error = %e, task_id = task_id, "task_sync_update: run_task_completed failed");
        }
        // R158: Auto-advance ::implement + ::validate subtasks to completed before finalize.
        {
            let impl_id = format!("{mirror_id}::implement");
            let validate_id = format!("{mirror_id}::validate");
            let _ = crate::cli_handlers::cli_decompose_update(
                rt,
                &serde_json::json!({
                    "task_id": &mirror_id,
                    "subtask_id": impl_id,
                    "status": "completed",
                    "priority": 5,
                }),
            );
            let _ = crate::cli_handlers::cli_decompose_update(
                rt,
                &serde_json::json!({
                    "task_id": &mirror_id,
                    "subtask_id": validate_id,
                    "status": "completed",
                    "priority": 5,
                }),
            );
        }
        // R15-S2: Auto-finalize decompose DAG — archive + RL 1.0 injected inline.
        let finalize_payload = serde_json::json!({"task_id": task_id});
        let finalize_result = crate::cli_handlers::cli_decompose_finalize(rt, &finalize_payload);
        let finalize_hint = if finalize_result.contains("\"archived\":true") {
            format!(" | decompose-finalized: {task_id} archived (RL 1.0 injected)")
        } else {
            format!(
                " | run `touring decompose finalize {task_id}` to archive when all subtasks complete"
            )
        };
        // R21-S3: Upsert completion marker to Tantivy.
        upsert_task_completion_to_tantivy(&rt.project_root, task_id);
        // R34-S1
        let checkpoint_hint = auto_checkpoint_on_task_complete(rt, task_id);
        // R39-S3
        let lesson_hint = auto_lesson_on_task_complete(rt, task_id);
        // R43-S3
        let wiring_check = maybe_wiring_orphan_hint_on_complete(task_id);
        // R45-S3
        let diary_entry = diary_entry_hint_on_task_complete(task_id);
        // R46-S3
        let plan_recall_hint = maybe_plan_recall_on_task_complete(task_id);
        // R135: ConsumerGenerator scaffold when wiring orphan check fires.
        let consumer_gen_hint = if !wiring_check.is_empty() {
            let truncated_id = &task_id[..task_id.len().min(40)];
            format!(
                " | consumer-wire: run `touring generate render ConsumerGenerator \
                --vars '{{\"source_module\":\"{truncated_id}\",\"event\":\"task:{truncated_id}:completed\"}}'` \
                to scaffold consumer for new artifacts if orphans detected"
            )
        } else {
            String::new()
        };
        // R135: Artifact memory mapping.
        if subject.len() > 3 {
            let truncated_subject = &subject[..subject.len().min(200)];
            let _ = crate::cli_handlers::cli_memory_store(
                rt,
                &serde_json::json!({
                    "key": format!("artifact:{task_id}"),
                    "value": format!("TaskUpdate {task_id} artifact: {truncated_subject} — completed"),
                    "tier": "semantic",
                    "entry_type": "lesson",
                }),
            );
        }
        // R135: Subject-based plan-replay hint.
        let subject_plan_replay = if subject.len() > 3 {
            let short_subject = &subject[..subject.len().min(60)];
            format!(
                " | replay: run `touring generate plan-recall --query \"{short_subject}\"` \
                to find and replay generator plan on a new subject"
            )
        } else {
            String::new()
        };
        format!(
            "touring-sync: decompose update {task_id} completed (applied){finalize_hint}{checkpoint_hint}{lesson_hint}{wiring_check}{consumer_gen_hint}{diary_entry}{plan_recall_hint}{subject_plan_replay} | \
            run `touring learning reward orchestrate 1.0 \"task:{task_id}:completed\"`"
        )
    } else {
        // R20-S3: For in_progress tasks, surface a GeneratorKind suggestion by keyword matching.
        let gen_hint = suggest_generator_for_task_subject(subject);
        // R27-S1/S3
        let status_hint = update_status_side_effects(rt, task_id, status);
        // R30-S3
        let ready_hint = dag_next_ready_hint(rt, task_id);
        // R38-S1
        let session_hint = maybe_start_session_on_in_progress(rt, task_id, subject, status);
        // R61-S3
        let mcts_blocked = maybe_mcts_hint_on_task_blocked(task_id, status).unwrap_or_default();
        // R118-R121: per-kind generator hints
        let update_kind_hints = collect_update_generator_hints(subject);
        let update_hints_str = if update_kind_hints.is_empty() {
            String::new()
        } else {
            format!(" | {}", update_kind_hints.join(" | "))
        };
        // R138
        let lesson_recall_hint = if status == "in_progress" && subject.len() > 3 {
            let short_subject = &subject[..subject.len().min(60)];
            format!(
                " | recall-lessons: run `touring memory recall \"{short_subject}\"` to surface past lessons before starting"
            )
        } else {
            String::new()
        };
        // R151
        let tantivy_inprogress_hint = if status == "in_progress" && subject.len() > 3 {
            let query = &subject[..subject.len().min(50)];
            format!(
                " | code-intel: run `touring tantivy search \"{query}\"` to find \
                existing symbols before implementing"
            )
        } else {
            String::new()
        };
        // R156: Auto-advance ::scout subtask to in_progress.
        let scout_advance_hint = if status == "in_progress" {
            let scout_id = format!("{task_id}::scout");
            let advance_result = crate::cli_handlers::cli_decompose_update(
                rt,
                &serde_json::json!({
                    "task_id": task_id,
                    "subtask_id": scout_id,
                    "status": "in_progress",
                    "priority": 5,
                }),
            );
            if advance_result.contains("\"subtask_updated\":true") {
                format!(" | dag-sync: {task_id}::scout auto-advanced to in_progress")
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        format!(
            "touring-sync: decompose update {task_id} {status} (applied){gen_hint}{status_hint}{ready_hint}{session_hint}{mcts_blocked}{update_hints_str}{lesson_recall_hint}{tantivy_inprogress_hint}{scout_advance_hint} | verify: `touring decompose get {task_id}`"
        )
    }
}

// ── D10: helpers moved from lifecycle.rs ─────────────────────────────────────
//
// The following functions were co-located in `lifecycle.rs` and accessed via
// `super::<helper>` by this module. D10 moves them here (single caller or
// primary owner) and re-exports them from `lifecycle.rs` so inline tests
// (`use super::*`) continue to resolve via `super::<fn>`.

// ── Completion/status helpers ─────────────────────────────────────────────────

/// R34-S1: Auto-save a session checkpoint when a task is marked completed (CC=2).
///
/// When `TaskUpdate(completed)` fires, the DAG state should be checkpointed so the
/// touring session can be recovered after a daemon restart or context compaction.
/// Calls `cli_session_checkpoint` with a JSON snapshot of the task completion event.
/// Returns a hint confirming the checkpoint, or empty string when the call fails.
pub(crate) fn auto_checkpoint_on_task_complete(rt: &mut HookRuntime, task_id: &str) -> String {
    let checkpoint_payload = serde_json::json!({
        "session_id": format!("cc-{}", &task_id[..task_id.len().min(20)]),
        "data": format!("{{\"task_id\":\"{task_id}\",\"status\":\"completed\"}}"),
    });
    let result = crate::cli_handlers::cli_session_checkpoint(rt, &checkpoint_payload);
    if result.contains("\"success\":true") || result.contains("checkpoint") {
        " | checkpoint: session state saved — run `touring session list` to verify".to_string()
    } else {
        String::new()
    }
}

/// R39-S3: Store AAAK-format lesson to memory when a task completes (CC≤2).
///
/// When TaskUpdate(completed) fires, persists a lesson to the knowledge DB using the AAAK
/// diary format (`#[P:phase] #[R:reward] #[L:lesson] #[W:warn] #[E:error]`) so future sessions
/// can recall what was learned at completion time. Returns a diary write hint so Claude Code
/// can optionally record a richer entry via `touring diary write`.
pub(crate) fn auto_lesson_on_task_complete(rt: &mut HookRuntime, task_id: &str) -> String {
    let aaak_entry = format!(
        "#[P:completed] #[R:1.0] #[L:task {task_id} completed via Claude Code] #[W:none] #[E:none]"
    );
    let _ = crate::cli_handlers::cli_memory_store(
        rt,
        &serde_json::json!({
            "key": format!("task:{task_id}:completed-lesson"),
            "value": &aaak_entry,
            "tier": "semantic",
            "entry_type": "lesson",
        }),
    );
    format!(
        " | lesson: AAAK stored — run `touring diary write claude_code \"{aaak_entry}\" --aaak` to record"
    )
}

/// R43-S3: Emit a wiring orphan check hint when a task completes (CC≤2).
///
/// After a task is marked completed, new pub symbols may have been introduced by the
/// implementation that are not yet wired to any consumer. This helper surfaces a
/// `touring wiring orphans -j` command to find those orphans before the next task starts.
/// Returns empty string for unknown or empty task IDs to avoid noise on synthetic completions.
pub(crate) fn maybe_wiring_orphan_hint_on_complete(task_id: &str) -> String {
    if task_id.is_empty() || task_id == "unknown" {
        return String::new();
    }
    format!(
        " | wiring-check: run `touring wiring orphans -j` to find new orphan pub symbols \
        introduced by {task_id} — wire or remove before next task"
    )
}

/// R45-S3: Suggest diary entry generation via touring-generator on task completion (CC=2).
///
/// Closes the task lifecycle loop: TaskCompleted → diary entry → institutional memory.
/// Emits `touring generate render diary_entry` so lessons learned during the task
/// are captured in AAAK format and persisted for future sessions via touring-generator.
/// Returns empty string when task_id is sentinel or empty.
pub(crate) fn diary_entry_hint_on_task_complete(task_id: &str) -> String {
    if task_id.is_empty() || task_id == "unknown" {
        return String::new();
    }
    format!(
        " | diary-entry: run `touring generate render diary_entry \
        --vars '{{\"agent\":\"claude-code\",\"task_id\":\"{task_id}\",\
        \"intent\":\"task:{task_id}:completed\",\"phase\":\"retrospective\"}}' -j` \
        to generate institutional memory entry via touring-generator"
    )
}

/// R46-S3: Suggest plan-recall search for historical GeneratorPlans on task completion (CC=2).
///
/// When TaskCompleted fires, surfaces `touring generate plan-recall --query "task:<task_id>"`
/// so Claude Code can discover prior plans that targeted the same task and reuse their
/// artifact templates rather than starting from scratch on follow-up tasks.
/// Closes the TaskCompleted → GeneratorPlan history loop with the touring-generator registry.
/// Returns empty string when task_id is sentinel or empty to avoid noise.
pub(crate) fn maybe_plan_recall_on_task_complete(task_id: &str) -> String {
    if task_id.is_empty() || task_id == "unknown" {
        return String::new();
    }
    format!(
        " | plan-recall: run `touring generate plan-recall --query \"task:{task_id}\"` \
        to find historical GeneratorPlans for this task via touring-generator registry"
    )
}

/// R38-S1: Auto-start Touring session when a task transitions to in_progress (CC≤3).
///
/// R36-S1 starts a session on TaskCreated. If the daemon restarted between TaskCreated and
/// TaskUpdate(in_progress), the session is gone. This helper ensures the session is recovered
/// by calling `cli_session_start` idempotently (upsert semantics). Returns empty string for
/// any status other than `"in_progress"` to avoid noise on blocked/paused transitions.
pub(crate) fn maybe_start_session_on_in_progress(
    rt: &mut HookRuntime,
    task_id: &str,
    subject: &str,
    status: &str,
) -> String {
    if status != "in_progress" {
        return String::new();
    }
    let sess_payload = serde_json::json!({
        "session_id": task_id,
        "task_type": "task",
        "objective": &subject[..subject.len().min(200)],
    });
    let result = crate::cli_handlers::cli_session_start(rt, &sess_payload);
    if result.contains("session_id") {
        format!(" | session:{task_id} ensured (in_progress recovery)")
    } else {
        String::new()
    }
}

/// R61-S3: Emit MCTS unblock path hint when TaskUpdate fires with `status=blocked` (CC=2).
///
/// When a Claude Code task is marked `blocked`, the TACO DAG may have no ready subtasks.
/// This helper surfaces `touring mcts search` so the engineer can find an alternative
/// execution path without manually scanning the DAG.
/// Returns `None` for all non-blocked statuses — zero CC contribution in callers.
pub(crate) fn maybe_mcts_hint_on_task_blocked(task_id: &str, status: &str) -> Option<String> {
    if status != "blocked" {
        return None;
    }
    Some(format!(
        " | mcts-unblock: run `touring mcts search \"unblock {task_id}\"` to find alternative execution path"
    ))
}

/// R27-S1/S3: Inject RL reward + memory lesson for non-completed status transitions (CC=4).
///
/// - `in_progress` → RL +0.2 (work started signal) + memory lesson for audit
/// - `blocked` → RL -0.1 (impediment signal) + memory warning + ready-subtask fallback hint
/// - Other statuses → no-op (returns empty string)
pub(crate) fn update_status_side_effects(
    rt: &mut HookRuntime,
    task_id: &str,
    status: &str,
) -> String {
    match status {
        "in_progress" => {
            let _ = crate::cli_handlers::cli_learning_reward(
                rt,
                &serde_json::json!({
                    "tool": "orchestrate",
                    "reward": 0.2_f64,
                    "context": format!("task:{task_id}:in_progress"),
                }),
            );
            let _ = crate::cli_handlers::cli_memory_store(
                rt,
                &serde_json::json!({
                    "key": format!("task:{task_id}:started"),
                    "value": format!("Task {task_id} started (in_progress) via Claude Code — RL +0.2 injected"),
                    "tier": "semantic",
                    "entry_type": "lesson",
                }),
            );
            tracing::debug!(task_id, "in_progress: RL +0.2 + memory stored (R27-S1)");
            String::new()
        }
        "blocked" => {
            let _ = crate::cli_handlers::cli_learning_reward(
                rt,
                &serde_json::json!({
                    "tool": "orchestrate",
                    "reward": -0.1_f64,
                    "context": format!("task:{task_id}:blocked"),
                }),
            );
            let _ = crate::cli_handlers::cli_memory_store(
                rt,
                &serde_json::json!({
                    "key": format!("task:{task_id}:blocked"),
                    "value": format!("Task {task_id} blocked — investigate impediment; run `touring decompose ready` to find alternative subtasks"),
                    "tier": "semantic",
                    "entry_type": "lesson",
                }),
            );
            tracing::debug!(task_id, "blocked: RL -0.1 + memory warning stored (R27-S3)");
            let mcts_hint = maybe_mcts_unblock_hint(task_id);
            format!(
                " | task blocked — run `touring decompose ready` to find alternative subtasks to progress{mcts_hint}"
            )
        }
        _ => String::new(),
    }
}

/// R43-S1: MCTS unblock hint when task transitions to blocked (CC≤2).
///
/// When a task is blocked, `touring decompose ready` surfaces sibling subtasks,
/// but does not suggest a multi-path unblocking strategy. This helper closes that gap
/// by emitting a `touring mcts search "unblock <task_id>"` command — the MCTS engine
/// runs Monte Carlo rollouts across the DAG to find an optimal alternative execution path.
/// Returns empty string for unknown or empty task IDs to avoid noise.
pub(crate) fn maybe_mcts_unblock_hint(task_id: &str) -> String {
    if task_id.is_empty() || task_id == "unknown" {
        return String::new();
    }
    format!(
        " | mcts-unblock: run `touring mcts search \"unblock {task_id}\"` \
        for optimal alternative execution path"
    )
}

/// R30-S3: Surface the next actionable DAG subtask after a status update (CC≤5).
///
/// When a TaskUpdate moves a subtask forward (e.g., scout → completed), sibling subtasks
/// with satisfied dependencies become ready. This helper queries `cli_decompose_ready`
/// filtered by `task_id` and surfaces the first ready subtask as an actionable hint,
/// closing the "update X → here's what to do next" loop automatically.
/// Returns empty string when no subtasks are ready or DAG is empty.
pub(crate) fn dag_next_ready_hint(rt: &mut HookRuntime, task_id: &str) -> String {
    let payload = serde_json::json!({"task_id": task_id});
    let result = crate::cli_handlers::cli_decompose_ready(rt, &payload);
    let v = match serde_json::from_str::<serde_json::Value>(&result) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let count = v.get("ready_count").and_then(|c| c.as_u64()).unwrap_or(0);
    if count == 0 {
        return String::new();
    }
    let first_id = v
        .get("ready_subtasks")
        .and_then(|rs| rs.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("subtask_id"))
        .and_then(|id| id.as_str())
        .unwrap_or("");
    if first_id.is_empty() {
        return format!(
            " | next-ready: {count} subtask(s) now actionable — `touring decompose ready {task_id}`"
        );
    }
    format!(" | next-ready: `{first_id}` is actionable — `touring decompose ready {task_id}`")
}

// ── R118-R120: 30 TaskUpdate per-kind hint helpers ────────────────────────────

/// R118-R121: Aggregate all 30 per-kind TaskUpdate generator hints (CC=1).
///
/// Mirrors `collect_subject_generator_hints` for TaskCreate but surfaces hints
/// in the context of a status update — "you are implementing X, here is the
/// scaffold command to run now". Returns only non-None hints so the caller
/// can join them with " | " and append to the format string.
///
/// `pub(crate)` so `hook_registry.rs` can reuse this in the `task-completed`
/// event handler to surface artifact generation hints on task completion (R122-S2).
pub(crate) fn collect_update_generator_hints(subject: &str) -> Vec<String> {
    type HintFn = fn(&str) -> Option<String>;
    const MATCHERS: &[HintFn] = &[
        // R118-S1..S10: first 10 kinds
        maybe_rust_module_hint_on_task_update,
        maybe_cli_handler_hint_on_task_update,
        maybe_mcp_tool_hint_on_task_update,
        maybe_hook_handler_hint_on_task_update,
        maybe_plan_md_hint_on_task_update,
        maybe_test_hint_on_task_update,
        maybe_python_script_hint_on_task_update,
        maybe_schema_hint_on_task_update,
        maybe_benchmark_hint_on_task_update,
        maybe_fuzz_target_hint_on_task_update,
        // R119-S1..S10: next 10 kinds
        maybe_derive_macro_hint_on_task_update,
        maybe_migration_hint_on_task_update,
        maybe_ffi_binding_hint_on_task_update,
        maybe_protobuf_hint_on_task_update,
        maybe_openapi_hint_on_task_update,
        maybe_shell_completion_hint_on_task_update,
        maybe_man_page_hint_on_task_update,
        maybe_error_catalog_hint_on_task_update,
        maybe_incremental_patch_hint_on_task_update,
        maybe_skill_document_hint_on_task_update,
        // R120-S1..S10: final 10 kinds
        maybe_diary_entry_hint_on_task_update,
        maybe_dockerfile_hint_on_task_update,
        maybe_k8s_manifest_hint_on_task_update,
        maybe_terraform_hint_on_task_update,
        maybe_ci_workflow_hint_on_task_update,
        maybe_changelog_hint_on_task_update,
        maybe_adr_hint_on_task_update,
        maybe_asyncapi_hint_on_task_update,
        maybe_consumer_generator_hint_on_task_update,
        maybe_task_scaffold_hint_on_task_update,
    ];
    MATCHERS.iter().filter_map(|f| f(subject)).collect()
}

// ── R118: First 10 TaskUpdate per-kind helpers ────────────────────────────────

/// R118-S1: Suggest `rust_module` generator when TaskUpdate subject involves Rust source (CC=2).
pub(crate) fn maybe_rust_module_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "rust module",
        "rust crate",
        "rust impl",
        "rust struct",
        "rust trait",
        "rust function",
        "rust library",
        "rust source",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "rust-module: update {s} — run `touring generate render RustModule` to scaffold via touring-generator"
    ))
}

/// R118-S2: Suggest `cli_handler` generator when TaskUpdate subject involves CLI work (CC=2).
pub(crate) fn maybe_cli_handler_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "cli command",
        "cli handler",
        "command handler",
        "clap command",
        "subcommand",
        "arg parse",
        "cli tool",
        "cli subcommand",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "cli-handler: update {s} — run `touring generate render CliHandler` to scaffold via touring-generator"
    ))
}

/// R118-S3: Suggest `mcp_tool` generator when TaskUpdate subject involves MCP tools (CC=2).
pub(crate) fn maybe_mcp_tool_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "mcp tool",
        "mcp server",
        "model context",
        "tool definition",
        "rmcp tool",
        "mcp endpoint",
        "tool schema",
        "mcp plugin",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "mcp-tool: update {s} — run `touring generate render McpTool` to scaffold via touring-generator"
    ))
}

/// R118-S4: Suggest `hook_handler` generator when TaskUpdate subject involves hooks (CC=2).
pub(crate) fn maybe_hook_handler_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "hook handler",
        "lifecycle hook",
        "pre-edit hook",
        "post-edit hook",
        "session hook",
        "hook registry",
        "claude code hook",
        "hook implementation",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "hook-handler: update {s} — run `touring generate render HookHandler` to scaffold via touring-generator"
    ))
}

/// R118-S5: Suggest `plan_md` generator when TaskUpdate subject involves planning docs (CC=2).
pub(crate) fn maybe_plan_md_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "project plan",
        "roadmap plan",
        "sprint plan",
        "milestone plan",
        "planning document",
        "plan document",
        "plan markdown",
        "planning doc",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "plan-md: update {s} — run `touring generate render PlanMd` to scaffold via touring-generator"
    ))
}

/// R118-S6: Suggest `test` generator when TaskUpdate subject involves test writing (CC=2).
pub(crate) fn maybe_test_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "write test",
        "add test",
        "test coverage",
        "unit test",
        "integration test",
        "test suite",
        "test case",
        "e2e test",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "test: update {s} — run `touring generate render Test` to scaffold via touring-generator"
    ))
}

/// R118-S7: Suggest `python_script` generator when TaskUpdate subject involves Python (CC=2).
pub(crate) fn maybe_python_script_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "python script",
        "python automation",
        "pyscript",
        "python tool",
        "python module",
        "python utility",
        "py script",
        "python cli",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "python-script: update {s} — run `touring generate render PythonScript` to scaffold via touring-generator"
    ))
}

/// R118-S8: Suggest `schema` generator when TaskUpdate subject involves data schemas (CC=2).
pub(crate) fn maybe_schema_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "data schema",
        "json schema",
        "schema design",
        "schema definition",
        "schema validator",
        "schema migration",
        "type schema",
        "validate schema",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "schema: update {s} — run `touring generate render Schema` to scaffold via touring-generator"
    ))
}

/// R118-S9: Suggest `benchmark` generator when TaskUpdate subject involves perf testing (CC=2).
pub(crate) fn maybe_benchmark_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "benchmark",
        "criterion",
        "performance test",
        "perf bench",
        "latency measure",
        "throughput test",
        "microbenchmark",
        "cargo bench",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "benchmark: update {s} — run `touring generate render Benchmark` to scaffold via touring-generator"
    ))
}

/// R118-S10: Suggest `fuzz_target` generator when TaskUpdate subject involves fuzzing (CC=2).
pub(crate) fn maybe_fuzz_target_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "fuzz target",
        "cargo fuzz",
        "libfuzzer",
        "afl fuzz",
        "fuzz test",
        "fuzzing",
        "fuzz corpus",
        "fuzz harness",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "fuzz-target: update {s} — run `touring generate render FuzzTarget` to scaffold via touring-generator"
    ))
}

// ── R119: Next 10 TaskUpdate per-kind helpers ─────────────────────────────────

/// R119-S1: Suggest `derive_macro` generator when TaskUpdate subject involves proc-macros (CC=2).
pub(crate) fn maybe_derive_macro_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "derive macro",
        "proc macro",
        "proc-macro",
        "procedural macro",
        "custom derive",
        "attribute macro",
        "macro crate",
        "derive trait",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "derive-macro: update {s} — run `touring generate render DeriveMacro` to scaffold via touring-generator"
    ))
}

/// R119-S2: Suggest `migration` generator when TaskUpdate subject involves DB migrations (CC=2).
pub(crate) fn maybe_migration_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "migration",
        "db schema",
        "database schema",
        "sql migration",
        "schema migration",
        "alembic",
        "diesel migration",
        "sqlx migration",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "migration: update {s} — run `touring generate render Migration` to scaffold via touring-generator"
    ))
}

/// R119-S3: Suggest `ffi_binding` generator when TaskUpdate subject involves FFI/native bindings (CC=2).
pub(crate) fn maybe_ffi_binding_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "ffi binding",
        "c binding",
        "native binding",
        "unsafe extern",
        "bindgen",
        "cbindgen",
        "ffi wrapper",
        "native ffi",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "ffi-binding: update {s} — run `touring generate render FfiBinding` to scaffold via touring-generator"
    ))
}

/// R119-S4: Suggest `protobuf_schema` generator when TaskUpdate subject involves protobuf (CC=2).
pub(crate) fn maybe_protobuf_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "protobuf",
        "proto schema",
        "grpc proto",
        ".proto",
        "protocol buffer",
        "proto definition",
        "tonic proto",
        "prost proto",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "protobuf-schema: update {s} — run `touring generate render ProtobufSchema` to scaffold via touring-generator"
    ))
}

/// R119-S5: Suggest `openapi_spec` generator when TaskUpdate subject involves REST APIs (CC=2).
pub(crate) fn maybe_openapi_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "openapi",
        "swagger",
        "rest api",
        "api spec",
        "oas3",
        "api schema",
        "rest endpoint",
        "http api",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "openapi-spec: update {s} — run `touring generate render OpenApiSpec` to scaffold via touring-generator"
    ))
}

/// R119-S6: Suggest `shell_completion` generator when TaskUpdate involves shell scripts (CC=2).
pub(crate) fn maybe_shell_completion_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "shell completion",
        "bash completion",
        "zsh completion",
        "fish completion",
        "shell script",
        "completion script",
        "cli completion",
        "autocomplete",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "shell-completion: update {s} — run `touring generate render ShellCompletion` to scaffold via touring-generator"
    ))
}

/// R119-S7: Suggest `man_page` generator when TaskUpdate subject involves man pages (CC=2).
pub(crate) fn maybe_man_page_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "man page",
        "man section",
        "manpage",
        "troff",
        "groff",
        "unix manual",
        "cli manual",
        "command manual",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "man-page: update {s} — run `touring generate render ManPage` to scaffold via touring-generator"
    ))
}

/// R119-S8: Suggest `error_catalog` generator when TaskUpdate involves error definitions (CC=2).
pub(crate) fn maybe_error_catalog_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "error catalog",
        "error types",
        "error codes",
        "error definitions",
        "error variants",
        "thiserror",
        "anyhow error",
        "error enum",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "error-catalog: update {s} — run `touring generate render ErrorCatalog` to scaffold via touring-generator"
    ))
}

/// R119-S9: Suggest `incremental_patch` generator when TaskUpdate involves patches (CC=2).
pub(crate) fn maybe_incremental_patch_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "patch",
        "incremental patch",
        "hotfix",
        "bugfix patch",
        "diff apply",
        "apply patch",
        "incremental update",
        "delta patch",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "incremental-patch: update {s} — run `touring generate render IncrementalPatch` to scaffold via touring-generator"
    ))
}

/// R119-S10: Suggest `skill_document` generator when TaskUpdate involves skill docs (CC=2).
pub(crate) fn maybe_skill_document_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "skill document",
        "skill guide",
        "playbook",
        "runbook",
        "tutorial",
        "how-to guide",
        "skill doc",
        "skill template",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "skill-document: update {s} — run `touring generate render SkillDocument` to scaffold via touring-generator"
    ))
}

// ── R120: Final 10 TaskUpdate per-kind helpers ────────────────────────────────

/// R120-S1: Suggest `diary_entry` generator when TaskUpdate involves agent diary (CC=2).
pub(crate) fn maybe_diary_entry_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "diary entry",
        "lesson learned",
        "session note",
        "retrospective",
        "post-mortem",
        "aaak entry",
        "agent diary",
        "memory entry",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "diary-entry: update {s} — run `touring generate render DiaryEntry` to scaffold via touring-generator"
    ))
}

/// R120-S2: Suggest `dockerfile` generator when TaskUpdate involves Docker images (CC=2).
pub(crate) fn maybe_dockerfile_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "dockerfile",
        "docker image",
        "container build",
        "docker container",
        "docker layer",
        "multi-stage build",
        "docker compose",
        "docker registry",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "dockerfile: update {s} — run `touring generate render Dockerfile` to scaffold via touring-generator"
    ))
}

/// R120-S3: Suggest `k8s_manifest` generator when TaskUpdate involves Kubernetes (CC=2).
pub(crate) fn maybe_k8s_manifest_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "kubernetes",
        "k8s manifest",
        "helm chart",
        "deployment yaml",
        "k8s pod",
        "k8s service",
        "k8s ingress",
        "kubectl apply",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "k8s-manifest: update {s} — run `touring generate render K8sManifest` to scaffold via touring-generator"
    ))
}

/// R120-S4: Suggest `terraform_module` generator when TaskUpdate involves IaC (CC=2).
pub(crate) fn maybe_terraform_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "terraform",
        "opentofu",
        "iac module",
        "terraform module",
        "tf resource",
        "hcl module",
        "infrastructure as code",
        "tf plan",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "terraform-module: update {s} — run `touring generate render TerraformModule` to scaffold via touring-generator"
    ))
}

/// R120-S5: Suggest `ci_workflow` generator when TaskUpdate involves CI/CD (CC=2).
pub(crate) fn maybe_ci_workflow_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "ci workflow",
        "github actions",
        "ci pipeline",
        "cd pipeline",
        "ci/cd",
        "pipeline workflow",
        "ci config",
        "release workflow",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "ci-workflow: update {s} — run `touring generate render CiWorkflow` to scaffold via touring-generator"
    ))
}

/// R120-S6: Suggest `changelog_entry` generator when TaskUpdate involves release notes (CC=2).
pub(crate) fn maybe_changelog_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "changelog",
        "release notes",
        "change log",
        "release entry",
        "version bump",
        "semver release",
        "release changelog",
        "news entry",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "changelog-entry: update {s} — run `touring generate render ChangelogEntry` to scaffold via touring-generator"
    ))
}

/// R120-S7: Suggest `adr` generator when TaskUpdate involves architecture decisions (CC=2).
pub(crate) fn maybe_adr_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "adr",
        "architecture decision",
        "design decision",
        "decision record",
        "architectural record",
        "madr",
        "nygard adr",
        "decision doc",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "adr: update {s} — run `touring generate render Adr` to scaffold via touring-generator"
    ))
}

/// R120-S8: Suggest `asyncapi_spec` generator when TaskUpdate involves event-driven APIs (CC=2).
pub(crate) fn maybe_asyncapi_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "asyncapi",
        "event-driven",
        "message broker",
        "kafka topic",
        "rabbitmq",
        "pubsub api",
        "async api",
        "event stream",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "asyncapi-spec: update {s} — run `touring generate render AsyncApiSpec` to scaffold via touring-generator"
    ))
}

/// R120-S9: Suggest `consumer_generator` generator when TaskUpdate involves event consumers (CC=2).
pub(crate) fn maybe_consumer_generator_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "event consumer",
        "message consumer",
        "consumer handler",
        "consumer service",
        "consumer worker",
        "kafka consumer",
        "stream consumer",
        "queue consumer",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "consumer-generator: update {s} — run `touring generate render ConsumerGenerator` to scaffold via touring-generator"
    ))
}

/// R120-S10: Suggest `task_scaffold` generator when TaskUpdate involves TACO task DAGs (CC=2).
pub(crate) fn maybe_task_scaffold_hint_on_task_update(subject: &str) -> Option<String> {
    if subject.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "task scaffold",
        "taco task",
        "task decompose",
        "dag scaffold",
        "task dag",
        "subtask plan",
        "task plan",
        "touring task",
    ];
    let lower = subject.to_lowercase();
    if !KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return None;
    }
    let s = &subject[..subject.len().min(40)];
    Some(format!(
        "task-scaffold: update {s} — run `touring generate render TaskScaffold` to scaffold via touring-generator"
    ))
}
