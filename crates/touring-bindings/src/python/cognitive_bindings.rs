//! PyO3 bindings for touring-cognitive: MCTS search, GoT engine, SemanticGraph.

use pyo3::prelude::*;
use touring_intelligence::reasoning::mcts::{MCTSConfig, MCTSEngine};

/// Run MCTS search with configurable rollouts and depth.
///
/// Args:
///     root_state: Initial state identifier (u64).
///     actions: List of possible action IDs for expansion.
///     num_rollouts: Number of MCTS rollout iterations.
///     max_depth: Maximum search depth.
///
/// Returns:
///     Dict with best_action, confidence, value, tree_depth, total_rollouts.
///     None if no actions available.
#[pyfunction]
pub fn py_mcts_search(
    root_state: u64,
    actions: Vec<u64>,
    num_rollouts: usize,
    max_depth: usize,
) -> Option<String> {
    let config = MCTSConfig {
        num_rollouts,
        max_depth,
        ..MCTSConfig::default()
    };
    let engine = MCTSEngine::new(config);

    let actions_clone = actions.clone();
    let expand_fn = move |_state: u64| -> Vec<u64> { actions_clone.clone() };
    let reward_fn = |_state: u64, action: u64| -> f64 {
        // Default reward: normalized position in action list
        0.5 + 0.5
            * (1.0 / (1.0 + action.wrapping_sub(actions.first().copied().unwrap_or(0)) as f64))
    };

    let result = engine.search(root_state, expand_fn, reward_fn)?;
    Some(
        serde_json::json!({
            "best_action": result.best_action,
            "confidence": result.confidence,
            "value": result.value,
            "tree_depth": result.tree_depth,
            "total_rollouts": result.total_rollouts,
        })
        .to_string(),
    )
}

/// Register cognitive bindings on the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_mcts_search, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcts_search_basic() {
        let result = py_mcts_search(0, vec![1, 2, 3], 10, 3);
        assert!(result.is_some());
        let val = result.unwrap();
        assert!(val.contains("best_action"));
        assert!(val.contains("confidence"));
    }

    #[test]
    fn test_mcts_search_empty_actions() {
        let result = py_mcts_search(0, vec![], 10, 3);
        // Empty actions → expand_fn returns [] → no result
        assert!(result.is_none());
    }
}
