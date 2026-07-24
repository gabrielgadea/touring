//! Stage **X4 PREDICT** of the Code Execution Gateway. Phase **P3.4** of CEG
//! Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X4 is the gateway's speculative check: *before* the real sandbox runs the
//! code (X5), X4 predicts the probability that the run will succeed, from the
//! reinforcement-learning outcome history of similar past executions.
//!
//! The execution is reduced to an [`ActionSignature`] — `(tool_class,
//! intent_class, context_qualifier)` — whose
//! [`to_key`](ActionSignature::to_key) is the storage key under which past
//! `success`/`failure` outcomes accumulate. [`ExecutionOutcomePredictor`]
//! turns the observed counts into a Laplace-smoothed success probability.
//!
//! # Plan correction (VGP — FIX-S4)
//!
//! P3.4's plan lists "Reused: `speculate/bridge.rs`". That module lives in
//! `touring-generator`, which depends on `touring-hooks` — reusing it here
//! would form a crate cycle. X4 therefore implements the *speculative
//! prediction* intent natively: the prediction itself is the speculative
//! check that runs ahead of the real sandbox.
//!
//! The outcome history is supplied as a closure, so the transition stays pure
//! and testable; the production wiring (P3.7) backs it with the touring memory
//! `outcome:*` ledger.

use super::typestate::{Execution, Predicted, Proven, RawInvocation};
use serde::{Deserialize, Serialize};
use touring_hooks_shared::action_signature::ActionSignature;

/// Observed success / failure counts for an action signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutcomeStats {
    /// Past executions of this signature that succeeded.
    pub successes: u32,
    /// Past executions of this signature that failed.
    pub failures: u32,
}

impl OutcomeStats {
    /// Total observations — `successes + failures`.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.successes + self.failures
    }
}

/// How much evidence backs an X4 prediction — derived from the sample size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PredictionConfidence {
    /// No prior observations — the prediction is the bare prior.
    None,
    /// `1..=4` observations.
    Low,
    /// `5..=19` observations.
    Medium,
    /// `20+` observations.
    High,
}

impl PredictionConfidence {
    /// Classify confidence from the number of observations.
    #[must_use]
    pub fn from_total(total: u32) -> Self {
        match total {
            0 => Self::None,
            1..=4 => Self::Low,
            5..=19 => Self::Medium,
            _ => Self::High,
        }
    }
}

/// The RL outcome predictor — a Laplace-smoothed Beta estimator.
///
/// The success probability of a signature with `s` successes and `f` failures
/// is `(s + α) / (s + f + 2α)`, where `α` is the [`prior`](Self::with_prior)
/// pseudo-count. With the default `α = 1` an unseen signature predicts `0.5`.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionOutcomePredictor {
    prior: f64,
}

impl Default for ExecutionOutcomePredictor {
    fn default() -> Self {
        Self { prior: 1.0 }
    }
}

/// ES4 P3 — a prediction with confidence bucket + per-prediction Brier
/// contribution. Used by callers (S-12 speculative driver, hooks) that
/// want a single object representing the full prediction state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibratedPrediction {
    /// Laplace-smoothed predicted success probability in `[0, 1]`.
    pub prob: f64,
    /// Number of past successful executions of this action signature.
    pub success_count: u32,
    /// Number of past failed executions of this action signature.
    pub failure_count: u32,
    /// Evidence-strength bucket derived from the total observation count.
    pub confidence: PredictionConfidence,
    /// Per-prediction Brier contribution `(1 - prob)^2` for the global
    /// Brier-score running sum (gate-metrics `outcome_learner_brier_*`).
    pub brier_contribution: f64,
}

impl ExecutionOutcomePredictor {
    /// A predictor with the default uniform prior (`α = 1`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A predictor with a custom Laplace prior. A negative prior is clamped
    /// to `0`.
    #[must_use]
    pub fn with_prior(prior: f64) -> Self {
        Self {
            prior: prior.max(0.0),
        }
    }

    /// The predicted success probability for the given observations, in
    /// `[0.0, 1.0]`.
    ///
    /// With no observations and a zero prior the estimator has no information
    /// to work with and returns the neutral `0.5` rather than a `NaN`.
    #[must_use]
    pub fn success_probability(&self, stats: &OutcomeStats) -> f64 {
        let s = f64::from(stats.successes);
        let f = f64::from(stats.failures);
        let denom = s + f + 2.0 * self.prior;
        if denom <= 0.0 {
            return 0.5;
        }
        (s + self.prior) / denom
    }

    /// ES4 P3 + P5 — calibrated prediction with confidence bucket + Brier
    /// contribution. The first production caller was added in P5
    /// (`cli_predict_action` and the speculative driver); the method was the
    /// missing half of the original P3 substrate (the struct existed but the
    /// constructor did not — honest correction to the P3 checkpoint, which
    /// overstated the work).
    ///
    /// Records three X4-observable metrics:
    /// - `outcome_learner_predict_count` (every call)
    /// - `outcome_learner_brier_running_sum` (bit-cast f64, per-prediction `(1-prob)^2`)
    /// - `outcome_learner_cold_start_count` (when `total < 10`)
    #[must_use]
    pub fn prediction_calibrated(&self, stats: &OutcomeStats) -> CalibratedPrediction {
        let prob = self.success_probability(stats);
        let total = stats.total();
        let confidence = PredictionConfidence::from_total(total);
        let brier_contribution = (1.0 - prob).powi(2);
        touring_hooks_shared::gate_metrics::record_outcome_learner_predict();
        touring_hooks_shared::gate_metrics::record_outcome_learner_brier(brier_contribution);
        if total < 10 {
            touring_hooks_shared::gate_metrics::record_outcome_learner_cold_start();
        }
        CalibratedPrediction {
            prob,
            success_count: stats.successes,
            failure_count: stats.failures,
            confidence,
            brier_contribution,
        }
    }
}

/// The X4 PREDICT result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PredictionReport {
    /// The `outcome:*` storage key the prediction was drawn from.
    pub signature: String,
    /// Predicted probability of a successful run, in `[0.0, 1.0]`.
    pub success_probability: f64,
    /// The observed outcome counts behind the prediction.
    pub observed: OutcomeStats,
    /// How much evidence backs the prediction.
    pub confidence: PredictionConfidence,
}

/// Reduce a captured invocation to its [`ActionSignature`].
///
/// The signature is the key under which this execution's eventual outcome is
/// recorded, so X4's prediction and the X9 LEARN outcome-store agree on the
/// same key.
#[must_use]
pub fn signature_for(raw: &RawInvocation) -> ActionSignature {
    let input = serde_json::json!({ "command": raw.payload });
    ActionSignature::from_pre_tool(&raw.tool, &input, None, 0, None, None)
}

impl Execution<Proven> {
    /// **X4 PREDICT** — predict the run's success probability from RL outcome
    /// history, attach the [`PredictionReport`] to the evidence ledger, and
    /// advance to [`Predicted`].
    ///
    /// ES1 P3 (2026-06-01): `predict` was moved from `Execution<Verified>`
    /// to `Execution<Proven>`. The X3.5 PROVE stage now sits between X3 VGP
    /// and X4 PREDICT, so the compiler enforces that every execution
    /// passes through `prove_claim` before prediction. Callers that
    /// previously did `.vgp_verify(...).predict(...)` must insert
    /// `.prove_claim(...)` (with `None` claim to keep zero overhead).
    ///
    /// `outcome_history` maps a signature key to its observed [`OutcomeStats`];
    /// the production wiring (P3.7) backs it with the touring memory
    /// `outcome:*` ledger, while tests pass a mock.
    pub fn predict<F>(
        mut self,
        predictor: &ExecutionOutcomePredictor,
        outcome_history: F,
    ) -> Execution<Predicted>
    where
        F: Fn(&str) -> OutcomeStats,
    {
        let key = signature_for(self.raw()).to_key();
        let observed = outcome_history(&key);
        let report = PredictionReport {
            success_probability: predictor.success_probability(&observed),
            confidence: PredictionConfidence::from_total(observed.total()),
            signature: key,
            observed,
        };
        self.evidence_mut().prediction = Some(report);
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::capture_tool_call;

    fn proven() -> Execution<Proven> {
        // ES1 P3 (2026-06-01): renamed from `verified()`; the X3.5 PROVE
        // stage now sits between X3 VGP and X4 PREDICT, so the helper must
        // thread `.prove_claim(...)` through before reaching `Proven`.
        capture_tool_call("Bash", "cargo build", None)
            .expect("Bash is code-bearing")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
    }

    #[test]
    fn outcome_stats_total_sums_counts() {
        assert_eq!(
            OutcomeStats {
                successes: 7,
                failures: 3
            }
            .total(),
            10
        );
        assert_eq!(OutcomeStats::default().total(), 0);
    }

    #[test]
    fn predictor_unseen_signature_is_neutral() {
        let p = ExecutionOutcomePredictor::default();
        assert!((p.success_probability(&OutcomeStats::default()) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn predictor_all_success_approaches_one() {
        let prob = ExecutionOutcomePredictor::new().success_probability(&OutcomeStats {
            successes: 100,
            failures: 0,
        });
        assert!(prob > 0.95, "got {prob}");
        assert!(prob < 1.0);
    }

    #[test]
    fn predictor_all_failure_approaches_zero() {
        let prob = ExecutionOutcomePredictor::new().success_probability(&OutcomeStats {
            successes: 0,
            failures: 100,
        });
        assert!(prob < 0.05, "got {prob}");
        assert!(prob > 0.0);
    }

    #[test]
    fn predictor_balanced_history_is_near_half() {
        let prob = ExecutionOutcomePredictor::new().success_probability(&OutcomeStats {
            successes: 20,
            failures: 20,
        });
        assert!((prob - 0.5).abs() < 1e-9);
    }

    #[test]
    fn predictor_zero_prior_no_history_returns_neutral() {
        // α = 0 with no observations would be 0/0; the estimator guards it.
        let prob = ExecutionOutcomePredictor::with_prior(0.0)
            .success_probability(&OutcomeStats::default());
        assert!((prob - 0.5).abs() < 1e-9);
    }

    #[test]
    fn prediction_confidence_tiers() {
        assert_eq!(
            PredictionConfidence::from_total(0),
            PredictionConfidence::None
        );
        assert_eq!(
            PredictionConfidence::from_total(3),
            PredictionConfidence::Low
        );
        assert_eq!(
            PredictionConfidence::from_total(10),
            PredictionConfidence::Medium
        );
        assert_eq!(
            PredictionConfidence::from_total(50),
            PredictionConfidence::High
        );
    }

    #[test]
    fn signature_for_is_deterministic_and_outcome_keyed() {
        let raw = RawInvocation::new("Bash", "cargo test");
        assert_eq!(signature_for(&raw).to_key(), signature_for(&raw).to_key());
        assert!(signature_for(&raw).to_key().starts_with("outcome:"));
    }

    #[test]
    fn predict_transition_attaches_report_and_advances() {
        let predicted = proven().predict(&ExecutionOutcomePredictor::default(), |_| OutcomeStats {
            successes: 8,
            failures: 2,
        });
        // ES1 P3 (2026-06-01): X3.5 PROVE inserted between X3 and X4.
        // X4 PREDICT ordinal is now 5 (was 4).
        assert_eq!(predicted.ordinal(), 5);
        assert_eq!(predicted.stage(), "X4-PREDICT");
        let report = predicted
            .evidence()
            .prediction
            .as_ref()
            .expect("predict must attach a PredictionReport");
        // (8 + 1) / (10 + 2) = 0.75
        assert!(
            (report.success_probability - 0.75).abs() < 1e-9,
            "got {}",
            report.success_probability
        );
        assert_eq!(report.confidence, PredictionConfidence::Medium);
    }

    #[test]
    fn predict_with_no_history_is_neutral_and_unconfident() {
        let predicted = proven().predict(&ExecutionOutcomePredictor::default(), |_| {
            OutcomeStats::default()
        });
        let report = predicted
            .evidence()
            .prediction
            .as_ref()
            .expect("prediction");
        assert!((report.success_probability - 0.5).abs() < 1e-9);
        assert_eq!(report.confidence, PredictionConfidence::None);
    }

    #[test]
    fn prediction_report_serde_roundtrip() {
        let predicted = proven().predict(&ExecutionOutcomePredictor::default(), |_| OutcomeStats {
            successes: 3,
            failures: 1,
        });
        let report = predicted
            .evidence()
            .prediction
            .as_ref()
            .expect("prediction");
        let json = serde_json::to_string(report).expect("serialize");
        let back: PredictionReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, &back);
    }

    #[test]
    fn e2e_full_chain_capture_to_predicted() {
        // X0 → X1 → X2 → X3 → X4 end to end.
        let predicted = capture_tool_call("Bash", "cargo test --release", None)
            .expect("admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true)
            .prove_claim(
                None,
                crate::gateway::offensive_integration::SolverBackendKind::Stub,
                &crate::gateway::offensive_integration::ClaimContext::default(),
            )
            .predict(&ExecutionOutcomePredictor::new(), |_| OutcomeStats {
                successes: 50,
                failures: 0,
            });
        // ES1 P3 (2026-06-01): X4 PREDICT ordinal is now 5 (was 4); stage_log has 6 records.
        assert_eq!(predicted.ordinal(), 5);
        assert_eq!(predicted.evidence().stage_log.len(), 6);
        let report = predicted
            .evidence()
            .prediction
            .as_ref()
            .expect("prediction");
        assert!(report.success_probability > 0.95);
        assert_eq!(report.confidence, PredictionConfidence::High);
    }

    // ─── ES4 P5 — 4 unit tests for prediction_calibrated ───
    // The 4 buckets come from `PredictionConfidence::from_total`:
    //   None=0, Low=1-4, Medium=5-19, High=20+
    // The cold-start rule is `total < 10` (catches None + Low + Medium-with-0-9).

    #[test]
    fn prediction_calibrated_none_bucket_zero_observations() {
        let p = ExecutionOutcomePredictor::new();
        let stats = OutcomeStats {
            successes: 0,
            failures: 0,
        };
        let c = p.prediction_calibrated(&stats);
        // Bare prior (0+1)/(0+0+2) = 0.5
        assert!((c.prob - 0.5).abs() < 1e-9);
        assert_eq!(c.confidence, PredictionConfidence::None);
        assert_eq!(c.success_count, 0);
        assert_eq!(c.failure_count, 0);
        assert!((c.brier_contribution - 0.25).abs() < 1e-9);
    }

    #[test]
    fn prediction_calibrated_low_bucket_three_observations() {
        let p = ExecutionOutcomePredictor::new();
        let stats = OutcomeStats {
            successes: 2,
            failures: 1,
        };
        let c = p.prediction_calibrated(&stats);
        // (2+1)/(3+2) = 0.6
        assert!((c.prob - 0.6).abs() < 1e-9);
        assert_eq!(c.confidence, PredictionConfidence::Low);
        assert_eq!(c.success_count, 2);
        assert_eq!(c.failure_count, 1);
        // (1 - 0.6)^2 = 0.16
        assert!((c.brier_contribution - 0.16).abs() < 1e-9);
    }

    #[test]
    fn prediction_calibrated_medium_bucket_ten_observations() {
        let p = ExecutionOutcomePredictor::new();
        let stats = OutcomeStats {
            successes: 8,
            failures: 2,
        };
        let c = p.prediction_calibrated(&stats);
        // (8+1)/(10+2) = 0.75
        assert!((c.prob - 0.75).abs() < 1e-9);
        assert_eq!(c.confidence, PredictionConfidence::Medium);
        // (1 - 0.75)^2 = 0.0625
        assert!((c.brier_contribution - 0.0625).abs() < 1e-9);
    }

    #[test]
    fn prediction_calibrated_high_bucket_fifty_observations() {
        let p = ExecutionOutcomePredictor::new();
        let stats = OutcomeStats {
            successes: 50,
            failures: 0,
        };
        let c = p.prediction_calibrated(&stats);
        // (50+1)/(50+2) = 51/52 ≈ 0.9808
        assert!((c.prob - 51.0 / 52.0).abs() < 1e-9);
        assert_eq!(c.confidence, PredictionConfidence::High);
        assert_eq!(c.success_count, 50);
        assert_eq!(c.failure_count, 0);
        // (1 - 51/52)^2 = (1/52)^2 ≈ 0.000370
        assert!(c.brier_contribution < 0.001);
    }
}
