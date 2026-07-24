//! Statistics trait definitions.
//!
//! Pln3 Phase 1B - L1: Interface Contracts

/// Statistical ranking with Wilson score interval.
pub trait StatisticalRanking {
    /// Compute Wilson score lower bound.
    ///
    /// Used for ranking items with uncertain ratings.
    /// Returns lower bound of confidence interval.
    fn wilson_score(&self, successes: u32, total: u32, confidence: f64) -> f64;

    /// Batch computation of Wilson scores.
    fn wilson_scores_batch(&self, data: &[(u32, u32)], confidence: f64) -> Vec<f64>;
}

/// Distribution drift detection.
pub trait DriftDetection {
    /// Compute Kolmogorov-Smirnov statistic between two samples.
    ///
    /// Returns the maximum distance between empirical CDFs.
    fn ks_statistic(&self, sample1: &[f64], sample2: &[f64]) -> f64;

    /// Compute Jensen-Shannon divergence between two distributions.
    ///
    /// Returns value in [0, 1] where 0 = identical distributions.
    fn js_divergence(&self, dist1: &[f64], dist2: &[f64]) -> f64;
}
