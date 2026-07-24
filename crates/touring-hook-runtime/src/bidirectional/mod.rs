//! Bidirectional suggestion framework — extracted from P1 (stuck_subtask) +
//! P2 (failure_threshold) patterns. Provides uniform `Suggester` trait +
//! storage helpers + a `run_suggester` driver so new suggesters (e.g.,
//! plan_mode CILA) can plug in with minimal boilerplate.
//!
//! # Structure
//! - [`suggester`] — `PendingSuggestion`, `Suggester` trait, `run_suggester` driver
//! - [`storage`] — `has_pending_suggestion`, `insert_suggestion` (SQLite helpers)
//!
//! # Usage
//! ```ignore
//! use crate::bidirectional::{Suggester, PendingSuggestion};
//!
//! pub struct MySuggester;
//! impl Suggester for MySuggester {
//!     fn action_type(&self) -> &'static str { "my_action" }
//!     fn detect(&self, rt: &HookRuntime) -> Vec<PendingSuggestion> { vec![] }
//! }
//! ```

pub mod storage;
pub mod suggester;

pub use suggester::{PendingSuggestion, Suggester, run_suggester};
