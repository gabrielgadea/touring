//! Reasoning Subsystem — Structured task decomposition
//!
//! Provides a deterministic DAG-based task planning engine with:
//! - O(1) subtask lookups via internal index
//! - Priority-aware topological sort (Kahn's algorithm)
//! - Full lifecycle status (`pending` → `in_progress` → `completed` | `failed` | `cancelled`)
//! - `ready_subtasks()` for executor-driven dispatch
//! - `completion_pct()` for progress tracking
//! - ACO pheromone wiring for task state transitions
//! - Verification gates (TestGate, ClippyGate)

pub mod budget; // C11 — budget conservation B∈ℕ⁶ (Σ children ≤ root) over the decompose DAG
#[path = "decomposer.rs"]
pub mod decomposer;
pub mod route; // C7 — RGAO task-scope vector → CILA level routing

#[path = "persistence.rs"]
pub mod persistence;

#[path = "verification.rs"]
pub mod verification;

pub mod granularity_adapter;

pub use decomposer::{AcoEvent, SubTask, SubTaskStatus, Task, TaskDecomposer};
pub use granularity_adapter::{GranularityHint, query_granularity_hint};
pub use persistence::CheckpointManager;
pub use verification::{ClippyGate, GateResult, TestGate, VerifyGate};
