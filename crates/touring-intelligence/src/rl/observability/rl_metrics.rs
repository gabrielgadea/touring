//! RL performance metrics collected over rolling windows.
//!
//! Provides snapshot-based metrics using internal `RingBuffer` for hot-path
//! collection without heap allocation on the critical path.

use crate::rl::observability::RingBuffer;
use std::sync::atomic::{AtomicU64, Ordering};

/// RL engine metrics snapshot — sampled at query time from atomic counters.
#[derive(Debug, Clone, Default)]
pub struct RlMetrics {
    /// Total reward updates processed.
    pub update_count: u64,
    /// Exponential moving average of recent rewards (scaled by 1000 for integer storage).
    pub ema_reward_x1000: u64,
    /// Most recent TD error magnitude (scaled by 1000).
    pub last_td_error_x1000: u64,
    /// Number of Q-table lookups in the last window.
    pub qtable_lookups: u64,
}

/// Rolling window metrics collector for RL signals.
///
/// Uses atomic counters for hot-path collection and a `RingBuffer` for
/// windowed history. All operations are lock-free.
#[derive(Debug)]
pub struct RlMetricsCollector {
    update_count: AtomicU64,
    ema_reward_x1000: AtomicU64,
    last_td_error_x1000: AtomicU64,
    qtable_lookups: AtomicU64,
    /// History of last N EMA reward values for trend analysis.
    ema_history: RingBuffer<i64>,
    /// History of last N TD error values for convergence detection.
    td_error_history: RingBuffer<i64>,
}

impl RlMetricsCollector {
    /// Create a new collector with the given history window sizes.
    pub fn new(window_size: usize) -> Self {
        Self {
            update_count: AtomicU64::new(0),
            ema_reward_x1000: AtomicU64::new(0),
            last_td_error_x1000: AtomicU64::new(0),
            qtable_lookups: AtomicU64::new(0),
            ema_history: RingBuffer::new(window_size.max(1)),
            td_error_history: RingBuffer::new(window_size.max(1)),
        }
    }

    /// Record a completed reward update.
    pub fn record_update(&mut self, ema_reward: f64, td_error: f64) {
        self.update_count.fetch_add(1, Ordering::Relaxed);
        let ema_i64 = (ema_reward * 1000.0).round() as i64;
        self.ema_reward_x1000
            .store(ema_i64.unsigned_abs(), Ordering::Relaxed);
        let td_i64 = (td_error.abs() * 1000.0).round() as i64;
        self.last_td_error_x1000
            .store(td_i64.unsigned_abs(), Ordering::Relaxed);
        self.ema_history.write(ema_i64);
        self.td_error_history.write(td_i64);
    }

    /// Record a Q-table lookup.
    pub fn record_qtable_lookup(&mut self) {
        self.qtable_lookups.fetch_add(1, Ordering::Relaxed);
    }

    /// Take a snapshot of current metrics.
    pub fn snapshot(&self) -> RlMetrics {
        RlMetrics {
            update_count: self.update_count.load(Ordering::Relaxed),
            ema_reward_x1000: self.ema_reward_x1000.load(Ordering::Relaxed),
            last_td_error_x1000: self.last_td_error_x1000.load(Ordering::Relaxed),
            qtable_lookups: self.qtable_lookups.load(Ordering::Relaxed),
        }
    }

    /// Mean absolute TD error over the rolling window.
    ///
    /// `td_error_history` was written on every update since the collector
    /// existed but never read back, so `touring learning status` reported the
    /// LAST TD error under the name `mean_td_error` (04/08/2026). A single
    /// sample is noise; the window is what tells convergence from divergence.
    ///
    /// Returns `None` while no update has been recorded — an empty window has
    /// no mean, and reporting `0.0` there would read as "perfectly converged".
    pub fn mean_td_error(&self) -> Option<f64> {
        let vals: Vec<i64> = self.td_error_history.iter().copied().collect();
        if vals.is_empty() {
            return None;
        }
        let sum: i64 = vals.iter().sum();
        // Values are stored as |td| * 1000 (see `record_update`).
        Some(sum as f64 / vals.len() as f64 / 1000.0)
    }

    /// Compute trend direction from EMA history: 1 = improving, 0 = stable, -1 = degrading.
    pub fn ema_trend(&self) -> i8 {
        let vals: Vec<i64> = self.ema_history.iter().copied().collect();
        if vals.len() < 2 {
            return 0;
        }
        let mid = vals.len() / 2;
        let older: i64 = vals
            .get(..mid)
            .map(|s| s.iter().sum::<i64>() / mid as i64)
            .unwrap_or(0);
        let recent_len = vals.len() - mid;
        let recent: i64 = vals
            .get(mid..)
            .map(|s| s.iter().sum::<i64>() / recent_len as i64)
            .unwrap_or(0);
        let delta = recent - older;
        if delta > 10 {
            1
        } else if delta < -10 {
            -1
        } else {
            0
        }
    }

    /// Reset all counters and history.
    pub fn reset(&mut self) {
        self.update_count.store(0, Ordering::Relaxed);
        self.ema_reward_x1000.store(0, Ordering::Relaxed);
        self.last_td_error_x1000.store(0, Ordering::Relaxed);
        self.qtable_lookups.store(0, Ordering::Relaxed);
        // RingBuffer doesn't have clear — recreate
        self.ema_history = RingBuffer::new(self.ema_history.capacity());
        self.td_error_history = RingBuffer::new(self.td_error_history.capacity());
    }
}

impl Default for RlMetricsCollector {
    fn default() -> Self {
        Self::new(32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rl_metrics_collector_record_and_snapshot() {
        let mut collector = RlMetricsCollector::new(16);
        collector.record_update(0.75, 0.1);
        collector.record_update(0.80, 0.05);
        collector.record_qtable_lookup();
        collector.record_qtable_lookup();

        let snap = collector.snapshot();
        assert_eq!(snap.update_count, 2);
        assert!((snap.ema_reward_x1000 as f64 / 1000.0 - 0.80).abs() < 0.01);
        assert_eq!(snap.qtable_lookups, 2);
    }

    #[test]
    fn test_ema_trend_improving() {
        let mut collector = RlMetricsCollector::new(10);
        // Older half: low values, recent half: high values
        for _ in 0..5 {
            collector.record_update(0.3, 0.1);
        }
        for _ in 0..5 {
            collector.record_update(0.8, 0.05);
        }
        assert_eq!(collector.ema_trend(), 1);
    }

    #[test]
    fn test_reset_clears_counters() {
        let mut collector = RlMetricsCollector::new(8);
        collector.record_update(0.5, 0.1);
        collector.record_qtable_lookup();
        collector.reset();
        let snap = collector.snapshot();
        assert_eq!(snap.update_count, 0);
        assert_eq!(snap.qtable_lookups, 0);
    }

    /// `mean_td_error` must average the window, not echo the last sample.
    ///
    /// `touring learning status` reported the LAST error under the name
    /// `mean_td_error` until 04/08/2026 because this window was written on
    /// every update and never read back.
    #[test]
    fn mean_td_error_averages_the_window_instead_of_echoing_the_last_sample() {
        let mut collector = RlMetricsCollector::new(8);
        assert_eq!(
            collector.mean_td_error(),
            None,
            "an empty window has no mean — reporting 0.0 would read as converged"
        );

        for td in [1.0, 2.0, 3.0, 4.0] {
            collector.record_update(0.5, td);
        }

        let mean = collector.mean_td_error().expect("window is non-empty");
        assert!(
            (mean - 2.5).abs() < 1e-9,
            "mean of [1,2,3,4] is 2.5, got {mean}"
        );
        // The discriminating half: the mean must NOT be the last sample (4.0).
        assert!(
            (mean - 4.0).abs() > 1.0,
            "mean {mean} collapsed onto the last sample — the window is unused"
        );
    }

    /// The window averages absolute values, so sign-flipping errors do not cancel.
    #[test]
    fn mean_td_error_uses_absolute_values() {
        let mut collector = RlMetricsCollector::new(8);
        collector.record_update(0.0, 3.0);
        collector.record_update(0.0, -3.0);

        let mean = collector.mean_td_error().expect("window is non-empty");
        assert!(
            (mean - 3.0).abs() < 1e-9,
            "+3 and -3 must average to 3.0 (magnitude), not 0.0; got {mean}"
        );
    }
}
