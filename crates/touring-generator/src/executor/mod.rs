//! `PlanExecutor` — typestate-driven pipeline for plan lifecycle management.
//!
//! States: Draft → Verified → Rendered → Speculated → Committed (terminal success)
//! Invalid transitions are compile errors, not runtime panics.

pub mod replan;
pub mod typestate;

pub use replan::{CompletedPlan, RejectedPlan, ReplanRequest};
pub use typestate::{Committed, Draft, PlanExecutor, Rendered, Speculated, Verified};
