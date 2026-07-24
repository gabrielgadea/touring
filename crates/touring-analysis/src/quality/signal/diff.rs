//! Diff two [`super::WorkspaceQualitySignal`] snapshots.
//!
//! Sentrux Master Plan Wave 2 P4 (2026-05-09). The diff is purely
//! structural — it does not read state from disk or the daemon. It
//! takes a `previous` and a `current` snapshot, computes per-root-cause
//! deltas, detects bottleneck rotation, and classifies the overall
//! trend as `Improving`, `Regressing`, or `Stable` (relative to a
//! configurable `epsilon` on the 0..=10000 scale).
//!
//! # Example
//!
//! ```ignore
//! use touring_analysis::quality::signal::{compute_quality_signal, diff::diff_signals};
//! let prev = compute_quality_signal(&old_ws);
//! let curr = compute_quality_signal(&new_ws);
//! let diff = diff_signals(&prev, &curr);
//! match diff.trend {
//!     SignalTrend::Improving => println!("good progress"),
//!     SignalTrend::Regressing => println!("regression — investigate {:?}", diff.delta_root_causes.worst()),
//!     SignalTrend::Stable => println!("no significant change"),
//! }
//! ```

use serde::{Deserialize, Serialize};

use super::types::{Bottleneck, RootCauseScores, WorkspaceQualitySignal};

/// Default epsilon for the trend classifier — 50 points on the
/// `[0, 10000]` Sentrux scale (i.e. half a percent of the maximum).
pub const DEFAULT_TREND_EPSILON: i32 = 50;

/// Coarse-grained trend label produced by [`diff_signals`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignalTrend {
    /// `current > previous` by at least `epsilon`.
    Improving,
    /// `current < previous` by at least `epsilon`.
    Regressing,
    /// `|current - previous| < epsilon`.
    Stable,
}

impl SignalTrend {
    /// Human-readable label for diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            SignalTrend::Improving => "improving",
            SignalTrend::Regressing => "regressing",
            SignalTrend::Stable => "stable",
        }
    }
}

/// Per-root-cause delta — `current - previous` for each normalized
/// score in `[-1.0, +1.0]`. Positive means improvement.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct RootCauseDeltas {
    /// Modularity score delta (positive = better community structure).
    pub modularity: f64,
    /// Acyclicity score delta (positive = fewer cycles).
    pub acyclicity: f64,
    /// Depth score delta (positive = shorter chains).
    pub depth: f64,
    /// Equality score delta (positive = more even complexity).
    pub equality: f64,
    /// Redundancy score delta (positive = less redundant code).
    pub redundancy: f64,
}

impl RootCauseDeltas {
    /// Compute deltas as `current - previous`.
    #[must_use]
    pub fn between(previous: &RootCauseScores, current: &RootCauseScores) -> Self {
        Self {
            modularity: current.modularity - previous.modularity,
            acyclicity: current.acyclicity - previous.acyclicity,
            depth: current.depth - previous.depth,
            equality: current.equality - previous.equality,
            redundancy: current.redundancy - previous.redundancy,
        }
    }

    /// Iterate `(label, delta)` in the canonical 5-axis order.
    #[must_use]
    pub fn iter_labelled(&self) -> [(&'static str, f64); 5] {
        [
            ("modularity", self.modularity),
            ("acyclicity", self.acyclicity),
            ("depth", self.depth),
            ("equality", self.equality),
            ("redundancy", self.redundancy),
        ]
    }

    /// Return `(label, delta)` for the most regressed dimension (most
    /// negative delta). Returns `None` if all dimensions are exactly
    /// equal in both snapshots.
    #[must_use]
    pub fn worst(&self) -> Option<(&'static str, f64)> {
        let pairs = self.iter_labelled();
        let mut worst: Option<(&'static str, f64)> = None;
        for (label, delta) in pairs {
            match worst {
                None => worst = Some((label, delta)),
                Some((_, w)) if delta < w => worst = Some((label, delta)),
                _ => {}
            }
        }
        worst.filter(|(_, d)| *d < 0.0)
    }

    /// Return `(label, delta)` for the most improved dimension.
    #[must_use]
    pub fn best(&self) -> Option<(&'static str, f64)> {
        let pairs = self.iter_labelled();
        let mut best: Option<(&'static str, f64)> = None;
        for (label, delta) in pairs {
            match best {
                None => best = Some((label, delta)),
                Some((_, b)) if delta > b => best = Some((label, delta)),
                _ => {}
            }
        }
        best.filter(|(_, d)| *d > 0.0)
    }
}

/// Comparison of two [`WorkspaceQualitySignal`] snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDiff {
    /// Aggregate signal at the previous snapshot (`0..=10000`).
    pub previous_signal: u32,
    /// Aggregate signal at the current snapshot (`0..=10000`).
    pub current_signal: u32,
    /// `current - previous` (signed, in `[-10000, +10000]`).
    pub delta_signal: i32,
    /// Per-root-cause deltas in `[-1.0, +1.0]`.
    pub delta_root_causes: RootCauseDeltas,
    /// Previous bottleneck label.
    pub previous_bottleneck: Bottleneck,
    /// Current bottleneck label.
    pub current_bottleneck: Bottleneck,
    /// Whether the bottleneck rotated (different root cause is the
    /// limiting factor now).
    pub bottleneck_changed: bool,
    /// Coarse-grained trend classification (`epsilon` aware).
    pub trend: SignalTrend,
    /// Epsilon used for the trend classifier.
    pub trend_epsilon: i32,
}

/// Diff two snapshots using [`DEFAULT_TREND_EPSILON`].
#[must_use]
pub fn diff_signals(
    previous: &WorkspaceQualitySignal,
    current: &WorkspaceQualitySignal,
) -> SignalDiff {
    diff_signals_with_epsilon(previous, current, DEFAULT_TREND_EPSILON)
}

/// Diff two snapshots with a caller-provided trend epsilon.
#[must_use]
pub fn diff_signals_with_epsilon(
    previous: &WorkspaceQualitySignal,
    current: &WorkspaceQualitySignal,
    epsilon: i32,
) -> SignalDiff {
    let delta_signal = i32::try_from(current.signal_0_10000).unwrap_or(0)
        - i32::try_from(previous.signal_0_10000).unwrap_or(0);
    let trend = classify_trend(delta_signal, epsilon);
    let delta_root_causes = RootCauseDeltas::between(&previous.root_causes, &current.root_causes);
    SignalDiff {
        previous_signal: previous.signal_0_10000,
        current_signal: current.signal_0_10000,
        delta_signal,
        delta_root_causes,
        previous_bottleneck: previous.bottleneck,
        current_bottleneck: current.bottleneck,
        bottleneck_changed: previous.bottleneck != current.bottleneck,
        trend,
        trend_epsilon: epsilon,
    }
}

fn classify_trend(delta_signal: i32, epsilon: i32) -> SignalTrend {
    let eps = epsilon.max(0);
    if delta_signal > eps {
        SignalTrend::Improving
    } else if delta_signal < -eps {
        SignalTrend::Regressing
    } else {
        SignalTrend::Stable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::signal::{FuncComplexity, Workspace, compute_quality_signal};

    fn perfect_signal() -> WorkspaceQualitySignal {
        compute_quality_signal(&Workspace::empty("/tmp/perfect"))
    }

    fn bad_signal() -> WorkspaceQualitySignal {
        let mut ws = Workspace::empty("/tmp/bad");
        for i in 0..3 {
            ws.edges.push((format!("a{i}.rs"), format!("b{i}.rs")));
            ws.edges.push((format!("b{i}.rs"), format!("a{i}.rs")));
        }
        ws.function_cc = (0..30)
            .map(|i| FuncComplexity {
                file: format!("a{i}.rs"),
                func: format!("f{i}"),
                cc: if i == 0 { 200 } else { 1 },
            })
            .collect();
        compute_quality_signal(&ws)
    }

    #[test]
    fn identical_snapshots_are_stable() {
        let s = perfect_signal();
        let diff = diff_signals(&s, &s);
        assert_eq!(diff.trend, SignalTrend::Stable);
        assert_eq!(diff.delta_signal, 0);
        assert!(!diff.bottleneck_changed);
    }

    #[test]
    fn regression_classified_when_signal_drops() {
        let prev = perfect_signal();
        let curr = bad_signal();
        let diff = diff_signals(&prev, &curr);
        assert_eq!(diff.trend, SignalTrend::Regressing);
        assert!(diff.delta_signal < -DEFAULT_TREND_EPSILON);
    }

    #[test]
    fn improvement_classified_when_signal_rises() {
        let prev = bad_signal();
        let curr = perfect_signal();
        let diff = diff_signals(&prev, &curr);
        assert_eq!(diff.trend, SignalTrend::Improving);
        assert!(diff.delta_signal > DEFAULT_TREND_EPSILON);
    }

    #[test]
    fn stable_within_epsilon() {
        let s = perfect_signal();
        let diff = diff_signals_with_epsilon(&s, &s, 100);
        assert_eq!(diff.trend, SignalTrend::Stable);
    }

    #[test]
    fn bottleneck_change_detected() {
        let prev = perfect_signal();
        let curr = bad_signal();
        let diff = diff_signals(&prev, &curr);
        // perfect has Tied; bad should have a non-Tied bottleneck.
        if diff.previous_bottleneck != diff.current_bottleneck {
            assert!(diff.bottleneck_changed);
        }
    }

    #[test]
    fn worst_root_cause_identifies_regression() {
        let prev = perfect_signal();
        let curr = bad_signal();
        let diff = diff_signals(&prev, &curr);
        let worst = diff.delta_root_causes.worst();
        assert!(worst.is_some(), "regression must surface a worst dimension");
        let (_, w) = worst.unwrap();
        assert!(w < 0.0);
    }

    #[test]
    fn best_root_cause_identifies_improvement() {
        let prev = bad_signal();
        let curr = perfect_signal();
        let diff = diff_signals(&prev, &curr);
        let best = diff.delta_root_causes.best();
        assert!(best.is_some());
        let (_, b) = best.unwrap();
        assert!(b > 0.0);
    }

    #[test]
    fn trend_label_round_trips() {
        assert_eq!(SignalTrend::Improving.label(), "improving");
        assert_eq!(SignalTrend::Regressing.label(), "regressing");
        assert_eq!(SignalTrend::Stable.label(), "stable");
    }

    #[test]
    fn epsilon_clamped_to_non_negative() {
        let prev = perfect_signal();
        let curr = perfect_signal();
        let diff = diff_signals_with_epsilon(&prev, &curr, -100);
        // Negative epsilon clamps to 0; identical snapshots → stable.
        assert_eq!(diff.trend, SignalTrend::Stable);
    }

    #[test]
    fn root_cause_deltas_iter_canonical_order() {
        let zero = RootCauseDeltas::default();
        let labels: Vec<&str> = zero.iter_labelled().iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            vec![
                "modularity",
                "acyclicity",
                "depth",
                "equality",
                "redundancy"
            ]
        );
    }
}
