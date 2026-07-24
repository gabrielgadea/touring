//! Distribution drift detection.
//!
//! Pln3 Phase 1B - L2: Implementation
//! Provides KS statistic and JS divergence for detecting concept drift.

use super::traits::DriftDetection;
use crate::simd_utils::reduce_sum_f64;

/// Drift detector with KS and JS divergence.
#[derive(Debug, Clone, Default)]
pub struct DriftDetector;

impl DriftDetector {
    /// Create new drift detector.
    pub fn new() -> Self {
        Self
    }
}

impl DriftDetection for DriftDetector {
    #[allow(clippy::indexing_slicing)] // indices bounded by while condition
    fn ks_statistic(&self, sample1: &[f64], sample2: &[f64]) -> f64 {
        if sample1.is_empty() || sample2.is_empty() {
            return 0.0;
        }

        // Sort both samples
        let mut s1: Vec<f64> = sample1.to_vec();
        let mut s2: Vec<f64> = sample2.to_vec();
        s1.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        s2.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let n1 = s1.len() as f64;
        let n2 = s2.len() as f64;

        // Optimized merge-walk: O(n+m) instead of O((n+m)*n) with linear scan
        // Walk both sorted arrays simultaneously, computing CDF differences
        let mut max_diff: f64 = 0.0;
        let mut i = 0usize;
        let mut j = 0usize;

        while i < s1.len() && j < s2.len() {
            if s1[i] < s2[j] {
                i += 1;
            } else if s1[i] > s2[j] {
                j += 1;
            } else {
                // Equal values: advance both
                i += 1;
                j += 1;
            }

            let cdf1 = i as f64 / n1;
            let cdf2 = j as f64 / n2;
            let diff = (cdf1 - cdf2).abs();
            max_diff = max_diff.max(diff);
        }

        // Handle remaining elements in either sample
        if i < s1.len() {
            // All remaining s1 values > all s2 values
            // cdf2 is already 1.0 (j == s2.len())
            let cdf1_final = s1.len() as f64 / n1; // = 1.0
            let diff = (cdf1_final - 1.0).abs();
            max_diff = max_diff.max(diff);
        }
        if j < s2.len() {
            let cdf2_final = s2.len() as f64 / n2;
            let diff = (1.0 - cdf2_final).abs();
            max_diff = max_diff.max(diff);
        }

        max_diff
    }

    fn js_divergence(&self, dist1: &[f64], dist2: &[f64]) -> f64 {
        if dist1.len() != dist2.len() || dist1.is_empty() {
            return 0.0;
        }

        // Normalize distributions
        let sum1 = reduce_sum_f64(dist1);
        let sum2 = reduce_sum_f64(dist2);

        if sum1 == 0.0 || sum2 == 0.0 {
            return 0.0;
        }

        let p: Vec<f64> = dist1.iter().map(|&x| x / sum1).collect();
        let q: Vec<f64> = dist2.iter().map(|&x| x / sum2).collect();

        // M = (P + Q) / 2
        let m: Vec<f64> = p
            .iter()
            .zip(q.iter())
            .map(|(pi, qi)| (pi + qi) / 2.0)
            .collect();

        // JS(P||Q) = (KL(P||M) + KL(Q||M)) / 2
        let divergence_p = kl_divergence(&p, &m);
        let divergence_q = kl_divergence(&q, &m);

        ((divergence_p + divergence_q) / 2.0).sqrt() // JS distance (square root of divergence)
    }
}

/// Compute KL divergence D_KL(P || Q).
///
/// Handles zero probabilities by adding small epsilon.
#[inline]
fn kl_divergence(p: &[f64], q: &[f64]) -> f64 {
    const EPSILON: f64 = 1e-12;

    let mut kl = 0.0;
    for (pi, qi) in p.iter().zip(q.iter()) {
        if *pi > EPSILON {
            let qi_safe = qi.max(EPSILON);
            kl += pi * (pi / qi_safe).ln();
        }
    }

    kl.max(0.0)
}

/// Check if drift exceeds threshold.
#[inline]
#[must_use]
pub fn has_significant_drift(statistic: f64, threshold: f64) -> bool {
    statistic > threshold
}

/// Compute critical value for KS test at given significance level.
///
/// Approximation for two-sample KS test.
#[must_use]
pub fn ks_critical_value(n1: usize, n2: usize, alpha: f64) -> f64 {
    let c_alpha = (-0.5 * (alpha / 2.0).ln()).sqrt();
    c_alpha * ((n1 + n2) as f64 / (n1 * n2) as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_ks_identical_samples() {
        let detector = DriftDetector::new();
        let sample = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ks = detector.ks_statistic(&sample, &sample);
        assert_relative_eq!(ks, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_ks_different_samples() {
        let detector = DriftDetector::new();
        let s1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let s2 = vec![6.0, 7.0, 8.0, 9.0, 10.0];
        let ks = detector.ks_statistic(&s1, &s2);
        // Completely disjoint samples should have KS = 1.0
        assert_relative_eq!(ks, 1.0, epsilon = 1e-10);
    }

    #[test]
    fn test_ks_overlapping_samples() {
        let detector = DriftDetector::new();
        let s1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let s2 = vec![3.0, 4.0, 5.0, 6.0, 7.0];
        let ks = detector.ks_statistic(&s1, &s2);
        assert!(ks > 0.0);
        assert!(ks < 1.0);
    }

    #[test]
    fn test_ks_empty_sample() {
        let detector = DriftDetector::new();
        let s1: Vec<f64> = vec![];
        let s2 = vec![1.0, 2.0, 3.0];
        assert_eq!(detector.ks_statistic(&s1, &s2), 0.0);
    }

    #[test]
    fn test_js_identical_distributions() {
        let detector = DriftDetector::new();
        let dist = vec![0.1, 0.2, 0.3, 0.4];
        let js = detector.js_divergence(&dist, &dist);
        assert_relative_eq!(js, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn test_js_different_distributions() {
        let detector = DriftDetector::new();
        let d1 = vec![1.0, 0.0, 0.0, 0.0];
        let d2 = vec![0.0, 0.0, 0.0, 1.0];
        let js = detector.js_divergence(&d1, &d2);
        // Different distributions should have JS > 0
        assert!(js > 0.0);
        assert!(js <= 1.0); // JS distance is bounded by 1
    }

    #[test]
    fn test_js_bounds() {
        let detector = DriftDetector::new();
        // Generate various distributions and check bounds
        for _ in 0..10 {
            let d1: Vec<f64> = (0..10).map(|i| (i + 1) as f64).collect();
            let d2: Vec<f64> = (0..10).map(|i| (10 - i) as f64).collect();

            let js = detector.js_divergence(&d1, &d2);
            assert!(js >= 0.0, "JS should be >= 0");
            assert!(js <= 1.0, "JS should be <= 1");
        }
    }

    #[test]
    fn test_js_symmetry() {
        let detector = DriftDetector::new();
        let d1 = vec![0.3, 0.2, 0.5];
        let d2 = vec![0.1, 0.4, 0.5];

        let js_12 = detector.js_divergence(&d1, &d2);
        let js_21 = detector.js_divergence(&d2, &d1);

        assert_relative_eq!(js_12, js_21, epsilon = 1e-10);
    }

    #[test]
    fn test_has_significant_drift() {
        assert!(has_significant_drift(0.5, 0.3));
        assert!(!has_significant_drift(0.2, 0.3));
    }

    #[test]
    fn test_ks_critical_value() {
        let cv = ks_critical_value(100, 100, 0.05);
        // For n1=n2=100 at alpha=0.05, critical value is approximately 0.192
        assert!(cv > 0.1);
        assert!(cv < 0.3);
    }
}
