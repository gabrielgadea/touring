//! PyO3 bindings for touring-simd — SIMD-accelerated similarity and statistics.
//!
//! Wraps `touring_simd` types with PyO3 attributes to expose them to Python.
//! Provides cosine similarity, Wilson score ranking, and drift detection.

use pyo3::prelude::*;

use touring_simd::similarity::{CosineComputer, CosineSimilarity};
use touring_simd::statistics::{
    DriftDetection, DriftDetector, StatisticalRanking, WilsonRanker, has_significant_drift,
    rank_by_wilson,
    reconciliation::{
        bayesian_fusion, coefficient_of_variation, reconcile_weighted, weighted_mean,
    },
};

// ═══════════════════════════════════════════════════════════════════════════
// A. Cosine Similarity
// ═══════════════════════════════════════════════════════════════════════════

/// SIMD-accelerated cosine similarity computer.
///
/// Wraps `touring_simd::CosineComputer` with optional parallel threshold.
#[pyclass(name = "CosineComputer")]
#[derive(Debug)]
pub struct PyCosineComputer {
    inner: CosineComputer,
}

#[pymethods]
impl PyCosineComputer {
    /// Create a new CosineComputer.
    ///
    /// Args:
    ///     parallel_threshold: Minimum batch size for parallel processing (default: 100).
    #[new]
    #[pyo3(signature = (parallel_threshold=100))]
    fn new(parallel_threshold: usize) -> Self {
        Self {
            inner: CosineComputer::with_threshold(parallel_threshold),
        }
    }

    /// Compute cosine similarity between two vectors.
    ///
    /// Returns value in [-1, 1] where 1 = same direction.
    /// Returns 0.0 for dimension mismatch or zero vectors.
    fn cosine(&self, a: Vec<f32>, b: Vec<f32>) -> f64 {
        self.inner.cosine(&a, &b)
    }

    /// Batch cosine similarity: all queries x all candidates.
    ///
    /// Returns flattened Vec of length `len(queries) * len(candidates)`.
    /// Uses parallel processing if batch exceeds the parallel threshold.
    fn cosine_batch(
        &self,
        py: Python<'_>,
        queries: Vec<Vec<f32>>,
        candidates: Vec<Vec<f32>>,
    ) -> Vec<f64> {
        py.detach(|| self.inner.cosine_batch(&queries, &candidates))
    }
}

/// Convenience function: compute cosine similarity between two vectors.
#[pyfunction]
pub fn py_cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> f64 {
    let computer = CosineComputer::new();
    computer.cosine(&a, &b)
}

// ═══════════════════════════════════════════════════════════════════════════
// B. Wilson Ranker
// ═══════════════════════════════════════════════════════════════════════════

/// Wilson score ranker with configurable confidence level.
///
/// Uses the Wilson score interval lower bound formula to rank items
/// with uncertain ratings, accounting for small sample sizes.
#[pyclass(name = "WilsonRanker")]
#[derive(Debug)]
pub struct PyWilsonRanker {
    inner: WilsonRanker,
}

#[pymethods]
impl PyWilsonRanker {
    /// Create a new WilsonRanker.
    ///
    /// Args:
    ///     confidence: Confidence level (e.g. 0.95 for 95%). Default: 0.95.
    #[new]
    #[pyo3(signature = (confidence=0.95))]
    fn new(confidence: f64) -> Self {
        Self {
            inner: WilsonRanker::new(confidence),
        }
    }

    /// Compute Wilson score lower bound for a single item.
    ///
    /// Args:
    ///     successes: Number of successes.
    ///     total: Total number of trials.
    ///
    /// Returns score in [0.0, 1.0].
    fn wilson_score(&self, successes: u32, total: u32) -> f64 {
        // confidence param is baked into z_score at construction
        self.inner.wilson_score(successes, total, 0.0)
    }

    /// Batch computation of Wilson scores.
    ///
    /// Args:
    ///     data: List of (successes, total) tuples.
    ///
    /// Returns list of scores.
    fn wilson_scores_batch(&self, py: Python<'_>, data: Vec<(u32, u32)>) -> Vec<f64> {
        py.detach(|| self.inner.wilson_scores_batch(&data, 0.0))
    }
}

/// Rank items by Wilson score, returning indices sorted by score descending.
///
/// Args:
///     data: List of (successes, total) tuples.
///     confidence: Confidence level (e.g. 0.95 for 95%).
///
/// Returns list of indices sorted by Wilson score (highest first).
#[pyfunction]
pub fn py_rank_by_wilson(data: Vec<(u32, u32)>, confidence: f64) -> Vec<usize> {
    rank_by_wilson(&data, confidence)
}

// ═══════════════════════════════════════════════════════════════════════════
// C. Drift Detector
// ═══════════════════════════════════════════════════════════════════════════

/// Distribution drift detector using KS statistic and JS divergence.
///
/// Wraps `touring_simd::DriftDetector` for concept drift detection.
#[pyclass(name = "DriftDetector")]
#[derive(Debug)]
pub struct PyDriftDetector {
    inner: DriftDetector,
}

#[pymethods]
impl PyDriftDetector {
    /// Create a new DriftDetector.
    #[new]
    fn new() -> Self {
        Self {
            inner: DriftDetector::new(),
        }
    }

    /// Compute Kolmogorov-Smirnov statistic between two samples.
    ///
    /// Returns the maximum distance between empirical CDFs.
    /// Returns 0.0 for empty samples.
    fn ks_statistic(&self, py: Python<'_>, sample1: Vec<f64>, sample2: Vec<f64>) -> f64 {
        py.detach(|| self.inner.ks_statistic(&sample1, &sample2))
    }

    /// Compute Jensen-Shannon divergence between two distributions.
    ///
    /// Returns value in [0, 1] where 0 = identical distributions.
    /// Distributions must have the same length.
    fn js_divergence(&self, py: Python<'_>, dist1: Vec<f64>, dist2: Vec<f64>) -> f64 {
        py.detach(|| self.inner.js_divergence(&dist1, &dist2))
    }
}

/// Check if a drift statistic exceeds a threshold.
///
/// Simple convenience function: returns `statistic > threshold`.
#[pyfunction]
pub fn py_has_significant_drift(statistic: f64, threshold: f64) -> bool {
    has_significant_drift(statistic, threshold)
}

// ═══════════════════════════════════════════════════════════════════════════
// D. Reconciliation (weighted mean, CV, Bayesian fusion)
// ═══════════════════════════════════════════════════════════════════════════

/// Compute weighted arithmetic mean of values with corresponding weights.
///
/// Args:
///     values: List of numeric values.
///     weights: List of weights (same length as values).
///
/// Returns:
///     Weighted mean. Returns 0.0 if inputs are empty or weights sum to zero.
#[pyfunction]
pub fn py_weighted_mean(values: Vec<f64>, weights: Vec<f64>) -> f64 {
    weighted_mean(&values, &weights)
}

/// Compute coefficient of variation (population stdev / |mean|).
///
/// Args:
///     values: List of numeric values.
///
/// Returns:
///     CV as a float. Returns 0.0 for fewer than 2 values or zero mean.
#[pyfunction]
pub fn py_coefficient_of_variation(values: Vec<f64>) -> f64 {
    coefficient_of_variation(&values)
}

/// Bayesian fusion of (value, confidence) pairs.
///
/// Higher-confidence estimates get proportionally more weight.
/// Combined confidence is ``1 - product(1 - c_i)``.
///
/// Args:
///     estimates: List of (value, confidence) tuples.
///
/// Returns:
///     Tuple of (fused_value, fused_confidence).
#[pyfunction]
pub fn py_bayesian_fusion(estimates: Vec<(f64, f64)>) -> (f64, f64) {
    bayesian_fusion(&estimates)
}

/// Reconcile multiple scalar values using weighted mean + CV.
///
/// Convenience wrapper that mirrors ``MultiValueReconciler.reconcile_scalar()``.
///
/// Args:
///     values: Numeric values from different sources.
///     weights: Confidence weight per value.
///
/// Returns:
///     Tuple of (reconciled_value, coefficient_of_variation).
#[pyfunction]
pub fn py_reconcile_weighted(values: Vec<f64>, weights: Vec<f64>) -> (f64, f64) {
    reconcile_weighted(&values, &weights)
}

// ═══════════════════════════════════════════════════════════════════════════
// E. Financial Calculations
// ═══════════════════════════════════════════════════════════════════════════

/// Compute Net Present Value for a cash flow series.
///
/// NPV = sum(CF_t / (1+r)^t) for t = 0..n
///
/// Args:
///     cash_flows: List of cash flows (index 0 = period 0).
///     discount_rate: Discount rate as decimal (e.g. 0.10 for 10%).
///
/// Returns:
///     The net present value.
#[pyfunction]
fn py_npv(cash_flows: Vec<f64>, discount_rate: f64) -> f64 {
    touring_simd::financial::npv(&cash_flows, discount_rate)
}

/// Compute NPV for multiple discount rates (batch sensitivity analysis).
///
/// Args:
///     cash_flows: List of cash flows.
///     discount_rates: List of discount rates to evaluate.
///
/// Returns:
///     List of NPV values, one per discount rate.
#[pyfunction]
fn py_npv_batch(cash_flows: Vec<f64>, discount_rates: Vec<f64>) -> Vec<f64> {
    touring_simd::financial::npv_batch(&cash_flows, &discount_rates)
}

/// Compute Internal Rate of Return using Newton-Raphson method.
///
/// Args:
///     cash_flows: List of cash flows.
///     tolerance: Convergence tolerance (e.g. 1e-6).
///     max_iterations: Maximum Newton-Raphson iterations.
///
/// Returns:
///     The IRR as a float, or None if it did not converge.
#[pyfunction]
fn py_irr(cash_flows: Vec<f64>, tolerance: f64, max_iterations: u32) -> Option<f64> {
    touring_simd::financial::irr(&cash_flows, tolerance, max_iterations)
}

/// Run multiple stress scenarios on a base cash flow series.
///
/// Each scenario applies a demand factor multiplier and computes NPV + IRR.
///
/// Args:
///     base_cash_flows: Base case cash flows.
///     discount_rate: Discount rate for NPV calculation.
///     scenarios: List of (name, demand_factor) tuples.
///
/// Returns:
///     List of (name, demand_factor, npv, irr_or_none) tuples.
#[pyfunction]
fn py_stress_scenarios(
    base_cash_flows: Vec<f64>,
    discount_rate: f64,
    scenarios: Vec<(String, f64)>,
) -> Vec<(String, f64, f64, Option<f64>)> {
    touring_simd::financial::stress_scenarios(&base_cash_flows, discount_rate, &scenarios)
        .into_iter()
        .map(|r| (r.scenario_name, r.demand_factor, r.npv, r.irr))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Module registration
// ═══════════════════════════════════════════════════════════════════════════

/// Register all SIMD PyO3 classes and functions in the parent module
/// under a "simd" submodule.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let simd = PyModule::new(m.py(), "simd")?;

    // Cosine similarity
    simd.add_class::<PyCosineComputer>()?;
    simd.add_function(wrap_pyfunction!(py_cosine_similarity, &simd)?)?;

    // Wilson ranking
    simd.add_class::<PyWilsonRanker>()?;
    simd.add_function(wrap_pyfunction!(py_rank_by_wilson, &simd)?)?;

    // Drift detection
    simd.add_class::<PyDriftDetector>()?;
    simd.add_function(wrap_pyfunction!(py_has_significant_drift, &simd)?)?;

    // Reconciliation (weighted mean, CV, Bayesian fusion)
    simd.add_function(wrap_pyfunction!(py_weighted_mean, &simd)?)?;
    simd.add_function(wrap_pyfunction!(py_coefficient_of_variation, &simd)?)?;
    simd.add_function(wrap_pyfunction!(py_bayesian_fusion, &simd)?)?;
    simd.add_function(wrap_pyfunction!(py_reconcile_weighted, &simd)?)?;

    // Financial calculations
    simd.add_function(wrap_pyfunction!(py_npv, &simd)?)?;
    simd.add_function(wrap_pyfunction!(py_npv_batch, &simd)?)?;
    simd.add_function(wrap_pyfunction!(py_irr, &simd)?)?;
    simd.add_function(wrap_pyfunction!(py_stress_scenarios, &simd)?)?;

    m.add_submodule(&simd)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use touring_simd::similarity::CosineSimilarity;
    use touring_simd::statistics::{DriftDetection, StatisticalRanking};

    #[test]
    fn test_cosine_computer_identical() {
        let computer = CosineComputer::new();
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let sim = computer.cosine(&a, &a);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_computer_orthogonal() {
        let computer = CosineComputer::new();
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = computer.cosine(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_wilson_ranker_perfect() {
        let ranker = WilsonRanker::new(0.95);
        let score = ranker.wilson_score(100, 100, 0.95);
        assert!(score > 0.95);
    }

    #[test]
    fn test_wilson_ranker_empty() {
        let ranker = WilsonRanker::new(0.95);
        let score = ranker.wilson_score(0, 0, 0.95);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_rank_by_wilson_order() {
        let data = vec![(100, 100), (50, 100), (25, 100)];
        let ranking = rank_by_wilson(&data, 0.95);
        assert_eq!(ranking[0], 0); // 100% first
        assert_eq!(ranking[1], 1); // 50% second
        assert_eq!(ranking[2], 2); // 25% third
    }

    #[test]
    fn test_drift_detector_identical() {
        let detector = DriftDetector::new();
        let sample = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ks = detector.ks_statistic(&sample, &sample);
        assert!(ks.abs() < 1e-10);
    }

    #[test]
    fn test_drift_detector_disjoint() {
        let detector = DriftDetector::new();
        let s1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let s2 = vec![6.0, 7.0, 8.0, 9.0, 10.0];
        let ks = detector.ks_statistic(&s1, &s2);
        assert!((ks - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_js_divergence_identical() {
        let detector = DriftDetector::new();
        let dist = vec![0.1, 0.2, 0.3, 0.4];
        let js = detector.js_divergence(&dist, &dist);
        assert!(js.abs() < 1e-10);
    }

    #[test]
    fn test_has_significant_drift_fn() {
        assert!(has_significant_drift(0.5, 0.3));
        assert!(!has_significant_drift(0.2, 0.3));
    }

    // ── Reconciliation ──────────────────────────────────────────────

    #[test]
    fn test_weighted_mean_basic() {
        let result = weighted_mean(&[1.0, 2.0, 3.0], &[0.5, 0.3, 0.2]);
        assert!((result - 1.7).abs() < 1e-10);
    }

    #[test]
    fn test_weighted_mean_empty() {
        assert_eq!(weighted_mean(&[], &[]), 0.0);
    }

    #[test]
    fn test_coefficient_of_variation_uniform() {
        let cv = coefficient_of_variation(&[10.0, 10.0, 10.0]);
        assert!(cv.abs() < 1e-10);
    }

    #[test]
    fn test_coefficient_of_variation_varied() {
        let cv = coefficient_of_variation(&[10.0, 20.0, 30.0]);
        assert!(cv > 0.3);
    }

    #[test]
    fn test_bayesian_fusion_basic() {
        let (val, conf) = bayesian_fusion(&[(10.0, 0.9), (12.0, 0.5)]);
        assert!(val > 10.0 && val < 12.0);
        assert!((conf - 0.95).abs() < 1e-10);
    }

    #[test]
    fn test_reconcile_weighted_basic() {
        let (reconciled, cv) = reconcile_weighted(&[10.0, 10.0, 10.0], &[1.0, 1.0, 1.0]);
        assert!((reconciled - 10.0).abs() < 1e-10);
        assert!(cv.abs() < 1e-10);
    }
}
