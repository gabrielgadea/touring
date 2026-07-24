//! E4-S1: Handler timeout and circuit breaker for pipeline resilience.
//!
//! With 81 handlers, a single slow handler can block the entire pipeline.
//! This module provides:
//! - Per-handler timeout tracking (configurable, default 50ms)
//! - Circuit breaker pattern: 3 consecutive timeouts → disable for 5 minutes
//! - Automatic recovery after the cooldown period

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default handler execution timeout in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 50;
/// Number of consecutive timeouts before tripping the circuit breaker.
pub const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;
/// Cooldown period in seconds after circuit breaker trips.
pub const COOLDOWN_SECONDS: u64 = 300; // 5 minutes

/// Per-handler circuit breaker state.
#[derive(Debug)]
pub struct HandlerBreaker {
    /// Consecutive timeout count.
    failure_count: AtomicU32,
    /// Unix timestamp (seconds) until which this handler is disabled.
    disabled_until: AtomicU64,
    /// Total timeouts recorded (for metrics).
    total_timeouts: AtomicU32,
}

impl Default for HandlerBreaker {
    fn default() -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            disabled_until: AtomicU64::new(0),
            total_timeouts: AtomicU32::new(0),
        }
    }
}

impl HandlerBreaker {
    /// Check if this handler should be skipped due to circuit breaker.
    pub fn should_skip(&self) -> bool {
        let disabled = self.disabled_until.load(Ordering::Relaxed);
        if disabled == 0 {
            return false;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now >= disabled {
            // Cooldown expired — reset
            self.disabled_until.store(0, Ordering::Relaxed);
            self.failure_count.store(0, Ordering::Relaxed);
            false
        } else {
            true
        }
    }

    /// Record a timeout (failure) for this handler.
    pub fn record_timeout(&self) {
        self._record_failure()
    }

    /// Record a failure for this handler — alias for `record_timeout`.
    pub fn record_failure(&mut self) {
        self._record_failure()
    }

    fn _record_failure(&self) {
        self.total_timeouts.fetch_add(1, Ordering::Relaxed);
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= CIRCUIT_BREAKER_THRESHOLD {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.disabled_until
                .store(now + COOLDOWN_SECONDS, Ordering::Relaxed);
            self.failure_count.store(0, Ordering::Relaxed);
        }
    }

    /// Record a successful (non-timeout) execution — resets consecutive count.
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
    }

    /// Total timeouts recorded for this handler.
    pub fn total_timeouts(&self) -> u32 {
        self.total_timeouts.load(Ordering::Relaxed)
    }
}

// ── CircuitBreaker trait impl ─────────────────────────────────────────────────

impl touring_foundation::shared::circuit_breaker::CircuitBreaker for HandlerBreaker {
    /// Returns `true` if the circuit is open (handler should be skipped).
    fn is_open(&self) -> bool {
        self.should_skip()
    }

    /// Records a failure — increments consecutive timeout count.
    fn record_failure(&mut self) {
        self._record_failure()
    }

    /// Records a successful execution — resets consecutive failure count.
    fn record_success(&mut self) {
        self.failure_count.store(0, Ordering::Relaxed);
    }
}

/// Registry of circuit breakers for all handlers.
/// Uses RefCell for interior mutability so Pipeline can use &self.
#[derive(Debug, Default)]
pub struct CircuitBreakerRegistry {
    breakers: RefCell<HashMap<String, HandlerBreaker>>,
}

impl CircuitBreakerRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create a breaker for the given handler name.
    /// Uses RefCell for interior mutability.
    pub fn get_or_create(&self, name: &str) -> std::cell::Ref<'_, HandlerBreaker> {
        let mut breakers = self.breakers.borrow_mut();
        breakers.entry(name.to_string()).or_default();
        std::cell::Ref::map(self.breakers.borrow(), |b| {
            b.get(name).expect("just inserted")
        })
    }

    /// Check if a handler should be skipped (read-only, no mutation).
    pub fn should_skip(&self, name: &str) -> bool {
        self.breakers
            .borrow()
            .get(name)
            .is_some_and(|b| b.should_skip())
    }

    /// Record a timeout for a handler.
    pub fn record_timeout(&self, name: &str) {
        let mut breakers = self.breakers.borrow_mut();
        breakers
            .entry(name.to_string())
            .or_default()
            .record_timeout();
    }

    /// Record success for a handler.
    pub fn record_success(&self, name: &str) {
        let mut breakers = self.breakers.borrow_mut();
        breakers
            .entry(name.to_string())
            .or_default()
            .record_success();
    }

    /// Number of tracked handlers.
    pub fn handler_count(&self) -> usize {
        self.breakers.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breaker_initial_state_not_skipped() {
        let breaker = HandlerBreaker::default();
        assert!(!breaker.should_skip());
    }

    #[test]
    fn test_breaker_trips_after_threshold() {
        let breaker = HandlerBreaker::default();
        for _ in 0..CIRCUIT_BREAKER_THRESHOLD {
            breaker.record_timeout();
        }
        assert!(
            breaker.should_skip(),
            "Should be disabled after threshold timeouts"
        );
    }

    #[test]
    fn test_breaker_success_resets_count() {
        let breaker = HandlerBreaker::default();
        breaker.record_timeout();
        breaker.record_timeout();
        breaker.record_success(); // reset
        breaker.record_timeout(); // only 1 now, not 3
        assert!(!breaker.should_skip());
    }

    #[test]
    fn test_breaker_total_timeouts_accumulate() {
        let breaker = HandlerBreaker::default();
        breaker.record_timeout();
        breaker.record_success();
        breaker.record_timeout();
        assert_eq!(breaker.total_timeouts(), 2);
    }

    #[test]
    fn test_breaker_below_threshold_not_tripped() {
        let breaker = HandlerBreaker::default();
        breaker.record_timeout();
        breaker.record_timeout();
        // Only 2, threshold is 3
        assert!(!breaker.should_skip());
    }

    #[test]
    #[allow(unused_mut)]
    fn test_registry_get_or_create() {
        let mut reg = CircuitBreakerRegistry::new();
        assert!(!reg.should_skip("handler_a"));
        reg.record_timeout("handler_a");
        reg.record_timeout("handler_a");
        reg.record_timeout("handler_a");
        assert!(reg.should_skip("handler_a"));
        assert!(!reg.should_skip("handler_b")); // different handler unaffected
    }

    #[test]
    #[allow(unused_mut)]
    fn test_registry_independent_handlers() {
        let mut reg = CircuitBreakerRegistry::new();
        // Trip handler_a
        for _ in 0..CIRCUIT_BREAKER_THRESHOLD {
            reg.record_timeout("handler_a");
        }
        // handler_b should be unaffected
        assert!(reg.should_skip("handler_a"));
        assert!(!reg.should_skip("handler_b"));
    }

    #[test]
    #[allow(unused_mut)]
    fn test_registry_handler_count() {
        let mut reg = CircuitBreakerRegistry::new();
        reg.record_timeout("a");
        reg.record_timeout("b");
        reg.record_success("c");
        assert_eq!(reg.handler_count(), 3);
    }

    #[test]
    #[allow(unused_mut)]
    fn test_registry_success_resets_across_calls() {
        let mut reg = CircuitBreakerRegistry::new();
        reg.record_timeout("h1");
        reg.record_timeout("h1");
        reg.record_success("h1");
        reg.record_timeout("h1");
        reg.record_timeout("h1");
        // Only 2 consecutive after reset, not 4
        assert!(!reg.should_skip("h1"));
    }
}
