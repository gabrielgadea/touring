//! Touring suggester modules — proactive action suggestions for Claude Code.
//!
//! Each suggester detects a specific pattern and emits an entry in
//! `cc_action_suggestions` so Claude Code can act on it via the appropriate
//! tool (TaskUpdate, TaskCreate, etc.).
//!
//! Rate-limiting: every suggester guards against duplicate emissions by
//! checking for an existing unconsumed suggestion before inserting.
//!
//! Wired from `instructions_loaded::run_returning` at session start.

pub mod failure_threshold;
pub mod plan_mode_complexity;
pub mod stuck_subtask;
