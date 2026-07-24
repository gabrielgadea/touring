//! Shared `CircuitBreaker` trait — extracted from touring-cortex for cross-crate use.
//!
//! Implementations in touring-cortex (HandlerBreaker) and touring-learning are
//! NOT required to change — only this trait moves to shared.
//!
//! Architecture:
//! ```text
//! touring-foundation (shared)
//!   └── CircuitBreaker trait
//!
//! touring-cortex
//!   └── HandlerBreaker implements CircuitBreaker
//!
//! touring-learning
//!   └── ReminderCircuitBreaker implements CircuitBreaker (future)
//! ```
//!
//! ## Note on touring-hooks OpClass
//!
//! `OpClass` in touring-hooks is a **classifier enum** (categorizes operations
//! into Light/Medium/Heavy/Critical to determine thresholds/cooldowns). It is
//! NOT a circuit breaker instance — the actual breaker state lives in the
//! top-level file-backed `CircuitState`. Therefore `OpClass` does NOT implement
//! `CircuitBreaker`; it is a parameter classifier, not a per-instance breaker.

/// Trait for circuit breaker implementations.
///
/// Implementations track consecutive failures; after a threshold is reached,
/// the circuit "opens" and remains open until the cooldown expires.
pub trait CircuitBreaker: Send + Sync {
    /// Returns `true` if the circuit is open (should skip).
    fn is_open(&self) -> bool;

    /// Records a successful operation — resets consecutive failure count.
    fn record_success(&mut self);

    /// Records a failure — increments consecutive failure count.
    fn record_failure(&mut self);

    /// Returns `true` if the circuit is currently open.
    ///
    /// Alias for `is_open()` for ergonomic use.
    #[inline]
    fn should_skip(&self) -> bool {
        self.is_open()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal mock circuit breaker for trait testing: trips after N failures.
    struct MockBreaker {
        failures: u32,
        threshold: u32,
        open: bool,
    }

    impl MockBreaker {
        fn new(threshold: u32) -> Self {
            Self {
                failures: 0,
                threshold,
                open: false,
            }
        }
    }

    impl CircuitBreaker for MockBreaker {
        fn is_open(&self) -> bool {
            self.open
        }

        fn record_success(&mut self) {
            self.failures = 0;
            self.open = false;
        }

        fn record_failure(&mut self) {
            self.failures += 1;
            if self.failures >= self.threshold {
                self.open = true;
            }
        }
    }

    #[test]
    fn trait_should_skip_defaults_to_is_open() {
        // A closed circuit should_skip returns false
        let mut breaker = MockBreaker::new(3);
        assert!(!breaker.should_skip());

        // After tripping, should_skip returns true
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.should_skip());
    }

    #[test]
    fn trait_record_success_resets_failure_count() {
        let mut breaker = MockBreaker::new(2);
        breaker.record_failure();
        breaker.record_failure(); // trips at 2
        assert!(breaker.is_open());

        breaker.record_success();
        assert!(!breaker.is_open());
        assert!(!breaker.should_skip());

        // Need 2 fresh failures to trip again
        breaker.record_failure();
        assert!(!breaker.is_open());
    }

    #[test]
    fn trait_record_failure_accumulates() {
        let mut breaker = MockBreaker::new(4);
        for _ in 0..3 {
            breaker.record_failure();
        }
        assert!(!breaker.is_open()); // not yet at threshold

        breaker.record_failure(); // 4th failure trips
        assert!(breaker.is_open());
    }
}
