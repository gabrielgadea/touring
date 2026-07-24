//! Code Execution Gateway (CEG) — the `X0..X9` execution-gating pipeline.
//!
//! Phase **P3** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`). The gateway
//! routes every code-bearing tool call through ten ordered stages before any
//! real execution is permitted:
//!
//! ```text
//! X0 CAPTURE → X1 CLASSIFY → X2 STATIC → X3 VGP → X4 PREDICT
//!   → X5 SANDBOX → X6 CAPABILITY-GATE → X7 DECISION → X8 EXECUTE → X9 LEARN
//! ```
//!
//! The stage order is not a convention — it is enforced by the [`typestate`]
//! module's [`Execution`] type, where each stage is a distinct compile-time
//! state and `X3` (VGP) and `X5` (SANDBOX) are structurally unskippable.
//!
//! # Phase layout
//!
//! | Deliverable | Module(s)                   | Status   |
//! |-------------|-----------------------------|----------|
//! | P3.1        | [`typestate`]               | complete |
//! | P3.2        | `capture`, `classify`       | complete |
//! | P3.3        | `static_stage`, `vgp_stage` | complete |
//! | P3.4        | `predict`                   | complete |
//! | P3.5        | `sandbox_stage`             | complete |
//! | P3.6        | `gate`, `decision`          | complete |
//! | P3.7        | `pre_exec`, `error`         | complete |
//! | ES1 P3 (2026-06-01) | `prove_claim` (typestate), `pre_exec` insert | complete |
//! | P4.2        | `supervised`                | complete |
//! | P4.4        | `exec_pool`                 | complete |
//! | P4.5        | `dry_run_cache`             | complete |
//! | P4.6        | `fast_path`                 | complete |
//! | P5.1        | `staging`                   | complete |
//! | P5.2        | `staging_registry`          | complete |
//!
//! `supervised` is the X8 SUPERVISED-EXEC stage (CEG Pln2 Phase P4): the real
//! run, confined by the landlock LSM — see [`supervised::run_supervised`].

// Master Plan C.W3.P2.T12 — dogfooding the CEG's own fail-open contract.
// The gateway documents (see `learn.rs` "Fail-open invariant") that no
// production path may panic the session. This gate makes that contract
// machine-enforced: production code in the X0..X9 pipeline may not use
// `.unwrap()`. `#[cfg(test)]` submodules are exempt so tests may unwrap freely.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod capture;
pub mod change_contract;
pub mod classify;
pub mod decision;
pub mod deps;
// S-13 (2026-06-06) — drift_corrector is 100% CEG-owned (uses only gateway types,
// used only by gateway learn.rs). Moving it here collapses the
// gateway<->drift_corrector cycle to intra-gateway. Re-exported at lib.rs.
pub mod drift_corrector;
pub mod dry_run_cache;
pub mod error;
pub mod exec_pool;
pub mod fast_path;
pub mod gate;
pub mod harness_contract;
pub mod harness_extension;
pub mod harness_metric;
// S-13 (2026-06-06) — the touring-offensive solver facade is gateway-owned, so it
// travels with the CEG at crate extraction. Re-exported at lib.rs as
// `crate::offensive_integration` for API compat (no behavioural change).
pub mod offensive_integration;
pub mod outcome_learner;
pub mod pre_exec;
pub mod predict;
pub mod quality_signal;
// S-13 (2026-06-06) — the sandbox runner is gateway-owned (it uses exec_pool +
// capability and is the CEG's actual sandbox). Moving it here collapses the
// gateway<->sandbox_executor module cycle to intra-gateway. Re-exported at lib.rs
// as `crate::sandbox_executor` for the 5 non-gateway consumers + API compat.
pub mod sandbox_executor;
pub mod sandbox_stage;
// C13 — selective checkpointing: only side-effecting actions get a compensating saga step.
pub mod selective_checkpoint;
pub mod speculative;
pub mod staging;
// S-13 (2026-06-06) — CEG temporal-split classification (was the root `staging`
// module; renamed to avoid collision with `staging` above, which is area/GC).
// Leaf-safe, used only by `staging_registry`. Aliased at lib.rs as `crate::staging`.
pub mod staging_classify;
pub mod staging_registry;
pub mod static_stage;
// C5 — Active Output Summarizer: inline, metadata-first digest of sandbox output.
pub mod summarize;
pub mod supervised;
pub mod txn;
pub mod typestate;
pub mod vgp_stage;

pub use capture::{ExecSurface, capture_tool_call};
pub use classify::{Classification, CodeBody, sniff_language};
pub use decision::{EvidenceBundle, GateDecision, Verdict, composite_score};
pub use deps::{CegRuntime, LearnRuntime};
pub use dry_run_cache::{
    CacheConfig, CacheStats, DryRunCache, cached_dry_run_in_sandbox, dry_run_cache_key,
};
pub use error::GatewayError;
#[cfg(feature = "txn_lock_enforcement")]
pub use exec_pool::TxnPermit;
pub use exec_pool::{ExecPool, PoolConfig, PoolError, PoolStats};
pub use fast_path::{
    FAST_PATH_PURE_MARKER, FastPathDecision, fast_path_decision, is_provably_pure,
    pure_skip_outcome,
};
pub use gate::{
    CapabilityNeed, GateReport, GatedCapability, capability_class, gate_capabilities,
    required_capabilities,
};
pub use pre_exec::{
    DEFERRED_PROFILE_MARKER, GatewayDeps, GatewayOutcome, REFUSED_PROFILE_MARKER, deferred_dry_run,
    guarded_dry_run, neutral_outcome_history, observe, run_gateway, run_gateway_speculative,
    soft_pass_symbol,
};
pub use predict::{
    ExecutionOutcomePredictor, OutcomeStats, PredictionConfidence, PredictionReport, signature_for,
};
pub use sandbox_stage::{SandboxCapabilities, SandboxOutcome, dry_run_in_sandbox};
pub use selective_checkpoint::{CheckpointDecision, SelectiveCheckpointStats, decide_checkpoint};
pub use speculative::{AcceptedPrefix, CandidateAction, rank_by_predicted, speculative_execute};
pub use staging::{
    DEFAULT_STAGING_RETENTION_SECS, GcReport, StagingArea, gc_staging, gc_staging_in, stage_path,
    staging_retention_secs, staging_root,
};
pub use staging_registry::{RegistryEntry, StagingRegistry, content_hash};
pub use static_stage::{StaticReport, StaticSeverity};
pub use summarize::{OutputSummary, summarize_output};
pub use supervised::{
    SupervisedOutcome, SupervisionPolicy, run_supervised, run_supervised_blocking,
};
pub use typestate::{
    Advance, Analyzed, Captured, Classified, Decided, Evidence, Execution, ExecutionId,
    ExecutionState, Gated, Predicted, Proven, RawInvocation, SandboxTested, StageRecord, Verified,
};
pub use vgp_stage::{VgpReport, extract_symbols};

// ── CEG Pln2 FASE 5a — P6 (learn) + P7 (metrics) ─────────────────────────
pub mod learn;
pub mod metrics;
pub use learn::{
    DryRunBridgeResult, SilentFailure, detect_silent_failure, dry_run_bridge, emit_gate_reward,
    persist_forbidden_as_gotcha, reconcile_drift,
};
pub use metrics::{
    record_antipattern_converted, record_ceg_blocked, record_ceg_captured, record_ceg_fast_path,
    record_ceg_sandboxed, record_verdict_counters, record_workflow_advice_emitted,
    record_workflow_antipattern_detected,
};
