//! Double Q-Learning to reduce overestimation bias (van Hasselt et al. 2015).
//!
//! Maintains two Q-tables: online (updated every step) and target (updated slowly).
//! Action SELECTION uses online; action EVALUATION uses target.
//!
//! Update rule:
//! a* = argmax Q_online(s', a)            (selection)
//! TD_target = r + γ · Q_target(s', a*)  (evaluation)
//! Q_online(s,a) += α · (TD_target - Q_online(s,a))
//!
//! Target sync: Q_target ← τ · Q_online + (1-τ) · Q_target  (soft update)

use super::qtable::{LearningParams, QLearning, QTable};

const DEFAULT_TAU: f64 = 0.01;
const DEFAULT_SYNC_EVERY: u64 = 100;

/// Double Q-Learning agent: online + target tables.
#[derive(Debug)]
pub struct DoubleQTable {
    /// Online Q-table updated every step and used for action selection.
    pub online: QTable,
    target: QTable,
    tau: f64,
    update_count: u64,
    sync_every: u64,
    params: LearningParams,
}

impl DoubleQTable {
    /// Create a double Q-learner with the given learning parameters and default sync schedule.
    pub fn new(params: LearningParams) -> Self {
        Self {
            online: QTable::with_params(params),
            target: QTable::with_params(params),
            tau: DEFAULT_TAU,
            update_count: 0,
            sync_every: DEFAULT_SYNC_EVERY,
            params,
        }
    }

    /// Create a double Q-learner with default learning parameters.
    pub fn with_defaults() -> Self {
        Self::new(LearningParams::default())
    }

    /// Override the soft-update rate `tau` (clamped to 0..=1) and the sync interval (min 1).
    pub fn with_sync(mut self, tau: f64, sync_every: u64) -> Self {
        self.tau = tau.clamp(0.0, 1.0);
        self.sync_every = sync_every.max(1);
        self
    }

    /// Double Q-Learning update. Returns TD error.
    ///
    /// Action selection uses online table; action evaluation uses target table.
    /// The computed td_target is applied directly to the online table by passing
    /// it as a reward with a terminal-like state (no known next actions), which
    /// causes QTable to set next_q=0 and compute: new_q = q + α·(td_target - q).
    pub fn double_update(
        &mut self,
        state: u64,
        action: u64,
        reward: f64,
        next_state: u64,
        terminal: bool,
    ) -> f64 {
        // Double Q selection: best action from online table
        let best_next = self.online.best_action(next_state);
        // Double Q evaluation: Q-value from target table
        let next_q_target = best_next.map_or(0.0, |a| self.target.get_q(next_state, a));
        let td_target = if terminal {
            reward
        } else {
            reward + self.params.gamma * next_q_target
        };
        let current_q = self.online.get_q(state, action);
        let td_error = td_target - current_q;

        // Apply update directly: new_q = current_q + alpha * td_error
        let new_q = current_q + self.params.alpha * td_error;
        self.online.load_q_value(state, action, new_q);

        self.update_count += 1;
        if self.update_count.is_multiple_of(self.sync_every) {
            self.sync_target();
        }
        td_error
    }

    /// Soft-update target: target = τ · online + (1-τ) · target.
    pub fn sync_target(&mut self) {
        let tau = self.tau;
        let online_vals: Vec<(u64, u64, f64)> = self.online.all_q_values();
        for (s, a, online_q) in online_vals {
            let target_q = self.target.get_q(s, a);
            self.target
                .load_q_value(s, a, tau * online_q + (1.0 - tau) * target_q);
        }
    }

    /// Number of updates applied to the online table so far.
    pub fn update_count(&self) -> u64 {
        self.update_count
    }

    /// Borrow the slowly-updated target Q-table.
    pub fn target(&self) -> &QTable {
        &self.target
    }
}

impl QLearning for DoubleQTable {
    fn update(
        &mut self,
        state: u64,
        action: u64,
        reward: f64,
        next_state: u64,
        _next_action: Option<u64>,
        terminal: bool,
    ) -> f64 {
        self.double_update(state, action, reward, next_state, terminal)
    }

    fn get_q(&self, state: u64, action: u64) -> f64 {
        self.online.get_q(state, action)
    }

    fn best_action(&self, state: u64) -> Option<u64> {
        self.online.best_action(state)
    }

    fn reset_traces(&mut self) {
        self.online.reset_traces();
        self.target.reset_traces();
    }

    fn get_params(&self) -> LearningParams {
        self.params
    }

    fn epsilon_greedy_action(&mut self, state: u64) -> Option<u64> {
        self.online.epsilon_greedy_action(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_update_finite_td_error() {
        let mut dq = DoubleQTable::with_defaults();
        for i in 0..50u64 {
            let td = dq.double_update(
                i % 5,
                i % 3,
                if i % 3 == 0 { 1.0 } else { 0.0 },
                (i + 1) % 5,
                false,
            );
            assert!(td.is_finite(), "TD error finite at step {i}");
        }
        assert_eq!(dq.update_count(), 50);
    }

    #[test]
    fn sync_with_tau_one_copies_online() {
        let mut dq = DoubleQTable::with_defaults().with_sync(1.0, 10);
        dq.online.load_q_value(0, 0, 0.75);
        dq.sync_target();
        let oq = dq.online.get_q(0, 0);
        let tq = dq.target().get_q(0, 0);
        assert!((oq - tq).abs() < 1e-9, "tau=1.0 → target == online");
    }

    #[test]
    fn ql_trait_works() {
        let mut dq = DoubleQTable::with_defaults();
        let td = dq.update(0, 0, 0.5, 1, None, false);
        assert!(td.is_finite());
    }

    #[test]
    fn terminal_uses_reward_only() {
        let mut dq = DoubleQTable::with_defaults().with_sync(1.0, 1_000);
        let td = dq.double_update(42, 7, 1.0, 99, true);
        // First visit: current_q = 0 → td_error = 1.0
        assert!(
            (td - 1.0).abs() < 1e-9,
            "terminal td_error should be 1.0, got {td}"
        );
    }
}
