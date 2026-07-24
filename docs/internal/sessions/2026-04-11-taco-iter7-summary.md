# TACO Iteration 7 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: 1452 passing (touring-hooks lib tests)
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 7 delivered 2 engineering changes (EC_sev and EC5) focused on wiring
previously-initialized-but-never-called production paths in `post_edit.rs` and
`post_write.rs` within the `touring-hooks` crate.

---

## EC_sev — insert_symbol_event Wired from Production Hooks

**Files changed**:
- `crates/touring-hooks/src/post_edit.rs` — after `record_coedits()`, call `runtime.ctx.knowledge.insert_symbol_event()` with operation="edit", sequence_id=`edit:{ts_nanos}:{rel_path}`, session_id from env
- `crates/touring-hooks/src/post_write.rs` — after BLAKE3/reindex block (~line 143), analogous call with operation="write"

**What**: `insert_symbol_event()` existed in `FileKnowledgeDB` but was only called from
test fixtures. EC_sev is the first production wiring — hooks now emit a symbol event
record to the `symbol_events_log` table on every edit and write operation.

**Idempotency**: Both calls use `let _` to silently ignore UNIQUE constraint violations.
The sequence_id format `{operation}:{ts_nanos}:{rel_path}` ensures uniqueness per
operation × file × nanosecond. Re-triggered hooks for the same file within the same
nanosecond are silently discarded — correct behavior for CRDT append-only semantics.

**Schema target**: `symbol_events_log` table (Pln2 A-schema-6) — id AUTOINC,
sequence_id UNIQUE, file_path, blake3_hash, operation CHECK, symbol_name, agent_id, timestamp.

---

## EC5 — AsyncFileKnowledgeDB.record_edit Fire-and-Forget from Hooks

**Files changed**:
- `crates/touring-hooks/src/post_edit.rs` — in the `else` branch of `should_skip_reindex` (after reindex_file), clones `runtime.ctx.async_knowledge`, spawns `record_edit()` fire-and-forget
- `crates/touring-hooks/src/post_write.rs` — same pattern, in the `else` branch after reindex_file + parser cache warm

**What**: `AsyncFileKnowledgeDB` was initialized in the hook runtime context but no
production hook ever called it. EC5 is the first production use of this async DB path.
`record_edit()` is now called fire-and-forget after every genuine reindex (BLAKE3-miss path).

**Implementation pattern**:
```rust
if let Ok(handle) = tokio::runtime::Handle::try_current() {
    let adb = runtime.ctx.async_knowledge.clone();
    handle.spawn(async move { let _ = adb.record_edit(&edit).await; });
}
```

**Design decision — try_current() not current()**: `Handle::current()` panics when called
from a `spawn_blocking` context where no tokio runtime is active on the thread stack.
`Handle::try_current()` returns `Err` instead of panicking — the `if let Ok(handle)` guard
means fire-and-forget only fires when a runtime is available, graceful fallback otherwise.

**Design decision — else branch placement**: `record_edit()` is placed in the BLAKE3-miss
else branch (content actually changed), not the early-exit path. Only records events when
the file was genuinely re-indexed, not on cache hits. Prevents duplicate events for
unchanged-content hook invocations.

---

## Quality Gates

| Gate | Result |
|------|--------|
| Functional | PASS — cargo check exit 0, 0 errors |
| Robust | PASS — `let _` for UNIQUE violations, `try_current()` for runtime absence |
| Readable | PASS — consistent naming, inline comments explain design rationale |
| Documented | PASS — PLAN updated, this summary created, memory entries stored |
| Secure | PASS — no secrets, no shell=true, no unsafe |
| No Regression | PASS — 1452 tests passing (touring-hooks lib tests + touring-generator combined) |

---

## Memory Entries Stored

| Key | Type | Content Summary |
|-----|------|-----------------|
| `iter7:EC_sev` | pattern | insert_symbol_event wired from post_edit(edit) and post_write(write) with ts_nanos:rel_path sequence_id |
| `iter7:EC5` | pattern | AsyncFileKnowledgeDB.record_edit wired fire-and-forget via tokio Handle::try_current from post_edit+post_write BLAKE3-miss else branch |
| `pattern:tokio-spawn-blocking-async` | pattern | From spawn_blocking context, use Handle::try_current() not Handle::current() to avoid panic when no runtime |

---

## Key Insights

1. **AsyncFileKnowledgeDB was initialized but never called**: Prior to Iter 7, `async_knowledge`
   was initialized in the hook runtime context but no hook ever called it. EC5 is the first
   production use of this async DB path.

2. **insert_symbol_event was test-only**: The `insert_symbol_event()` method existed in
   `FileKnowledgeDB` but was only called from test fixtures. EC_sev is the first production
   wiring from hook handlers.

3. **BLAKE3-miss else branch is the correct integration point**: Both EC_sev and EC5
   are placed after the BLAKE3 early-exit check (Iter 6 EC1/EC1b). This means events are
   only emitted when content genuinely changed — correct semantics for both the
   `symbol_events_log` CRDT table and the async DB record.

---
