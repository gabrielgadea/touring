//! The Code Execution Gateway **driver** and the **pre-exec hook**. Phase
//! **P3.7** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`) — the production
//! wiring that turns the `X0..X7` typestate pipeline into something a CLI and a
//! hook can drive end to end.
//!
//! # The X0..X9 mapping
//!
//! [`run_gateway`] runs the typestate pipeline `X0..X7` (`Execution`) with
//! dependency-injected closures, and returns a [`GatewayOutcome`].
//!
//! - **X0..X7** — capture → classify → static → VGP → predict → sandbox →
//!   capability-gate → decision. Infallible once entered (the typestate
//!   guarantees it); the only failures are entry-layer ([`GatewayError`]).
//! - **X8 EXECUTE** — the caller *honours* the verdict: `touring exec` reflects
//!   it in its exit code, the hook in its `HookResponse`. The *kernel-enforced
//!   supervised run* is CEG Pln2 **P4.2** (`run_supervised` + landlock); P3.7
//!   delivers the decision that gates it.
//! - **X9 LEARN** — the [`GatewayOutcome`] (id + decision + the full evidence
//!   ledger) *is* the structured learn-record. Feeding it to the RL
//!   `outcome:*` ledger as a run success / failure needs the real run, so that
//!   write also lands with P4.2.
//!
//! # Why X5 is deferred by default
//!
//! X5's value is *behavioural* — observe the code actually running. But the
//! sandbox is not yet filesystem-isolated (landlock is **P4.2**): a real
//! dry-run of untrusted code without isolation would *be* the harm the gateway
//! exists to prevent. So the production X5 runner is [`deferred_dry_run`] — it
//! does **not** execute. P3.7's gateway decides on the four *non-executing*
//! analyses (X2 static, X3 VGP, X4 predict, X6 capability) — which is exactly
//! what a gate should do before a safe room exists. [`guarded_dry_run`] is the
//! opt-in real runner: it refuses the destructive-command catalogue outright
//! and only ever spawns what the structural validator has cleared.
//!
//! # Where the hook driver lives (Session A · F2, 2026-06-10)
//!
//! The `HookRuntime` / `HookResponse` adapter — `verdict_to_response`,
//! `gate_hook_input`, `run_observe_only`, `run_returning`, `run` — lives in
//! `crate::ceg_adapter`, keeping this module (and the whole `gateway/` tree)
//! parent-type-free and extraction-ready. The pure observe entry ([`observe`])
//! stays here: it touches no hook-protocol type.

use super::capture::capture_tool_call;
use super::decision::{GateDecision, Verdict};
use super::error::GatewayError;
use super::metrics::{record_ceg_captured, record_verdict_counters};
use super::speculative::{AcceptedPrefix, CandidateAction, rank_by_predicted, speculative_execute};
// ES1 P3.5 (2026-06-02): X3.5 PROVE pre-filter is wired in
// `run_gateway_speculative` via `filter_by_proof`, which dispatches the
// production prove_claim to the configured Z3/CVC5/Stub backend.
use super::dry_run_cache::cached_dry_run_in_sandbox;
#[cfg(feature = "txn_lock_enforcement")]
use super::exec_pool::ExecPool;
use super::fast_path::{fast_path_decision, pure_skip_outcome};
use super::predict::{ExecutionOutcomePredictor, OutcomeStats};
use super::sandbox_stage::{SandboxCapabilities, SandboxOutcome};
#[cfg(feature = "txn_lock_enforcement")]
use super::txn::AccessDeclaration;
use super::typestate::{Evidence, ExecutionId, RawInvocation};
use crate::capability::{CapabilityProfile, builtins};
use crate::gateway::offensive_integration::prove_claim;
use serde::Serialize;
use touring_hooks_shared::bash_ast_validator::{Verdict as BashVerdict, validate_command};

// ── Gateway dependencies + outcome ────────────────────────────────────────────

/// The dependency-injected closures and values the [`run_gateway`] driver needs
/// to run the `X3` / `X4` / `X5` / `X6` stages.
///
/// Closure injection keeps the driver crate-agnostic and unit-testable: tests
/// pass mocks, `touring exec` and the pre-exec hook pass the production
/// closures ([`soft_pass_symbol`], [`neutral_outcome_history`],
/// [`deferred_dry_run`] / [`guarded_dry_run`]).
pub struct GatewayDeps<'a> {
    /// **X3** — does this symbol resolve in the index?
    pub symbol_exists: &'a dyn Fn(&str) -> bool,
    /// **X4** — the observed outcome history for a signature key.
    pub outcome_history: &'a dyn Fn(&str) -> OutcomeStats,
    /// **X5** — the sandbox dry-run runner.
    pub sandbox_runner: &'a dyn Fn(&RawInvocation) -> SandboxOutcome,
    /// **X4** — the Beta success-probability estimator.
    pub predictor: &'a ExecutionOutcomePredictor,
    /// **X6** — the capability profile to gate against.
    pub profile: &'a CapabilityProfile,
    /// **X3.5 PROVE** — optional claim to verify before X4 prediction
    /// (ES1 P3, 2026-06-01). `None` ⇒ no Z3 cost (zero overhead for
    /// default callers); `Some(claim)` ⇒ `prove_claim` runs the
    /// configured backend and attaches a `ProofReport` to
    /// [`Evidence::proof_report`]. The stub backend returns
    /// `ProofStatus::Void` (honest no-op contract).
    pub claim: Option<crate::gateway::offensive_integration::ClaimKind>,
    /// **X3.5 PROVE** — context (free variables + budget) for the
    /// claim. Sensible defaults via `ClaimContext::default()` when
    /// the caller has no specific context to attach.
    pub claim_context: crate::gateway::offensive_integration::ClaimContext,
    /// **X3.5 PROVE** — which SMT backend to use. `Stub` is always
    /// available (returns `ProofStatus::Void`); `Z3` / `Cvc5` are
    /// feature-gated in `touring-offensive`.
    pub solver_backend: crate::gateway::offensive_integration::SolverBackendKind,
}

/// The result of a full [`run_gateway`] drive — the **X9 LEARN** record.
///
/// Carries the deterministic [`ExecutionId`], the terminal [`GateDecision`],
/// and the complete [`Evidence`] ledger (every stage's result). Serialisable so
/// `touring exec -j` can emit it verbatim.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GatewayOutcome {
    /// The content-addressed id of the execution.
    pub id: ExecutionId,
    /// The terminal X7 verdict.
    pub decision: GateDecision,
    /// The full per-stage evidence ledger.
    pub evidence: Evidence,
}

// ── Production closures ───────────────────────────────────────────────────────

/// **X3 default** — soft-pass: an unreachable index never flags a symbol.
///
/// P3.3 documents this as the degraded-caller behaviour: an unresolved symbol
/// is a yellow flag, never a hard block, so passing everything is the safe
/// default when no index is wired.
#[must_use]
pub fn soft_pass_symbol(_symbol: &str) -> bool {
    true
}

/// **X4 default** — neutral: no outcome history yields the neutral `0.5`
/// prediction (the Beta estimator's uniform-prior midpoint).
#[must_use]
pub fn neutral_outcome_history(_signature: &str) -> OutcomeStats {
    OutcomeStats::default()
}

/// The `capability_profile` marker a [`deferred_dry_run`] outcome carries — so
/// the deferral is fully visible in the evidence ledger and `-j` output.
pub const DEFERRED_PROFILE_MARKER: &str = "<X5-deferred:landlock-pending-P4.2>";

/// The `capability_profile` marker a [`guarded_dry_run`] outcome carries when
/// it *refuses* a structurally destructive command rather than spawning it.
pub const REFUSED_PROFILE_MARKER: &str = "<X5-refused:destructive-pattern>";

/// **X5 default (safe)** — a sandbox runner that does **not** execute.
///
/// Until P4.2 lands landlock isolation, a real dry-run of untrusted code is
/// itself unsafe (see the module docs). This runner returns a non-executing
/// [`SandboxOutcome`] marked with [`DEFERRED_PROFILE_MARKER`]: X5 abstains, the
/// decision rests on the non-executing analyses. The pre-exec hook and the
/// default `touring exec` use this runner — it is safe for *any* input.
#[must_use]
pub fn deferred_dry_run(_raw: &RawInvocation) -> SandboxOutcome {
    SandboxOutcome {
        exit_code: 0,
        output_bytes: 0,
        was_truncated: false,
        timed_out: false,
        content_hash: String::new(),
        capability_profile: DEFERRED_PROFILE_MARKER.to_owned(),
        summary: crate::gateway::OutputSummary::empty(0),
    }
}

/// **X5 opt-in (real)** — run the sandbox dry-run, but refuse the destructive
/// catalogue first.
///
/// A structurally destructive command (`rm -rf`, `git push --force`, …) is
/// caught by [`validate_command`] and reported as a refused outcome
/// ([`REFUSED_PROFILE_MARKER`]) — it is **never spawned**. This keeps the real
/// dry-run safe even before P4.2's landlock isolation. Everything the validator
/// clears runs in the rlimit + timeout sandbox via [`cached_dry_run_in_sandbox`]
/// — the content-hash-cached X5 runner (P4.5): a byte-identical code body skips
/// the sandbox subprocess and returns the cached outcome.
///
/// P4.6 — before the sandbox, a provably-pure body (no I/O, network, or
/// subprocess) takes the static-only fast path: [`fast_path_decision`] returns
/// [`pure_skip_outcome`] without running anything at all.
#[must_use]
pub fn guarded_dry_run(raw: &RawInvocation, caps: &SandboxCapabilities) -> SandboxOutcome {
    if let BashVerdict::Block { .. } = validate_command(&raw.payload) {
        return SandboxOutcome {
            exit_code: 126,
            output_bytes: 0,
            was_truncated: false,
            timed_out: false,
            content_hash: String::new(),
            capability_profile: REFUSED_PROFILE_MARKER.to_owned(),
            summary: crate::gateway::OutputSummary::empty(126),
        };
    }
    // P4.6 — a provably-pure body has no behaviour for X5 to observe; skip the
    // sandbox subprocess entirely via the static-only fast path.
    if fast_path_decision(raw).skips_sandbox() {
        return pure_skip_outcome();
    }
    cached_dry_run_in_sandbox(raw, caps)
}

// ── The driver ────────────────────────────────────────────────────────────────

/// Drive a code-bearing tool call through the full `X0..X7` gateway pipeline.
///
/// Returns the [`GatewayOutcome`] — the X9 learn-record — or a
/// [`GatewayError`] when the *entry layer* fails (an empty body, or a tool that
/// does not run code). The `X0..X7` pipeline itself cannot fail once entered.
///
/// `deps` injects the X3 / X4 / X5 / X6 dependencies; see [`GatewayDeps`].
pub fn run_gateway(
    tool: &str,
    payload: &str,
    intent: Option<String>,
    deps: &GatewayDeps<'_>,
) -> Result<GatewayOutcome, GatewayError> {
    if payload.trim().is_empty() {
        return Err(GatewayError::EmptyPayload);
    }
    let captured =
        capture_tool_call(tool, payload, intent).ok_or_else(|| GatewayError::NotCodeBearing {
            tool: tool.to_owned(),
        })?;
    // X0 CAPTURE — every call that enters the X1..X7 pipeline counts.
    // Placed here (after the early-return guards) so only code-bearing, non-empty
    // payloads increment the counter, matching the "captured" semantics.
    record_ceg_captured();
    // ES3 P1 / S-10 (2026-06-01) — defense-in-depth: route every CEG X0 capture
    // through `ExecPool::acquire_txn` so concurrent sandbox-execs declare a
    // read/write-set and the `TxnLockManager` can refuse to grant two
    // declarations that conflict (write-write or write-read). The permit is
    // held for the duration of this synchronous call (run_gateway is single-
    // threaded) and dropped at scope end, releasing the access-set.
    //
    // **Honest scope note**: this is a READ-ONLY observability hook. Because
    // `run_gateway` returns *before* any actual sandbox execution happens
    // (X8 `supervised.rs` is the real execution surface), the lock cannot
    // prevent a lost update on writes today. The full OP4 §5.2.4 lost-update
    // guard for writes is the ES3 P2 deliverable: wire `acquire_txn` into
    // `supervised::run_supervised` (X8) so the permit spans the actual I/O.
    // What this *does* give us now: (a) a per-tool txid in gate-metrics,
    // (b) serialization of any future synchronous write-back hooks that
    // observe the same path/symbol, and (c) regression coverage proving the
    // declare-set path works end-to-end.
    #[cfg(feature = "txn_lock_enforcement")]
    let _txn_permit =
        match ExecPool::global().acquire_txn(AccessDeclaration::from_tool_payload(tool, payload)) {
            Ok(permit) => Some(permit),
            Err(conflict) => {
                tracing::warn!(
                    ?conflict,
                    tool,
                    "run_gateway: txn permit conflict — proceeding read-only"
                );
                None
            }
        };
    let decided = captured
        .classify() // X1
        .static_analyze() // X2
        .vgp_verify(deps.symbol_exists) // X3
        .prove_claim(
            // X3.5 (ES1 P3, 2026-06-01)
            deps.claim.clone(),
            deps.solver_backend,
            &deps.claim_context,
        )
        .predict(deps.predictor, deps.outcome_history) // X4
        .sandbox_dry_run(deps.sandbox_runner) // X5
        .capability_gate(deps.profile) // X6
        .decide(); // X7
    let decision = decided
        .evidence()
        .decision
        .clone()
        .expect("decide() attaches a GateDecision — P3.6 typestate invariant");
    Ok(GatewayOutcome {
        id: decided.id().clone(),
        decision,
        evidence: decided.evidence().clone(),
    })
}

/// **S-12** — drive a *batch* of candidate actions through the gateway and accept
/// the longest valid leading prefix: the harness analogue of EAGLE speculative
/// decoding's accepted draft length.
///
/// 1. **Rank** — order candidates most-likely-to-succeed first via
///    [`rank_by_predicted`], backed by the S-11 online model
///    ([`global_model_snapshot`](super::outcome_learner::global_model_snapshot) —
///    a clone, so no lock is held across a [`run_gateway`] call). Ranking changes
///    only *what is tried first*, never *what is accepted* (the lossless contract).
/// 2. **Speculate** — [`speculative_execute`] runs each ranked candidate's
///    `payload` through the full `X0..X7` pipeline as a `Bash` body and accepts
///    the longest leading run the gateway does **not** `Deny`; it stops at the
///    first `Deny` (truncation, never reordering past a failure).
///
/// Returns the [`AcceptedPrefix`] — the accepted indices (into the *ranked* list)
/// and the last accepted candidate's outcome. An empty prefix means the
/// best-ranked candidate was itself denied.
#[must_use]
pub fn run_gateway_speculative(
    candidates: &[CandidateAction],
    deps: &GatewayDeps<'_>,
) -> AcceptedPrefix {
    // ES1 P4 (2026-06-02): X3.5 PROVE per-candidate pre-filter. Each
    // candidate's `ClaimKind` is derived from its `ActionSignature.intent_class`
    // via `claim_from_intent`; the same `prove_claim` dispatch (Stub/Z3/CVC5)
    // is used, but with a per-candidate claim rather than the single-shared
    // `deps.claim` from P3.5. The `deps.claim` field is now UNUSED at this
    // call site (kept on `GatewayDeps` for backward compat — see P3 leftover
    // rule).
    //
    // MUST-KNOW edge case: per-candidate path + real Z3/CVC5 backend +
    // empty `ClaimContext::default()` → `prove_claim` returns `Error` →
    // ALL candidates with a derivable intent are DROPPED. Adopters
    // switching from `Stub` to a real backend MUST populate
    // `ClaimContext` with variables bound to the generated predicates.
    // Default callers (`Stub` backend) are safe — `prove_claim` returns
    // `Void` (kept) for the Stub backend.
    let proven = super::speculative::filter_by_proof_per_candidate(
        candidates,
        crate::gateway::offensive_integration::claim_from_intent,
        &deps.claim_context,
        deps.solver_backend,
        prove_claim,
    );
    let ranked = rank_by_predicted(proven, deps.predictor);
    speculative_execute(&ranked, |candidate| {
        // The gateway verdict is the speculative oracle: a non-Deny verdict keeps
        // the accepted prefix growing (exit 0 → succeeded()); a Deny or a
        // non-code-bearing payload truncates it (exit 126 → not succeeded()).
        let valid = matches!(
            run_gateway("Bash", &candidate.payload, None, deps),
            Ok(ref outcome) if outcome.decision.verdict != Verdict::Deny
        );
        SandboxOutcome {
            exit_code: if valid { 0 } else { 126 },
            output_bytes: 0,
            was_truncated: false,
            timed_out: false,
            content_hash: String::new(),
            capability_profile: String::new(),
            summary: crate::gateway::OutputSummary::empty(if valid { 0 } else { 126 }),
        }
    })
}

// ── The observe-only entry ──────────────────────────────────────────────────────

/// Wave 7.A (2026-05-23) — observe-only CEG entry, no `HookRuntime` required.
///
/// Drives the full X0..X7 pipeline against `(tool, body)` purely for the
/// side-effect of incrementing the gate-metrics counters
/// (`ceg_captured_count`, `ceg_fast_path_count`, `ceg_blocked_count`,
/// `workflow_advice_emitted_count`). Returns the [`GatewayOutcome`] so
/// callers can integrate the verdict if they want, but `None` is fully
/// acceptable — fail-open.
///
/// **Use case**: lifecycle hooks like `pre-bash` (the Claude Code Bash
/// hook) that already have their own deny gates and just want to feed the
/// CEG counters so the daemon's `touring gate-metrics -j` snapshot
/// reflects every Bash invocation, not just `touring exec`.
///
/// **Does not** call X9 LEARN — that requires a mutable runtime for RL
/// reward emission. The post-tool hooks (`post-bash`, etc.) already feed
/// the RL loop through a different path.
#[must_use = "the GatewayOutcome may carry useful evidence; ignore with `let _ = ...`"]
pub fn observe(tool: &str, body: &str) -> Option<GatewayOutcome> {
    let profile = builtins::trusted();
    let predictor = ExecutionOutcomePredictor::new();
    let deps = GatewayDeps {
        symbol_exists: &soft_pass_symbol,
        // S-11 — X4 PREDICT reads the global online model instead of the flat
        // neutral prior, so a seen action class predicts its learned success rate.
        outcome_history: &super::outcome_learner::global_model_outcome_history,
        sandbox_runner: &deferred_dry_run,
        predictor: &predictor,
        profile: &profile,
        // ES1 P3 (2026-06-01) — observe path also uses no claim by default
        // (zero Z3 cost). The CEG observability counters are unaffected.
        claim: None,
        claim_context: crate::gateway::offensive_integration::ClaimContext::default(),
        solver_backend: crate::gateway::offensive_integration::SolverBackendKind::Stub,
    };
    match run_gateway(tool, body, None, &deps) {
        Ok(outcome) => {
            record_verdict_counters(outcome.decision.verdict);
            Some(outcome)
        }
        // Non-code-bearing / empty / any other error → fail-open silently.
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trusted_caps() -> SandboxCapabilities {
        SandboxCapabilities::from_profile(&builtins::trusted())
    }

    /// Build a `GatewayDeps` body for a test — the caller owns `profile` and
    /// `predictor`, the closures are the production-safe defaults.
    fn deferred_deps<'a>(
        profile: &'a CapabilityProfile,
        predictor: &'a ExecutionOutcomePredictor,
    ) -> GatewayDeps<'a> {
        GatewayDeps {
            symbol_exists: &soft_pass_symbol,
            outcome_history: &neutral_outcome_history,
            sandbox_runner: &deferred_dry_run,
            predictor,
            profile,
            // ES1 P3 (2026-06-01) — test helper: no claim, Stub backend.
            claim: None,
            claim_context: crate::gateway::offensive_integration::ClaimContext::default(),
            solver_backend: crate::gateway::offensive_integration::SolverBackendKind::Stub,
        }
    }

    // ── production closures ───────────────────────────────────────────────

    #[test]
    fn soft_pass_symbol_always_passes() {
        assert!(soft_pass_symbol("AnySymbol"));
        assert!(soft_pass_symbol(""));
    }

    #[test]
    fn neutral_outcome_history_is_zero_zero() {
        let stats = neutral_outcome_history("outcome:bash:x:none");
        assert_eq!(stats.successes, 0);
        assert_eq!(stats.failures, 0);
    }

    #[test]
    fn deferred_dry_run_does_not_execute() {
        let outcome = deferred_dry_run(&RawInvocation::new("Bash", "echo hi"));
        assert_eq!(outcome.capability_profile, DEFERRED_PROFILE_MARKER);
        assert_eq!(outcome.output_bytes, 0);
        // The deferred runner abstains — it reports no failure.
        assert!(outcome.succeeded());
    }

    #[test]
    fn guarded_dry_run_refuses_destructive_without_spawning() {
        // `rm -rf /` is in the destructive catalogue — `guarded_dry_run` must
        // refuse it via the structural validator and NEVER spawn a subprocess.
        let outcome = guarded_dry_run(&RawInvocation::new("Bash", "rm -rf /"), &trusted_caps());
        assert_eq!(outcome.capability_profile, REFUSED_PROFILE_MARKER);
        assert_eq!(outcome.exit_code, 126);
        assert!(
            !outcome.succeeded(),
            "a refused command must not look successful"
        );
    }

    // ── run_gateway ───────────────────────────────────────────────────────

    #[test]
    fn run_gateway_rejects_empty_payload() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = deferred_deps(&profile, &predictor);
        assert_eq!(
            run_gateway("Bash", "   ", None, &deps),
            Err(GatewayError::EmptyPayload)
        );
    }

    #[test]
    fn run_gateway_rejects_non_code_bearing_tool() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = deferred_deps(&profile, &predictor);
        match run_gateway("Read", "/etc/hosts", None, &deps) {
            Err(GatewayError::NotCodeBearing { tool }) => assert_eq!(tool, "Read"),
            other => panic!("expected NotCodeBearing, got {other:?}"),
        }
    }

    #[test]
    fn run_gateway_produces_a_decision_for_a_clean_command() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = deferred_deps(&profile, &predictor);
        let outcome = run_gateway("Bash", "echo hello", None, &deps).expect("clean command");
        assert!(outcome.id.as_str().starts_with("exec-"));
        // `echo` under Trusted is clean — Allow.
        assert_eq!(outcome.decision.verdict, Verdict::Allow);
        assert_eq!(outcome.evidence.stage_log.len(), 9);
    }

    #[test]
    fn run_gateway_denies_a_destructive_command() {
        // The deferred X5 runner never spawns, so this is safe to run: the
        // verdict comes from X2 STATIC (a destructive pattern → Block) and X6.
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = deferred_deps(&profile, &predictor);
        let outcome = run_gateway("Bash", "rm -rf /", None, &deps).expect("gated");
        assert_eq!(
            outcome.decision.verdict,
            Verdict::Deny,
            "`rm -rf /` must never be allowed"
        );
        assert!(outcome.decision.canonical_fix.is_some());
    }

    // ── E2E ───────────────────────────────────────────────────────────────

    #[test]
    fn e2e_guarded_dry_run_executes_a_safe_command_for_real() {
        // `guarded_dry_run` on a non-destructive command DOES spawn the real
        // sandbox subprocess. `echo` is safe and in no destructive catalogue.
        let outcome = guarded_dry_run(
            &RawInvocation::new("Bash", "echo ceg-p37-probe"),
            &trusted_caps(),
        );
        // It really ran — the profile marker is the sandbox's, not a refusal.
        assert_ne!(outcome.capability_profile, REFUSED_PROFILE_MARKER);
        assert_ne!(outcome.capability_profile, DEFERRED_PROFILE_MARKER);
    }

    #[test]
    fn e2e_run_gateway_full_pipeline_records_all_eight_stages() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = deferred_deps(&profile, &predictor);
        let outcome = run_gateway("Bash", "cargo build", None, &deps).expect("gated");
        // X9 record completeness: every stage X0..X7 logged, a decision present.
        assert_eq!(outcome.evidence.stage_log.len(), 9);
        assert!(outcome.evidence.classification.is_some());
        assert!(outcome.evidence.gate_report.is_some());
        assert!(outcome.evidence.decision.is_some());
    }

    // ── S-12 — speculative batch driver ─────────────────────────────────────

    #[test]
    fn speculative_accepts_prefix_until_first_deny() {
        use touring_hooks_shared::action_signature::{ActionSignature, ContextQualifier};

        // A unique, unseen feature class → every candidate predicts the neutral
        // 0.5, so rank_by_predicted's stable sort preserves input order and the
        // test is deterministic regardless of the process-global model's state.
        let mk = |id: &str, payload: &str| {
            CandidateAction::new(
                id,
                payload,
                ActionSignature {
                    tool_class: "bash".to_owned(),
                    intent_class: "spec12unseen".to_owned(),
                    context_qualifier: ContextQualifier::Plain,
                },
            )
        };
        let candidates = vec![
            mk("c0", "echo one"),
            mk("c1", "echo two"),
            mk("c2", "rm -rf /"),  // destructive → X2 STATIC Deny
            mk("c3", "echo four"), // never drafted (past the failure)
        ];

        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = GatewayDeps {
            symbol_exists: &soft_pass_symbol,
            outcome_history: &neutral_outcome_history,
            sandbox_runner: &deferred_dry_run,
            predictor: &predictor,
            profile: &profile,
            // ES1 P3 (2026-06-01) — speculative test: no claim, Stub.
            claim: None,
            claim_context: crate::gateway::offensive_integration::ClaimContext::default(),
            solver_backend: crate::gateway::offensive_integration::SolverBackendKind::Stub,
        };

        let prefix = run_gateway_speculative(&candidates, &deps);
        // The two clean echoes are accepted; the destructive command truncates
        // the prefix and the candidate after it is never drafted (lossless).
        assert_eq!(
            prefix.valid_indices,
            vec![0, 1],
            "the accepted prefix must stop before the Deny"
        );
        assert_eq!(prefix.len(), 2);
        assert!(prefix.final_outcome.is_some_and(|o| o.succeeded()));
    }

    // ── ES1 P3 (2026-06-01) — X3.5 PROVE integration test ─────────────────

    // ── ES1 P3.5 (2026-06-02) — X3.5 PROVE pre-filter integration test ─────

    /// End-to-end: when the speculative driver is given a `Claim` with
    /// the Stub backend, `filter_by_proof` returns `Void` for every
    /// candidate (the P1 honest no-op contract) and every candidate
    /// passes the pre-filter. The accepted prefix therefore contains
    /// all 3 candidate indices — the lossless contract is preserved
    /// through the new pre-pass.
    #[test]
    fn run_gateway_speculative_with_proof_filter_passes_via_stub() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let mut deps = deferred_deps(&profile, &predictor);
        // ES1 P3.5: wire a real claim with the Stub backend.
        deps.claim = Some(
            crate::gateway::offensive_integration::ClaimKind::Postcondition {
                predicate: "rc == 0".to_owned(),
            },
        );
        deps.claim_context = crate::gateway::offensive_integration::ClaimContext::default();
        deps.solver_backend = crate::gateway::offensive_integration::SolverBackendKind::Stub;

        use super::super::speculative::CandidateAction;
        use touring_hooks_shared::action_signature::ActionSignature;
        let mk = |id: &str, payload: &str| {
            CandidateAction::new(
                id,
                payload,
                ActionSignature {
                    tool_class: "bash".to_owned(),
                    intent_class: "noop".to_owned(),
                    context_qualifier:
                        touring_hooks_shared::action_signature::ContextQualifier::Plain,
                },
            )
        };
        let candidates = vec![
            mk("c1", "echo one"),
            mk("c2", "echo two"),
            mk("c3", "echo three"),
        ];

        let prefix = run_gateway_speculative(&candidates, &deps);
        // Stub returns Void → all 3 candidates pass the pre-filter;
        // none is destructive → the speculative loop accepts all 3.
        assert_eq!(
            prefix.len(),
            3,
            "Stub Void + non-destructive payloads must accept all 3 candidates"
        );
        assert_eq!(prefix.valid_indices, vec![0, 1, 2]);
        assert!(prefix.final_outcome.is_some_and(|o| o.succeeded()));
    }

    /// End-to-end: with `claim: None`, the filter is a no-op (zero
    /// overhead identity). The speculative driver produces the same
    /// prefix it would have produced before P3.5.
    #[test]
    fn run_gateway_speculative_with_proof_filter_opt_in_none_identity() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = deferred_deps(&profile, &predictor);
        // claim remains None (the default).
        assert!(deps.claim.is_none(), "test precondition: claim is None");

        use super::super::speculative::CandidateAction;
        use touring_hooks_shared::action_signature::ActionSignature;
        let mk = |id: &str, payload: &str| {
            CandidateAction::new(
                id,
                payload,
                ActionSignature {
                    tool_class: "bash".to_owned(),
                    intent_class: "noop".to_owned(),
                    context_qualifier:
                        touring_hooks_shared::action_signature::ContextQualifier::Plain,
                },
            )
        };
        let candidates = vec![mk("a", "echo a"), mk("b", "echo b")];
        let prefix = run_gateway_speculative(&candidates, &deps);
        assert_eq!(prefix.len(), 2, "opt-in None must be a pure pass-through");
        assert_eq!(prefix.valid_indices, vec![0, 1]);
    }

    /// End-to-end: when a `Claim` is wired with the Stub backend and a
    /// candidate is destructive (`rm -rf /`), the pre-filter is
    /// neutral (Stub=Void) and the speculative loop still truncates
    /// at the first Deny — the lossless contract holds across the
    /// new pre-pass even when a destructive candidate is present.
    #[test]
    fn run_gateway_speculative_with_proof_filter_still_truncates_on_deny() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let mut deps = deferred_deps(&profile, &predictor);
        deps.claim = Some(
            crate::gateway::offensive_integration::ClaimKind::Postcondition {
                predicate: "always".to_owned(),
            },
        );
        deps.claim_context = crate::gateway::offensive_integration::ClaimContext::default();
        deps.solver_backend = crate::gateway::offensive_integration::SolverBackendKind::Stub;

        use super::super::speculative::CandidateAction;
        use touring_hooks_shared::action_signature::{ActionSignature, ContextQualifier};
        // Use a unique, unseen intent class so the ranker leaves the
        // input order intact (test is deterministic regardless of the
        // process-global model state).
        let mk = |id: &str, payload: &str| {
            CandidateAction::new(
                id,
                payload,
                ActionSignature {
                    tool_class: "bash".to_owned(),
                    intent_class: "spec12p35unseen".to_owned(),
                    context_qualifier: ContextQualifier::Plain,
                },
            )
        };
        let candidates = vec![
            mk("c0", "echo one"),
            mk("c1", "echo two"),
            mk("c2", "rm -rf /"),  // destructive → X2 STATIC Deny
            mk("c3", "echo four"), // never drafted (past the failure)
        ];
        let prefix = run_gateway_speculative(&candidates, &deps);
        // The two clean echoes are accepted; the destructive command
        // truncates the prefix; the trailing echo is never drafted.
        assert_eq!(
            prefix.valid_indices,
            vec![0, 1],
            "lossless: prefix stops before the Deny, no reorder"
        );
        assert_eq!(prefix.len(), 2);
    }

    // ── ES1 P4 (2026-06-02) — per-candidate X3.5 PROVE integration tests ──

    /// End-to-end: with default `GatewayDeps` (claim=None, Stub backend, empty
    /// ClaimContext), the per-candidate path is invoked at L295 with
    /// `claim_from_intent` as the claim derivation closure. For intent
    /// classes that DO derive a claim (`cargo`, `rs`), the Stub backend
    /// returns `Void` → all kept. For intent classes that don't (`unknown`,
    /// `md`, `mcp-*`), the per-candidate closure returns `None` → identity
    /// transform. Net effect: all 3 candidates pass the pre-filter and are
    /// accepted by the speculative loop.
    #[test]
    fn run_gateway_speculative_with_per_candidate_filter_default_deps_is_identity() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let deps = deferred_deps(&profile, &predictor);
        // default: claim=None, ClaimContext::default(), solver_backend=Stub

        use super::super::speculative::CandidateAction;
        use touring_hooks_shared::action_signature::ContextQualifier;
        let mk = |id: &str, intent: &str| {
            CandidateAction::new(
                id,
                "echo p4-acceptance",
                touring_hooks_shared::action_signature::ActionSignature {
                    tool_class: "Bash".to_owned(),
                    intent_class: intent.to_owned(),
                    context_qualifier: ContextQualifier::Plain,
                },
            )
        };
        // Per-candidate claim derivation path:
        //   "cargo"  → claim_from_intent returns Some(...) → Stub→Void → kept
        //   "rs"     → claim_from_intent returns Some(...) → Stub→Void → kept
        //   "unknown"→ claim_from_intent returns None → identity
        let candidates = vec![mk("a", "cargo"), mk("b", "rs"), mk("c", "unknown")];

        let prefix = run_gateway_speculative(&candidates, &deps);
        assert_eq!(
            prefix.len(),
            3,
            "default deps + Stub backend must preserve all 3 candidates \
             (per-candidate Some→Void is kept, None is identity); got prefix {:?}",
            prefix.valid_indices
        );
    }

    /// End-to-end: even with a `Claim` wired on `GatewayDeps` (kept for
    /// backward compat — see P3 leftover rule), the per-candidate path
    /// uses `claim_from_intent` for each candidate's own claim. With
    /// `cargo` and `md` candidates, the per-candidate closure returns
    /// `Some(...)` for `cargo` (derivable) and `None` for `md` (no
    /// claim). The Stub backend returns `Void` for the cargo candidate
    /// (kept); the `md` candidate is identity (kept). Net: both pass
    /// the pre-filter and the speculative loop.
    #[test]
    fn run_gateway_speculative_with_per_candidate_filter_bash_cargo_with_stub() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let mut deps = deferred_deps(&profile, &predictor);
        // Wire a real `Claim` (kept on `GatewayDeps` for backward compat
        // per P3 leftover rule) — the per-candidate path IGNORES it in
        // favour of `claim_from_intent` per candidate.
        deps.claim = Some(
            crate::gateway::offensive_integration::ClaimKind::Postcondition {
                predicate: "rc == 0".to_owned(),
            },
        );
        deps.claim_context = crate::gateway::offensive_integration::ClaimContext::default();
        deps.solver_backend = crate::gateway::offensive_integration::SolverBackendKind::Stub;

        use super::super::speculative::CandidateAction;
        use touring_hooks_shared::action_signature::ContextQualifier;
        let mk = |id: &str, intent: &str, payload: &str| {
            CandidateAction::new(
                id,
                payload,
                touring_hooks_shared::action_signature::ActionSignature {
                    tool_class: "Bash".to_owned(),
                    intent_class: intent.to_owned(),
                    context_qualifier: ContextQualifier::Plain,
                },
            )
        };
        let candidates = vec![
            mk("a", "cargo", "echo p4-cargo-acceptance"),
            mk("b", "md", "echo p4-md-acceptance"),
        ];

        let prefix = run_gateway_speculative(&candidates, &deps);
        assert_eq!(
            prefix.len(),
            2,
            "lossless contract: both candidates must pass with Stub backend \
             (cargo→Some→Void→kept, md→None→identity)"
        );
    }

    /// End-to-end: when the caller supplies a `Claim`, the gateway
    #[test]
    fn run_gateway_with_claim_attaches_proof_report_to_evidence() {
        let profile = builtins::trusted();
        let predictor = ExecutionOutcomePredictor::new();
        let mut deps = deferred_deps(&profile, &predictor);
        // ES1 P3 (2026-06-01) — X3.5 PROVE: override the default `None`
        // claim with a real ClaimKind so the integration can verify
        // that a `ProofReport` is attached to the evidence.
        deps.claim = Some(
            crate::gateway::offensive_integration::ClaimKind::Postcondition {
                predicate: "x > 0".to_owned(),
            },
        );
        deps.claim_context = crate::gateway::offensive_integration::ClaimContext::default();
        deps.solver_backend = crate::gateway::offensive_integration::SolverBackendKind::Stub;
        let outcome = run_gateway("Bash", "ls", None, &deps).expect("clean ls must run");
        assert!(
            outcome.evidence.proof_report.is_some(),
            "prove_claim must attach a ProofReport when called with Some(_)"
        );
        let report = outcome.evidence.proof_report.as_ref().unwrap();
        assert_eq!(
            report.status,
            crate::gateway::offensive_integration::ProofStatus::Void,
            "Stub backend must return Void (honest no-op contract)"
        );
        assert_eq!(
            report.backend_used,
            crate::gateway::offensive_integration::SolverBackendKind::Stub
        );
        // The stage_log must contain X3.5-PROVE (9 records: X0 + 8 transitions).
        let log = &outcome.evidence.stage_log;
        assert_eq!(log.len(), 9);
        assert_eq!(log[4].stage, "X3.5-PROVE");
    }
}

// =====================================================================
// ES3 P1 / S-10 (2026-06-01) — live txn-permit E2E coverage
//
// Verifies that `ExecPool::acquire_txn` + `AccessDeclaration::from_tool_payload`
// form a working defense-in-depth path: a permit is granted, the active
// count goes 0→1→0 across the scope, and read-read overlap does not
// conflict. This is the same call site that `run_gateway` uses after
// S-10 default-on; we exercise the building blocks directly to avoid
// pulling the full `GatewayDeps` lifetime-bound subsystem into a test.
// =====================================================================
#[cfg(test)]
#[cfg(feature = "txn_lock_enforcement")]
mod txn_live_e2e {
    use super::super::exec_pool::{ExecPool, PoolConfig};
    use super::super::txn::{AccessDeclaration, AccessPath, AcquireResult, TxnLockManager};
    use std::time::Duration;

    #[test]
    fn acquire_txn_grants_permit_for_pure_reader_decl_from_tool_payload() {
        // Own pool, not `ExecPool::global()`: the singleton's txn count is
        // shared across the parallel test binary, so absolute before/after
        // assertions are racy (hardened 2026-06-10 after a scheduling flake).
        let pool = ExecPool::new(PoolConfig::new(4, Duration::from_secs(1)));
        let baseline = pool.active_txn_count();

        // Build the same AccessDeclaration that run_gateway's call site
        // would build, via the new from_tool_payload helper.
        let decl = AccessDeclaration::from_tool_payload("Read", "/tmp/shared_target.rs");

        // Verify the declared access set is exactly what we expect.
        assert!(
            decl.reads.contains(&AccessPath::Symbol("Read".to_owned())),
            "must include Symbol(\"Read\") entry; got reads={:?}",
            decl.reads
        );
        assert!(
            decl.reads
                .contains(&AccessPath::Path("/tmp/shared_target.rs".to_owned())),
            "must include Path entry; got reads={:?}",
            decl.reads
        );
        assert!(decl.writes.is_empty(), "pure reader must have empty writes");
        assert!(decl.is_read_only());

        // Acquire the permit; for pure readers, this always grants.
        let permit = pool
            .acquire_txn(decl.clone())
            .expect("read-only declaration should be granted");

        // While the permit is held, the active count must increment.
        let during = pool.active_txn_count();
        assert!(
            during > baseline,
            "active_txn_count must increase while permit is held (baseline={}, during={})",
            baseline,
            during
        );

        // Drop the permit explicitly; the active count must return to baseline.
        drop(permit);
        let after = pool.active_txn_count();
        assert_eq!(
            after, baseline,
            "active_txn_count must return to baseline after permit drop (baseline={}, after={})",
            baseline, after
        );
    }

    #[test]
    fn back_to_back_pure_reader_permits_coexist() {
        // Two pure readers declaring the same Path read-set must coexist —
        // read-read is not a conflict in the lock manager. This proves the
        // ES3 P1 default-on wiring does not introduce a false-positive
        // conflict for the dominant read-only traffic pattern.
        // Own pool, not `ExecPool::global()`: the singleton's txn count is
        // shared across the parallel test binary, so absolute before/after
        // assertions are racy (hardened 2026-06-21 — same scheduling flake as
        // the sibling pure-reader test above, which already uses an own pool).
        let pool = ExecPool::new(PoolConfig::new(4, Duration::from_secs(1)));
        let baseline = pool.active_txn_count();

        let decl_a = AccessDeclaration::from_tool_payload("Read", "/tmp/shared_target.rs");
        let decl_b = AccessDeclaration::from_tool_payload("Bash", "/tmp/shared_target.rs");

        let p_a = pool.acquire_txn(decl_a).expect("decl A granted");
        let p_b = pool
            .acquire_txn(decl_b)
            .expect("decl B granted (read-read coexist)");

        // Two pure readers both held simultaneously.
        let during = pool.active_txn_count();
        assert_eq!(
            during,
            baseline + 2,
            "both pure-reader permits must be active concurrently (baseline={}, during={})",
            baseline,
            during
        );

        drop(p_a);
        drop(p_b);

        let after = pool.active_txn_count();
        assert_eq!(after, baseline, "all permits released after drop");
    }

    #[test]
    fn from_tool_payload_via_lock_manager_managed_directly() {
        // Sanity: from_tool_payload declarations are well-formed for the
        // underlying TxnLockManager (not just ExecPool::acquire_txn).
        let mut mgr = TxnLockManager::new();
        let decl = AccessDeclaration::from_tool_payload("Read", "/tmp/x.rs");
        assert_eq!(mgr.try_acquire(42, decl), AcquireResult::Granted);
        assert_eq!(mgr.active_count(), 1);
    }

    // ── ES1 P3 (2026-06-01) — X3.5 PROVE integration test ─────────────────

    // ── ES1 P3 (2026-06-01) — X3.5 PROVE integration test ─────────────────

    // (Duplicate removed — the canonical integration test lives in
    // the first `mod tests` block above, where `use super::*;` is
    // already in scope. This cfg-gated sub-mod deliberately omits
    // the gateway-level imports to keep its scope focused on the
    // txn-permit E2E coverage.)
}
