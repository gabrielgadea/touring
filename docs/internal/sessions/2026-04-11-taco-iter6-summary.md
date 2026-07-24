# TACO Iteration 6 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: 1452+ passing
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 6 delivered 5 engineering changes (EC0 through EC4) focused on performance
optimization, content-identity gating, and parser cache modernization within the
`touring-hooks` and `touring-generator` crates.

---

## EC0 — consumer_generator.tera Registered in Template Engine

**Files changed**:
- `crates/touring-generator/src/template/engine.rs` — templates count 28 → 29, added `consumer_generator.tera` to both `add_raw_templates()` and `template_names()`
- `crates/touring-generator/tests/e2e_pipeline.rs` — test assertions updated to 29, `ConsumerGenerator` added to `all_kinds()`
- `crates/touring-generator/templates/consumer_generator.tera` — added `| default(value=...)` filters to all 5 interpolated variables

**Auditor fix**: Template required `| default(value=...)` on all 5 variables (`symbol_name`, `module_path`, `trait_name`, `method_name`, `doc_comment`) to pass `template_engine_renders_all_29_kinds_with_empty_vars`. This was caught and fixed before the test suite ran.

---

## EC1 — BLAKE3 Early-Exit in post_edit phase1_tracking()

**File changed**: `crates/touring-hooks/src/post_edit.rs`

**What**: In `phase1_tracking()`, before calling `reindex_file`, compute BLAKE3 hash
of new content and compare with `knowledge.get_blake3_hash(rel_path)`. If hashes match,
skip `reindex_file` entirely.

**Performance**: ~15-30ms saved per matching file — eliminates a full tree-sitter parse
and SQLite symbol upsert for unchanged files.

**Design decision**: Scoped to `phase1_tracking()` only. `phase2_quality()` operates on
already-indexed symbols with a different content access pattern; early-exit there yields
no measurable benefit.

**Implementation pattern**: Closure returning `Option<bool>`, `unwrap_or(false)` for safe
fallback when hash is unavailable. This is the first true content-identity gate — prior
mtime dedup was informational only.

---

## EC1b + EC2 — BLAKE3 Early-Exit + FileParserCache in post_write

**File changed**: `crates/touring-hooks/src/post_write.rs`

**EC1b**: `input_content` moved before reindex block. BLAKE3 early-exit uses in-payload
content (no disk read required — content already available from hook input).

**EC2**: `POST_WRITE_PARSER_CACHE` as `OnceLock` added. `FileParserCache` warm-up
executed in the else branch (when BLAKE3 hash differs, i.e., file content changed).

---

## EC3 — TokenBudget Wired in pre_read

**File changed**: `crates/touring-hooks/src/pre_read.rs`

**What**: Imported `TokenBudget`, created `TokenBudget::pre_read()` with `max_tokens=2000`.
Layer 1 context injection consumes from budget. Layer 2 (deeper analysis) is gated by
`token_budget.has_remaining()`.

**Purpose**: Implements the explicit degradation waterfall specified in Pln2 dimension (c):
pre_read token budget prevents unbounded context injection for large files.

---

## EC4 — FileParserCache Rewritten with moka TTL

**File changed**: `crates/touring-hooks/src/shared/parser_cache.rs`

**What**: Complete rewrite replacing `DashMap` with `moka::sync::Cache`:
- `MAX_CAPACITY = 1000` entries (prevents unbounded growth)
- `TIME_TO_IDLE = 300s` (entries evicted after 5 minutes of inactivity)
- `get_with()` for atomic get-or-insert (eliminates TOCTOU race from prior DashMap pattern)
- `run_pending_tasks()` method added for test determinism
- 4 unit tests passing

**Design decision**: `TIME_TO_IDLE` (not `TIME_TO_LIVE`) — eviction timer resets on access,
appropriate for a parser cache where hot files should remain cached as long as they're
being edited.

**Testing gotcha**: `moka::sync::Cache::entry_count()` is eventually consistent. Tests must
call `cache.run_pending_tasks()` before asserting entry counts, otherwise the count may
lag behind actual insertions.

---

## Quality Gates

| Gate | Result |
|------|--------|
| Functional | PASS — cargo check exit 0, 0 errors |
| Robust | PASS — fallbacks on all hash comparisons, OnceLock initialization |
| Readable | PASS — consistent naming, documented design decisions |
| Documented | PASS — PLAN updated, changelog created, memory entries stored |
| Secure | PASS — no secrets, no shell=true, no unsafe |
| No Regression | PASS — 1452+ tests passing |

---

## Memory Entries Stored

| Key | Type | Content Summary |
|-----|------|-----------------|
| `lesson:blake3_early_exit_pattern` | lesson | Closure pattern, fallback strategy, scope rationale |
| `lesson:moka_entry_count_eventual` | lesson | run_pending_tasks() before assertions, TIME_TO_IDLE semantics |
| `lesson:tera_template_default_filter` | lesson | All 5 variables need default() in consumer_generator.tera |
| `pattern:blake3_early_exit_scope_decision` | pattern | Apply content-identity gates at I/O boundary, not everywhere |

---

## Relationship to Pln2

| Pln2 Task | EC | Status |
|-----------|-----|--------|
| B-parser-cache (FileParserCache via moka) | EC4 | DONE |
| BLAKE3 early-exit post_edit | EC1 | DONE |
| BLAKE3 early-exit post_write (in-payload) | EC1b | DONE |
| FileParserCache warm-up post_write | EC2 | DONE |
| TokenBudget pre_read degradation waterfall | EC3 | DONE |
| consumer_generator.tera template registration | EC0 | DONE |

---

*Generated by touring-scriber FASE 7 | TACO Iteration 6 | 2026-04-11*
