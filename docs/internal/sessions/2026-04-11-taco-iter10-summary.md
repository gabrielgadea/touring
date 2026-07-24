# TACO Iteration 10 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: touring-hooks 1452/1452 PASS
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 10 delivers two changes that surface the co-edit signal from TABLE_FILE_COEDITS
in two additional CLI surfaces: `touring wiring suggest` (EC12) and `touring ast blast` (EC13).

Prior to this iteration, `touring wiring suggest` always returned empty results in production
because TABLE_WIRING_SUGGESTIONS is only populated by test code. After Iter 10, the handler
computes suggestions on-demand using `get_coedit_neighbors()` and caches them best-effort.

`touring ast blast` previously returned only structural import consumers. After Iter 10, the
response also includes `coedit_files` — files that are historically edited alongside the
target file, providing a temporal coupling signal to complement the structural one.

A VP-Scout false positive was avoided: the PLAN doc mentioned `verify_batch_parallel` as a
symbol to integrate, but `touring index find` returned count=0. Symbol does not exist.

---

## EC12 — cli_wiring_suggest: Live Co-Edit-Based Suggestions (compute-and-cache)

**File**: `crates/touring-hooks/src/cli_handlers.rs`

**Problem**: `touring wiring suggest` always returned empty results. TABLE_WIRING_SUGGESTIONS
is only populated by `upsert_wiring_suggestion()`, which only has callers in test code
(`pln2_e2e.rs`). Production path never writes to this table.

**Fix**: Two-phase handler replacing the previous 36-line single-phase read-only impl:

### Phase 1 (cached fast path)
- Query TABLE_WIRING_SUGGESTIONS for rows matching the symbol
- If rows found: return immediately with `"source": "cached"`
- If empty: proceed to Phase 2

### Phase 2 (compute-and-cache)
1. Look up orphan file from TABLE_WIRING_MAP by symbol name
2. If no file found: return empty array with `"source": "no_orphan_file"`
3. Call `db.get_coedit_neighbors(file, 10)` — sync, bidirectional (sums A→B + B→A counts)
4. Normalize scores: `score = count / max_count` → range 0.0–1.0
5. For each neighbor: `upsert_wiring_suggestion()` — best-effort, errors swallowed via `let _ = ...`
6. Return suggestions with `"source": "computed"`

**Key design decisions**:
- `get_coedit_neighbors()` (sync, bidirectional) preferred over `get_coedits_from()` (async, unidirectional) — handler runs in sync context, and bidirectional signal captures symmetrical co-edit coupling
- `upsert_wiring_suggestion()` errors swallowed — suggestions must always be returned; caching is best-effort
- `let Some(ref file) = orphan_file else { ... }` pattern — Rust let-else for graceful fallback

**Implementation size**: 80 lines (replaced 36-line single-phase impl)

---

## EC13 — cli_ast_blast: Coedit Files in Blast Radius

**File**: `crates/touring-hooks/src/cli_handlers.rs`

**Problem**: `touring ast blast <file>` returned only `{file_path, blast_radius, consumers}` —
structural import consumers from TABLE_WIRING_MAP. No temporal co-edit signal.

**Fix**: After the existing consumers query, call `db.get_coedit_neighbors(file_path, 5)` and
include `coedit_files` in the JSON output.

**New output schema**:
```json
{
  "file_path": "src/post_edit.rs",
  "blast_radius": 12,
  "consumers": ["src/lib.rs", "src/hook_registry.rs"],
  "coedit_files": ["src/pre_read.rs", "src/async_knowledge.rs"]
}
```

**Implementation size**: 7-line addition after the consumers query block

**Signal complementarity**:
- `consumers` = structural coupling (what imports this file via TABLE_WIRING_MAP)
- `coedit_files` = temporal coupling (what is edited alongside this file historically)
- Together: fuller blast radius picture for risk assessment during pre-edit analysis

---

## VP-Scout False Positive Avoided

**Original PLAN doc claim**: "verify_batch_parallel" mentioned as a symbol to integrate.

**VP-Scout Chain 3 execution**:
```
touring index find "verify_batch_parallel" → count=0
```

**Verdict**: Symbol does not exist in codebase. PLAN doc described INTENT not ground truth.
Task discarded before reaching engineers. 1 false positive avoided.

---

## Decisions Made

### Decision 1 — EC12: Sync get_coedit_neighbors over async get_coedits_from

**Decision**: Use `get_coedit_neighbors()` (sync, bidirectional) not `get_coedits_from()` (async, unidirectional).

**Rationale**: `cli_wiring_suggest` handler runs in synchronous context. Using the async method would require a `block_on()` wrapper adding complexity and potential deadlock risk. The bidirectional signal (sums A→B + B→A counts) is richer for wiring suggestions since symmetrical co-edit history indicates stronger coupling.

**Alternative considered**: Use async `get_coedits_from()` with `block_on()` wrapper — rejected due to complexity and unidirectional limitation.

### Decision 2 — EC12: Best-effort upsert (errors swallowed)

**Decision**: Swallow `upsert_wiring_suggestion()` errors via `let _ = result`.

**Rationale**: Suggestions must always be returned to caller even if caching fails. Caching is an optimization. A DB error on upsert does not degrade the quality of the returned suggestions.

**Alternative considered**: Log the error and return it to the caller — rejected because it would fail the whole request on a transient cache write error.

### Decision 3 — EC12: let-else for orphan file lookup

**Decision**: Use `let Some(ref file) = orphan_file else { return empty }` Rust pattern.

**Rationale**: When a symbol is not in TABLE_WIRING_MAP (i.e., it is already wired / has consumers), there is no meaningful coedit file to derive suggestions from. Returning an empty array with `source=no_orphan_file` is the correct behavior rather than an error.

---

## Changes Made

| File | Change | Impact |
|------|--------|--------|
| `crates/touring-hooks/src/cli_handlers.rs` | `cli_wiring_suggest`: replaced 36-line single-phase impl with 80-line two-phase compute-and-cache impl | `touring wiring suggest` now returns live results in production |
| `crates/touring-hooks/src/cli_handlers.rs` | `cli_ast_blast`: added 7-line coedit_files block after consumers query | `touring ast blast` now includes temporal coupling signal |

---

## Validation Results

| Suite | Result |
|-------|--------|
| `cargo check -p touring-hooks` | exit 0 — 0 errors |
| `cargo test -p touring-hooks --lib` | 1452/1452 PASS |
| `cargo check --workspace` | exit 0 — 0 errors |

---

## Architectural Impact

### Before Iter 10

```
touring wiring suggest <symbol>
  → query TABLE_WIRING_SUGGESTIONS
  → always empty (test-only writes)
  → return []

touring ast blast <file>
  → query TABLE_WIRING_MAP consumers
  → return {file_path, blast_radius, consumers}
```

### After Iter 10

```
touring wiring suggest <symbol>
  → Phase 1: query TABLE_WIRING_SUGGESTIONS
    → if cached: return {suggestions, source: "cached"}
  → Phase 2: find orphan file in TABLE_WIRING_MAP
    → get_coedit_neighbors(file, 10) bidirectional sync
    → normalize scores 0.0-1.0
    → upsert best-effort
    → return {suggestions, source: "computed"}

touring ast blast <file>
  → query TABLE_WIRING_MAP consumers
  → get_coedit_neighbors(file_path, 5)
  → return {file_path, blast_radius, consumers, coedit_files}
```

---

## Connection to Pln2 Goals

Iter 10 advances Pln2 dimension **(g) Integração Sistêmica**:

> "touring wiring suggest automated (LeidenCluster + FunctionalSignature match) = 500+ orphans/dia"

EC12 activates `touring wiring suggest` in production for the first time. The co-edit signal
from get_coedit_neighbors is the first live source of wiring suggestions (pre-Leiden wiring
based on edit history, not community detection).

Also advances **(d) Aplicabilidade**:

> "touring ast blast enriched with coedit_files for fuller blast radius assessment"

EC13 makes blast radius analysis more actionable by distinguishing structural vs temporal coupling.

---

## Issues Encountered

None. VP-Scout correctly identified the `verify_batch_parallel` false positive before implementation.

---

## Next Steps

- [ ] EC14: Wire `touring wiring suggest` output into TACO Phase 4.5 anti-FP gate (use computed suggestions to pre-filter orphan tasks)
- [ ] EC15: Monitor TABLE_WIRING_SUGGESTIONS row count growth: `touring memory recall "wiring suggestions count"`
- [ ] Consider adding `coedit_files` to `touring e2e --depth deep` analysis report
- [ ] Consider LeidenCluster integration in Phase 2 to replace pure coedit-based scoring with community-overlap scoring
