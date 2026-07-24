# TACO Iteration 13 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: touring-server 8/8 PASS
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 13 delivers EC18: the `access_count: i64` field added to `GraphFocusCtx` in
`graph_service.rs`. This change completes the read path for file access frequency — the
`AsyncFileKnowledgeDB::access_count()` method (whose underlying table `file_access_log` has
been populated since Iter 7 via EC5) now has one production caller.

Prior to this iteration:
- `GraphFocusCtx` exposed co-edit signals (`coedit_files`, added in GS-EC11/Iter9) and
  graph topology signals, but had no access-frequency signal.
- `AsyncFileKnowledgeDB::access_count()` existed but had zero production callers — orphan
  read path.
- The `inject()` method in GraphService emitted `coedit_files` in `graph_ctx` JSON but
  not `access_count`.

After Iter 13:
- `GraphFocusCtx.access_count` exposes per-file access frequency to every downstream
  consumer of `pre_edit` graph context.
- `resolve_ctx()` queries `adb.access_count(file).await` with graceful `.unwrap_or(0)`.
- `inject()` emits `"access_count"` in the `graph_ctx` JSON block.
- `access_count()` transitions from orphan → 1 production caller.

---

## EC18 — `access_count: i64` added to `GraphFocusCtx`

**File**: `crates/touring-server/src/graph_service.rs`

**Pattern**: Same 4-step pattern established by GS-EC11 (`coedit_files`):
1. Add field + doc comment to struct
2. Add default value (0) to `Default` impl
3. Populate in `resolve_ctx()` via async db call with `.unwrap_or(0)`
4. Emit in `inject()` JSON under `graph_ctx`

### Struct change (step 1 + 2)

```rust
// In GraphFocusCtx:
/// EC18: how many times focused_file has been accessed (from TABLE_FILE_ACCESS_LOG).
/// 0 when focused_file is None or AsyncFileKnowledgeDB is not initialized.
pub access_count: i64,

// In Default impl:
access_count: 0,
```

### resolve_ctx() change (step 3)

```rust
// EC18: file access frequency from TABLE_FILE_ACCESS_LOG.
let access_count: i64 = if let Some(ref adb) = self.async_knowledge {
    adb.access_count(file).await.unwrap_or(0)
} else { 0 };
// ...struct construction includes: access_count,
```

### inject() change (step 4)

```rust
// EC18: file access frequency from TABLE_FILE_ACCESS_LOG.
"access_count": ctx.access_count,
```

---

## Decisions Made

| Decision | Rationale | Alternatives Considered |
|----------|-----------|------------------------|
| `unwrap_or(0)` on `access_count()` | Graceful degradation: DB unavailable or file never accessed both produce a valid `0` for downstream consumers — no error propagation needed for observational data | Return `Option<i64>`; use `?` operator — rejected because callers want a scalar for JSON serialization |
| `i64` type (not `u64` or `usize`) | Matches SQLite INTEGER type (which is signed). `access_count()` returns `i64` from `rusqlite`. Using `i64` avoids a cast and is consistent with Iter 12 (EC17a used `i64` for COUNT(*) fields) | `u32` — rejected: COUNT(*) can exceed u32 on very active projects; `u64` — rejected: SQLite binding is i64 |
| Same 4-step pattern as GS-EC11 | GS-EC11 established a proven template for adding async DB fields to GraphFocusCtx. Deviating would require architectural justification. Consistency = lower cognitive load for future contributors | Alternative inject approach (separate method) — rejected: unnecessary indirection |
| `else { 0 }` branch when `async_knowledge` is None | GraphService can run without AsyncFileKnowledgeDB (e.g. in test mode). The `0` sentinel is the correct default matching `Default::default()` | Panic/error when adb absent — rejected: test contexts and degraded mode both need graceful behavior |

---

## Files Changed

| File | Change | Impact |
|------|--------|--------|
| `crates/touring-server/src/graph_service.rs` | EC18: 4 additions (field, default, resolve_ctx, inject) | `GraphFocusCtx` gains `access_count: i64`; downstream consumers of `pre_edit` graph_ctx JSON gain `"access_count"` key |

---

## Validation Results

| Check | Result |
|-------|--------|
| `cargo check --workspace` | exit 0 — 0 errors, `Finished dev profile in 2.55s` |
| `cargo test -p touring-server` | 8/8 PASS, 0 failed |
| New orphan pub symbols | 0 |
| Regression | None |

---

## Relationship to Prior Iterations

| Iter | Change | Relationship to EC18 |
|------|--------|----------------------|
| Iter 7 (EC5) | `post_read` wires `file_access_log` writes via `adb.record_access()` | EC18 reads what EC5 writes |
| Iter 9 (GS-EC11) | `coedit_files` added to GraphFocusCtx — 4-step pattern established | EC18 follows identical pattern |
| Iter 12 (EC17a) | `stats()` stub fields filled; `access_count()` confirmed returning real data | EC18 depends on EC17a for accuracy; without EC17a, `access_count()` would have been reading real data anyway (separate method from stats) |

---

## Architecture Impact

`access_count` is now part of the `graph_ctx` JSON injected into every `pre_edit` hook
invocation when a focused file is present. This means:

- The Claude Code `pre-edit` hook context gains file access frequency without any additional
  tooling or queries.
- Future TACO engineers can use `ctx.graph_ctx.access_count` to weight suggestions, prioritize
  co-edit partners, or calibrate blast-radius warnings by file activity.
- The signal is zero-cost when `AsyncFileKnowledgeDB` is absent (test contexts, cold start).

---

## Next Steps (for future iterations)

- EC19 candidate: Expose `bash_count` or `edit_count` from AsyncFileKnowledgeDB in GraphFocusCtx
  following the same 4-step pattern — extends the observability surface further.
- Integration test: add an integration test that verifies `access_count > 0` in graph_ctx JSON
  after a `record_access()` call — closes the test coverage gap for EC18.

---

*Session report generated by touring-scriber | TACO Iter13 | 2026-04-11*
