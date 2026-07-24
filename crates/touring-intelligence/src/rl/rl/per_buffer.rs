//! Prioritized Experience Replay (PER) buffer.
//!
//! Schaul et al. 2016 — transitions sampled proportional to |TD error| + ε.
//! Higher priority = sampled more often, improving sample efficiency.
//!
//! Priority: p_i = (|td_error| + EPSILON_PER)^ALPHA_PER
//! IS weight: w_i = (N · P(i))^{-BETA_PER}, normalized by max weight.

use rustc_hash::FxHashMap;

const EPSILON_PER: f64 = 0.01;
const ALPHA_PER: f64 = 0.6;
const BETA_PER: f64 = 0.4;

/// A single transition with priority weight.
#[derive(Debug, Clone)]
pub struct PrioritizedTransition {
    /// Encoded state before the action.
    pub state: u64,
    /// Action taken in `state`.
    pub action: u64,
    /// Reward received for the transition.
    pub reward: f64,
    /// Encoded state reached after the action.
    pub next_state: u64,
    /// Sampling priority derived from the TD error.
    pub priority: f64,
    /// Buffer index/slot of this transition.
    pub idx: usize,
}

/// Prioritized experience replay buffer.
#[derive(Debug)]
pub struct PrioritizedReplayBuffer {
    transitions: FxHashMap<usize, PrioritizedTransition>,
    capacity: usize,
    next_idx: usize,
    max_priority: f64,
}

impl PrioritizedReplayBuffer {
    /// Create a buffer holding up to `capacity` transitions (clamped to at least 1).
    pub fn new(capacity: usize) -> Self {
        Self {
            transitions: FxHashMap::default(),
            capacity: capacity.max(1),
            next_idx: 0,
            max_priority: 1.0,
        }
    }

    /// Create a buffer with the default capacity (1000).
    pub fn with_defaults() -> Self {
        Self::new(1000)
    }

    /// Push a transition. New transitions receive max_priority so they are sampled at least once.
    pub fn push(&mut self, state: u64, action: u64, reward: f64, next_state: u64) -> usize {
        let idx = self.next_idx;
        self.next_idx = self.next_idx.wrapping_add(1);
        if self.transitions.len() >= self.capacity {
            if let Some(&oldest) = self.transitions.keys().min() {
                self.transitions.remove(&oldest);
            }
        }
        self.transitions.insert(
            idx,
            PrioritizedTransition {
                state,
                action,
                reward,
                next_state,
                priority: self.max_priority,
                idx,
            },
        );
        idx
    }

    /// Update priority after computing TD error.
    pub fn update_priority(&mut self, idx: usize, td_error: f64) {
        let p = (td_error.abs() + EPSILON_PER).powf(ALPHA_PER);
        if let Some(t) = self.transitions.get_mut(&idx) {
            t.priority = p;
        }
        if p > self.max_priority {
            self.max_priority = p;
        }
    }

    /// Sample `n` transitions weighted by priority. Returns (transition, IS_weight) pairs.
    pub fn sample(&self, n: usize, seed: u64) -> Vec<(&PrioritizedTransition, f64)> {
        if self.transitions.len() < 2 {
            return vec![];
        }
        let n = n.min(self.transitions.len());
        let total: f64 = self.transitions.values().map(|t| t.priority).sum();
        if total < 1e-10 {
            return vec![];
        }
        let n_f = self.transitions.len() as f64;
        let mut weighted: Vec<(&PrioritizedTransition, f64)> = self
            .transitions
            .values()
            .map(|t| {
                let prob = t.priority / total;
                (t, (n_f * prob).powf(-BETA_PER))
            })
            .collect();
        let max_w = weighted
            .iter()
            .map(|(_, w)| *w)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_w = if max_w > 0.0 { max_w } else { 1.0 };
        for (_, w) in &mut weighted {
            *w /= max_w;
        }
        weighted.sort_unstable_by(|a, b| {
            b.0.priority
                .partial_cmp(&a.0.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let len = weighted.len();
        let mut result = Vec::with_capacity(n);
        let mut rng = seed.wrapping_add(1);
        let mut used = std::collections::HashSet::new();
        while result.len() < n && used.len() < len {
            rng = rng
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let i = (rng >> 33) as usize % len;
            if used.insert(i) {
                #[allow(clippy::indexing_slicing)]
                result.push(weighted[i]);
            }
        }
        result
    }

    /// Number of transitions currently stored.
    pub fn len(&self) -> usize {
        self.transitions.len()
    }

    /// Returns `true` if the buffer holds no transitions.
    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }

    /// The current maximum priority across stored transitions.
    pub fn max_priority(&self) -> f64 {
        self.max_priority
    }
}

impl Default for PrioritizedReplayBuffer {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_sample() {
        let mut buf = PrioritizedReplayBuffer::new(10);
        for i in 0..5u64 {
            buf.push(i, i, i as f64 * 0.1, i + 1);
        }
        assert_eq!(buf.len(), 5);
        let s = buf.sample(3, 42);
        assert!(!s.is_empty());
    }

    #[test]
    fn update_priority_increases_max() {
        let mut buf = PrioritizedReplayBuffer::new(100);
        let idx = buf.push(0, 0, 0.5, 1);
        buf.update_priority(idx, 10.0);
        assert!(buf.max_priority() > 1.0);
    }

    #[test]
    fn capacity_evicts_oldest() {
        let mut buf = PrioritizedReplayBuffer::new(3);
        for i in 0..5u64 {
            buf.push(i, i, 0.5, i + 1);
        }
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn sample_empty_returns_empty() {
        let buf = PrioritizedReplayBuffer::new(10);
        assert!(buf.sample(5, 0).is_empty());
    }
}
