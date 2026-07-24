//! CUSUM Latency Drift Detector — change-point detection for hook latencies.
//!
//! Implements the Cumulative Sum (CUSUM) algorithm with P99 tracking.
//! Simpler and equally effective as KS+Wilson for latency monitoring.
//!
//! # Algorithm
//!
//! CUSUM tracks cumulative deviations from the mean. When the cumulative
//! sum exceeds a threshold `h`, a change-point is declared.
//!
//! - `S⁺ = max(0, S⁺ + (x - μ) - k)` (upward shift)
//! - `S⁻ = max(0, S⁻ - (x - μ) - k)` (downward shift)
//!
//! Where `k` is half the expected shift magnitude and `h` is the detection threshold.

use std::collections::VecDeque;

/// Signal emitted when drift is detected.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DriftSignal {
    /// No drift detected — latency is stable.
    Normal,
    /// Change detected — mean has shifted significantly.
    ChangeDetected {
        /// Current estimated mean latency (ms).
        mean_ms: u64,
        /// P99 latency (ms).
        p99_ms: u64,
        /// Direction of drift.
        direction: DriftDirection,
    },
}

impl DriftSignal {
    /// Returns `true` if drift was detected (regardless of direction).
    #[must_use]
    pub fn is_drifting(&self) -> bool {
        matches!(self, Self::ChangeDetected { .. })
    }

    /// Returns `true` if latency is degrading (increasing).
    #[must_use]
    pub fn is_degrading(&self) -> bool {
        matches!(
            self,
            Self::ChangeDetected {
                direction: DriftDirection::Increase,
                ..
            }
        )
    }
}

impl std::fmt::Display for DriftSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::ChangeDetected {
                mean_ms,
                p99_ms,
                direction,
            } => {
                write!(f, "drift:{direction} mean={mean_ms}ms p99={p99_ms}ms")
            }
        }
    }
}

/// Direction of the detected drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DriftDirection {
    /// Latency increased (degradation).
    Increase,
    /// Latency decreased (improvement).
    Decrease,
}

impl std::fmt::Display for DriftDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Increase => write!(f, "increase"),
            Self::Decrease => write!(f, "decrease"),
        }
    }
}

/// CUSUM-based latency drift detector with sliding window.
#[derive(Debug, Clone)]
pub struct LatencyDriftDetector {
    /// Sliding window of recent latency samples.
    window: VecDeque<u64>,
    /// Maximum window size.
    window_size: usize,
    /// CUSUM parameter k: half of the expected shift magnitude.
    cusum_k: f64,
    /// CUSUM threshold h: triggers change detection.
    cusum_h: f64,
    /// Cumulative sum for positive shifts (degradation).
    cusum_pos: f64,
    /// Cumulative sum for negative shifts (improvement).
    cusum_neg: f64,
    /// Total samples seen.
    total_samples: u64,
}

impl LatencyDriftDetector {
    /// Create a new detector with default parameters.
    ///
    /// Default: window=100, k=5.0ms (expected half-shift), h=50.0ms (threshold).
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_intelligence::rl::ranking::cusum::{LatencyDriftDetector, DriftSignal, DriftDirection};
    ///
    /// let mut detector = LatencyDriftDetector::new();
    ///
    /// // Feed normal latencies — no drift
    /// for _ in 0..50 {
    ///     let signal = detector.push(45); // ~45ms each
    ///     assert!(!signal.is_drifting());
    /// }
    ///
    /// // Latency spikes — drift detected
    /// let signal = detector.push(200); // sudden spike
    /// assert!(matches!(signal, DriftSignal::ChangeDetected {
    ///     direction: DriftDirection::Increase,
    ///     ..
    /// }));
    /// ```
    pub fn new() -> Self {
        Self::with_params(100, 5.0, 50.0)
    }

    /// Create a detector with custom parameters.
    ///
    /// - `window_size`: number of recent samples to track
    /// - `cusum_k`: half of the expected shift magnitude (in ms)
    /// - `cusum_h`: detection threshold (in ms)
    pub(crate) fn with_params(window_size: usize, cusum_k: f64, cusum_h: f64) -> Self {
        Self {
            window: VecDeque::with_capacity(window_size),
            window_size,
            cusum_k,
            cusum_h,
            cusum_pos: 0.0,
            cusum_neg: 0.0,
            total_samples: 0,
        }
    }

    /// Push a new latency sample and check for drift.
    pub fn push(&mut self, latency_ms: u64) -> DriftSignal {
        self.window.push_back(latency_ms);
        if self.window.len() > self.window_size {
            self.window.pop_front();
        }
        self.total_samples += 1;

        // Need at least 10 samples for meaningful statistics
        if self.window.len() < 10 {
            return DriftSignal::Normal;
        }

        let mean = self.window.iter().sum::<u64>() as f64 / self.window.len() as f64;
        let x = latency_ms as f64 - mean;

        self.cusum_pos = (self.cusum_pos + x - self.cusum_k).max(0.0);
        self.cusum_neg = (self.cusum_neg - x - self.cusum_k).max(0.0);

        if self.cusum_pos > self.cusum_h {
            self.cusum_pos = 0.0; // Reset after detection
            DriftSignal::ChangeDetected {
                mean_ms: mean as u64,
                p99_ms: self.p99(),
                direction: DriftDirection::Increase,
            }
        } else if self.cusum_neg > self.cusum_h {
            self.cusum_neg = 0.0; // Reset after detection
            DriftSignal::ChangeDetected {
                mean_ms: mean as u64,
                p99_ms: self.p99(),
                direction: DriftDirection::Decrease,
            }
        } else {
            DriftSignal::Normal
        }
    }

    /// Compute P99 latency from the current window.
    pub(crate) fn p99(&self) -> u64 {
        if self.window.is_empty() {
            return 0;
        }
        let mut sorted: Vec<u64> = self.window.iter().copied().collect();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64) * 0.99).ceil() as usize;
        // SAFETY: sorted is non-empty (early return above), and idx is clamped to sorted.len()-1.
        #[allow(clippy::indexing_slicing)]
        sorted[idx.min(sorted.len() - 1)]
    }

    /// Compute current mean latency.
    pub fn mean(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        self.window.iter().sum::<u64>() as f64 / self.window.len() as f64
    }

    /// Get total number of samples processed.
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }

    /// Get current window size.
    pub fn current_window_len(&self) -> usize {
        self.window.len()
    }

    /// Reset the detector state (keeps configuration).
    pub fn reset(&mut self) {
        self.window.clear();
        self.cusum_pos = 0.0;
        self.cusum_neg = 0.0;
        self.total_samples = 0;
    }

    /// Peek at the last accumulated CUSUM signal without modifying state.
    ///
    /// Returns the last `DriftSignal` that was produced, or `None` if no
    /// sample has been processed yet.
    ///
    /// Unlike `push()`, this does not mutate the internal CUSUM accumulators.
    pub fn peek_last(&self) -> Option<DriftSignal> {
        if self.total_samples == 0 {
            return None;
        }
        // Reconstruct signal from current accumulator state
        let mean = self.mean();
        if self.cusum_pos > self.cusum_h {
            Some(DriftSignal::ChangeDetected {
                mean_ms: mean as u64,
                p99_ms: self.p99(),
                direction: DriftDirection::Increase,
            })
        } else if self.cusum_neg > self.cusum_h {
            Some(DriftSignal::ChangeDetected {
                mean_ms: mean as u64,
                p99_ms: self.p99(),
                direction: DriftDirection::Decrease,
            })
        } else {
            Some(DriftSignal::Normal)
        }
    }
}

impl Default for LatencyDriftDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_latency_no_drift() {
        let mut detector = LatencyDriftDetector::new();
        // Stable latency around 50ms
        for _ in 0..100 {
            let signal = detector.push(50);
            assert_eq!(signal, DriftSignal::Normal);
        }
        assert_eq!(detector.total_samples(), 100);
    }

    #[test]
    fn test_sudden_increase_detects_drift() {
        let mut detector = LatencyDriftDetector::with_params(50, 5.0, 30.0);
        // Stable at 20ms for 30 samples
        for _ in 0..30 {
            detector.push(20);
        }
        // Sudden spike to 200ms
        let mut detected = false;
        for _ in 0..20 {
            if let DriftSignal::ChangeDetected { direction, .. } = detector.push(200) {
                assert_eq!(direction, DriftDirection::Increase);
                detected = true;
                break;
            }
        }
        assert!(detected, "Should detect upward drift");
    }

    #[test]
    fn test_sudden_decrease_detects_drift() {
        let mut detector = LatencyDriftDetector::with_params(50, 5.0, 30.0);
        // Stable at 200ms for 30 samples
        for _ in 0..30 {
            detector.push(200);
        }
        // Sudden drop to 5ms
        let mut detected = false;
        for _ in 0..20 {
            if let DriftSignal::ChangeDetected { direction, .. } = detector.push(5) {
                assert_eq!(direction, DriftDirection::Decrease);
                detected = true;
                break;
            }
        }
        assert!(detected, "Should detect downward drift");
    }

    #[test]
    fn test_p99_calculation() {
        let mut detector = LatencyDriftDetector::new();
        for i in 1..=100 {
            detector.push(i);
        }
        // P99 of 1..100 should be ~99
        assert!(detector.p99() >= 99);
    }

    #[test]
    fn test_mean_calculation() {
        let mut detector = LatencyDriftDetector::new();
        for _ in 0..10 {
            detector.push(100);
        }
        assert!((detector.mean() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_reset() {
        let mut detector = LatencyDriftDetector::new();
        for _ in 0..50 {
            detector.push(42);
        }
        assert_eq!(detector.total_samples(), 50);
        detector.reset();
        assert_eq!(detector.total_samples(), 0);
        assert_eq!(detector.current_window_len(), 0);
    }

    #[test]
    fn test_insufficient_samples_no_drift() {
        let mut detector = LatencyDriftDetector::new();
        // Less than 10 samples — always Normal
        for _ in 0..9 {
            assert_eq!(detector.push(1000), DriftSignal::Normal);
        }
    }

    #[test]
    fn test_default_params() {
        let detector = LatencyDriftDetector::default();
        assert_eq!(detector.window_size, 100);
        assert_eq!(detector.total_samples(), 0);
    }
}
