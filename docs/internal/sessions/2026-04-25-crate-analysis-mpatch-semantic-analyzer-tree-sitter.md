# Crate Analysis: mpatch, semantic-analyzer, tree-sitter → Touring Integration

**Date**: 2026-04-25
**Source**: https://crates.io — API v1 for mpatch, semantic-analyzer, tree-sitter

## Executive Summary

| Crate | Priority | Approach | Risk | Action |
|-------|----------|----------|------|--------|
| **mpatch** | HIGH | Incremental | MEDIUM | Add as optional dep, integrate in touring-generator |
| **semantic-analyzer** | LOW-MEDIUM | Spike first | HIGH | Create isolated crate to test tree-sitter AST adapter |
| **tree-sitter** | HIGH | Full | LOW | Enable WASM feature + benchmark incremental parsing |

---

## 1. mpatch (v1.4.1) — Fuzzy Patch Application

**Source**: https://github.com/romelium/mpatch
**Downloads**: 165K total, 72K recent
**License**: MIT | **LOC**: 2,602 | **Edition**: 2021

### Purpose
Smart, context-aware patch tool that applies diffs using fuzzy matching. Designed for AI-generated code where line numbers may be hallucinated.

### Key Features
- **parallel feature** (rayon for data-parallel fuzzy scan)
- 3-step fallback: exact match → whitespace-insensitive → fuzzy match
- Format agnostic: markdown code blocks, unified diffs, conflict markers
- Smart indentation handling, auto-deletion of stale content
- Dry-run mode, path security (../../ traversal prevention)

### Architecture Fit

| Touring Component | Integration Point | Role |
|---|---|---|
| `touring-generator` | `plan_commit` pipeline | Apply RL-generated patches via fuzzy matching |
| `touring-hooks/pre_write` | pre-write validation | Dry-run patch preview before disk write |
| `touring-hooks/health_delta` | regression detection | patch_complexity_delta signal beyond syntactic quality |

### Net-New Capability
**Pre-write fuzzy patch preview**: Claude sees exact changes BEFORE committing to disk. Currently pre_write validates but doesn't preview deltas.

### Enhancement Vector
Upgrades RL/suggestion patch application from brittle line-number diffs to fuzzy context-aware patching. Handles LLM hallucinated line numbers gracefully.

### Rayon Fit
EXCELLENT. touring-hooks already uses rayon in pre_edit_prevention.rs:375, post_edit.rs:1352, post_write.rs:587. mpatch parallel feature aligns with existing pattern.

### Risk Assessment
- **NEW dependency risk**: LOW (MIT, stable, well-scoped, 16 versions)
- **Performance**: MEDIUM — O(N×M) fuzzy scan on large files needs benchmarking
- **Path security alignment**: mpatch prevents ../../ traversal; touring-hooks has PII/path validation — good alignment

### Integration Phases
1. Add `mpatch` as optional dep in `touring-hooks/Cargo.toml`, feature `mpatch-fuzzy`
2. Integrate in touring-generator `plan-commit` pipeline (most isolated point)
3. Add dry-run preview in `pre_write.rs` hook (opt-in via config)
4. health_delta enhancement with `patch_complexity_delta` signal

---

## 2. semantic-analyzer (v0.4.7) — Compiler Semantic Analysis

**Source**: https://github.com/mrLSD/semantic-analyzer-rs
**Downloads**: 260K total, 32K recent
**License**: MIT | **LOC**: 3,657 | **Edition**: Rust 2024 (1.88.0+ required)

### Purpose
Semantic analyzer library for compilers: symbol resolution, type checking, scope management, flow control validation.

### Key Features
- **codec feature** (serde support)
- Symbol table building
- Type checking (concrete types, not just trait bounds)
- Flow control semantic validation
- Scope rules and name binding

### Architecture Fit

| Touring Component | Integration Point | Role |
|---|---|---|
| `touring-ast` | `rust_semantic.rs` | Complement syn with actual type resolution |
| `touring-analysis` | `quality.rs` | Type complexity metric for cross-language analysis |

### BLOCKER: Rust Version Conflict
- touring workspace minimum: Rust 1.75
- semantic-analyzer requires: Rust 2024 edition (1.88.0+)
- Cannot add as workspace dependency without bumping minimum across all crates

### Existing vs New
- **RustSemanticReport** (syn): generics, lifetimes, trait bounds, where clauses, derives, unsafe/async counts — SYNTACTIC
- **semantic-analyzer**: actual type resolution (concrete types) — SEMANTIC
- They COMPOSE: syn for parsing → semantic-analyzer for semantic analysis on top

### Net-New Capability
1. **Type resolution** for Rust beyond trait bounds (actual concrete types)
2. **Cross-language** semantic analysis via tree-sitter AST input (Python/JS)
3. **API change detection**: Function Foo(T) → Foo(T, U) is semantic change not just syntactic
4. **codec feature**: serialize semantic state for persistence

### Risk Assessment
- **HIGH** — Rust 1.88+ requirement blocks direct workspace integration
- Semantic-analyzer designed for compiler frontends owning AST input — not tree-sitter AST
- Adapter layer between tree-sitter AST → semantic-analyzer input is non-trivial
- Overlaps significantly with syn scope (generic params, trait bounds)

### Integration Phases
1. **SPIKE FIRST**: Create `touring-semantic-analysis` isolated crate
2. Test tree-sitter AST → semantic-analyzer adapter architecture
3. If spike succeeds: wire type resolution into touring-analysis quality signals
4. If spike fails: abandon, no impact on existing code

---

## 3. tree-sitter (v0.26.8) — ALREADY INTEGRATED

**Source**: https://github.com/tree-sitter/tree-sitter
**Downloads**: 18M total, 6.4M recent — MASSIVE mainstream adoption
**License**: MIT | **LOC**: 4,265 Rust + 12,108 C

### Status: ALREADY INTEGRATED as workspace dependency

Current usage in touring:
- `touring-ast/src/languages.rs`: Lang detection
- `touring-ast/src/quality.rs`: compute_complexity_for_source
- `touring-ast/src/surgery.rs`: byte-exact AST surgical edits via tree_sitter::Parser
- `touring-hooks/src/health_delta.rs`: tree-sitter for non-Rust quality
- `crates/touring-ast/src/parser.rs`: thread-local parser pool

### Gap Analysis
1. **WASM feature not enabled** — tree-sitter has `wasm` feature opt-in
2. **Incremental parsing not used** — pre_write/pre_edit re-parse entire files from scratch
3. **Query API not exposed** — tree-sitter pattern matching (.scm files) not available to consumers

### Net-New Capability

#### WASM Feature (aligns with THSF Fase 4)
- tree-sitter WASM enables browser/live parsing as 4th transport option
- touring-wasm (THSF Fase 4) already uses WASM — natural alignment
- No native behavior change, just adds wasm32 target

#### Incremental Parsing Optimization
- Current: O(full_file) re-parse in pre_write/pre_edit hooks
- Opportunity: O(changed_range) re-parse via `tree.edit(&InputEdit) + parse(source, Some(&old_tree))`
- Benchmark needed: largest file in workspace, measure current vs incremental

#### Query API Exposure
- touring-ast already uses tree-sitter internally but not the Query API
- Query pattern matching (.scm files) enables structural grep beyond regex
- touring ast grep could be enhanced with tree-sitter Query for ast-grep-style search

### Risk Assessment
- **LOW** — no new dependency, WASM is opt-in feature
- Incremental parsing needs benchmarking before committing to hook integration
- Query API exposure would be new public API surface requiring stability

### Integration Phases
1. Enable `wasm` feature in `touring-ast/Cargo.toml` for tree-sitter
2. Benchmark incremental parsing in pre_write hook for large files (>30% speedup threshold)
3. Expose tree_sitter::Query as `touring_ast::tree_sitter` module if benchmark validates
4. THSF integration: tree-sitter WASM as 4th transport option

---

## Cross-Crate Synergies

### mpatch + tree-sitter (without semantic-analyzer)
**Pipeline**: mpatch applies fuzzy AI-generated patches → tree-sitter edits incremental AST → pre_write has both fuzzy patch preview AND incremental AST validation
**Benefit**: Handles LLM fuzzy patches without losing incremental parsing optimization

### All 3 crates together
**Complete LLM-assisted code editing pipeline**:
1. mpatch::patch_content_str() computes fuzzy diff
2. tree-sitter edits incremental AST (O(changed_range))
3. semantic-analyzer analyzes changed symbols/types
4. health_delta emits semantic_regression_delta signal

---

## VP-Scout Verification

| Check | mpatch | semantic-analyzer | tree-sitter |
|-------|--------|-------------------|-------------|
| Already integrated? | NO → valid opportunity | NO → valid opportunity | YES → NOT an opportunity |
| Homonimia | No conflict in touring | touring-learning/src/lib.rs 'semantic' is different subsystem | Already correctly identified |
| Feature trace | N/A | N/A | WASM feature opt-in, not enabled in any consumer |
| Compilation evidence | N/A (not in workspace) | Rust 1.88+ required vs workspace 1.75 | ALREADY compiles |

---

## touring CLI Verification

```
touring index find mpatch          → count=0 (NOT in workspace)
touring index find semantic_analyzer → count=0 (NOT in workspace)
touring index find tree_sitter      → count=0 (internal use not indexed)
touring wiring impact RustSemanticReport --depth 1
  → Direct consumers: 5, Max depth: 1 (syn-based semantic already wired)
```