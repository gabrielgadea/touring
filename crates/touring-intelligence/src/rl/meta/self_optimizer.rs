//! Self-modifying hyperparameter optimizer (autoresearch-inspired).
//!
//! Inspired by Karpathy's autoresearch KEEP/DISCARD/RESET loop.
//! Observes reward over EVAL_WINDOW steps. When window fills:
//!
//! - KEEP: delta > KEEP_THRESHOLD → reward improving, keep current config.
//! - DISCARD: |delta| <= KEEP_THRESHOLD → neutral, minor tuning adjustment.
//! - RESET: delta < RESET_THRESHOLD → reward degrading, reset to defaults.

use std::collections::VecDeque;

const EVAL_WINDOW: usize = 50;
const KEEP_THRESHOLD: f64 = 0.02;
const RESET_THRESHOLD: f64 = -0.05;

/// Proposed hyperparameter adjustment emitted by [`SelfOptimizer`].
#[derive(Debug, Clone, PartialEq)]
pub enum HyperparamAdjustment {
    /// Reward improving — no change needed.
    Keep,
    /// Increase EMA alpha (faster adaptation to new signal).
    IncreaseEmaAlpha,
    /// Decrease EMA alpha (more smoothing, less noise sensitivity).
    DecreaseEmaAlpha,
    /// Increase forced exploration interval (less frequent forced explore).
    IncreaseExploreInterval,
    /// Decrease forced exploration interval (more frequent forced explore).
    DecreaseExploreInterval,
    /// Reward degrading — reset config to defaults and perturb.
    Reset,
}

/// Self-modifying optimizer that watches reward and proposes hyperparameter adjustments.
///
/// Accumulates `EVAL_WINDOW` reward samples per evaluation epoch.
/// Compares current window mean to previous window mean and emits an adjustment.
#[derive(Debug, Clone)]
pub struct SelfOptimizer {
    window: VecDeque<f64>,
    prev_mean: Option<f64>,
    eval_count: u64,
    keep_count: u64,
    discard_count: u64,
    reset_count: u64,
}

impl SelfOptimizer {
    /// Construct a `SelfOptimizer` with an empty evaluation window and zeroed counters.
    pub fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(EVAL_WINDOW),
            prev_mean: None,
            eval_count: 0,
            keep_count: 0,
            discard_count: 0,
            reset_count: 0,
        }
    }

    /// Observe a reward value. Returns `Some(adjustment)` when evaluation window fills.
    pub fn observe(&mut self, reward: f64) -> Option<HyperparamAdjustment> {
        self.window.push_back(reward);
        if self.window.len() < EVAL_WINDOW {
            return None;
        }
        let current_mean: f64 = self.window.iter().sum::<f64>() / EVAL_WINDOW as f64;
        self.window.clear();
        self.eval_count += 1;

        let adjustment = match self.prev_mean {
            None => HyperparamAdjustment::Keep,
            Some(prev) => {
                let delta = current_mean - prev;
                if delta > KEEP_THRESHOLD {
                    self.keep_count += 1;
                    HyperparamAdjustment::Keep
                } else if delta < RESET_THRESHOLD {
                    self.reset_count += 1;
                    HyperparamAdjustment::Reset
                } else {
                    self.discard_count += 1;
                    if current_mean < 0.0 {
                        HyperparamAdjustment::DecreaseExploreInterval
                    } else {
                        HyperparamAdjustment::IncreaseEmaAlpha
                    }
                }
            }
        };

        self.prev_mean = Some(current_mean);
        Some(adjustment)
    }

    /// Number of completed evaluation windows.
    pub fn eval_count(&self) -> u64 {
        self.eval_count
    }
    /// Number of evaluations that decided to keep current hyperparameters.
    pub fn keep_count(&self) -> u64 {
        self.keep_count
    }
    /// Number of evaluations that discarded (adjusted) hyperparameters.
    pub fn discard_count(&self) -> u64 {
        self.discard_count
    }
    /// Number of evaluations that triggered a hyperparameter reset.
    pub fn reset_count(&self) -> u64 {
        self.reset_count
    }
    /// Mean reward of the previous evaluation window, if one completed.
    pub fn prev_mean(&self) -> Option<f64> {
        self.prev_mean
    }
}

impl Default for SelfOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_output_before_window_fills() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW - 1 {
            assert!(opt.observe(0.5).is_none());
        }
    }

    #[test]
    fn keep_when_improving() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW {
            opt.observe(0.1);
        }
        let result = (0..EVAL_WINDOW).filter_map(|_| opt.observe(0.2)).last();
        assert_eq!(result, Some(HyperparamAdjustment::Keep));
        assert_eq!(opt.keep_count(), 1);
    }

    #[test]
    fn reset_when_degrading() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW {
            opt.observe(0.5);
        }
        let result = (0..EVAL_WINDOW).filter_map(|_| opt.observe(0.3)).last();
        assert_eq!(result, Some(HyperparamAdjustment::Reset));
        assert_eq!(opt.reset_count(), 1);
    }

    #[test]
    fn discard_when_neutral_positive_mean() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW {
            opt.observe(0.5);
        }
        let result = (0..EVAL_WINDOW).filter_map(|_| opt.observe(0.5)).last();
        assert_eq!(result, Some(HyperparamAdjustment::IncreaseEmaAlpha));
        assert_eq!(opt.discard_count(), 1);
    }

    #[test]
    fn discard_when_neutral_negative_mean() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW {
            opt.observe(-0.1);
        }
        let result = (0..EVAL_WINDOW).filter_map(|_| opt.observe(-0.1)).last();
        assert_eq!(result, Some(HyperparamAdjustment::DecreaseExploreInterval));
        assert_eq!(opt.discard_count(), 1);
    }

    #[test]
    fn eval_count_increments() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW * 3 {
            opt.observe(0.5);
        }
        assert_eq!(opt.eval_count(), 3);
    }

    #[test]
    fn test_new_initial_state() {
        let opt = SelfOptimizer::new();
        assert_eq!(opt.eval_count(), 0);
        assert_eq!(opt.keep_count(), 0);
        assert_eq!(opt.discard_count(), 0);
        assert_eq!(opt.reset_count(), 0);
        assert!(opt.prev_mean().is_none());
    }

    #[test]
    fn test_observe_returns_some_at_window_boundary() {
        let mut opt = SelfOptimizer::new();
        // Exactly EVAL_WINDOW observations — the last one must return Some
        let mut last = None;
        for _ in 0..EVAL_WINDOW {
            last = opt.observe(0.5);
        }
        assert!(
            last.is_some(),
            "50th observation must return Some(adjustment)"
        );
    }

    #[test]
    fn test_default_same_as_new() {
        let a = SelfOptimizer::new();
        let b = SelfOptimizer::default();
        assert_eq!(a.eval_count(), b.eval_count());
        assert_eq!(a.keep_count(), b.keep_count());
        assert_eq!(a.discard_count(), b.discard_count());
        assert_eq!(a.reset_count(), b.reset_count());
        assert_eq!(a.prev_mean(), b.prev_mean());
    }

    #[test]
    fn test_prev_mean_none_before_first_window() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW - 1 {
            opt.observe(0.5);
        }
        assert!(
            opt.prev_mean().is_none(),
            "prev_mean must be None before first window fills"
        );
    }

    #[test]
    fn test_prev_mean_some_after_first_window() {
        let mut opt = SelfOptimizer::new();
        for _ in 0..EVAL_WINDOW {
            opt.observe(0.5);
        }
        assert!(
            opt.prev_mean().is_some(),
            "prev_mean must be Some after first window"
        );
        let mean = opt.prev_mean().expect("prev_mean set after first window");
        assert!((mean - 0.5).abs() < 1e-9, "mean of 50x 0.5 should be 0.5");
    }
}
