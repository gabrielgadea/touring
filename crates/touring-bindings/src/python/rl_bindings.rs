//! PyO3 bindings for Reinforcement Learning (QTable + LinUCB).
//!
//! Exposes the touring-learning RL subsystem to Python as module-level functions.
//! Uses global singletons (OnceLock + Mutex) so Python callers can interact with
//! a persistent QTable and LinUCBBandit across calls without managing Rust lifetimes.
//!
//! # GIL Note
//!
//! These functions hold a Mutex while performing RL operations. The Mutex guard
//! is not `Send`, so `py.allow_threads()` cannot be used while the guard is held.
//! This is acceptable because the RL operations (QTable lookup, LinUCB scoring)
//! are O(d^2) at most and complete in microseconds.
//!
//! # Python API
//!
//! ```python
//! from claude_learning_kernel import (
//!     process_reward,      # QTable TD(lambda) update
//!     select_arm,          # LinUCB arm selection
//!     get_q_value,         # Read Q(state, action)
//!     get_best_action,     # argmax_a Q(state, a)
//!     get_linucb_arm_stats # Per-arm statistics
//! )
//! ```

use std::sync::{Mutex, OnceLock};

use ndarray::Array1;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use touring_intelligence::rl::bandit::{FEATURE_DIM, LinUCBBandit};
use touring_intelligence::rl::rl::{QLearning, QTable};

// ═══════════════════════════════════════════════════════════════════════════
// Global singletons — initialized once, accessed via Mutex
// ═══════════════════════════════════════════════════════════════════════════

/// Global QTable instance for TD(lambda) Q-learning.
static QTABLE: OnceLock<Mutex<QTable>> = OnceLock::new();

/// Global LinUCBBandit instance for contextual bandit arm selection.
static LINUCB: OnceLock<Mutex<LinUCBBandit>> = OnceLock::new();

/// Get or initialize the global QTable.
fn get_qtable() -> &'static Mutex<QTable> {
    QTABLE.get_or_init(|| Mutex::new(QTable::new()))
}

/// Get or initialize the global LinUCBBandit.
fn get_linucb() -> &'static Mutex<LinUCBBandit> {
    LINUCB.get_or_init(|| Mutex::new(LinUCBBandit::new()))
}

// ═══════════════════════════════════════════════════════════════════════════
// Python-exposed functions
// ═══════════════════════════════════════════════════════════════════════════

/// Process a reward signal through QTable TD(lambda) update.
///
/// Args:
///     state: State identifier (u64).
///     action: Action identifier (u64).
///     reward: Scalar reward in [-1.0, 1.0].
///     next_state: Successor state identifier.
///     terminal: Whether this is a terminal transition (default: False).
///
/// Returns:
///     The TD error (float), indicating learning progress.
#[pyfunction]
#[pyo3(signature = (state, action, reward, next_state, terminal = false))]
pub fn process_reward(
    _py: Python<'_>,
    state: u64,
    action: u64,
    reward: f64,
    next_state: u64,
    terminal: bool,
) -> PyResult<f64> {
    let mut qtable = get_qtable()
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("QTable lock poisoned: {e}")))?;

    // QTable update is O(n_traces) — fast enough to hold the GIL.
    let td_error = qtable.update(state, action, reward, next_state, None, terminal);
    Ok(td_error)
}

/// Select the best LinUCB arm for the given feature vector.
///
/// Args:
///     features: List of floats with exactly 19 dimensions (FEATURE_DIM).
///               See linucb.rs for the feature encoding scheme.
///
/// Returns:
///     Arm index (int) in [0, 7].
///
/// Raises:
///     RuntimeError: If features length != 19 or lock is poisoned.
#[pyfunction]
pub fn select_arm(_py: Python<'_>, features: Vec<f64>) -> PyResult<usize> {
    if features.len() != FEATURE_DIM {
        return Err(PyRuntimeError::new_err(format!(
            "Feature vector must have {FEATURE_DIM} dimensions, got {}",
            features.len()
        )));
    }

    let mut linucb = get_linucb()
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("LinUCB lock poisoned: {e}")))?;

    let features_arr = Array1::from_vec(features);
    // LinUCB select_arm is O(K * d^2) where K=8, d=19 — ~3000 FLOPs, sub-microsecond.
    let (arm_index, _score) = linucb.select_arm(&features_arr);
    Ok(arm_index)
}

/// Update the LinUCB arm with an observed reward (Sherman-Morrison rank-1 update).
///
/// Must be called after `select_arm` once the reward is known to close the
/// bandit learning loop. Without this call, `select_arm` explores but never
/// exploits — the covariance matrices and theta estimates stay at their priors.
///
/// Args:
///     arm_index: Arm index returned by `select_arm` (int in [0, 7]).
///     features:  Same 19-dim feature vector passed to `select_arm`.
///     reward:    Observed reward in [-1.0, 1.0].
///
/// Raises:
///     RuntimeError: If arm_index >= NUM_ARMS, features length != 19, or lock poisoned.
#[pyfunction]
pub fn update_arm(
    _py: Python<'_>,
    arm_index: usize,
    features: Vec<f64>,
    reward: f64,
) -> PyResult<()> {
    if arm_index >= touring_intelligence::rl::bandit::NUM_ARMS {
        return Err(PyRuntimeError::new_err(format!(
            "arm_index {arm_index} out of range [0, {})",
            touring_intelligence::rl::bandit::NUM_ARMS
        )));
    }
    if features.len() != FEATURE_DIM {
        return Err(PyRuntimeError::new_err(format!(
            "Feature vector must have {FEATURE_DIM} dimensions, got {}",
            features.len()
        )));
    }

    let mut linucb = get_linucb()
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("LinUCB lock poisoned: {e}")))?;

    let features_arr = Array1::from_vec(features);
    // LinUCB update is O(d^2) — Sherman-Morrison rank-1 update on the arm's A matrix.
    linucb.update(arm_index, &features_arr, reward);
    Ok(())
}

/// Get the Q-value for a specific state-action pair.
///
/// Args:
///     state: State identifier (u64).
///     action: Action identifier (u64).
///
/// Returns:
///     Q-value (float). Returns the initial Q-value (0.0) for unexplored pairs.
#[pyfunction]
pub fn get_q_value(_py: Python<'_>, state: u64, action: u64) -> PyResult<f64> {
    let qtable = get_qtable()
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("QTable lock poisoned: {e}")))?;

    Ok(qtable.get_q(state, action))
}

/// Get the best action (argmax Q) for a given state.
///
/// Args:
///     state: State identifier (u64).
///
/// Returns:
///     Best action (int) or None if no actions are known for the state.
#[pyfunction]
pub fn get_best_action(_py: Python<'_>, state: u64) -> PyResult<Option<u64>> {
    let qtable = get_qtable()
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("QTable lock poisoned: {e}")))?;

    Ok(qtable.best_action(state))
}

/// Get statistics for each LinUCB arm.
///
/// Returns:
///     List of dicts, each with keys:
///         - "arm_index": int (0-7)
///         - "pulls": int (number of times arm was selected)
///         - "avg_reward": float (average observed reward)
///         - "cumulative_reward": float (pulls * avg_reward)
#[pyfunction]
pub fn get_linucb_arm_stats(py: Python<'_>) -> PyResult<Vec<Py<PyDict>>> {
    let linucb = get_linucb()
        .lock()
        .map_err(|e| PyRuntimeError::new_err(format!("LinUCB lock poisoned: {e}")))?;

    let stats = linucb.arm_stats();

    let result = stats
        .iter()
        .map(|&(arm_index, pulls, avg_reward)| -> PyResult<Py<PyDict>> {
            let dict = PyDict::new(py);
            dict.set_item("arm_index", arm_index)?;
            dict.set_item("pulls", pulls)?;
            dict.set_item("avg_reward", avg_reward)?;
            dict.set_item("cumulative_reward", pulls as f64 * avg_reward)?;
            Ok(dict.into())
        })
        .collect::<PyResult<Vec<_>>>()?;

    Ok(result)
}

// ═══════════════════════════════════════════════════════════════════════════
// Module registration
// ═══════════════════════════════════════════════════════════════════════════

/// Register all RL bindings into the parent Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(process_reward, m)?)?;
    m.add_function(wrap_pyfunction!(select_arm, m)?)?;
    m.add_function(wrap_pyfunction!(update_arm, m)?)?;
    m.add_function(wrap_pyfunction!(get_q_value, m)?)?;
    m.add_function(wrap_pyfunction!(get_best_action, m)?)?;
    m.add_function(wrap_pyfunction!(get_linucb_arm_stats, m)?)?;

    // Expose constants for Python consumers
    m.add("FEATURE_DIM", FEATURE_DIM)?;
    m.add("NUM_ARMS", touring_intelligence::rl::bandit::NUM_ARMS)?;

    Ok(())
}
