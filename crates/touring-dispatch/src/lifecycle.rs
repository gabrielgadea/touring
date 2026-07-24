//! Intelligent handlers for lifecycle hook events (S4).
//!
//! Replaces the previous no-op behavior (just record_access) with targeted
//! intelligent actions that improve context quality and cache efficiency.
//!
//! Handlers:
//! - `file-changed`: invalidate result_cache for the changed file
//! - `cwd-changed`: record directory switch in knowledge graph
//! - `subagent-start`: inject top project gotchas as context for subagent
//! - `pre-compact`: flush rkyv snapshots before context compaction
//! - `worktree-create`: record worktree path for future context

// Test-only import: HookRuntime is used in inline tests via `use super::*`.
#[cfg(test)]
use crate::runtime::HookRuntime;

// ── FIX-3 Fase A: extracted submodules ───────────────────────────────────────
//
// Each submodule owns one lifecycle hook handler. The parent module re-exports
// the handler with `pub(crate)` visibility so `hook_registry.rs` callers and
// inline tests continue to resolve via `crate::lifecycle::handle_*` and
// `super::handle_*` respectively — no external breakage.
// D2 — shared helpers used across submodules (must come first so later
// submodule declarations can reference via `super::<helper>`).
mod shared;
// Production-required exports from shared (used by hook_registry + cli_handlers).
pub(crate) use shared::{
    classify_file_to_generator_kind, file_stem, maybe_generator_kind_hint,
    maybe_vgp_verify_hint_for_rs_file, suggest_generator_for_task_subject,
};
// Test-only helpers from shared (accessed via `use super::*` in inline tests).
#[cfg(test)]
pub(crate) use shared::{
    classify_rust_to_generator_kind, find_kind_by_keywords, stem_to_camel_case,
};

mod subagent;
pub(crate) use subagent::handle_subagent_start;

mod pre_compact;
pub(crate) use pre_compact::handle_pre_compact;

mod worktree;
pub(crate) use worktree::handle_worktree_create;
// Re-export helper so inline tests in this module can still reach it via
// `super::worktree_create_hint` — test-only (not used in production code).
#[cfg(test)]
pub(crate) use worktree::worktree_create_hint;

mod cwd_changed;
pub(crate) use cwd_changed::handle_cwd_changed;
// Test-only helpers from cwd_changed (accessed via `use super::*` in inline tests).
#[cfg(test)]
pub(crate) use cwd_changed::{cwd_wiring_hint, generator_kind_for_dir_pattern};

// D1 — simple TaskSync handlers
mod task_delete;
pub(crate) use task_delete::handle_task_sync_post_delete;

mod task_stop;
pub(crate) use task_stop::handle_task_sync_post_stop;
// Test-only: inline tests access session_assess_on_task_stop via `use super::*`.
#[cfg(test)]
pub(crate) use task_stop::session_assess_on_task_stop;

// D3 — task_update (first mega-handler extraction, template for D4/D6/D7/D8)
mod task_update;
pub(crate) use task_update::handle_task_sync_post_update;

// D4 — task_get + 38 co-located hint helpers (co-located pattern)
mod task_get;
pub(crate) use task_get::*;

// D8 — task_create + 32 co-located hint helpers (biggest mega-handler)
pub mod task_create;
pub(crate) use task_create::*;

// E2E contract tests for all lifecycle/ submodules (co-located with source).
#[cfg(test)]
mod e2e_tests;

// D5 — file_changed handler + 30 co-located path-pattern hint helpers
// `handle_file_changed`, `collect_wiring_warnings`, `maybe_gotcha_match_hint_for_file`,
// `maybe_active_dag_hint_for_file`, and all 30 `maybe_*_hint_on_file_changed` helpers
// moved to `lifecycle/file_changed/` (FIX-3 D5).
mod file_changed;
pub(crate) use file_changed::handle_file_changed;
// Re-export hint helpers so inline tests in this file can reach them via `super::maybe_*`.
// Gated #[cfg(test)] because none of these are called in production code — they are
// invoked inside handle_file_changed which takes ownership of dispatch.
#[cfg(test)]
pub(crate) use file_changed::{
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

// D10: All helpers moved to submodules. Re-exported below so inline tests
// (`use super::*`) and external callers (`crate::lifecycle::*`) continue to resolve.

// D10: collect_subject_generator_hints moved to task_create.rs (owns the _on_task_create fns).
// D10: collect_update_generator_hints + 30 maybe_*_on_task_update + completion helpers
//      moved to task_update.rs (primary owner).
// D10: persist_task_creation + plan_scaffold_for_subject moved to shared.rs (cross-module).

// Re-export from task_create (D10)
pub(crate) use task_create::collect_subject_generator_hints;

// Re-export from task_update (D10) — all pub(crate) items (completion helpers +
// status helpers + 30 maybe_*_hint_on_task_update functions) so inline tests
// resolve them via `use super::*`.
pub(crate) use task_update::*;

// Re-export from shared (D10) — cross-module task creation helpers
pub(crate) use shared::{persist_task_creation, plan_scaffold_for_subject};

// D7 — task_list (handle_task_sync_post_list + co-located helpers)
mod task_list;
pub(crate) use task_list::*;

// D6 — task_output (task-sync-post-output handler + 44 co-located hint helpers)
mod task_output;
pub(crate) use task_output::*;

// D9 — plan_mode (handle_enter_plan_mode + handle_exit_plan_mode + all hint helpers)
mod plan_mode;
pub(crate) use plan_mode::handle_enter_plan_mode;
pub(crate) use plan_mode::handle_exit_plan_mode;
// Pull all plan_mode helpers into lifecycle scope so inline tests (`use super::*`) find them.
// Gated #[cfg(test)]: these helpers are called inside handle_enter/exit_plan_mode,
// not directly from production callers.
#[cfg(test)]
use plan_mode::*;

// Re-export from task_update (D10) — private helper needed by inline tests via `use super::*`.
// Test-only: not called from production code.
#[cfg(test)]
pub(crate) use task_update::maybe_mcts_unblock_hint;

#[cfg(test)]
mod tests;
