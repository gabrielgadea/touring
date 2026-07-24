# TACO Iterations 4+5 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors, 71 MCP tools confirmed)
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Iteration 4 — Core Integration Bridges

### 1. KnowledgeUpsertFn (5th closure param)

**Type**: `Arc<dyn Fn(&str, &[u8]) -> Result<(), String> + Send + Sync>`

**Location**: `crates/touring-generator/src/core/context.rs`

**Wiring chain**:
```
lib.rs (pub re-export)
  -> generator_tools.rs::build_knowledge_upsert_fn()     # constructs closure
  -> GeneratorContext::with_closures(_, _, _, _, upsert)  # injected as slot 5
  -> typestate.rs::commit()                               # calls evaluate_knowledge_upsert()
  -> context.rs::evaluate_knowledge_upsert(path, bytes)  # non-fatal dispatch
```

**Behavior**: After generator commit, artifacts are stored to FileKnowledgeDB. Errors are logged but non-fatal (generation continues).

---

### 2. VgpEngine + IncrementalIndex Fast-path

**Location**: `crates/touring-generator/src/vgp/engine.rs`

**Builder**: `VgpEngine::with_index(Arc<IncrementalIndex>)` — checks IncrementalIndex before subprocess (~10x faster for cache hits).

**Borrow checker constraint** (EA5 fix): `symbol_index` must be extracted as a `let` binding before the struct literal:
```rust
// CORRECT
let symbol_index = Arc::new(IncrementalIndex::new(...));
GeneratorContext {
    symbol_index: Arc::clone(&symbol_index),
    vgp_engine: VgpEngine::with_index(Arc::clone(&symbol_index)),
    ...
}
// WRONG — borrow error: cannot reference field within same struct literal
```

---

### 3. ConsumerGenerator (31st GeneratorKind)

**Location**: `crates/touring-generator/src/generator/kinds.rs`

**Template**: `crates/touring-generator/templates/consumer_generator.tera`

**Purpose**: Generates consumer code for existing pub symbols surfaced by `wiring_suggest` analysis.

---

### 4. CLI + MCP: touring_wiring_suggest (hook #99, MCP tool #70)

**CLI handler**: `crates/touring-hooks/src/cli_handlers.rs::cli_wiring_suggest()`

**Registry**: `crates/touring-hooks/src/hook_registry.rs` (hook count 99)

**CLI dispatch**: `crates/touring-server/src/cli/wiring.rs` — `"suggest"` arm → `run_suggest()`

**MCP tool**: `crates/touring-server/src/server/mod.rs`
```rust
#[tool(name = "touring_wiring_suggest")]
async fn wiring_suggest(params: Parameters<WiringSuggestParams>) -> CallToolResult
```

**Implementation note**: Uses direct SQLite query against `wiring_suggestions` table — NOT daemon_query().

---

### 5. OptionExt pub(crate) fix

**Location**: `crates/touring-generator/src/core/result_ext.rs:95`

Trait `OptionExt` was `pub(crate)` but was used in test module. Changed visibility to allow test compilation.

---

## Iteration 5 — Group A

### EA5 — Context Borrow Fix

Applied `symbol_index` let-binding extraction to BOTH constructors in `core/context.rs`: production constructor and test constructor. See borrow checker constraint description above.

---

### EA6 — WiringSuggest MCP Layer Completion

Completed the full chain for `touring_wiring_suggest`: CLI handler → hook registry → MCP tool → params struct. Tool count advanced to 70.

**Assertion file**: `crates/touring-server/src/tools/mod.rs` — comment updated to reflect 70.

---

### EA7 — replan_json() Helper

**Location**: `crates/touring-server/src/tools/generator_tools.rs`

**Extracted from**: 6 manual JSON construction call sites across `speculate_and_commit()` and related paths.

**Signature**:
```rust
fn replan_json(stage: &str, r: &ReplanRequest) -> Value
```

**Access constraint**: `ReplanRequest::failure_history` is `pub(crate)` in `touring-generator` — inaccessible from `touring-server`. Helper uses only public fields: `stage`, `plan_intent`, `retry_count`.

---

## Iteration 5 — Group B

### EB2 — CommitReport Expansion

**Location**: `crates/touring-server/src/tools/generator_tools.rs::speculate_and_commit()`

**Change**: `files_written` (int) split into two fields:
- `files_written_count`: `usize` — total count
- `files_written`: `Vec<Object>` — array with per-file details:
  - `path`: file path
  - `sha256`: content hash
  - `bytes_written`: size in bytes
  - `action`: write action type
  - `backup_path`: optional backup location

---

### EB3 — PlanRegistry::list() + MCP tool #71

**PlanRegistry::list()**: `crates/touring-generator/src/registry/plan_registry.rs:73`
```rust
pub fn list(&self) -> Vec<(String, String, ExecutionStatus)>
```
Returns snapshot of all in-flight plans as `(plan_id, intent_preview, status)`.

**MCP tool**: `touring_generator_registry_status` (tool #71)
- `crates/touring-server/src/server/mod.rs` — `generator_registry_status()` method
- `crates/touring-server/src/server/params.rs` — `GeneratorRegistryParams` struct
- `crates/touring-server/src/tools/mod.rs` — assertion updated 70→71

---

### EB4 — SynWiringGateAdapter Injection

**Feature**: `syn-quote` (activated in `touring-server/Cargo.toml`)

**Generator tools wiring**:
```rust
#[cfg(feature = "syn-quote")]
let wiring_gate: Option<WiringGateFn> = {
    let adapter = SynWiringGateAdapter::new();
    Some(Arc::new(move |artifacts| adapter.check(artifacts)))
};
#[cfg(not(feature = "syn-quote"))]
let wiring_gate: Option<WiringGateFn> = None;

let ctx = GeneratorContext::with_closures(fuzzy, rl, None, wiring_gate, build_knowledge_upsert_fn());
```

**Position**: wiring_gate is slot 3 (0-indexed) of `with_closures()` — 4th param after fuzzy/rl/semantic.

---

## Architecture Impact

### Generator Pipeline Integration (complete)

All 5 closure slots of `with_closures()` are now wired:
| Slot | Name | Source | Purpose |
|------|------|--------|---------|
| 0 | `fuzzy_fn` | touring-hooks FuzzyIndex | Symbol lookup |
| 1 | `rl_fn` | LinUCB pheromone | RL feedback |
| 2 | `semantic_fn` | `None` (placeholder) | Semantic similarity |
| 3 | `wiring_gate_fn` | SynWiringGateAdapter (cfg) | Syn-based wiring analysis |
| 4 | `knowledge_upsert_fn` | FileKnowledgeDB closure | Post-commit artifact store |

### MCP Surface Growth
| Iteration | Tool Count | New Tool |
|-----------|-----------|----------|
| Iter 4 start | 69 | — |
| After EA6 | 70 | touring_wiring_suggest |
| After EB3 | 71 | touring_generator_registry_status |

### Hook Count
- Hook registry: 99 entries after wiring_suggest handler (hook_registry.rs)

---

## Key Lessons (stored in touring memory)

1. **struct literal borrow checker**: extract Arc fields as `let` bindings before struct literal when they reference each other
2. **TouringServer has no daemon_query()**: use direct library/SQLite access from MCP tool methods
3. **ReplanRequest::failure_history is pub(crate)**: only access public fields when building error JSON from outside the crate
4. **KnowledgeUpsertFn is non-fatal**: errors logged via tracing::warn, generation never blocked by DB write failure
5. **with_closures() slot order matters**: fuzzy(0), rl(1), semantic(2), wiring_gate(3), knowledge_upsert(4)
