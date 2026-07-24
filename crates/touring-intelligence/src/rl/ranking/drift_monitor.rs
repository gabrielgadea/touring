//! Kolmogorov-Smirnov two-sample drift detector.
//!
//! Monitors distributions of performance metrics (latencies, rewards, success rates)
//! and detects when the current distribution significantly differs from a reference
//! distribution using the KS two-sample test.
//!
//! # Algorithm
//!
//! The KS statistic D = max|F1(x) - F2(x)| over all observed values, where F1 and F2
//! are the empirical CDFs of the reference and current samples. The p-value is approximated
//! via the Kolmogorov distribution using the asymptotic formula.

use std::collections::VecDeque;

/// Report produced by a KS two-sample test.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KsDriftReport {
    /// KS statistic (0..=1).
    pub ks_statistic: f64,
    /// Approximate p-value from the Kolmogorov distribution.
    pub p_value: f64,
    /// Whether drift was detected (p_value < alpha).
    pub drift_detected: bool,
    /// Number of samples in the reference window.
    pub reference_size: usize,
    /// Number of samples in the current window.
    pub current_size: usize,
    /// Significance level used for the test.
    pub alpha: f64,
}

/// Backward-compatible alias for [`KsDriftReport`].
pub type DriftReport = KsDriftReport;

/// Kolmogorov-Smirnov two-sample drift detector.
///
/// Maintains a reference (baseline) distribution and a current (live) distribution.
/// When [`test`] is called, it computes the KS statistic and approximate p-value
/// to determine whether the two distributions differ significantly.
pub struct DriftMonitor {
    /// Reference distribution (baseline).
    reference_window: VecDeque<f64>,
    /// Current (live) distribution.
    current_window: VecDeque<f64>,
    /// Maximum window size for both windows.
    window_size: usize,
    /// Significance level (default: 0.05).
    alpha: f64,
}

impl std::fmt::Debug for DriftMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriftMonitor")
            .field("window_size", &self.window_size)
            .field("alpha", &self.alpha)
            .field("reference_len", &self.reference_window.len())
            .field("current_len", &self.current_window.len())
            .finish()
    }
}

impl DriftMonitor {
    /// Create a new drift monitor with custom window size and significance level.
    #[must_use]
    pub fn new(window_size: usize, alpha: f64) -> Self {
        Self {
            reference_window: VecDeque::with_capacity(window_size),
            current_window: VecDeque::with_capacity(window_size),
            window_size,
            alpha,
        }
    }

    /// Create a drift monitor with default parameters (window=100, alpha=0.05).
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(100, 0.05)
    }

    /// Add an observation to the current window.
    ///
    /// If the window exceeds `window_size`, the oldest value is dropped.
    pub fn observe(&mut self, value: f64) {
        self.current_window.push_back(value);
        if self.current_window.len() > self.window_size {
            self.current_window.pop_front();
        }
    }

    /// Set the reference distribution (baseline) from a slice of values.
    ///
    /// If more values are provided than `window_size`, only the last `window_size` are kept.
    pub fn set_reference(&mut self, values: &[f64]) {
        self.reference_window.clear();
        let start = values.len().saturating_sub(self.window_size);
        for &v in values.iter().skip(start) {
            self.reference_window.push_back(v);
        }
    }

    /// Promote the current window to become the new reference, clearing current.
    pub fn promote_current_to_reference(&mut self) {
        self.reference_window = self.current_window.clone();
        self.current_window.clear();
    }

    /// Run the KS two-sample test between reference and current windows.
    ///
    /// Returns `None` if either window has fewer than 5 samples (insufficient data).
    #[must_use]
    pub fn test(&self) -> Option<DriftReport> {
        const MIN_SAMPLES: usize = 5;
        if self.reference_window.len() < MIN_SAMPLES || self.current_window.len() < MIN_SAMPLES {
            return None;
        }

        let ref_slice: Vec<f64> = self.reference_window.iter().copied().collect();
        let cur_slice: Vec<f64> = self.current_window.iter().copied().collect();

        let ks_stat = ks_statistic(&ref_slice, &cur_slice);
        let p_val = ks_p_value(ks_stat, ref_slice.len(), cur_slice.len());

        Some(DriftReport {
            ks_statistic: ks_stat,
            p_value: p_val,
            drift_detected: p_val < self.alpha,
            reference_size: ref_slice.len(),
            current_size: cur_slice.len(),
            alpha: self.alpha,
        })
    }

    /// Quick check: is drift currently detected?
    ///
    /// Returns `false` if insufficient data (same as no drift).
    #[must_use]
    pub fn is_drifting(&self) -> bool {
        self.test().is_some_and(|r| r.drift_detected)
    }

    /// Get the current window size configuration.
    #[must_use]
    pub fn window_size(&self) -> usize {
        self.window_size
    }

    /// Get the significance level.
    #[must_use]
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Get the number of samples in the reference window.
    #[must_use]
    pub fn reference_len(&self) -> usize {
        self.reference_window.len()
    }

    /// Get the number of samples in the current window.
    #[must_use]
    pub fn current_len(&self) -> usize {
        self.current_window.len()
    }
}

impl Default for DriftMonitor {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Compute the two-sample KS statistic: D = max|F1(x) - F2(x)|.
///
/// Both samples must be non-empty. Returns 0.0 if either is empty.
fn ks_statistic(sample1: &[f64], sample2: &[f64]) -> f64 {
    if sample1.is_empty() || sample2.is_empty() {
        return 0.0;
    }

    // Merge and sort all unique evaluation points
    let mut combined: Vec<f64> = Vec::with_capacity(sample1.len() + sample2.len());
    combined.extend_from_slice(sample1);
    combined.extend_from_slice(sample2);
    combined.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    combined.dedup();

    let n1 = sample1.len() as f64;
    let n2 = sample2.len() as f64;

    combined.iter().fold(0.0_f64, |max_d, &x| {
        let f1 = sample1.iter().filter(|&&v| v <= x).count() as f64 / n1;
        let f2 = sample2.iter().filter(|&&v| v <= x).count() as f64 / n2;
        max_d.max((f1 - f2).abs())
    })
}

/// Approximate p-value for the KS statistic using the Kolmogorov asymptotic distribution.
///
/// Uses the series expansion: P(D > d) ~ 2 * sum_{k=1..20} (-1)^{k+1} exp(-2 k^2 lambda^2)
/// where lambda = (sqrt(n_eff) + 0.12 + 0.11/sqrt(n_eff)) * D.
fn ks_p_value(ks_stat: f64, n1: usize, n2: usize) -> f64 {
    if n1 == 0 || n2 == 0 || ks_stat <= 0.0 {
        return 1.0;
    }

    let n_eff = (n1 * n2) as f64 / (n1 + n2) as f64;
    let n_eff_sqrt = n_eff.sqrt();
    let lambda = (n_eff_sqrt + 0.12 + 0.11 / n_eff_sqrt) * ks_stat;

    // Truncate series at k=20 for accuracy
    let mut p = 0.0_f64;
    for k in 1..=20_i32 {
        let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
        let k_f64 = f64::from(k);
        p += sign * (-2.0 * k_f64.powi(2) * lambda.powi(2)).exp();
    }
    (2.0 * p).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drift_monitor_new() {
        let dm = DriftMonitor::new(50, 0.01);
        assert_eq!(dm.window_size(), 50);
        assert!((dm.alpha() - 0.01).abs() < f64::EPSILON);
        assert_eq!(dm.reference_len(), 0);
        assert_eq!(dm.current_len(), 0);
    }

    #[test]
    fn test_drift_monitor_defaults() {
        let dm = DriftMonitor::with_defaults();
        assert_eq!(dm.window_size(), 100);
        assert!((dm.alpha() - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_observe_fills_window() {
        let mut dm = DriftMonitor::new(10, 0.05);
        for i in 0..15 {
            dm.observe(i as f64);
        }
        // Window should cap at 10
        assert_eq!(dm.current_len(), 10);
    }

    #[test]
    fn test_set_reference() {
        let mut dm = DriftMonitor::new(5, 0.05);
        dm.set_reference(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
        // Only last 5 should be kept
        assert_eq!(dm.reference_len(), 5);
    }

    #[test]
    fn test_ks_statistic_identical_distributions() {
        let a: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let b: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let ks = ks_statistic(&a, &b);
        assert!(
            ks < 0.01,
            "Identical distributions should have KS ~ 0, got {ks}"
        );
    }

    #[test]
    fn test_ks_statistic_completely_different_distributions() {
        let a: Vec<f64> = (0..50).map(|i| i as f64).collect();
        let b: Vec<f64> = (100..150).map(|i| i as f64).collect();
        let ks = ks_statistic(&a, &b);
        assert!(
            ks > 0.9,
            "Completely separated distributions should have KS ~ 1.0, got {ks}"
        );
    }

    #[test]
    fn test_drift_detected_when_distributions_differ() {
        let mut dm = DriftMonitor::new(100, 0.05);
        // Reference: values around 10
        let ref_data: Vec<f64> = (0..50).map(|i| 10.0 + (i % 5) as f64).collect();
        dm.set_reference(&ref_data);
        // Current: values around 200
        for i in 0..50 {
            dm.observe(200.0 + (i % 5) as f64);
        }
        let report = dm.test().expect("should have enough data");
        assert!(
            report.drift_detected,
            "Should detect drift: ks={}, p={}",
            report.ks_statistic, report.p_value
        );
        assert!(report.ks_statistic > 0.5);
        assert!(report.p_value < 0.05);
    }

    #[test]
    fn test_no_drift_when_distributions_same() {
        let mut dm = DriftMonitor::new(100, 0.05);
        // Reference and current: same distribution
        let data: Vec<f64> = (0..50).map(|i| 50.0 + (i % 10) as f64).collect();
        dm.set_reference(&data);
        for &v in &data {
            dm.observe(v);
        }
        let report = dm.test().expect("should have enough data");
        assert!(
            !report.drift_detected,
            "Should NOT detect drift for same data: ks={}, p={}",
            report.ks_statistic, report.p_value
        );
    }

    #[test]
    fn test_promote_current_to_reference() {
        let mut dm = DriftMonitor::new(100, 0.05);
        for i in 0..20 {
            dm.observe(i as f64);
        }
        assert_eq!(dm.reference_len(), 0);
        assert_eq!(dm.current_len(), 20);
        dm.promote_current_to_reference();
        assert_eq!(dm.reference_len(), 20);
        assert_eq!(dm.current_len(), 0);
    }

    #[test]
    fn test_test_returns_none_when_insufficient_data() {
        let mut dm = DriftMonitor::new(100, 0.05);
        // Less than MIN_SAMPLES (5) in both windows
        dm.set_reference(&[1.0, 2.0, 3.0]);
        dm.observe(4.0);
        assert!(dm.test().is_none());
    }

    #[test]
    fn test_p_value_range_is_0_to_1() {
        // Test with various configurations
        for n in [10, 50, 100] {
            let a: Vec<f64> = (0..n).map(|i| i as f64).collect();
            let b: Vec<f64> = (0..n).map(|i| (i * 3) as f64).collect();
            let ks = ks_statistic(&a, &b);
            let p = ks_p_value(ks, n as usize, n as usize);
            assert!(
                (0.0..=1.0).contains(&p),
                "p-value should be in [0,1], got {p} for n={n}"
            );
        }
    }

    #[test]
    fn test_drift_report_serialization() {
        let report = DriftReport {
            ks_statistic: 0.42,
            p_value: 0.003,
            drift_detected: true,
            reference_size: 100,
            current_size: 100,
            alpha: 0.05,
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let deser: DriftReport = serde_json::from_str(&json).expect("deserialize");
        assert!((deser.ks_statistic - 0.42).abs() < f64::EPSILON);
        assert!(deser.drift_detected);
        assert_eq!(deser.reference_size, 100);
    }

    #[test]
    fn test_is_drifting_shortcut() {
        let mut dm = DriftMonitor::new(100, 0.05);
        // No data → not drifting
        assert!(!dm.is_drifting());
        // Same data → not drifting
        let data: Vec<f64> = (0..30).map(|i| i as f64).collect();
        dm.set_reference(&data);
        for &v in &data {
            dm.observe(v);
        }
        assert!(!dm.is_drifting());
    }

    #[test]
    fn test_ks_statistic_empty_sample() {
        assert!((ks_statistic(&[], &[1.0, 2.0])).abs() < f64::EPSILON);
        assert!((ks_statistic(&[1.0], &[])).abs() < f64::EPSILON);
    }

    #[test]
    fn test_ks_p_value_edge_cases() {
        // Zero statistic → p-value = 1.0
        assert!((ks_p_value(0.0, 100, 100) - 1.0).abs() < f64::EPSILON);
        // Empty samples → p-value = 1.0
        assert!((ks_p_value(0.5, 0, 100) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_trait() {
        let dm = DriftMonitor::default();
        assert_eq!(dm.window_size(), 100);
        assert!((dm.alpha() - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_debug_format() {
        let dm = DriftMonitor::new(50, 0.01);
        let debug = format!("{dm:?}");
        assert!(debug.contains("DriftMonitor"));
        assert!(debug.contains("50"));
    }
}
