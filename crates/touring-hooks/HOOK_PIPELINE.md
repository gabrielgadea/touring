# Hook Pipeline — Read → Edit/Write/Bash Dynamics

> **Version**: 1.1 | **Status**: Active | **Last Updated**: 2026-03-30
>
> **Key Invariant**: Claude Code **always reads before editing or writing**.
> This creates a natural pipeline where `post_read` populates the `FileKnowledgeDB`
> that subsequent hooks (`pre_edit`, `pre_write`, `pre_bash`) query.

---

## Pipeline Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          CLAUDE CODE LIFECYCLE                               │
│                                                                              │
│   [Read Tool] ────────────────→ [Edit/Write/Bash Tools]                     │
│        ↓                                    ↓                               │
│   pre_read ──→ post_read              pre_edit                               │
│              → FileKnowledgeDB       pre_edit_prevention                     │
│              (symbols, imports,       pre_write                              │
│               dependents, gotchas,    pre_bash                               │
│               notes, risk)                                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Read (pre_read → post_read)

| Hook | Role | Key Action |
|------|------|------------|
| `pre_read` | **Aferente** | Injects context *before* file is read: gotchas, dependency signals, symbol map |
| `post_read` | **Eferente** | Populates `FileKnowledgeDB` with: symbols, imports, language, content hash, wiring |

### Phase 2: Edit/Write/Bash (query FileKnowledgeDB)

| Hook | Queries | Injects |
|------|---------|---------|
| `pre_edit` | `FileKnowledgeDB`, `SymbolStore`, `ErrorPredictor` | Rich context: blast radius, call graph, feature gates, trait impls, quality evolution |
| `pre_edit_prevention` | `Gotcha`, `ErrorPattern` | Proactive warnings: syntax, complexity, scope shadowing |
| `pre_write` | `FileKnowledgeDB`, `LintFailure`, `WiringMap` | Speculative validation, antipatterns, wiring orphan signals |
| `pre_bash` | `BashFailure` | Command-specific failure recall |

---

## Data Flow — FileKnowledgeDB

`post_read.rs:run_inner()` populates the knowledge base:

```rust
// 1. Upsert file knowledge (symbols + imports)
runtime.ctx.knowledge.upsert(&knowledge);

// 2. Build import-based relations
relations: Vec<FileRelation> = imports_for_relations
    .iter()
    .filter_map(|imp| resolve_import_path(imp, &language))
    .map(|target| FileRelation { source, target, relation_type: "imports" })
    .collect();
runtime.ctx.knowledge.replace_relations_from(&rel_path, &relations);

// 3. Populate wiring_map (pub symbols + consumer tracking)
populate_wiring_map(&runtime.ctx.knowledge, &rel_path, &knowledge, &rich_imports);

// 4. Register module in ecosystem
crate::ecosystem::register_module(&runtime.ctx.knowledge, &rel_path, pub_count, import_count, 0);

// 5. Record access for recency scoring
runtime.ctx.knowledge.record_access(&rel_path, session_id);
```

Subsequent hooks (`pre_edit`, `pre_write`) query this via `runtime.ctx.knowledge`.

---

## Hook → HookRuntime Access Pattern

| Hook | `runtime.ctx.knowledge` | `runtime.infra.symbol_index` | `runtime.ctx.error_predictor` |
|------|------------------------|------------------------------|-------------------------------|
| `pre_read` | read | read | — |
| `post_read` | **write** | — | — |
| `pre_edit` | read | read | read |
| `pre_edit_prevention` | read | — | — |
| `pre_write` | read | — | — |
| `pre_bash` | read | — | — |

---

## CILA Budget by Phase

| Phase | Hooks | L0-L1 Budget | L2-L3 Budget | L4+ Budget |
|-------|-------|-------------|-------------|-----------|
| Read | `pre_read` | 800 chars | 2000 chars | 4000 chars |
| Read | `post_read` | — (no context injection) | — | — |
| Edit | `pre_edit` | 1200 chars | 3000 chars | 6000 chars |
| Edit | `pre_write` | 1200 chars | 3000 chars | 6000 chars |
| Bash | `pre_bash` | 800 chars | 2000 chars | 4000 chars |

---

## Silence-is-Default Invariant

All hooks follow: **if no high-signal content exists, emit `HookResponse::Allow` with empty context**.

- `pre_read`: Silence if file unknown, no gotchas, no dependents
- `pre_edit`: Silence if no DB signals, no quality issues, no wiring orphans
- `pre_edit_prevention`: Silence if no high-confidence gotchas or error patterns
- `pre_write`: Silence if content passes all speculative checks
- `pre_bash`: Silence if no command-specific failure history

---

## Wiring Intelligence Integration

`post_read.rs:populate_wiring_map()` registers:

1. **Pub symbols** → `wiring_map` table (who exports)
2. **Consumer entries** → tracking which files import from this module
3. **Module ecosystem** → pub_count + import_count for integration scoring

Subsequent `pre_edit` queries `wiring_signal()` to detect orphan pub symbols
that have no consumers — suggesting they should be private or are dead code.

---

## Error Prediction Pipeline

```
post_read → populate FileKnowledgeDB
pre_edit  → queries ErrorPredictor (trained from post_bash failures)
           → prediction_signal() injects "this pattern historically failed"
pre_write → speculative_validation_signals() runs syntax check before Write
```

---

## Dual Cache Architecture (v1.1)

### Cache 1: Precomputed Signals (HookResultCache)

File-level signals precomputed in `post_read` and cached in `HookResultCache`:

- **Key**: `"__precomputed:{rel_path}"`
- **Storage**: `HookResultCache` (moka TinyLFU, 256 entries)
- **Computed** (v1.2): wiring_signal, ecosystem_signals, gotcha_signals,
  dependents_signal, notes_signal, risk_signal, blast_radius_signal,
  similar_symbol_signal, feature_gate_signal
- **Consumed by**: `pre_edit`, `pre_write`

See `src/precomputed_signals.rs` and `HookRuntime::precompute_signals_for_file()`.

### Cache 2: ANN Semantic Memory (PersistedAnnMemoryRecall)

Cross-session similarity search via ANN on embeddings:

- **Storage**: `PersistedAnnMemoryRecall` → SQLite-backed (`ann_memory.db`)
- **Initialized**: `HookRuntime::init_ann_memory()` via `session_start` hook
- **Query**: `runtime.ctx.ann_recall.search(embedding, k)`
- **Use case**: "Similar files had similar errors"

### Integration

```
SessionStart hook:
  → HookRuntime::init_ann_memory()
  → Loads/creates .claude/data/ann_memory.db
  → PersistedAnnMemoryRecall available for all subsequent hooks

post_read (v1.2):
  → Populates FileKnowledgeDB
  → Precomputes signals → HookResultCache under "__precomputed:{rel_path}"
  → Calls runtime.precompute_signals_for_file() with all static signals

pre_edit:
  → Check __precomputed:{rel_path} cache (O(1)) — fast path
  → Falls back to live DB/index queries if cache miss
  → Appends edit-specific signals (prediction, prevention)
```

### Invalidação

Both caches are invalidated by `file-changed` lifecycle hook (same as `HookResultCache`).

---

## File: `src/post_read.rs`

Key functions:
- `run_inner()` — main logic, populates knowledge
- `populate_wiring_map()` — wiring intelligence registration
- `build_knowledge_ast()` — AST-based extraction (precise)
- `build_knowledge_regex()` — regex fallback (fast, for unsupported languages)
- `detect_language()` — maps extension to language string
- `extract_imports_fast()` — lightweight import extraction
- `resolve_import_path()` — resolves relative imports to module paths

## File: `src/pre_edit.rs`

Key functions:
- `run_returning()` — main entry, composes rich context
- `compose_edit_context()` — combines all DB signals
- `collect_db_signals()` — gathers signals from FileKnowledgeDB
- `build_callgraph_signal()` — external callers via SymbolIndex
- `feature_gate_signal()` — detects `#[cfg(feature = "...")]` patterns
- `trait_impl_signal()` — detects trait implementation blocks

## File: `src/pre_edit_prevention.rs`

Key functions:
- `collect_gotcha_warnings()` — high-confidence gotcha patterns
- `collect_module_existence_issues()` — cross-session error patterns
- `extract_pub_fn_signature()` — extracts function signatures for warnings

## File: `src/pre_write.rs`

Key functions:
- `speculative_validation_signals()` — syntax/import completeness checks
- `quality_baseline_signals()` — antipattern detection
- `wiring_orphan_signal()` — warns if writing to orphan module
- `bench_required_features_signal()` — warns if benchmark missing feature flags

## File: `src/pre_bash.rs`

Key functions:
- `collect_file_scoped_failure()` — same-file failure recall (highest signal)
- `collect_dir_scoped_failure()` — same-directory failure recall
- `compose_relevant_context()` — tiered relevance composition

---

## Derived Constants

| Constant | Value | Location |
|----------|-------|----------|
| `MIN_GOTCHA_HITS` | 2 | `pre_edit_prevention.rs:31` |
| `MAX_GOTCHAS_IN_WARNING` | 3 | `pre_edit_prevention.rs:34` |
| `MAX_ERRORS_IN_WARNING` | 3 | `pre_edit_prevention.rs:37` |
| `DECAY_HALF_LIFE_DAYS` | 7.0 | `pre_edit_prevention.rs:40` |
| `COMPLEXITY_THRESHOLD` | 10 | `pre_edit_prevention.rs:43` |
| `MAX_SHADOW_WARNINGS` | 3 | `pre_edit_prevention.rs:46` |
| `DEFAULT_CONTEXT_BUDGET` | 3200 | `pre_read.rs:267` |
| `MIN_SYMBOL_DEFS` | 3 | `pre_read.rs:271` |
| `MAX_SYMBOL_MAP_ENTRIES` | 50 | `pre_read.rs:274` |

---

## Test Coverage

Each hook module has comprehensive tests:

- `pre_read.rs:tests` — ~54 tests covering signal injection, budget enforcement
- `post_read.rs:tests` — ~15 tests covering AST vs regex parity, wiring registration
- `pre_edit.rs:tests` — ~50 tests covering signal composition, CILA budget
- `pre_write.rs:tests` — ~40 tests covering antipatterns, speculative validation
- `pre_bash.rs:tests` — ~15 tests covering silence-default invariant

---

## Related Documentation

- `HOOK-ARCHITECTURE.md` — complete hook architecture, data flows, CILA budgets
- `touring-hooks-architecture.md` — quick reference (1-page)
- `crates/touring-hooks/src/shared/` — shared infrastructure (signals, cila, quality)
