//! TRANSFORMADOR #2: TrackerRlBridge — connects TrackerReport to 3 RL systems.
//!
//! Distributes a `TrackerReport`'s 9 dimension scores simultaneously to:
//!   1. LinUCB context vector (via `DimensionalFeatures::append_to_context`)
//!   2. MultiObjectivePheromonoLayer (via `MultiObjectiveMapping`)
//!   3. SessionPredictor scalar reward

use super::tracker::{DimensionalFeatures, TrackerReport};

/// Maps 9 tracker dimensions → 4 Pareto objectives.
///
/// Each objective is a weighted sum of specific dimension scores:
/// - latency    ← D3 (performance)
/// - coverage   ← D2 (robustness) + D4 (readability)
/// - tokens     ← D9 (deliverable)
/// - precision  ← D1 (functional) + D6 (spec) + D8 (no-halluc)
#[derive(Debug, Clone)]
pub struct MultiObjectiveMapping {
    /// (dim_index_0..8, weight) pairs for the latency objective.
    pub latency_dims: Vec<(usize, f64)>,
    /// (dim_index, weight) pairs for the coverage objective.
    pub coverage_dims: Vec<(usize, f64)>,
    /// (dim_index, weight) pairs for the token-efficiency objective.
    pub token_dims: Vec<(usize, f64)>,
    /// (dim_index, weight) pairs for the precision objective.
    pub precision_dims: Vec<(usize, f64)>,
}

impl Default for MultiObjectiveMapping {
    fn default() -> Self {
        Self::default_mapping()
    }
}

impl MultiObjectiveMapping {
    /// Default mapping as described in TRANSFORMADOR #2 spec.
    pub fn default_mapping() -> Self {
        Self {
            latency_dims: vec![(2, 1.0)],                       // D3
            coverage_dims: vec![(1, 0.5), (3, 0.5)],            // D2 + D4
            token_dims: vec![(8, 1.0)],                         // D9
            precision_dims: vec![(0, 0.4), (5, 0.3), (7, 0.3)], // D1+D6+D8
        }
    }

    /// Compute 4 objective scores from a 9-element dims array.
    pub fn compute_objectives(&self, dims: &[f64; 9]) -> [f64; 4] {
        let obj = |mapping: &Vec<(usize, f64)>| -> f64 {
            mapping
                .iter()
                .map(|(i, w)| dims.get(*i).copied().unwrap_or(0.0) * w)
                .sum()
        };
        [
            obj(&self.latency_dims),
            obj(&self.coverage_dims),
            obj(&self.token_dims),
            obj(&self.precision_dims),
        ]
    }
}

/// Bridge distributing a `TrackerReport` to 3 RL subsystems.
#[derive(Debug)]
pub struct TrackerRlBridge {
    /// Mapping from 9 dims → 4 Pareto objectives.
    pub multi_obj_mapping: MultiObjectiveMapping,
    /// Weight for the scalar SessionPredictor reward (0.0–1.0).
    pub session_reward_weight: f64,
}

impl Default for TrackerRlBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl TrackerRlBridge {
    /// Create a new bridge with default settings.
    pub fn new() -> Self {
        Self {
            multi_obj_mapping: MultiObjectiveMapping::default_mapping(),
            session_reward_weight: 0.8,
        }
    }

    /// Extract `DimensionalFeatures` for LinUCB from a `TrackerReport`.
    pub fn extract_linucb_features(&self, report: &TrackerReport) -> DimensionalFeatures {
        report.as_linucb_features()
    }

    /// Compute Pareto objectives from a `TrackerReport`'s dimension scores.
    pub fn compute_pareto_objectives(&self, report: &TrackerReport) -> [f64; 4] {
        let feats = report.as_linucb_features();
        let dims = feats.as_array();
        self.multi_obj_mapping.compute_objectives(&dims)
    }

    /// Compute a scalar reward for SessionPredictor from a `TrackerReport`.
    pub fn scalar_reward(&self, report: &TrackerReport) -> f64 {
        report.as_rl_reward() * self.session_reward_weight
    }

    /// Process a `TrackerReport` and return all 3 signals in one call.
    ///
    /// Returns `(linucb_features, pareto_objectives[4], session_reward)`.
    pub fn process_report(&self, report: &TrackerReport) -> (DimensionalFeatures, [f64; 4], f64) {
        let feats = self.extract_linucb_features(report);
        let objectives = self.compute_pareto_objectives(report);
        let reward = self.scalar_reward(report);
        (feats, objectives, reward)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::aco::tracker::{build_report, dim_from_checks};

    fn make_report(score: f64) -> TrackerReport {
        let dims: Vec<_> = (1..=9)
            .map(|i| {
                dim_from_checks(
                    &format!("D{i}"),
                    &format!("dim-{i}"),
                    &[("c1", score >= 0.8), ("c2", score >= 0.5)],
                )
            })
            .collect();
        build_report(dims, 1)
    }

    #[test]
    fn test_process_report_returns_three_signals() {
        let bridge = TrackerRlBridge::new();
        let report = make_report(1.0);
        let (feats, objectives, reward) = bridge.process_report(&report);
        assert!(feats.d1 >= 0.0 && feats.d1 <= 1.0);
        assert_eq!(objectives.len(), 4);
        assert!(reward >= 0.0 && reward <= 1.0);
    }

    #[test]
    fn test_scalar_reward_respects_weight() {
        let mut bridge = TrackerRlBridge::new();
        bridge.session_reward_weight = 0.5;
        let report = make_report(1.0);
        let reward = bridge.scalar_reward(&report);
        // as_rl_reward for Pass with composite ≈ 1.0 → 0.5 + 0.5*1.0 = 1.0
        // reward = 1.0 * 0.5 = 0.5
        assert!(reward <= 0.5 + 1e-6);
    }

    #[test]
    fn test_pareto_objectives_non_negative() {
        let bridge = TrackerRlBridge::new();
        let report = make_report(0.9);
        let objectives = bridge.compute_pareto_objectives(&report);
        for obj in &objectives {
            assert!(*obj >= 0.0);
        }
    }

    #[test]
    fn test_mapping_default_indices() {
        let mapping = MultiObjectiveMapping::default_mapping();
        // latency_dims should reference index 2 (D3, 0-indexed)
        assert_eq!(mapping.latency_dims[0].0, 2);
        // token_dims should reference index 8 (D9, 0-indexed)
        assert_eq!(mapping.token_dims[0].0, 8);
    }
}
