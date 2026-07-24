# TACO Iteration 12 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: touring-hooks 1452/1452 PASS
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 12 delivers two changes that complete the observability surface of `AsyncFileKnowledgeDB`
and `cli_wiring_status`. Both changes target previously stub-filled zero fields, replacing them
with real SQL queries against existing tables.

Prior to this iteration:
- `AsyncFileKnowledgeDB::stats()` returned hardcoded `0` for `access_count`, `bash_count`,
  `edit_count`, and `gotcha_count`. These tables were being written to (by EC5, EC6, EC7 in
  Iters 7-8) but the stats surface never reflected them.
- `touring wiring status` output a flat JSON with orphan/module/symbol counts but had no
  visibility into the knowledge activity tables that back the async DB layer.

After Iter 12:
- `stats()` now queries its four real activity tables, making the stats surface accurate.
- `cli_wiring_status` enriches its output with a `knowledge_activity` sub-object containing
  5 metrics (access, bash, edit, gotcha, coedit_pairs), giving operators a single command
  to assess both wiring health and knowledge capture activity.

---

## EC17a — `AsyncFileKnowledgeDB::stats()` — 4 stub zero fields filled

**File**: `crates/touring-hooks/src/async_knowledge.rs`

**Location**: `stats()` method, `interact` closure

**Problem**: The `stats()` method returned `KnowledgeStats` with 4 hardcoded zero fields:
```rust
access_count: 0,
bash_count: 0,
edit_count: 0,
gotcha_count: 0,
```
These fields correspond to real tables (`file_access_log`, `bash_outcomes`, `edit_history`,
`gotchas`) that were being actively populated since Iters 7-8 (EC5, EC6, EC7). The stats
surface was technically correct in schema but observationally blind — it reported 0 even
when thousands of records existed.

**Fix**: Each field now issues a `conn.query_row("SELECT COUNT(*) FROM <table>", [], |r| r.get(0))`
query via the interact closure, using `.unwrap_or(0)` for resilience:

```rust
access_count: conn.query_row("SELECT COUNT(*) FROM file_access_log", [], |r| r.get(0))
    .unwrap_or(0),
bash_count: conn.query_row("SELECT COUNT(*) FROM bash_outcomes", [], |r| r.get(0))
    .unwrap_or(0),
edit_count: conn.query_row("SELECT COUNT(*) FROM edit_history", [], |r| r.get(0))
    .unwrap_or(0),
gotcha_count: conn.query_row("SELECT COUNT(*) FROM gotchas", [], |r| r.get(0))
    .unwrap_or(0),
```

`task_metrics_count` remains `0` — TABLE_TASK_METRICS is not present in this schema version.
This is intentional: the field is reserved for a future schema upgrade.

**Design decision — `unwrap_or(0)` not `?`**: Consistent with the existing pattern for
`file_count` and `relation_count` in the same method. Missing or empty tables degrade
gracefully to `0` rather than propagating an error that would cause `stats()` to return
an `Err` variant. Stats queries are observational — a missing table should not prevent
the caller from receiving the rest of the stats.

**Impact**: `touring memory stats` and any MCP caller using `stats()` now receives accurate
counts for all 4 activity tables. The observability gap between "data written" and "data
visible in stats" is closed.

**Implementation size**: 4 query additions within the existing `interact` closure.

---

## EC17b — `cli_wiring_status` — `knowledge_activity` enrichment

**File**: `crates/touring-hooks/src/cli_handlers.rs`

**Location**: `cli_wiring_status` handler

**Problem**: `touring wiring status` returned a useful but incomplete diagnostic — it showed
wiring-layer health (orphan count, module count, pub symbols, consumers) but had no visibility
into whether the knowledge capture tables were active. An operator diagnosing a "cold" daemon
(no async DB activity) could not distinguish between "no edits happened" vs "edits happened
but recording is broken" from `wiring status` alone.

**Fix**: After constructing `WiringStatus`, 5 sync queries are executed:

```rust
let access_count: i64 = db.query_row("SELECT COUNT(*) FROM file_access_log", ...)?;
let bash_count: i64 = db.query_row("SELECT COUNT(*) FROM bash_outcomes", ...)?;
let edit_count: i64 = db.query_row("SELECT COUNT(*) FROM edit_history", ...)?;
let gotcha_count: i64 = db.query_row("SELECT COUNT(*) FROM gotchas", ...)?;
let coedit_pairs: i64 = db.query_row("SELECT COUNT(*) FROM file_coedits", ...)?;
```

The return changes from `serde_json::to_string(&status)` to a merged JSON object:

```json
{
  "orphan_count": N,
  "module_count": N,
  "total_pub_symbols": N,
  "total_consumers": N,
  "knowledge_activity": {
    "access_count": N,
    "bash_count": N,
    "edit_count": N,
    "gotcha_count": N,
    "coedit_pairs": N
  }
}
```

The `knowledge_activity.coedit_pairs` field is new relative to EC17a — it queries
`TABLE_FILE_COEDITS` (the co-edit signal table introduced in Iter 10/11) and provides
co-edit population health at a glance.

**Design decision — sync queries in cli_wiring_status (no block_on)**: The handler runs
in synchronous context. Sync `db.query_row()` is consistent with the existing handler
pattern throughout `cli_handlers.rs`. Introducing `block_on()` for async queries would
add complexity and a potential panic risk (block_on on a thread inside a tokio runtime).
The 5 COUNT(*) queries are trivially fast (index scans on SQLite tables) — no async
bridging is needed.

**Design decision — JSON merge vs extending WiringStatus struct**: Rather than adding
`knowledge_activity` fields to `WiringStatus` (which would require modifying the struct
definition and potentially breaking callers that pattern-match on it), the handler manually
merges `serde_json::Value` objects. This is a deliberate boundary: `WiringStatus` remains
a wiring-only concern; `knowledge_activity` is a knowledge-layer concern appended at the
serialization boundary.

**Impact**: `touring wiring status -j` is now the single command that assesses both wiring
health and knowledge capture activity. Useful for diagnosing daemon cold-start, verifying
that async DB recording is working after hook changes, and monitoring knowledge table growth
over time.

**Implementation size**: 5 query additions + JSON merge replacing `to_string(&status)`.

---

## Files Changed

| File | Change | Lines Added |
|------|--------|-------------|
| `crates/touring-hooks/src/async_knowledge.rs` | 4 real queries replacing 4 stub zeros in `stats()` | ~8 |
| `crates/touring-hooks/src/cli_handlers.rs` | 5 queries + JSON merge in `cli_wiring_status` | ~20 |

---

## Design Decisions Logged

| Decision | Rationale | Alternative Considered |
|----------|-----------|------------------------|
| `unwrap_or(0)` not `?` in stats() | Observational queries degrade gracefully; missing table ≠ error | `?` propagation — rejected: breaks stats() for partial schemas |
| Sync queries in cli_wiring_status | Consistent with handler pattern; COUNT(*) is trivially fast | `block_on()` async — rejected: complexity + panic risk in tokio context |
| JSON merge vs struct extension | Preserves WiringStatus as wiring-only concern; knowledge is separate layer | Extend WiringStatus struct — rejected: broadens struct responsibility |
| task_metrics_count remains 0 | TABLE_TASK_METRICS not in current schema version | Query non-existent table — rejected: would panic/error on every stats() call |
| coedit_pairs in knowledge_activity | Completes co-edit signal visibility at wiring status level | Separate command — rejected: operators prefer single-command diagnostics |

---

## Validation

- `cargo check --workspace` → exit 0 (0 errors)
- `cargo test -p touring-hooks --lib` → 1452/1452 PASS
- No new orphan pub symbols introduced
- No regression in existing wiring status output shape (additive change)

---

## Iteration Context

This iteration closes the observability gap that opened when EC5/EC6/EC7 (Iters 7-8)
wired async DB recording. Data has been flowing into `file_access_log`, `bash_outcomes`,
`edit_history`, and `gotchas` since then, but the stats surface showed 0. Iter 12
makes the system self-consistent: every table that is written to is now also counted
in the stats surface.

The `knowledge_activity` addition to wiring status follows the same pattern established
by EC15 (E2E phase_knowledge check for coedit_pairs) — making knowledge table health
visible at multiple diagnostic entry points.
