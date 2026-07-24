//! **R5 / OP1 — the unified harness-quality metric.**
//!
//! Touring's observability surface is a scatter of counters
//! ([`GateMetricsSnapshot`]): CEG capture/sandbox/block tallies, enrichment
//! emission, health-delta streaks, LinUCB routing, MCTS shadow rollouts, cache
//! ratios. Useful individually, but there was **no single inspectable measure
//! of how elite the code-agent harness actually is** — the gap the conformance
//! diagnostic flagged as OP1 (🔴).
//!
//! [`HarnessQuality`] closes it. It collapses the scattered counters — plus the
//! per-axis [`EvidenceBundle`] from X7 DECISION (S-05) — into **six dimensions
//! in `0.0..=1.0`**, each anchored on a concrete, documented counter, and a
//! `composite` that is their arithmetic mean. The six dimensions are the
//! north-star harness properties from *Code as Agent Harness* (arXiv 2605.18747)
//! plus the two operational axes Touring optimizes for:
//!
//! | Dimension | What it measures | Anchor counter(s) |
//! |---|---|---|
//! | `executable` | the harness actually runs actions through the CEG | `ceg_captured_count` |
//! | `inspectable` | every action's *why* is observable (evidence + enrichment) | `ceg_captured_count`, `enrichment_emit_count`, [`EvidenceBundle::composite`] |
//! | `stateful` | per-action state is tracked across the trajectory | `health_delta_record_count` |
//! | `governed` | every execution passes a policy gate and gets a verdict | `ceg_{sandboxed,blocked,fast_path}_count` |
//! | `performant` | the hot paths take their fast routes | `query_cache_hit_ratio`, `pre_edit_fast_ratio`, `pre_write_fast_ratio` |
//! | `evolving` | the system learns and adapts (RL + planning + recovery) | `health_delta_improvement_count`, `linucb_route_*`, `mcts_shadow_run_count` |
//!
//! # Liveness, not enforcement frequency
//!
//! Each dimension scores the **liveness** of its subsystem — is the machinery
//! active and healthy — rather than how often a gate fired. A harness that
//! correctly allows safe commands is well-governed even with zero blocks, so
//! `governed` counts every verdict-producing outcome (`sandboxed + blocked +
//! fast_path`), not just denials. Activity dimensions use a smooth saturating
//! map `saturate` (`x / (x + k)`) so the score rises with sustained activity
//! and is bounded in `0.0..=1.0`; ratio dimensions (`performant_score`) average
//! only the *live* ratios so a cold harness reads the neutral `0.5`, never a
//! false `0.0`.
//!
//! # Why this is the elite KPI
//!
//! `composite` is a single number a human (or the evolution agent, S-02) can
//! watch climb as the harness becomes more elite — and because the struct is the
//! extensible registry, a new dimension is one field plus one anchor, never a
//! schema migration. Exposed to operators via `touring harness-metric -j`.

use super::decision::EvidenceBundle;
use serde::{Deserialize, Serialize};
use touring_hooks_shared::gate_metrics::GateMetricsSnapshot;

/// Half-saturation constant for the `executable` dimension — the captured-exec
/// count at which the dimension reads `0.5`.
const K_EXEC: f64 = 50.0;
/// Half-saturation constant for the enrichment component of `inspectable`.
const K_INSPECT: f64 = 100.0;
/// Half-saturation constant for the `stateful` dimension.
const K_STATE: f64 = 50.0;
/// Half-saturation constant for the `governed` dimension.
const K_GOVERN: f64 = 50.0;
/// Half-saturation constant for the `evolving` dimension.
const K_EVOLVE: f64 = 30.0;

/// The neutral score for a dimension with no live signal yet.
const NEUTRAL: f64 = 0.5;

/// Smooth saturating map `x -> x / (x + k)`, bounded in `0.0..=1.0`.
///
/// `x == 0` → `0.0`; `x == k` → `0.5`; `x -> ∞` → `1.0`. Monotonic and
/// continuous, so a dimension's score rises with sustained activity and never
/// jumps. `k` is the activity level at which the dimension is "half alive".
#[must_use]
fn saturate(x: u64, k: f64) -> f64 {
    if x == 0 {
        return 0.0;
    }
    let x = x as f64;
    (x / (x + k)).clamp(0.0, 1.0)
}

/// The `performant` dimension: the mean of the **live** efficiency ratios.
///
/// A ratio counts as live only when its underlying observation count is `> 0`,
/// so a cold harness (no cache lookups, no edits yet) scores the neutral
/// [`NEUTRAL`] rather than a misleading `0.0`. Each ratio is clamped defensively
/// in case a producer emitted an out-of-range value.
#[must_use]
fn performant_score(snap: &GateMetricsSnapshot) -> f64 {
    let mut sum = 0.0;
    let mut live = 0u32;
    if snap.query_cache_hit_count + snap.query_cache_miss_count > 0 {
        sum += snap.query_cache_hit_ratio.clamp(0.0, 1.0);
        live += 1;
    }
    if snap.pre_edit_fast_path + snap.pre_edit_full > 0 {
        sum += snap.pre_edit_fast_ratio.clamp(0.0, 1.0);
        live += 1;
    }
    if snap.pre_write_fast_path + snap.pre_write_full > 0 {
        sum += snap.pre_write_fast_ratio.clamp(0.0, 1.0);
        live += 1;
    }
    if live == 0 {
        NEUTRAL
    } else {
        sum / f64::from(live)
    }
}

/// The unified six-dimension harness-quality metric (R5 / OP1).
///
/// Every field is in `0.0..=1.0`; `composite` is the arithmetic mean of the six
/// dimensions. Built from a [`GateMetricsSnapshot`] (and, optionally, the most
/// recent X7 [`EvidenceBundle`]) by [`HarnessQuality::from_snapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HarnessQuality {
    /// The harness actually executes actions through the CEG pipeline.
    pub executable: f64,
    /// Every action's verification signal is observable (evidence + enrichment).
    pub inspectable: f64,
    /// Per-action state is recorded and tracked across the trajectory.
    pub stateful: f64,
    /// Every execution passes a policy gate and yields a verdict.
    pub governed: f64,
    /// The hot paths take their fast routes (cache hits, fast-path enrichment).
    pub performant: f64,
    /// The system learns and adapts — RL routing, planning, recovery.
    pub evolving: f64,
    /// Arithmetic mean of the six dimensions — the single elite KPI.
    pub composite: f64,
}

impl HarnessQuality {
    /// Aggregate a [`GateMetricsSnapshot`] (and optionally the latest X7
    /// [`EvidenceBundle`]) into the six-dimension metric.
    ///
    /// When an `evidence` bundle is supplied, `inspectable` blends the structural
    /// per-axis inspectability (`evidence.composite()`) with the counter-derived
    /// liveness 50/50 — the bundle *being carried* is direct proof the harness
    /// surfaces its reasoning. With no bundle, `inspectable` rests on the
    /// counters alone.
    #[must_use]
    pub fn from_snapshot(snap: &GateMetricsSnapshot, evidence: Option<&EvidenceBundle>) -> Self {
        let executable = saturate(snap.ceg_captured_count, K_EXEC);

        let inspect_base = 0.5 * saturate(snap.ceg_captured_count, K_EXEC)
            + 0.5 * saturate(snap.enrichment_emit_count, K_INSPECT);
        let inspectable = match evidence {
            Some(e) => 0.5 * inspect_base + 0.5 * e.composite(),
            None => inspect_base,
        };

        let stateful = saturate(snap.health_delta_record_count, K_STATE);

        let governed = saturate(
            snap.ceg_sandboxed_count + snap.ceg_blocked_count + snap.ceg_fast_path_count,
            K_GOVERN,
        );

        let performant = performant_score(snap);

        let evolving = saturate(
            snap.health_delta_improvement_count
                + snap.linucb_route_manual_count
                + snap.linucb_route_generator_count
                + snap.linucb_route_hint_count
                + snap.mcts_shadow_run_count,
            K_EVOLVE,
        );

        let composite =
            (executable + inspectable + stateful + governed + performant + evolving) / 6.0;

        Self {
            executable,
            inspectable,
            stateful,
            governed,
            performant,
            evolving,
            composite,
        }
    }

    /// `true` when every dimension is in `0.0..=1.0` — the invariant the metric
    /// guarantees by construction. Used as a self-check at the CLI boundary.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        let dims = [
            self.executable,
            self.inspectable,
            self.stateful,
            self.governed,
            self.performant,
            self.evolving,
            self.composite,
        ];
        dims.iter().all(|d| (0.0..=1.0).contains(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A snapshot with chosen counts, built from `Default` so it never touches
    /// the process-global atomics (tests in a shared binary stay deterministic).
    fn snap() -> GateMetricsSnapshot {
        GateMetricsSnapshot::default()
    }

    #[test]
    fn saturate_is_zero_half_and_bounded() {
        assert_eq!(saturate(0, 50.0), 0.0);
        assert!((saturate(50, 50.0) - 0.5).abs() < 1e-9);
        assert!(saturate(1_000_000, 50.0) < 1.0);
        assert!(saturate(1_000_000, 50.0) > 0.99);
    }

    #[test]
    fn cold_snapshot_is_well_formed_and_neutral_performant() {
        let hq = HarnessQuality::from_snapshot(&snap(), None);
        assert!(hq.is_well_formed(), "cold metric must stay in range");
        assert_eq!(hq.executable, 0.0, "no captures → not executable yet");
        assert_eq!(hq.performant, NEUTRAL, "no ratios live → neutral, not 0.0");
    }

    #[test]
    fn all_six_dims_in_range_for_active_harness() {
        let mut s = snap();
        s.ceg_captured_count = 120;
        s.enrichment_emit_count = 300;
        s.health_delta_record_count = 80;
        s.ceg_sandboxed_count = 40;
        s.ceg_fast_path_count = 60;
        s.ceg_blocked_count = 5;
        s.query_cache_hit_count = 90;
        s.query_cache_miss_count = 10;
        s.query_cache_hit_ratio = 0.9;
        s.pre_edit_fast_path = 8;
        s.pre_edit_full = 2;
        s.pre_edit_fast_ratio = 0.8;
        s.health_delta_improvement_count = 12;
        s.mcts_shadow_run_count = 20;
        let hq = HarnessQuality::from_snapshot(&s, None);
        for d in [
            hq.executable,
            hq.inspectable,
            hq.stateful,
            hq.governed,
            hq.performant,
            hq.evolving,
            hq.composite,
        ] {
            assert!((0.0..=1.0).contains(&d), "dimension {d} out of range");
        }
        assert!(hq.is_well_formed());
    }

    #[test]
    fn composite_is_the_mean_of_the_six() {
        let mut s = snap();
        s.ceg_captured_count = 50;
        s.enrichment_emit_count = 100;
        s.health_delta_record_count = 50;
        s.ceg_sandboxed_count = 50;
        let hq = HarnessQuality::from_snapshot(&s, None);
        let mean = (hq.executable
            + hq.inspectable
            + hq.stateful
            + hq.governed
            + hq.performant
            + hq.evolving)
            / 6.0;
        assert!((hq.composite - mean).abs() < 1e-9);
    }

    #[test]
    fn evidence_bundle_lifts_inspectable() {
        let mut s = snap();
        s.ceg_captured_count = 50;
        s.enrichment_emit_count = 100;
        let without = HarnessQuality::from_snapshot(&s, None);
        // A spotless per-axis bundle (composite == 1.0) should not lower, and
        // here lifts, the inspectable dimension relative to counters alone.
        let spotless = EvidenceBundle {
            static_score: 1.0,
            vgp_score: 1.0,
            predict_score: 1.0,
            sandbox_score: 1.0,
            gate_score: 1.0,
        };
        let with = HarnessQuality::from_snapshot(&s, Some(&spotless));
        assert!(
            with.inspectable >= without.inspectable,
            "a spotless evidence bundle must not reduce inspectability"
        );
        assert!(with.is_well_formed());
    }
}
