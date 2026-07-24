# Wiring Intelligence System v2 — Design Specification

> **Date**: 27/03/2026
> **Status**: Approved
> **Scope**: touring-core, touring-ast, touring-hooks, touring-cortex, touring-learning
> **Version Target**: Touring v28.0.0
> **Estimated Effort**: ~52h across 4 sprints (H0-H3)

---

## 1. Problem Statement

### The Wiring Gap

When implementing multi-module features, modules are created with `pub` symbols, exported via
`lib.rs`, tested unitarily, but **never connected to the actual pipeline**. This happened with
3 out of 15 strategies in the touring-cognitive Excellence sprint:

- `tfidf.rs` (S13): Created `TfIdfVectorizer` — never wired into `nexus.rs`
- `adaptive_engine.rs` (S14): Created `AdaptiveEngine` — never wired into `bridge.rs`
- `sqlite_graph.rs` (S15): Created `SqliteGraphStore` — never wired into persistence pipeline

The gap went undetected because:
1. Rust/Clippy does **not** warn about unused `pub` items (assumes external consumers)
2. Unit tests pass even for unintegrated modules (they test in isolation)
3. The knowledge graph tracks file-level imports but **loses symbol-level granularity**

### Root Cause

The information pipeline has a specific break point:

```
touring-ast::extract_imports()  -> Vec<ImportInfo> { module_path, symbols: ["A", "B"] }
                    | (information LOST here)
post_read::extract_imports_fast() -> Vec<String> ["module_path"]  (loses symbols!)
                    |
knowledge.upsert() -> file_relations (source, target, "imports")  (no symbols)
```

`DependencyEdge.symbols: Vec<String>` exists in touring-ast but is never populated from hook data.
`FileRelation` in touring-hooks has no `imported_symbols` field at all.

---

## 2. Solution Architecture — 6 Layers

```
+---------------------------------------------------------------------+
|                  WIRING INTELLIGENCE SYSTEM v2                        |
|                                                                      |
|  L0: ECOSYSTEM MAP                                                   |
|  |  session-start -> scan directory tree -> build ModuleEcosystem    |
|  |  -> map: Cargo.toml deps, mod declarations, pub use chains       |
|  |  -> detect: entry points (main.rs, lib.rs, tests/, benches/)     |
|  |  -> build: expected_wiring_graph (who SHOULD import whom)        |
|  +------------------------------------------------------------------+
|                              |                                       |
|  L1: AST-READ ENRICHMENT                                            |
|  |  post-read -> touring-ast::extract_imports() (WITH symbols)      |
|  |  -> populate file_relations WITH imported_symbols (SCHEMA v6)    |
|  |  -> update WiringMap: {pub_symbol -> [consumers]}                |
|  |  -> calculate integration_score per module in real time          |
|  +------------------------------------------------------------------+
|                              |                                       |
|  L2: PRE-EDIT INTELLIGENCE                                          |
|  |  pre-edit Write/Edit -> AST parse of new_content                 |
|  |  -> Signal 6a: Wiring Check (new pub symbols without consumer)   |
|  |  -> Signal 6b: Import Prediction (suggest needed imports)        |
|  |  -> Signal 6c: Ecosystem Fit (suggest possible integrations)     |
|  +------------------------------------------------------------------+
|                              |                                       |
|  L3: POST-EDIT TRACKING                                             |
|  |  post-edit -> diff AST (before vs after pub symbols)             |
|  |  -> new pub? -> register pending WiringContract                  |
|  |  -> new import? -> resolve WiringContract                        |
|  |  -> integration_score updated                                    |
|  +------------------------------------------------------------------+
|                              |                                       |
|  L4: FILE-CHANGED VERIFICATION                                      |
|  |  file-changed -> re-parse AST of modified file                   |
|  |  -> re-verify: valid imports? referenced symbols exist?          |
|  |  -> re-calculate: integration_score of module AND its consumers  |
|  |  -> detect: broken import (symbol removed but consumer remains)  |
|  |  -> cascade: propagate verification to all dependents            |
|  +------------------------------------------------------------------+
|                              |                                       |
|  L5: SESSION AUDIT + SUGGESTIONS                                    |
|  |  session-end / post-compact -> full integration audit            |
|  |  -> orphan report + wiring contract balance                      |
|  |  -> suggest new integrations based on ecosystem map              |
|  |  -> persist gotchas + RL reward                                  |
|  +------------------------------------------------------------------+
|                                                                      |
|  RL: integration_score -> LinUCB -> adaptive thresholds             |
|  -> learns: orphan module patterns, ideal warning timing            |
+---------------------------------------------------------------------+
```

---

## 3. Layer Details

### 3.1 L0 — Ecosystem Map (Session Start)

**Purpose**: Build a complete map of the project's module ecosystem on session start.

**Data Structures**:

```rust
pub struct ModuleEcosystem {
    /// All modules with their role and metadata
    pub modules: HashMap<String, ModuleInfo>,
    /// Entry points (reachability roots)
    pub entry_points: Vec<String>,
    /// External dependencies (from Cargo.toml/pyproject.toml)
    pub external_deps: Vec<ExternalDep>,
    /// Expected wiring: pub symbols that SHOULD be imported somewhere
    pub expected_wiring: Vec<WiringExpectation>,
}

pub struct ModuleInfo {
    pub file_path: String,
    pub module_role: ModuleRole,
    pub pub_symbols: Vec<String>,
    pub imports: Vec<ImportInfo>,
    pub re_exports: Vec<String>,
    pub parent_module: Option<String>,
}

pub enum ModuleRole {
    EntryPoint,   // main.rs, bin/*.rs
    Library,      // lib.rs (crate root)
    Internal,     // any other src/*.rs
    Test,         // tests/*.rs, #[cfg(test)] mod
    Bench,        // benches/*.rs
    BuildScript,  // build.rs
}

pub struct ExternalDep {
    pub name: String,
    pub version: String,
    pub features: Vec<String>,
}

pub struct WiringExpectation {
    pub module_file: String,
    pub symbol_name: String,
    pub expected_consumer: Option<String>,
    pub confidence: f64,
    pub reason: String,
}
```

**SQLite table**:
```sql
CREATE TABLE IF NOT EXISTS module_ecosystem (
    file_path TEXT PRIMARY KEY,
    module_role TEXT NOT NULL,
    parent_module TEXT,
    pub_symbol_count INTEGER DEFAULT 0,
    import_count INTEGER DEFAULT 0,
    re_export_count INTEGER DEFAULT 0,
    integration_score REAL DEFAULT 0.0,
    last_scanned_at TEXT
);
```

**Behavior**:
1. Scan project directory for source files
2. Detect entry points by convention (main.rs, lib.rs, tests/, benches/)
3. Parse Cargo.toml/pyproject.toml for external deps
4. Build module tree from `mod` declarations and directory structure
5. Track `pub use` re-export chains
6. Compute expected wiring based on module roles and naming patterns

**Trigger**: `session-start` hook event.

---

### 3.2 L1 — AST-Read Enrichment (Foundation)

**The Core Fix**: Replace `extract_imports_fast()` (Vec<String>) with
`touring_ast::extract_imports()` (Vec<ImportInfo> with symbols) in post_read.

**Schema Change** (SCHEMA_VERSION 5 -> 6):
```sql
-- Add imported_symbols to file_relations
ALTER TABLE file_relations ADD COLUMN imported_symbols TEXT DEFAULT '[]';

-- New wiring map table
CREATE TABLE IF NOT EXISTS wiring_map (
    module_file TEXT NOT NULL,
    symbol_name TEXT NOT NULL,
    symbol_kind TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'public',
    consumer_file TEXT,
    import_line INTEGER,
    contract_source TEXT DEFAULT 'ast_read',
    resolved_at TEXT,
    PRIMARY KEY(module_file, symbol_name, COALESCE(consumer_file, ''))
);
CREATE INDEX IF NOT EXISTS idx_wiring_orphans
    ON wiring_map(consumer_file) WHERE consumer_file IS NULL;
CREATE INDEX IF NOT EXISTS idx_wiring_module
    ON wiring_map(module_file);
```

**Post-Read Flow**:
```
1. Read file -> touring_ast::extract_imports(content, lang) -> Vec<ImportInfo>
2. For each ImportInfo:
   a. Upsert file_relations WITH imported_symbols JSON
   b. For each imported symbol: upsert wiring_map with consumer_file = this file
3. Extract pub symbols from current file (already done via symbols_json)
4. For each pub symbol: upsert wiring_map with consumer_file = NULL
   (will be filled when someone imports it)
5. Calculate integration_score = symbols_with_consumer / total_pub_symbols
```

---

### 3.3 L2 — Pre-Edit Intelligence (3 New Signals)

Added to `pre_edit.rs::compose_edit_context` as signals 6a, 6b, 6c.

**Signal 6a — Wiring Check**:
When Claude creates new code with `pub` items:
```
AST parse new_content -> find `pub struct Foo`, `pub fn bar()`
-> Query wiring_map: does Foo/bar have a planned consumer?
-> If not: inject warning before edit
```

Example output:
```
wiring: new `pub struct TfIdfVectorizer` in tfidf.rs has no consumers.
  Wire into nexus.rs (replaces bytes().take(384) pseudo-embedding)
  or use `pub(crate)` if internal-only.
```

**Signal 6b — Import Prediction**:
When Claude edits a file, predict which imports will be needed:
```
AST parse new_content -> find type reference `TfIdfVectorizer` not imported
-> Query wiring_map + module_ecosystem: TfIdfVectorizer is in tfidf.rs
-> Suggest: "import needed: `use crate::tfidf::TfIdfVectorizer;`"
```

**Signal 6c — Ecosystem Fit**:
Suggest integrations based on the dependency graph:
```
File being edited: nexus.rs
-> Query module_ecosystem: nexus.rs uses SemanticGraph + SessionPredictor
-> Available but unintegrated modules: TfIdfVectorizer (tfidf.rs), AnnIndex (ann_index.rs)
-> Suggest integration opportunity with confidence score
```

Confidence scoring based on:
- Semantic similarity of type/function names
- Dependency graph proximity (hops)
- Historical patterns (RL: modules with similar roles had successful integration)
- API compatibility (parameter types match consumer needs)

---

### 3.4 L3 — Post-Edit Tracking

After each successful edit:

```
1. Re-parse AST of edited file
2. Diff with previous state (symbols_json from knowledge):
   - New pub symbols? -> Insert into wiring_map (consumer=NULL)
   - Pub symbols removed? -> Mark as resolved in wiring_map
   - New imports? -> Resolve corresponding wiring_map entries
   - Imports removed? -> Re-open wiring_map entries (consumer back to NULL)
3. Recalculate integration_score for the module
4. If score dropped (was 1.0, now 0.8): flag as regression
5. Update module_ecosystem entry
```

---

### 3.5 L4 — FileChanged Verification (Systemic Check)

**Current behavior**: `handle_file_changed` only invalidates `result_cache`.

**Enhanced behavior**:

```
1. Re-parse AST of modified file
2. Extract current imports + pub symbols
3. Compare with knowledge graph state:
   a. BROKEN IMPORT: file imports symbol X from module Y,
      but X was removed from Y
      -> "broken import: main.rs imports TfIdfVectorizer from tfidf.rs
          but it was removed"
   b. LOST CONSUMER: file was consumer of symbol X,
      but import was removed -> re-open wiring_map entry
   c. NEW ORPHAN: pub symbol added but nobody imports yet
4. Recalculate integration_score of module AND ALL its consumers
5. Cascade verification to dependents:
   - Who imports from this file? -> [a.rs, b.rs, c.rs]
   - For each consumer:
     - Do imported symbols still exist?
     - Are interfaces (types, signatures) compatible?
     - Flag if something broke
6. Update dependency_cache with new information
```

---

### 3.6 L5 — Session Audit + Suggestions

On `session-end` or `post-compact`, execute deepest analysis:

**Orphan Report**:
```sql
SELECT module_file, symbol_name, symbol_kind
FROM wiring_map
WHERE consumer_file IS NULL
  AND visibility = 'public'
ORDER BY module_file;
```

**Integration Score Board**:
```
Module              Score    Status
metrics.rs          1.00     Fully wired
reasoning_engine.rs 1.00     Fully wired
tfidf.rs            0.00     ORPHAN (0/1 pub symbols used)
sqlite_graph.rs     0.00     ORPHAN (0/1 pub symbols used)
adaptive_engine.rs  0.50     Partial (1/2 pub symbols used)
```

**Integration Suggestions** (confidence-scored):
```
1. tfidf.rs::TfIdfVectorizer -> nexus.rs
   Reason: nexus.rs uses bytes().take(384) pseudo-embedding
   Confidence: 0.92

2. sqlite_graph.rs::SqliteGraphStore -> persistence.rs
   Reason: persistence.rs uses serde_json for graph storage
   Confidence: 0.88

3. adaptive_engine.rs::AdaptiveEngine -> bridge.rs
   Reason: bridge.rs orchestrates CognitiveRuntime
   Confidence: 0.85
```

**Gotcha Persistence**:
Orphans are saved as gotchas so next session's pre-read injects:
```
3 orphan modules from previous session: tfidf.rs, sqlite_graph.rs
  - tfidf.rs: wire into nexus.rs (replaces pseudo-embedding)
  - sqlite_graph.rs: wire into persistence.rs (replaces JSON)
```

---

### 3.7 RL Layer — Adaptive Learning

`integration_score` per module feeds LinUCB bandit in touring-learning:

- **Reward**: Module goes from score < 0.5 to 1.0 in same session = high reward
- **Penalty**: Module remains orphan across sessions = negative reward
- **Bandit learns**: Which creation patterns tend to produce orphans
- **Adapts thresholds**: If history shows `pub use` in `lib.rs` without consumer
  are 90% orphans, lowers warning threshold for those cases

---

## 4. Schema Changes

### SCHEMA_VERSION 5 -> 6

```sql
-- Migration only runs if PRAGMA user_version < 6

-- L1: Add symbol granularity to file relations
ALTER TABLE file_relations ADD COLUMN imported_symbols TEXT DEFAULT '[]';

-- L1: Wiring map for tracking pub symbol -> consumer connections
CREATE TABLE IF NOT EXISTS wiring_map (
    module_file TEXT NOT NULL,
    symbol_name TEXT NOT NULL,
    symbol_kind TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'public',
    consumer_file TEXT,
    import_line INTEGER,
    contract_source TEXT DEFAULT 'ast_read',
    resolved_at TEXT,
    PRIMARY KEY(module_file, symbol_name, COALESCE(consumer_file, ''))
);
CREATE INDEX IF NOT EXISTS idx_wiring_orphans
    ON wiring_map(consumer_file) WHERE consumer_file IS NULL;
CREATE INDEX IF NOT EXISTS idx_wiring_module
    ON wiring_map(module_file);

-- L0: Module ecosystem tracking
CREATE TABLE IF NOT EXISTS module_ecosystem (
    file_path TEXT PRIMARY KEY,
    module_role TEXT NOT NULL DEFAULT 'internal',
    parent_module TEXT,
    pub_symbol_count INTEGER DEFAULT 0,
    import_count INTEGER DEFAULT 0,
    re_export_count INTEGER DEFAULT 0,
    integration_score REAL DEFAULT 0.0,
    last_scanned_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_ecosystem_score
    ON module_ecosystem(integration_score);

PRAGMA user_version = 6;
```

---

## 5. Implementation Map

| Crate | File | Change | Type | Sprint |
|-------|------|--------|------|--------|
| **touring-core** | `migration.rs` | SCHEMA_VERSION 5->6 + DDL | Schema | H0 |
| **touring-ast** | `graph.rs` | `ImportInfo` add `source_line: usize` | Enhance | H0 |
| **touring-ast** | New: `wiring.rs` | `WiringAnalyzer`: analyze_pub_symbols, diff_symbols, predict_imports | New | H1 |
| **touring-hooks** | `knowledge.rs` | CRUD wiring_map + module_ecosystem + integration_score queries | New | H0 |
| **touring-hooks** | `post_read.rs` | Replace extract_imports_fast with touring_ast::extract_imports | **Foundation** | H0 |
| **touring-hooks** | `post_read.rs` | After extract: populate wiring_map with pub symbols + consumers | New | H0 |
| **touring-hooks** | `pre_edit.rs` | Signal 6a (wiring check) + 6b (import prediction) + 6c (ecosystem fit) | **Key** | H1 |
| **touring-hooks** | `post_edit.rs` | Diff AST + wiring contract resolve + integration_score update | New | H1 |
| **touring-hooks** | `lifecycle.rs` | `handle_file_changed` expanded with systemic verification + cascade | **Key** | H2 |
| **touring-hooks** | New: `ecosystem.rs` | `ModuleEcosystem::scan()`, entry point detection, pub use chains | New | H2 |
| **touring-cortex** | New: `handlers/integration.rs` | H83 IntegrationCompletenessHandler (audit + suggestions) | New handler | H3 |
| **touring-learning** | RL existing | integration_score as reward signal in LinUCB | Enhance | H3 |

---

## 6. Sprint Plan

### H0 — Foundation (~8h)
L1: Schema v6 + wiring_map CRUD + extract_imports rich + pub symbol tracking

### H1 — Pre/Post Edit Intelligence (~16h)
L2 + L3: Pre-edit signals (6a/6b/6c) + post-edit AST diff + wiring tracking

### H2 — Ecosystem & FileChanged (~16h)
L0 + L4: Module ecosystem scanner + file-changed systemic verification + cascade

### H3 — Audit & RL (~12h)
L5 + RL: Session audit handler H83 + integration suggestions + adaptive learning

---

## 7. Success Criteria

1. **Zero orphan modules go undetected**: Any `pub` symbol without a consumer
   within the same crate/workspace triggers a warning within 1 edit cycle
2. **Import prediction accuracy >= 80%**: Suggested imports are correct 4 out of 5 times
3. **FileChanged cascade detects broken imports**: When a pub symbol is removed,
   all consumers are flagged within the same hook cycle
4. **Integration score is visible**: Every module has a measurable score (0.0-1.0)
5. **Suggestions are actionable**: At least 70% of integration suggestions are accepted
6. **RL improves over time**: False positive rate decreases across sessions
7. **Latency preserved**: pre-edit < 10ms additional, post-edit < 5ms additional,
   file-changed < 15ms, session audit < 100ms

---

## 8. Invariants

1. **Exit 0 always**: Wiring intelligence NEVER blocks Claude Code
2. **Clippy deny all**: All new code passes clippy with -D warnings
3. **Zero unwrap in production**: Use `?`, `.expect()`, `.unwrap_or_default()`
4. **Schema gate**: SCHEMA_VERSION=6 gated — migration only runs once
5. **Backward compat**: Existing knowledge DBs work with new schema (ALTER TABLE)
6. **Latency budget**: Each layer has explicit latency target; degrade gracefully

---

*Wiring Intelligence System v2 — Design by Gabriel Gadea + Claude Opus 4.6*
*touring v28.0.0 target — 6 layers, 4 sprints, 12 file changes across 5 crates*
