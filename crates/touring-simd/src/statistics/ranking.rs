//! Wilson score ranking for skill selection.
//!
//! Pln3 Phase 1B - L2: Implementation
//! Formula: (p + z²/2n - z√(p(1-p)/n + z²/4n²)) / (1 + z²/n)

use super::traits::StatisticalRanking;
use rayon::prelude::*;
use statrs::distribution::{ContinuousCDF, Normal};

/// Wilson score ranker with configurable z-score.
#[derive(Debug, Clone)]
pub struct WilsonRanker {
    /// Z-score for confidence level (1.96 for 95%).
    z_score: f64,
    /// Parallel threshold for batch operations.
    parallel_threshold: usize,
}

impl Default for WilsonRanker {
    fn default() -> Self {
        Self::new(0.95)
    }
}

impl WilsonRanker {
    /// Create ranker with specified confidence level.
    ///
    /// Common values: 0.90 (z=1.645), 0.95 (z=1.96), 0.99 (z=2.576)
    pub fn new(confidence: f64) -> Self {
        let normal = Normal::new(0.0, 1.0).unwrap_or_else(|_| Normal::standard());
        let z_score = normal.inverse_cdf((1.0 + confidence) / 2.0);

        Self {
            z_score,
            parallel_threshold: 100,
        }
    }

    /// Create with explicit z-score.
    pub fn with_z_score(z_score: f64) -> Self {
        Self {
            z_score,
            parallel_threshold: 100,
        }
    }

    /// Compute Wilson score lower bound.
    ///
    /// This is the lower bound of the confidence interval for the
    /// true success rate, accounting for small sample sizes.
    #[inline]
    fn wilson_lower_bound(&self, successes: u32, total: u32) -> f64 {
        if total == 0 {
            return 0.0;
        }

        let n = total as f64;
        let p = successes as f64 / n;
        let z = self.z_score;
        let z2 = z * z;

        // Wilson score interval lower bound formula
        let numerator = p + z2 / (2.0 * n) - z * ((p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt());
        let denominator = 1.0 + z2 / n;

        (numerator / denominator).clamp(0.0, 1.0)
    }
}

impl WilsonRanker {
    /// Compute Wilson score lower bound (public, for direct use outside trait).
    ///
    /// Convenience wrapper that calls the internal method.
    #[inline]
    #[must_use]
    pub fn wilson_bound(&self, successes: u32, total: u32) -> f64 {
        self.wilson_lower_bound(successes, total)
    }
}

impl StatisticalRanking for WilsonRanker {
    fn wilson_score(&self, successes: u32, total: u32, _confidence: f64) -> f64 {
        // Note: confidence is already baked into z_score at construction
        self.wilson_lower_bound(successes, total)
    }

    fn wilson_scores_batch(&self, data: &[(u32, u32)], _confidence: f64) -> Vec<f64> {
        if data.is_empty() {
            return vec![];
        }

        if data.len() >= self.parallel_threshold {
            data.par_iter()
                .map(|(s, t)| self.wilson_lower_bound(*s, *t))
                .collect()
        } else {
            data.iter()
                .map(|(s, t)| self.wilson_lower_bound(*s, *t))
                .collect()
        }
    }
}

/// Rank items by Wilson score, returning indices sorted by score descending.
#[must_use]
pub fn rank_by_wilson(data: &[(u32, u32)], confidence: f64) -> Vec<usize> {
    let ranker = WilsonRanker::new(confidence);
    let scores = ranker.wilson_scores_batch(data, confidence);

    let mut indices: Vec<usize> = (0..scores.len()).collect();
    indices.sort_by(|&idx_a, &idx_b| {
        let score_a = scores.get(idx_a).copied().unwrap_or(0.0);
        let score_b = scores.get(idx_b).copied().unwrap_or(0.0);
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    indices
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs are asserted non-empty before indexing
    use super::*;

    #[test]
    fn test_wilson_perfect_score() {
        let ranker = WilsonRanker::new(0.95);
        // 100% success rate should give high score
        let score = ranker.wilson_score(100, 100, 0.95);
        assert!(score > 0.95);
    }

    #[test]
    fn test_wilson_zero_score() {
        let ranker = WilsonRanker::new(0.95);
        // 0% success rate should give low score
        let score = ranker.wilson_score(0, 100, 0.95);
        assert!(score < 0.05);
    }

    #[test]
    fn test_wilson_empty() {
        let ranker = WilsonRanker::new(0.95);
        // No trials = 0 score
        let score = ranker.wilson_score(0, 0, 0.95);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_wilson_small_sample() {
        let ranker = WilsonRanker::new(0.95);
        // Small sample with 100% should not be 1.0
        let score = ranker.wilson_score(5, 5, 0.95);
        assert!(score < 1.0);
        assert!(score > 0.5); // But should still be relatively high
    }

    #[test]
    fn test_wilson_sample_size_matters() {
        let ranker = WilsonRanker::new(0.95);
        // Same proportion, different sample sizes
        let small = ranker.wilson_score(8, 10, 0.95);
        let large = ranker.wilson_score(80, 100, 0.95);

        // Larger sample should give higher confidence (higher lower bound)
        assert!(large > small);
    }

    #[test]
    fn test_wilson_batch() {
        let ranker = WilsonRanker::new(0.95);
        let data = vec![(100, 100), (50, 100), (0, 100)];
        let scores = ranker.wilson_scores_batch(&data, 0.95);

        assert_eq!(scores.len(), 3);
        assert!(scores[0] > scores[1]);
        assert!(scores[1] > scores[2]);
    }

    #[test]
    fn test_rank_by_wilson() {
        let data = vec![
            (50, 100),  // 50% - index 0
            (100, 100), // 100% - index 1
            (25, 100),  // 25% - index 2
        ];
        let ranking = rank_by_wilson(&data, 0.95);

        // Should be sorted: 100% first, then 50%, then 25%
        assert_eq!(ranking[0], 1);
        assert_eq!(ranking[1], 0);
        assert_eq!(ranking[2], 2);
    }

    #[test]
    fn test_wilson_bounds() {
        let ranker = WilsonRanker::new(0.95);

        for successes in [0, 5, 10, 50, 100] {
            let score = ranker.wilson_score(successes, 100, 0.95);
            assert!(score >= 0.0, "Score should be >= 0");
            assert!(score <= 1.0, "Score should be <= 1");
        }
    }
}
