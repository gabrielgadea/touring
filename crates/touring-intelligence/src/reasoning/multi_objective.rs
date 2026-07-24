//! INS-C1: Multi-objective pheromone layer with Pareto-front selection.
//!
//! Extends the single-scalar pheromone model to four objectives:
//!   \[0\] latency    — how quickly the action completes
//!   \[1\] coverage   — how much of the codebase is exercised
//!   \[2\] tokens     — token efficiency (inverse of cost)
//!   \[3\] precision  — correctness / hallucination-free score
//!
//! Pareto domination: candidate B dominates A when B ≥ A on all objectives
//! AND B > A on at least one objective.

use std::collections::HashMap;

/// Number of objectives tracked per (state, action) pair.
pub const NUM_OBJECTIVES: usize = 4;

/// Objective index constants for readability.
pub mod objective {
    /// Objective slot: action completion latency.
    pub const LATENCY: usize = 0;
    /// Objective slot: fraction of the codebase exercised.
    pub const COVERAGE: usize = 1;
    /// Objective slot: token efficiency (inverse of cost).
    pub const TOKENS: usize = 2;
    /// Objective slot: correctness / hallucination-free score.
    pub const PRECISION: usize = 3;
}

/// INS-C1: Pheromone layer tracking `NUM_OBJECTIVES` independent objectives per
/// `(state, action)` pair. Supports Pareto-front selection over a candidate set.
pub struct MultiObjectivePheromonoLayer {
    /// Pheromone per (state_hash, action_hash) → [latency, coverage, tokens, precision].
    trails: HashMap<(u64, u64), [f64; NUM_OBJECTIVES]>,
    /// Scalarisation weights for tie-breaking (must sum to ≤ 1.0 — not enforced).
    pub weights: [f64; NUM_OBJECTIVES],
    /// Per-objective evaporation rates.
    pub evaporation_rates: [f64; NUM_OBJECTIVES],
    /// Prune threshold — entries whose weighted sum falls below this are removed.
    prune_threshold: f64,
}

impl Default for MultiObjectivePheromonoLayer {
    fn default() -> Self {
        Self::new([0.25; NUM_OBJECTIVES])
    }
}

impl MultiObjectivePheromonoLayer {
    /// Create a new layer with the given scalarisation `weights`.
    ///
    /// Default evaporation rates: 0.05 per objective.
    pub fn new(weights: [f64; NUM_OBJECTIVES]) -> Self {
        Self {
            trails: HashMap::new(),
            weights,
            evaporation_rates: [0.05; NUM_OBJECTIVES],
            prune_threshold: 0.001,
        }
    }

    /// Set per-objective evaporation rates.
    pub fn with_evaporation_rates(mut self, rates: [f64; NUM_OBJECTIVES]) -> Self {
        self.evaporation_rates = rates;
        self
    }

    /// Deposit `rewards` (one per objective) on the `(state, action)` pair.
    ///
    /// Each objective accumulates independently after evaporation:
    /// `trail[i] = trail[i] * (1 - rate[i]) + rewards[i]`
    pub fn deposit(&mut self, state: u64, action: u64, rewards: [f64; NUM_OBJECTIVES]) {
        let rates = self.evaporation_rates;
        let trail = self
            .trails
            .entry((state, action))
            .or_insert([0.0; NUM_OBJECTIVES]);
        for (slot, (r, rate)) in trail.iter_mut().zip(rewards.iter().zip(rates.iter())) {
            *slot = *slot * (1.0 - rate) + r;
        }
    }

    /// Return the pheromone trail for `(state, action)` (zeros if unknown).
    pub fn trail(&self, state: u64, action: u64) -> [f64; NUM_OBJECTIVES] {
        self.trails
            .get(&(state, action))
            .copied()
            .unwrap_or([0.0; NUM_OBJECTIVES])
    }

    /// Weighted sum of a trail (used for tie-breaking and ranking).
    fn weighted_sum(&self, trail: &[f64; NUM_OBJECTIVES]) -> f64 {
        trail
            .iter()
            .zip(self.weights.iter())
            .map(|(v, w)| v * w)
            .sum()
    }

    /// Return `true` if candidate `b` Pareto-dominates candidate `a`.
    ///
    /// B dominates A iff B ≥ A on all objectives AND B > A on at least one.
    pub fn dominates(&self, b: &(u64, u64), a: &(u64, u64)) -> bool {
        let ta = self.trails.get(a).copied().unwrap_or([0.0; NUM_OBJECTIVES]);
        let tb = self.trails.get(b).copied().unwrap_or([0.0; NUM_OBJECTIVES]);
        tb.iter().zip(ta.iter()).all(|(bv, av)| bv >= av)
            && tb.iter().zip(ta.iter()).any(|(bv, av)| bv > av)
    }

    /// Return `true` if `a` is dominated by at least one other candidate.
    pub fn is_dominated(&self, a: &(u64, u64), candidates: &[(u64, u64)]) -> bool {
        candidates.iter().any(|b| b != a && self.dominates(b, a))
    }

    /// Select the Pareto-optimal candidate with the highest weighted sum.
    ///
    /// 1. Filter out dominated candidates → Pareto front.
    /// 2. Among the front, return the one with the highest `weighted_sum`.
    ///
    /// Returns `None` if `candidates` is empty.
    pub fn pareto_best(&self, candidates: &[(u64, u64)]) -> Option<(u64, u64)> {
        if candidates.is_empty() {
            return None;
        }
        candidates
            .iter()
            .filter(|c| !self.is_dominated(c, candidates))
            .max_by(|a, b| {
                let sa = self.weighted_sum(
                    &self
                        .trails
                        .get(*a)
                        .copied()
                        .unwrap_or([0.0; NUM_OBJECTIVES]),
                );
                let sb = self.weighted_sum(
                    &self
                        .trails
                        .get(*b)
                        .copied()
                        .unwrap_or([0.0; NUM_OBJECTIVES]),
                );
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
    }

    /// Apply evaporation to all trails and prune weak entries.
    pub fn evaporate_all(&mut self) {
        let rates = self.evaporation_rates;
        let threshold = self.prune_threshold;
        let weights = self.weights;
        self.trails.retain(|_, trail| {
            for (slot, rate) in trail.iter_mut().zip(rates.iter()) {
                *slot *= 1.0 - rate;
            }
            let ws: f64 = trail.iter().zip(weights.iter()).map(|(v, w)| v * w).sum();
            ws >= threshold
        });
    }

    /// Total number of (state, action) entries tracked.
    pub fn entry_count(&self) -> usize {
        self.trails.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_accumulates() {
        let mut layer = MultiObjectivePheromonoLayer::new([0.25; 4]);
        layer.deposit(1, 2, [1.0, 0.5, 0.8, 0.9]);
        let trail = layer.trail(1, 2);
        assert!(trail[0] > 0.0);
        assert!(trail[1] > 0.0);
    }

    #[test]
    fn test_pareto_best_selects_non_dominated() {
        let mut layer = MultiObjectivePheromonoLayer::new([0.25; 4]);
        // A: weak on all
        layer.deposit(0, 1, [0.1, 0.1, 0.1, 0.1]);
        // B: strong on all → dominates A
        layer.deposit(0, 2, [0.9, 0.9, 0.9, 0.9]);

        let best = layer.pareto_best(&[(0, 1), (0, 2)]);
        assert_eq!(best, Some((0, 2)));
    }

    #[test]
    fn test_pareto_front_excludes_dominated() {
        let mut layer = MultiObjectivePheromonoLayer::new([0.25; 4]);
        layer.deposit(0, 1, [0.5, 0.5, 0.5, 0.5]);
        layer.deposit(0, 2, [0.6, 0.6, 0.6, 0.6]); // dominates (0,1)
        layer.deposit(0, 3, [0.9, 0.1, 0.9, 0.1]); // not dominated by (0,2)

        assert!(layer.is_dominated(&(0, 1), &[(0, 1), (0, 2), (0, 3)]));
        assert!(!layer.is_dominated(&(0, 2), &[(0, 1), (0, 2), (0, 3)]));
        assert!(!layer.is_dominated(&(0, 3), &[(0, 1), (0, 2), (0, 3)]));
    }

    #[test]
    fn test_evaporate_all_decays() {
        let mut layer = MultiObjectivePheromonoLayer::new([0.25; 4]);
        layer.deposit(1, 1, [1.0, 1.0, 1.0, 1.0]);
        let before = layer.trail(1, 1)[0];
        layer.evaporate_all();
        let after = layer.trail(1, 1)[0];
        assert!(after < before, "evaporation must decay the trail");
    }

    #[test]
    fn test_empty_candidates_returns_none() {
        let layer = MultiObjectivePheromonoLayer::default();
        assert_eq!(layer.pareto_best(&[]), None);
    }
}
