//! touring-capnp-server — transparent shim.
//!
//! All implementation lives in `touring-bindings` (feature `bind-capnp`).
//! This crate re-exports the full public surface so existing consumers
//! compile without any import changes.
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub use touring_bindings::capnp::*;
// Re-export generated Cap'n Proto modules at the crate root level so that
// existing code using `touring_capnp_server::holon_core_capnp::` continues
// to compile unchanged.
pub use touring_bindings::holon_core_capnp;
pub use touring_bindings::holon_generator_capnp;
