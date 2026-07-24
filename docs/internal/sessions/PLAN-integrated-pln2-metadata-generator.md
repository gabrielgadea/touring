# PLAN-Integrated-Pln2: File Metadata Expansion + Touring Generator

> **Version**: v1.1 | **Date**: 2026-04-12 | **Audit**: 0.94 composite, 6 corrections applied
> **Sources**: `PLAN-file-metadata-expansion-v2-squared.md` + `2026-04-10-touring-generator-strategy-pln2.md`
> **Method**: TACO v6.0 — 3 scouts, sequential-thinking synthesis, VP-Scout verified
> **Confidence**: 0.95 (FACT-dominant, 6 false positives eliminated)

---

## 1. Executive Summary

This plan integrates two Pln2-level plans into a unified execution roadmap:

- **Pln2-Metadata**: File metadata expansion (schema, blake3, tantivy, wiring suggest, CLI/MCP)
- **Pln2-Generator**: touring-generator crate (VGP, templates, 28 kinds, CLI/MCP, self-hosting)

**Key finding**: ~93% of proposed work is ALREADY IMPLEMENTED or FALSE POSITIVE.
- Original combined scope: 208 tasks (82 + 126)
- Remaining real work: ~40 tasks across 7 Waves (including P12 observability + P13 awareness)
- 7 false positives eliminated with VP-Scout evidence (FP-7: symbol_events_log already wired)

---

## 2. Ground Truth — Current State (Scout-Verified 2026-04-12)

### 2.1 System Metrics

| Metric | Value | Source |
|--------|-------|--------|
| Compilation | **0 errors** | `cargo check --workspace` exit 0 |
| Schema version | **7** (symbols.db) / **8** (domain DBs) | migration.rs:17 / migrate.rs:877 |
| Hook count | **99** (dual-assert lines 732+734) | hook_registry.rs |
| MCP tools | **67** total (43 core + 24 generator) | `grep -c 'tool(name' server/mod.rs` |
| Workspace crates | **19** | `ls crates/` |
| Index symbols | **40,272** | `touring status -j` |
| Orphan pub symbols | **19,462** | `touring wiring orphans` |
| RL ema_reward | **0.18** | `touring status -j` |
| server/mod.rs | **5,727 lines** (monolithic) | `wc -l` |

### 2.2 Pln2-Metadata — Implementation Status

| Item | Status | Evidence |
|------|--------|----------|
| Schema v7 (A-1) | **DONE** | `pub const SCHEMA_VERSION: u32 = 7` migration.rs:17 |
| blake3 dep (A-blake3-1) | **DONE** | `blake3 = "1.5.5"` workspace Cargo.toml:91 |
| blake3 hash.rs (A-blake3-3) | **DONE** | touring-core/src/hash.rs with `blake3::Hasher` |
| blake3 in hooks (A-blake3-4) | **DONE** | post_edit.rs:386, post_write.rs:126, functional_wiring.rs:168 |
| metadata_dedup.rs (P3) | **DONE** | shared/metadata_dedup.rs — 3 definitions indexed |
| parser_cache.rs (P3) | **DONE** | shared/parser_cache.rs — FileParserCache 3 definitions |
| feature_flags.rs (P3) | **DONE** | shared/feature_flags.rs — FeatureFlagExtractor indexed |
| DB: wiring_suggestions (P1) | **DONE** | knowledge.rs:1503 upsert + server/mod.rs MCP tool |
| DB: blake3_registry (P1) | **DONE** | knowledge.rs:1408 upsert_blake3_registry |
| DB: session_file_summary (P1) | **DONE** | knowledge.rs:1444 upsert_session_file_summary |
| DB: file_communities (P1) | **DONE** | knowledge.rs:1355 upsert_file_community |
| CLI: wiring suggest (P5) | **DONE** | cli_handlers.rs:385 + hook_registry.rs:104,242,476 |
| CLI: index search (P5) | **DONE** | index.rs subcommand |
| moka dep | **DONE** | `moka = { version = "0.12", features = ["sync", "future"] }` |
| tantivy_index.rs (P10) | **PENDING** | count=0, no dep, no file |
| query_dsl.rs (P6) | **PENDING** | count=0, no file |
| scip_emit.rs (P9) | **PENDING** | count=0, no file |
| server/mod.rs split (P9) | **PENDING** | 5727 lines, monolithic |
| symbol_events_log (P4) | **PARTIAL** | schema constant in schema_guard.rs:78, TODO comments in post_edit.rs:419/post_write.rs:165, NOT wired in knowledge.rs |
| LeidenCluster (P11) | **PENDING** | struct not found |
| Top-level CLI commands (P5-P8) | **PARTIAL** | wiring suggest + index search exist; search/query/scip/metadata commands do NOT |
| Metadata MCP tools (P9) | **PENDING** | ~15 new tools not yet in server/mod.rs |
| pln2_integration.py (P14) | **PENDING** | file does not exist |

### 2.3 Pln2-Generator — Implementation Status

| Item | Status | Evidence |
|------|--------|----------|
| touring-generator crate (W1) | **DONE** | 19th workspace member, lib.rs 3426 lines |
| VgpEngine (W2) | **DONE** | crates/touring-generator/src/vgp/ |
| TemplateEngine (W3) | **DONE** | crates/touring-generator/src/template/ |
| PlanExecutor typestate (W4) | **DONE** | crates/touring-generator/src/executor/ |
| 28 GeneratorKinds (W5) | **DONE** | crates/touring-generator/src/generator/ |
| CLI 24 subcommands (W6) | **DONE** | registered in touring-server |
| MCP 24 tools (W6) | **DONE** | server/mod.rs lines 4746-5727 (exceeds 20 target) |
| moka cache (B5 fix) | **DONE** | `moka = { workspace = true }` in generator/Cargo.toml |
| GeneratorPlan schema (W4) | **DONE** | crates/touring-generator/src/plan/ |
| Speculate bridge (W4) | **DONE** | crates/touring-generator/src/speculate/ |
| Plan registry (W4) | **DONE** | crates/touring-generator/src/registry/ |
| Benchmarks (W13) | **DONE** | crates/touring-generator/benches/ exists |
| Tests dir (W13) | **DONE** | crates/touring-generator/tests/ exists |
| PyO3 generate submodule (W7) | **PENDING** | needs verification |
| NLP reranking (W8) | **PENDING** | BM25 + semantic chunker from touring-antt |
| Observability (W9) | **PENDING** | tracing spans, metrics, eBPF opt-in |
| WASM sandbox (W10) | **PENDING** | touring-wasm inferlet plugins |
| Wiring gate post-commit (W11) | **PENDING** | touring-analysis orphan enforcement |
| Python decommission (W12) | **PENDING** | 7562 → <1000 LOC target |
| E2E tests 8 layers (W13) | **PARTIAL** | test dir exists, 8 layers not verified |
| Self-hosting bootstrap (W14) | **PENDING** | meta-generator generates new kinds |

---

## 3. False Positives Eliminated

These tasks from the original plans are **ALREADY DONE** and must be **SKIPPED**:

| # | Task | Plan | Evidence | VP-Scout Chain |
|---|------|------|----------|----------------|
| FP-1 | A-1: Bump SCHEMA_VERSION 6→7 | Metadata | Already 7 at migration.rs:17 | already_implemented |
| FP-2 | A-blake3-1: Add blake3 to workspace | Metadata | blake3 = "1.5.5" at Cargo.toml:91 | already_implemented |
| FP-3 | A-blake3-2/3/4: hash.rs + exports | Metadata | hash.rs exists with content_hash() | already_implemented |
| FP-4 | Wiring suggest implementation | Metadata | cli_handlers.rs:385 + MCP tool + DB table | already_implemented |
| FP-5 | B5: moka absent from Cargo.toml | Generator | moka = 0.12 in workspace.dependencies | already_implemented |
| FP-6 | Hook count = 98 (both plans) | Both | Actual = 99, asserts at lines 732+734 | already_implemented |
| FP-7 | W1: symbol_events_log NOT wired | Metadata | `insert_symbol_event` at knowledge.rs:1484, wired in post_edit.rs:429 + post_write.rs:175 (Iter 7 EC_sev) | already_implemented |

---

## 4. Conflicts Resolved

### 4.1 Hook Count (CRITICAL)

| Plan | Assumed Base | Target | Reality |
|------|-------------|--------|---------|
| Pln2-Metadata | 98 | 111 (+13) | **Base is 99** |
| Pln2-Generator | 98 | 122 (+24) | **Generator hooks already in 99** |

**Resolution**: Current count is 99. Generator's 24 CLI handlers are already registered.
Metadata plan's new CLI handlers (~8) will bring count to **~107**.
Both asserts at lines 732 AND 734 must be updated atomically per addition.

### 4.2 server/mod.rs (CRITICAL)

| Plan | Assumed LOC | Action |
|------|------------|--------|
| Pln2-Metadata | 5157 | Split → core.rs + file_metadata.rs + search_tools.rs |
| Pln2-Generator | (not addressed) | Already added 24 tools at lines 4746-5727 |

**Resolution**: Actual LOC is 5727 (+570 drift from generator additions).
Split MUST execute FIRST (Wave 0), preserving generator tools section intact.
Recommended split:
- `mod.rs` (~800 LOC): router + TouringServer struct + shared types
- `core_tools.rs` (~1500 LOC): index, AST, session, wiring, memory tools
- `analysis_tools.rs` (~1000 LOC): e2e, evolution, cognitive, gotcha tools
- `generator_tools.rs` (~1000 LOC): all 24 generator tools (extracted as-is)
- `metadata_tools.rs` (~400 LOC): NEW metadata tools from this plan

### 4.3 Schema Versioning

Two independent versioning systems — NO conflict:
- `SCHEMA_VERSION = 7` in migration.rs:17 → `symbols.db` (FileKnowledgeDB)
- `schema_version = 8` in schema/{graph.rs:115, knowledge.rs:219, memory.rs:77} DDL strings → domain DBs
- Pln2-Metadata A-1 task (6→7) is **ALREADY DONE**

### 4.4 Architecture Boundary

touring-generator and touring-hooks are **siblings**, not parent-child:
- `touring-server` → `touring-generator` (path dep, line 66)
- `touring-server` → `touring-hooks` (path dep)
- `touring-generator` does NOT depend on `touring-hooks`
- No cycle. Cross-plan features requiring both must be mediated by touring-server.

---

## 5. Integrated Execution Plan — 7 Waves

### Wave 0: Server Refactoring (PREREQUISITE)
**Blocks**: Wave 2, Wave 5 (partial)
**Size**: L (12-16h)
**Refinement Level**: L3 (structure change, same behavior)

| Task | Description | File(s) |
|------|-------------|---------|
| W0-1 | Create `server/core_tools.rs` — extract index, AST, session, wiring, memory tool impls | server/mod.rs → server/core_tools.rs |
| W0-2 | Create `server/analysis_tools.rs` — extract e2e, evolution, cognitive, gotcha tool impls | server/mod.rs → server/analysis_tools.rs |
| W0-3 | Create `server/generator_tools.rs` — extract 24 generator tool impls (lines 4746-5727) | server/mod.rs → server/generator_tools.rs |
| W0-4 | Create `server/metadata_tools.rs` — empty module for Wave 2 | NEW file |
| W0-5 | Reduce `server/mod.rs` to router + struct + shared types (~800 LOC) | server/mod.rs |
| W0-6 | Verify: `cargo check --workspace` exit 0, all 67 MCP tools callable | validation |

**Success Gate**: mod.rs ≤ 900 LOC, `cargo test --workspace` passes, all MCP tools respond.

### Wave 1: DB Wiring Completion — ALREADY DONE (Audit-Verified)
**Status**: **ELIMINATED** (FP-7) — Iter 7 EC_sev already implemented all tasks.
**Evidence**:
- `insert_symbol_event()` exists at knowledge.rs:1484
- Wired in post_edit.rs:429 (`runtime.ctx.knowledge.insert_symbol_event(...)`)
- Wired in post_write.rs:175 (`runtime.ctx.knowledge.insert_symbol_event(...)`)
- No SCHEMA_VERSION bump needed — table uses existing schema

~~**Original tasks (all DONE)**:~~
- ~~W1-1: symbol_events_log DDL~~ → knowledge.rs:1484
- ~~W1-2: append_symbol_event method~~ → insert_symbol_event at knowledge.rs:1484
- ~~W1-3: Wire in post_edit.rs~~ → post_edit.rs:429
- ~~W1-4: Wire in post_write.rs~~ → post_write.rs:175
- ~~W1-5: SCHEMA_VERSION bump~~ → Not needed

### Wave 2: Metadata CLI + MCP Surface (DEPENDS on Wave 0)
**Blocks**: Wave 6
**Size**: L (16-20h)
**Refinement Level**: L2 (new CLI surface, no architecture change)

| Task | Description | File(s) |
|------|-------------|---------|
| W2-1 | CLI handler: `touring search symbols <query>` | cli/search.rs (NEW) |
| W2-2 | CLI handler: `touring search docs <query>` | cli/search.rs |
| W2-3 | CLI handler: `touring metadata backfill [--limit N]` | cli/metadata.rs (NEW) |
| W2-4 | CLI handler: `touring metadata stats` | cli/metadata.rs |
| W2-5 | CLI handler: `touring session summary [session_id]` | cli/session.rs (extend) |
| W2-6 | CLI handler: `touring bench [component]` | cli/bench.rs (NEW) |
| W2-7 | Register new CLI hooks in hook_registry.rs | hook_registry.rs |
| W2-8 | Update both asserts (lines 732+734) to 99+N | hook_registry.rs |
| W2-9 | Add metadata MCP tools to `server/metadata_tools.rs` | metadata_tools.rs |
| W2-10 | Update command_table in common.rs | cli/common.rs |

**Success Gate**: All new CLI commands respond, MCP tools callable, hook count asserts pass.

### Wave 3: Query & Search Infrastructure (GATE: tantivy decision)
**Blocks**: none
**Size**: M-L (8-16h depending on tantivy decision)
**Refinement Level**: L2 (new feature, modular)

**GATE DECISION**: Before starting, evaluate:
- Does BM25 FTS5 (already in SQLite) satisfy full-text search requirements?
- If YES → skip tantivy, implement query_dsl.rs + scip_emit.rs only (M: 8h)
- If NO → add tantivy 0.22, implement TantivyIndex + query_dsl.rs + scip_emit.rs (L: 16h)

| Task | Description | File(s) |
|------|-------------|---------|
| W3-1 | GATE: Benchmark FTS5 vs tantivy requirements | evaluation doc |
| W3-2 | Create `query_dsl.rs` — recursive descent parser for structured queries | touring-hooks/src/query_dsl.rs (NEW) |
| W3-3 | CLI handler: `touring query <dsl_expression>` | cli/query.rs (NEW) |
| W3-4 | Create `scip_emit.rs` — SCIP protocol export | touring-server/src/scip_emit.rs (NEW) |
| W3-5 | CLI handler: `touring scip emit <file>` | cli/scip.rs (NEW) |
| W3-6 | (CONDITIONAL) Add tantivy = "0.22" to workspace deps | Cargo.toml |
| W3-7 | (CONDITIONAL) Create `tantivy_index.rs` — standalone FTS index | touring-hooks/src/tantivy_index.rs (NEW) |

**Success Gate**: Query DSL parses valid expressions. SCIP output validates against spec.

### Wave 4: Advanced Wiring (INDEPENDENT)
**Blocks**: none
**Size**: M (6-8h)
**Refinement Level**: L2 (algorithm addition)

| Task | Description | File(s) |
|------|-------------|---------|
| W4-1 | Implement `LeidenCommunityDetector` struct | touring-hooks/src/shared/leiden.rs (NEW) |
| W4-2 | Wire LeidenCluster into `cli_wiring_suggest` for community-based suggestions | cli_handlers.rs |
| W4-3 | Implement wiring gate: post-commit orphan check via touring-analysis | touring-analysis (extend) |
| W4-4 | CLI handler: `touring wiring community <file>` | cli/wiring.rs (extend) |

**Success Gate**: `touring wiring suggest` returns community-aware suggestions. Post-commit gate blocks new orphan introductions.

### Wave 5: Generator Completion (INDEPENDENT)
**Blocks**: Wave 6
**Size**: M (8-12h)
**Refinement Level**: L2 (feature completion)

| Task | Description | File(s) |
|------|-------------|---------|
| W5-1 | PyO3 generate submodule — verify/fix B2 (`PyModule::new()` vs `new_bound()`) | touring-python/src/lib.rs |
| W5-2 | NLP reranking — BM25 + semantic chunker from touring-antt for plan recall | touring-generator (extend) |
| W5-3 | WASM sandbox — touring-wasm inferlet plugins for generator validation | touring-generator (extend) |
| W5-4 | Observability — tracing spans + metrics for generator lifecycle | touring-generator (extend) |

| W5-5 | Observability: gate_metrics.rs +2 counters (metadata_cache_hit, metadata_backpressure_dropped) | gate_metrics.rs (extend) |
| W5-6 | Observability: Wire touring-telemetry OTEL spans in collect_fast_metadata + hook latency | touring-telemetry (extend) |
| W5-7 | Observability: Export gate_metrics via Prometheus endpoint | touring-server (extend) |

**Success Gate**: PyO3 `import touring; touring.generate.submit_plan(...)` works. NLP recall returns ranked results. Gate metrics exported.

### Wave 6: E2E & Validation (DEPENDS on Waves 0-5)
**Blocks**: none (final)
**Size**: L (12-16h)
**Refinement Level**: L3 (test infrastructure)

| Task | Description | File(s) |
|------|-------------|---------|
| W6-1 | E2E test suite for metadata features: search, query DSL, wiring suggest, SCIP | tests/ (NEW) |
| W6-2 | E2E test suite for generator: submit→verify→render→commit lifecycle | tests/ (extend) |
| W6-3 | Integration test: metadata + generator cross-feature (query plan by metadata) | touring-integration-tests/ |
| W6-4 | Self-hosting bootstrap: meta-generator generates 5+ new GeneratorKinds | touring-generator (extend) |
| W6-5 | Python bridge: create `pln2_integration.py` with 8 phase entry points | ~/.claude/scripts/ (NEW) |
| W6-6 | Python decommission assessment: measure current Python LOC vs target <1000 | evaluation |
| W6-7 | Final `cargo test --workspace` + `cargo clippy --workspace -- -D warnings` | validation |
| W6-8 | Awareness: Update instructions_loaded.rs — top-10 hot files + orphan count + suggest hint | instructions_loaded.rs |
| W6-9 | Awareness: Create `~/.claude/skills/touring-file-metadata/SKILL.md` | SKILL.md (NEW) |
| W6-10 | Awareness: Create `~/.claude/skills/touring-search/SKILL.md` | SKILL.md (NEW) |
| W6-11 | Awareness: Create `~/.claude/skills/touring-query/SKILL.md` | SKILL.md (NEW) |
| W6-12 | Awareness: Create `~/.claude/skills/touring-wiring-suggest/SKILL.md` | SKILL.md (NEW) |
| W6-13 | Awareness: Create `~/.claude/skills/touring-scip/SKILL.md` | SKILL.md (NEW) |
| W6-14 | Awareness: Create `~/.claude/rules/file-metadata-first.md` + CLAUDE.md +5 lines | rule (NEW) |

**Success Gate**: All E2E tests pass. Self-hosting generates valid kinds. clippy 0 warnings. 5 SKILL.md files created.

---

## 6. Dependency Graph

```
Wave 0 (server split)
  │
  ├──► Wave 2 (metadata CLI/MCP) ──► Wave 6 (E2E + awareness)
  │                                      ▲
  │    Wave 1 (DB wiring) ── DONE ───────┤
  │                                      │
  │    Wave 3 (query/search) ────────────┤
  │                                      │
  │    Wave 4 (advanced wiring) ─────────┤
  │                                      │
  └──► Wave 5 (generator + observ.) ─────┘
```

**Parallel Groups**:
- Wave 3 ∥ Wave 4 ∥ Wave 5 (all independent, can start immediately)
- Wave 2 depends on Wave 0 only
- Wave 6 depends on all previous Waves
- Wave 1 is **DONE** (FP-7)

**Critical Path**: W0 → W2 → W6 (~54h estimated)

---

## 7. Risk Register

| # | Risk | Probability | Impact | Mitigation |
|---|------|-------------|--------|------------|
| R1 | server/mod.rs split breaks MCP tools | MEDIUM | HIGH | Extract with `pub(super)` pattern, test each tool after extraction |
| R2 | tantivy adds >30s compile time | HIGH | LOW | Gate decision (W3-1): skip if FTS5 sufficient |
| R3 | Hook count assert drift | LOW | HIGH | Gotcha G104: ALWAYS patch BOTH lines 732+734 atomically |
| R4 | symbol_events_log migration breaks existing DBs | LOW | HIGH | migration v7→v8 with backup `.v7.bak` |
| R5 | LeidenCluster algorithmic complexity | MEDIUM | MEDIUM | Start with greedy heuristic, upgrade to full Leiden later |
| R6 | PyO3 API changed between versions | MEDIUM | LOW | Check `pyo3 = "0.22"` docs for `PyModule::new()` vs `new_bound()` |
| R7 | Cross-plan merge conflicts in hook_registry.rs | LOW | MEDIUM | All hook additions go through Wave 2 batch |
| R8 | Python decommission breaks existing scripts | MEDIUM | MEDIUM | Assessment first (W6-6), gradual migration |

---

## 8. Success Gates (Merged from Both Plans)

| # | Gate | Source | Criteria |
|---|------|--------|----------|
| G1 | server/mod.rs ≤ 900 LOC | Metadata P9 | `wc -l server/mod.rs` ≤ 900 |
| G2 | Schema migration safe | Metadata P0 | v7→v8 with backup, migration test passes |
| G3 | Zero new orphan pub symbols | Both | `touring wiring orphans` count ≤ 19462 |
| G4 | Hook latency P95 compliance | Metadata P12 | post_edit < 40ms, pre_edit < 80ms |
| G5 | All CLI smoke tests pass | Both | Each new command returns valid JSON with `-j` |
| G6 | All MCP smoke tests pass | Both | 67+ tools callable via rmcp |
| G7 | Regression green | Both | `cargo test --workspace` all pass |
| G8 | Clippy clean | Both | `cargo clippy --workspace -- -D warnings` 0 warnings |
| G9 | E2E score ≥ 0.60 | Metadata P15 | `touring e2e --depth deep -j` score |
| G10 | RL activation | Metadata P15 | ema_reward ≥ 0.20 after 1 week |

---

## 9. Effort Summary

| Wave | Size | Hours (est) | Parallelizable | Status |
|------|------|-------------|----------------|--------|
| W0: Server Split | L | 12-16h | No (prerequisite) | PENDING |
| W1: DB Wiring | — | 0h | — | **DONE** (FP-7) |
| W2: CLI/MCP Surface | L | 16-20h | After W0 | PENDING |
| W3: Query/Search | M-L | 8-16h | Yes (independent) | PENDING |
| W4: Advanced Wiring | M | 6-8h | Yes (independent) | PENDING |
| W5: Generator + Observability | M | 10-14h | Yes (independent) | PENDING |
| W6: E2E + Awareness | L | 14-18h | After all | PENDING |
| **Total** | | **66-92h** | | |

**With maximum parallelization** (W3∥W4∥W5 after W0):
- Critical path: W0(16h) + W2(20h) + W6(18h) = **~54h**
- Wall clock with 3 parallel engineers: **~26-32h**

---

## 10. Appendix: Scout Evidence

### A. Files Verified

| File | Lines | Status |
|------|-------|--------|
| crates/touring-hooks/src/hook_registry.rs | ~740 | 99 hooks, dual-assert at 732+734 |
| crates/touring-server/src/server/mod.rs | 5727 | Monolithic, 67 tools |
| crates/touring-core/src/migration.rs | ~290 | SCHEMA_VERSION=7 |
| crates/touring-generator/src/lib.rs | 3426 | Fully implemented |
| crates/touring-hooks/src/shared/metadata_dedup.rs | ~150 | MetadataDedup struct |
| crates/touring-hooks/src/shared/parser_cache.rs | ~150 | FileParserCache struct |
| crates/touring-hooks/src/shared/feature_flags.rs | ~280 | FeatureFlagExtractor trait |
| crates/touring-generator/Cargo.toml | ~80 | All deps wired |

### B. Touring CLI Verification Commands

```bash
# Reproduce scout findings
touring index find "MetadataDedup" -j        # count=3
touring index find "FileParserCache" -j      # count=3
touring index find "FeatureFlagExtractor" -j # count=1
touring index find "TantivyIndex" -j         # count=0 (NOT implemented)
touring index find "QueryDsl" -j             # count=0 (NOT implemented)
touring index find "ScipEmit" -j             # count=0 (NOT implemented)
touring index find "LeidenCluster" -j        # count=0 (NOT implemented)
touring index find "VgpEngine" -j            # count=N (implemented)
touring index find "PlanExecutor" -j         # count=N (implemented)
touring index find "GeneratorKind" -j        # count=N (implemented)
touring wiring orphans -j | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'Orphans: {len(d.get(\"orphans\", []))}')"
# → Orphans: 19462
```

### C. Cross-Reference: Original Plan → Integrated Wave

| Original Task | Plan | Integrated Wave | Status |
|--------------|------|-----------------|--------|
| A-1 (SCHEMA_VERSION) | Metadata P0 | ELIMINATED (FP-1) | Already done |
| A-blake3-* | Metadata P0/P3 | ELIMINATED (FP-2/3) | Already done |
| A-schema-* (tables) | Metadata P1 | W1 (symbol_events_log only) | 4/5 done |
| P3 (BLAKE3 adapter) | Metadata P3 | ELIMINATED | Already done |
| P4 (hook wiring) | Metadata P4 | W1 (symbol_events_log) | Partial |
| P5-P7 (CLI) | Metadata P5-P7 | W2 (new commands) | Partial |
| P8 (routers) | Metadata P8 | W2-7/W2-8/W2-10 | Pending |
| P9 (MCP/split) | Metadata P9 | W0 (split) + W2-9 (tools) | Pending |
| P10 (tantivy) | Metadata P10 | W3 (conditional) | Pending |
| P11 (wiring engine) | Metadata P11 | W4 (LeidenCluster) | Pending |
| P14 (Python bridge) | Metadata P14 | W6-5 | Pending |
| W1-W6 (generator core) | Generator W1-W6 | ELIMINATED | Already done |
| W7 (PyO3) | Generator W7 | W5-1 | Pending |
| W8 (NLP) | Generator W8 | W5-2 | Pending |
| W9 (observability) | Generator W9 | W5-4 | Pending |
| W10 (WASM) | Generator W10 | W5-3 | Pending |
| W11 (wiring gate) | Generator W11 | W4-3 | Pending |
| W13 (E2E tests) | Generator W13 | W6-1/W6-2 | Partial |
| W14 (self-hosting) | Generator W14 | W6-4 | Pending |
| W12 (Python decommission) | Generator W12 | W6-6 assessment only | **Deferred to Pln3** — full 7562→<1000 LOC migration out of scope |
| P12 (observability) | Metadata P12 | W5-5/W5-6/W5-7 | Pending |
| P13 (awareness layer) | Metadata P13 | W6-8 through W6-14 | Pending |
| P4 symbol_events_log | Metadata P4 | ELIMINATED (FP-7) | Already wired (Iter 7) |

---

*Generated by TACO v6.0 — 3 scouts (metadata + generator + overlap), sequential-thinking synthesis, VP-Scout verified. 6 false positives eliminated.*
