# TACO Iteration 8 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: 1452 passing (touring-hooks lib tests)
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 8 delivered 5 engineering changes (EC6, EC7, EC8, EC9, P3) expanding the
`AsyncFileKnowledgeDB` wiring surface and closing a telemetry gap in the VGP engine.

EC6, EC7, EC9 follow the fire-and-forget pattern established in EC5 (Iter 7).
EC8 is the most architecturally significant: it ensures the async DB WAL reaches disk
during graceful daemon shutdown via a two-pass refactor that avoids a subtle Rust
`Send` analysis issue with `MutexGuard` + `await` in `tokio::spawn` contexts.

---

## EC6 — AsyncFileKnowledgeDB.record_bash_outcome from post_bash.rs

**File**: `crates/touring-hooks/src/post_bash.rs`

**What**: After the existing synchronous `runtime.ctx.knowledge.record_bash_outcome(&outcome)`
call (~line 73), a fire-and-forget spawn was added to also call the async variant on
`AsyncFileKnowledgeDB`. The `BashOutcome` struct was extended with `#[derive(Clone)]` to
support the move into the spawned future.

**Pattern**:
```rust
if let Ok(handle) = tokio::runtime::Handle::try_current() {
    let adb = runtime.ctx.async_knowledge.clone();
    let outcome_clone = outcome.clone();
    handle.spawn(async move { let _ = adb.record_bash_outcome(&outcome_clone).await; });
}
```

**Design decision — dual write (sync + async)**: The sync call maintains backward
compatibility and immediate consistency. The async call is fire-and-forget — it adds
the outcome to the deadpool-sqlite write queue without blocking the hook handler.
Both writes are idempotent via UNIQUE constraints.

---

## EC7 — AsyncFileKnowledgeDB.record_access from pre_read.rs

**File**: `crates/touring-hooks/src/pre_read.rs`

**What**: After the HeatMap `hm.record_access(file_path, now)` block (~line 169), a
fire-and-forget spawn was added to call `AsyncFileKnowledgeDB.record_access()` with
the relative path and session ID.

**Pattern**:
```rust
if let Ok(handle) = tokio::runtime::Handle::try_current() {
    let adb = runtime.ctx.async_knowledge.clone();
    let path_str = rel_path.to_string();
    let session_id = std::env::var("CLAUDE_SESSION_ID").unwrap_or_default();
    handle.spawn(async move { let _ = adb.record_access(&path_str, &session_id).await; });
}
```

**Design decision — HeatMap vs async DB**: These are complementary, not redundant.
HeatMap is an in-memory hot-cache for quick access frequency lookups. The async DB
persists access history across daemon restarts and supports cross-session analytics.

---

## EC8 — AsyncFileKnowledgeDB.wal_checkpoint in daemon.rs graceful_shutdown

**File**: `crates/touring-hooks/src/daemon.rs`

**What**: `run_graceful_shutdown()` now calls `wal_checkpoint()` on every
`async_knowledge` instance before `process::exit`, ensuring deadpool-sqlite WAL
frames reach disk.

**Two-pass refactor — the key architectural insight**:

The naive approach — `drop(rt)` before `.await` inside a `tokio::spawn` — looks
correct at runtime but **fails Rust's static `Send` analysis**:

```rust
// WRONG — does not compile:
tokio::spawn(async move {
    let mut guard = mutex.lock().unwrap();
    drop(guard);          // runtime says: guard dropped here
    adb.wal_checkpoint().await;  // but Send analysis sees MutexGuard in scope
});
```

Rust's future `Send` analysis is **syntactic**, not NLL-aware. Even though `guard` is
dropped before the `.await` point, the compiler sees `MutexGuard<T>` (which is `!Send`)
as a live variable across the `await`, causing a compile error.

**Solution — two-pass loop**:

```rust
// Phase 1: hold MutexGuard, collect async_knowledge clones (no await)
let async_dbs: Vec<Arc<AsyncFileKnowledgeDB>> = {
    let ctx_map = contexts.lock().unwrap();
    ctx_map.values().map(|ctx| ctx.async_knowledge.clone()).collect()
};
// Phase 2: MutexGuard is gone, now safe to await
for adb in async_dbs {
    let _ = adb.wal_checkpoint().await;
}
```

By splitting into two separate `for` loops — one that holds the `MutexGuard` to collect
clones and one that awaits — the `MutexGuard` never lives across an `await` point, and
the futures are `Send`.

**Design decision — sync await vs fire-and-forget**: Unlike EC6/EC7/EC9, EC8 uses
synchronous await (not fire-and-forget). This is intentional: graceful_shutdown is the
last chance to flush. Fire-and-forget would be discarded when the process exits.

---

## EC9 — AsyncFileKnowledgeDB.wal_checkpoint from session_hooks.rs

**File**: `crates/touring-hooks/src/session_hooks.rs`

**What**: At the end of `run_session_stop()` (~line 408, before `Ok(())`), a
fire-and-forget spawn was added to call `wal_checkpoint()` on the session's
`async_knowledge` instance.

**Pattern**:
```rust
if let Ok(handle) = tokio::runtime::Handle::try_current() {
    let adb = runtime.ctx.async_knowledge.clone();
    handle.spawn(async move { let _ = adb.wal_checkpoint().await; });
}
```

**Design decision — EC9 is opportunistic, EC8 is authoritative**: EC9 fires a
checkpoint at session boundaries (every session stop), providing incremental WAL
flushing during normal operation. EC8 is the authoritative final flush during daemon
shutdown. Both are necessary: EC9 reduces WAL size growth during long sessions; EC8
guarantees no data loss on exit.

---

## P3 — VGP Cache Hit Ratio Metric via TelemetrySink

**File**: `crates/touring-generator/src/vgp/engine.rs`

**What**: After the existing `increment_counter("vgp.verify_batch.calls", 1)` counter
(~line 271), a histogram metric `"vgp.cache.hit_ratio"` is now emitted via
`record_histogram` to close the gap identified in strategy doc section 7.2.

**Pattern**:
```rust
let total = cache_hits_val + cache_misses_val;
if total > 0 {
    telemetry.record_histogram(
        "vgp.cache.hit_ratio",
        f64::from(cache_hits_val) / f64::from(total),
    );
}
```

**Design decision — total > 0 guard**: Without this guard, the first call to
`verify_batch` (when both `cache_hits_val` and `cache_misses_val` are 0) would emit
`0.0 / 0.0 = NaN` to the TelemetrySink. NaN values in histograms corrupt percentile
calculations. The guard ensures the ratio is only emitted when meaningful.

---

## Pre-existing Issue (not caused by Iter 8)

`touring-simd bench_throughput_strong_scaling` fails in the virtualized CI/test environment
due to SIMD performance expectations not met under virtualization. This failure predates
Iter 8 and is unrelated to any changes made in this iteration.

---

## Validation

| Check | Result |
|-------|--------|
| `cargo check --workspace` | exit 0 (0 errors) |
| `touring-hooks` lib tests | 1452 passed, 0 failed |
| workspace (excl. touring-simd) | all pass |
| `touring-simd bench_throughput_strong_scaling` | pre-existing SIMD perf failure (virtualized env) |

---

## Changes Summary

| EC | File | Method | Pattern |
|----|------|--------|---------|
| EC6 | `post_bash.rs` | `record_bash_outcome` | fire-and-forget + `BashOutcome::clone()` |
| EC7 | `pre_read.rs` | `record_access` | fire-and-forget + `CLAUDE_SESSION_ID` env var |
| EC8 | `daemon.rs` | `wal_checkpoint` | two-pass loop (MutexGuard + await separation) |
| EC9 | `session_hooks.rs` | `wal_checkpoint` | fire-and-forget at session boundary |
| P3 | `vgp/engine.rs` | telemetry histogram | `total > 0` guard for NaN prevention |
