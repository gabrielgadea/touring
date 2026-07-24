//! Plan-mode lifecycle handlers (D9).
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D9 modularisation.
//! Contains the Enter/ExitPlanMode handlers and all co-located hint helpers.
//!
//! Public surface (re-exported by `lifecycle.rs`):
//! - `handle_enter_plan_mode` — PreToolUse(EnterPlanMode)
//! - `handle_exit_plan_mode`  — PostToolUse(ExitPlanMode)
//!
//! Test-visible helpers (re-exported for `lifecycle.rs` inline tests via `use super::*`):
//! - enter helpers: `active_dag_ready_hint`, `task_id_session_recall_hint`,
//!   `top_orphans_for_plan_hint`, `evolution_drift_hint_on_enter_plan`,
//!   `maybe_diary_write_on_plan`, `memory_recall_hint_for_intent`,
//!   `maybe_plan_recall_hint_for_intent`
//! - exit helpers: `exit_plan_mode_generator_hint`, `maybe_tantivy_search_hint_on_exit`,
//!   `assess_plan_session`, `plan_critique_hint_on_exit`, `auto_finalize_plan_on_exit`,
//!   `plan_session_link_hint`, `plan_file_from_intent`

mod enter;
mod exit;
mod hints;

pub(crate) use enter::handle_enter_plan_mode;
pub(crate) use exit::handle_exit_plan_mode;

// Re-export all hint helpers so lifecycle.rs inline tests (`use super::*`) can reach them.
// Test-only: these are called inside handle_enter/exit_plan_mode, not from outside callers.
#[cfg(test)]
pub(crate) use hints::*;

// Re-export enter/exit helpers for lifecycle.rs inline tests (use super::*)
// Test-only: not called from production code outside their respective handlers.
#[cfg(test)]
pub(crate) use enter::{
    active_dag_ready_hint, evolution_drift_hint_on_enter_plan, maybe_diary_write_on_plan,
    maybe_plan_recall_hint_for_intent, memory_recall_hint_for_intent, task_id_session_recall_hint,
    top_orphans_for_plan_hint,
};
#[cfg(test)]
pub(crate) use exit::{
    assess_hint_from_session_result, assess_session_id_from_recall, auto_finalize_plan_on_exit,
    exit_plan_mode_generator_hint, maybe_parallel_subagent_hint, maybe_tantivy_search_hint_on_exit,
    plan_critique_hint_from_recall, plan_critique_hint_on_exit, plan_file_from_intent,
    plan_session_link_hint, plan_session_link_hint_from_recall,
};
