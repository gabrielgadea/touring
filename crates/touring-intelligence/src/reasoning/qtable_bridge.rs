//! QTableBridge — Real [`RlBridge`] implementation wrapping touring-learning's QTable.
//!
//! Provides the cognitive engine (MCTS/GoT) with live Q-values from the
//! learning engine without hard-coupling to QTable internals.
//!
//! ## Thread Safety
//!
//! QTable is shared across hooks and the cognitive engine via `Arc<Mutex<QTable>>`.
//! All lock acquisitions use poison-recovery (`unwrap_or_else(|e| e.into_inner())`)
//! to ensure forward progress even after a panicked thread.

use std::sync::{Arc, Mutex};

use crate::rl::rl::qtable::QTable;

use crate::reasoning::rl_bridge::RlBridge;

/// Bridge from touring-learning's [`QTable`] to the cognitive [`RlBridge`] trait.
///
/// Wraps a shared `Arc<Mutex<QTable>>` and delegates every method to the
/// underlying table with poison-safe locking.
pub struct QTableBridge {
    qtable: Arc<Mutex<QTable>>,
}

impl QTableBridge {
    /// Create a new bridge wrapping the given shared QTable.
    pub fn new(qtable: Arc<Mutex<QTable>>) -> Self {
        Self { qtable }
    }

    /// Acquire a poison-safe lock on the inner QTable.
    fn lock(&self) -> std::sync::MutexGuard<'_, QTable> {
        self.qtable.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl RlBridge for QTableBridge {
    fn get_q_value(&self, state: u64, action: u64) -> f64 {
        self.lock().get_value(state, action)
    }

    fn has_observation(&self, state: u64, action: u64) -> bool {
        let qt = self.lock();
        qt.get_state_q_values(state)
            .iter()
            .any(|&(a, _)| a == action)
    }

    fn state_visit_count(&self, state: u64) -> u64 {
        // QTable tracks known actions per state but not raw visit counts.
        // Use the number of distinct actions observed as a proxy for visit count.
        // This is consistent with the UCB1 formula: N = total observations from state.
        self.lock().get_state_q_values(state).len() as u64
    }

    fn action_visit_count(&self, state: u64, action: u64) -> u64 {
        // QTable does not track per-action visit counts.
        // Return 1 if the action has been observed (has a Q-entry), 0 otherwise.
        // This gives a binary signal: explored (1) vs unexplored (0).
        if self.has_observation(state, action) {
            1
        } else {
            0
        }
    }

    fn top_k_actions(&self, state: u64, k: usize) -> Vec<(u64, f64)> {
        let qt = self.lock();
        let mut actions = qt.get_state_q_values(state);
        actions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        actions.truncate(k);
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::rl::qtable::QLearning;

    /// Helper: create a shared QTable wrapped in Arc<Mutex>.
    fn shared_qtable() -> Arc<Mutex<QTable>> {
        Arc::new(Mutex::new(QTable::new()))
    }

    #[test]
    fn test_new_bridge_wraps_qtable() {
        let qt = shared_qtable();
        let bridge = QTableBridge::new(Arc::clone(&qt));
        // Empty table: no observations
        assert!(!bridge.has_observation(1, 1));
        assert_eq!(bridge.state_visit_count(1), 0);
    }

    #[test]
    fn test_get_q_value_default_for_unknown() {
        let bridge = QTableBridge::new(shared_qtable());
        // Default initial_q is 0.0
        let q = bridge.get_q_value(42, 99);
        assert!(
            (q - 0.0).abs() < f64::EPSILON,
            "Unknown state-action should return initial_q (0.0), got {q}"
        );
    }

    #[test]
    fn test_get_q_value_returns_stored_value() {
        let qt = shared_qtable();
        // Insert a Q-value via update (TD learning)
        {
            let mut table = qt.lock().unwrap_or_else(|e| e.into_inner());
            // update(state, action, reward, next_state, next_action, terminal)
            // Terminal state with current_q=0.0: QTable sets Q directly to reward.
            let _ = table.update(10, 20, 0.75, 10, None, true);
        }
        let bridge = QTableBridge::new(qt);
        let q = bridge.get_q_value(10, 20);
        assert!(
            (q - 0.75).abs() < 1e-6,
            "Expected Q=0.75 after terminal update, got {q}"
        );
    }

    #[test]
    fn test_top_k_actions_returns_sorted() {
        let qt = shared_qtable();
        {
            let mut table = qt.lock().unwrap_or_else(|e| e.into_inner());
            // Create distinct Q-values by doing multiple terminal updates
            // Action 1: reward=1.0 → Q ~0.1
            let _ = table.update(1, 100, 1.0, 1, None, true);
            // Action 2: reward=5.0 → Q ~0.5
            let _ = table.update(1, 200, 5.0, 1, None, true);
            // Action 3: reward=3.0 → Q ~0.3
            let _ = table.update(1, 300, 3.0, 1, None, true);
        }
        let bridge = QTableBridge::new(qt);
        let top2 = bridge.top_k_actions(1, 2);
        assert_eq!(top2.len(), 2, "Should return exactly 2 actions");
        // Highest Q should be first
        assert!(
            top2[0].1 >= top2[1].1,
            "Actions should be sorted descending: {:?}",
            top2
        );
        assert_eq!(
            top2[0].0, 200,
            "Action 200 (highest reward) should be first"
        );
    }

    #[test]
    fn test_has_observation_checks_existence() {
        let qt = shared_qtable();
        {
            let mut table = qt.lock().unwrap_or_else(|e| e.into_inner());
            let _ = table.update(5, 10, 0.5, 5, None, true);
        }
        let bridge = QTableBridge::new(qt);
        assert!(
            bridge.has_observation(5, 10),
            "Should detect observed state-action"
        );
        assert!(
            !bridge.has_observation(5, 99),
            "Should not detect unobserved action"
        );
        assert!(
            !bridge.has_observation(99, 10),
            "Should not detect unobserved state"
        );
    }

    #[test]
    fn test_top_k_actions_empty_for_unknown_state() {
        let bridge = QTableBridge::new(shared_qtable());
        let actions = bridge.top_k_actions(999, 10);
        assert!(actions.is_empty(), "Unknown state should have no actions");
    }

    #[test]
    fn test_action_visit_count_binary() {
        let qt = shared_qtable();
        {
            let mut table = qt.lock().unwrap_or_else(|e| e.into_inner());
            let _ = table.update(7, 42, 1.0, 7, None, true);
        }
        let bridge = QTableBridge::new(qt);
        assert_eq!(
            bridge.action_visit_count(7, 42),
            1,
            "Observed action should return 1"
        );
        assert_eq!(
            bridge.action_visit_count(7, 99),
            0,
            "Unobserved action should return 0"
        );
    }

    #[test]
    fn test_state_visit_count_reflects_known_actions() {
        let qt = shared_qtable();
        {
            let mut table = qt.lock().unwrap_or_else(|e| e.into_inner());
            let _ = table.update(3, 10, 1.0, 3, None, true);
            let _ = table.update(3, 20, 2.0, 3, None, true);
            let _ = table.update(3, 30, 3.0, 3, None, true);
        }
        let bridge = QTableBridge::new(qt);
        assert_eq!(
            bridge.state_visit_count(3),
            3,
            "State with 3 known actions should report visit count 3"
        );
    }
}
