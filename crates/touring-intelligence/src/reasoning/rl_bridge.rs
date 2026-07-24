//! RlBridge — Trait connecting cognitive engine (MCTS/GoT) to learning engine (QTable/LinUCB).
//!
//! Inspired by autoresearch P4: Single Scalar Metric (compose).
//! MCTS planning uses learned Q-values for informed rollout evaluation,
//! with exploration bonus for under-explored actions.
//!
//! ## Design
//!
//! The trait abstracts over the specific RL implementation so that:
//! - MCTS can be tested with a mock RlBridge (no real QTable needed)
//! - The cognitive crate doesn't couple to specific QTable field layout
//! - Exploration bonus formula can be tuned independently
//!
//! ## Integration
//!
//! In touring-server, the real implementation wraps `Arc<Mutex<QTable>>` + `Arc<Mutex<LinUCBBandit>>`.
//! The `MCTSEngine::search_with_rl()` method composes Q-value with exploration bonus.

/// Bridge between cognitive engine and learning engine.
///
/// Allows MCTS to leverage learned Q-values without hard coupling.
pub trait RlBridge: Send + Sync {
    /// Get Q-value for a (state, action) pair.
    ///
    /// Returns the learned Q-value if the pair has been observed,
    /// or `default_q` (typically 0.0) for unseen pairs.
    fn get_q_value(&self, state: u64, action: u64) -> f64;

    /// Check if a (state, action) pair has been observed at least once.
    fn has_observation(&self, state: u64, action: u64) -> bool;

    /// Get the total number of times this state has been visited.
    /// Used for UCB exploration bonus computation.
    fn state_visit_count(&self, state: u64) -> u64;

    /// Get the number of times this specific action was taken from this state.
    fn action_visit_count(&self, state: u64, action: u64) -> u64;

    /// Get top-K actions for a state, sorted by Q-value descending.
    ///
    /// Returns `(action_id, q_value)` pairs. Empty if no actions known.
    fn top_k_actions(&self, state: u64, k: usize) -> Vec<(u64, f64)>;
}

/// Returns a `QTableBridge` wrapping the global QTable singleton.
///
/// This is the canonical way to get a cognitive RL bridge backed by
/// the shared QTable from `touring_intelligence::rl::rl::global_qtable()`.
pub fn qtable_bridge_from_global() -> crate::reasoning::qtable_bridge::QTableBridge {
    crate::reasoning::qtable_bridge::QTableBridge::new(crate::rl::rl::global_qtable())
}

/// Compute reward for MCTS rollout, composing Q-value with exploration bonus.
///
/// Formula: `reward = q_value + exploration_bonus`
/// where `exploration_bonus = C * sqrt(ln(N) / n_a)`
///   - C = exploration constant (typically sqrt(2))
///   - N = total visits to parent state
///   - n_a = visits to this specific action
///
/// For unseen actions (n_a = 0), returns a high exploration bonus
/// to ensure they get tried at least once.
pub fn rl_informed_reward(
    bridge: &dyn RlBridge,
    state: u64,
    action: u64,
    exploration_c: f64,
) -> f64 {
    let q = bridge.get_q_value(state, action);
    let n_total = bridge.state_visit_count(state).max(1) as f64;
    let n_action = bridge.action_visit_count(state, action) as f64;

    if n_action < 1.0 {
        // Unseen action: return high value to encourage exploration
        q + exploration_c * 2.0
    } else {
        // UCB1-style exploration bonus
        let bonus = exploration_c * (n_total.ln() / n_action).sqrt();
        q + bonus
    }
}

/// No-op bridge that returns uniform rewards (baseline behavior).
///
/// Used as fallback when no RL data is available, and in tests.
#[derive(Debug, Default)]
pub struct UniformBridge {
    /// Constant Q-value returned for every `(state, action)` query.
    pub default_q: f64,
}

impl RlBridge for UniformBridge {
    fn get_q_value(&self, _state: u64, _action: u64) -> f64 {
        self.default_q
    }

    fn has_observation(&self, _state: u64, _action: u64) -> bool {
        false
    }

    fn state_visit_count(&self, _state: u64) -> u64 {
        0
    }

    fn action_visit_count(&self, _state: u64, _action: u64) -> u64 {
        0
    }

    fn top_k_actions(&self, _state: u64, _k: usize) -> Vec<(u64, f64)> {
        Vec::new()
    }
}

/// Mock bridge for testing with pre-configured Q-values.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockBridge {
    /// Pre-configured Q-values keyed by `(state, action)`.
    pub q_values: std::collections::HashMap<(u64, u64), f64>,
    /// Pre-configured visit counts keyed by `(state, action)`.
    pub visit_counts: std::collections::HashMap<(u64, u64), u64>,
    /// Pre-configured per-state visit totals keyed by `state`.
    pub state_visits: std::collections::HashMap<u64, u64>,
}

#[cfg(test)]
impl RlBridge for MockBridge {
    fn get_q_value(&self, state: u64, action: u64) -> f64 {
        self.q_values.get(&(state, action)).copied().unwrap_or(0.0)
    }

    fn has_observation(&self, state: u64, action: u64) -> bool {
        self.q_values.contains_key(&(state, action))
    }

    fn state_visit_count(&self, state: u64) -> u64 {
        self.state_visits.get(&state).copied().unwrap_or(0)
    }

    fn action_visit_count(&self, state: u64, action: u64) -> u64 {
        self.visit_counts
            .get(&(state, action))
            .copied()
            .unwrap_or(0)
    }

    fn top_k_actions(&self, state: u64, k: usize) -> Vec<(u64, f64)> {
        let mut actions: Vec<(u64, f64)> = self
            .q_values
            .iter()
            .filter(|((s, _), _)| *s == state)
            .map(|((_, a), q)| (*a, *q))
            .collect();
        actions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        actions.truncate(k);
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::QTableBridge;
    use std::collections::HashMap;

    /// Integration test: QTableBridge reads QTable updates from touring-learning.
    ///
    /// Verifies that the bridge correctly surfaces Q-values recorded in the
    /// shared global QTable back to the cognitive engine via the RlBridge trait.
    #[test]
    fn test_qtable_bridge_reads_global_qtable() {
        use crate::rl::QLearning;
        use crate::rl::rl::global_qtable;

        let qtable = global_qtable();
        // Record a Q-value in the global table
        {
            let mut table = qtable.lock().unwrap_or_else(|e| e.into_inner());
            let _ = QLearning::update(&mut *table, 1, 10, 0.8, 1, None, true);
        }
        let bridge = QTableBridge::new(qtable);
        // Bridge should surface the recorded Q-value
        let q = bridge.get_q_value(1, 10);
        assert!(
            (q - 0.8).abs() < 1e-6,
            "Expected Q=0.8 after terminal update, got {q}"
        );
        assert!(bridge.has_observation(1, 10));
    }

    #[test]
    fn test_uniform_bridge_default() {
        let bridge = UniformBridge::default();
        assert_eq!(bridge.get_q_value(1, 2), 0.0);
        assert!(!bridge.has_observation(1, 2));
        assert_eq!(bridge.state_visit_count(1), 0);
        assert!(bridge.top_k_actions(1, 5).is_empty());
    }

    #[test]
    fn test_rl_informed_reward_unseen_action() {
        let bridge = UniformBridge::default();
        let reward = rl_informed_reward(&bridge, 1, 2, std::f64::consts::SQRT_2);
        // Unseen: q(0.0) + C * 2.0 = sqrt(2) * 2.0 ≈ 2.83
        assert!(
            reward > 2.0,
            "Unseen action should get high exploration bonus"
        );
    }

    #[test]
    fn test_rl_informed_reward_with_q_values() {
        let mut bridge = MockBridge::default();
        bridge.q_values.insert((1, 10), 0.8); // Good action
        bridge.q_values.insert((1, 20), 0.2); // Bad action
        bridge.visit_counts.insert((1, 10), 10);
        bridge.visit_counts.insert((1, 20), 10);
        bridge.state_visits.insert(1, 20);

        let c = std::f64::consts::SQRT_2;
        let r_good = rl_informed_reward(&bridge, 1, 10, c);
        let r_bad = rl_informed_reward(&bridge, 1, 20, c);

        // Both have same visit count, so exploration bonus is equal
        // Difference should be purely from Q-values
        assert!(
            r_good > r_bad,
            "Higher Q-value action should have higher reward: {r_good} vs {r_bad}"
        );
        assert!(
            (r_good - r_bad - 0.6).abs() < 0.01,
            "Difference should be ~0.6 (Q diff): actual {}",
            r_good - r_bad
        );
    }

    #[test]
    fn test_rl_informed_reward_exploration_bonus() {
        let mut bridge = MockBridge::default();
        bridge.q_values.insert((1, 10), 0.5);
        bridge.q_values.insert((1, 20), 0.5); // Same Q-value
        bridge.visit_counts.insert((1, 10), 100); // Well-explored
        bridge.visit_counts.insert((1, 20), 2); // Under-explored
        bridge.state_visits.insert(1, 102);

        let c = std::f64::consts::SQRT_2;
        let r_explored = rl_informed_reward(&bridge, 1, 10, c);
        let r_under = rl_informed_reward(&bridge, 1, 20, c);

        // Same Q-value but under-explored action should get higher exploration bonus
        assert!(
            r_under > r_explored,
            "Under-explored action should get higher reward: {r_under} vs {r_explored}"
        );
    }

    #[test]
    fn test_mock_bridge_top_k() {
        let mut bridge = MockBridge::default();
        bridge.q_values = HashMap::from([
            ((1, 10), 0.9),
            ((1, 20), 0.3),
            ((1, 30), 0.7),
            ((1, 40), 0.1),
            ((2, 50), 0.5), // Different state — should not appear
        ]);

        let top2 = bridge.top_k_actions(1, 2);
        assert_eq!(top2.len(), 2);
        assert_eq!(top2[0], (10, 0.9)); // Highest Q
        assert_eq!(top2[1], (30, 0.7)); // Second highest
    }

    #[test]
    fn test_mock_bridge_top_k_empty() {
        let bridge = MockBridge::default();
        assert!(bridge.top_k_actions(1, 5).is_empty());
    }
}
