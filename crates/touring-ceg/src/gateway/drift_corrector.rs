//! **S-14 / R13 — the system-wide drift-correction loop.**
//!
//! `health_delta::compute_signals_delta` runs *per action*, but nothing
//! re-anchors the whole-system state against deterministic sensors across the
//! trajectory — so slow degradation (the post-norm divergence the paper's §B-4
//! warns about) accumulates unseen. R13 adds that re-grounding: after each
//! accepted action, capture a [`SensorReading`] from the deterministic sensors —
//! the [`HarnessQuality`] composite (S-06), the [`EvidenceBundle`] composite
//! (S-05), and the latest `health_delta` — and [`reconcile`] it against the
//! pre-action reading. A drop beyond the threshold on any axis flags drift and
//! names the diverged sensor(s).
//!
//! # No false positive on the first action
//!
//! The first action has no prior reading, so reconciliation against `None`
//! returns `diverged = false` (it is the baseline). Drift can only be *relative*
//! to an established anchor.

use crate::gateway::decision::EvidenceBundle;
use crate::gateway::harness_contract::HarnessContract;
use crate::gateway::harness_metric::HarnessQuality;

/// A deterministic-sensor reading at one point in the trajectory. Every field is
/// sourced from a replayable sensor — never an LLM estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensorReading {
    /// The [`HarnessQuality`] composite KPI (S-06).
    pub harness_composite: f64,
    /// The X7 [`EvidenceBundle`] composite (S-05).
    pub evidence_composite: f64,
    /// The latest per-action `health_delta` value (negative = regression).
    pub health_delta: f64,
    /// ES2 P4 — first 8 ASCII bytes of the [`HarnessContract`] blake3 digest
    /// (B-6 sink-token self-verification). A change between `pre` and `post`
    /// means the constitution (`CLAUDE.md` + `rules/*.md`) was edited
    /// mid-session — a first-class drift axis. Zero-filled (no contract
    /// attested yet) does NOT trigger drift; it is the pre-attestation
    /// baseline state.
    pub constitutional_digest_prefix: [u8; 8],
}

impl SensorReading {
    /// Build a reading from the live signals, without a constitutional contract.
    /// The `constitutional_digest_prefix` defaults to `[0; 8]` (pre-attestation
    /// baseline — never itself flagged as drift).
    #[must_use]
    pub fn from_signals(
        harness: &HarnessQuality,
        evidence: &EvidenceBundle,
        health_delta: f64,
    ) -> Self {
        Self {
            harness_composite: harness.composite,
            evidence_composite: evidence.composite(),
            health_delta,
            constitutional_digest_prefix: [0u8; 8],
        }
    }

    /// ES2 P4 — build a reading that also captures the constitutional contract
    /// digest prefix. When `contract` is `None`, the prefix is zero-filled (the
    /// pre-attestation baseline).
    #[must_use]
    pub fn from_signals_with_contract(
        harness: &HarnessQuality,
        evidence: &EvidenceBundle,
        health_delta: f64,
        contract: Option<&HarnessContract>,
    ) -> Self {
        let mut prefix = [0u8; 8];
        if let Some(c) = contract {
            let bytes = c.digest.as_bytes();
            let n = bytes.len().min(8);
            prefix[..n].copy_from_slice(&bytes[..n]);
        }
        Self {
            harness_composite: harness.composite,
            evidence_composite: evidence.composite(),
            health_delta,
            constitutional_digest_prefix: prefix,
        }
    }
}

/// The result of re-grounding a post-action reading against the pre-action anchor.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftReconciliation {
    /// `true` when any sensor regressed beyond the threshold.
    pub diverged: bool,
    /// The harness-composite delta (`post - pre`); negative is a regression.
    pub composite_delta: f64,
    /// Names of the sensor axes that diverged (empty when `diverged` is false).
    pub diverged_axes: Vec<String>,
}

/// Re-ground the post-action state against the pre-action sensors.
///
/// Returns the baseline (`diverged = false`) when `pre` is `None` (the first
/// action). Otherwise flags any axis whose post reading dropped by more than
/// `threshold` below its pre reading; `health_delta` is flagged on its own
/// absolute sign (a negative post delta is itself a regression signal).
///
/// ES2 P4 — also flags the `constitutional_digest` axis when the pre and
/// post [`HarnessContract`] digest prefixes differ (constitution was edited
/// mid-session). The pre-attestation baseline (`[0; 8]`) is excluded so the
/// very first contract attestation does not itself trip drift.
#[must_use]
pub fn reconcile(
    pre: Option<SensorReading>,
    post: SensorReading,
    threshold: f64,
) -> DriftReconciliation {
    let Some(pre) = pre else {
        return DriftReconciliation {
            diverged: false,
            composite_delta: 0.0,
            diverged_axes: Vec::new(),
        };
    };
    let mut diverged_axes = Vec::new();
    let composite_delta = post.harness_composite - pre.harness_composite;
    if composite_delta < -threshold {
        diverged_axes.push("harness_composite".to_owned());
    }
    if post.evidence_composite - pre.evidence_composite < -threshold {
        diverged_axes.push("evidence_composite".to_owned());
    }
    if post.health_delta < -threshold {
        diverged_axes.push("health_delta".to_owned());
    }
    // ES2 P4: constitutional_digest axis. Skip when pre is the pre-attestation
    // baseline (all zeros) — otherwise the very first contract attestation
    // would always trip drift. Only flag real changes.
    if pre.constitutional_digest_prefix != [0u8; 8]
        && post.constitutional_digest_prefix != pre.constitutional_digest_prefix
    {
        diverged_axes.push("constitutional_digest".to_owned());
    }
    DriftReconciliation {
        diverged: !diverged_axes.is_empty(),
        composite_delta,
        diverged_axes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hq(composite: f64) -> HarnessQuality {
        HarnessQuality {
            executable: composite,
            inspectable: composite,
            stateful: composite,
            governed: composite,
            performant: composite,
            evolving: composite,
            composite,
        }
    }

    fn ev(score: f64) -> EvidenceBundle {
        EvidenceBundle {
            static_score: score,
            vgp_score: score,
            predict_score: score,
            sandbox_score: score,
            gate_score: score,
        }
    }

    #[test]
    fn first_action_is_baseline_not_drift() {
        let post = SensorReading::from_signals(&hq(0.8), &ev(1.0), 0.0);
        let result = reconcile(None, post, 0.05);
        assert!(
            !result.diverged,
            "first action (pre=None) must never be flagged"
        );
        assert!(result.diverged_axes.is_empty());
    }

    #[test]
    fn composite_regression_beyond_threshold_flags_drift() {
        let pre = SensorReading::from_signals(&hq(0.8), &ev(1.0), 0.0);
        let post = SensorReading::from_signals(&hq(0.6), &ev(1.0), 0.0);
        let result = reconcile(Some(pre), post, 0.05);
        assert!(result.diverged);
        assert!(
            result
                .diverged_axes
                .contains(&"harness_composite".to_owned())
        );
        assert!(result.composite_delta < 0.0);
    }

    #[test]
    fn improvement_is_not_drift() {
        let pre = SensorReading::from_signals(&hq(0.6), &ev(0.8), 0.0);
        let post = SensorReading::from_signals(&hq(0.7), &ev(0.9), 0.1);
        let result = reconcile(Some(pre), post, 0.05);
        assert!(
            !result.diverged,
            "an across-the-board improvement is not drift"
        );
    }

    #[test]
    fn negative_health_delta_flags_its_axis() {
        let pre = SensorReading::from_signals(&hq(0.8), &ev(1.0), 0.0);
        let post = SensorReading::from_signals(&hq(0.8), &ev(1.0), -0.2);
        let result = reconcile(Some(pre), post, 0.05);
        assert!(result.diverged);
        assert_eq!(result.diverged_axes, vec!["health_delta".to_owned()]);
    }

    #[test]
    fn sub_threshold_noise_is_not_drift() {
        let pre = SensorReading::from_signals(&hq(0.80), &ev(1.0), 0.0);
        let post = SensorReading::from_signals(&hq(0.78), &ev(1.0), -0.01);
        let result = reconcile(Some(pre), post, 0.05);
        assert!(
            !result.diverged,
            "sub-threshold jitter must not trip the loop"
        );
    }

    // ── ES2 P4 — constitutional_digest axis ──────────────────────────────────

    /// Helper: a reading with a specific constitutional_digest_prefix.
    fn hq_with_prefix(score: f64, prefix: [u8; 8]) -> SensorReading {
        let mut r = SensorReading::from_signals(&hq(score), &ev(1.0), 0.0);
        r.constitutional_digest_prefix = prefix;
        r
    }

    #[test]
    fn constitutional_digest_change_flags_drift() {
        // Pre was attested (nonzero prefix), post has a different prefix.
        let pre = hq_with_prefix(0.8, *b"abcd1234");
        let post = hq_with_prefix(0.8, *b"wxyz5678");
        let result = reconcile(Some(pre), post, 0.05);
        assert!(
            result.diverged,
            "mid-session constitution edit must trip drift"
        );
        assert!(
            result
                .diverged_axes
                .contains(&"constitutional_digest".to_owned()),
            "expected constitutional_digest in axes, got {:?}",
            result.diverged_axes
        );
    }

    #[test]
    fn constitutional_digest_unchanged_does_not_flag() {
        let pre = hq_with_prefix(0.8, *b"abcd1234");
        let post = hq_with_prefix(0.8, *b"abcd1234");
        let result = reconcile(Some(pre), post, 0.05);
        assert!(
            !result
                .diverged_axes
                .contains(&"constitutional_digest".to_owned()),
            "unchanged digest must not fire, got {:?}",
            result.diverged_axes
        );
    }

    #[test]
    fn pre_attestation_baseline_skipped_no_false_positive() {
        // Pre is the pre-attestation baseline ([0; 8]); post is a real attestation.
        // The very first real attestation must NOT trip drift (it would always
        // diverge if the comparison were unconditional).
        let pre = SensorReading::from_signals(&hq(0.8), &ev(1.0), 0.0);
        let post = hq_with_prefix(0.8, *b"abcd1234");
        let result = reconcile(Some(pre), post, 0.05);
        assert!(
            !result
                .diverged_axes
                .contains(&"constitutional_digest".to_owned()),
            "first real attestation after baseline must not trip, got {:?}",
            result.diverged_axes
        );
    }

    #[test]
    fn constitutional_digest_drift_alone_is_enough() {
        // No other axis diverges, but the digest changes.
        let pre = hq_with_prefix(0.8, *b"abcd1234");
        let post = hq_with_prefix(0.8, *b"wxyz5678");
        let result = reconcile(Some(pre), post, 0.05);
        assert_eq!(
            result.diverged_axes,
            vec!["constitutional_digest".to_owned()],
            "digest change alone must yield exactly the constitutional_digest axis"
        );
    }

    #[test]
    fn first_action_with_real_attestation_does_not_trip_baseline() {
        // First action ever (pre = None) and the post already has a real
        // attestation. The baseline-wins rule applies: no drift on the first
        // action regardless of what the post sensor shows.
        let post = hq_with_prefix(0.8, *b"abcd1234");
        let result = reconcile(None, post, 0.05);
        assert!(!result.diverged, "first action is always the baseline");
        assert!(result.diverged_axes.is_empty());
    }
}
