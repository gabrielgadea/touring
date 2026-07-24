//! Adaptive exploration parameter (alpha) for LinUCB.
//!
//! Tracks regret (|expected - actual reward|) over a sliding window and
//! adjusts alpha to minimize long-run regret:
//! - mean_regret > HIGH_REGRET: alpha *= 1.05 (more exploration)
//! - mean_regret < LOW_REGRET:  alpha *= 0.95 (less exploration)
//! - alpha clamped to [ALPHA_MIN, ALPHA_MAX]

use std::collections::VecDeque;

const HIGH_REGRET: f64 = 0.3;
const LOW_REGRET: f64 = 0.05;
const ALPHA_MIN: f64 = 0.1;
const ALPHA_MAX: f64 = 5.0;
const DEFAULT_ALPHA: f64 = 1.0;
const WINDOW_SIZE: usize = 50;

/// Adaptive alpha controller for LinUCB.
#[derive(Debug, Clone)]
pub struct AdaptiveAlpha {
    alpha: f64,
    regret_window: VecDeque<f64>,
    window_size: usize,
    update_count: u64,
    adjustments: u64,
}

impl AdaptiveAlpha {
    /// Construct an `AdaptiveAlpha` with `initial_alpha` (clamped to valid range) and `window_size`.
    pub fn new(initial_alpha: f64, window_size: usize) -> Self {
        Self {
            alpha: initial_alpha.clamp(ALPHA_MIN, ALPHA_MAX),
            regret_window: VecDeque::with_capacity(window_size),
            window_size: window_size.max(1),
            update_count: 0,
            adjustments: 0,
        }
    }

    /// Construct an `AdaptiveAlpha` using the crate's default alpha and window size.
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_ALPHA, WINDOW_SIZE)
    }

    /// Record a pull result and adjust alpha.
    /// Returns the new alpha value.
    pub fn update(&mut self, expected_reward: f64, actual_reward: f64) -> f64 {
        let regret = (expected_reward - actual_reward).abs();
        self.regret_window.push_back(regret);
        if self.regret_window.len() > self.window_size {
            self.regret_window.pop_front();
        }
        self.update_count += 1;
        // Only adjust after warming up
        if self.regret_window.len() < self.window_size / 2 {
            return self.alpha;
        }
        let mean = self.regret_window.iter().sum::<f64>() / self.regret_window.len() as f64;
        if mean > HIGH_REGRET {
            self.alpha = (self.alpha * 1.05).clamp(ALPHA_MIN, ALPHA_MAX);
            self.adjustments += 1;
        } else if mean < LOW_REGRET {
            self.alpha = (self.alpha * 0.95).clamp(ALPHA_MIN, ALPHA_MAX);
            self.adjustments += 1;
        }
        self.alpha
    }

    /// Current exploration parameter `alpha`.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }
    /// Mean regret over the current sliding window (0.0 when empty).
    pub fn mean_regret(&self) -> f64 {
        if self.regret_window.is_empty() {
            0.0
        } else {
            self.regret_window.iter().sum::<f64>() / self.regret_window.len() as f64
        }
    }
    /// Number of times `alpha` has been adjusted up or down.
    pub fn adjustments(&self) -> u64 {
        self.adjustments
    }
    /// Total number of `update` calls recorded.
    pub fn update_count(&self) -> u64 {
        self.update_count
    }
}

impl Default for AdaptiveAlpha {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_increases_with_high_regret() {
        let mut aa = AdaptiveAlpha::new(1.0, 10);
        let init = aa.alpha();
        for _ in 0..15 {
            aa.update(1.0, 0.5);
        }
        assert!(aa.alpha() >= init);
    }

    #[test]
    fn alpha_decreases_with_low_regret() {
        let mut aa = AdaptiveAlpha::new(1.0, 10);
        let init = aa.alpha();
        for _ in 0..15 {
            aa.update(0.8, 0.79);
        }
        assert!(aa.alpha() <= init);
    }

    #[test]
    fn alpha_clamped() {
        let mut aa = AdaptiveAlpha::new(ALPHA_MAX, 5);
        for _ in 0..20 {
            aa.update(1.0, 0.0);
        }
        assert!(aa.alpha() <= ALPHA_MAX);
        assert!(aa.alpha() >= ALPHA_MIN);
    }

    #[test]
    fn no_adjust_before_warmup() {
        let mut aa = AdaptiveAlpha::new(1.0, 100);
        aa.update(1.0, 0.0);
        aa.update(1.0, 0.0);
        assert_eq!(aa.adjustments(), 0);
    }
}
