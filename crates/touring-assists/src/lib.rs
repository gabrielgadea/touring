//! `touring-assists` — Refactor-as-CLI framework.
//!
//! Based on rust-analyzer's assist pattern: `Assist` = labeled `SourceChange` with lazy evaluation.
//! Handlers are functions `fn(&mut Assists, &AssistContext) -> Option<()>` registered in a catalog.
//!
//! # Core types
//!
//! - [`Assist`] — a single refactor action with id, label, group, target range, and lazy SourceChange
//! - [`AssistContext`] — input data for a handler (db, file, range, syntax tree)
//! - [`Assists`] — accumulator collecting applicable assists
//! - [`AssistHandler`] — function signature for all handlers
//! - [`AssistCatalog`] — registry of all known handlers

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod framework;
mod handlers;

pub use framework::{
    Assist, AssistCatalog, AssistCodeAction, AssistContext, AssistEdit, AssistGroup, AssistHandler,
    AssistId, AssistTarget, AssistTextChange, Assists, LazySourceChange,
};
pub use handlers::*;
