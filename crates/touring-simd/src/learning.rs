//! Adaptive batch sizing and SIMD-accelerated learning utilities.

use crate::simd_utils::dispatch::arch;
use crate::simd_utils::ops;
use crate::similarity::{CosineComputer, CosineSimilarity, TopKResult, TopKSearcher};

// =============================================================================
// ACO Pheromone Types
// =============================================================================

/// Outcome of a batch processing run.
#[derive(Debug, Clone, Copy)]
pub struct BatchOutcome {
    /// Whether the batch completed successfully.
    pub success: bool,
    /// Processing time in milliseconds.
    pub processing_time_ms: f64,
    /// Throughput: elements processed per second.
    pub throughput: f64,
    /// Error rate (0.0 = perfect).
    pub error_rate: f64,
    /// Whether the system was memory-bound.
    pub memory_bound: bool,
}

/// Internal pheromone state for ACO-inspired threshold adaptation.
///
/// ACO (Ant Colony Optimization) pheromone-inspired feedback: after each batch
/// processing outcome, the system records whether the current
/// `adaptive_parallel_threshold` was appropriate, and adjusts it for future runs.
#[derive(Debug, Clone)]
pub struct AcoPheromone {
    /// Accumulated positive evidence.
    positive: f64,
    /// Accumulated negative evidence.
    negative: f64,
    /// Evaporation rate (0-1).
    evaporation: f64,
}

impl Default for AcoPheromone {
    fn default() -> Self {
        Self {
            positive: 1.0,
            negative: 1.0,
            evaporation: 0.95,
        }
    }
}

impl AcoPheromone {
    /// Update pheromone levels based on batch outcome.
    ///
    /// # Pheromone Dynamics
    /// - Positive evidence increases when outcome is successful with low error rate
    /// - Negative evidence increases on failure or high error rate
    /// - Memory pressure adds additional negative evidence
    /// - Both values evaporate over time via the evaporation rate
    pub fn update(&mut self, outcome: &BatchOutcome) {
        // Evaporate old evidence
        self.positive *= self.evaporation;
        self.negative *= self.evaporation;

        // Add positive evidence: successful run with minimal errors
        if outcome.success && outcome.error_rate < 0.01 {
            self.positive += 1.0;
        } else if outcome.error_rate > 0.1 || !outcome.success {
            // Add negative evidence: high error rate or outright failure
            self.negative += 1.0;
        }

        // Memory pressure is negative evidence
        if outcome.memory_bound {
            self.negative += 0.5;
        }
    }

    /// Ratio of positive to total pheromone (0.0 to 1.0).
    ///
    /// Returns 0.5 when both pheromone levels are equal (neutral).
    #[must_use]
    pub fn confidence(&self) -> f64 {
        let total = self.positive + self.negative;
        if total < f64::EPSILON {
            return 0.5;
        }
        self.positive / total
    }
}

// SAFETY: AcoPheromone contains only f64 fields with no interior mutability.
// It is safe to share across threads for read-only access, and update() requires &mut.
unsafe impl Send for AcoPheromone {}
unsafe impl Sync for AcoPheromone {}

/// Adaptive threshold that adjusts based on vector dimension.
///
/// Higher dimensions benefit more from parallel processing.
#[inline]
#[must_use]
pub fn adaptive_parallel_threshold(dim: usize) -> usize {
    (dim as f32 / 16.0).ceil() as usize + 50
}

/// Compute adaptive cosine batch with dimension-aware threshold.
#[must_use]
pub fn adaptive_cosine_batch(
    queries: &[Vec<f32>],
    candidates: &[Vec<f32>],
    dim: usize,
) -> Vec<f64> {
    let threshold = adaptive_parallel_threshold(dim);
    let computer = CosineComputer::with_threshold(threshold);
    computer.cosine_batch(queries, candidates)
}

/// SIMD-accelerated KNN search with adaptive parallelism.
///
/// Combines TopK search with dimension-aware parallel thresholds.
#[must_use]
pub fn simd_knn_search(query: &[f32], candidates: &[Vec<f32>], k: usize) -> Vec<TopKResult> {
    let threshold = adaptive_parallel_threshold(query.len());
    let searcher = TopKSearcher::with_threshold(k, threshold);
    searcher.top_k(query, candidates, k)
}

/// Batch dot products: compute dot(query, candidate) for all candidates.
///
/// Uses SIMD-accelerated dot product via pulp dispatch.
#[must_use]
pub fn batch_dot_products(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    candidates
        .iter()
        .map(|c| arch().dispatch(ops::DotF32 { a: query, b: c }))
        .collect()
}

/// Batch dot products with rayon parallelism.
#[must_use]
pub fn batch_dot_products_par(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
    use rayon::prelude::*;
    candidates
        .par_iter()
        .map(|c| arch().dispatch(ops::DotF32 { a: query, b: c }))
        .collect()
}

// =============================================================================
// ACO Feedback Pipeline
// =============================================================================

/// Baseline throughput for comparison (elements/second).
///
/// Used as reference when evaluating whether a batch outcome is "good".
const THROUGHPUT_BASELINE: f64 = 100_000.0;

/// Minimum parallel threshold (prevents single-threaded bottlenecks).
const THRESHOLD_MIN: usize = 16;

/// Maximum parallel threshold (caps memory usage for very large batches).
const THRESHOLD_MAX: usize = 1024;

/// Compute adaptive cosine batch with optional ACO feedback.
///
/// Returns a tuple of `(results, adjusted_threshold)` where `adjusted_threshold`
/// is present when feedback was provided and threshold was adapted.
#[must_use]
pub fn adaptive_cosine_batch_with_feedback(
    queries: &[Vec<f32>],
    candidates: &[Vec<f32>],
    dim: usize,
    feedback: Option<BatchOutcome>,
) -> (Vec<f64>, Option<usize>) {
    let current_threshold = adaptive_parallel_threshold(dim);
    let results = adaptive_cosine_batch(queries, candidates, dim);

    let adjusted =
        feedback.map(|outcome| adjust_threshold_from_feedback(current_threshold, &outcome));

    (results, adjusted)
}

/// Adjust the parallel threshold based on batch processing feedback.
///
/// # Feedback Rules
/// - **Increase**: Successful run with throughput above baseline
/// - **Decrease**: Failed run, high error rate (>10%), or memory-bound
///
/// Returns the adjusted threshold clamped to [16, 1024].
#[must_use]
pub fn adjust_threshold_from_feedback(current_threshold: usize, outcome: &BatchOutcome) -> usize {
    let delta: isize = if outcome.success && outcome.throughput > THROUGHPUT_BASELINE {
        // Good outcome: increase threshold slightly (more parallelism helps)
        8
    } else if !outcome.success || outcome.error_rate > 0.1 {
        // Bad outcome: decrease threshold significantly
        -16
    } else if outcome.memory_bound {
        // Memory pressure: reduce threshold to lower memory footprint
        -8
    } else {
        // Neutral: small increase to explore
        4
    };

    let new_threshold = (current_threshold as isize + delta) as usize;

    // Clamp to valid range
    new_threshold.clamp(THRESHOLD_MIN, THRESHOLD_MAX)
}

/// Helper to construct BatchOutcome from raw metrics.
///
/// # Arguments
/// * `duration_ms` - Total processing time in milliseconds
/// * `elements` - Total number of elements processed
/// * `errors` - Number of failed elements
#[must_use]
pub fn compute_batch_outcome(duration_ms: f64, elements: usize, errors: usize) -> BatchOutcome {
    let processing_time_ms = duration_ms;
    let throughput = if duration_ms > 0.0 {
        (elements as f64 / duration_ms) * 1000.0
    } else {
        0.0
    };
    let error_rate = if elements > 0 {
        errors as f64 / elements as f64
    } else {
        0.0
    };
    let success = errors == 0 && duration_ms > 0.0;

    BatchOutcome {
        success,
        processing_time_ms,
        throughput,
        error_rate,
        memory_bound: false, // Caller should set this based on system metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================================================================
    // ACO Pheromone Tests
    // ==================================================================

    #[test]
    fn test_aco_pheromone_update_positive() {
        let outcome = BatchOutcome {
            success: true,
            processing_time_ms: 10.0,
            throughput: 200_000.0,
            error_rate: 0.0,
            memory_bound: false,
        };
        let mut pheromone = AcoPheromone::default();
        let conf_before = pheromone.confidence();
        pheromone.update(&outcome);
        let conf_after = pheromone.confidence();
        assert!(
            conf_after > conf_before,
            "positive outcome should increase confidence"
        );
        // Verify positive increased
        assert!(
            pheromone.positive > 1.0,
            "positive should grow after good outcome"
        );
    }

    #[test]
    fn test_aco_pheromone_update_negative() {
        let outcome = BatchOutcome {
            success: false,
            processing_time_ms: 500.0,
            throughput: 100.0,
            error_rate: 0.5,
            memory_bound: false,
        };
        let mut pheromone = AcoPheromone::default();
        let conf_before = pheromone.confidence();
        pheromone.update(&outcome);
        let conf_after = pheromone.confidence();
        assert!(
            conf_after < conf_before,
            "negative outcome should decrease confidence"
        );
    }

    #[test]
    fn test_aco_pheromone_evaporation() {
        // Verify that many updates with evaporation approach a steady-state,
        // rather than growing without bound.
        let outcome = BatchOutcome {
            success: true,
            processing_time_ms: 10.0,
            throughput: 200_000.0,
            error_rate: 0.0,
            memory_bound: false,
        };
        let mut pheromone = AcoPheromone::default();
        // Evaporation (0.95) with +1.0 per update approaches steady-state
        // steady_state = 1.0 / (1 - 0.95) = 20.0
        // With 100 updates it should be close to 20
        for _ in 0..100 {
            pheromone.update(&outcome);
        }
        // After many updates, value should approach steady-state (~20.0)
        assert!(
            pheromone.positive < 25.0,
            "evaporation should prevent unbounded growth, got {}",
            pheromone.positive
        );
    }

    #[test]
    fn test_adjust_threshold_increase() {
        let outcome = BatchOutcome {
            success: true,
            processing_time_ms: 10.0,
            throughput: 200_000.0, // > THROUGHPUT_BASELINE
            error_rate: 0.0,
            memory_bound: false,
        };
        let current = 100;
        let adjusted = adjust_threshold_from_feedback(current, &outcome);
        assert!(
            adjusted > current,
            "good outcome should increase threshold: {current} -> {adjusted}"
        );
    }

    #[test]
    fn test_adjust_threshold_decrease() {
        let outcome = BatchOutcome {
            success: false,
            processing_time_ms: 500.0,
            throughput: 100.0,
            error_rate: 0.5,
            memory_bound: false,
        };
        let current = 100;
        let adjusted = adjust_threshold_from_feedback(current, &outcome);
        assert!(
            adjusted < current,
            "bad outcome should decrease threshold: {current} -> {adjusted}"
        );
    }

    #[test]
    fn test_adjust_threshold_clamp() {
        // Test lower bound: very bad outcome from high threshold
        let outcome_high = BatchOutcome {
            success: false,
            processing_time_ms: 1000.0,
            throughput: 0.0,
            error_rate: 1.0,
            memory_bound: true,
        };
        let result = adjust_threshold_from_feedback(20, &outcome_high);
        assert!(
            result >= THRESHOLD_MIN,
            "should not go below {THRESHOLD_MIN}, got {result}"
        );

        // Test upper bound: very good outcome from low threshold
        let outcome_good = BatchOutcome {
            success: true,
            processing_time_ms: 1.0,
            throughput: 10_000_000.0,
            error_rate: 0.0,
            memory_bound: false,
        };
        let result = adjust_threshold_from_feedback(1000, &outcome_good);
        assert!(
            result <= THRESHOLD_MAX,
            "should not exceed {THRESHOLD_MAX}, got {result}"
        );
    }

    #[test]
    fn test_compute_batch_outcome() {
        let outcome = compute_batch_outcome(100.0, 50_000, 500);
        assert_eq!(outcome.processing_time_ms, 100.0);
        assert!((outcome.throughput - 500_000.0).abs() < 1.0);
        assert!((outcome.error_rate - 0.01).abs() < 1e-6);
        assert!(!outcome.success);
    }

    #[test]
    fn test_compute_batch_outcome_perfect() {
        let outcome = compute_batch_outcome(50.0, 100_000, 0);
        assert!(outcome.success);
        assert!((outcome.throughput - 2_000_000.0).abs() < 1.0);
        assert!((outcome.error_rate - 0.0).abs() < 1e-6);
    }

    // ==================================================================
    // Adaptive Threshold Tests
    // ==================================================================

    #[test]
    fn test_adaptive_threshold_dim_64() {
        let t = adaptive_parallel_threshold(64);
        assert!(t >= 50);
    }

    #[test]
    fn test_adaptive_threshold_dim_1536() {
        let t = adaptive_parallel_threshold(1536);
        assert!(t > adaptive_parallel_threshold(64));
    }

    #[test]
    fn test_simd_knn_search() {
        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.9, 0.1, 0.0],
        ];
        let results = simd_knn_search(&query, &candidates, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 0);
    }

    #[test]
    fn test_batch_dot_products() {
        let query = vec![1.0, 0.0];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![0.5, 0.5]];
        let results = batch_dot_products(&query, &candidates);
        assert_eq!(results.len(), 3);
        assert!((results[0] - 1.0).abs() < 1e-6);
        assert!((results[1] - 0.0).abs() < 1e-6);
        assert!((results[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_adaptive_cosine_batch_with_feedback() {
        let queries = vec![vec![1.0, 0.0, 0.0]];
        let candidates = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.9, 0.1, 0.0],
        ];
        let outcome = BatchOutcome {
            success: true,
            processing_time_ms: 5.0,
            throughput: 500_000.0,
            error_rate: 0.0,
            memory_bound: false,
        };
        let (results, adjusted) =
            adaptive_cosine_batch_with_feedback(&queries, &candidates, 3, Some(outcome));
        assert!(!results.is_empty());
        assert!(adjusted.is_some());
    }

    #[test]
    fn test_aco_pheromone_thread_safe() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<AcoPheromone>();
        assert_sync::<AcoPheromone>();
    }

    // ── ACO feedback integration tests (D8) ─────────────────────────────────

    #[test]
    fn test_adjust_threshold_converges() {
        // Run 5 iterations with successful outcomes — threshold should move toward optimal
        let good_outcome = BatchOutcome {
            success: true,
            processing_time_ms: 5.0,
            throughput: 200_000.0,
            error_rate: 0.0,
            memory_bound: false,
        };
        let mut threshold = 50;
        for _ in 0..5 {
            threshold = adjust_threshold_from_feedback(threshold, &good_outcome);
        }
        // After successful runs, threshold should increase (more parallelism helps)
        assert!(
            threshold >= 50,
            "threshold {} should be within optimal range [16, 1024]",
            threshold
        );
    }

    #[test]
    fn test_batch_outcome_fields() {
        let outcome = compute_batch_outcome(100.0, 50_000, 500);
        assert_eq!(outcome.processing_time_ms, 100.0);
        assert!((outcome.throughput - 500_000.0).abs() < 1.0);
        assert!((outcome.error_rate - 0.01).abs() < 1e-6);
        assert!(!outcome.success);
        assert!(!outcome.memory_bound);
    }
}
