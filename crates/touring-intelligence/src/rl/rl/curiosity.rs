//! Count-based intrinsic curiosity bonus.
//!
//! Bonus = β / sqrt(n_visits + 1), capped at MAX_BONUS.
//! Encourages exploration of under-visited states.

use rustc_hash::FxHashMap;

const DEFAULT_BETA: f64 = 0.1;
const MAX_BONUS: f64 = 0.5;

/// Count-based intrinsic curiosity module.
#[derive(Debug, Clone)]
pub struct CuriosityModule {
    visit_counts: FxHashMap<u64, u64>,
    beta: f64,
    max_bonus: f64,
    total_visits: u64,
}

impl CuriosityModule {
    /// Create a curiosity module with the given exploration weight `beta` (clamped to non-negative).
    pub fn new(beta: f64) -> Self {
        Self {
            visit_counts: FxHashMap::default(),
            beta: beta.max(0.0),
            max_bonus: MAX_BONUS,
            total_visits: 0,
        }
    }

    /// Create a curiosity module with the default exploration weight.
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_BETA)
    }

    /// Visit a state and return the intrinsic curiosity bonus.
    ///
    /// Increments visit count and returns `β / sqrt(n + 1)`, capped at `max_bonus`.
    pub fn bonus(&mut self, state: u64) -> f64 {
        let count = self.visit_counts.entry(state).or_insert(0);
        *count += 1;
        self.total_visits += 1;
        let n = *count;
        let b = self.beta / (n as f64 + 1.0).sqrt();
        b.min(self.max_bonus)
    }

    /// Visit count for a state (without updating).
    pub fn visit_count(&self, state: u64) -> u64 {
        self.visit_counts.get(&state).copied().unwrap_or(0)
    }

    /// Total number of state visits recorded.
    pub fn total_visits(&self) -> u64 {
        self.total_visits
    }

    /// Number of distinct states visited at least once.
    pub fn unique_states(&self) -> usize {
        self.visit_counts.len()
    }

    /// The exploration weight `beta` in use.
    pub fn beta(&self) -> f64 {
        self.beta
    }
}

impl Default for CuriosityModule {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonus_decreases_with_visits() {
        let mut c = CuriosityModule::new(0.1);
        let b1 = c.bonus(42);
        let b2 = c.bonus(42);
        assert!(b2 < b1, "bonus should decrease with more visits");
    }

    #[test]
    fn novel_state_gets_max_bonus_when_beta_large() {
        let mut c = CuriosityModule::new(10.0);
        let b = c.bonus(99);
        assert_eq!(b, MAX_BONUS, "first visit with large beta should hit cap");
    }

    #[test]
    fn zero_beta_returns_zero() {
        let mut c = CuriosityModule::new(0.0);
        assert_eq!(c.bonus(1), 0.0);
    }

    #[test]
    fn visit_count_tracks_correctly() {
        let mut c = CuriosityModule::with_defaults();
        c.bonus(7);
        c.bonus(7);
        c.bonus(7);
        assert_eq!(c.visit_count(7), 3);
        assert_eq!(c.visit_count(99), 0);
    }

    #[test]
    fn total_and_unique_states() {
        let mut c = CuriosityModule::with_defaults();
        c.bonus(1);
        c.bonus(2);
        c.bonus(1);
        assert_eq!(c.total_visits(), 3);
        assert_eq!(c.unique_states(), 2);
    }
}
