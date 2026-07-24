//! touring-hooks-saga — distributed saga 2PC coordinator (A01 leaf extraction).
//!
//! Extracted verbatim from `touring-hooks::saga` (A01 refactor, 2026-06-06).
//! Genuine leaf crate: depends only on `dashmap`, `tokio`, and `touring-rkyv`
//! (`SagaError`) — zero `crate::` deps back into touring-hooks. Re-exported by
//! touring-hooks as `touring_hooks::saga` so every call-site is preserved.
//!
//! The `saga` feature gates the `TestAgent` helper + its feature-gated tests,
//! mirroring the original touring-hooks `saga` feature semantics.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free leaf — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod distributed;

#[cfg(test)]
mod tests;

pub use distributed::{
    AgentSession, DistributedSagaCoordinator, SagaAgent, StepResult, TransactionPhase,
};
