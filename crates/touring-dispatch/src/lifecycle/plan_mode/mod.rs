//! Plan-mode lifecycle handlers (D9).
//!
//! Extracted from `lifecycle.rs` as part of FIX-3 D9 modularisation.
//! Contains the Enter/ExitPlanMode handlers and all co-located hint helpers.
//!
//! Public surface (re-exported by `lifecycle.rs`):
//! - `handle_enter_plan_mode` — PreToolUse(EnterPlanMode)
//! - `handle_exit_plan_mode`  — PostToolUse(ExitPlanMode)
//!
//! Everything else here is a helper of those two handlers and is private to this
//! module tree; it is exercised through the handlers (`lifecycle::surface_tests`).

mod enter;
mod exit;
mod hints;

pub(crate) use enter::handle_enter_plan_mode;
pub(crate) use exit::handle_exit_plan_mode;
