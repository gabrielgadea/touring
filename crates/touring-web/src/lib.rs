//! touring-web — transparent shim.
//!
//! All implementation lives in `touring-bindings` (feature `bind-web`).
//! This crate re-exports the full public surface so existing consumers
//! compile without any import changes.
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub use touring_bindings::web::*;
