//! IC-4: AcoRewardPropagator — TD(λ) backward propagation of RL rewards
//! into MCTS pheromone trails.
//!
//! # Algorithm
//!
//! Given a terminal reward `R` and a chronological history of `(state, action)`
//! pairs, backward eligibility traces assign a decayed portion of `R` to each
//! past step:
//!
//! ```text
//! decay = λ × γ
//!
//! For step i (0 = oldest, N−1 = newest):
//!   exponent      = (N − 1) − i          (0 for the newest step)
//!   eligibility   = decay ^ exponent      (1.0 for the newest step)
//!   deposit(state_i, action_i, R × eligibility)
//! ```
//!
//! # Parameters
//!
//! * `lambda` (λ) — eligibility trace decay:
//!   - `0.0` → only the **last** step receives reward
//!   - `1.0` → **uniform** reward across all steps (when γ = 1.0)
//! * `gamma` (γ) — future discount factor:
//!   - `1.0` → no discounting
//!   - `0.99` → typical RL discounting for long histories
//!
//! Both parameters are clamped to `[0.0, 1.0]` at construction.
//!
//! # Integration with TrackerReport
//!
//! [`AcoRewardPropagator::propagate_from_report`] bridges the 9-dimension ACO
//! tracker quality signal directly into MCTS pheromone, closing the learning
//! loop between code-generation quality and future search biases.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::rl::TrackerReport;

use crate::reasoning::mcts::MctsPheromonoLayer;

/// History window for adaptive lambda variance calculation.
const REWARD_HISTORY_CAPACITY: usize = 20;

/// IC-4: Propagates RL rewards backward into MCTS pheromone using TD(λ).
///
/// Holds a shared `MctsPheromonoLayer` so that rewards deposited here are
/// visible to any `CognitiveMCTS` or other component sharing the same layer.
/// INS-L10: `base_lambda` + `reward_history` enable adaptive λ via reward variance.
pub struct AcoRewardPropagator {
    /// Shared pheromone layer that receives TD(λ) deposits.
    pheromone: Arc<Mutex<MctsPheromonoLayer>>,
    /// INS-L10: Base eligibility trace coefficient λ ∈ [0.0, 1.0].
    base_lambda: f64,
    /// Future discount factor γ ∈ [0.0, 1.0].
    gamma: f64,
    /// INS-L10: Rolling window of recent rewards for variance-based adaptation.
    reward_history: Mutex<VecDeque<f64>>,
}

impl AcoRewardPropagator {
    /// Create a new `AcoRewardPropagator`.
    ///
    /// # Arguments
    ///
    /// * `pheromone` — shared pheromone layer to deposit rewards into.
    /// * `lambda` — base eligibility trace decay (clamped to `[0.0, 1.0]`).
    /// * `gamma` — future discount factor (clamped to `[0.0, 1.0]`).
    pub fn new(pheromone: Arc<Mutex<MctsPheromonoLayer>>, lambda: f64, gamma: f64) -> Self {
        Self {
            pheromone,
            base_lambda: lambda.clamp(0.0, 1.0),
            gamma: gamma.clamp(0.0, 1.0),
            reward_history: Mutex::new(VecDeque::with_capacity(REWARD_HISTORY_CAPACITY)),
        }
    }

    /// INS-L10: Compute adaptive λ based on variance of recent rewards.
    ///
    /// - variance > 0.5  → `base_lambda * 0.5`  (conservative: high volatility)
    /// - variance < 0.1  → `(base_lambda * 1.5).min(0.99)` (aggressive: stable env)
    /// - otherwise       → `base_lambda`
    pub fn adaptive_lambda(&self) -> f64 {
        let history = self
            .reward_history
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let n = history.len();
        if n < 2 {
            return self.base_lambda;
        }
        let mean = history.iter().sum::<f64>() / n as f64;
        let variance = history.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n as f64;
        if variance > 0.5 {
            self.base_lambda * 0.5
        } else if variance < 0.1 {
            (self.base_lambda * 1.5).min(0.99)
        } else {
            self.base_lambda
        }
    }

    /// Propagate `reward` backward through `history` using TD(λ) eligibility traces.
    ///
    /// `history` must be in **chronological order** (oldest pair first, newest last).
    /// Each `(state_hash, action_hash)` pair receives a decayed fraction of `reward`:
    /// the newest step is deposited with `reward × 1.0`; each prior step decays by
    /// `(λ × γ)` per step toward the oldest.
    ///
    /// Zero or negative rewards are silently ignored — pheromone only reinforces
    /// positive outcomes (consistent with [`MctsPheromonoLayer::deposit`]).
    /// INS-L10: Uses `adaptive_lambda()` and records the reward in `reward_history`.
    pub fn propagate(&self, reward: f64, history: &[(u64, u64)]) {
        // INS-L10: always record reward for variance tracking (even if non-positive).
        {
            let mut rh = self
                .reward_history
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if rh.len() >= REWARD_HISTORY_CAPACITY {
                rh.pop_front();
            }
            rh.push_back(reward);
        }

        if reward <= 0.0 || history.is_empty() {
            return;
        }
        let n = history.len();
        // INS-L10: use adaptive lambda instead of fixed base_lambda.
        let lambda = self.adaptive_lambda();
        let decay = lambda * self.gamma;
        let mut ph = self.pheromone.lock().unwrap_or_else(|e| e.into_inner());
        for (i, &(state, action)) in history.iter().enumerate() {
            // exponent = distance from the end: 0 for newest, n−1 for oldest.
            let exponent = (n - 1 - i) as u32;
            let eligibility = decay.powi(exponent as i32);
            ph.deposit(state, action, reward * eligibility);
        }
    }

    /// Convert `report` to an RL reward via [`TrackerReport::as_rl_reward`] and propagate.
    ///
    /// This bridges the ACO code-generation tracker (9-dimension quality signal)
    /// and the MCTS pheromone layer, so tracker outcomes directly shape future search.
    pub fn propagate_from_report(&self, report: &TrackerReport, history: &[(u64, u64)]) {
        self.propagate(report.as_rl_reward(), history);
    }

    /// Return the base eligibility trace coefficient (λ).
    pub fn lambda(&self) -> f64 {
        self.base_lambda
    }

    /// Return the discount factor (γ).
    pub fn gamma(&self) -> f64 {
        self.gamma
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::{TrackerReport, TrackerStatus};

    /// Build a propagator sharing a fresh pheromone layer.
    fn make(lambda: f64, gamma: f64) -> (AcoRewardPropagator, Arc<Mutex<MctsPheromonoLayer>>) {
        let layer = Arc::new(Mutex::new(MctsPheromonoLayer::new(0.1, 0.0)));
        let prop = AcoRewardPropagator::new(Arc::clone(&layer), lambda, gamma);
        (prop, layer)
    }

    fn strength(layer: &Arc<Mutex<MctsPheromonoLayer>>, state: u64, action: u64) -> f64 {
        layer.lock().unwrap().strength(state, action)
    }

    // ── TD(λ) deposit shape ────────────────────────────────────────────────

    #[test]
    fn test_newest_step_receives_full_reward() {
        let (prop, layer) = make(0.8, 0.95);
        let history = [(1u64, 10u64), (2u64, 20u64), (3u64, 30u64)];
        prop.propagate(1.0, &history);

        // Newest (3,30): exponent=0 → eligibility=1.0
        assert!(
            (strength(&layer, 3, 30) - 1.0).abs() < 1e-9,
            "newest step should receive full reward"
        );
    }

    #[test]
    fn test_reward_decays_toward_oldest() {
        let (prop, layer) = make(0.8, 0.95);
        let history = [(1u64, 10u64), (2u64, 20u64), (3u64, 30u64)];
        prop.propagate(1.0, &history);

        let s30 = strength(&layer, 3, 30); // newest — exponent 0
        let s20 = strength(&layer, 2, 20); // middle — exponent 1
        let s10 = strength(&layer, 1, 10); // oldest — exponent 2

        assert!(s30 > s20, "newest must beat middle: {s30} > {s20}");
        assert!(s20 > s10, "middle must beat oldest: {s20} > {s10}");
    }

    #[test]
    fn test_lambda_zero_only_last_step_receives_reward() {
        let (prop, layer) = make(0.0, 1.0); // decay = 0*1 = 0
        let history = [(1u64, 10u64), (2u64, 20u64)];
        prop.propagate(1.0, &history);

        // Oldest (1,10): exponent=1 → 0.0^1 = 0.0 → no deposit
        // Newest (2,20): exponent=0 → 0.0^0 = 1.0 → full deposit
        assert!(
            (strength(&layer, 2, 20) - 1.0).abs() < 1e-9,
            "newest step gets 1.0"
        );
        assert_eq!(
            strength(&layer, 1, 10),
            0.0,
            "oldest step gets 0.0 when lambda=0"
        );
    }

    #[test]
    fn test_lambda_one_gamma_one_uniform_reward() {
        let (prop, layer) = make(1.0, 1.0); // decay = 1.0 → all steps get 1.0
        let history = [(1u64, 10u64), (2u64, 20u64), (3u64, 30u64)];
        prop.propagate(1.0, &history);

        assert!((strength(&layer, 1, 10) - 1.0).abs() < 1e-9);
        assert!((strength(&layer, 2, 20) - 1.0).abs() < 1e-9);
        assert!((strength(&layer, 3, 30) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_single_step_history() {
        let (prop, layer) = make(0.8, 0.95);
        prop.propagate(2.5, &[(5u64, 50u64)]);
        assert!(
            (strength(&layer, 5, 50) - 2.5).abs() < 1e-9,
            "single step must receive full reward"
        );
    }

    // ── edge cases ─────────────────────────────────────────────────────────

    #[test]
    fn test_negative_reward_ignored() {
        let (prop, layer) = make(0.8, 0.95);
        prop.propagate(-1.0, &[(1u64, 10u64)]);
        assert_eq!(strength(&layer, 1, 10), 0.0);
    }

    #[test]
    fn test_zero_reward_ignored() {
        let (prop, layer) = make(0.8, 0.95);
        prop.propagate(0.0, &[(1u64, 10u64)]);
        assert_eq!(strength(&layer, 1, 10), 0.0);
    }

    #[test]
    fn test_empty_history_no_panic() {
        let (prop, _) = make(0.8, 0.95);
        prop.propagate(1.0, &[]); // must not panic
    }

    #[test]
    fn test_clamped_lambda_and_gamma() {
        let (prop, _) = make(2.0, -0.5); // both out of [0,1]
        assert!((prop.lambda() - 1.0).abs() < 1e-9, "lambda clamped to 1.0");
        assert!((prop.gamma() - 0.0).abs() < 1e-9, "gamma clamped to 0.0");
    }

    // ── TrackerReport integration ──────────────────────────────────────────

    #[test]
    fn test_propagate_from_report_pass_reward() {
        let (prop, layer) = make(1.0, 1.0);
        // Pass with composite=0.8 → as_rl_reward = 0.5 + 0.5 * 0.8 = 0.9
        let report = TrackerReport {
            dims: vec![],
            composite: 0.8,
            status: TrackerStatus::Pass,
            iteration: 1,
        };
        prop.propagate_from_report(&report, &[(1u64, 10u64)]);
        assert!(
            (strength(&layer, 1, 10) - 0.9).abs() < 1e-6,
            "Pass composite=0.8 → reward=0.9, got {}",
            strength(&layer, 1, 10)
        );
    }

    #[test]
    fn test_propagate_from_report_halt_no_deposit() {
        let (prop, layer) = make(1.0, 1.0);
        // Halt → as_rl_reward = 0.0 → no deposit
        let report = TrackerReport {
            dims: vec![],
            composite: 0.0,
            status: TrackerStatus::Halt,
            iteration: 1,
        };
        prop.propagate_from_report(&report, &[(1u64, 10u64)]);
        assert_eq!(strength(&layer, 1, 10), 0.0, "Halt reward=0.0 → no deposit");
    }

    #[test]
    fn test_propagate_from_report_veto_partial_deposit() {
        let (prop, layer) = make(1.0, 1.0);
        // Veto with composite=0.5 → as_rl_reward = 0.1 + 0.4 * 0.5 = 0.3
        let report = TrackerReport {
            dims: vec![],
            composite: 0.5,
            status: TrackerStatus::Veto,
            iteration: 1,
        };
        prop.propagate_from_report(&report, &[(2u64, 20u64)]);
        assert!(
            (strength(&layer, 2, 20) - 0.3).abs() < 1e-6,
            "Veto composite=0.5 → reward=0.3, got {}",
            strength(&layer, 2, 20)
        );
    }

    // ── accessor methods ───────────────────────────────────────────────────

    #[test]
    fn test_accessors() {
        let (prop, _) = make(0.7, 0.9);
        assert!((prop.lambda() - 0.7).abs() < 1e-9);
        assert!((prop.gamma() - 0.9).abs() < 1e-9);
    }

    // ── E2E integration: IC-1 ↔ IC-4 shared pheromone loop ────────────────
    //
    // These tests prove that `CognitiveMCTS` and `AcoRewardPropagator` can
    // share the *same* `MctsPheromonoLayer` via `Arc<Mutex<>>`, closing the
    // ACO feedback loop: MCTS search biases future exploration, and TD(λ)
    // backward propagation of quality signals further reinforces those trails.
    //
    // IMPORTANT: child_state = parent_state.wrapping_mul(31).wrapping_add(action).
    // Deposits occur at (child_state, action), NOT (root_state, action).
    // Computed values used below:
    //   root=0,  action=2  → child_state = 0*31+2  =   2
    //   root=10, action=20 → child_state = 10*31+20 = 330
    //   root=7,  action=77 → child_state = 7*31+77  = 294

    #[test]
    fn test_e2e_mcts_and_propagator_share_pheromone_layer() {
        use crate::reasoning::mcts::{MCTSConfig, PheromoneMCTS};

        // Shared layer: alpha=1.0 (full deposit), no evaporation.
        let shared = Arc::new(Mutex::new(MctsPheromonoLayer::new(1.0, 0.0)));

        // IC-1: CognitiveMCTS wired to the shared layer.
        let mut mcts = PheromoneMCTS::with_shared_pheromone(
            MCTSConfig {
                num_rollouts: 10,
                max_depth: 1,
                ..MCTSConfig::default()
            },
            Arc::clone(&shared),
        );

        // IC-4: AcoRewardPropagator also wired to the same shared layer.
        let propagator = AcoRewardPropagator::new(Arc::clone(&shared), 1.0, 1.0);

        // MCTS search: action=2 earns reward 1.0, action=1 earns 0.0.
        // Deposit lands at (child_state=2, action=2) since 0*31+2=2.
        mcts.search_with_pheromone(
            0,
            |_| vec![1u64, 2u64],
            |_state, action| if action == 2 { 1.0 } else { 0.0 },
        );

        // entry_count > 0 confirms the shared Arc received deposits.
        let entry_count = mcts.pheromone_entry_count();
        assert!(
            entry_count > 0,
            "MCTS must have deposited at least one entry; count = {entry_count}"
        );

        // Pheromone at (2, 2) — child of root=0 via action=2.
        let mcts_strength = mcts.pheromone_strength(2, 2);
        assert!(
            mcts_strength > 0.0,
            "MCTS must deposit at (child=2, action=2); got {mcts_strength}"
        );

        // TD(λ) propagation adds 0.8 to the SAME (2,2) entry via the shared Arc.
        propagator.propagate(0.8, &[(2u64, 2u64)]);

        let final_strength = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .strength(2, 2);
        assert!(
            final_strength > mcts_strength,
            "propagator must augment MCTS trail: {final_strength} > {mcts_strength}"
        );
    }

    #[test]
    fn test_e2e_propagate_from_report_augments_mcts_trail() {
        use crate::reasoning::mcts::{MCTSConfig, PheromoneMCTS};

        let shared = Arc::new(Mutex::new(MctsPheromonoLayer::new(1.0, 0.0)));

        let mut mcts = PheromoneMCTS::with_shared_pheromone(
            MCTSConfig {
                num_rollouts: 5,
                max_depth: 1,
                ..MCTSConfig::default()
            },
            Arc::clone(&shared),
        );
        let propagator = AcoRewardPropagator::new(Arc::clone(&shared), 1.0, 1.0);

        // root=10, action=20 → child_state = 10*31+20 = 330.
        // Deposit at (330, 20) from each rewarded rollout.
        mcts.search_with_pheromone(10, |_| vec![20u64], |_s, _a| 1.0);
        let baseline = mcts.pheromone_strength(330, 20);
        assert!(
            baseline > 0.0,
            "baseline must be positive after MCTS; got {baseline}"
        );

        // Pass with composite=1.0 → as_rl_reward = 0.5 + 0.5*1.0 = 1.0 → full deposit.
        let report = TrackerReport {
            dims: vec![],
            composite: 1.0,
            status: TrackerStatus::Pass,
            iteration: 1,
        };
        propagator.propagate_from_report(&report, &[(330u64, 20u64)]);

        let final_strength = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .strength(330, 20);
        assert!(
            final_strength > baseline,
            "propagate_from_report must increase strength: {final_strength} > {baseline}"
        );
    }

    #[test]
    fn test_e2e_halt_report_leaves_mcts_trail_unchanged() {
        use crate::reasoning::mcts::{MCTSConfig, PheromoneMCTS};

        let shared = Arc::new(Mutex::new(MctsPheromonoLayer::new(1.0, 0.0)));

        let mut mcts = PheromoneMCTS::with_shared_pheromone(
            MCTSConfig {
                num_rollouts: 5,
                max_depth: 1,
                ..MCTSConfig::default()
            },
            Arc::clone(&shared),
        );
        let propagator = AcoRewardPropagator::new(Arc::clone(&shared), 1.0, 1.0);

        // root=7, action=77 → child_state = 7*31+77 = 294.
        mcts.search_with_pheromone(7, |_| vec![77u64], |_s, _a| 1.0);
        let baseline = mcts.pheromone_strength(294, 77);
        assert!(baseline > 0.0, "baseline must be positive; got {baseline}");

        // Halt → as_rl_reward = 0.0 → propagate is a no-op.
        let report = TrackerReport {
            dims: vec![],
            composite: 0.0,
            status: TrackerStatus::Halt,
            iteration: 1,
        };
        propagator.propagate_from_report(&report, &[(294u64, 77u64)]);

        let unchanged = shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .strength(294, 77);
        assert!(
            (unchanged - baseline).abs() < 1e-12,
            "Halt must not modify the pheromone trail: expected {baseline}, got {unchanged}"
        );
    }
}
