//! Intelligent handlers for lifecycle hook events (S4).
//!
//! Replaces the previous no-op behavior (just record_access) with targeted
//! intelligent actions that improve context quality and cache efficiency.
//!
//! The interface of this module is the fourteen `handle_*` entry points below. The
//! hint helpers behind them are internal to their submodules and are not re-exported
//! for tests: the tests drive the handlers (`surface_tests`), so a helper can be
//! refactored without touching a test as long as the handler output does not change.
//!
//! Handlers:
//! - `file-changed`: invalidate result_cache for the changed file
//! - `cwd-changed`: record directory switch in knowledge graph
//! - `subagent-start`: inject top project gotchas as context for subagent
//! - `pre-compact`: flush rkyv snapshots before context compaction
//! - `worktree-create`: record worktree path for future context
//! - `enter-plan-mode` / `exit-plan-mode`: plan-session hints and DAG bookkeeping
//! - `task-sync-*` (create, update, list, get, output, stop, delete): Claude Code task
//!   mirror into the Touring decompose DAG

// Test-only import: HookRuntime is used in the test modules via `use super::*`.
#[cfg(test)]
use crate::runtime::HookRuntime;

// Each submodule owns one lifecycle handler (or a family of them). The handlers are
// re-exported `pub(crate)` for `hook_registry.rs`; the few helpers that SIBLING submodules
// share are imported privately below (a private `use` is visible to the child modules).
mod shared;
use shared::{
    classify_file_to_generator_kind, file_stem, maybe_generator_kind_hint,
    maybe_vgp_verify_hint_for_rs_file, persist_task_creation, plan_scaffold_for_subject,
    suggest_generator_for_task_subject,
};

mod subagent;
pub(crate) use subagent::handle_subagent_start;

mod pre_compact;
pub(crate) use pre_compact::handle_pre_compact;

mod worktree;
pub(crate) use worktree::handle_worktree_create;

mod cwd_changed;
pub(crate) use cwd_changed::handle_cwd_changed;

mod task_delete;
pub(crate) use task_delete::handle_task_sync_post_delete;

mod task_stop;
pub(crate) use task_stop::handle_task_sync_post_stop;

// `maybe_wiring_orphan_hint_on_complete` is also called by `hook_registry` (task-completed).
mod task_update;
pub(crate) use task_update::{handle_task_sync_post_update, maybe_wiring_orphan_hint_on_complete};

mod task_get;
pub(crate) use task_get::handle_task_sync_post_get;

mod task_create;
pub(crate) use task_create::handle_task_sync_post_create;
// The subject matchers live in touring-hooks-core; `hook_registry` consumes the dispatcher.
pub(crate) use touring_hooks_core::generator_hints::collect_subject_generator_hints;

mod file_changed;
pub(crate) use file_changed::handle_file_changed;

mod task_list;
pub(crate) use task_list::handle_task_sync_post_list;
// Shared with the file-changed and task-update handlers (Tantivy audit trail).
use task_list::{upsert_file_changed_to_tantivy, upsert_task_completion_to_tantivy};

mod task_output;
pub(crate) use task_output::handle_task_sync_post_output;

mod plan_mode;
pub(crate) use plan_mode::{handle_enter_plan_mode, handle_exit_plan_mode};

// E2E contract tests for the lifecycle/ submodules (co-located with source).
#[cfg(test)]
mod e2e_tests;

// Handler-driven scenario tables + the replay corpus recorded before the visibility refactor.
#[cfg(test)]
mod surface_tests;

// Every test of the old suite is accounted for in tests/fixtures/old_dispatch_tests.tsv.
#[cfg(test)]
mod contract_tests;

#[cfg(test)]
mod tests;
