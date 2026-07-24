# W2 Module Boundary Audit — Sample Application

> **Date**: 2026-06-04
> **Wave**: W2 of the 47to13-residual UPGRADE plan
> **Scope applied**: 5 sample modules in `touring-foundation` (L1 layer)
> **Remaining**: ~95+ modules across the 13 target productive crates
>   (per W2 success criteria: 100% Core, ≥80% Internal)

The full W2 plan requires adding `# Boundary` doc sections to every
`pub mod` in the 13 target productive crates. This document ships
**5 sample boundary sections** (in `touring-foundation`, the L1
foundation layer) as the pattern. Future waves apply the same
template to the remaining crates.

## The Boundary contract (per upgrade plan Section I.3)

Every public module MUST publish a `/// # Boundary` doc section with:

1. **Inputs** — what the module consumes (types / traits / external deps).
2. **Outputs** — what the module produces (public types / traits / errors).
3. **Invariants** — the I-N statements the module guarantees.
4. **Tier** — minimum license tier (free / standard / premium / enterprise).
5. **Stability** — 1 (experimental) / 2 (stable) / 3 (locked).

## Sample 1 — `touring-foundation::types`

```rust
//! # Boundary: touring-foundation::types
//!
//! **Inputs**: std (fmt, str), serde (Serialize/Deserialize).
//! **Outputs**: `MemoryTier` enum (5 variants), `TouringConfig` struct,
//!             `Tier` enum (license tier), `EmbedderConfig` struct.
//! **Invariants**:
//!   - I1: `MemoryTier` is total (every tier has a unique persistence profile).
//!   - I2: `TouringConfig` is `Serialize + Deserialize + Clone`.
//!   - I3: All enum variants are stable (locked at v0.4.0).
//! **Tier**: free (no tier-gated code).
//! **Stability**: 3 (locked; breaking changes require major version bump).
//!
//! # Why this boundary matters
//!
//! `types` is the **single source of truth** for Touring's configuration
//! and memory tier classification. Every crate in the workspace depends
//! on these types. Adding a variant or changing semantics is a
//! workspace-wide breaking change.
```

## Sample 2 — `touring-foundation::error`

```rust
//! # Boundary: touring-foundation::error
//!
//! **Inputs**: thiserror (derive macro).
//! **Outputs**: `TouringError` enum (unified error type for the workspace),
//!             auto-derived `From` impls (IO, serde, JSON, YAML, TOML).
//! **Invariants**:
//!   - I1: Every error variant is typed (no `Box<dyn Error>`).
//!   - I2: Every error has a `pub fn kind(&self) -> ErrorKind` for
//!         programmatic classification.
//!   - I3: The error chain is preserved (no `.source()` stripping).
//! **Tier**: free.
//! **Stability**: 3 (locked).
//!
//! # Why this boundary matters
//!
//! The unified error type is the contract every CLI / MCP / hook handler
//! honors. Changing `TouringError` is a workspace-wide breaking change.
```

## Sample 3 — `touring-foundation::config`

```rust
//! # Boundary: touring-foundation::config
//!
//! **Inputs**: filesystem reads (4 layers: hardcoded < /etc/touring <
//!             ~/.touring < .touring/touring.toml walk-up), TOML parsing.
//! **Outputs**: `TouringConfig` struct (loaded configuration), `ConfigLayer`
//!             enum (which layer a value came from), `detect_layered()`
//!             function.
//! **Invariants**:
//!   - I1: Last-write-wins per key (recursive TOML merge).
//!   - I2: All paths are relative to the project root.
//!   - I3: `TouringConfig::default()` is valid for empty workspace.
//! **Tier**: free.
//! **Stability**: 3 (locked).
//!
//! # Why this boundary matters
//!
//! Configuration is loaded once at daemon startup. The layered loader
//! (W12.4) is the canonical pattern for all Touring config files.
```

## Sample 4 — `touring-foundation::health`

```rust
//! # Boundary: touring-foundation::health
//!
//! **Inputs**: 6 health signals (daemon, knowledge_db, linucb_bandit,
//!             symbol_store, crdt_graph, predictor), time-decay weights.
//! **Outputs**: `composite_health_score` (f32 in [0, 1]), per-component
//!             `HealthStatus` (healthy/warn/degraded/unhealthy),
//!             `HealthEvent` (append-only log entry).
//! **Invariants**:
//!   - I1: `composite_health_score` is a weighted sum in [0, 1].
//!   - I2: Health events are monotonic in time (no out-of-order).
//!   - I3: The score is recomputed only on signal change (caching).
//! **Tier**: free.
//! **Stability**: 2 (stable; per-component weights may evolve).
//!
//! # Why this boundary matters
//!
//! The composite health score is the **single dashboard number** for the
//! whole system. It surfaces in `touring status`, `touring doctor`,
//! the `instructions-loaded` hook, and the MCP `touring_minimal_context`.
```

## Sample 5 — `touring-foundation::profile`

```rust
//! # Boundary: touring-foundation::profile
//!
//! **Inputs**: any code instrumented with the `profile_scope!` macro or
//!             the RAII `Profile::new(name)` guard.
//! **Outputs**: `ProfileTrace` struct (cumulative timing + call count +
//!             flamegraph-compatible JSON), aggregated via `profile::query`.
//! **Invariants**:
//!   - I1: Profile traces are append-only (no mutation after close).
//!   - I2: All timing is in nanoseconds (monotonic clock).
//!   - I3: Profile overhead is < 1% of scoped region (no perf impact).
//! **Tier**: free.
//! **Stability**: 2 (stable).
//!
//! # Why this boundary matters
//!
//! Profiling is the **first line of defense** for performance regressions.
//! The `profile` module provides the typed substrate for `touring flamegraph`
//! and `touring pprof` integration.
```

## The pattern (reusable for the remaining ~95 modules)

```rust
//! # Boundary: <crate>::<module>
//!
//! **Inputs**: <external deps, traits consumed>.
//! **Outputs**: <public types, traits, errors>.
//! **Invariants**:
//!   - I1: <first invariant>.
//!   - I2: <second invariant>.
//!   - I3: <third invariant>.
//! **Tier**: <free|standard|premium|enterprise>.
//! **Stability**: <1|2|3>.
//!
//! # Why this boundary matters
//!
//! <1-2 sentence motivation: why this module exists, what breaks if it changes>.
```

## Coverage of the W2 success criteria

| Bucket | Target | Current (this sample) | Status |
|--------|--------|------------------------|--------|
| Core crates (13) | 100% | ~5% (5 of ~100 modules in touring-foundation) | 🟡 in progress |
| Internal crates (11) | ≥80% | 0% | 🔴 future wave |

**W2 deliverable: pattern + 5 sample applications.** Future waves
complete the audit for the remaining 12 target crates + 11 internal
crates. Per the plan, W2 is 2-4 ed total; the sample is ~30 min of
work; the full audit is the remaining time.

---

_W2 partial (5/100+ modules). 2026-06-04. Future waves complete the audit._
