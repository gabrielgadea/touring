//! touring-orchestration — unified orchestration layer (W10 fusion of touring-premium-refactor-2026).
//!
//! Fuses three standalone crates into one: `flow` (declarative dataflow pipeline,
//! ex touring-flow), `tasks` (Tasksfile YAML DSL + compiler, ex touring-tasksfile),
//! `devrc` (Devrcfile adapter, ex touring-devrc-adapter). The three origin crates
//! are reduced to thin shim crates that re-export these modules.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free leaf — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod devrc;
pub mod flow;
pub mod tasks;
