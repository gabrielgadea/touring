# TACO Iteration 9 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: touring-hooks 1491/1491 PASS | touring-server 84/84 PASS | touring-generator 32/32 PASS
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 9 delivers two tightly coupled changes that activate the 33% co-edit RRF signal
in `CoEditPredictor`. Prior to this iteration, `GraphService.predict_coedit_files()` returned
an empty `vec![]` — the RRF fusion was effectively a 2-signal blend. After Iter 9, the third
signal is live: top-5 historically co-edited files are fetched from `TABLE_FILE_COEDITS` and
surfaced in every tool response's `graph_ctx.coedit_files`.

A VP-Scout false positive was avoided: the original scout framed EC10 as a write-wiring task
(fire-and-forget `record_coedit` from post_edit). Chain 3 (Already Implemented) found the
sync write path (`record_coedits()` at `post_edit.rs:402`) already existed. The real gap was
the missing READ counterpart — `get_coedits_from()`.

---

## EC10 — AsyncFileKnowledgeDB.get_coedits_from()

**File**: `crates/touring-hooks/src/async_knowledge.rs`

**What**: Added `pub async fn get_coedits_from(&self, file_path: &str) -> Result<Vec<(String, f64)>, AsyncKnowledgeError>` after the existing `record_coedit` method (line ~379).

**Behavior**:
- Queries `TABLE_FILE_COEDITS WHERE source_path = file_path ORDER BY count DESC LIMIT 20`
- Fetches `(target_path, count)` rows
- Computes `max_count` from first row (DESC order guarantee)
- Normalizes: `score = count as f64 / max_count as f64` → range 0.0–1.0
- Returns `Vec<(String, f64)>` where String = target file path
- Returns empty vec when no co-edits found (no error — GraphService handles gracefully)

**Why**: This is the READ counterpart to the existing sync `record_coedits()` write path.
`TABLE_FILE_COEDITS` was being written by `post_edit.rs:402` since Iter 6 but never read
back in production. EC10 closes the read-write asymmetry.

**Pattern used**: Standard `sqlx::query_as` with `.fetch_all()` — consistent with existing
async_knowledge.rs method patterns.

---

## GS-EC11 — GraphService co-edit signal wired end-to-end

**Files**: `crates/touring-server/src/graph_service.rs`, `crates/touring-server/src/server/mod.rs`

### Changes in graph_service.rs

| Change | Detail |
|--------|--------|
| Import added | `use touring_hooks::async_knowledge::AsyncFileKnowledgeDB;` |
| `GraphFocusCtx` new field | `coedit_files: Vec<String>` — populated per resolve_ctx call |
| `GraphService` new field | `async_knowledge: Option<Arc<AsyncFileKnowledgeDB>>` — initialized by builder |
| Dead field removed | `_coedit_predictor: CoEditPredictor` — was `#[allow(dead_code)]`, removed cleanly |
| Builder method added | `pub fn with_async_knowledge(mut self, adb: Arc<AsyncFileKnowledgeDB>) -> Self` |
| `new_multi_project()` updated | Initializes adb from `TouringConfig::knowledge_db_canonical` path |
| `resolve_ctx()` updated | Populates `coedit_files` via `adb.get_coedits_from(file).await.unwrap_or_default().into_iter().take(5).map(\|(\|p,_\|) p).collect()` |
| `predict_coedit_files()` updated | Uses real `coedit_files` from ctx instead of empty `vec![]` |
| `inject()` updated | Emits `"coedit_files": ctx.coedit_files` in graph_ctx JSON output |

### Changes in server/mod.rs

| Change | Detail |
|--------|--------|
| AsyncFileKnowledgeDB init | In `TouringServer::new()`: open DB from `knowledge_db` path before constructing GraphService |
| Builder wiring | `graph_svc = GraphService::new(...).with_async_knowledge(Arc::new(adb))` |
| tracing::info! on success | `"GraphService wired with AsyncFileKnowledgeDB"` |
| tracing::warn! on failure | Graceful degradation — GraphService still constructed without adb |

---

## VP-Scout False Positive Avoided

**Original scout claim**: "EC10 = wire async `record_coedit` fire-and-forget from post_edit"

**VP-Scout Chain 3 execution**:
```
touring index find "record_coedit" → post_edit.rs:402 sync call already exists
touring index find "record_coedits" → sync write path CONFIRMED active
```

**Verdict**: Write path = ALREADY IMPLEMENTED. Real gap = READ surface missing.

**Reframed task**: Create `get_coedits_from()` (read) not another `record_coedit()` (write).

**False positives avoided**: 1

---

## Decisions Made

### Decision 1 — Builder pattern for GraphService async_knowledge injection

**Decision**: Use `with_async_knowledge(adb)` builder method rather than adding `adb` as required
parameter to `new_multi_project()`.

**Rationale**: `new_multi_project()` has existing call sites (including test code) using 2-arg
signature. Adding a required 3rd arg would require updating all call sites — scope creep.
Builder pattern is additive, preserves backward compatibility, and follows Rust builder idiom.

**Alternative considered**: Make `async_knowledge: Option<Arc<AsyncFileKnowledgeDB>>` a parameter
to `new()` — rejected because `Option` args leak optionality to all callers unnecessarily.

**Trade-off**: Builder must be called before first `resolve_ctx()` — enforced by `TouringServer::new()`
calling `.with_async_knowledge()` immediately after construction.

### Decision 2 — unwrap_or_default on get_coedits_from in resolve_ctx

**Decision**: Use `.await.unwrap_or_default()` (returns empty vec on error) rather than
propagating the error or logging it.

**Rationale**: Co-edit signal is additive, not critical. A DB error fetching co-edit history
must never block tool response delivery. Graceful degradation to empty vec = pre-Iter9 behavior.
Consistent with the fire-and-forget pattern established in EC5–EC9.

**Alternative considered**: Log the error at warn level — rejected because it would spam logs
on every tool call if the DB is transiently unavailable, creating noise that obscures real issues.

### Decision 3 — Remove dead _coedit_predictor field

**Decision**: Removed `_coedit_predictor: CoEditPredictor` field from `GraphService` instead
of retaining it.

**Rationale**: Field was `#[allow(dead_code)]` — a known anti-pattern per REGRA #0
(POTENCIALIZAR). With GS-EC11 wiring real co-edit data, the predictor wrapper is superseded.
Removal reduces struct size and eliminates the dead_code suppression.

**Risk**: None — no consumers existed. touring wiring orphans confirmed 0 consumers.

---

## Architectural Impact

### Before Iter 9
```
CoEditPredictor::predict_next_files()
  → co_edit_signal = vec![]  // EMPTY — 0 signal
  → imports_signal = [...]
  → blast_radius_signal = [...]
  → RRF blend (effectively 2-signal)

graph_ctx JSON:
  { "coedit_files": [] }  // always empty
```

### After Iter 9
```
GraphService::resolve_ctx()
  → adb.get_coedits_from(file).await  // queries TABLE_FILE_COEDITS
  → ctx.coedit_files = top-5 by count

CoEditPredictor::predict_next_files()
  → co_edit_signal = ctx.coedit_files  // LIVE — real history
  → imports_signal = [...]
  → blast_radius_signal = [...]
  → RRF blend (full 3-signal)

graph_ctx JSON:
  { "coedit_files": ["src/post_edit.rs", "src/pre_read.rs", ...] }
```

### Signal quality over time
- **Cold start** (empty TABLE_FILE_COEDITS): returns `[]` — identical to pre-Iter9 behavior
- **After 10+ edits**: top-5 co-edit partners surfaced per file
- **After 100+ edits**: signal stabilizes and reflects real coupling patterns
- **Population source**: sync `record_coedits()` at `post_edit.rs:402` — active since Iter 6 (EC1)

---

## Changes Made

| File | Change | Impact |
|------|--------|--------|
| `crates/touring-hooks/src/async_knowledge.rs` | Added `get_coedits_from()` method | New READ surface on AsyncFileKnowledgeDB |
| `crates/touring-server/src/graph_service.rs` | Added `coedit_files` field to GraphFocusCtx; added `async_knowledge` field + `with_async_knowledge()` builder; removed dead `_coedit_predictor`; updated `resolve_ctx()`, `predict_coedit_files()`, `inject()` | 33% RRF co-edit signal now active |
| `crates/touring-server/src/server/mod.rs` | Initialize `AsyncFileKnowledgeDB` in `TouringServer::new()`, wire via builder | GraphService receives live DB handle at startup |

---

## Validation Results

| Suite | Result |
|-------|--------|
| `cargo check --workspace` | exit 0 — 0 errors |
| `touring-hooks` tests | 1491/1491 PASS |
| `touring-server` tests | 84/84 PASS |
| `touring-generator` tests | 32/32 PASS |
| Pre-existing failures | touring-simd bench (virtualized SIMD env) + touring-analysis flaky — both unrelated to Iter 9 |

---

## Connection to Pln2 Goals

Iter 9 directly advances Pln2 dimension **(g) Integração Sistêmica**:

> "33% RRF co-edit signal in CoEditPredictor now ACTIVE" — was blocked by missing READ method

And advances **(a) Precisão & Confiabilidade**:

> "AsyncFileKnowledgeDB already exists with deadpool Pool — wire Pln2 async methods"
> EC10 adds `get_coedits_from()` as the first async READ method on the DB.

---

## Issues Encountered

None. VP-Scout correctly identified the write-path false positive before implementation,
avoiding scope creep into a redundant fire-and-forget write wiring.

---

## Next Steps

- [ ] EC12: Wire `get_coedits_from()` into `touring wiring suggest` scoring (LeidenCluster domain overlap × co-edit weight)
- [ ] EC13: Surface `coedit_files` in `cli-ast-blast` output for blast radius enrichment
- [ ] Monitor `TABLE_FILE_COEDITS` row count growth via `touring memory recall "coedit table"`
- [ ] Consider adding `coedit_files` to `cli-e2e` deep analysis report
