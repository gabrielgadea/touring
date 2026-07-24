# touring-hooks Architecture — Quick Reference

> v3.0 | Touring v30.3.0 | 391+ tests (touring-hooks) | Full doc: `crates/touring-hooks/HOOK-ARCHITECTURE.md`

## Hook → Tool Mapping

| Hook | Claude Tool | Paradigm | Output |
|------|------------|----------|--------|
| `pre_read` | Read (PreToolUse) | Aferente | additionalContext |
| `pre_edit` | Edit (PreToolUse, 1st) | Aferente | additionalContext |
| `pre_edit_prevention` | Edit (PreToolUse, 2nd) | Aferente | additionalContext |
| `pre_write` | Write (PreToolUse) | Aferente | additionalContext |
| `pre_bash` | Bash (PreToolUse) | Aferente | additionalContext |
| `post_edit` | Edit (PostToolUse) | Eferente | additionalContext + DB |
| `post_write` | Write (PostToolUse) | Eferente | additionalContext + DB |
| `post_bash` | Bash (PostToolUse) | Eferente | DB only |
| `post_tool_failure` | Any (PostToolUseFailure) | Eferente | additionalContext + DB |
| `post_compact` | PostCompact | Eferente | DB (cache re-warm) |
| `instructions_loaded` | InstructionsLoaded | Aferente | additionalContext |

## What Each Hook Does (one line each)

- **pre_read**: BM25-ranked gotchas + blast radius + similar symbols + TS/JS exports + scope shadowing + **ANN recall** (path-hash similarity from persistent memory.db)
- **pre_edit**: Scored signal pipeline (1.5=quality_failures … 0.8=wiring) + rayon AST (quality_evolution + file_overview)
- **pre_edit_prevention**: syntax pre-check + complexity threshold + scope shadowing in new_string
- **pre_write**: speculate_v2 + 8-lang antipatterns + import completeness + pub symbol count + wiring prediction
- **pre_bash**: recall past failures for same command+dir, BM25-ranked errors
- **post_edit**: Phase1=track+reindex+ACO+**ANN store** (path-hash embedding via RefCell) | Phase2=verify+feedback (run_returning) | Phase3=RL reward
- **post_write**: reindex + wiring registration + quality feedback
- **post_bash**: record outcome → future pre_bash recall
- **post_tool_failure**: records failure in knowledge graph + auto-creates gotcha + circuit breaker (Halt after 5+ failures on same file)
- **post_compact**: re-warms result cache for top accessed files after context compaction
- **instructions_loaded**: injects project knowledge stats on session init (files tracked, edits, commands, gotchas)

## HookResponse Variants (v30.3.0 — Comprehensive E2E Validated 2026-04-14)

| Variant | Field | Trigger | Effect |
|---------|-------|---------|--------|
| **Allow** | — | Default for non-blocking hooks | Permits tool execution |
| **Context** | `additionalContext` | Default | Injects context string into Claude's view |
| **Deny** | `permissionDecision: "deny"` | `pre_edit_prevention` when speculate score < 0.3 AND syntax failure | Blocks PreToolUse entirely |
| **Block** | `decision: "block"` | `post_edit` when 4+ new antipatterns detected | Blocks PostToolUse continuation |
| **Halt** | `continue: false` | `post_tool_failure` when 5+ failures on same file | Circuit breaker — stops tool loop |
| **ContextWithUpdatedInput** | `updatedInput` | `pre_read` for relative→absolute path normalization | Modifies tool input before execution |

**Validated** (2026-04-14): All 6 variants tested in `potentialization_comprehensive_e2e.rs` Dim 1 + Dim 7.

**Context truncation**: All context output capped at 9,500 chars (UTF-8 safe) to stay under Claude Code's 10K limit.

## Shared Infrastructure

```
shared/signals.rs         → rank_gotchas/symbols/errors_by_relevance() (BM25)
shared/antipatterns.rs    → detect_antipatterns() SIMD memmem, 8 langs
shared/signal_pipeline.rs → StaticSignalLayer, FnSignalLayer, CilaGatedLayer
shared/quality.rs         → is_test_file(), measure_quality_snapshot()
shared/cila.rs            → budget(level): L0-L1=1200, L2-L3=3000, L4+=6000
shared/reindex.rs         → reindex_file() (post_edit, post_write) — PLANKTON PLN2
```

**PLN2 — 11 Extended Knowledge Tables** (reindex.rs:105-165):

| Table | Wired by | Purpose |
|-------|----------|---------|
| `file_feature_flags` | `upsert_feature_flags_batch` | Cargo.toml, pyproject.toml, package.json features |
| `file_todos` | `insert_todo` | TODO/FIXME/XXX from content |
| `edge_confidence` | `upsert_edge_confidence` | Import graph edge confidence |
| `file_communities` | `upsert_file_community` | Louvain community per file |
| `file_test_coverage` | `upsert_test_coverage` | Coverage %, tested/total functions |
| `file_blake3_registry` | `upsert_blake3_registry` | BLAKE3 hash + symbol count |
| `session_file_summary` | `upsert_session_file_summary` | Session-file skeleton summaries |
| `symbol_events_log` | `insert_symbol_event` | Symbol create/modify/delete events |
| `wiring_suggestions` | `upsert_wiring_suggestion` | Orphan symbol wiring recommendations |
| `metadata_benchmark_runs` | `insert_benchmark_run` | Benchmark results (p50/p95/p99) |
| `cognitive_enrichment` | `upsert_cognitive_enrichment` | Complexity, fan-in/out, doc signals |

**Schema constants**: `touring-analysis/src/e2e/schema_guard.rs:57-87`
**E2E tests**: `cargo test -p touring-hooks --test pln2_e2e` (23 tests)

## CILA Budget (pre_edit / pre_write) — Validated 2026-04-14

| Level | Chars |
|-------|-------|
| L0-L1 | 800 |
| L2-L3 | 2.000 |
| L4+   | 4.000 |

Override: `TOURING_CILA_BUDGET_L0` / `_L2` / `_L4` env vars.

> **Validated** (2026-04-14): `complexity_cila_budget_respected` test confirms L0<L2<L4 tiering, L4+ same budget.

## System Invariants

1. **Exit 0 always** — hooks never block Claude Code
2. **Test files skipped** — `is_test_file()` prevents antipattern noise in tests
3. **No silent errors** — all fallible ops use `if let Err(e) { tracing::debug!(...) }`
4. **RL loop** — post_edit quality delta → LinUCB reward → better pre_edit signals
5. **Wiring integrity** — post_write registers all pub symbols → no orphaned exports
6. **Decision variants** — hooks can Deny (block PreToolUse), Block (block PostToolUse), or Halt (circuit breaker) beyond just injecting context
7. **Context cap** — 9,500 char UTF-8 safe truncation on all additionalContext output

## Complete Cycle (edit → verify → learn)

```
pre_read → [Read] → pre_edit → pre_edit_prevention → [Edit]
                                                         ↓
                                                     post_edit
                                                    (track+verify+RL)
                                                         ↓
                                              pre_bash → [Bash] → post_bash
```
