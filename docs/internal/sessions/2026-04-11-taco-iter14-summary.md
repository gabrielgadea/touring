# TACO Iteration 14 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: touring-hooks 1/1 PASS
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 14 delivers EC19a and EC19b — two parallel enrichments that bring the
`pre_write` hook and the `cli_e2e` knowledge phase to feature parity with their
counterparts in `pre_edit` (EC14/Iter11) and `cli_wiring_status` (EC17b/Iter12).

Both changes follow the "mirror and adapt" pattern: they replicate an already-proven
signal collection mechanism from a sibling hook/handler, adjusting only the API
surface differences (field names, accessor style) dictated by each context.

---

## EC19a — Signal 12: co-edit neighbors in `pre_write.rs`

**File**: `crates/touring-hooks/src/pre_write.rs`

**Function**: `collect_upfront_signals()`

### What changed

Signal 12 was added to `collect_upfront_signals()` in `pre_write.rs`. It queries
co-edit neighbors for the file currently being written and appends the result to the
upfront signals vector emitted before any write is applied.

**Signal text format**: `"co-edits: N file(s) frequently written together [file1, file2, ...]"`

### How it mirrors EC14 (pre_edit.rs, Iter11)

EC14 added the identical signal to `pre_edit.rs`. The semantic intent is the same:
surface temporal coupling between files so that the agent is aware of blast radius
before applying a change.

### Key API differences vs pre_edit.rs

| Aspect | `pre_edit.rs` (EC14) | `pre_write.rs` (EC19a) |
|--------|---------------------|------------------------|
| DB accessor | `db` (bare `FileKnowledgeDB`) | `runtime.ctx.knowledge` (`FileKnowledgeDB` via `CognitiveRuntime`) |
| File path variable | `file_path` | `rel_path` |
| Join separator | `.short_list()` (helper) | `.join(", ")` (stdlib) |
| Signal slot | Signal 12 | Signal 12 |
| Score | 1.1 | 1.1 |
| Neighbor count | 5 | 5 |

The differences are purely structural — the underlying data source (`TABLE_FILE_COEDITS`)
and the information conveyed are identical.

### Design decisions

**Decision — use `runtime.ctx.knowledge` not bare `db`**: `pre_write.rs` does not
receive a `FileKnowledgeDB` reference directly. The `CognitiveRuntime` exposes it
via `ctx.knowledge`. Using the runtime-provided accessor is consistent with all
other knowledge calls in `pre_write.rs` and avoids coupling to a specific injection
path.

**Decision — `.join(", ")` not `.short_list()`**: `short_list()` is a custom helper
available in `pre_edit.rs` via its imports. In `pre_write.rs` the stdlib `.join()`
achieves the same output for 5 elements with zero additional import surface.

**Decision — graceful degradation via `.unwrap_or_default()`**: If `knowledge` is
None or the query fails, the signal is silently skipped. This follows the same
resilience pattern as all other upfront signals: observational signals must never
block or panic write operations.

---

## EC19b — `access_count` + `knowledge_activity` in `cli_e2e.rs`

**File**: `crates/touring-hooks/src/cli_e2e.rs`

**Function**: `phase_knowledge()`

### What changed

`phase_knowledge()` now emits two additional fields in its JSON output:

1. `"access_count"` — an `i64` from `SELECT COUNT(*) FROM file_access_log`
2. `"knowledge_activity"` — a structured sub-object with 5 fields:
   - `access_count`
   - `bash_count`
   - `edit_count`
   - `gotcha_total`
   - `coedit_pairs`

### How it mirrors EC17b (cli_wiring_status, Iter12)

EC17b added `knowledge_activity` to `cli_wiring_status` output. EC19b replicates
the same structure in the E2E phase output, so both `touring wiring status -j` and
`touring e2e -j` now expose the knowledge capture activity surface consistently.

### Variable reuse (no new queries)

The 5 count values in `knowledge_activity` were already computed as local variables
within `phase_knowledge()` before EC19b. EC19b only reorganizes them into the
structured JSON sub-object and adds the top-level `access_count` field (which
mirrors the first of the 5 sub-fields for quick access).

**Design decision — no new SQL queries**: `bash_count`, `edit_count`, `gotcha_total`,
and `coedit_pairs` were already queried via `SELECT COUNT(*)` in the function body.
EC19b adds zero additional DB round-trips. Only the output serialization changes.

**Design decision — `access_count` at top level AND inside `knowledge_activity`**:
Consumers that already parse the flat output see `access_count` at the expected path.
Consumers that read the structured sub-object also find it there. This dual presence
avoids breaking existing parsers while enabling structured access.

---

## Validation

```
cargo check --workspace   → exit 0 (0 errors)
cargo test -p touring-hooks → 1/1 PASS
```

---

## Files Changed

| File | Change |
|------|--------|
| `crates/touring-hooks/src/pre_write.rs` | Signal 12 (co-edit neighbors) added to `collect_upfront_signals()` |
| `crates/touring-hooks/src/cli_e2e.rs` | `access_count` + `knowledge_activity` structured output in `phase_knowledge()` |

---

## Pattern Established

Iter14 confirms the "mirror and adapt" pattern as the primary delivery mechanism for
Pln2 enrichments:

1. Identify a proven signal/output in hook A
2. Locate the analogous function in hook B
3. Adapt API surface (accessor names, variable names, join helpers) to hook B's context
4. Validate: cargo check + targeted test
5. Document: session summary + PLAN changelog entry + memory store

This pattern reduces implementation risk (adapting existing code, not writing new logic),
enables fast delivery (one EC per iter), and produces a verifiable audit trail.
