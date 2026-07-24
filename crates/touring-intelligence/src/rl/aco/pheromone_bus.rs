//! TRANSFORMADOR #1: UnifiedPheromoneBus — shared pheromone backbone.
//!
//! Provides a single `Arc<Mutex<PheromoneBusInner>>` that HeatMap, MctsPheromoneLayer,
//! and TemplateLibrary can all deposit to and read from.
//! Keys are typed via `PheroKey` to avoid collisions across subsystems.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Key type for the unified pheromone bus.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum PheroKey {
    /// Heat pheromone for a file path (used by HeatMap / IncrementalPipeline).
    FilePath(String),
    /// MCTS pheromone for a (state_hash, action_hash) pair.
    ActionPair(u64, u64),
    /// Template pheromone for a named template (used by TemplateLibrary).
    TemplateId(String),
    /// N1: Pheromone for a task completion heat (team orchestration).
    /// High value = task frequently completed successfully.
    TaskId(String),
    /// N1: Pheromone for a teammate's success heat (team orchestration).
    /// High value = teammate reliably completes tasks.
    TeammateId(String),
    /// N1: Pheromone for limbo pattern detection (team orchestration).
    /// Accumulates negative heat when a teammate idles with uncompleted tasks.
    /// Key encodes teammate_id for per-teammate limbo tracking.
    LimboPattern(String),
}

#[derive(Debug, Default)]
struct PheromoneBusInner {
    trails: HashMap<PheroKey, f64>,
    evaporation_rate: f64,
}

/// Shared pheromone bus shared by all subsystems.
///
/// Cheap to clone — all clones share the same inner state via `Arc<Mutex<>>`.
#[derive(Clone, Debug)]
pub struct UnifiedPheromoneBus {
    inner: Arc<Mutex<PheromoneBusInner>>,
}

impl UnifiedPheromoneBus {
    /// Create a new bus with the given global evaporation rate.
    pub fn new(evaporation_rate: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PheromoneBusInner {
                trails: HashMap::new(),
                evaporation_rate: evaporation_rate.clamp(0.0, 1.0),
            })),
        }
    }

    /// Set the global evaporation rate (0.0 to 1.0).
    pub fn set_evaporation_rate(&self, rate: f64) {
        let mut bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        bus.evaporation_rate = rate.clamp(0.0, 1.0);
    }

    /// Get the current evaporation rate.
    ///
    /// Used internally by [`evaporate_all()`](Self::evaporate_all) and available
    /// externally for metrics and monitoring via `EvaporationRateMetrics`.
    pub fn evaporation_rate(&self) -> f64 {
        let bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        bus.evaporation_rate
    }

    /// Deposit `delta` pheromone on `key`.
    pub fn deposit(&self, key: PheroKey, delta: f64) {
        let mut bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        bus.trails
            .entry(key)
            .and_modify(|v| *v += delta)
            .or_insert(delta);
    }

    /// Return the current pheromone level for `key` (0.0 if absent).
    pub fn get(&self, key: &PheroKey) -> f64 {
        let bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        bus.trails.get(key).copied().unwrap_or(0.0)
    }

    /// Apply global evaporation to all trails.
    ///
    /// Entries that decay to ≤ 0.0 are pruned to keep memory bounded.
    pub fn evaporate_all(&self) {
        let mut bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        let rate = bus.evaporation_rate;
        bus.trails.values_mut().for_each(|v| *v *= 1.0 - rate);
        bus.trails.retain(|_, v| *v > 0.0);
    }

    /// Return the top-k entries by pheromone strength (descending).
    pub fn top_k(&self, k: usize) -> Vec<(PheroKey, f64)> {
        let bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        let mut entries: Vec<(PheroKey, f64)> =
            bus.trails.iter().map(|(k, v)| (k.clone(), *v)).collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        entries.truncate(k);
        entries
    }

    /// Number of active trail entries.
    pub fn entry_count(&self) -> usize {
        let bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        bus.trails.len()
    }

    /// Snapshot all current pheromone values (unordered).
    ///
    /// Used for drift detection — compare snapshots before/after evaporation.
    pub fn snapshot_values(&self) -> Vec<f64> {
        let bus = self
            .inner
            .lock()
            .expect("UnifiedPheromoneBus: lock poisoned");
        bus.trails.values().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_and_get() {
        let bus = UnifiedPheromoneBus::new(0.1);
        bus.deposit(PheroKey::TemplateId("tpl_1".into()), 0.5);
        assert!((bus.get(&PheroKey::TemplateId("tpl_1".into())) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_deposit_accumulates() {
        let bus = UnifiedPheromoneBus::new(0.0);
        bus.deposit(PheroKey::ActionPair(1, 2), 1.0);
        bus.deposit(PheroKey::ActionPair(1, 2), 0.5);
        assert!((bus.get(&PheroKey::ActionPair(1, 2)) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn test_evaporate_all_decays() {
        let bus = UnifiedPheromoneBus::new(0.5);
        bus.deposit(PheroKey::FilePath("src/lib.rs".into()), 1.0);
        bus.evaporate_all();
        let v = bus.get(&PheroKey::FilePath("src/lib.rs".into()));
        assert!((v - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_top_k_sorted() {
        let bus = UnifiedPheromoneBus::new(0.0);
        bus.deposit(PheroKey::TemplateId("a".into()), 3.0);
        bus.deposit(PheroKey::TemplateId("b".into()), 1.0);
        bus.deposit(PheroKey::TemplateId("c".into()), 2.0);
        let top = bus.top_k(2);
        assert_eq!(top.len(), 2);
        assert!((top[0].1 - 3.0).abs() < 1e-9);
        assert!((top[1].1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_clone_shares_state() {
        let bus = UnifiedPheromoneBus::new(0.0);
        let bus2 = bus.clone();
        bus.deposit(PheroKey::TemplateId("shared".into()), 7.0);
        assert!((bus2.get(&PheroKey::TemplateId("shared".into())) - 7.0).abs() < 1e-9);
    }
}
