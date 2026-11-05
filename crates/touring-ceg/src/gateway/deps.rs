//! `gateway::deps` — the Code Execution Gateway's **dependency-inversion seam**.
//!
//! S-13 (2026-06-06). The CEG (`gateway/` + `capability/`) is a tightly coupled
//! module cluster inside `touring-hooks`. Extracting it into its own crate
//! (`touring-ceg`) was blocked because gateway modules reached *out* to
//! parent-crate modules via `crate::…` paths; Cargo forbids crate-level cycles
//! even though Rust permits the intra-crate module cycles that exist now
//! (`touring wiring cycles` = 0 confirms no *crate* cycle yet, only module ones).
//!
//! This module inverts those outward edges: the gateway declares the *traits* it
//! needs, and the parent crate implements them on its concrete types. The gateway
//! then depends only on its own abstraction, not on the parent layout — the
//! classic dependency inversion that makes a future leaf-crate extraction
//! possible *without rewriting call sites*.
//!
//! # The X9 LEARN seam — [`LearnRuntime`]
//!
//! The X9 LEARN stage (`gateway/learn.rs`) persists RL rewards, gotchas, and
//! memory lessons, and was hard-wired to
//! `crate::cli_handlers::{cli_learning_reward, cli_gotcha_add, cli_memory_store}`.
//! [`LearnRuntime`] abstracts exactly those three operations; `HookRuntime`
//! implements it in `cli_handlers.rs` by delegating to the same free functions.
//! After this seam `gateway/learn.rs` no longer names `crate::cli_handlers` — the
//! 7.8k-LOC handlers module is no longer an *inbound* dependency of the gateway,
//! and the edge now points the correct way for extraction
//! (`cli_handlers → gateway::deps`, i.e. parent → leaf).
//!
//! From 2026-06-09 (Master Plan **A.W3.P1**) until 2026-11-02 the
//! [`LearnRuntime`] trait lived in a separate leaf crate, `touring-contracts`,
//! re-exported here, as the foundation for this very `touring-ceg` extraction.
//! The extraction landed (see below) and that crate never gained a second
//! consumer or a second trait: `touring-ceg` was its only dependent, and the
//! only production implementor of [`LearnRuntime`] (`HookRuntime`) also
//! implements [`CegRuntime`] — which lives here and extends it — so it could
//! not avoid depending on `touring-ceg` anyway (the rest are test mocks). The
//! trait is therefore defined below, next to [`CegRuntime`]; the
//! `crate::gateway::deps::LearnRuntime` path and the `impl … for HookRuntime`
//! are unchanged. If a crate *below* the CEG ever needs the seam, extract it
//! again then — with a real second consumer.
//!
//! # CEG → parent extraction map (remaining edges — S-13 follow-up)
//!
//! Measured 2026-06-06 by `grep 'crate::<mod>'` over `gateway/` + `capability/`.
//! "Cycle?" = whether the parent module also references `crate::gateway` back
//! (a true mutual module cycle); one-way edges still block *crate* extraction
//! (parent → gateway crate cycle) but are mechanically simpler to cut.
//!
//! | Parent edge | Refs | Cycle? | Treatment for full extraction |
//! |-------------|-----:|:------:|-------------------------------|
//! | `cli_handlers` (3 fns) | — | no | **DONE (this change)** via [`LearnRuntime`] |
//! | `offensive_integration` | 116 | no | **DONE (2026-06-06)** — relocated to `gateway/offensive_integration.rs` (gateway-owned); re-exported at `lib.rs` as `crate::offensive_integration` for API compat, so all 116 call sites + the public API are unchanged. The cosmetic flip landed 2026-06-10 (Session A F1): all gateway-internal call sites now use the portable `crate::gateway::offensive_integration` path |
//! | `action_signature` | 19 | no | **DONE (2026-06-06)** — relocated to `touring-hooks-shared` (leaf, zero `crate::` deps); re-exported at `lib.rs` as `crate::action_signature`. Breaks the edge for the gateway *and* for `offensive_integration` (which also used it). 42 tests now run in the leaf crate. As of 2026-06-10 (Session A F3) gateway call sites name `touring_hooks_shared::action_signature` directly — the parent re-export is for non-gateway consumers only |
//! | `sandbox_executor` (1361 LOC — the CEG sandbox runner) | 13 | yes | **DONE (2026-06-06)** — moved the *whole module* into the gateway (`gateway/sandbox_executor.rs`); it uses `exec_pool` + `capability` and is used by `sandbox_stage`/`supervised`, so it IS the CEG sandbox. This collapses the gateway↔sandbox_executor cycle to intra-gateway. Re-exported at `lib.rs` for the 5 non-gateway consumers (0 production unwrap → safe under the gateway `deny(unwrap_used)`). 34 tests run under `gateway::sandbox_executor` |
//! | `runtime::HookRuntime` | 2 | no | **DONE for the X9 LEARN logic (2026-06-06)** via [`CegRuntime`] (supertrait of [`LearnRuntime`] adding ledger / drift-cache / contract-attestation). `learn.rs` is now generic over `CegRuntime` and names **zero** `crate::runtime` symbols. **FULLY DONE (2026-06-10, Session A F2)** — the hook-driver boundary (`verdict_to_response` / `gate_hook_input` / `run_observe_only` / `run_returning` / `run`) moved to the parent module `crate::ceg_adapter`; `gateway/` + `capability/` now contain **zero** production references to `crate::runtime` |
//! | `hook_runtime::IsolationMode` | 1 | yes | **DONE (2026-06-06)** — relocated the leaf-safe enum to `touring-hooks-shared::isolation_mode`; `hook_runtime` re-exports it for compat; `txn.rs` now names it from the leaf → the gateway→hook_runtime edge is eliminated |
//! | `drift_corrector` (`reconcile`, `SensorReading`, `DriftReconciliation`) | 1 | yes | **DONE (2026-06-06)** — moved the whole module into the gateway (`gateway/drift_corrector.rs`); it uses only gateway types and is used only by `learn.rs`, so the cycle collapses to intra-gateway. Re-exported at `lib.rs` |
//! | `workflow::antipattern` + `workflow::stage::WorkflowState` | 2 | yes | **DONE (2026-06-06)** — 2-part fix. VGP found the real coupling is `StaticSeverity` (returned by `antipattern_severity` via `super::super::gateway`), not `Verdict`. Part 1: relocated `StaticSeverity` → `shared::severity` (shared vocab; re-exported at `gateway::static_stage`). Part 2: relocated the leaf-safe core `{baseline, stage, antipattern}` → `shared::workflow` (re-exported at `workflow/mod.rs` for advise/convert/cli_suggester); `gateway/static_stage` imports them from the leaf → forward edge eliminated. `convert.rs`→`gateway::Verdict` stays (parent→child, allowed) |
//! | `staging` (root) | 2 | no (one-way; the back-edge was a doc-link) | **DONE (2026-06-06)** — VGP showed it is the leaf-safe temporal-split *classification* (disjoint symbols from gateway's `staging` area/GC, despite the shared name). Moved into the gateway as `staging_classify`; `lib.rs` aliases it back to `crate::staging` for the public API; as of 2026-06-10 (Session A F1) `staging_registry` names `crate::gateway::staging_classify` directly |
//! | `shared::{gate_metrics ×11, bash_ast_validator ×3, ast_grep_signal, hook_events, risk_patterns}` | 17 | no | **DONE (2026-06-10, Session B F4-pre)** — all 6 submodules (incl. `memory_stats_probe`, dragged by `gate_metrics`) relocated to `touring-hooks-shared`; the `SignalContext`/`SignalLayer` vocabulary was extracted to `touring_hooks_shared::signal_layer` so `ast_grep_signal` no longer reaches into the parent's `signal_pipeline` (which keeps its heavy `touring-analysis` deps in the parent and re-exports the vocab). The parent re-exports every module at `crate::shared::<mod>`, so all 32+ historical consumers and the cross-crate `touring_hooks::shared::*` paths are unchanged |
//!
//! The crate extraction LANDED on 2026-06-10 (Session B F4): `gateway/` +
//! `capability/` now live in **`touring-ceg`**, with `touring-hooks` re-exporting
//! both at its root (`pub use touring_ceg::{gateway, capability}`) — zero churn
//! for the 3 touring-server consumers, the `ceg_e2e` suite, and parent modules.

use serde_json::Value;

/// The X9 LEARN persistence operations a subsystem needs from its host runtime.
///
/// Implemented by the host (e.g. `touring_hook_runtime::HookRuntime`) by
/// delegating to the canonical `cli_learning_reward` / `cli_gotcha_add` /
/// `cli_memory_store` handlers. Abstracting them here lets the CEG
/// (`gateway/learn.rs`) close the RL / gotcha / memory feedback loops without
/// importing the host's handler module — inverting that edge so the gateway
/// stays a standalone crate.
///
/// Every method mirrors the underlying handler contract: it returns the
/// handler's JSON string (which may itself encode an error) and is **fail-open**
/// — an implementation must never panic.
pub trait LearnRuntime {
    /// Inject an RL reward. `payload` carries `{tool_name, reward, context}`.
    /// Mirrors `cli_learning_reward`.
    fn learning_reward(&mut self, payload: &Value) -> String;

    /// Persist a gotcha entry. `payload` carries `{pattern, description, severity}`.
    /// Mirrors `cli_gotcha_add`.
    fn gotcha_add(&mut self, payload: &Value) -> String;

    /// Persist a memory entry. `payload` carries `{key, value, tier, type}`.
    /// Mirrors `cli_memory_store`.
    fn memory_store(&mut self, payload: &Value) -> String;
}

/// The full X9 LEARN runtime-service surface the gateway needs from the host
/// runtime — a superset of [`LearnRuntime`] that additionally abstracts the
/// cross-agent ledger, the drift result-cache, and the constitutional contract
/// attestation that `learn.rs::reconcile_drift` + `emit_gate_reward` consume.
///
/// S-13 (2026-06-06) — this is the seam that removes `gateway/learn.rs`'s
/// dependency on `crate::runtime::HookRuntime` entirely: the X9 LEARN functions
/// become generic over `CegRuntime`, so the gateway's learn stage no longer names
/// the host runtime type. `HookRuntime` implements it in `cli_handlers.rs`.
///
/// All methods are **fail-open** — an implementation must never panic (matching
/// the gateway's fail-open invariant). The cross-agent ledger / cache accessors
/// silently no-op when their backing store is absent.
pub trait CegRuntime: LearnRuntime {
    /// Record a tool outcome to the cross-agent ledger (actor + ledger). No-op
    /// when no actor or ledger is configured. Mirrors the `actor_id` +
    /// `cross_agent_ledger.write_event` field access.
    fn record_tool_outcome(&self, event_type: &str, payload: &[u8]);

    /// Read a stashed value from the host's drift result-cache. `None` when
    /// absent or unparsable (the caller treats it as "no prior" — baseline).
    fn drift_cache_get(&self, scope: &str, key: &str) -> Option<String>;

    /// Stash a value into the host's drift result-cache (interior-mutable).
    fn drift_cache_put(&self, scope: &str, key: &str, value: String);

    /// The constitutional contract attestation, if one is attached to the host.
    /// Feeds the `constitutional_digest` axis of the drift sensor reading.
    fn contract_attestation(&self) -> Option<&super::harness_contract::HarnessContract>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock [`LearnRuntime`] that records calls without any runtime — proves
    /// the seam is host-agnostic and that callers compile against the trait
    /// alone (no host runtime, no handler module).
    #[derive(Default)]
    struct MockLearn {
        rewards: Vec<String>,
        gotchas: Vec<String>,
        memories: Vec<String>,
    }

    impl LearnRuntime for MockLearn {
        fn learning_reward(&mut self, payload: &Value) -> String {
            self.rewards.push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
        fn gotcha_add(&mut self, payload: &Value) -> String {
            self.gotchas.push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
        fn memory_store(&mut self, payload: &Value) -> String {
            self.memories.push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
    }

    #[test]
    fn mock_learn_records_each_op_independently() {
        let mut m = MockLearn::default();
        let _ = m.learning_reward(&serde_json::json!({"tool_name": "ceg", "reward": 1.0}));
        let _ = m.gotcha_add(&serde_json::json!({"pattern": "rm -rf /"}));
        let _ = m.memory_store(&serde_json::json!({"key": "k", "value": "v"}));
        assert_eq!(m.rewards.len(), 1, "reward recorded");
        assert_eq!(m.gotchas.len(), 1, "gotcha recorded");
        assert_eq!(m.memories.len(), 1, "memory recorded");
    }

    /// Fail-open contract: a returned JSON string (even an error one) is the
    /// only channel — the trait surface itself cannot panic the caller.
    #[test]
    fn learn_runtime_methods_return_json_string() {
        let mut m = MockLearn::default();
        let out = m.learning_reward(&serde_json::json!({}));
        assert!(out.starts_with('{'), "must return a JSON object string");
    }
}
