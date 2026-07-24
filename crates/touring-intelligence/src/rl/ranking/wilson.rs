//! Wilson score ranking and drift detection.
//!
//! Unified from touring/src/learning/ranking.rs

use std::collections::HashMap;

/// Wilson score interval bounds for ranking with confidence.
#[derive(Debug, Clone, Copy)]
pub struct WilsonScore {
    /// Lower bound of confidence interval.
    pub lower: f64,
    /// Upper bound of confidence interval.
    pub upper: f64,
    /// Center score (for ranking).
    pub center: f64,
}

impl WilsonScore {
    /// Calculate Wilson score interval for a binomial proportion.
    #[must_use]
    pub fn calculate(successes: u64, trials: u64, confidence: f64) -> Option<Self> {
        if trials == 0 {
            return None;
        }

        let z = Self::z_score(confidence);
        let n = trials as f64;
        let p = successes as f64 / n;

        let z_sq = z * z;
        let denominator = 1.0 + z_sq / n;

        let center = (p + z_sq / (2.0 * n)) / denominator;
        let margin = z * (p * (1.0 - p) / n + z_sq / (4.0 * n * n)).sqrt() / denominator;

        let lower = (center - margin).max(0.0);
        let upper = (center + margin).min(1.0);

        Some(Self {
            lower,
            upper,
            center: (lower + upper) / 2.0,
        })
    }

    fn z_score(confidence: f64) -> f64 {
        match (confidence * 100.0).round() as u64 {
            90 => 1.645,
            95 => 1.96,
            99 => 2.576,
            _ => 1.96,
        }
    }
}

/// Ranked item with Wilson score.
#[derive(Debug, Clone)]
pub struct RankedItem {
    /// Identifier of the ranked item.
    pub id: String,
    /// Wilson score interval used for ranking.
    pub score: WilsonScore,
    /// Observed success rate (successes / trials).
    pub raw_rate: f64,
    /// Number of trials backing the score.
    pub trials: u64,
}

/// Wilson ranker for confidence-aware ranking.
#[derive(Debug, Clone)]
pub struct WilsonRanker {
    confidence: f64,
    stats: HashMap<String, (u64, u64)>,
}

impl WilsonRanker {
    /// Create a new ranker with 95% confidence interval.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_intelligence::rl::ranking::wilson::WilsonRanker;
    ///
    /// let mut ranker = WilsonRanker::new();
    ///
    /// // Record successes (true) and failures (false) for each item
    /// for _ in 0..95 { ranker.record("read_file", true); }
    /// for _ in 0..5 { ranker.record("read_file", false); }
    /// for _ in 0..80 { ranker.record("write_file", true); }
    /// for _ in 0..20 { ranker.record("write_file", false); }
    ///
    /// // Get ranked items (highest Wilson lower bound first)
    /// let ranked = ranker.rank();
    /// assert_eq!(ranked[0].id, "read_file"); // highest lower bound
    /// ```
    pub fn new() -> Self {
        Self {
            confidence: 0.95,
            stats: HashMap::new(),
        }
    }

    /// Create a ranker with an explicit `confidence` level (clamped to `[0, 1]`).
    pub fn with_confidence(confidence: f64) -> Self {
        Self {
            confidence: confidence.clamp(0.0, 1.0),
            stats: HashMap::new(),
        }
    }

    /// Record one outcome (`success` true/false) for item `id`.
    pub fn record(&mut self, id: &str, success: bool) {
        let entry = self.stats.entry(id.to_string()).or_insert((0, 0));
        entry.1 += 1;
        if success {
            entry.0 += 1;
        }
    }

    /// Return `(successes, trials)` recorded for item `id`, if any.
    pub fn get_stats(&self, id: &str) -> Option<(u64, u64)> {
        self.stats.get(id).copied()
    }

    /// Rank all items by descending Wilson lower bound.
    pub fn rank(&self) -> Vec<RankedItem> {
        let mut items: Vec<RankedItem> = self
            .stats
            .iter()
            .filter_map(|(id, (successes, trials))| {
                WilsonScore::calculate(*successes, *trials, self.confidence).map(|score| {
                    RankedItem {
                        id: id.clone(),
                        score,
                        raw_rate: *successes as f64 / *trials as f64,
                        trials: *trials,
                    }
                })
            })
            .collect();

        items.sort_by(|a, b| {
            b.score
                .lower
                .partial_cmp(&a.score.lower)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        items
    }

    /// Return the top `k` ranked items.
    pub fn top_k(&self, k: usize) -> Vec<RankedItem> {
        let mut ranked = self.rank();
        ranked.truncate(k);
        ranked
    }

    /// Remove all recorded statistics.
    pub fn clear(&mut self) {
        self.stats.clear();
    }

    /// Number of distinct items with recorded statistics.
    pub fn len(&self) -> usize {
        self.stats.len()
    }

    /// Returns `true` when no statistics have been recorded.
    pub fn is_empty(&self) -> bool {
        self.stats.is_empty()
    }

    #[must_use]
    /// Wilson lower-bound score for item `id` (0.0 when unknown).
    pub fn score(&self, id: &str) -> f64 {
        self.stats
            .get(id)
            .and_then(|(s, t)| WilsonScore::calculate(*s, *t, self.confidence))
            .map(|ws| ws.lower)
            .unwrap_or(0.0)
    }

    /// Snapshot of all items as `(id, successes, trials)` tuples.
    pub fn all_stats(&self) -> Vec<(String, u64, u64)> {
        self.stats
            .iter()
            .map(|(id, (s, t))| (id.clone(), *s, *t))
            .collect()
    }

    pub(crate) fn load_stats(&mut self, id: &str, successes: u64, trials: u64) {
        self.stats.insert(id.to_string(), (successes, trials));
    }
}

impl Default for WilsonRanker {
    fn default() -> Self {
        Self::new()
    }
}

/// Drift detection result.
#[derive(Debug, Clone)]
pub struct DriftResult {
    /// Whether drift exceeded the configured threshold.
    pub drift_detected: bool,
    /// Absolute mean shift between the older and recent halves of the window.
    pub magnitude: f64,
    /// Drift direction: `"up"`, `"down"`, or `"none"`.
    pub direction: String,
    /// Confidence in the result, scaled by sample count.
    pub confidence: f64,
}

/// Drift detector for monitoring pattern changes over time.
#[derive(Debug, Clone)]
pub struct DriftDetector {
    window_size: usize,
    threshold: f64,
    history: HashMap<String, Vec<f64>>,
}

impl DriftDetector {
    /// Construct a `DriftDetector` with default window size and threshold.
    pub fn new() -> Self {
        Self {
            window_size: 100,
            threshold: 0.1,
            history: HashMap::new(),
        }
    }

    /// Construct a `DriftDetector` with explicit `window_size` and `threshold` (clamped to `[0, 1]`).
    pub fn with_params(window_size: usize, threshold: f64) -> Self {
        Self {
            window_size,
            threshold: threshold.clamp(0.0, 1.0),
            history: HashMap::new(),
        }
    }

    /// Append a `value` to the rolling history for `metric`, evicting the oldest past the window.
    pub fn record(&mut self, metric: &str, value: f64) {
        let entry = self.history.entry(metric.to_string()).or_default();
        entry.push(value);
        if entry.len() > self.window_size {
            entry.remove(0);
        }
    }

    /// Detect drift for `metric` by comparing means of the older and recent halves of its window.
    pub fn detect(&self, metric: &str) -> DriftResult {
        let values = match self.history.get(metric) {
            Some(v) if v.len() >= 10 => v,
            _ => {
                return DriftResult {
                    drift_detected: false,
                    magnitude: 0.0,
                    direction: "none".to_string(),
                    confidence: 0.0,
                };
            }
        };

        let mid = values.len() / 2;
        // SAFETY: mid = len/2, so mid <= len. Both slices [..mid] and [mid..] are in-bounds.
        #[allow(clippy::indexing_slicing)]
        let older = &values[..mid];
        #[allow(clippy::indexing_slicing)]
        let recent = &values[mid..];

        let older_mean: f64 = older.iter().sum::<f64>() / older.len() as f64;
        let recent_mean: f64 = recent.iter().sum::<f64>() / recent.len() as f64;

        let magnitude = (recent_mean - older_mean).abs();
        let drift_detected = magnitude >= self.threshold;

        let direction = if recent_mean > older_mean {
            "up"
        } else if recent_mean < older_mean {
            "down"
        } else {
            "none"
        }
        .to_string();

        let n = values.len() as f64;
        let confidence = (1.0 - 1.0 / n.sqrt()).clamp(0.0, 1.0);

        DriftResult {
            drift_detected,
            magnitude,
            direction,
            confidence,
        }
    }

    /// Run drift detection across every recorded metric.
    pub fn detect_all(&self) -> HashMap<String, DriftResult> {
        self.history
            .keys()
            .map(|metric| (metric.clone(), self.detect(metric)))
            .collect()
    }

    /// Borrow the recorded value history for `metric`, if any.
    pub fn get_history(&self, metric: &str) -> Option<&[f64]> {
        self.history.get(metric).map(|v| v.as_slice())
    }

    /// Drop all recorded metric histories.
    pub fn clear(&mut self) {
        self.history.clear();
    }

    /// Drop the recorded history for a single `metric`.
    pub fn clear_metric(&mut self, metric: &str) {
        self.history.remove(metric);
    }

    /// Snapshot of every metric's value history.
    pub fn all_histories(&self) -> Vec<(String, Vec<f64>)> {
        self.history
            .iter()
            .map(|(m, v)| (m.clone(), v.clone()))
            .collect()
    }

    pub(crate) fn load_history(&mut self, metric: &str, values: Vec<f64>) {
        self.history.insert(metric.to_string(), values);
    }
}

impl Default for DriftDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wilson_score_perfect() {
        let score = WilsonScore::calculate(100, 100, 0.95).unwrap();
        assert!(score.lower > 0.9);
        assert!(score.upper <= 1.0);
    }

    #[test]
    fn test_wilson_score_empty() {
        assert!(WilsonScore::calculate(0, 0, 0.95).is_none());
    }

    #[test]
    fn test_ranker_record() {
        let mut r = WilsonRanker::new();
        r.record("item1", true);
        r.record("item1", true);
        r.record("item1", false);
        let stats = r.get_stats("item1").unwrap();
        assert_eq!(stats.0, 2);
        assert_eq!(stats.1, 3);
    }

    #[test]
    fn test_drift_detector_no_data() {
        let d = DriftDetector::new();
        let result = d.detect("metric1");
        assert!(!result.drift_detected);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_drift_detector_upward() {
        let mut d = DriftDetector::with_params(20, 0.05);
        for _ in 0..10 {
            d.record("metric1", 0.1);
        }
        for _ in 0..10 {
            d.record("metric1", 0.9);
        }
        let result = d.detect("metric1");
        assert!(result.drift_detected);
        assert_eq!(result.direction, "up");
    }
}
