//! ACO Pheromone Layer traits — formalizes the interface for pheromone-based
//! ACO (Ant Colony Optimization) layers so any implementation can be shared
//! across touring-cortex, touring-hooks, and touring-learning.
//!
//! Pln2 Phase 2 — S6: ACO PheromoneLayer trait formalization
//!
//! Two implementations exist and must remain separate (different key types):
//! - `MctsPheromonoLayer` in `touring-cognitive` —
//!   keyed by `(u64, u64)` state/action hashes for MCTS UCB augmentation.
//! - `UnifiedPheromoneBus` in `touring-learning` —
//!   keyed by `PheroKey` enum for multi-dimension
//!   ACO tracking (file paths, task IDs, teammate IDs, etc.).
//!
//! This trait is provided so both can be accessed through a common interface
//! where appropriate, without forcing them to share the same key space.

use std::sync::Arc;
use std::sync::Mutex;

/// ACO pheromone layer — deposit, query, and evaporate pheromone trails.
///
/// Implementors store pheromone as `Arc<Mutex<>>` so the layer can be shared
/// across async boundaries and between components (e.g. MCTS engine +
/// AcoRewardPropagator).
///
/// # Why two implementations with different keys
///
/// `MctsPheromonoLayer` uses raw `u64` hashes because it operates inside the
/// MCTS hot loop where string keys would add allocation overhead on every
/// rollout. `UnifiedPheromoneBus` uses the typed `PheroKey` enum because it
/// bridges multiple subsystems (file heat, task completion, teammate routing)
/// where collision-free typed keys prevent accidental cross-contamination.
/// Forcing these into a single key type would compromise both performance
/// (for MCTS) and safety (for the multi-subsystem bus).
pub trait PheromoneLayer: Send + Sync {
    /// Deposit `reward` units of pheromone on `(state, action)`.
    ///
    /// Zero or negative rewards are silently ignored (only positive
    /// reinforcement is deposited).
    fn deposit(&mut self, state: u64, action: u64, reward: f64);

    /// Return the current pheromone strength for `(state, action)`.
    ///
    /// Returns `0.0` when no trail exists for the pair.
    fn strength(&self, state: u64, action: u64) -> f64;

    /// Apply exponential decay to all trails.
    ///
    /// Implementations may also prune entries below a strength threshold
    /// to keep memory bounded.
    fn evaporate(&mut self);

    /// Total number of (state, action) pairs currently tracked.
    fn entry_count(&self) -> usize;

    /// Return `base_ucb + alpha * strength(state, action)`.
    ///
    /// Provides the UCB augmentation used by cognitive MCTS.
    /// The `alpha` parameter controls pheromone influence weight.
    fn augment_ucb(&self, state: u64, action: u64, base_ucb: f64, alpha: f64) -> f64 {
        base_ucb + alpha * self.strength(state, action)
    }
}

/// Smart pointer alias for a shared, synchronizable pheromone layer.
pub type SharedPheromoneLayer = Arc<Mutex<dyn PheromoneLayer>>;
