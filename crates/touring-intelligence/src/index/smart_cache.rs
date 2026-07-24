//! RL-guided cache priority via touring-learning.
//!
//! Wraps `FileCache` with a priority scorer backed
//! by touring-learning's LinUCB bandit. Files with higher predicted value get
//! priority retention during cache eviction.

use std::path::{Path, PathBuf};

use crate::rl::bandit::{FEATURE_DIM, LinUCBBandit};
use ndarray::Array1;

/// Cache priority features extracted from file access patterns.
///
/// These features are fed to the LinUCB bandit to predict which files
/// are most valuable to keep cached. The feature vector is zero-padded
/// to match the bandit's expected FEATURE_DIM (25).
#[derive(Debug, Clone)]
pub struct CachePriorityFeatures {
    /// How many times this file was accessed in the session.
    pub access_count: u32,
    /// Time since last access (seconds).
    pub recency_secs: f64,
    /// Number of symbols in the file.
    pub symbol_count: u32,
    /// Number of edits in the session.
    pub edit_count: u32,
    /// Whether the file had errors recently.
    pub had_recent_error: bool,
    /// File size in KB.
    pub size_kb: f64,
}

impl CachePriorityFeatures {
    /// Convert to a fixed-size ndarray feature vector for the bandit.
    #[allow(clippy::indexing_slicing)] // All indices 0..5 are within FEATURE_DIM=25
    pub fn to_features(&self) -> Array1<f64> {
        let mut features = Array1::zeros(FEATURE_DIM);
        features[0] = (self.access_count as f64).ln_1p() / 5.0;
        features[1] = (-self.recency_secs / 300.0).exp();
        features[2] = (self.symbol_count as f64).ln_1p() / 8.0;
        features[3] = (self.edit_count as f64).ln_1p() / 3.0;
        features[4] = if self.had_recent_error { 1.0 } else { 0.0 };
        features[5] = (self.size_kb / 100.0).min(1.0);
        // Remaining dimensions zero-padded
        features
    }
}

/// RL-guided cache priority scorer.
///
/// Uses a LinUCB bandit to score files by predicted value, allowing
/// informed eviction decisions. The bandit learns from reward signals:
/// - Cache hit → positive reward
/// - Evicted file re-requested soon → negative reward
#[derive(Debug)]
pub struct SmartCachePriority {
    bandit: LinUCBBandit,
    /// Track files and their last-seen features for reward attribution.
    feature_log: std::collections::HashMap<PathBuf, CachePriorityFeatures>,
}

impl SmartCachePriority {
    /// Create a new priority scorer.
    pub fn new() -> Self {
        Self {
            bandit: LinUCBBandit::new(),
            feature_log: std::collections::HashMap::new(),
        }
    }

    /// Score a file for cache priority (higher = keep longer).
    ///
    /// Returns the UCB score for the "keep" arm (arm 0).
    pub fn score(&mut self, _path: &Path, features: &CachePriorityFeatures) -> f64 {
        let feat_vec = features.to_features();
        let (arm, ucb_score) = self.bandit.select_arm(&feat_vec);
        if arm == 0 {
            ucb_score // bandit suggests keep — return confidence
        } else {
            0.0 // bandit suggests evict
        }
    }

    /// Record features for a file access (for later reward attribution).
    pub fn observe_access(&mut self, path: &Path, features: CachePriorityFeatures) {
        self.feature_log.insert(path.to_path_buf(), features);
    }

    /// Provide reward signal after a cache event.
    ///
    /// - `reward > 0`: cache hit — file was useful
    /// - `reward < 0`: cache miss for recently evicted file
    pub fn reward(&mut self, path: &Path, reward: f64) {
        if let Some(features) = self.feature_log.get(path) {
            let feat_vec = features.to_features();
            let arm = if reward > 0.0 { 0 } else { 1 };
            self.bandit.update(arm, &feat_vec, reward.abs());
        }
    }

    /// Rank files by priority (highest first).
    pub fn rank_files(
        &mut self,
        files: &[(PathBuf, CachePriorityFeatures)],
    ) -> Vec<(PathBuf, f64)> {
        let mut scored: Vec<(PathBuf, f64)> = files
            .iter()
            .map(|(path, features)| {
                let s = self.score(path, features);
                (path.clone(), s)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }
}

impl Default for SmartCachePriority {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_features(access_count: u32, recency: f64) -> CachePriorityFeatures {
        CachePriorityFeatures {
            access_count,
            recency_secs: recency,
            symbol_count: 10,
            edit_count: 2,
            had_recent_error: false,
            size_kb: 5.0,
        }
    }

    #[test]
    fn test_features_to_vector() {
        let features = sample_features(10, 60.0);
        let vec = features.to_features();
        assert_eq!(vec.len(), FEATURE_DIM);
        // access_count = ln(11)/5 ≈ 0.48
        assert!(vec[0] > 0.4 && vec[0] < 0.6);
        // recency = exp(-60/300) ≈ 0.82
        assert!(vec[1] > 0.7 && vec[1] < 0.9);
    }

    #[test]
    fn test_smart_cache_priority_score() {
        let mut scorer = SmartCachePriority::new();
        let features = sample_features(5, 30.0);
        let score = scorer.score(Path::new("test.py"), &features);
        assert!(score >= 0.0);
    }

    #[test]
    fn test_rank_files() {
        let mut scorer = SmartCachePriority::new();
        let files = vec![
            (PathBuf::from("a.py"), sample_features(100, 5.0)),
            (PathBuf::from("b.py"), sample_features(1, 3600.0)),
            (PathBuf::from("c.py"), sample_features(50, 30.0)),
        ];
        let ranked = scorer.rank_files(&files);
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn test_observe_and_reward() {
        let mut scorer = SmartCachePriority::new();
        let features = sample_features(10, 10.0);
        scorer.observe_access(Path::new("hot.py"), features);
        // Should not panic
        scorer.reward(Path::new("hot.py"), 1.0);
        scorer.reward(Path::new("unknown.py"), -0.5); // no-op for unknown
    }
}
