//! Handler for the `file-changed` lifecycle hook event.
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D5. The handler orchestrates
//! cache invalidation, wiring updates, Tantivy upsert, and hint collection for
//! every file modification event from Claude Code.
//!
//! # Module layout
//!
//! - `mod.rs` (this file): `handle_file_changed` + helpers that require `&mut HookRuntime`
//! - `hints.rs`: pure path-pattern helpers (no `HookRuntime` dependency)
//!
//! # Visibility
//!
//! Only `handle_file_changed` is `pub(crate)` — re-exported from `lifecycle.rs`.
//! All other items in this module are private or `pub(super)`.

mod hints;

// Re-export all hint functions so that inline tests in `lifecycle.rs` can reach
// them via `super::maybe_*`. Gated #[cfg(test)] — none are called from outside
// handle_file_changed in production code.
#[cfg(test)]
pub(crate) use hints::{
    maybe_adr_hint_on_file_changed, maybe_asyncapi_hint_on_file_changed,
    maybe_benchmark_hint_on_file_changed, maybe_cargo_toml_hint,
    maybe_changelog_hint_on_file_changed, maybe_ci_workflow_hint_on_file_changed,
    maybe_consumer_generator_hint_on_file_changed, maybe_derive_macro_hint_on_file_changed,
    maybe_diary_entry_hint_on_file_changed, maybe_dockerfile_hint_on_file_changed,
    maybe_error_catalog_hint_on_file_changed, maybe_ffi_binding_hint_on_file_changed,
    maybe_fuzz_target_hint_on_file_changed, maybe_incremental_patch_hint_on_file_changed,
    maybe_index_stale_hint, maybe_k8s_hint_on_file_changed, maybe_man_page_hint_on_file_changed,
    maybe_migration_hint_on_file_changed, maybe_openapi_hint_on_file_changed,
    maybe_plan_md_hint_on_file_changed, maybe_protobuf_hint_on_file_changed,
    maybe_python_script_hint_on_file_changed, maybe_rust_module_hint_on_file_changed,
    maybe_schema_hint_on_file_changed, maybe_shell_completion_hint_on_file_changed,
    maybe_skill_document_hint_on_file_changed, maybe_task_scaffold_hint_on_file_changed,
    maybe_tera_template_hint, maybe_terraform_hint_on_file_changed, maybe_test_file_hint,
    maybe_test_hint_on_file_changed, maybe_wiring_chains_hint_for_handler_file,
};

use crate::runtime::HookRuntime;
use serde_json::Value;

// Bring shared helpers into scope via the grandparent re-export in lifecycle.rs.
// `super::` resolves to `lifecycle` module which re-exports all of shared.rs.
use super::{file_stem, maybe_generator_kind_hint, maybe_vgp_verify_hint_for_rs_file};

/// file-changed: invalidate result_cache, re-verify wiring, cascade to dependents.
///
/// When a file is modified (by Claude, user, or build tool), this handler:
/// 1. Invalidates cached pre-read context (existing behavior)
/// 2. Auto-resolves stale gotchas (existing behavior)
/// 3. Re-verifies wiring map for the changed file
/// 4. Checks if dependents have broken imports
pub(crate) fn handle_file_changed(rt: &mut HookRuntime, input: &Value) -> String {
    let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p,
        _ => return String::new(),
    };

    let evicted = rt.ctx.result_cache.invalidate_file(file_path);
    tracing::debug!(
        file = file_path,
        evicted,
        "result_cache invalidated for changed file"
    );
    let _ = rt.ctx.knowledge.maybe_auto_resolve_gotchas(file_path);

    let rel_path = crate::runtime::make_relative(file_path, &rt.project_root);
    crate::wiring::update_wiring_after_edit(&rt.ctx.knowledge, &rel_path);
    // R22-S2: Upsert file change event to Tantivy for BM25 audit trail.
    super::upsert_file_changed_to_tantivy(&rel_path);
    // R129: Persist file change event to knowledge graph — enables `touring memory recall "file_changed:<path>"`.
    // Closes the gap: file changes are searchable via Tantivy but were not recallable via memory graph.
    // Tier "semantic" so cross-session `touring memory recall` finds recently changed files.
    let _ = crate::cli_handlers::cli_memory_store(
        rt,
        &serde_json::json!({
            "key": format!("file_changed:{rel_path}"),
            "value": format!("File {rel_path} modified during active session — run `touring ast overview {rel_path}` to inspect symbols"),
            "tier": "semantic",
            "entry_type": "insight",
        }),
    );

    // Delegate complex CC-heavy logic to helpers to keep this function lean (CC≤6).
    let (wiring_warnings, has_dependents) = collect_wiring_warnings(rt, &rel_path);
    let mut warnings = wiring_warnings;

    if let Some(w) = hints::maybe_new_file_hint(warnings.is_empty(), has_dependents, &rel_path) {
        warnings.push(w);
    }
    if let Some(w) = hints::maybe_plan_json_hint(&rel_path) {
        warnings.push(w);
    }
    // R26-S1: Detect Tera template changes → surface validate + test pipeline commands.
    // When Claude Code edits a .tera file, the hook immediately closes the generator loop.
    if let Some(w) = hints::maybe_tera_template_hint(&rel_path) {
        warnings.push(w);
    }
    // R29-S2: Detect test file changes → surface Test generator + cargo test reminder.
    // Test files are prime candidates for expanding coverage via the Test generator kind.
    if let Some(w) = hints::maybe_test_file_hint(&rel_path) {
        warnings.push(w);
    }
    // R19-S1: Map changed file extension → relevant GeneratorKind for instant scaffold hint.
    if let Some(w) = maybe_generator_kind_hint(&rel_path) {
        warnings.push(w);
    }
    // R33-S1: Detect Cargo.toml changes → surface feature-gate + wiring audit commands.
    if let Some(w) = hints::maybe_cargo_toml_hint(&rel_path) {
        warnings.push(w);
    }
    // R36-S2: Emit VGP verify hint for changed .rs files — surfaces likely symbol name.
    // Converts file stem to CamelCase and emits `generate verify --symbol <Name>`.
    if let Some(w) = maybe_vgp_verify_hint_for_rs_file(&rel_path) {
        warnings.push(w);
    }
    // R41-S1: When .rs file with dependents changes, emit targeted index rebuild hint.
    // Changed source file with consumers means the symbol graph may be stale for VGP.
    if let Some(w) = hints::maybe_index_stale_hint(&rel_path, has_dependents) {
        warnings.push(w);
    }
    // R44-S1: For hook handler / lifecycle files, emit wiring chains hint — shows functional
    // chain membership that may shift when a handler body changes.
    if let Some(w) = hints::maybe_wiring_chains_hint_for_handler_file(&rel_path) {
        warnings.push(w);
    }
    // R70: file-path-pattern hints (asyncapi/error-catalog/adr) via dispatcher — CC=0 here.
    warnings.extend(hints::collect_path_pattern_warnings(&rel_path));
    // R145: Gotcha match — closes the FileChanged → gotcha DB read loop.
    // R139/R140 write to the gotcha DB on TaskCreate/TaskStop; FileChanged never read from it.
    // Now each file change surfaces known pitfalls from the gotcha DB for that specific file.
    // Closes the prevention loop: past failures → gotcha DB → warn at next edit of same file.
    if let Some(w) = maybe_gotcha_match_hint_for_file(rt, &rel_path) {
        warnings.push(w);
    }
    // R147: Inject RL reward when changed file has dependents (high-impact edit signal).
    // FileChanged with dependents = productive structural edit. Reward 0.2 reinforces
    // impactful coding patterns in the RL engine, closing the FileChanged → RL feedback loop.
    // Silent for isolated files (no dependents) — avoids rewarding trivial/leaf changes.
    if has_dependents {
        let context = format!("file_changed:{}", &rel_path[..rel_path.len().min(40)]);
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "edit",
                "reward_value": 0.2,
                "context": context,
            }),
        );
    }
    // R164: Surface active Touring DAG tasks when a file changes.
    // FileChanged had no CC task awareness — engineer had no signal that a file edit may
    // affect an in-progress Touring DAG task. Now surfaces advisory when tasks exist in the DAG.
    if let Some(w) = maybe_active_dag_hint_for_file(rt, &rel_path) {
        warnings.push(w);
    }

    warnings.join(" | ")
}

/// Collect wiring-related warnings for a changed file.
/// Invalidates dependent caches and checks integration score.
/// Returns (warnings, has_dependents).
fn collect_wiring_warnings(rt: &mut HookRuntime, rel_path: &str) -> (Vec<String>, bool) {
    let mut warnings = Vec::new();
    let dependents = match rt.ctx.knowledge.get_dependents(rel_path) {
        Ok(d) => d,
        Err(_) => return (warnings, false),
    };
    let has_dependents = !dependents.is_empty();
    for dep in dependents.iter().take(10) {
        let _ = rt.ctx.result_cache.invalidate_file(&dep.source);
    }
    if let Ok(score) = rt.ctx.knowledge.integration_score(rel_path) {
        if score < 0.5 {
            let stem = file_stem(rel_path);
            warnings.push(format!(
                "wiring: {rel_path} changed (score={:.0}%) — {} dependents may be affected — run: touring generate plan-suggest --intent \"wire {stem} orphan symbols into consumers\"",
                score * 100.0,
                dependents.len(),
            ));
        }
    }
    (warnings, has_dependents)
}

/// R145: Surface known gotcha pitfalls from the gotcha DB for the changed file (CC≤3).
///
/// When a file is edited, `cli_gotcha_match` queries the gotcha DB for patterns matching
/// that file path. If any are found, the first pitfall is surfaced as an advisory warning.
/// Closes the read-side of the R139/R140 write loop: TaskStop adds pitfall → next edit reveals it.
/// Returns `None` when no gotchas exist for the file (silent — avoids noise on clean files).
fn maybe_gotcha_match_hint_for_file(rt: &mut HookRuntime, rel_path: &str) -> Option<String> {
    let result = crate::cli_handlers::cli_gotcha_match(
        rt,
        &serde_json::json!({
            "file_path": rel_path,
        }),
    );
    let count = serde_json::from_str::<serde_json::Value>(&result)
        .ok()
        .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
        .unwrap_or(0);
    if count == 0 {
        return None;
    }
    Some(format!(
        "gotcha: {count} known pitfall(s) for {rel_path} — run `touring gotcha match {rel_path}` \
        before editing to review failure patterns from past tasks"
    ))
}

/// R164: Surface active Touring DAG tasks when a file changes — closes FileChanged → task awareness gap.
///
/// Queries `cli_decompose_status` to count open tasks. When tasks exist, emits an advisory
/// reminding the engineer to check `touring decompose status` for in_progress tasks touching
/// the changed file. Silent when no tasks exist (zero DAG overhead on idle projects).
fn maybe_active_dag_hint_for_file(rt: &mut HookRuntime, rel_path: &str) -> Option<String> {
    let status_json = crate::cli_handlers::cli_decompose_status(rt, &serde_json::json!({}));
    let total_tasks = serde_json::from_str::<serde_json::Value>(&status_json)
        .ok()
        .and_then(|v| v.get("total_tasks").and_then(|t| t.as_i64()))
        .unwrap_or(0);
    if total_tasks == 0 {
        return None;
    }
    Some(format!(
        "dag: {total_tasks} task(s) in touring decompose DAG — run: touring decompose status -j \
        to check if in-progress tasks touch {rel_path}"
    ))
}
