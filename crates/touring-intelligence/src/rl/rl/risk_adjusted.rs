//! Risk-adjusted Q-learning that modulates exploration by blast radius.
//!
//! High blast-radius operations (editing a symbol referenced by many files) are
//! inherently riskier. This module wraps [`QTable`] to reduce exploration (lower
//! epsilon) when the blast radius is high, forcing the agent to exploit known-safe
//! actions rather than exploring potentially destructive ones.
//!
//! # Risk model
//!
//! ```text
//! effective_epsilon = base_epsilon * risk_factor
//! risk_factor = 1.0 / (1.0 + (blast_radius / risk_threshold))
//! ```
//!
//! - `blast_radius = 0` => `risk_factor = 1.0` => full exploration
//! - `blast_radius = risk_threshold` => `risk_factor = 0.5` => half exploration
//! - `blast_radius >> risk_threshold` => `risk_factor -> 0` => nearly pure exploitation

use super::qtable::{LearningParams, QLearning, QTable};

/// Default blast-radius threshold above which risk is considered HIGH.
const DEFAULT_RISK_THRESHOLD: usize = 10;

/// Risk-adjusted Q-learning agent.
///
/// Wraps a [`QTable`] and adjusts the exploration rate (epsilon) based on the
/// blast radius of the current operation. Higher blast radius => lower epsilon
/// => more conservative (exploit known-safe actions).
#[derive(Debug)]
pub struct RiskAdjustedQLearning {
    /// Underlying Q-table for value estimation and updates.
    pub qtable: QTable,
    /// Blast radius above this value is considered HIGH risk.
    /// Controls the sigmoid-like scaling of epsilon.
    pub risk_threshold: usize,
}

impl RiskAdjustedQLearning {
    /// Create a new risk-adjusted Q-learner with default risk threshold.
    ///
    /// Uses `DEFAULT_RISK_THRESHOLD` (10) as the inflection point.
    pub fn new(params: LearningParams) -> Self {
        Self {
            qtable: QTable::with_params(params),
            risk_threshold: DEFAULT_RISK_THRESHOLD,
        }
    }

    /// Create with a custom risk threshold.
    ///
    /// # Arguments
    ///
    /// * `params` - Q-learning hyperparameters
    /// * `risk_threshold` - Blast radius value that halves the exploration rate.
    ///   Must be > 0 (clamped to 1 if zero).
    pub fn with_threshold(params: LearningParams, risk_threshold: usize) -> Self {
        Self {
            qtable: QTable::with_params(params),
            risk_threshold: risk_threshold.max(1),
        }
    }

    /// Compute exploration rate adjusted by blast radius.
    ///
    /// Uses a hyperbolic decay: `epsilon * threshold / (threshold + blast_radius)`.
    ///
    /// - `blast_radius = 0` => returns base epsilon unchanged
    /// - `blast_radius = threshold` => returns `epsilon / 2`
    /// - `blast_radius = 2 * threshold` => returns `epsilon / 3`
    ///
    /// This is smoother than a hard cutoff and gives proportional risk reduction.
    pub fn epsilon_with_risk(&self, blast_radius: usize) -> f64 {
        let base_epsilon = self.qtable.get_params().epsilon;
        let threshold = self.risk_threshold as f64;
        let risk_factor = threshold / (threshold + blast_radius as f64);
        base_epsilon * risk_factor
    }

    /// Select an action considering blast-radius risk.
    ///
    /// Temporarily adjusts the QTable's epsilon to the risk-modulated value,
    /// selects an action via epsilon-greedy, then restores the original epsilon
    /// decay progression.
    ///
    /// Returns `None` if no actions are known for the state.
    pub fn select_action(&mut self, state: u64, blast_radius: usize) -> Option<u64> {
        let original_epsilon = self.qtable.get_params().epsilon;
        let risk_epsilon = self.epsilon_with_risk(blast_radius);

        // Temporarily set risk-adjusted epsilon
        self.qtable.set_epsilon(risk_epsilon);

        let action = self.qtable.epsilon_greedy_action(state);

        // Restore the original epsilon (the QTable's internal decay already
        // applied during epsilon_greedy_action, so we restore the pre-risk value
        // to keep the decay schedule on track)
        let decayed = self.qtable.get_params().epsilon;
        // The decay ratio that was applied: decayed_risk / risk_epsilon
        // Apply same ratio to original: original * (decayed_risk / risk_epsilon)
        if risk_epsilon > 0.0 {
            let decay_ratio = decayed / risk_epsilon;
            self.qtable.set_epsilon(original_epsilon * decay_ratio);
        } else {
            self.qtable.set_epsilon(original_epsilon);
        }

        action
    }

    /// Delegate TD(lambda) update to the underlying QTable.
    ///
    /// Returns the TD error for monitoring.
    #[must_use = "TD error indicates learning progress"]
    pub fn update(
        &mut self,
        state: u64,
        action: u64,
        reward: f64,
        next_state: u64,
        next_action: u64,
        terminal: bool,
    ) -> f64 {
        self.qtable.update(
            state,
            action,
            reward,
            next_state,
            Some(next_action),
            terminal,
        )
    }

    /// Get the current risk threshold.
    pub fn risk_threshold(&self) -> usize {
        self.risk_threshold
    }

    /// Set a new risk threshold. Clamped to minimum of 1.
    pub fn set_risk_threshold(&mut self, threshold: usize) {
        self.risk_threshold = threshold.max(1);
    }

    /// Get current (non-risk-adjusted) epsilon from the underlying QTable.
    pub fn base_epsilon(&self) -> f64 {
        self.qtable.get_params().epsilon
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::nonminimal_bool)] // vecs asserted non-empty; || true guards against flaky assertions
    use super::*;

    fn default_params() -> LearningParams {
        LearningParams {
            epsilon: 0.3,
            epsilon_decay: 1.0, // no decay for deterministic tests
            epsilon_min: 0.01,
            ..LearningParams::default()
        }
    }

    #[test]
    fn zero_blast_radius_preserves_epsilon() {
        let rl = RiskAdjustedQLearning::new(default_params());
        let eps = rl.epsilon_with_risk(0);
        assert!(
            (eps - 0.3).abs() < f64::EPSILON,
            "zero blast_radius should preserve epsilon, got {eps}"
        );
    }

    #[test]
    fn high_blast_radius_reduces_epsilon() {
        let rl = RiskAdjustedQLearning::with_threshold(default_params(), 10);

        let eps_low = rl.epsilon_with_risk(1);
        let eps_mid = rl.epsilon_with_risk(10);
        let eps_high = rl.epsilon_with_risk(100);

        // Monotonically decreasing
        assert!(
            eps_low > eps_mid,
            "eps_low={eps_low} should > eps_mid={eps_mid}"
        );
        assert!(
            eps_mid > eps_high,
            "eps_mid={eps_mid} should > eps_high={eps_high}"
        );

        // At threshold, epsilon should be halved
        assert!(
            (eps_mid - 0.15).abs() < 0.01,
            "at threshold, epsilon should be ~half: got {eps_mid}"
        );
    }

    #[test]
    fn very_high_blast_radius_nearly_zero_epsilon() {
        let rl = RiskAdjustedQLearning::with_threshold(default_params(), 10);
        let eps = rl.epsilon_with_risk(10_000);
        assert!(
            eps < 0.001,
            "very high blast radius should yield near-zero epsilon, got {eps}"
        );
    }

    #[test]
    fn threshold_clamped_to_minimum_one() {
        let rl = RiskAdjustedQLearning::with_threshold(default_params(), 0);
        assert_eq!(rl.risk_threshold(), 1);
    }

    #[test]
    fn select_action_returns_none_for_unknown_state() {
        let mut rl = RiskAdjustedQLearning::new(default_params());
        let action = rl.select_action(9999, 5);
        assert!(action.is_none());
    }

    #[test]
    fn update_and_select_cycle() {
        let mut rl = RiskAdjustedQLearning::new(default_params());

        // Seed some state-action pairs via update
        let td1 = rl.update(0, 1, 1.0, 1, 2, false);
        let td2 = rl.update(0, 2, 0.5, 1, 1, false);

        // TD errors should be non-zero (learning happened)
        assert!(td1.abs() > 0.0, "first TD error should be non-zero");
        assert!(td2.is_finite(), "second TD error should be finite");

        // Now select should return a known action
        let action = rl.select_action(0, 5);
        assert!(action.is_some(), "should find action for seeded state");
        let a = action.unwrap();
        assert!(a == 1 || a == 2, "action should be one of the seeded ones");
    }

    #[test]
    fn set_risk_threshold_works() {
        let mut rl = RiskAdjustedQLearning::new(default_params());
        rl.set_risk_threshold(50);
        assert_eq!(rl.risk_threshold(), 50);

        rl.set_risk_threshold(0);
        assert_eq!(rl.risk_threshold(), 1, "should clamp to 1");
    }
}
