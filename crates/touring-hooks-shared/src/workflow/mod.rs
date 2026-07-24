//! Workflow Intelligence **core** (leaf) — relocated from `touring-hooks::workflow`
//! (S-13, 2026-06-06).
//!
//! The leaf-safe data + detection primitives — [`baseline`] (forensic antipattern
//! baseline), [`stage`] (workflow-stage inference), [`antipattern`] (combination
//! antipattern detector) — live here so both the CEG X2 STATIC stage
//! (`touring-hooks::gateway::static_stage`) and `cli_suggester` can use them
//! without a crate cycle. The higher-level advisors that depend on the gateway
//! (`convert` → `gateway::decision::Verdict`) or are pure advisory (`advise`,
//! `glob_diag`) stay in `touring-hooks::workflow`, which re-exports these three.

pub mod antipattern;
pub mod baseline;
pub mod stage;
