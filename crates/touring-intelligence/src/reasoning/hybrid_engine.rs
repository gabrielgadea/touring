//! TRANSFORMADOR #3: HybridCognitiveEngine — MCTS selects next GotNode.
//!
//! Bridges `CognitiveMCTS` and `GotEngine` via a shared `MctsPheromonoLayer`:
//! - MCTS selects the next `ThoughtResult` to expand (pheromone-guided).
//! - After a GoT path completes, its accumulated reward is deposited back into
//!   the shared layer, closing the MCTS → GoT → pheromone feedback loop.

use std::sync::{Arc, Mutex};

use crate::reasoning::got::ThoughtResult;
use crate::reasoning::mcts::MctsPheromonoLayer;

/// Hybrid engine wiring MCTS pheromone guidance into GoT node selection.
///
/// Both `CognitiveMCTS` and `GotEngine` share the same `MctsPheromonoLayer`,
/// so pheromone deposited during MCTS search is visible to GoT beam pruning
/// and vice-versa.
pub struct HybridCognitiveEngine {
    /// Shared pheromone layer connecting MCTS and GoT.
    pub shared_pheromone: Arc<Mutex<MctsPheromonoLayer>>,
    /// Beam width for GoT level pruning (mirrors `GotEngine::beam_width`).
    pub beam_width: usize,
}

impl HybridCognitiveEngine {
    /// Create a new engine backed by the given shared pheromone layer.
    pub fn new(shared_pheromone: Arc<Mutex<MctsPheromonoLayer>>) -> Self {
        Self {
            shared_pheromone,
            beam_width: 16,
        }
    }

    /// Create a new engine with a fresh pheromone layer (alpha=0.5, rate=0.05).
    pub fn with_fresh_pheromone() -> Self {
        Self::new(Arc::new(Mutex::new(MctsPheromonoLayer::new(0.5, 0.05))))
    }

    /// Select the best candidate `ThoughtResult` using MCTS pheromone guidance.
    ///
    /// Score = `result.score * pheromone_strength(parent_id, node_id)`.
    /// Falls back to `result.score` when no pheromone trail exists.
    /// Returns `None` if `candidates` is empty.
    pub fn select_next_node<'a>(
        &self,
        candidates: &'a [ThoughtResult],
        parent_state: u64,
    ) -> Option<&'a ThoughtResult> {
        if candidates.is_empty() {
            return None;
        }
        let pheromone = self
            .shared_pheromone
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        candidates.iter().max_by(|a, b| {
            let score_a = a.score * (1.0 + pheromone.strength(parent_state, a.node_id));
            let score_b = b.score * (1.0 + pheromone.strength(parent_state, b.node_id));
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Deposit reward into the shared pheromone layer for an entire GoT path.
    ///
    /// For each consecutive pair `(parent, child)` in `path`, deposits `reward`
    /// on `(parent.node_id, child.node_id)`.
    pub fn deposit_got_reward(&self, path: &[ThoughtResult], reward: f64) {
        if reward <= 0.0 || path.len() < 2 {
            return;
        }
        let mut pheromone = self
            .shared_pheromone
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        for window in path.windows(2) {
            if let [parent, child] = window {
                pheromone.deposit(parent.node_id, child.node_id, reward);
            }
        }
    }

    /// Prune `candidates` to `beam_width` using pheromone-guided scoring.
    ///
    /// Candidates are sorted by `score * (1 + strength)` descending, then
    /// truncated to `beam_width`.
    pub fn beam_prune(
        &self,
        mut candidates: Vec<ThoughtResult>,
        parent_state: u64,
    ) -> Vec<ThoughtResult> {
        if candidates.len() <= self.beam_width {
            return candidates;
        }
        let pheromone = self
            .shared_pheromone
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        candidates.sort_by(|a, b| {
            let sa = a.score * (1.0 + pheromone.strength(parent_state, a.node_id));
            let sb = b.score * (1.0 + pheromone.strength(parent_state, b.node_id));
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(self.beam_width);
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(node_id: u64, score: f64) -> ThoughtResult {
        ThoughtResult {
            node_id,
            score,
            output: String::new(),
            depth: 0,
            relevance: 1.0,
            confidence: 0.5,
            novelty: 1.0,
        }
    }

    #[test]
    fn test_select_next_prefers_higher_score() {
        let engine = HybridCognitiveEngine::with_fresh_pheromone();
        let candidates = vec![
            make_result(1, 0.3),
            make_result(2, 0.9),
            make_result(3, 0.6),
        ];
        let best = engine
            .select_next_node(&candidates, 0)
            .expect("should find best");
        assert_eq!(best.node_id, 2);
    }

    #[test]
    fn test_select_empty_returns_none() {
        let engine = HybridCognitiveEngine::with_fresh_pheromone();
        assert!(engine.select_next_node(&[], 0).is_none());
    }

    #[test]
    fn test_deposit_got_reward_increases_strength() {
        let engine = HybridCognitiveEngine::with_fresh_pheromone();
        let path = vec![make_result(10, 0.8), make_result(20, 0.9)];
        engine.deposit_got_reward(&path, 1.0);
        let strength = engine
            .shared_pheromone
            .lock()
            .expect("lock")
            .strength(10, 20);
        assert!(strength > 0.0, "pheromone must be deposited");
    }

    #[test]
    fn test_deposit_non_positive_reward_is_noop() {
        let engine = HybridCognitiveEngine::with_fresh_pheromone();
        let path = vec![make_result(1, 0.5), make_result(2, 0.5)];
        engine.deposit_got_reward(&path, 0.0);
        let strength = engine.shared_pheromone.lock().expect("lock").strength(1, 2);
        assert_eq!(strength, 0.0);
    }

    #[test]
    fn test_beam_prune_truncates_to_beam_width() {
        let mut engine = HybridCognitiveEngine::with_fresh_pheromone();
        engine.beam_width = 2;
        let candidates: Vec<ThoughtResult> = (0..5).map(|i| make_result(i, i as f64)).collect();
        let pruned = engine.beam_prune(candidates, 0);
        assert_eq!(pruned.len(), 2);
    }
}
