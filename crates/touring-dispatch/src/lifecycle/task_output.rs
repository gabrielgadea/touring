//! `task-sync-post-output` hook handler + 44 co-located hint helpers.
//!
//! Mirrors Claude Code's TaskSyncOutput to the Touring decompose DAG.
//! Every helper in this file is exclusive to `handle_task_sync_post_output` —
//! `output_outcome_hint`, `completion_signal_hint`, `failure_signal_hint`,
//! `advance_dag_validate_on_success`, `advance_dag_implement_on_artifact`,
//! all 30 `maybe_*_hint_on_output` helpers, `artifact_file_gen_hint`,
//! `maybe_test_pass_rl_reward`, `maybe_diary_lesson_on_output_success`,
//! `maybe_plan_validate_from_output`, `generator_hint_from_output`,
//! `extract_backtick_symbols`, and `extract_file_paths`.
//!
//! Co-location rationale: these helpers share the same "TaskOutput → generator
//! kind hint" design and change together; moving them to `lifecycle/shared.rs`
//! would bloat the cross-cutting API surface.
//!
//! All helpers are `pub(crate)` so the inline tests in `lifecycle::tests`
//! continue to reach them via `super::<helper>` after
//! `pub(crate) use task_output::*` re-export.
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D6.

use serde_json::Value;

use crate::runtime::HookRuntime;

// Pull in shared helpers used by this handler.
use super::{classify_file_to_generator_kind, file_stem, suggest_generator_for_task_subject};

pub(crate) fn handle_task_sync_post_output(rt: &mut HookRuntime, input: &Value) -> String {
    let tool_input = input.get("tool_input").unwrap_or(input);
    let task_id = tool_input
        .get("task_id")
        .or_else(|| tool_input.get("taskId"))
        .or_else(|| input.get("task_id"))
        .or_else(|| input.get("taskId"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let output_text = tool_input
        .get("output")
        .or_else(|| input.get("output"))
        .or_else(|| tool_input.get("result"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // R12-S3: Persist task output directly to knowledge DB — real sync bridge:
    // TaskOutput (Claude Code) → record_bash_outcome → SQLite (no hint needed from user).
    if !output_text.is_empty() {
        let summary = &output_text[..output_text.len().min(200)];
        let _ = rt
            .ctx
            .knowledge
            .record_bash_outcome(&crate::knowledge::BashOutcome {
                command: format!("task_output:{}:{}", task_id, summary),
                command_short: "task_output".to_string(),
                exit_code: 0,
                success: true,
                error_pattern: None,
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(),
            });
        tracing::debug!(
            task_id = task_id,
            len = output_text.len(),
            "task output persisted to knowledge DB"
        );
    }
    let _ = rt
        .ctx
        .knowledge
        .record_access("__task_sync_output__", task_id);

    // R143: Persist output summary to memory recall — backs up the format string claim.
    // The format string says "memory store 'task:{task_id}:output' auto-persisted" but only
    // record_bash_outcome was called (command history), making `touring memory recall` return nothing.
    // This makes `touring memory recall "task:<task_id>:output"` actually work cross-session.
    // Output is capped at 400 chars — longer than R12-S3 (200) to preserve more context.
    if !output_text.is_empty() {
        let output_snippet = &output_text[..output_text.len().min(400)];
        let _ = crate::cli_handlers::cli_memory_store(
            rt,
            &serde_json::json!({
                "key": format!("task:{}:output", task_id),
                "value": format!("task output for {task_id}: {output_snippet}"),
                "tier": "semantic",
                "entry_type": "lesson",
            }),
        );
    }

    // R16-S1: Upsert backtick-quoted symbols from output into Tantivy for BM25 searchability.
    // Makes task outputs findable via `touring tantivy search "<symbol>"`.
    // Feature-gated: only active when tantivy-fts is enabled (default ON).
    #[cfg(feature = "tantivy-fts")]
    let tantivy_hint = {
        let symbols = extract_backtick_symbols(output_text);
        if !symbols.is_empty() {
            if let Some(idx) = crate::tantivy_index::tantivy_for(Some(&rt.project_root)) {
                let docstring = output_text[..output_text.len().min(300)].to_string();
                for sym in &symbols {
                    let doc = crate::tantivy_index::SymbolDoc {
                        symbol_name: sym.clone(),
                        file_path: format!("task_output:{task_id}"),
                        symbol_kind: "task_output".to_string(),
                        module_path: Some(format!("task:{task_id}")),
                        docstring: Some(docstring.clone()),
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
                }
                let _ = idx.commit();
                tracing::debug!(
                    task_id = task_id,
                    count = symbols.len(),
                    "task output: backtick symbols upserted to Tantivy"
                );
                format!(
                    " | tantivy: {} symbol(s) indexed — run `touring tantivy search \"{task_id}\"` to find",
                    symbols.len()
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };
    #[cfg(not(feature = "tantivy-fts"))]
    let tantivy_hint = String::new();

    // R17-S2: Detect file paths in task output and refresh wiring map for each.
    // When a task output mentions modified files (e.g. "crates/foo/src/lib.rs"),
    // we proactively update wiring so orphan tracking stays current without waiting
    // for the next file-changed hook.
    let wiring_count = {
        let paths = extract_file_paths(output_text);
        let mut updated = 0usize;
        for path in &paths {
            crate::wiring::update_wiring_after_edit(&rt.ctx.knowledge, path);
            updated += 1;
        }
        updated
    };
    let wiring_hint = if wiring_count > 0 {
        format!(" | wiring: {wiring_count} file(s) re-verified")
    } else {
        String::new()
    };

    // R160: Auto-store artifact→file mapping when file paths detected in task output.
    // Closes the TaskOutput(file paths) → Touring memory → cross-session artifact recall loop.
    // Enables `touring memory recall "artifact:<task_id>:files"` to return exact files produced
    // by this task, without needing to re-read the full output or DAG state.
    // Complements R143 (raw output recall), R17-S2 (wiring update), and R135 (subject at complete).
    // Only fires when wiring_count > 0 (file paths were already detected by R17-S2).
    // Re-calls extract_file_paths (cheap pure fn) to avoid restructuring the wiring block.
    // Capped at 5 paths in the memory value to keep it concise (wiring handles all paths).
    if wiring_count > 0 {
        let artifact_paths = extract_file_paths(output_text);
        if !artifact_paths.is_empty() {
            let files_csv: String = artifact_paths
                .iter()
                .take(5)
                .map(|p| p.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let _ = crate::cli_handlers::cli_memory_store(
                rt,
                &serde_json::json!({
                    "key": format!("artifact:{task_id}:files"),
                    "value": format!("Files produced by task {task_id}: {files_csv}"),
                    "tier": "semantic",
                    "entry_type": "lesson",
                }),
            );
        }
    }

    // R24-S3: Keyword-map output text to a GeneratorKind — surfaces the right artifact type
    // without requiring the engineer to manually inspect the output and pick a generator.
    let gen_hint = generator_hint_from_output(output_text);

    // R27-S2 + R29-S1: Detect success OR failure signal — mutually exclusive (500-char window).
    // Extracted to `output_outcome_hint` to keep CC(handle_task_sync_post_output) ≤ 15.
    let outcome_hint = output_outcome_hint(rt, output_text, task_id);
    // R31-S2: Auto-advance validate subtask in Touring DAG when success signal confirmed.
    let dag_advance = advance_dag_validate_on_success(rt, &outcome_hint, task_id);
    // R32-S2: Auto-advance implement subtask when artifacts (file paths) detected in output.
    let impl_advance = advance_dag_implement_on_artifact(rt, wiring_count, task_id);
    // R34-S3: Detect plan.json or plan-submit in output → surface validate + submit commands.
    let plan_hint = maybe_plan_validate_from_output(output_text);
    // R35-S1: Detect Rust compilation errors → surface generate verify + render hints.
    // When TaskOutput contains `error[E...]`, close the generator loop immediately.
    let rust_error_hint = maybe_rust_error_generator_hint(output_text);
    // R40-S2: Map first file path in output to a GeneratorKind — closes artifact → scaffold loop.
    let artifact_gen = artifact_file_gen_hint(output_text);
    // R39-S1: Symmetric RL +0.5 injection when test pass confirmed — closes the asymmetry
    // where failure_signal_hint injects -0.1 but success path had no RL injection.
    let test_pass_rl = maybe_test_pass_rl_reward(rt, &outcome_hint, task_id);
    // R161: Auto-advance ::implement subtask when test pass confirmed — closes the gap where
    // R32-S2 (file-path-based ::implement advance) misses test-only outputs (no file paths).
    // When tests pass, the implementation is demonstrably complete, so ::implement must be terminal.
    // R31-S2 handles ::validate; R161 symmetrically handles ::implement on the same success signal.
    // Lifecycle invariant: after test pass, both ::implement AND ::validate are completed,
    // enabling cli_decompose_finalize to archive the DAG on the next TaskUpdate(completed).
    if outcome_hint.contains("✓ success") {
        let impl_id = format!("{task_id}::implement");
        let _ = crate::cli_handlers::cli_decompose_update(
            rt,
            &serde_json::json!({
                "task_id": task_id,
                "subtask_id": impl_id,
                "status": "completed",
                "priority": 5,
            }),
        );
    }
    // R42-S2: Emit AAAK diary lesson hint when test pass confirmed in output.
    // Complements R39-S3 (TaskUpdate completed) — captures lesson mid-task, not just at close.
    let diary_hint = maybe_diary_lesson_on_output_success(&outcome_hint, task_id);
    // R44-S3: When failure detected in output, suggest evolution drift analysis.
    // Closes the loop: failure signals → systemic degradation detection → self-correction.
    let drift_hint = maybe_evolution_drift_on_failure(&outcome_hint);
    // R47-S3: When diff/patch markers detected in output, suggest incremental_patch generator.
    // Bridges TaskOutput(diff) → touring-generator incremental_patch template.
    let patch_hint = maybe_incremental_patch_hint_on_output(output_text).unwrap_or_default();
    // R48-S1: When JSON/YAML schema markers detected, suggest schema generator.
    // Closes TaskOutput(schema) → schema.tera template → touring-generator commit loop.
    let schema_hint = maybe_schema_generator_hint_on_output(output_text).unwrap_or_default();
    // R49-S2: When CI/CD markers detected in output, suggest ci_workflow generator.
    // Closes TaskOutput(CI/CD) → ci_workflow.tera → touring-generator automation scaffold.
    let ci_hint = maybe_ci_workflow_hint_on_output(output_text).unwrap_or_default();
    // R50-S2: When SQL/migration markers detected in output, suggest migration generator.
    // Closes TaskOutput(schema change) → migration.tera → touring-generator versioned artifact.
    let migration_hint = maybe_migration_hint_on_output(output_text).unwrap_or_default();
    // R51-S2: When Docker/container markers detected in output, suggest dockerfile generator.
    // Closes TaskOutput(container work) → dockerfile.tera → touring-generator container scaffold.
    let docker_hint = maybe_dockerfile_hint_on_output(output_text).unwrap_or_default();
    // R52-S2: When Kubernetes markers detected in output, suggest k8s_manifest generator.
    // Closes TaskOutput(k8s deploy) → k8s_manifest.tera → touring-generator manifest scaffold.
    let k8s_hint = maybe_k8s_manifest_hint_on_output(output_text).unwrap_or_default();
    // R53-S2: When Rust code markers detected in output, suggest rust_module generator.
    // Closes TaskOutput(Rust code) → rust_module.tera → touring-generator module scaffold.
    let rust_module_hint = maybe_rust_module_hint_on_output(output_text).unwrap_or_default();
    // R54-S2: When test code markers detected in output, suggest test generator.
    // Closes TaskOutput(test code) → test.tera → touring-generator test scaffold.
    let test_hint = maybe_test_hint_on_output(output_text).unwrap_or_default();
    // R55-S2: When Python code markers detected in output, suggest python_script generator.
    // Closes TaskOutput(Python code) → python_script.tera → touring-generator script scaffold.
    let python_hint = maybe_python_script_hint_on_output(output_text).unwrap_or_default();
    // R56-S2: When FFI markers detected in output, suggest ffi_binding generator.
    // Closes TaskOutput(FFI code) → ffi_binding.tera → touring-generator binding scaffold.
    let ffi_hint = maybe_ffi_binding_hint_on_output(output_text).unwrap_or_default();
    // R57-S2: When changelog/release markers detected in output, suggest changelog_entry generator.
    // Closes TaskOutput(release notes) → changelog_entry.tera → touring-generator release scaffold.
    let changelog_hint = maybe_changelog_hint_on_output(output_text).unwrap_or_default();
    // R60-S2: When shell completion markers detected in output, suggest shell_completion generator.
    // Closes TaskOutput(completion script) → shell_completion.tera → touring-generator scaffold.
    let shell_completion_hint =
        maybe_shell_completion_hint_on_output(output_text).unwrap_or_default();
    // R65-S1: When benchmark/criterion markers detected in output, suggest benchmark generator.
    // Closes TaskOutput(perf work) → benchmark.tera → touring-generator Criterion scaffold.
    let benchmark_hint = maybe_benchmark_hint_on_output(output_text).unwrap_or_default();
    // R65-S2: When fuzz/proptest markers detected in output, suggest fuzz_target generator.
    // Closes TaskOutput(fuzzing) → fuzz_target.tera → touring-generator libFuzzer scaffold.
    let fuzz_hint = maybe_fuzz_target_hint_on_output(output_text).unwrap_or_default();
    // R65-S3: When proc-macro/derive markers detected in output, suggest derive_macro generator.
    // Closes TaskOutput(proc-macro) → derive_macro.tera → touring-generator derive scaffold.
    let derive_macro_hint = maybe_derive_macro_hint_on_output(output_text).unwrap_or_default();
    // R68-S1: When AsyncAPI/event-driven markers detected in output, suggest asyncapi_spec generator.
    // Closes TaskOutput(event-driven API) → asyncapi_spec.tera → touring-generator AsyncAPI scaffold.
    let asyncapi_hint = maybe_asyncapi_hint_on_output(output_text).unwrap_or_default();
    // R68-S2: When architecture-decision markers detected in output, suggest adr generator.
    // Closes TaskOutput(design decision) → adr.tera → touring-generator ADR scaffold.
    let adr_output_hint = maybe_adr_hint_on_output(output_text).unwrap_or_default();
    // R68-S3: When man-page/groff markers detected in output, suggest man_page generator.
    // Closes TaskOutput(man page work) → man_page.tera → touring-generator Unix man page scaffold.
    let man_page_output_hint = maybe_man_page_hint_on_output(output_text).unwrap_or_default();
    // R69-S1: When error-catalog markers detected in output, suggest error_catalog generator.
    // Closes TaskOutput(thiserror/error enum) → error_catalog.tera → touring-generator scaffold.
    let error_catalog_output_hint =
        maybe_error_catalog_hint_on_output(output_text).unwrap_or_default();
    // R69-S2: When decompose/DAG markers detected in output, suggest task_scaffold generator.
    // Closes TaskOutput(touring decompose) → task_scaffold.tera → touring-generator DAG scaffold.
    let task_scaffold_output_hint =
        maybe_task_scaffold_hint_on_output(output_text).unwrap_or_default();
    // R69-S3: When diary/lesson markers detected in output, suggest diary_entry generator.
    // Closes TaskOutput(lesson learned) → diary_entry.tera → touring-generator AAAK diary scaffold.
    let diary_entry_output_hint = maybe_diary_entry_hint_on_output(output_text).unwrap_or_default();
    // R98-S1..S3: openapi/hook_handler/mcp_tool — API/hook/MCP output markers surface scaffold hints.
    let openapi_output_hint = maybe_openapi_hint_on_output(output_text).unwrap_or_default();
    let hook_handler_output_hint =
        maybe_hook_handler_hint_on_output(output_text).unwrap_or_default();
    let mcp_tool_output_hint = maybe_mcp_tool_hint_on_output(output_text).unwrap_or_default();
    // R99-S1..S3: terraform_module/cli_handler/plan_md — IaC/CLI/plan output markers surface scaffold hints.
    let terraform_output_hint = maybe_terraform_hint_on_output(output_text).unwrap_or_default();
    let cli_handler_output_hint = maybe_cli_handler_hint_on_output(output_text).unwrap_or_default();
    let plan_md_output_hint = maybe_plan_md_hint_on_output(output_text).unwrap_or_default();
    // R100-S1..S3: skill_document/protobuf_schema/consumer_generator — TaskOutput 30/30 COMPLETE.
    let skill_document_output_hint =
        maybe_skill_document_hint_on_output(output_text).unwrap_or_default();
    let protobuf_schema_output_hint =
        maybe_protobuf_schema_hint_on_output(output_text).unwrap_or_default();
    let consumer_generator_output_hint =
        maybe_consumer_generator_hint_on_output(output_text).unwrap_or_default();

    // R155: RL reward when Tantivy symbols were indexed from task output — closes
    // the TaskOutput → symbol extraction → Tantivy → RL feedback loop.
    // tantivy_hint is non-empty only when symbols were extracted and committed to the BM25 index.
    // Reward +0.1 signals that output produced searchable knowledge, distinct from test_pass_rl (+1.0).
    if !tantivy_hint.is_empty() {
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "orchestrate",
                "reward_value": 0.1,
                "context": format!("task_output:tantivy_indexed:{task_id}"),
            }),
        );
    }

    format!(
        "touring-sync: task {task_id} output captured — \
        memory store \"task:{task_id}:output\" auto-persisted{tantivy_hint}{wiring_hint}{gen_hint}{outcome_hint}{dag_advance}{impl_advance}{plan_hint}{rust_error_hint}{artifact_gen}{test_pass_rl}{diary_hint}{drift_hint}{patch_hint}{schema_hint}{ci_hint}{migration_hint}{docker_hint}{k8s_hint}{rust_module_hint}{test_hint}{python_hint}{ffi_hint}{changelog_hint}{shell_completion_hint}{benchmark_hint}{fuzz_hint}{derive_macro_hint}{asyncapi_hint}{adr_output_hint}{man_page_output_hint}{error_catalog_output_hint}{task_scaffold_output_hint}{diary_entry_output_hint}{openapi_output_hint}{hook_handler_output_hint}{mcp_tool_output_hint}{terraform_output_hint}{cli_handler_output_hint}{plan_md_output_hint}{skill_document_output_hint}{protobuf_schema_output_hint}{consumer_generator_output_hint} | \
        run `touring wiring orphans -j` if output produced new artifacts"
    )
}

/// R29-S1 + R27-S2: Detect success XOR failure signals in task output (CC≤3).
///
/// Tries `completion_signal_hint` first (success path — RL+1.0 hint).
/// Falls back to `failure_signal_hint` when no success was detected (failure path — RL-0.1).
/// Mutual exclusion: a 500-char cargo test output contains either ok OR FAILED, not both.
/// Extracted from `handle_task_sync_post_output` so its CC stays ≤ 15 (R29-S1 fix).
pub(crate) fn output_outcome_hint(rt: &mut HookRuntime, output: &str, task_id: &str) -> String {
    let completion = completion_signal_hint(output, task_id);
    if !completion.is_empty() {
        return completion;
    }
    failure_signal_hint(rt, output, task_id)
}

/// R31-S2: Auto-advance the `{task_id}::validate` subtask when success is confirmed (CC≤2).
///
/// Called after `output_outcome_hint` — if the outcome hint contains the success marker
/// ("✓ success"), the validate subtask in the Touring DAG is automatically marked completed
/// via `cli_decompose_update`. Closes the TaskOutput(success) → Touring DAG completion loop
/// without requiring a manual `touring decompose update` call.
///
/// Returns a formatted confirmation hint when the advance succeeds, empty string otherwise.
pub(crate) fn advance_dag_validate_on_success(
    rt: &mut HookRuntime,
    outcome_hint: &str,
    task_id: &str,
) -> String {
    if !outcome_hint.contains("✓ success") {
        return String::new();
    }
    let validate_subtask = format!("{task_id}::validate");
    // R165: Fix subtask branch activation — `priority: 5` required for cli_decompose_update to
    // enter the subtask SQL branch (line 1278: `if priority.is_some() || quality_score.is_some()`).
    // Prior payload {task_id: validate_subtask} without priority was a no-op: it tried to update
    // task_decompositions WHERE task_id = "T-123::validate" (no match) and skipped the subtask
    // branch (priority.is_none()). Now correctly uses {task_id, subtask_id, priority: 5}.
    let payload = serde_json::json!({
        "task_id": task_id,
        "subtask_id": &validate_subtask,
        "status": "completed",
        "priority": 5,
    });
    let result = crate::cli_handlers::cli_decompose_update(rt, &payload);
    if result.contains("\"subtask_updated\":true") {
        format!(" | dag-auto: `{validate_subtask}` marked completed in Touring DAG")
    } else {
        format!(" | dag-auto: `{validate_subtask}` advance attempted (subtask may not exist yet)")
    }
}

/// R32-S2: Auto-advance `{task_id}::implement` in Touring DAG when artifacts are detected (CC=2).
///
/// When `artifact_count > 0` (file paths extracted from task output by `extract_file_paths`),
/// marks the `{task_id}::implement` subtask as `completed` via `cli_decompose_update`.
/// This closes the TaskOutput(artifact) → Touring DAG implement-stage loop without requiring
/// a manual `touring decompose update` call.
///
/// Returns a formatted confirmation hint when artifacts were found; empty string otherwise.
pub(crate) fn advance_dag_implement_on_artifact(
    rt: &mut HookRuntime,
    artifact_count: usize,
    task_id: &str,
) -> String {
    if artifact_count == 0 {
        return String::new();
    }
    let implement_subtask = format!("{task_id}::implement");
    // R165: Fix subtask branch activation — same bug as R31-S2 (advance_dag_validate_on_success).
    // Prior payload {task_id: implement_subtask} without priority was a no-op: updated no row in
    // task_decompositions (wrong task_id) and skipped the subtask branch (priority.is_none()).
    // Now correctly uses {task_id, subtask_id, priority: 5} to trigger the subtask SQL branch.
    let payload = serde_json::json!({
        "task_id": task_id,
        "subtask_id": &implement_subtask,
        "status": "completed",
        "priority": 5,
    });
    let _ = crate::cli_handlers::cli_decompose_update(rt, &payload);
    format!(
        " | dag-auto: `{implement_subtask}` marked completed ({artifact_count} artifact(s) detected)"
    )
}

/// R29-S1: Detect failure signals in task output → inject RL -0.1 + memory lesson (CC≤5).
///
/// Scans the first 500 chars for failure patterns commonly produced by Rust tooling:
/// - "test result: failed" — cargo test failure
/// - "panicked at" — Rust panic (including assertion failures in tests)
/// - "error[e" — Rust compile error
/// - "process didn't exit successfully" — non-zero exit code from cargo run
/// - "failures:" combined with "failed" — cargo test failure summary block
///
/// Side effects: injects `cli_learning_reward(-0.1)` + `cli_memory_store(lesson)`.
/// Returns a formatted hint pointing to debugging commands.
pub(crate) fn failure_signal_hint(rt: &mut HookRuntime, output: &str, task_id: &str) -> String {
    if output.is_empty() {
        return String::new();
    }
    let window = output[..output.len().min(500)].to_lowercase();
    let is_failed = window.contains("test result: failed")
        || window.contains("panicked at")
        || window.contains("error[e")
        || window.contains("process didn't exit successfully")
        || (window.contains("failures:") && window.contains("failed"));
    if !is_failed {
        return String::new();
    }
    let _ = crate::cli_handlers::cli_learning_reward(
        rt,
        &serde_json::json!({
            "tool": "orchestrate",
            "reward": -0.1_f64,
            "context": format!("task:{task_id}:output-failed"),
        }),
    );
    let _ = crate::cli_handlers::cli_memory_store(
        rt,
        &serde_json::json!({
            "key": format!("task:{task_id}:output-failed"),
            "value": format!("Task {task_id} produced failure output — investigate root cause | run `touring memory recall \"task:{task_id}\"` for history"),
            "tier": "semantic",
            "entry_type": "lesson",
        }),
    );
    // R166: Advance ::validate subtask to failed — closes the symmetric gap with R31-S2/R165.
    // R31-S2/R165: test pass → ::validate = completed.
    // R166: test fail → ::validate = failed (so cli_decompose_finalize accounts for failures).
    // Without R166: ::validate stays pending after failure, blocking finalize archive.
    // Uses the same corrected payload format as R165 (task_id=parent, subtask_id=path, priority=5).
    let validate_subtask = format!("{task_id}::validate");
    let _ = crate::cli_handlers::cli_decompose_update(
        rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": &validate_subtask,
            "status": "failed",
            "priority": 5,
        }),
    );
    format!(
        " | ✗ failure detected — RL -0.1 injected | lesson stored | {validate_subtask} marked failed | \
        run `touring tantivy search \"{task_id}\"` to find affected symbols"
    )
}

/// R35-S1: Detect Rust compilation errors in task output and surface VGP verify + generator hint (CC=4).
///
/// When task output contains `error[E` (Rust compiler error format), the engineer needs to:
/// 1. Verify the erring symbol exists in the index (VGP)
/// 2. Use the appropriate generator to produce a fix
///
/// Scans the first 400 chars of output for error codes:
/// - `error[E0425]` / `error[E0412]` → undefined symbol → `touring generate verify --symbol`
/// - `error[E0308]` → type mismatch → `RustModule` generator
/// - Any other `error[E` → generic `plan-suggest` scaffold
///
/// Returns empty string when no compilation errors are detected.
pub(crate) fn maybe_rust_error_generator_hint(output_text: &str) -> String {
    let window = &output_text[..output_text.len().min(400)];
    if !window.contains("error[E") {
        return String::new();
    }
    if window.contains("error[E0425]") || window.contains("error[E0412]") {
        return " | rust-error: undefined symbol — run `touring generate verify --symbol <name>` to check index | then `touring generate render RustModule` to scaffold fix".to_string();
    }
    if window.contains("error[E0308]") {
        return " | rust-error: type mismatch — run `touring generate render RustModule --vars '{\"module_name\":\"fix\"}'` to scaffold correction".to_string();
    }
    " | rust-error: compilation failure — run `touring generate plan-suggest --intent \"fix compilation error\"` to scaffold resolution".to_string()
}

/// R44-S3: Emit `touring evolution drift -j` hint when task output signals failure (CC≤2).
///
/// Repeated test failures can indicate structural drift — degrading quality metrics, increasing
/// error rates, or decoupled modules. When `failure_signal_hint` fires (outcome contains the
/// failure marker), this helper additionally surfaces `touring evolution drift -j` so the
/// engineer can run drift analysis to distinguish a one-off bug from a systemic regression.
/// Returns empty string when outcome_hint does not contain the failure marker.
pub(crate) fn maybe_evolution_drift_on_failure(outcome_hint: &str) -> String {
    if !outcome_hint.contains("✗ failure detected") {
        return String::new();
    }
    " | evolution-drift: run `touring evolution drift -j` to detect systemic degradation pattern"
        .to_string()
}

/// R47-S3: When task output contains diff/patch indicators, suggest `incremental_patch` generator (CC=2).
///
/// Detects standard unified diff markers ("diff --git", "--- a/", "+++ b/", "@@ ") in task output.
/// When detected, surfaces `touring generate render incremental_patch` so Claude Code can scaffold
/// a structured patch artifact via the generator pipeline rather than applying diffs manually.
/// Closes the loop: TaskOutput(diff) → incremental_patch template → touring-generator commit.
/// Returns `None` when no diff patterns are found — avoids noise on non-patch outputs.
pub(crate) fn maybe_incremental_patch_hint_on_output(output_text: &str) -> Option<String> {
    const DIFF_MARKERS: &[&str] = &["diff --git", "--- a/", "+++ b/", "@@ "];
    let has_diff = DIFF_MARKERS
        .iter()
        .any(|marker| output_text.contains(marker));
    if !has_diff {
        return None;
    }
    Some(
        " | incremental-patch: diff detected — run `touring generate render incremental_patch` \
        to scaffold a structured patch artifact via touring-generator"
            .to_string(),
    )
}

/// R48-S1: Detect JSON/YAML schema patterns in task output and suggest schema generator (CC=2).
///
/// When task output contains standard JSON Schema markers (`"$schema"`, `"properties":`, etc.),
/// surfaces `touring generate render schema` so Claude Code can scaffold a formal schema artifact
/// via the generator pipeline. Closes the loop: TaskOutput(schema) → schema.tera → generator commit.
/// Returns `None` when no schema markers are found — avoids noise for non-schema outputs.
pub(crate) fn maybe_schema_generator_hint_on_output(output_text: &str) -> Option<String> {
    const SCHEMA_MARKERS: &[&str] = &[
        "\"$schema\"",
        "\"type\": \"object\"",
        "\"properties\":",
        "definitions:",
        "\"required\":",
    ];
    let has_schema = SCHEMA_MARKERS.iter().any(|m| output_text.contains(m));
    if !has_schema {
        return None;
    }
    Some(
        " | schema-gen: JSON/YAML schema detected — run `touring generate render schema` \
        to scaffold a formal schema artifact via touring-generator"
            .to_string(),
    )
}

/// R49-S2: Detect CI/CD keywords in task output and suggest `ci_workflow` generator (CC=2).
///
/// When TaskOutput contains keywords like "github actions", "dockerfile", "pipeline",
/// "ci:", "cd:", or "workflow", surfaces `touring generate render ci_workflow` so
/// Claude Code scaffolds the automation artifact immediately.
/// Returns `None` when output is empty or contains no CI/CD markers.
pub(crate) fn maybe_ci_workflow_hint_on_output(output_text: &str) -> Option<String> {
    const CI_MARKERS: &[&str] = &[
        "github actions",
        "dockerfile",
        "pipeline",
        "ci:",
        "cd:",
        "workflow",
        ".github/",
        "jenkins",
        "circleci",
        "travis",
        "github_actions",
    ];
    let lower = output_text.to_lowercase();
    let has_ci = CI_MARKERS.iter().any(|m| lower.contains(m));
    if !has_ci {
        return None;
    }
    Some(
        " | ci-workflow: CI/CD markers detected — run `touring generate render ci_workflow` \
        to scaffold automation pipeline via touring-generator"
            .to_string(),
    )
}

/// R50-S2: Detect SQL/migration keywords in task output and suggest `migration` generator (CC=2).
///
/// When TaskOutput contains CREATE TABLE, ALTER TABLE, migration, flyway, liquibase,
/// or schema change markers, surfaces `touring generate render migration` so Claude Code
/// captures the schema evolution as a versioned migration artifact.
/// Returns `None` when output is empty or contains no SQL/migration markers.
pub(crate) fn maybe_migration_hint_on_output(output_text: &str) -> Option<String> {
    const SQL_MARKERS: &[&str] = &[
        "create table",
        "alter table",
        "drop table",
        "migration",
        "flyway",
        "liquibase",
        "schema change",
        "add column",
        "drop column",
    ];
    let lower = output_text.to_lowercase();
    let has_sql = SQL_MARKERS.iter().any(|m| lower.contains(m));
    if !has_sql {
        return None;
    }
    Some(
        " | migration: SQL/schema change detected — run `touring generate render migration` \
        to scaffold a versioned migration artifact via touring-generator"
            .to_string(),
    )
}

/// R51-S2: Detect Docker/container keywords in task output and suggest `dockerfile` generator (CC=2).
///
/// When TaskOutput contains docker, container, image, compose, kubernetes markers,
/// surfaces `touring generate render dockerfile` so Claude Code scaffolds the
/// container definition immediately after detecting container-related work.
/// Returns `None` when output is empty or contains no container markers.
pub(crate) fn maybe_dockerfile_hint_on_output(output_text: &str) -> Option<String> {
    const DOCKER_MARKERS: &[&str] = &[
        "docker",
        "container",
        "dockerfile",
        "docker-compose",
        "compose",
        "kubernetes",
        "k8s",
        "podman",
        "image build",
    ];
    let lower = output_text.to_lowercase();
    let has_docker = DOCKER_MARKERS.iter().any(|m| lower.contains(m));
    if !has_docker {
        return None;
    }
    Some(
        " | dockerfile: container markers detected — run `touring generate render dockerfile` \
        to scaffold container definition via touring-generator"
            .to_string(),
    )
}

/// R56-S2: Detect FFI-specific markers in task output and suggest `ffi_binding` generator (CC=2).
///
/// When TaskOutput contains FFI markers like `extern "C"`, `#[no_mangle]`, `unsafe extern`,
/// `libc::`, `c_void`, or `bindgen`, surfaces `touring generate render ffi_binding` so
/// Claude Code scaffolds a safe FFI binding wrapper immediately after FFI code appears.
/// Closes the loop: TaskOutput(FFI code) → ffi_binding.tera → touring-generator.
/// Returns `None` when output is empty or contains no FFI-specific markers.
pub(crate) fn maybe_ffi_binding_hint_on_output(output_text: &str) -> Option<String> {
    const FFI_MARKERS: &[&str] = &[
        "extern \"C\"",
        "#[no_mangle]",
        "unsafe extern",
        "libc::",
        "c_void",
        "bindgen",
    ];
    let has_ffi = FFI_MARKERS.iter().any(|m| output_text.contains(m));
    if !has_ffi {
        return None;
    }
    Some(
        " | ffi-binding: FFI code markers detected — run `touring generate render ffi_binding` \
        to scaffold safe FFI binding wrapper via touring-generator"
            .to_string(),
    )
}

/// R60-S2: Detect shell completion markers in task output and suggest `shell_completion` (CC=2).
///
/// When TaskOutput contains shell completion markers like `complete -`, `compdef`, `_arguments`,
/// `zsh completion`, `bash completion`, `fish completion`, or `--completion` keywords,
/// surfaces `touring generate render shell_completion` so Claude Code scaffolds a shell
/// completion script immediately after detecting completion-related output.
/// Closes the loop: TaskOutput(completion markers) → shell_completion.tera → touring-generator.
/// Returns `None` when output is empty or contains no shell completion markers.
pub(crate) fn maybe_shell_completion_hint_on_output(output_text: &str) -> Option<String> {
    const COMPLETION_MARKERS: &[&str] = &[
        "complete -",
        "compdef ",
        "_arguments",
        "zsh completion",
        "bash completion",
        "fish completion",
        "--completion",
        "shell-completion",
    ];
    let has_completion = COMPLETION_MARKERS.iter().any(|m| output_text.contains(m));
    if !has_completion {
        return None;
    }
    Some(
        " | shell-completion: completion markers detected — run `touring generate render shell_completion` \
        to scaffold shell completion script via touring-generator"
        .to_string(),
    )
}

/// R57-S2: Detect changelog/release-notes markers in task output and suggest `changelog_entry` (CC=2).
///
/// When TaskOutput contains changelog, CHANGELOG, release notes, release version, bump version,
/// semver, or ## [Unreleased] markers, surfaces `touring generate render changelog_entry` so
/// Claude Code scaffolds a structured changelog entry immediately after release-related work.
/// Closes the loop: TaskOutput(release markers) → changelog_entry.tera → touring-generator.
/// Returns `None` when output is empty or contains no changelog-related markers.
pub(crate) fn maybe_changelog_hint_on_output(output_text: &str) -> Option<String> {
    const CHANGELOG_MARKERS: &[&str] = &[
        "CHANGELOG",
        "changelog",
        "release notes",
        "release version",
        "bump version",
        "## [Unreleased]",
        "## [",
        "semver",
    ];
    let has_changelog = CHANGELOG_MARKERS.iter().any(|m| output_text.contains(m));
    if !has_changelog {
        return None;
    }
    Some(
        " | changelog-entry: release markers detected — run `touring generate render changelog_entry` \
        to scaffold structured changelog entry via touring-generator"
        .to_string(),
    )
}

/// R55-S2: Detect Python-specific markers in task output and suggest `python_script` generator (CC=2).
///
/// When TaskOutput contains Python-specific markers like `if __name__`, `import asyncio`,
/// `@dataclass`, `argparse`, `from typing import`, or `#!/usr/bin/env python`, surfaces
/// `touring generate render python_script` so Claude Code scaffolds a Python script module.
/// Closes the loop: TaskOutput(Python code) → python_script.tera → touring-generator.
/// Returns `None` when output is empty or contains no Python-specific markers.
pub(crate) fn maybe_python_script_hint_on_output(output_text: &str) -> Option<String> {
    const PYTHON_MARKERS: &[&str] = &[
        "if __name__",
        "import asyncio",
        "@dataclass",
        "argparse.ArgumentParser",
        "from typing import",
        "#!/usr/bin/env python",
        "@pytest.fixture",
    ];
    let has_python = PYTHON_MARKERS.iter().any(|m| output_text.contains(m));
    if !has_python {
        return None;
    }
    Some(
        " | python-script: Python code markers detected — run `touring generate render python_script` \
        to scaffold a Python script via touring-generator"
        .to_string(),
    )
}

/// R54-S2: Detect test-code markers in task output and suggest `test` generator (CC=2).
///
/// When TaskOutput contains test-specific markers like `#[test]`, `fn test_`, `assert_eq!`,
/// `#[cfg(test)]`, `pytest`, or `describe(`, surfaces `touring generate render test` so
/// Claude Code scaffolds a dedicated test module immediately after test code appears.
/// Closes the loop: TaskOutput(test code) → test.tera → touring-generator test scaffold.
/// Returns `None` when output is empty or contains no test-specific markers.
pub(crate) fn maybe_test_hint_on_output(output_text: &str) -> Option<String> {
    const TEST_MARKERS: &[&str] = &[
        "#[test]",
        "fn test_",
        "assert_eq!",
        "assert_ne!",
        "#[cfg(test)]",
        "pytest",
        "describe(",
        "it!(",
        "#[tokio::test]",
    ];
    let has_test = TEST_MARKERS.iter().any(|m| output_text.contains(m));
    if !has_test {
        return None;
    }
    Some(
        " | test-scaffold: test code markers detected — run `touring generate render test` \
        to scaffold a test module via touring-generator"
            .to_string(),
    )
}

/// R53-S2: Detect Rust-specific markers in task output and suggest `rust_module` generator (CC=2).
///
/// When TaskOutput contains Rust-specific markers like `pub struct`, `pub fn`, `impl `, `#[derive`,
/// `mod `, or `use crate::`, surfaces `touring generate render rust_module` so Claude Code
/// scaffolds a new Rust module scaffold immediately after Rust code appears in task output.
/// Closes the loop: TaskOutput(Rust code) → rust_module.tera → touring-generator module scaffold.
/// Returns `None` when output is empty or contains no Rust-specific markers.
pub(crate) fn maybe_rust_module_hint_on_output(output_text: &str) -> Option<String> {
    const RUST_MARKERS: &[&str] = &[
        "pub struct ",
        "pub fn ",
        "pub trait ",
        "impl ",
        "#[derive",
        "use crate::",
        "mod ",
        "pub enum ",
        "async fn ",
    ];
    let has_rust = RUST_MARKERS.iter().any(|m| output_text.contains(m));
    if !has_rust {
        return None;
    }
    Some(
        " | rust-module: Rust code markers detected — run `touring generate render rust_module` \
        to scaffold a new module via touring-generator"
            .to_string(),
    )
}

/// R52-S2: Detect Kubernetes-specific markers in task output and suggest `k8s_manifest` generator (CC=2).
///
/// When TaskOutput contains `kubectl`, `helm`, `deployment:`, `configmap`, `namespace:`,
/// `serviceaccount`, or `kind: ` (k8s manifest key), surfaces `touring generate render k8s_manifest`
/// so Claude Code scaffolds the Kubernetes manifest immediately.
/// Distinct from `maybe_dockerfile_hint_on_output` which targets container build, not orchestration.
/// Returns `None` when output is empty or contains no k8s-specific markers.
pub(crate) fn maybe_k8s_manifest_hint_on_output(output_text: &str) -> Option<String> {
    const K8S_MARKERS: &[&str] = &[
        "kubectl",
        "helm install",
        "helm upgrade",
        "deployment:",
        "configmap",
        "namespace:",
        "serviceaccount",
        "kind: deployment",
        "kind: service",
    ];
    let lower = output_text.to_lowercase();
    let has_k8s = K8S_MARKERS.iter().any(|m| lower.contains(m));
    if !has_k8s {
        return None;
    }
    Some(
        " | k8s-manifest: Kubernetes markers detected — run `touring generate render k8s_manifest` \
        to scaffold manifest via touring-generator"
            .to_string(),
    )
}

/// R65-S1: Detect benchmark/criterion markers in task output → suggest benchmark generator (CC≤2).
///
/// Keywords that signal the output is related to performance benchmarking:
/// criterion, bench, bencher, benchmark, flamegraph, perf, hyperfine, divan.
/// Returns a hint to run `touring generate render benchmark` via touring-generator.
pub(crate) fn maybe_benchmark_hint_on_output(output_text: &str) -> Option<String> {
    const BENCH_MARKERS: &[&str] = &[
        "criterion",
        "bencher",
        "benchmark",
        "flamegraph",
        "hyperfine",
        "divan",
        "cargo bench",
        "#[bench]",
        "benchmarking",
    ];
    let lower = output_text.to_lowercase();
    let has_bench = BENCH_MARKERS.iter().any(|m| lower.contains(m));
    if !has_bench {
        return None;
    }
    Some(
        " | benchmark: performance markers detected — run `touring generate render benchmark` \
        to scaffold a Criterion benchmark via touring-generator"
            .to_string(),
    )
}

/// R65-S2: Detect fuzz/proptest markers in task output → suggest fuzz_target generator (CC≤2).
///
/// Keywords that signal fuzzing or property-based testing activity:
/// libfuzzer, cargo-fuzz, fuzz_target, proptest, arbitrary, honggfuzz, afl.
/// Returns a hint to run `touring generate render fuzz_target` via touring-generator.
pub(crate) fn maybe_fuzz_target_hint_on_output(output_text: &str) -> Option<String> {
    const FUZZ_MARKERS: &[&str] = &[
        "libfuzzer",
        "cargo fuzz",
        "cargo-fuzz",
        "fuzz_target",
        "proptest",
        "arbitrary",
        "honggfuzz",
        "afl-fuzz",
        "#[fuzz]",
    ];
    let lower = output_text.to_lowercase();
    let has_fuzz = FUZZ_MARKERS.iter().any(|m| lower.contains(m));
    if !has_fuzz {
        return None;
    }
    Some(
        " | fuzz-target: fuzzing markers detected — run `touring generate render fuzz_target` \
        to scaffold a libFuzzer fuzz target via touring-generator"
            .to_string(),
    )
}

/// R65-S3: Detect proc-macro/derive markers in task output → suggest derive_macro generator (CC≤2).
///
/// Keywords that signal derive macro or procedural macro development:
/// proc_macro, derive macro, #[proc_macro_derive], syn::derive, quote::quote, proc-macro2.
/// Returns a hint to run `touring generate render derive_macro` via touring-generator.
pub(crate) fn maybe_derive_macro_hint_on_output(output_text: &str) -> Option<String> {
    const DERIVE_MARKERS: &[&str] = &[
        "proc_macro",
        "proc-macro2",
        "proc_macro_derive",
        "derive macro",
        "#[proc_macro",
        "syn::derive",
        "quote::quote",
        "macros crate",
    ];
    let lower = output_text.to_lowercase();
    let has_derive = DERIVE_MARKERS.iter().any(|m| lower.contains(m));
    if !has_derive {
        return None;
    }
    Some(
        " | derive-macro: proc-macro markers detected — run `touring generate render derive_macro` \
        to scaffold a derive procedural macro via touring-generator"
            .to_string(),
    )
}

/// R69-S1: Detect error-catalog markers in task output → suggest error_catalog generator (CC≤2).
///
/// Keywords: error catalog, thiserror, error enum, error registry, error variants, custom error codes.
/// Returns a hint to run `touring generate render error_catalog` via touring-generator.
pub(crate) fn maybe_error_catalog_hint_on_output(output_text: &str) -> Option<String> {
    const ERROR_CATALOG_MARKERS: &[&str] = &[
        "error catalog",
        "thiserror",
        "error enum",
        "error registry",
        "error variants",
        "custom error codes",
        "error_catalog",
        "#[error(",
    ];
    let lower = output_text.to_lowercase();
    let has_errors = ERROR_CATALOG_MARKERS.iter().any(|m| lower.contains(m));
    if !has_errors {
        return None;
    }
    Some(
        " | error-catalog: error-type markers detected — run `touring generate render error_catalog` \
        to scaffold an error catalog via touring-generator"
            .to_string(),
    )
}

/// R69-S2: Detect task-scaffold markers in task output → suggest task_scaffold generator (CC≤2).
///
/// Keywords: touring decompose, dag scaffold, task scaffold, taco task, subtask dag, decompose create.
/// Returns a hint to run `touring generate render task_scaffold` via touring-generator.
pub(crate) fn maybe_task_scaffold_hint_on_output(output_text: &str) -> Option<String> {
    const TASK_SCAFFOLD_MARKERS: &[&str] = &[
        "touring decompose",
        "dag scaffold",
        "task scaffold",
        "taco task",
        "subtask dag",
        "decompose create",
        "task_scaffold",
        "decompose add",
    ];
    let lower = output_text.to_lowercase();
    let has_scaffold = TASK_SCAFFOLD_MARKERS.iter().any(|m| lower.contains(m));
    if !has_scaffold {
        return None;
    }
    Some(
        " | task-scaffold: decompose markers detected — run `touring generate render task_scaffold` \
        to scaffold a TACO task DAG via touring-generator"
            .to_string(),
    )
}

/// R69-S3: Detect diary/lesson markers in task output → suggest diary_entry generator (CC≤2).
///
/// Keywords: touring diary, lesson learned, retrospective, postmortem, aaak format, diary write.
/// Returns a hint to run `touring generate render diary_entry` via touring-generator.
pub(crate) fn maybe_diary_entry_hint_on_output(output_text: &str) -> Option<String> {
    const DIARY_MARKERS: &[&str] = &[
        "touring diary",
        "lesson learned",
        "retrospective",
        "postmortem",
        "aaak format",
        "diary write",
        "diary entry",
        "#[p:phase]",
    ];
    let lower = output_text.to_lowercase();
    let has_diary = DIARY_MARKERS.iter().any(|m| lower.contains(m));
    if !has_diary {
        return None;
    }
    Some(
        " | diary-entry: lesson markers detected — run `touring generate render diary_entry` \
        to scaffold an AAAK diary entry via touring-generator"
            .to_string(),
    )
}

/// R68-S1: Detect AsyncAPI/event-driven markers in task output → suggest asyncapi_spec (CC≤2).
///
/// Keywords: asyncapi, amqp, event-driven api, message broker, kafka topic, pubsub schema.
/// Returns a hint to run `touring generate render asyncapi_spec` via touring-generator.
pub(crate) fn maybe_asyncapi_hint_on_output(output_text: &str) -> Option<String> {
    const ASYNCAPI_MARKERS: &[&str] = &[
        "asyncapi",
        "amqp",
        "event-driven api",
        "event driven api",
        "message broker",
        "kafka topic",
        "pubsub schema",
        "async api spec",
    ];
    let lower = output_text.to_lowercase();
    let has_asyncapi = ASYNCAPI_MARKERS.iter().any(|m| lower.contains(m));
    if !has_asyncapi {
        return None;
    }
    Some(
        " | asyncapi: event-driven API markers detected — run `touring generate render asyncapi_spec` \
        to scaffold an AsyncAPI specification via touring-generator"
            .to_string(),
    )
}

/// R68-S2: Detect architecture-decision markers in task output → suggest adr generator (CC≤2).
///
/// Keywords: architecture decision, adr, design decision, architectural record, tech tradeoff.
/// Returns a hint to run `touring generate render adr` via touring-generator.
pub(crate) fn maybe_adr_hint_on_output(output_text: &str) -> Option<String> {
    const ADR_MARKERS: &[&str] = &[
        "architecture decision",
        "adr",
        "design decision",
        "architectural record",
        "tech tradeoff",
        "technical tradeoff",
        "decision record",
        "madr",
    ];
    let lower = output_text.to_lowercase();
    let has_adr = ADR_MARKERS.iter().any(|m| lower.contains(m));
    if !has_adr {
        return None;
    }
    Some(
        " | adr: architecture-decision markers detected — run `touring generate render adr` \
        to scaffold an Architecture Decision Record via touring-generator"
            .to_string(),
    )
}

/// R68-S3: Detect man-page/groff markers in task output → suggest man_page generator (CC≤2).
///
/// Keywords: man page, groff, troff, manual section, .TH, .SH NAME, roff format.
/// Returns a hint to run `touring generate render man_page` via touring-generator.
pub(crate) fn maybe_man_page_hint_on_output(output_text: &str) -> Option<String> {
    const MAN_MARKERS: &[&str] = &[
        "man page",
        "groff",
        "troff",
        "manual section",
        ".th ",
        ".sh name",
        "roff format",
        "man section",
        "nroff",
    ];
    let lower = output_text.to_lowercase();
    let has_man = MAN_MARKERS.iter().any(|m| lower.contains(m));
    if !has_man {
        return None;
    }
    Some(
        " | man-page: man-page markers detected — run `touring generate render man_page` \
        to scaffold a Unix man page via touring-generator"
            .to_string(),
    )
}

/// R98-S1: Detect OpenAPI/Swagger markers in task output → suggest openapi_spec generator (CC≤2).
pub(crate) fn maybe_openapi_hint_on_output(output_text: &str) -> Option<String> {
    const OPENAPI_MARKERS: &[&str] = &[
        "openapi",
        "swagger",
        "oas3",
        "rest api spec",
        "api contract",
        "api specification",
        "http api schema",
        "rest specification",
    ];
    let lower = output_text.to_lowercase();
    let has_openapi = OPENAPI_MARKERS.iter().any(|m| lower.contains(m));
    if !has_openapi {
        return None;
    }
    Some(
        " | openapi-spec: OpenAPI markers detected — run `touring generate render OpenApiSpec` \
        to scaffold an OpenAPI specification via touring-generator"
            .to_string(),
    )
}

/// R98-S2: Detect hook handler markers in task output → suggest hook_handler generator (CC≤2).
pub(crate) fn maybe_hook_handler_hint_on_output(output_text: &str) -> Option<String> {
    const HOOK_MARKERS: &[&str] = &[
        "hook handler",
        "lifecycle hook",
        "pre_read",
        "post_edit",
        "pre-read",
        "post-edit",
        "hook registry",
        "claude code hook",
    ];
    let lower = output_text.to_lowercase();
    let has_hook = HOOK_MARKERS.iter().any(|m| lower.contains(m));
    if !has_hook {
        return None;
    }
    Some(
        " | hook-handler: hook handler markers detected — run `touring generate render HookHandler` \
        to scaffold a hook handler via touring-generator"
            .to_string(),
    )
}

/// R98-S3: Detect MCP tool markers in task output → suggest mcp_tool generator (CC≤2).
pub(crate) fn maybe_mcp_tool_hint_on_output(output_text: &str) -> Option<String> {
    const MCP_MARKERS: &[&str] = &[
        "mcp tool",
        "mcp server",
        "#[tool]",
        "model context protocol",
        "mcp__touring",
        "tool macro",
        "rmcp",
        "mcp handler",
    ];
    let lower = output_text.to_lowercase();
    let has_mcp = MCP_MARKERS.iter().any(|m| lower.contains(m));
    if !has_mcp {
        return None;
    }
    Some(
        " | mcp-tool: MCP tool markers detected — run `touring generate render McpTool` \
        to scaffold an MCP tool via touring-generator"
            .to_string(),
    )
}

/// R99-S1: Detect Terraform/IaC markers in task output → suggest terraform_module generator (CC≤2).
pub(crate) fn maybe_terraform_hint_on_output(output_text: &str) -> Option<String> {
    const TF_MARKERS: &[&str] = &[
        "terraform",
        "opentofu",
        "cloudformation",
        "pulumi",
        "infrastructure as code",
        "iac module",
        ".tf file",
        "terraform apply",
    ];
    let lower = output_text.to_lowercase();
    let has_tf = TF_MARKERS.iter().any(|m| lower.contains(m));
    if !has_tf {
        return None;
    }
    Some(
        " | terraform-module: Terraform/IaC markers detected — run `touring generate render TerraformModule` \
        to scaffold a Terraform module via touring-generator"
            .to_string(),
    )
}

/// R99-S2: Detect CLI handler markers in task output → suggest cli_handler generator (CC≤2).
pub(crate) fn maybe_cli_handler_hint_on_output(output_text: &str) -> Option<String> {
    const CLI_MARKERS: &[&str] = &[
        "cli handler",
        "clap",
        "subcommand",
        "argument parser",
        "cli command",
        "command_table",
        "daemon_query",
        "cli subcommand",
    ];
    let lower = output_text.to_lowercase();
    let has_cli = CLI_MARKERS.iter().any(|m| lower.contains(m));
    if !has_cli {
        return None;
    }
    Some(
        " | cli-handler: CLI handler markers detected — run `touring generate render CliHandler` \
        to scaffold a CLI handler via touring-generator"
            .to_string(),
    )
}

/// R99-S3: Detect plan/markdown markers in task output → suggest plan_md generator (CC≤2).
pub(crate) fn maybe_plan_md_hint_on_output(output_text: &str) -> Option<String> {
    const PLAN_MARKERS: &[&str] = &[
        "plan.md",
        "planning document",
        "markdown plan",
        "task plan",
        "# phase",
        "## subtask",
        "implementation plan",
        "execution plan",
    ];
    let lower = output_text.to_lowercase();
    let has_plan = PLAN_MARKERS.iter().any(|m| lower.contains(m));
    if !has_plan {
        return None;
    }
    Some(
        " | plan-md: plan/markdown markers detected — run `touring generate render PlanMd` \
        to scaffold a Markdown plan via touring-generator"
            .to_string(),
    )
}

/// R100-S1: Detect skill/documentation markers in task output → suggest skill_document generator (CC≤2).
pub(crate) fn maybe_skill_document_hint_on_output(output_text: &str) -> Option<String> {
    const SKILL_MARKERS: &[&str] = &[
        "skill document",
        "skill.md",
        "claude skill",
        "skill definition",
        "agent skill",
        "skill scaffold",
        "skill template",
        "skill yaml",
    ];
    let lower = output_text.to_lowercase();
    let has_skill = SKILL_MARKERS.iter().any(|m| lower.contains(m));
    if !has_skill {
        return None;
    }
    Some(
        " | skill-document: skill markers detected — run `touring generate render SkillDocument` \
        to scaffold a Claude Code skill document via touring-generator"
            .to_string(),
    )
}

/// R100-S2: Detect protobuf/gRPC markers in task output → suggest protobuf_schema generator (CC≤2).
pub(crate) fn maybe_protobuf_schema_hint_on_output(output_text: &str) -> Option<String> {
    const PROTO_MARKERS: &[&str] = &[
        "protobuf",
        "proto3",
        "grpc service",
        ".proto file",
        "protocol buffer",
        "rpc method",
        "message type",
        "grpc channel",
        "prost generated",
    ];
    let lower = output_text.to_lowercase();
    let has_proto = PROTO_MARKERS.iter().any(|m| lower.contains(m));
    if !has_proto {
        return None;
    }
    Some(
        " | protobuf-schema: protobuf/gRPC markers detected — run `touring generate render ProtobufSchema` \
        to scaffold a .proto schema via touring-generator"
            .to_string(),
    )
}

/// R100-S3: Detect consumer/event-driven markers in task output → suggest consumer_generator (CC≤2).
pub(crate) fn maybe_consumer_generator_hint_on_output(output_text: &str) -> Option<String> {
    const CONSUMER_MARKERS: &[&str] = &[
        "consumer generator",
        "event consumer",
        "kafka consumer",
        "message consumer",
        "event handler loop",
        "consume events",
        "async consumer",
        "stream consumer",
    ];
    let lower = output_text.to_lowercase();
    let has_consumer = CONSUMER_MARKERS.iter().any(|m| lower.contains(m));
    if !has_consumer {
        return None;
    }
    Some(
        " | consumer-generator: consumer/event markers detected — run `touring generate render ConsumerGenerator` \
        to scaffold an event consumer via touring-generator"
            .to_string(),
    )
}

/// R40-S2: Map the first file path in task output to a GeneratorKind scaffold hint (CC≤3).
///
/// Calls `extract_file_paths` on the output text, then uses `classify_file_to_generator_kind`
/// to find the first path with a known generator mapping (e.g. `.sql` → Migration).
/// Returns empty string when no known file type is detected in the output.
/// This closes the TaskOutput → artifact path → Generator scaffold loop.
pub(crate) fn artifact_file_gen_hint(output_text: &str) -> String {
    let paths = extract_file_paths(output_text);
    for path in &paths {
        if let Some(kind) = classify_file_to_generator_kind(path) {
            let stem = file_stem(path);
            return format!(
                " | artifact-gen: `touring generate render {kind} \
                --vars '{{\"module_name\":\"{stem}\"}}' ` for {path}"
            );
        }
    }
    String::new()
}

/// R39-S1: Inject RL +0.5 reward when test pass confirmed in task output (CC≤2).
///
/// Piggybacks on the `outcome_hint` produced by `completion_signal_hint` — avoids re-scanning
/// the output text. When "✓ success" is present, injects `cli_learning_reward(+0.5)` to close
/// the RL asymmetry where `failure_signal_hint` injects -0.1 but success had no injection.
/// Returns a diary write hint so Claude Code can record the lesson in AAAK format.
pub(crate) fn maybe_test_pass_rl_reward(
    rt: &mut HookRuntime,
    outcome_hint: &str,
    task_id: &str,
) -> String {
    if !outcome_hint.contains("✓ success") {
        return String::new();
    }
    let _ = crate::cli_handlers::cli_learning_reward(
        rt,
        &serde_json::json!({
            "tool": "edit",
            "reward": 0.5_f64,
            "context": format!("test_pass:{task_id}"),
        }),
    );
    format!(
        " | test-pass-rl: +0.5 injected | run `touring diary write claude_code \
        \"#{task_id}:tests_ok\" --aaak` to record lesson"
    )
}

/// R42-S2: Emit AAAK diary lesson hint when task output confirms test pass (CC≤2).
///
/// R39-S1 injects RL +0.5; R39-S3 stores a lesson on TaskUpdate(completed). This helper
/// closes the mid-task gap: test pass in TaskOutput → diary write hint with AAAK format.
/// The lesson is captured at the moment tests pass, not only when the user marks the task done.
/// Returns empty string when outcome_hint contains no "✓ success" marker.
pub(crate) fn maybe_diary_lesson_on_output_success(outcome_hint: &str, task_id: &str) -> String {
    if !outcome_hint.contains("✓ success") {
        return String::new();
    }
    format!(
        " | diary-lesson: run `touring diary write claude_code \
        \"#[P:validate] #[R:1.0] #[L:tests passed for {task_id}] #[W:none] #[E:none]\" --aaak` \
        to record this success"
    )
}

/// R34-S3: Detect plan.json or plan-submit mentions in task output and surface validate hint (CC=3).
///
/// When task output contains a path ending in `.json` or the string `plan-submit`, a
/// GeneratorPlan was likely produced. This helper extracts the first plausible plan path
/// from the output (up to 300 chars) and surfaces `touring generate plan-validate --plan-file`.
/// Returns empty string when no plan artifact is detected.
pub(crate) fn maybe_plan_validate_from_output(output_text: &str) -> String {
    let window = &output_text[..output_text.len().min(300)];
    // Detect explicit plan-submit mention
    if window.contains("plan-submit") || window.contains("plan.json") {
        // Try to extract a path token ending in .json
        let plan_path = window
            .split_whitespace()
            .find(|tok| tok.ends_with(".json"))
            .unwrap_or("plan.json");
        return format!(
            " | plan-validate: run `touring generate plan-validate --plan-file {plan_path}` \
            then `touring generate plan-submit --plan-file {plan_path}`"
        );
    }
    String::new()
}

/// Suggest a generator kind from the first N words of task output text (R24-S3).
///
/// Samples the first 20 whitespace-delimited tokens of the output (a reasonable keyword
/// window) and passes them to `suggest_generator_for_task_subject`. Returns empty string
/// when the output is empty or no SUBJECT_KEYWORD_MAP entry matches.
pub(crate) fn generator_hint_from_output(output: &str) -> String {
    if output.is_empty() {
        return String::new();
    }
    let window: String = output
        .split_whitespace()
        .take(20)
        .collect::<Vec<_>>()
        .join(" ");
    suggest_generator_for_task_subject(&window)
}

/// R27-S2: Detect test/build success signals in task output and suggest TaskUpdate(completed) (CC=3).
///
/// Scans the first 500 chars of output for completion patterns:
/// - "test result: ok" / "0 failed" — cargo test success
/// - "all tests pass" / "tests passed" — generic test suite success
/// - "build [finished]" / "Finished" — cargo build success
///
/// Returns a completion hint when detected, empty string otherwise.
/// This automates the CC task lifecycle: output confirms success → Claude Code
/// can mark the task completed without manually reading the full output.
pub(crate) fn completion_signal_hint(output: &str, task_id: &str) -> String {
    if output.is_empty() {
        return String::new();
    }
    let window = output[..output.len().min(500)].to_lowercase();
    let is_complete = window.contains("test result: ok")
        || window.contains("; 0 failed")
        || window.contains("0 failed;")
        || window.contains("all tests pass")
        || window.contains("tests passed")
        || (window.contains("finished") && window.contains("release"))
        || (window.contains("finished") && window.contains("debug"));
    if is_complete {
        format!(
            " | ✓ success detected — consider `TaskUpdate {task_id} completed` + \
            `touring decompose update {task_id} completed`"
        )
    } else {
        String::new()
    }
}

/// Extract backtick-quoted identifier tokens from text (up to 10, ≤ 80 chars each).
///
/// Matches patterns like `` `MyStruct` ``, `` `main.rs` ``, `` `cargo:build` ``.
/// Non-identifier characters inside backticks (spaces, `(`, `"`, etc.) cause the
/// token to be discarded so prose like `` `cargo test` `` does not pollute the index.
pub(crate) fn extract_backtick_symbols(text: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    let mut in_backtick = false;
    let mut current = String::new();
    for c in text.chars() {
        if symbols.len() >= 10 {
            break;
        }
        if c == '`' {
            if in_backtick {
                if !current.is_empty() && current.len() <= 80 {
                    symbols.push(current.clone());
                }
                current.clear();
                in_backtick = false;
            } else {
                in_backtick = true;
            }
        } else if in_backtick {
            if c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.' | '/') {
                current.push(c);
            } else {
                // Discard partial token — not a clean identifier
                current.clear();
                in_backtick = false;
            }
        }
    }
    symbols
}

/// Extract file paths from task output text (up to 5, ≤ 200 chars each).
///
/// Detects tokens that look like source file paths: contain `/`, end in a known
/// extension (`.rs`, `.toml`, `.py`, `.ts`, `.json`), and have no embedded spaces.
/// Used by R17-S2 to proactively refresh wiring after task output mentions modified files.
pub(crate) fn extract_file_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for word in text.split_whitespace().take(200) {
        // Strip leading/trailing punctuation that is not path-relevant.
        let word = word.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '/' && c != '_' && c != '.' && c != '-'
        });
        if word.len() > 200 || !word.contains('/') {
            continue;
        }
        let lower = word.to_lowercase();
        if lower.ends_with(".rs")
            || lower.ends_with(".toml")
            || lower.ends_with(".py")
            || lower.ends_with(".ts")
            || lower.ends_with(".json")
        {
            paths.push(word.to_string());
        }
        if paths.len() >= 5 {
            break;
        }
    }
    paths
}
