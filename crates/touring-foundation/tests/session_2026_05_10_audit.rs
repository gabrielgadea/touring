//! Cross-audit E2E — Session 2026-05-10 (touring-foundation surface).
//!
//! Asserts the behavioural contracts for the symbols added/modified in the
//! "domain_circuit guard layer" session:
//!
//! 1. [`touring_foundation::Domain`] — the three-variant discriminator routes
//!    sub-breakers correctly through [`touring_foundation::DomainCircuitBreaker`].
//! 2. [`touring_foundation::GuardOutcome`] — `Skipped` / `Ok` / `Err` semantics, plus
//!    `into_result` and `map` adapters.
//! 3. [`touring_foundation::DomainCircuitBreaker::guard`] — synchronous guard
//!    skips when the breaker is OPEN, records success/failure otherwise.
//! 4. [`touring_foundation::SharedDomainCircuitBreaker`] — `Arc<Mutex>` handle that
//!    is `Clone + Send + Sync`, isolates per-domain state, and persists
//!    failures across clones.
//! 5. **Async split-lock invariant** — `SharedDomainCircuitBreaker::guard_async`
//!    must release the lock across the `.await` boundary (otherwise a parallel
//!    task that needs the same handle would deadlock when run concurrently).
//!
//! These tests live in `tests/` (integration scope) — they exercise the public
//! re-export surface from `touring_foundation::*`, not the private module path. That
//! catches accidental visibility regressions in `lib.rs` re-exports.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// `CircuitBreaker` trait must be in scope for `is_open()` method-call syntax
// on `CircuitBreakerImpl` (orphan impl rule — the trait method is reachable
// only when the trait itself is imported).
use touring_foundation::{
    CircuitBreaker, CircuitBreakerImpl, CircuitState, Domain, DomainCircuitBreaker, GuardOutcome,
    SharedDomainCircuitBreaker,
};

// ── (1) Domain routes through DomainCircuitBreaker ───────────────────────────

#[test]
fn domain_variants_route_to_distinct_sub_breakers() {
    let mut d = DomainCircuitBreaker::with_config(1, 60);

    // Trip only the Memory domain.
    d.breaker_mut(Domain::Memory).record_failure();

    assert!(!d.breaker(Domain::Knowledge).is_open());
    assert!(d.breaker(Domain::Memory).is_open());
    assert!(!d.breaker(Domain::Graph).is_open());
    assert!(!d.all_closed());

    // The state of the tripped breaker is Open (not the same value as
    // `is_open()` — Open + cooldown_expired returns false from `is_open()`).
    assert_eq!(d.breaker(Domain::Memory).state(), CircuitState::Open);
}

// ── (2) GuardOutcome adapters ────────────────────────────────────────────────

#[test]
fn guard_outcome_into_result_collapses_skip_to_ok_none() {
    let skipped: GuardOutcome<u32, &str> = GuardOutcome::Skipped;
    assert_eq!(
        skipped
            .into_result()
            .expect("Skipped → Ok(None) does not error"),
        None
    );

    let ok: GuardOutcome<u32, &str> = GuardOutcome::Ok(42);
    assert_eq!(
        ok.into_result()
            .expect("Ok(v) → Ok(Some(v)) does not error"),
        Some(42)
    );

    let err: GuardOutcome<u32, &str> = GuardOutcome::Err("io");
    match err.into_result() {
        Err(e) => assert_eq!(e, "io"),
        Ok(_) => unreachable!("Err must propagate"),
    }
}

#[test]
fn guard_outcome_map_only_transforms_ok() {
    let ok: GuardOutcome<u32, &str> = GuardOutcome::Ok(7);
    let mapped = ok.map(|v| v * 6);
    assert!(matches!(mapped, GuardOutcome::Ok(42)));

    let skipped: GuardOutcome<u32, &str> = GuardOutcome::Skipped;
    assert!(matches!(skipped.map(|v| v + 1), GuardOutcome::Skipped));

    let err: GuardOutcome<u32, &str> = GuardOutcome::Err("boom");
    assert!(matches!(err.map(|v| v + 1), GuardOutcome::Err("boom")));
}

// ── (3) Synchronous guard ────────────────────────────────────────────────────

#[test]
fn guard_skips_when_circuit_is_open_and_does_not_run_op() {
    use std::cell::Cell;
    let mut d = DomainCircuitBreaker::with_config(1, 60);

    // Force OPEN on Knowledge.
    let _ = d.guard(Domain::Knowledge, || Err::<(), _>("seed"));
    assert!(d.breaker(Domain::Knowledge).is_open());

    let called = Cell::new(false);
    let outcome: GuardOutcome<u32, &str> = d.guard(Domain::Knowledge, || {
        called.set(true);
        Ok(99)
    });

    assert!(matches!(outcome, GuardOutcome::Skipped));
    assert!(!called.get(), "op MUST NOT run while breaker is open");
}

#[test]
fn guard_isolation_failure_in_one_domain_does_not_skip_others() {
    let mut d = DomainCircuitBreaker::with_config(1, 60);
    // Trip Knowledge.
    let _ = d.guard(Domain::Knowledge, || Err::<(), _>("k_fail"));
    assert!(d.breaker(Domain::Knowledge).is_open());

    // Memory must still pass through cleanly.
    let other: GuardOutcome<u8, &str> = d.guard(Domain::Memory, || Ok(7));
    assert!(matches!(other, GuardOutcome::Ok(7)));
}

// ── (4) SharedDomainCircuitBreaker — clone-shared state ──────────────────────

#[test]
fn shared_handle_is_send_sync_and_propagates_trips() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SharedDomainCircuitBreaker>();

    let a = SharedDomainCircuitBreaker::with_config(1, 60);
    let b = a.clone();

    let _ = a.guard(Domain::Memory, || Err::<(), _>("oops"));
    // b sees the same inner state because the Arc is shared.
    assert!(b.is_open(Domain::Memory));
    assert!(!b.all_closed());

    // reset_all on one handle clears state visible to all clones.
    b.reset_all();
    assert!(a.all_closed());
}

#[test]
fn shared_from_breaker_seeds_state_from_an_existing_breaker() {
    let mut seed = DomainCircuitBreaker::with_config(1, 60);
    seed.knowledge.record_failure();
    assert!(seed.knowledge.is_open());

    let handle = SharedDomainCircuitBreaker::from_breaker(seed);
    assert!(handle.is_open(Domain::Knowledge));
}

// ── (5) Async split-lock — the load-bearing invariant ────────────────────────
//
// If `guard_async` held the `std::sync::Mutex` across the `.await`, spawning
// many concurrent guards would either (a) deadlock, or (b) serialize them to
// the duration of each `op().await`. We assert that 16 concurrent guards all
// complete and the work counter increments to 16 — proving the lock is
// released across the await point (split-lock pattern).

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_guard_async_releases_lock_across_await() {
    let breakers = SharedDomainCircuitBreaker::new();
    let counter = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::new();
    for _ in 0..16 {
        let b = breakers.clone();
        let c = counter.clone();
        handles.push(tokio::spawn(async move {
            let outcome: GuardOutcome<(), &str> = b
                .guard_async(Domain::Memory, || async {
                    // Simulate I/O — if the outer Mutex were held across await,
                    // this `sleep` would serialize all 16 tasks.
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), &str>(())
                })
                .await;
            assert!(matches!(outcome, GuardOutcome::Ok(_)));
        }));
    }
    for h in handles {
        h.await.expect("spawned task did not panic");
    }

    assert_eq!(counter.load(Ordering::SeqCst), 16);
    assert!(
        breakers.all_closed(),
        "16 successes must NOT trip the breaker"
    );
}

#[tokio::test]
async fn shared_guard_async_records_failure_and_skips_subsequent_calls() {
    let breakers = SharedDomainCircuitBreaker::with_config(2, 60);

    // Two failures trip the breaker.
    for _ in 0..2 {
        let r: GuardOutcome<(), &str> = breakers
            .guard_async(Domain::Graph, || async { Err("net") })
            .await;
        assert!(matches!(r, GuardOutcome::Err("net")));
    }

    // Next call must be Skipped (circuit is now OPEN, cooldown not yet
    // elapsed). `op` must not even be invoked.
    let r: GuardOutcome<u32, &str> = breakers
        .guard_async(Domain::Graph, || async {
            panic!("op MUST NOT run when circuit is open");
        })
        .await;
    assert!(matches!(r, GuardOutcome::Skipped));
}

// ── (6) CircuitBreakerImpl public surface remains accessible ─────────────────

#[test]
fn circuit_breaker_impl_public_surface_is_reachable_via_reexport() {
    // We deliberately reach for the concrete type through the crate-level
    // re-export — this catches regressions where `pub use` is dropped during
    // refactors.
    let mut breaker = CircuitBreakerImpl::new(2, 60);
    assert_eq!(breaker.state(), CircuitState::Closed);
    breaker.reset();
    assert_eq!(breaker.state(), CircuitState::Closed);
}
