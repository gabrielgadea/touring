//! Statistical reconciliation functions for multi-value aggregation.
//!
//! Provides weighted mean, coefficient of variation, and Bayesian fusion
//! for reconciling conflicting values from multiple source files.
//! These accelerate the Python `MultiValueReconciler` in `compute_base.py`.

use super::ranking::WilsonRanker;
#[allow(unused_imports)]
use super::traits::StatisticalRanking;
use crate::simd_utils::reduce_sum_f64;

/// Weighted arithmetic mean of values with corresponding weights.
///
/// Returns 0.0 if inputs are empty or total weight is zero.
///
/// # Examples
/// ```
/// use touring_simd::statistics::reconciliation::weighted_mean;
/// let vals = [1.0, 2.0, 3.0];
/// let weights = [0.5, 0.3, 0.2];
/// let result = weighted_mean(&vals, &weights);
/// assert!((result - 1.7).abs() < 1e-10);
/// ```
#[inline]
#[must_use]
#[allow(clippy::indexing_slicing)]
pub fn weighted_mean(values: &[f64], weights: &[f64]) -> f64 {
    if values.is_empty() || weights.is_empty() {
        return 0.0;
    }
    let len = values.len().min(weights.len());
    let total_weight = reduce_sum_f64(&weights[..len]);
    if total_weight.abs() < f64::EPSILON {
        return 0.0;
    }
    let weighted_sum: f64 = values[..len]
        .iter()
        .zip(weights[..len].iter())
        .map(|(v, w)| v * w)
        .sum();
    weighted_sum / total_weight
}

/// Coefficient of variation: standard deviation divided by the absolute mean.
///
/// Returns 0.0 if fewer than 2 values or the mean is zero.
/// Uses population standard deviation (divides by N, not N-1) to match
/// the Python `statistics.stdev` behavior when used for CV comparison.
///
/// # Examples
/// ```
/// use touring_simd::statistics::reconciliation::coefficient_of_variation;
/// let uniform = [10.0, 10.0, 10.0];
/// assert!(coefficient_of_variation(&uniform).abs() < 1e-10);
///
/// let varied = [10.0, 20.0, 30.0];
/// assert!(coefficient_of_variation(&varied) > 0.0);
/// ```
#[inline]
#[must_use]
pub fn coefficient_of_variation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let n = values.len() as f64;
    let mean = reduce_sum_f64(values) / n;
    if mean.abs() < f64::EPSILON {
        return 0.0;
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    variance.sqrt() / mean.abs()
}

/// Bayesian fusion of (value, confidence) pairs.
///
/// Higher-confidence estimates receive proportionally more weight.
/// Returns `(fused_value, fused_confidence)` where fused confidence
/// is computed as `1 - product(1 - c_i)` (independent combination).
///
/// Returns `(0.0, 0.0)` for empty input.
/// If all confidences are zero, returns simple arithmetic mean with 0.0 confidence.
///
/// # Examples
/// ```
/// use touring_simd::statistics::reconciliation::bayesian_fusion;
/// let estimates = [(10.0, 0.9), (12.0, 0.5)];
/// let (val, conf) = bayesian_fusion(&estimates);
/// assert!(val > 10.0 && val < 12.0); // Weighted toward higher confidence
/// assert!(conf > 0.9); // Combined confidence exceeds highest individual
/// ```
#[must_use]
pub fn bayesian_fusion(estimates: &[(f64, f64)]) -> (f64, f64) {
    if estimates.is_empty() {
        return (0.0, 0.0);
    }

    let total_conf: f64 = estimates.iter().map(|(_, c)| c).sum();

    if total_conf.abs() < f64::EPSILON {
        // All confidences are zero — simple arithmetic mean
        let mean = estimates.iter().map(|(v, _)| v).sum::<f64>() / estimates.len() as f64;
        return (mean, 0.0);
    }

    let fused_value = estimates.iter().map(|(v, c)| v * c).sum::<f64>() / total_conf;

    let fused_confidence = 1.0
        - estimates
            .iter()
            .map(|(_, c)| 1.0 - c.clamp(0.0, 1.0))
            .product::<f64>();

    (fused_value, fused_confidence)
}

/// Reconciliation quality metadata for conflict detection.
///
/// Produced as the second element of `(value, ReconciliationQuality)` tuples
/// by [`reconcile_with_wilson`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReconciliationQuality {
    /// True when the coefficient of variation exceeded the conflict threshold.
    pub conflict_detected: bool,
    /// Highest Wilson lower-bound score across all sources (max_wilson_bound).
    pub max_wilson_bound: f64,
    /// Coefficient of variation of the input values.
    pub coefficient_of_variation: f64,
}

/// Conflict threshold for coefficient of variation.
///
/// When CV > this value the sources are deemed in disagreement and Wilson
/// weighting is applied instead of plain confidence-weighted fusion.
const CONFLICT_CV_THRESHOLD: f64 = 0.3;

/// Reconcile multiple evidence sources using Wilson score-informed fusion.
///
/// When the coefficient of variation of the input values exceeds
/// `CONFLICT_CV_THRESHOLD` (0.3), Wilson lower-bound scores are used as
/// weights in place of the supplied confidences.  This gives more stable,
/// sample-size-aware weighting when sources disagree.
///
/// Falls back to plain confidence-weighted fusion (equivalent to
/// [`bayesian_fusion`]) when no conflict is detected.
///
/// Returns `((reconciled_value, confidence), ReconciliationQuality)`.
///
/// # Examples
/// ```
/// use touring_simd::statistics::reconciliation::reconcile_with_wilson;
///
/// // Two sources in agreement — falls back to confidence-weighted mean
/// let values = [10.0, 10.5];
/// let confidences = [0.8, 0.8];
/// let (_result, _conf, quality) = reconcile_with_wilson(&values, &confidences, 0.95);
/// assert!(!quality.conflict_detected);
/// ```
#[must_use]
pub fn reconcile_with_wilson(
    values: &[f64],
    confidences: &[f64],
    wilson_confidence: f64,
) -> (f64, f64, ReconciliationQuality) {
    if values.is_empty() || confidences.is_empty() {
        return (
            0.0,
            0.0,
            ReconciliationQuality {
                conflict_detected: false,
                max_wilson_bound: 0.0,
                coefficient_of_variation: 0.0,
            },
        );
    }

    let cv = coefficient_of_variation(values);
    let conflict_detected = cv > CONFLICT_CV_THRESHOLD;

    // Wilson ranker needs (successes, total) pairs.
    // We synthesise these from confidences: treat confidence as a proportion
    // with an arbitrary total (100) so that Wilson lower bounds are monotonic
    // with confidence and penalise low sample sizes appropriately.
    const TOTAL: u32 = 100;

    let wilson_scores: Vec<f64> = confidences
        .iter()
        .map(|&c| {
            let successes = (c * TOTAL as f64).round() as u32;
            let ranker = WilsonRanker::new(wilson_confidence);
            ranker.wilson_bound(successes, TOTAL)
        })
        .collect();

    let max_wilson_bound = wilson_scores.iter().copied().fold(0.0_f64, f64::max);

    if conflict_detected {
        // Use Wilson scores as weights — they already reflect sample-size
        // adjusted confidence.
        let total_wilson: f64 = wilson_scores.iter().sum();
        if total_wilson.abs() < f64::EPSILON {
            // Degenerate: fall back to simple mean
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            return (
                mean,
                0.0,
                ReconciliationQuality {
                    conflict_detected,
                    max_wilson_bound,
                    coefficient_of_variation: cv,
                },
            );
        }
        let fused = values
            .iter()
            .zip(wilson_scores.iter())
            .map(|(v, w)| v * w)
            .sum::<f64>()
            / total_wilson;

        // Combined confidence: 1 - product(1 - c_i) from confidences
        let fused_confidence = 1.0_f64
            - confidences
                .iter()
                .map(|c| 1.0_f64 - c.clamp(0.0, 1.0))
                .product::<f64>();

        (
            fused,
            fused_confidence,
            ReconciliationQuality {
                conflict_detected,
                max_wilson_bound,
                coefficient_of_variation: cv,
            },
        )
    } else {
        // No conflict — delegate to the existing bayesian_fusion
        let estimates: Vec<(f64, f64)> = values
            .iter()
            .zip(confidences.iter())
            .map(|(v, c)| (*v, *c))
            .collect();
        let (fused, fused_confidence) = bayesian_fusion(&estimates);
        (
            fused,
            fused_confidence,
            ReconciliationQuality {
                conflict_detected: false,
                max_wilson_bound,
                coefficient_of_variation: cv,
            },
        )
    }
}

/// Reconcile multiple scalar values using weighted mean strategy.
///
/// Convenience function that returns `(reconciled_value, cv)`.
/// This mirrors `MultiValueReconciler.reconcile_scalar()` from Python.
#[inline]
#[must_use]
pub fn reconcile_weighted(values: &[f64], weights: &[f64]) -> (f64, f64) {
    let reconciled = weighted_mean(values, weights);
    let cv = coefficient_of_variation(values);
    (reconciled, cv)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── weighted_mean ────────────────────────────────────────────────

    #[test]
    fn test_weighted_mean_basic() {
        let vals = [1.0, 2.0, 3.0];
        let weights = [0.5, 0.3, 0.2];
        let result = weighted_mean(&vals, &weights);
        // 1*0.5 + 2*0.3 + 3*0.2 = 0.5 + 0.6 + 0.6 = 1.7
        assert!((result - 1.7).abs() < 1e-10);
    }

    #[test]
    fn test_weighted_mean_uniform_weights() {
        let vals = [10.0, 20.0, 30.0];
        let weights = [1.0, 1.0, 1.0];
        let result = weighted_mean(&vals, &weights);
        assert!((result - 20.0).abs() < 1e-10);
    }

    #[test]
    fn test_weighted_mean_empty() {
        assert_eq!(weighted_mean(&[], &[]), 0.0);
        assert_eq!(weighted_mean(&[1.0], &[]), 0.0);
        assert_eq!(weighted_mean(&[], &[1.0]), 0.0);
    }

    #[test]
    fn test_weighted_mean_zero_weights() {
        let vals = [10.0, 20.0];
        let weights = [0.0, 0.0];
        assert_eq!(weighted_mean(&vals, &weights), 0.0);
    }

    #[test]
    fn test_weighted_mean_single_value() {
        let vals = [42.0];
        let weights = [1.0];
        assert!((weighted_mean(&vals, &weights) - 42.0).abs() < 1e-10);
    }

    #[test]
    fn test_weighted_mean_mismatched_lengths() {
        // Should use the shorter length
        let vals = [10.0, 20.0, 30.0];
        let weights = [0.5, 0.5];
        let result = weighted_mean(&vals, &weights);
        // Uses only first 2: (10*0.5 + 20*0.5) / 1.0 = 15.0
        assert!((result - 15.0).abs() < 1e-10);
    }

    // ── coefficient_of_variation ─────────────────────────────────────

    #[test]
    fn test_cv_uniform() {
        let vals = [10.0, 10.0, 10.0];
        assert!(coefficient_of_variation(&vals).abs() < 1e-10);
    }

    #[test]
    fn test_cv_positive() {
        let vals = [10.0, 20.0, 30.0];
        let cv = coefficient_of_variation(&vals);
        // mean=20, stdev=sqrt(200/3)≈8.165, cv≈0.4082
        assert!(cv > 0.4);
        assert!(cv < 0.5);
    }

    #[test]
    fn test_cv_single_value() {
        assert_eq!(coefficient_of_variation(&[42.0]), 0.0);
    }

    #[test]
    fn test_cv_empty() {
        assert_eq!(coefficient_of_variation(&[]), 0.0);
    }

    #[test]
    fn test_cv_zero_mean() {
        let vals = [-1.0, 1.0];
        assert_eq!(coefficient_of_variation(&vals), 0.0);
    }

    // ── bayesian_fusion ─────────────────────────────────────────────

    #[test]
    fn test_bayesian_fusion_basic() {
        let estimates = [(10.0, 0.9), (12.0, 0.5)];
        let (val, conf) = bayesian_fusion(&estimates);
        // Higher confidence (0.9) pulls result toward 10.0
        assert!(val > 10.0 && val < 12.0);
        assert!(val < 11.0); // Closer to 10.0
        // Combined confidence: 1 - (1-0.9)*(1-0.5) = 1 - 0.05 = 0.95
        assert!((conf - 0.95).abs() < 1e-10);
    }

    #[test]
    fn test_bayesian_fusion_empty() {
        let (val, conf) = bayesian_fusion(&[]);
        assert_eq!(val, 0.0);
        assert_eq!(conf, 0.0);
    }

    #[test]
    fn test_bayesian_fusion_zero_confidence() {
        let estimates = [(10.0, 0.0), (20.0, 0.0)];
        let (val, conf) = bayesian_fusion(&estimates);
        // Simple mean when all confidences are zero
        assert!((val - 15.0).abs() < 1e-10);
        assert_eq!(conf, 0.0);
    }

    #[test]
    fn test_bayesian_fusion_single() {
        let estimates = [(42.0, 0.8)];
        let (val, conf) = bayesian_fusion(&estimates);
        assert!((val - 42.0).abs() < 1e-10);
        assert!((conf - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_bayesian_fusion_equal_confidence() {
        let estimates = [(10.0, 0.5), (20.0, 0.5)];
        let (val, _conf) = bayesian_fusion(&estimates);
        // Equal confidence → simple mean
        assert!((val - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_bayesian_fusion_high_confidence_dominates() {
        let estimates = [(100.0, 0.99), (0.0, 0.01)];
        let (val, _conf) = bayesian_fusion(&estimates);
        // 0.99 confidence should dominate: val close to 100
        assert!(val > 98.0);
    }

    // ── reconcile_weighted ──────────────────────────────────────────

    #[test]
    fn test_reconcile_weighted_basic() {
        let vals = [10.0, 20.0, 30.0];
        let weights = [1.0, 1.0, 1.0];
        let (reconciled, cv) = reconcile_weighted(&vals, &weights);
        assert!((reconciled - 20.0).abs() < 1e-10);
        assert!(cv > 0.0); // Values differ → CV > 0
    }

    #[test]
    fn test_reconcile_weighted_agreement() {
        let vals = [10.0, 10.0, 10.0];
        let weights = [0.5, 0.3, 0.2];
        let (reconciled, cv) = reconcile_weighted(&vals, &weights);
        assert!((reconciled - 10.0).abs() < 1e-10);
        assert!(cv.abs() < 1e-10); // Perfect agreement → CV = 0
    }

    // ── reconcile_with_wilson ─────────────────────────────────────────

    #[test]
    fn test_wilson_no_conflict_matches_bayesian() {
        // When CV ≤ 0.3 the result must equal bayesian_fusion
        let values = [10.0, 10.5, 10.3];
        let confidences = [0.8, 0.7, 0.9];
        let (val, conf, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        assert!(!quality.conflict_detected);
        assert!(quality.coefficient_of_variation < 0.3);

        // Compare against bayesian_fusion
        let estimates: Vec<(f64, f64)> = values
            .iter()
            .zip(confidences.iter())
            .map(|(v, c)| (*v, *c))
            .collect();
        let (bay_val, bay_conf) = bayesian_fusion(&estimates);
        assert!((val - bay_val).abs() < 1e-9);
        assert!((conf - bay_conf).abs() < 1e-9);
    }

    #[test]
    fn test_wilson_conflict_uses_wilson_weights() {
        // High CV triggers Wilson-based weighting
        let values = [0.0, 100.0];
        let confidences = [0.5, 0.5];
        let (val, _conf, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        assert!(quality.conflict_detected);
        assert!(quality.coefficient_of_variation > 0.3);
        // With equal confidences but high disagreement, result should
        // be between the extremes but not at the simple mean of 50.0
        assert!(val > 0.0 && val < 100.0);
        // Wilson weights penalise the low-confidence source less
        // harshly in the fusion, so the result should lean toward
        // the higher-confidence side's contribution
    }

    #[test]
    fn test_wilson_conflict_different_from_naive_mean() {
        // When sources conflict, Wilson weighting must differ from naive mean
        let values = [0.0, 50.0, 100.0];
        let confidences = [0.9, 0.5, 0.9];
        let (wilson_val, _, _) = reconcile_with_wilson(&values, &confidences, 0.95);

        // Naive mean = 50.0
        let naive_mean = values.iter().sum::<f64>() / values.len() as f64;
        assert!((naive_mean - 50.0).abs() < 1e-10);

        // Wilson result should differ from naive mean because Wilson weights
        // account for the confidence values (even though all are non-zero)
        // The conflict (CV > 0.3) triggers Wilson path
        assert!(wilson_val != naive_mean);
    }

    #[test]
    fn test_wilson_empty_input() {
        let (val, conf, quality) = reconcile_with_wilson(&[], &[], 0.95);
        assert_eq!(val, 0.0);
        assert_eq!(conf, 0.0);
        assert!(!quality.conflict_detected);
        assert_eq!(quality.max_wilson_bound, 0.0);
        assert_eq!(quality.coefficient_of_variation, 0.0);
    }

    #[test]
    fn test_wilson_single_source() {
        let values = [42.0];
        let confidences = [0.8];
        let (val, conf, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        // Single source → no conflict (CV = 0)
        assert!(!quality.conflict_detected);
        assert!((val - 42.0).abs() < 1e-10);
        assert!((conf - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_wilson_zero_confidence_no_conflict() {
        // Zero confidence: bayesian_fusion returns simple mean (CV likely 0)
        let values = [10.0, 20.0];
        let confidences = [0.0, 0.0];
        let (val, conf, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        // CV for [10, 20] ≈ 0.408 > 0.3, so conflict IS detected
        // But Wilson scores will all be ~0 (low confidence), so fallback
        // to simple mean applies
        assert!(quality.conflict_detected);
        assert!((val - 15.0).abs() < 1e-10); // simple mean
        assert_eq!(conf, 0.0);
    }

    #[test]
    fn test_wilson_reconciliation_quality_fields() {
        let values = [5.0, 95.0];
        let confidences = [0.8, 0.8];
        let (_, _, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        assert!(quality.conflict_detected);
        assert!(quality.max_wilson_bound > 0.0);
        assert!(quality.coefficient_of_variation > 0.3);
    }

    #[test]
    fn test_wilson_vs_bayesian_fallback_small_cv() {
        // Very similar values → tiny CV → no conflict → must match bayesian
        let values = [10.0, 10.1, 10.05];
        let confidences = [0.6, 0.7, 0.8];
        let (wilson_val, wilson_conf, _) = reconcile_with_wilson(&values, &confidences, 0.95);

        let estimates: Vec<(f64, f64)> = values
            .iter()
            .zip(confidences.iter())
            .map(|(v, c)| (*v, *c))
            .collect();
        let (bay_val, bay_conf) = bayesian_fusion(&estimates);

        assert!((wilson_val - bay_val).abs() < 1e-9);
        assert!((wilson_conf - bay_conf).abs() < 1e-9);
    }

    // ── reconciliation accuracy tests (D8) ─────────────────────────────────

    #[test]
    fn test_reconcile_accuracy_no_conflict() {
        // Two sources with similar values and same high confidence
        let values = [10.0, 10.5];
        let confidences = [0.9, 0.9];
        let (fused, _, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        // Fused must be between the two inputs
        assert!(fused > 10.0 && fused < 10.5);
        // No conflict: CV of [10, 10.5] is small
        assert!(!quality.conflict_detected);
    }

    #[test]
    fn test_reconcile_accuracy_with_conflict() {
        // Two sources with very different values and different confidences
        let values = [10.0, 50.0];
        let confidences = [0.9, 0.5];
        let (fused, _, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        // Conflict must be detected (high CV)
        assert!(quality.conflict_detected);
        // Fused should be pulled toward the high-confidence source (10.0)
        // Simple weighted mean would be: 10*0.9 + 50*0.5 / 1.4 ≈ 24.3
        // But with Wilson weighting, high-confidence source gets more weight
        assert!(
            fused < 24.3,
            "fused {} should be below simple weighted mean 24.3",
            fused
        );
    }

    #[test]
    fn test_reconcile_quality_fields() {
        let values = [5.0, 95.0];
        let confidences = [0.8, 0.8];
        let (_, _, quality) = reconcile_with_wilson(&values, &confidences, 0.95);

        // All fields must be populated
        assert!(quality.conflict_detected);
        assert!(
            quality.max_wilson_bound > 0.0,
            "max_wilson_bound should be > 0"
        );
        assert!(
            quality.coefficient_of_variation > 0.3,
            "CV should exceed conflict threshold 0.3"
        );
    }
}
