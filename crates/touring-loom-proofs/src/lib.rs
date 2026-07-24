//! Isolated loom proofs for the daemon actor pattern.
//!
//! This crate has zero touring dependencies on purpose — see
//! `Cargo.toml` for the rationale. All tests live under
//! `tests/actor_invariants.rs` gated by `#![cfg(loom)]`.
//!
//! To run the proofs:
//!
//! ```text
//! RUSTFLAGS="--cfg loom" cargo test -p touring-loom-proofs --release
//! ```

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
