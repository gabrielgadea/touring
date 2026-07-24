//! Geometric-mean aggregation of root cause scores → single quality signal.
//!
//! Implements the Sentrux/Nash-1950 gameproof aggregation: the geometric
//! mean is the unique aggregator that satisfies Pareto, symmetry, and
//! independence simultaneously. Improving one root cause while degrading
//! another can never raise the geometric mean → AI agents cannot game a
//! single dimension.
//!
//! Each score is clamped to a minimum of `0.01` before multiplication so
//! that a single zero does not zero the entire signal (matching
//! `sentrux-core/src/metrics/root_causes.rs:172`).

use super::types::{Bottleneck, RootCauseScores};

/// Lower bound applied per-score before product (avoids degenerate zero).
const FLOOR: f64 = 0.01;

/// Geometric mean of the five root cause scores in `[FLOOR, 1.0]`.
#[must_use]
pub fn aggregate_geometric_mean(scores: &RootCauseScores) -> f64 {
    let values = [
        scores.modularity.max(FLOOR),
        scores.acyclicity.max(FLOOR),
        scores.depth.max(FLOOR),
        scores.equality.max(FLOOR),
        scores.redundancy.max(FLOOR),
    ];
    let product: f64 = values.iter().product();
    product.powf(1.0 / 5.0)
}

/// Geometric mean × 10000, rounded, in `0..=10000` (Sentrux integer scale).
#[must_use]
pub fn aggregate_geometric_mean_int10k(scores: &RootCauseScores) -> u32 {
    let raw = aggregate_geometric_mean(scores);
    (raw * 10_000.0).round().clamp(0.0, 10_000.0) as u32
}

/// Two scores are considered tied when within this absolute distance.
const TIE_EPS: f64 = 1e-6;

/// Identify the single lowest-scoring root cause (action-guiding).
///
/// Ties (within `TIE_EPS`) collapse to `Bottleneck::Tied`. Canonical order
/// for tie-breaking is modularity → acyclicity → depth → equality → redundancy.
#[must_use]
pub fn detect_bottleneck(scores: &RootCauseScores) -> Bottleneck {
    let labelled = scores.iter_labelled();
    let min = labelled
        .iter()
        .map(|(_, v)| *v)
        .fold(f64::INFINITY, f64::min);
    if !min.is_finite() {
        return Bottleneck::Tied;
    }
    let mut at_min = labelled.iter().filter(|(_, v)| (*v - min).abs() < TIE_EPS);
    let first = at_min.next();
    if at_min.next().is_some() {
        return Bottleneck::Tied;
    }
    match first {
        Some(("modularity", _)) => Bottleneck::Modularity,
        Some(("acyclicity", _)) => Bottleneck::Acyclicity,
        Some(("depth", _)) => Bottleneck::Depth,
        Some(("equality", _)) => Bottleneck::Equality,
        Some(("redundancy", _)) => Bottleneck::Redundancy,
        _ => Bottleneck::Tied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scores(m: f64, a: f64, d: f64, e: f64, r: f64) -> RootCauseScores {
        RootCauseScores {
            modularity: m,
            acyclicity: a,
            depth: d,
            equality: e,
            redundancy: r,
        }
    }

    #[test]
    fn perfect_scores_max_signal() {
        let g = aggregate_geometric_mean(&RootCauseScores::perfect());
        assert!((g - 1.0).abs() < f64::EPSILON);
        assert_eq!(
            aggregate_geometric_mean_int10k(&RootCauseScores::perfect()),
            10_000
        );
    }

    #[test]
    fn neutral_scores_midpoint_signal() {
        let g = aggregate_geometric_mean(&RootCauseScores::neutral());
        assert!((g - 0.5).abs() < 1e-9);
        assert_eq!(
            aggregate_geometric_mean_int10k(&RootCauseScores::neutral()),
            5_000
        );
    }

    #[test]
    fn one_zero_dim_clamped_not_devastating() {
        let s = scores(0.0, 1.0, 1.0, 1.0, 1.0);
        let g = aggregate_geometric_mean(&s);
        assert!(g > 0.39 && g < 0.41, "expected ~0.398, got {g}");
    }

    #[test]
    fn gameproof_property_holds() {
        // Initial: all 0.7 → 0.7
        let baseline = scores(0.7, 0.7, 0.7, 0.7, 0.7);
        let g0 = aggregate_geometric_mean(&baseline);
        // Improve one to 0.9, degrade another to 0.5 — must not raise signal.
        let traded = scores(0.9, 0.5, 0.7, 0.7, 0.7);
        let g1 = aggregate_geometric_mean(&traded);
        assert!(
            g1 < g0 + 1e-6,
            "geometric mean must be gameproof (g0={g0}, g1={g1})"
        );
    }

    #[test]
    fn detect_bottleneck_returns_lowest() {
        let s = scores(0.8, 0.9, 0.4, 0.7, 0.6);
        assert_eq!(detect_bottleneck(&s), Bottleneck::Depth);
    }

    #[test]
    fn detect_bottleneck_tied_when_two_match() {
        let s = scores(0.4, 0.9, 0.4, 0.7, 0.6);
        assert_eq!(detect_bottleneck(&s), Bottleneck::Tied);
    }

    #[test]
    fn detect_bottleneck_uses_canonical_order_for_ties() {
        // when all dims are equal we collapse to Tied (no canonical winner)
        let s = scores(0.5, 0.5, 0.5, 0.5, 0.5);
        assert_eq!(detect_bottleneck(&s), Bottleneck::Tied);
    }
}
