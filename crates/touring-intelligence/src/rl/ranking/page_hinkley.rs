//! Page-Hinkley test for online concept drift detection.
//!
//! O(1) per update -- faster than KS test (O(n log n)) for sequential data.
//! Detects abrupt mean shifts in reward/latency streams.
//!
//! Algorithm:
//! m_t = x_t - mu_hat_t - delta
//! M_t = M_{t-1} + m_t
//! drift: M_t - min_{i<=t}(M_i) > lambda
//!
//! Page, E.S. (1954). Biometrika 41:100-115.

/// Page-Hinkley drift detector.
#[derive(Debug, Clone)]
pub struct PageHinkley {
    cumsum: f64,
    min_cumsum: f64,
    running_mean: f64,
    n: u64,
    threshold: f64,
    delta: f64,
    drift_count: u64,
}

impl PageHinkley {
    /// Construct a `PageHinkley` detector with the given drift `threshold` (lambda) and `delta` slack.
    pub fn new(threshold: f64, delta: f64) -> Self {
        Self {
            cumsum: 0.0,
            min_cumsum: 0.0,
            running_mean: 0.0,
            n: 0,
            threshold: threshold.max(0.0),
            delta: delta.max(0.0),
            drift_count: 0,
        }
    }

    /// Default parameters: threshold=50.0, delta=0.005 (for `[0,1]` range streams).
    pub fn with_defaults() -> Self {
        Self::new(50.0, 0.005)
    }

    /// Sensitive detector: threshold=10.0, delta=0.001.
    pub fn sensitive() -> Self {
        Self::new(10.0, 0.001)
    }

    /// Update with a new observation. Returns `true` if drift detected.
    pub fn update(&mut self, x: f64) -> bool {
        self.n += 1;
        // Welford online mean
        self.running_mean += (x - self.running_mean) / self.n as f64;
        let m_t = x - self.running_mean - self.delta;
        self.cumsum += m_t;
        if self.cumsum < self.min_cumsum {
            self.min_cumsum = self.cumsum;
        }
        if self.cumsum - self.min_cumsum > self.threshold {
            self.drift_count += 1;
            true
        } else {
            false
        }
    }

    /// Warm reset: clears sums but preserves running_mean.
    pub fn reset(&mut self) {
        self.cumsum = 0.0;
        self.min_cumsum = 0.0;
    }

    /// Cold reset: full reset including running_mean.
    pub fn reset_cold(&mut self) {
        self.cumsum = 0.0;
        self.min_cumsum = 0.0;
        self.running_mean = 0.0;
        self.n = 0;
    }

    /// Current Page-Hinkley statistic (`cumsum - min_cumsum`).
    pub fn statistic(&self) -> f64 {
        self.cumsum - self.min_cumsum
    }
    /// Total number of drift events detected so far.
    pub fn drift_count(&self) -> u64 {
        self.drift_count
    }
    /// Current Welford running mean of observations.
    pub fn running_mean(&self) -> f64 {
        self.running_mean
    }
    /// Number of observations processed.
    pub fn n(&self) -> u64 {
        self.n
    }
}

impl Default for PageHinkley {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_stream_no_drift() {
        let mut ph = PageHinkley::new(50.0, 0.005);
        for _ in 0..100 {
            assert!(!ph.update(0.5));
        }
    }

    #[test]
    fn detects_abrupt_shift() {
        let mut ph = PageHinkley::sensitive();
        for _ in 0..50 {
            ph.update(0.1);
        }
        let mut detected = false;
        for _ in 0..50 {
            if ph.update(0.9) {
                detected = true;
                break;
            }
        }
        assert!(detected, "Should detect shift 0.1->0.9");
    }

    #[test]
    fn reset_clears_statistic() {
        let mut ph = PageHinkley::new(5.0, 0.001);
        for _ in 0..30 {
            ph.update(0.9);
        }
        ph.reset();
        assert!(ph.statistic() < 1e-10);
    }

    #[test]
    fn drift_count_increments() {
        let mut ph = PageHinkley::new(5.0, 0.001);
        for i in 0..100 {
            if ph.update(if i < 50 { 0.1_f64 } else { 0.9 }) {
                ph.reset();
            }
        }
        assert!(ph.drift_count() >= 1);
    }

    #[test]
    fn running_mean_converges() {
        let mut ph = PageHinkley::with_defaults();
        for _ in 0..1000 {
            ph.update(0.7);
        }
        assert!((ph.running_mean() - 0.7).abs() < 0.01);
    }
}
