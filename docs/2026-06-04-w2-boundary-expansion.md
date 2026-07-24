# W2 Boundary Audit — Expansion (10 modules in touring-foundation)

> **Date**: 2026-06-04
> **Wave**: W2 of the 47to13-residual UPGRADE plan
> **Coverage after this batch**: 10 of ~100 modules in `touring-foundation` (~10% of L1 layer)

This document ships 5 additional boundary doc sections (continuing from
the W2 sample at `2026-06-04-w2-boundary-audit.md`). The pattern is
identical to the sample: each section declares Inputs / Outputs /
Invariants / Tier / Stability + a "Why this boundary matters"
motivation.

## Sample 6 — `touring-foundation::alloc`

```rust
//! # Boundary: touring-foundation::alloc
//!
//! **Inputs**: `std::alloc::System` (default), `mimalloc` (opt-in via feature).
//! **Outputs**: `set_allocator()` function to override the global allocator
//!              at startup; `AllocatorKind` enum to query which is active.
//! **Invariants**:
//!   - I1: The allocator choice is set EXACTLY ONCE (at process startup).
//!   - I2: After set_allocator(), the choice is immutable for the process lifetime.
//!   - I3: mimalloc is preferred when the `mimalloc-allocator` feature is enabled
//!         (Linux only).
//! **Tier**: free.
//! **Stability**: 3 (locked; allocator change is a binary-level commitment).
//!
//! # Why this boundary matters
//!
//! Allocator choice is the single largest perf+memory lever for the
//! daemon. A 5-10% improvement is achievable with mimalloc. Changing
//! the API would require a major version bump.
```

## Sample 7 — `touring-foundation::migration`

```rust
//! # Boundary: touring-foundation::migration
//!
//! **Inputs**: schema version (u32), migration function registry (HashMap).
//! **Outputs**: `run_migrations(from: u32, to: u32) -> Result<(), MigrationError>`
//!              function; `Migration` trait that consumers implement.
//! **Invariants**:
//!   - I1: Migrations are monotonic (no downgrade).
//!   - I2: Each migration is idempotent (re-runnable without side effects).
//!   - I3: Failed migrations roll back the transaction (atomicity).
//! **Tier**: free.
//! **Stability**: 2 (stable; new migrations can be added).
//!
//! # Why this boundary matters
//!
//! The schema version is the **single source of truth** for the on-disk
//! database layout. A failed migration is data loss; a non-idempotent
//! migration is a deadlock.
```

## Sample 8 — `touring-foundation::drift`

```rust
//! # Boundary: touring-foundation::drift
//!
//! **Inputs**: `touring evolution drift -j` (periodic snapshot),
//!             baseline metrics (composite_health_score, index_size, etc.).
//! **Outputs**: `DriftLevel` enum (`None | Degraded | Structural`),
//!              `DriftReport` struct (per-signal delta), `drift::assess()`
//!              function.
//! **Invariants**:
//!   - I1: Drift is computed only on signal change (caching).
//!   - I2: `Structural` drift triggers a circuit-breaker open event.
//!   - I3: Drift is non-negative (no improvement signal, only degradation).
//! **Tier**: free.
//! **Stability**: 2 (stable).
//!
//! # Why this boundary matters
//!
//! Drift detection is the **early-warning system** for the substrate.
//! A `Structural` drift level means the workspace is no longer in
//! harmony; the user should investigate before proceeding.
```

## Sample 9 — `touring-foundation::feedback`

```rust
//! # Boundary: touring-foundation::feedback
//!
//! **Inputs**: feedback events (text + score + intent), append-only log.
//! **Outputs**: `FeedbackRecord` struct, `feedback::log()` function,
//!              `feedback::query(intent: &str) -> Vec<FeedbackRecord>`.
//! **Invariants**:
//!   - I1: Feedback is append-only (no mutation after write).
//!   - I2: Score is in [0.0, 1.0] (clamped at write).
//!   - I3: Feedback is correlated with the action via the
//!         `ActionSignature` (the feedback knows what it's about).
//! **Tier**: free.
//! **Stability**: 2 (stable).
//!
//! # Why this boundary matters
//!
//! Feedback is the **closed-loop input** to the RL substrate. Without
//! it, the bandit cannot learn; the routing is random.
```

## Sample 10 — `touring-foundation::health_events`

```rust
//! # Boundary: touring-foundation::health_events
//!
//! **Inputs**: health events (component, status, timestamp, message).
//! **Outputs**: `HealthEvent` struct, append-only log,
//!              `health_events::query(since: DateTime) -> Vec<HealthEvent>`.
//! **Invariants**:
//!   - I1: Health events are monotonic in time (no out-of-order).
//!   - I2: Status transitions follow the state machine: healthy → warn →
//!         degraded → unhealthy → healthy (any direction).
//!   - I3: Each event has a unique `seq` (monotonic counter).
//! **Tier**: free.
//! **Stability**: 2 (stable).
//!
//! # Why this boundary matters
//!
//! Health events are the **temporal trace** of the daemon. They're the
//! input to time-series dashboards, alert rules, and post-incident
//! forensics.
```

## Summary

| Sample | Module | Status |
|--------|--------|--------|
| 1-5 | types, error, config, health, profile | ✅ (in W2 sample) |
| 6-10 | alloc, migration, drift, feedback, health_events | ✅ (this batch) |

**10 of ~100 modules in touring-foundation have boundary docs (~10% of L1).**
**The remaining 90 modules** (5 in foundation + 100+ in 12 other Core
crates + 80+ in 11 Internal crates) are future-session work following
the same template.

---

_W2 expansion 2026-06-04 (10 modules documented). Future waves continue._
