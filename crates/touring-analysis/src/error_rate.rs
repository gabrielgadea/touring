//! Per-module error rate tracking via rolling window.
//!
//! `ErrorRateTracker` maintains a rolling window of success/failure events
//! per module and emits a `tracing::warn!` when the rate exceeds the
//! configured threshold.
//!
//! # Example
//! ```
//! use touring_analysis::error_rate::ErrorRateTracker;
//!
//! let mut tracker = ErrorRateTracker::new(100, 0.05);
//! tracker.record("wiring", true);   // success
//! tracker.record("wiring", false);  // failure
//! let rate = tracker.error_rate("wiring");
//! assert!(rate >= 0.0 && rate <= 1.0);
//! ```

use std::collections::{HashMap, VecDeque};

/// Rolling-window error rate per module.
///
/// Window size: `window_size` events. Alert threshold: `threshold` (0.0–1.0).
#[derive(Debug)]
pub struct ErrorRateTracker {
    /// Window capacity (max events tracked per module).
    window_size: usize,
    /// Alert threshold: warn when error_rate > this value.
    threshold: f64,
    /// Per-module rolling windows: `true` = success, `false` = failure.
    windows: HashMap<String, VecDeque<bool>>,
}

/// Per-module error rate snapshot.
#[derive(Debug, Clone)]
pub struct ModuleErrorRate {
    /// Module name.
    pub module: String,
    /// Error rate in [0.0, 1.0].
    pub error_rate: f64,
    /// Total events in the current window.
    pub window_len: usize,
    /// Whether the rate exceeds the alert threshold.
    pub alert: bool,
}

impl ErrorRateTracker {
    /// Create a new tracker with the given window size and alert threshold.
    ///
    /// - `window_size`: max events per module (rolling window capacity).
    /// - `threshold`: error rate above which `tracing::warn!` is emitted (0.0–1.0).
    pub fn new(window_size: usize, threshold: f64) -> Self {
        Self {
            window_size: window_size.max(1),
            threshold: threshold.clamp(0.0, 1.0),
            windows: HashMap::new(),
        }
    }

    /// Record an event for a module.
    ///
    /// `success = true` means the operation succeeded; `false` means it failed.
    /// Emits `tracing::warn!` if the error rate exceeds the configured threshold.
    pub fn record(&mut self, module: &str, success: bool) {
        let window = self
            .windows
            .entry(module.to_string())
            .or_insert_with(|| VecDeque::with_capacity(self.window_size));

        if window.len() >= self.window_size {
            window.pop_front();
        }
        window.push_back(success);

        let rate = compute_error_rate(window);
        if rate > self.threshold {
            tracing::warn!(
                module = module,
                error_rate = rate,
                threshold = self.threshold,
                window_len = window.len(),
                "module error rate exceeds threshold"
            );
        }
    }

    /// Return the current error rate for a module (0.0 if no events recorded).
    pub fn error_rate(&self, module: &str) -> f64 {
        match self.windows.get(module) {
            Some(w) => compute_error_rate(w),
            None => 0.0,
        }
    }

    /// Return error rate snapshots for all modules.
    pub fn all_rates(&self) -> Vec<ModuleErrorRate> {
        self.windows
            .iter()
            .map(|(module, window)| {
                let rate = compute_error_rate(window);
                ModuleErrorRate {
                    module: module.clone(),
                    error_rate: rate,
                    window_len: window.len(),
                    alert: rate > self.threshold,
                }
            })
            .collect()
    }

    /// Clear all recorded events.
    pub fn reset(&mut self) {
        self.windows.clear();
    }
}

fn compute_error_rate(window: &VecDeque<bool>) -> f64 {
    if window.is_empty() {
        return 0.0;
    }
    let failures = window.iter().filter(|&&s| !s).count();
    failures as f64 / window.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_module_rate_is_zero() {
        let tracker = ErrorRateTracker::new(100, 0.05);
        assert_eq!(tracker.error_rate("nonexistent"), 0.0);
    }

    #[test]
    fn test_all_success_rate_is_zero() {
        let mut tracker = ErrorRateTracker::new(10, 0.05);
        for _ in 0..5 {
            tracker.record("quality", true);
        }
        assert_eq!(tracker.error_rate("quality"), 0.0);
    }

    #[test]
    fn test_all_failure_rate_is_one() {
        let mut tracker = ErrorRateTracker::new(10, 0.5);
        for _ in 0..5 {
            tracker.record("wiring", false);
        }
        assert!((tracker.error_rate("wiring") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_rolling_window_evicts_oldest() {
        let mut tracker = ErrorRateTracker::new(4, 0.5);
        // Add 4 successes
        for _ in 0..4 {
            tracker.record("analysis", true);
        }
        assert_eq!(tracker.error_rate("analysis"), 0.0);
        // Add 4 failures -- window evicts the successes
        for _ in 0..4 {
            tracker.record("analysis", false);
        }
        assert!((tracker.error_rate("analysis") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_all_rates_returns_all_modules() {
        let mut tracker = ErrorRateTracker::new(10, 0.05);
        tracker.record("wiring", true);
        tracker.record("quality", false);
        let rates = tracker.all_rates();
        assert_eq!(rates.len(), 2);
        let wiring = rates.iter().find(|r| r.module == "wiring").expect("wiring");
        assert_eq!(wiring.error_rate, 0.0);
        let quality = rates
            .iter()
            .find(|r| r.module == "quality")
            .expect("quality");
        assert!((quality.error_rate - 1.0).abs() < 1e-9);
        assert!(quality.alert, "should alert when rate > threshold");
    }

    #[test]
    fn test_reset_clears_all_windows() {
        let mut tracker = ErrorRateTracker::new(10, 0.05);
        tracker.record("wiring", false);
        tracker.reset();
        assert_eq!(tracker.error_rate("wiring"), 0.0);
        assert!(tracker.all_rates().is_empty());
    }
}
