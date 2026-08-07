//! DomainCircuitBreaker — per-domain circuit breaker isolation.
//!
//! Provides a concrete [`CircuitBreakerImpl`] that implements the [`CircuitBreaker`]
//! trait, and a [`DomainCircuitBreaker`] that composes three independent breakers:
//! one for each of the `knowledge`, `memory`, and `graph` DB domains.
//!
//! # Why per-domain?
//!
//! A global circuit breaker causes cascading failures: a transient graph-DB
//! hiccup would block all knowledge queries. Per-domain isolation ensures each
//! domain degrades independently, preserving partial availability.
//!
//! # State machine
//!
//! ```text
//! CLOSED ──(N failures)──► OPEN ──(cooldown expires)──► HALF_OPEN
//!                                                            │
//!                              success → CLOSED ◄───────────┤
//!                              failure → OPEN  ◄───────────┘
//! ```
//!
//! Note: `is_open()` takes `&self` and computes the OPEN/HALF_OPEN boundary
//! without mutating state. State transitions only happen in `record_success`
//! and `record_failure` (`&mut self`).
//!
//! # Guard pattern (production wiring)
//!
//! For wiring real DB query layers, use [`Domain`] + [`DomainCircuitBreaker::guard`]
//! (synchronous) or [`SharedDomainCircuitBreaker::guard_async`] (async, Tokio-safe
//! via split-lock):
//!
//! ```ignore
//! use touring_foundation::{Domain, GuardOutcome, SharedDomainCircuitBreaker};
//!
//! let breakers = SharedDomainCircuitBreaker::new();
//! match breakers.guard_async(Domain::Knowledge, || async { db.query(sql).await }).await {
//!     GuardOutcome::Ok(rows)  => process(rows),
//!     GuardOutcome::Err(e)    => log_and_fallback(e),
//!     GuardOutcome::Skipped   => fallback(), // breaker open — serve stale or empty
//! }
//! ```

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::circuit_breaker::CircuitBreaker;

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — requests pass through.
    Closed,
    /// Circuit tripped — requests are skipped until cooldown expires.
    Open,
    /// Cooldown expired — one probe request is allowed to test recovery.
    HalfOpen,
}

/// Concrete circuit breaker with configurable threshold and cooldown.
///
/// Implements `CircuitBreaker` from `touring-foundation::shared`.
/// `is_open()` takes `&self` — it computes the effective state without mutation.
/// State transitions (Open → HalfOpen, HalfOpen → Closed/Open) are applied lazily
/// at the start of each `record_success` / `record_failure` call.
#[derive(Debug)]
pub struct CircuitBreakerImpl {
    state: CircuitState,
    consecutive_failures: u32,
    failure_threshold: u32,
    cooldown: Duration,
    opened_at: Option<Instant>,
}

impl CircuitBreakerImpl {
    /// Create a new breaker: trips after `failure_threshold` consecutive failures,
    /// recovers after `cooldown_secs`.
    pub fn new(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            failure_threshold,
            cooldown: Duration::from_secs(cooldown_secs),
            opened_at: None,
        }
    }

    /// Current stored state (does **not** re-evaluate cooldown).
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Reset to CLOSED regardless of current state.
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.consecutive_failures = 0;
        self.opened_at = None;
    }

    /// Advance OPEN → HALF_OPEN if cooldown has elapsed.
    /// Called at the start of `record_success` / `record_failure`.
    fn sync_half_open(&mut self) {
        if self.state == CircuitState::Open
            && let Some(opened) = self.opened_at
            && opened.elapsed() >= self.cooldown
        {
            self.state = CircuitState::HalfOpen;
        }
    }
}

impl CircuitBreaker for CircuitBreakerImpl {
    /// Returns `true` if the circuit is effectively OPEN (requests should be skipped).
    ///
    /// When in OPEN state but cooldown has expired, returns `false`
    /// (the next call is a probe; actual state update happens in `record_*`).
    fn is_open(&self) -> bool {
        match self.state {
            CircuitState::Closed | CircuitState::HalfOpen => false,
            CircuitState::Open => match self.opened_at {
                Some(t) => t.elapsed() < self.cooldown,
                None => true,
            },
        }
    }

    fn record_success(&mut self) {
        self.sync_half_open(); // advance OPEN→HALF_OPEN if expired
        self.consecutive_failures = 0;
        self.state = CircuitState::Closed;
        self.opened_at = None;
    }

    fn record_failure(&mut self) {
        self.sync_half_open(); // advance OPEN→HALF_OPEN if expired

        self.consecutive_failures += 1;

        let should_trip = self.consecutive_failures >= self.failure_threshold
            || self.state == CircuitState::HalfOpen;

        if should_trip {
            self.state = CircuitState::Open;
            self.opened_at = Some(Instant::now());
            self.consecutive_failures = 0;
        }
    }
}

// ── DomainCircuitBreaker ─────────────────────────────────────────────────────

/// Per-domain circuit breakers for the three Touring database domains.
///
/// Each domain (`knowledge`, `memory`, `graph`) has an independent breaker.
/// A failure in one domain does NOT affect the others.
///
/// # Defaults
///
/// - `failure_threshold = 3` (trip after 3 consecutive failures)
/// - `cooldown_secs = 60` (one-minute recovery window per domain)
#[derive(Debug)]
pub struct DomainCircuitBreaker {
    /// Breaker for the `knowledge` domain (symbols, file metadata, pipeline).
    pub knowledge: CircuitBreakerImpl,
    /// Breaker for the `memory` domain (RLM entries, semantic graph).
    pub memory: CircuitBreakerImpl,
    /// Breaker for the `graph` domain (sessions, GoT snapshots).
    pub graph: CircuitBreakerImpl,
}

impl DomainCircuitBreaker {
    /// Create with default thresholds (3 failures, 60 s cooldown).
    pub fn new() -> Self {
        Self {
            knowledge: CircuitBreakerImpl::new(3, 60),
            memory: CircuitBreakerImpl::new(3, 60),
            graph: CircuitBreakerImpl::new(3, 60),
        }
    }

    /// Create with custom threshold and cooldown applied to all three domains.
    pub fn with_config(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            knowledge: CircuitBreakerImpl::new(failure_threshold, cooldown_secs),
            memory: CircuitBreakerImpl::new(failure_threshold, cooldown_secs),
            graph: CircuitBreakerImpl::new(failure_threshold, cooldown_secs),
        }
    }

    /// Returns `true` if ALL three domains are not open.
    pub fn all_closed(&self) -> bool {
        !self.knowledge.is_open() && !self.memory.is_open() && !self.graph.is_open()
    }

    /// Reset all domains to CLOSED.
    pub fn reset_all(&mut self) {
        self.knowledge.reset();
        self.memory.reset();
        self.graph.reset();
    }
}

impl Default for DomainCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Guard layer (production wiring) ──────────────────────────────────────────

/// Discriminator for selecting which sub-breaker an operation targets.
///
/// Used by [`DomainCircuitBreaker::guard`] and [`SharedDomainCircuitBreaker`]
/// to route per-operation skip/record decisions to the correct domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Domain {
    /// Knowledge DB (symbols, file metadata, indexing pipeline).
    Knowledge,
    /// Memory DB (RLM entries, semantic graph, lessons).
    Memory,
    /// Graph DB (sessions, GoT snapshots, decomposition DAGs).
    Graph,
}

/// Outcome of a guarded operation — explicit three-state result.
///
/// Prefer this over `Result<Option<T>, E>` for clarity at call sites: each
/// variant maps to a distinct branch (run-success / run-failure / not-run).
#[derive(Debug)]
pub enum GuardOutcome<T, E> {
    /// Circuit was open — operation did not run, no state was recorded.
    Skipped,
    /// Operation ran and succeeded; `record_success` was called.
    Ok(T),
    /// Operation ran and failed; `record_failure` was called.
    Err(E),
}

impl<T, E> GuardOutcome<T, E> {
    /// Returns `true` if the operation was skipped because the circuit was open.
    #[inline]
    pub fn is_skipped(&self) -> bool {
        matches!(self, GuardOutcome::Skipped)
    }

    /// Returns `true` only on `Ok` — failures and skips both return `false`.
    #[inline]
    pub fn is_ok(&self) -> bool {
        matches!(self, GuardOutcome::Ok(_))
    }

    /// Collapses to `Result<Option<T>, E>`: `Skipped → Ok(None)`, `Ok(v) → Ok(Some(v))`,
    /// `Err(e) → Err(e)`. Useful when callers want to treat a skip as a recoverable absence.
    pub fn into_result(self) -> Result<Option<T>, E> {
        match self {
            GuardOutcome::Skipped => Ok(None),
            GuardOutcome::Ok(v) => Ok(Some(v)),
            GuardOutcome::Err(e) => Err(e),
        }
    }

    /// Map the success value through `f`, leaving `Skipped` / `Err` unchanged.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> GuardOutcome<U, E> {
        match self {
            GuardOutcome::Skipped => GuardOutcome::Skipped,
            GuardOutcome::Ok(v) => GuardOutcome::Ok(f(v)),
            GuardOutcome::Err(e) => GuardOutcome::Err(e),
        }
    }
}

impl DomainCircuitBreaker {
    /// Borrow the sub-breaker for `domain` immutably (for point-in-time queries).
    ///
    /// Use when you only need to inspect state (`is_open()`, `state()`) without
    /// transitioning. For state transitions, use [`Self::breaker_mut`] or
    /// the [`Self::guard`] helper.
    pub fn breaker(&self, domain: Domain) -> &CircuitBreakerImpl {
        match domain {
            Domain::Knowledge => &self.knowledge,
            Domain::Memory => &self.memory,
            Domain::Graph => &self.graph,
        }
    }

    /// Borrow the sub-breaker for `domain` mutably (for `record_*` / `reset`).
    pub fn breaker_mut(&mut self, domain: Domain) -> &mut CircuitBreakerImpl {
        match domain {
            Domain::Knowledge => &mut self.knowledge,
            Domain::Memory => &mut self.memory,
            Domain::Graph => &mut self.graph,
        }
    }

    /// Guard a synchronous fallible operation behind the `domain` breaker.
    ///
    /// Semantics:
    /// 1. If the breaker `is_open()`, returns [`GuardOutcome::Skipped`] without invoking `op`.
    /// 2. Otherwise, runs `op`:
    ///    - on `Ok(v)` calls `record_success()` and returns [`GuardOutcome::Ok`].
    ///    - on `Err(e)` calls `record_failure()` and returns [`GuardOutcome::Err`].
    ///
    /// # Example
    ///
    /// ```
    /// use touring_foundation::shared::domain_circuit::{Domain, DomainCircuitBreaker, GuardOutcome};
    ///
    /// let mut breakers = DomainCircuitBreaker::new();
    /// let outcome: GuardOutcome<u32, &'static str> =
    ///     breakers.guard(Domain::Knowledge, || Ok::<u32, &'static str>(42));
    /// assert!(outcome.is_ok());
    /// ```
    pub fn guard<T, E, F>(&mut self, domain: Domain, op: F) -> GuardOutcome<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if self.breaker(domain).is_open() {
            return GuardOutcome::Skipped;
        }
        match op() {
            Ok(value) => {
                self.breaker_mut(domain).record_success();
                GuardOutcome::Ok(value)
            }
            Err(err) => {
                self.breaker_mut(domain).record_failure();
                GuardOutcome::Err(err)
            }
        }
    }
}

/// Thread-safe, cheaply-cloneable handle to a [`DomainCircuitBreaker`].
///
/// Wraps `Arc<Mutex<DomainCircuitBreaker>>` so a single set of three sub-breakers
/// can be shared across many concurrent DB call sites (typical Tokio service).
/// The [`Self::guard_async`] method implements **split-lock**: it acquires the
/// mutex only to check `is_open` and to record the outcome — never across an
/// `await` point, eliminating the classic "MutexGuard held across .await"
/// anti-pattern.
///
/// # Poison recovery
///
/// If another thread panicked while holding the lock, this handle recovers via
/// `PoisonError::into_inner()` — the breaker state is treated as still valid
/// (the panic was in the caller code, not in this struct's invariants).
#[derive(Clone, Debug)]
pub struct SharedDomainCircuitBreaker {
    inner: Arc<Mutex<DomainCircuitBreaker>>,
}

impl SharedDomainCircuitBreaker {
    /// Create with default thresholds (3 failures, 60 s cooldown).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(DomainCircuitBreaker::new())),
        }
    }

    /// Create with custom threshold and cooldown applied to all three domains.
    pub fn with_config(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DomainCircuitBreaker::with_config(
                failure_threshold,
                cooldown_secs,
            ))),
        }
    }

    /// Wrap an existing [`DomainCircuitBreaker`] in a shareable handle.
    pub fn from_breaker(breaker: DomainCircuitBreaker) -> Self {
        Self {
            inner: Arc::new(Mutex::new(breaker)),
        }
    }

    /// Lock-and-check: returns `true` if `domain`'s breaker is currently OPEN.
    ///
    /// Fast path for callers that want to skip an expensive setup before the
    /// actual call. Note: this is point-in-time — by the time the caller
    /// dispatches the operation, the state may have changed. Use [`Self::guard`]
    /// or [`Self::guard_async`] for the atomic check+record pattern.
    pub fn is_open(&self, domain: Domain) -> bool {
        let inner = lock_recover(&self.inner);
        inner.breaker(domain).is_open()
    }

    /// Returns `true` if all three domains are currently closed.
    pub fn all_closed(&self) -> bool {
        let inner = lock_recover(&self.inner);
        inner.all_closed()
    }

    /// Reset all three breakers to CLOSED.
    pub fn reset_all(&self) {
        let mut inner = lock_recover_mut(&self.inner);
        inner.reset_all();
    }

    /// Guard a synchronous fallible operation (see [`DomainCircuitBreaker::guard`]).
    ///
    /// The whole call (check + op + record) holds the lock — fine for fast,
    /// non-blocking closures (in-memory lookups, parsed-config access). For
    /// I/O or anything potentially slow, prefer [`Self::guard_async`].
    pub fn guard<T, E, F>(&self, domain: Domain, op: F) -> GuardOutcome<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let mut inner = lock_recover_mut(&self.inner);
        inner.guard(domain, op)
    }

    /// Guard an async fallible operation via split-lock (await-safe).
    ///
    /// # Implementation
    ///
    /// 1. Acquire lock, read `is_open(domain)`, release lock.
    /// 2. If open → return [`GuardOutcome::Skipped`].
    /// 3. Otherwise: call `op()` to obtain the `Future`, `.await` it **without
    ///    holding the lock** (this is the critical correctness property).
    /// 4. Re-acquire lock, dispatch `record_success` / `record_failure` based
    ///    on the awaited result, release lock.
    ///
    /// # Race semantics
    ///
    /// Between step 1 and step 4 another task may transition the same breaker.
    /// This is acceptable: it matches the Circuit Breaker pattern's
    /// eventually-consistent semantics. Failures during a transient OPEN window
    /// only re-record failures, which is the desired conservative behavior.
    pub async fn guard_async<T, E, F, Fut>(&self, domain: Domain, op: F) -> GuardOutcome<T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        // Phase 1: short lock — read is_open.
        if self.is_open(domain) {
            return GuardOutcome::Skipped;
        }
        // Phase 2: run async op WITHOUT holding the lock.
        let result = op().await;
        // Phase 3: short lock — record outcome.
        let mut inner = lock_recover_mut(&self.inner);
        match result {
            Ok(value) => {
                inner.breaker_mut(domain).record_success();
                GuardOutcome::Ok(value)
            }
            Err(err) => {
                inner.breaker_mut(domain).record_failure();
                GuardOutcome::Err(err)
            }
        }
    }
}

impl Default for SharedDomainCircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// Lock the mutex, recovering from poison by treating the inner state as still
/// valid (the panic was in caller code, not in our invariants).
#[inline]
fn lock_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `lock_recover` mirror for `&mut`-yielding callers — same recovery policy.
/// Both helpers exist as distinct functions so the call site reads naturally
/// (`lock_recover_mut` clearly signals "we will mutate").
#[inline]
fn lock_recover_mut<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaker_starts_closed() {
        let b = CircuitBreakerImpl::new(3, 60);
        assert_eq!(b.state(), CircuitState::Closed);
        assert!(!b.is_open());
    }

    #[test]
    fn breaker_opens_after_threshold() {
        let mut b = CircuitBreakerImpl::new(3, 60);
        b.record_failure();
        b.record_failure();
        assert!(!b.is_open(), "should not open before threshold");
        b.record_failure();
        assert!(b.is_open(), "should open at threshold");
    }

    #[test]
    fn breaker_resets_on_success() {
        let mut b = CircuitBreakerImpl::new(3, 60);
        b.record_failure();
        b.record_failure();
        b.record_failure();
        assert!(b.is_open());
        b.record_success();
        assert!(!b.is_open());
        assert_eq!(b.state(), CircuitState::Closed);
    }

    #[test]
    fn open_with_zero_cooldown_is_not_open_immediately() {
        let mut b = CircuitBreakerImpl::new(1, 0); // instant cooldown
        b.record_failure();
        // Cooldown = 0s: elapsed() >= 0 always, so is_open() returns false
        assert!(!b.is_open(), "zero-cooldown breaker should not be open");
    }

    #[test]
    fn half_open_probe_failure_reopens() {
        let mut b = CircuitBreakerImpl::new(1, 0); // instant cooldown
        b.record_failure(); // → OPEN
        // cooldown=0 means sync_half_open() transitions OPEN→HALF_OPEN immediately
        b.record_failure(); // sync_half_open → HALF_OPEN, then probe fails → OPEN
        // is_open() returns false when cooldown=0 (by design — see open_with_zero_cooldown_is_not_open_immediately).
        // The important invariant: stored state is OPEN after probe failure.
        assert_eq!(
            b.state(),
            CircuitState::Open,
            "probe failure must re-trip to OPEN"
        );
    }

    #[test]
    fn half_open_probe_success_closes() {
        let mut b = CircuitBreakerImpl::new(1, 0);
        b.record_failure(); // → OPEN
        b.record_success(); // sync_half_open → HALF_OPEN, then → CLOSED
        assert!(!b.is_open());
    }

    #[test]
    fn domain_breakers_are_independent() {
        let mut d = DomainCircuitBreaker::new();
        d.knowledge.record_failure();
        d.knowledge.record_failure();
        d.knowledge.record_failure();
        assert!(d.knowledge.is_open());
        assert!(!d.memory.is_open());
        assert!(!d.graph.is_open());
        assert!(!d.all_closed());
    }

    #[test]
    fn reset_all_closes_everything() {
        let mut d = DomainCircuitBreaker::with_config(1, 60);
        d.knowledge.record_failure();
        d.memory.record_failure();
        d.graph.record_failure();
        d.reset_all();
        assert!(d.all_closed());
    }

    // ── Guard-layer tests ────────────────────────────────────────────────────

    #[test]
    fn domain_selector_routes_to_correct_breaker() {
        let mut d = DomainCircuitBreaker::with_config(1, 60);
        d.breaker_mut(Domain::Memory).record_failure(); // trip ONLY memory
        assert!(!d.breaker(Domain::Knowledge).is_open());
        assert!(d.breaker(Domain::Memory).is_open());
        assert!(!d.breaker(Domain::Graph).is_open());
    }

    #[test]
    fn guard_passes_closed_circuit() {
        let mut d = DomainCircuitBreaker::new();
        let outcome: GuardOutcome<u32, &'static str> = d.guard(Domain::Knowledge, || Ok(42));
        assert!(
            matches!(outcome, GuardOutcome::Ok(42)),
            "expected Ok(42), got {:?}",
            outcome
        );
        assert!(d.all_closed(), "success must NOT trip the breaker");
    }

    #[test]
    fn guard_records_failure_and_trips_at_threshold() {
        let mut d = DomainCircuitBreaker::with_config(2, 60);
        // First failure — recorded but doesn't trip yet.
        let r1: GuardOutcome<(), &'static str> = d.guard(Domain::Graph, || Err("io"));
        assert!(matches!(r1, GuardOutcome::Err("io")));
        assert!(!d.breaker(Domain::Graph).is_open());
        // Second failure — trips at threshold.
        let r2: GuardOutcome<(), &'static str> = d.guard(Domain::Graph, || Err("io"));
        assert!(matches!(r2, GuardOutcome::Err("io")));
        assert!(d.breaker(Domain::Graph).is_open(), "must trip at threshold");
    }

    #[test]
    fn guard_skips_when_open_and_does_not_invoke_op() {
        use std::cell::Cell;
        let mut d = DomainCircuitBreaker::with_config(1, 60);
        // Force OPEN on knowledge.
        let _ = d.guard(Domain::Knowledge, || Err::<(), _>("seed"));
        assert!(d.breaker(Domain::Knowledge).is_open());

        let called = Cell::new(false);
        let outcome: GuardOutcome<u32, &'static str> = d.guard(Domain::Knowledge, || {
            called.set(true);
            Ok(99)
        });
        assert!(matches!(outcome, GuardOutcome::Skipped));
        assert!(!called.get(), "op MUST NOT run when circuit is open");
    }

    #[test]
    fn guard_isolation_failure_in_one_domain_does_not_skip_others() {
        let mut d = DomainCircuitBreaker::with_config(1, 60);
        let _ = d.guard(Domain::Knowledge, || Err::<(), _>("k_fail"));
        assert!(d.breaker(Domain::Knowledge).is_open());

        let other: GuardOutcome<u8, &'static str> = d.guard(Domain::Memory, || Ok(7));
        assert!(matches!(other, GuardOutcome::Ok(7)));
    }

    #[test]
    fn guard_outcome_into_result_maps_skipped_to_ok_none() {
        let skipped: GuardOutcome<u32, &'static str> = GuardOutcome::Skipped;
        assert_eq!(
            skipped
                .into_result()
                .expect("Skipped → Ok(None) must not error"),
            None
        );

        let ok: GuardOutcome<u32, &'static str> = GuardOutcome::Ok(11);
        assert_eq!(
            ok.into_result()
                .expect("Ok(v) → Ok(Some(v)) must not error"),
            Some(11)
        );

        let err: GuardOutcome<u32, &'static str> = GuardOutcome::Err("boom");
        match err.into_result() {
            Err(e) => assert_eq!(e, "boom"),
            Ok(other) => unreachable!("Err(e) must propagate as Err, got Ok({:?})", other),
        }
    }

    #[test]
    fn guard_outcome_map_transforms_only_ok() {
        let ok: GuardOutcome<u32, &'static str> = GuardOutcome::Ok(2);
        let mapped = ok.map(|v| v * 21);
        assert!(
            matches!(mapped, GuardOutcome::Ok(42)),
            "expected Ok(42), got {:?}",
            mapped
        );
        let skipped: GuardOutcome<u32, &'static str> = GuardOutcome::Skipped;
        assert!(matches!(skipped.map(|v| v + 1), GuardOutcome::Skipped));
        let err: GuardOutcome<u32, &'static str> = GuardOutcome::Err("e");
        assert!(matches!(err.map(|v| v + 1), GuardOutcome::Err("e")));
    }

    // ── SharedDomainCircuitBreaker tests ─────────────────────────────────────

    #[test]
    fn shared_guard_sync_round_trip() {
        let breakers = SharedDomainCircuitBreaker::with_config(2, 60);
        let r: GuardOutcome<u32, &'static str> = breakers.guard(Domain::Knowledge, || Ok(1));
        assert!(r.is_ok());
        assert!(breakers.all_closed());
    }

    #[test]
    fn shared_guard_propagates_trip_across_clones() {
        let a = SharedDomainCircuitBreaker::with_config(1, 60);
        let b = a.clone(); // share the same Arc
        let _ = a.guard(Domain::Memory, || Err::<(), _>("oops"));
        // b sees the tripped state because the inner Arc is shared.
        assert!(b.is_open(Domain::Memory));
    }

    #[test]
    fn shared_reset_all_clears_every_domain() {
        let breakers = SharedDomainCircuitBreaker::with_config(1, 60);
        let _ = breakers.guard(Domain::Knowledge, || Err::<(), _>("e"));
        let _ = breakers.guard(Domain::Memory, || Err::<(), _>("e"));
        let _ = breakers.guard(Domain::Graph, || Err::<(), _>("e"));
        assert!(!breakers.all_closed());
        breakers.reset_all();
        assert!(breakers.all_closed());
    }

    #[tokio::test]
    async fn shared_guard_async_passes_closed_circuit() {
        let breakers = SharedDomainCircuitBreaker::new();
        let r: GuardOutcome<u32, std::io::Error> = breakers
            .guard_async(Domain::Knowledge, || async { Ok::<u32, std::io::Error>(7) })
            .await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn shared_guard_async_records_failure_and_eventually_skips() {
        let breakers = SharedDomainCircuitBreaker::with_config(2, 60);
        // Two consecutive failures trip the breaker.
        for _ in 0..2 {
            let r: GuardOutcome<(), &'static str> = breakers
                .guard_async(Domain::Graph, || async { Err("net") })
                .await;
            assert!(matches!(r, GuardOutcome::Err("net")));
        }
        // Next call must be skipped (circuit is OPEN).
        let r: GuardOutcome<u32, &'static str> = breakers
            .guard_async(Domain::Graph, || async { Ok(42) })
            .await;
        assert!(matches!(r, GuardOutcome::Skipped));
    }

    #[tokio::test]
    async fn shared_guard_async_concurrent_access() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let breakers = SharedDomainCircuitBreaker::new();
        let counter = Arc::new(AtomicU32::new(0));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let b = breakers.clone();
            let c = counter.clone();
            handles.push(tokio::spawn(async move {
                let r: GuardOutcome<u32, &'static str> = b
                    .guard_async(Domain::Memory, || async {
                        c.fetch_add(1, Ordering::SeqCst);
                        Ok::<u32, &'static str>(0)
                    })
                    .await;
                assert!(r.is_ok());
            }));
        }
        for h in handles {
            h.await.expect("spawned task panicked");
        }
        assert_eq!(counter.load(Ordering::SeqCst), 16);
        assert!(breakers.all_closed());
    }
}
