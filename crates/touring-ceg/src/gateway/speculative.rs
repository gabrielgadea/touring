//! **S-12 / R11 — the self-speculative execution loop.**
//!
//! The CEG already has the three ingredients of EAGLE-style speculation — X4
//! PREDICT (a drafter via `LearnedOutcomeModel`), X5 SANDBOX
//! ([`dry_run_in_sandbox`](super::sandbox_stage::dry_run_in_sandbox)), and the
//! blake3-keyed [`DryRunCache`](super::dry_run_cache) — but no loop that ties
//! them into *draft → verify → accept-longest-valid-prefix*.
//!
//! This module is that loop, at the **action** level (the EAGLE analogue of
//! token-level speculative decoding):
//!
//! 1. **Draft** N candidate actions, ranked by the predictor
//!    ([`rank_by_predicted`]).
//! 2. **Verify** each candidate via a dry-run (the injected `verify` closure;
//!    production wraps `dry_run_in_sandbox`, tests pass a mock).
//! 3. **Accept the longest valid leading prefix** ([`speculative_execute`]) —
//!    the maximal run of candidates that each verify clean, in order.
//!
//! # The lossless contract
//!
//! Acceptance is **truncation only, never reordering**: the accepted prefix is
//! exactly the maximal leading run of clean verifications, so it is identical to
//! what sequential verify-then-stop would accept. A candidate after the first
//! failure is *never* promoted past it — even if it would have verified clean —
//! because doing so would change the action order the prefix represents. This is
//! the `speculative_loop_lossless` invariant the test pins.

use super::outcome_learner::ActionFeatures;
use super::predict::{ExecutionOutcomePredictor, PredictionConfidence};
use super::sandbox_stage::SandboxOutcome;
use touring_hooks_shared::action_signature::ActionSignature;

/// A drafted candidate action awaiting verification.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateAction {
    /// A human-readable id for the candidate (e.g. `"high-conf"`).
    pub id: String,
    /// The command / code payload to dry-run.
    pub payload: String,
    /// The action signature (drives the predictor ranking + X9 learn key).
    pub signature: ActionSignature,
    /// The predicted success probability — drafting/ordering signal only; it is
    /// **never** used for acceptance (acceptance is by verification).
    pub predicted_success: f64,
    /// ES4 P5 — calibrated confidence bucket (None / Low / Medium / High).
    /// Surfaced in `touring exec-speculative -j` for operator visibility.
    /// `None` when the candidate has no observations yet (the predictor returns
    /// the bare prior; calibration has nothing to bucket).
    pub predicted_confiance: Option<PredictionConfidence>,
    /// ES4 P5 — per-candidate Brier contribution `(1 - predicted_success)^2`.
    /// Surfaced in `touring exec-speculative -j` and feeds the global running
    /// Brier sum via `record_outcome_learner_brier` (in rank_by_predicted).
    pub brier_contribution: Option<f64>,
}

impl CandidateAction {
    /// A candidate with a not-yet-scored prediction (`0.0`) and no calibrated
    /// fields yet (filled in by [`rank_by_predicted`]).
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        payload: impl Into<String>,
        signature: ActionSignature,
    ) -> Self {
        Self {
            id: id.into(),
            payload: payload.into(),
            signature,
            predicted_success: 0.0,
            predicted_confiance: None,
            brier_contribution: None,
        }
    }
}

/// The longest valid leading prefix accepted by [`speculative_execute`].
#[derive(Debug, Clone)]
pub struct AcceptedPrefix {
    /// Indices `0..k` of the candidates that verified clean, in order.
    pub valid_indices: Vec<usize>,
    /// The dry-run outcome of the last accepted candidate; `None` when the prefix
    /// is empty (the first candidate already failed).
    pub final_outcome: Option<SandboxOutcome>,
}

impl AcceptedPrefix {
    /// `true` when no candidate verified clean.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.valid_indices.is_empty()
    }

    /// The accepted prefix length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.valid_indices.len()
    }
}

/// Rank candidates by the learned predictor — the **drafter**. Each candidate's
/// `predicted_success` is set from the **durable** `LearnedOutcomeModel`
/// (cold-start-aware, S-11 + ES4 P4 wiring) — `global_model_snapshot()` is
/// read LIVE inside this function, so speculative decisions see any
/// observation learned mid-session (not a function-entry snapshot). The
/// list is sorted by descending probability (stable: ties keep input
/// order). Ranking changes *what is tried first*, never *what is accepted*.
#[must_use]
pub fn rank_by_predicted(
    mut candidates: Vec<CandidateAction>,
    prior: &ExecutionOutcomePredictor,
) -> Vec<CandidateAction> {
    // ES4 P4 — pull the LIVE global model (RwLock shared with the writer
    // path in post_tool_rl) so speculative decisions see any observation
    // learned mid-session, not the in-memory snapshot at function-entry.
    let model = crate::gateway::outcome_learner::global_model_snapshot();
    touring_hooks_shared::gate_metrics::record_speculative_durable_model_queries(candidates.len());
    for c in &mut candidates {
        let feats = ActionFeatures::from_signature(&c.signature);
        let (p, _, _) = model.predict_from_features(&feats, prior);
        c.predicted_success = p;
        // ES4 P5 — also populate the calibrated fields (confidence bucket +
        // Brier contribution) via the new prediction_calibrated substrate.
        // The 3 X4-observable counters (predict / brier / cold_start) get
        // incremented here as a side effect of every draft. `stats_for`
        // returns `None` for an unseen (tool, intent, ctx) triple; we use
        // a default `OutcomeStats` so the predictor returns the bare prior
        // and the bucket is `PredictionConfidence::None` (cold start).
        let stats = model.stats_for(&feats).unwrap_or_default();
        let calibrated = prior.prediction_calibrated(&stats);
        c.predicted_confiance = Some(calibrated.confidence);
        c.brier_contribution = Some(calibrated.brier_contribution);
    }
    candidates.sort_by(|a, b| {
        b.predicted_success
            .partial_cmp(&a.predicted_success)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

/// X3.5 PROVE pre-filter for speculative execution (ES1 P3.5, 2026-06-02).
///
/// Drops candidates whose claim is `Unsat` (claim provably false = reject) or
/// `Error` (encoder/solver failure = fail-closed). Keeps `Sat` (candidate
/// satisfies claim), `Void` (Stub = neutral no-op), and `Unknown` (solver
/// inconclusive = neutral).
///
/// **OPT-IN**: when `claim` is `None`, returns the input slice unchanged
/// (zero overhead, default callers unaffected — preserves the existing
/// `run_gateway_speculative` callers that never wire a claim).
///
/// **INJECTABLE PROVER**: the `prove_closure` parameter lets tests inject a
/// fake prover returning deterministic statuses. Production code passes
/// [`crate::gateway::offensive_integration::prove_claim`] (the real
/// Z3/CVC5/Stub backend dispatch).
#[must_use]
pub fn filter_by_proof<F>(
    candidates: &[CandidateAction],
    claim: Option<&crate::gateway::offensive_integration::ClaimKind>,
    claim_ctx: &crate::gateway::offensive_integration::ClaimContext,
    backend: crate::gateway::offensive_integration::SolverBackendKind,
    prove_closure: F,
) -> Vec<CandidateAction>
where
    F: Fn(
        &crate::gateway::offensive_integration::ClaimKind,
        &crate::gateway::offensive_integration::ClaimContext,
        crate::gateway::offensive_integration::SolverBackendKind,
    ) -> crate::gateway::offensive_integration::ProofReport,
{
    let Some(claim) = claim else {
        return candidates.to_vec();
    };
    candidates
        .iter()
        .filter(|_c| {
            let report = prove_closure(claim, claim_ctx, backend);
            !matches!(
                report.status,
                crate::gateway::offensive_integration::ProofStatus::Unsat
                    | crate::gateway::offensive_integration::ProofStatus::Error
            )
        })
        .cloned()
        .collect()
}

/// X3.5 PROVE pre-filter, **per-candidate** variant (ES1 P4, 2026-06-02).
///
/// Each candidate gets its own `ClaimKind` derived from its `ActionSignature`
/// via the `claim_for` closure. Identity transform when the closure returns
/// `None` for a candidate (no claim derivable — same zero-overhead guarantee
/// as `filter_by_proof`'s `claim: None` path).
///
/// # Veto logic
///
/// Same as [`filter_by_proof`]: drop on `ProofStatus::Unsat` (claim
/// provably false = reject) or `ProofStatus::Error` (encoder/solver failure
/// = fail-closed). Keep on `Sat` (candidate satisfies claim), `Void`
/// (Stub = neutral no-op), and `Unknown` (solver inconclusive = neutral).
///
/// # Per-candidate cost
///
/// 1 `claim_for` call + 1 `prove_closure` call per candidate (vs
/// `filter_by_proof` which is 0 + 1 per candidate, but with the **SAME**
/// claim for all). Stub backend is effectively free (Void). Real
/// Z3/CVC5 cost is N× (acceptable for the X9 RL filter use case, typically
/// N ≤ 10).
///
/// # MUST-KNOW edge case
///
/// `claim_for` returns `Some` AND caller wires a real Z3/CVC5 backend AND
/// `ClaimContext::default()` is empty → `prove_closure` (the production
/// `prove_claim`) returns `ProofStatus::Error` (encoder cannot bind
/// free variables). All candidates with a derivable claim are then
/// dropped. Adopters switching from `Stub` to a real backend MUST
/// populate `ClaimContext` with the variables bound to the generated
/// predicates. Default callers (`Stub` backend) are safe.
#[must_use]
pub fn filter_by_proof_per_candidate<F, G>(
    candidates: &[CandidateAction],
    claim_for: F,
    ctx: &crate::gateway::offensive_integration::ClaimContext,
    backend: crate::gateway::offensive_integration::SolverBackendKind,
    prove_closure: G,
) -> Vec<CandidateAction>
where
    F: Fn(
        &touring_hooks_shared::action_signature::ActionSignature,
    ) -> Option<crate::gateway::offensive_integration::ClaimKind>,
    G: Fn(
        &crate::gateway::offensive_integration::ClaimKind,
        &crate::gateway::offensive_integration::ClaimContext,
        crate::gateway::offensive_integration::SolverBackendKind,
    ) -> crate::gateway::offensive_integration::ProofReport,
{
    candidates
        .iter()
        .filter(|c| {
            // Identity transform: when no claim is derivable for a candidate,
            // it passes through untouched. Conservative under-declaration.
            let Some(claim) = claim_for(&c.signature) else {
                return true;
            };
            let report = prove_closure(&claim, ctx, backend);
            !matches!(
                report.status,
                crate::gateway::offensive_integration::ProofStatus::Unsat
                    | crate::gateway::offensive_integration::ProofStatus::Error
            )
        })
        .cloned()
        .collect()
}

/// Verify candidates in order and accept the **longest valid leading prefix**.
///
/// `verify` is the dry-run oracle — production passes a closure over
/// [`dry_run_in_sandbox`](super::sandbox_stage::dry_run_in_sandbox); tests pass a
/// mock. The loop accepts each candidate whose outcome
/// [`succeeded`](super::sandbox_stage::SandboxOutcome::succeeded), and **stops at
/// the first failure** (truncation, never reordering — the lossless contract).
/// The first failing candidate is dry-run (to discover it fails) but not
/// accepted, and nothing past it is even drafted into the prefix.
pub fn speculative_execute<F>(candidates: &[CandidateAction], verify: F) -> AcceptedPrefix
where
    F: Fn(&CandidateAction) -> SandboxOutcome,
{
    let mut valid_indices = Vec::new();
    let mut final_outcome = None;
    for (i, candidate) in candidates.iter().enumerate() {
        let outcome = verify(candidate);
        if outcome.succeeded() {
            valid_indices.push(i);
            final_outcome = Some(outcome);
        } else {
            break;
        }
    }
    AcceptedPrefix {
        valid_indices,
        final_outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_hooks_shared::action_signature::ContextQualifier;

    fn sig(intent: &str) -> ActionSignature {
        ActionSignature {
            tool_class: "bash".to_owned(),
            intent_class: intent.to_owned(),
            context_qualifier: ContextQualifier::Plain,
        }
    }

    fn cand(id: &str, intent: &str) -> CandidateAction {
        CandidateAction::new(id, format!("echo {id}"), sig(intent))
    }

    fn outcome(exit_code: i32) -> SandboxOutcome {
        SandboxOutcome {
            exit_code,
            output_bytes: 4,
            was_truncated: false,
            timed_out: false,
            content_hash: "blake3-stub".to_owned(),
            capability_profile: "Sandboxed".to_owned(),
            summary: crate::gateway::OutputSummary::empty(exit_code),
        }
    }

    #[test]
    fn accepts_longest_valid_prefix_and_rejects_beyond() {
        // [ok, ok, fail] → prefix [0,1]; index 2 dry-run but not accepted.
        let candidates = vec![cand("a", "cargo"), cand("b", "cargo"), cand("c", "cargo")];
        let result = speculative_execute(&candidates, |c| outcome(if c.id == "c" { 1 } else { 0 }));
        assert_eq!(result.valid_indices, vec![0, 1]);
        assert_eq!(result.len(), 2);
        assert!(result.final_outcome.is_some());
        assert_eq!(result.final_outcome.unwrap().exit_code, 0);
    }

    #[test]
    fn speculative_loop_lossless_no_reorder_past_failure() {
        // [ok, fail, ok] → prefix [0] ONLY. The trailing ok is NOT promoted past
        // the failure — truncation, never reordering. This is the lossless contract.
        let candidates = vec![cand("a", "cargo"), cand("b", "cargo"), cand("c", "cargo")];
        let result = speculative_execute(&candidates, |c| outcome(if c.id == "b" { 1 } else { 0 }));
        assert_eq!(
            result.valid_indices,
            vec![0],
            "must truncate at first failure, not reorder"
        );
    }

    #[test]
    fn empty_prefix_when_first_candidate_fails() {
        let candidates = vec![cand("a", "cargo")];
        let result = speculative_execute(&candidates, |_| outcome(1));
        assert!(result.is_empty());
        assert!(result.final_outcome.is_none());
    }

    #[test]
    fn timed_out_candidate_breaks_the_prefix() {
        let candidates = vec![cand("a", "cargo"), cand("b", "cargo")];
        let timeout = SandboxOutcome {
            exit_code: 0,
            output_bytes: 0,
            was_truncated: false,
            timed_out: true,
            content_hash: String::new(),
            capability_profile: "Sandboxed".to_owned(),
            summary: crate::gateway::OutputSummary::empty(0),
        };
        let result = speculative_execute(&candidates, |c| {
            if c.id == "a" {
                outcome(0)
            } else {
                timeout.clone()
            }
        });
        assert_eq!(
            result.valid_indices,
            vec![0],
            "a timed-out dry-run is not a clean verify"
        );
    }

    #[test]
    fn rank_by_predicted_orders_by_learned_probability() {
        use super::super::outcome_learner::{ActionFeatures, LearnedOutcomeModel, OutcomeExample};
        // ES4 P4 — rank_by_predicted now reads `global_model_snapshot()` LIVE
        // (the function no longer takes a model argument). We seed the
        // global model via `merge_into_global` so the test exercises the
        // production wiring path.
        let mut examples = Vec::new();
        for _ in 0..9 {
            examples.push(OutcomeExample::new(
                ActionFeatures::from_parts("bash", "cargo", "plain"),
                true,
            ));
        }
        examples.push(OutcomeExample::new(
            ActionFeatures::from_parts("bash", "cargo", "plain"),
            false,
        ));
        for _ in 0..9 {
            examples.push(OutcomeExample::new(
                ActionFeatures::from_parts("bash", "flaky", "plain"),
                false,
            ));
        }
        examples.push(OutcomeExample::new(
            ActionFeatures::from_parts("bash", "flaky", "plain"),
            true,
        ));
        let model = LearnedOutcomeModel::train_from_examples(examples);
        let _ = model.merge_into_global();
        let prior = ExecutionOutcomePredictor::new();

        let ranked = rank_by_predicted(
            vec![cand("flaky-one", "flaky"), cand("cargo-one", "cargo")],
            &prior,
        );
        assert_eq!(
            ranked[0].id, "cargo-one",
            "the higher-success class must draft first"
        );
        assert!(ranked[0].predicted_success > ranked[1].predicted_success);
    }

    // ── ES1 P3.5 (2026-06-02) — X3.5 PROVE pre-filter tests ─────────────

    /// Build a `ProofReport` with a chosen status. The rest of the fields
    /// are zero-values; the filter only inspects `status`, so the
    /// other fields are noise for these tests.
    fn report_with_status(
        status: crate::gateway::offensive_integration::ProofStatus,
    ) -> crate::gateway::offensive_integration::ProofReport {
        crate::gateway::offensive_integration::ProofReport {
            status,
            counterexample: None,
            model: None,
            backend_used: crate::gateway::offensive_integration::SolverBackendKind::Stub,
            latency_ms: 0,
            claim_text: String::new(),
            smtlib: String::new(),
            timestamp_unix_ms: 0,
        }
    }

    /// OPT-IN: when `claim` is None, the filter returns the input
    /// slice unchanged (zero overhead, identity transform).
    #[test]
    fn filter_by_proof_opt_in_none_returns_input_unchanged() {
        let candidates = vec![cand("a", "x"), cand("b", "y"), cand("c", "z")];
        let out = filter_by_proof(
            &candidates,
            None,
            &crate::gateway::offensive_integration::ClaimContext::default(),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            |_, _, _| report_with_status(crate::gateway::offensive_integration::ProofStatus::Unsat),
        );
        // Identity: same order, same content.
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].id, "a");
        assert_eq!(out[1].id, "b");
        assert_eq!(out[2].id, "c");
    }

    /// STUB contract: when the prover returns `Void` (the P1 honest
    /// no-op), every candidate passes — the filter must NOT
    /// incorrectly drop candidates in the default callers' path.
    #[test]
    fn filter_by_proof_stub_void_keeps_all_candidates() {
        let candidates = vec![cand("a", "x"), cand("b", "y"), cand("c", "z")];
        let claim = crate::gateway::offensive_integration::ClaimKind::Postcondition {
            predicate: "x > 0".to_owned(),
        };
        let out = filter_by_proof(
            &candidates,
            Some(&claim),
            &crate::gateway::offensive_integration::ClaimContext::default(),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            |_, _, _| report_with_status(crate::gateway::offensive_integration::ProofStatus::Void),
        );
        assert_eq!(out.len(), 3, "Void must be neutral; all candidates pass");
    }

    /// UNSAT contract: the prover says "claim is provably false" → drop
    /// every candidate (the claim is a hard veto on the slice).
    #[test]
    fn filter_by_proof_unsat_drops_all_candidates() {
        let candidates = vec![cand("a", "x"), cand("b", "y")];
        let claim = crate::gateway::offensive_integration::ClaimKind::Postcondition {
            predicate: "contradiction".to_owned(),
        };
        let out = filter_by_proof(
            &candidates,
            Some(&claim),
            &crate::gateway::offensive_integration::ClaimContext::default(),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            |_, _, _| report_with_status(crate::gateway::offensive_integration::ProofStatus::Unsat),
        );
        assert!(out.is_empty(), "Unsat must reject every candidate (veto)");
    }

    /// ERROR contract: encoder or solver failure → fail-closed (drop
    /// every candidate). The gateway must not promote candidates it
    /// could not reason about.
    #[test]
    fn filter_by_proof_error_fails_closed() {
        let candidates = vec![cand("a", "x"), cand("b", "y")];
        let claim = crate::gateway::offensive_integration::ClaimKind::Postcondition {
            predicate: "broken".to_owned(),
        };
        let out = filter_by_proof(
            &candidates,
            Some(&claim),
            &crate::gateway::offensive_integration::ClaimContext::default(),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            |_, _, _| report_with_status(crate::gateway::offensive_integration::ProofStatus::Error),
        );
        assert!(out.is_empty(), "Error must fail-closed (drop everything)");
    }

    /// UNKNOWN contract: solver could not decide → neutral (keep all).
    /// This matches the Stub contract: conservative callers do not
    /// silently strip candidates when the prover is inconclusive.
    #[test]
    fn filter_by_proof_unknown_keeps_all_candidates() {
        let candidates = vec![cand("a", "x"), cand("b", "y"), cand("c", "z")];
        let claim = crate::gateway::offensive_integration::ClaimKind::Postcondition {
            predicate: "indecidable".to_owned(),
        };
        let out = filter_by_proof(
            &candidates,
            Some(&claim),
            &crate::gateway::offensive_integration::ClaimContext::default(),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            |_, _, _| {
                report_with_status(crate::gateway::offensive_integration::ProofStatus::Unknown)
            },
        );
        assert_eq!(out.len(), 3, "Unknown is neutral; keep every candidate");
    }

    // ── ES1 P4 (2026-06-02) — per-candidate X3.5 PROVE pre-filter tests ──

    /// Under-declaration identity: when `claim_for` returns `None` for every
    /// candidate, the filter must be a pure identity transform — and the
    /// `prove_closure` is never invoked (zero overhead for callers that
    /// cannot derive a claim from the action signature).
    #[test]
    fn filter_by_proof_per_candidate_under_declaration_defaults_to_identity() {
        let candidates = vec![
            cand("a", "unknown"),
            cand("b", "md"),
            cand("c", "mcp-touring-memory-store"),
        ];
        let result = filter_by_proof_per_candidate(
            &candidates,
            |_sig| None, // claim_for returns None → identity, no prove call
            &crate::gateway::offensive_integration::ClaimContext::default(),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            |_, _, _| panic!("prove_closure must NOT be called when claim_for returns None"),
        );
        assert_eq!(
            result.len(),
            3,
            "all 3 candidates must pass through with identity"
        );
        assert_eq!(
            result.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    /// Per-candidate derivation works: `claim_for` returns `Some` for `cargo` and
    /// `rs`, `None` for `md`; the mock `prove_closure` returns `Sat` for any
    /// claim. All three candidates pass: the two with derivable claims
    /// satisfy the filter, the one with `None` is identity.
    #[test]
    fn filter_by_proof_per_candidate_per_candidate_derivation_works() {
        let candidates = vec![cand("a", "cargo"), cand("b", "md"), cand("c", "rs")];
        let result = filter_by_proof_per_candidate(
            &candidates,
            |sig| match sig.intent_class.as_str() {
                "cargo" => Some(
                    crate::gateway::offensive_integration::ClaimKind::Postcondition {
                        predicate: "exit == 0".to_owned(),
                    },
                ),
                "rs" => Some(
                    crate::gateway::offensive_integration::ClaimKind::Postcondition {
                        predicate: "rustc succeeds".to_owned(),
                    },
                ),
                _ => None,
            },
            &crate::gateway::offensive_integration::ClaimContext::default(),
            crate::gateway::offensive_integration::SolverBackendKind::Stub,
            |_, _, _| report_with_status(crate::gateway::offensive_integration::ProofStatus::Sat),
        );
        assert_eq!(
            result.len(),
            3,
            "all 3 candidates should pass (2 with claims derive Sat, 1 with None is identity)"
        );
        assert_eq!(
            result.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }
}
