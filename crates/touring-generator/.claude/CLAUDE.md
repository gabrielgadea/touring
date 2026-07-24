# touring-generator — Crate Instructions

## What this crate does

LLM-as-Planner / Touring-as-Generator. Receives `GeneratorPlan` JSON from an LLM, verifies symbols (VGP), renders templates (Tera), validates speculatively, and commits atomically with RL feedback, session lifecycle tracking, and decompose task integration.

## Key invariants

- **Typestate pipeline**: Draft -> Verified -> Rendered -> Speculated -> Committed. Invalid transitions are compile errors.
- **NormalizedScore [0.0, 1.0]**: Never use raw `f64` for scores. Always `NormalizedScore::new()` or `::clamped()`.
- **No `unwrap()` in production**: Use `?`, `.expect()` only for infallible startup (rayon pool, regex).
- **No `#[allow(dead_code)]` or `#[allow(unused)]`**: REGRA #0 POTENCIALIZAR. The `SynWiringGateAdapter` enforces this at commit time.
- **No `Arc::get_mut` in make_context**: `with_closures()` returns `GeneratorContext` (not `Arc<Self>`). All 13 closure fields are injected via direct `ctx.field = ...` assignment, then wrapped in `Arc::new()` at end. Previous pattern using `Arc::get_mut` only succeeded for the FIRST conditional block — all subsequent injections were silently dropped.
- **RL reward on decompose failure**: `decompose_create_task()` and `decompose_update_status()` inject RL penalty (-0.5 / -0.3) when the decompose bridge fails, so the RL system learns the failure pattern.
- **All templates pre-compiled**: `OnceLock<Tera>` in `template/engine.rs`. Add new templates there + update `template_names()`.
- **Closure dispatch for cross-crate**: Use the 13 closure fields in `GeneratorContext`, never add direct deps on touring-hooks or touring-cortex.
- **All 10 feature-gated adapters activated by default** via `full` feature in `default`.
- **Session lifecycle auto-calls**: Each typestate transition auto-calls session start/checkpoint/assess.
- **Decompose bridge auto-calls**: Each plan execution creates a task in the touring decompose DAG.
- **`BkTreeFuzzyAdapter` uses BK-tree O(log N)**: `BkTreeFuzzyAdapter::top_k()` uses a real BK-tree with `sz_edit_distance` (feature `simd-fuzzy`) — NOT Vec brute-force O(N×m×n). Lazy-seeded on first `top_k()` call if pool is empty. ~2125× faster for N=10,000 symbol pools.

## How to run tests

```bash
cargo test -p touring-generator                    # 221 tests (full is default now, +10 bktree_e2e)
cargo clippy -p touring-generator -- -D warnings   # must be 0
```

## GeneratorContext closure fields (13 total)

| Field | Type | Injected From | Purpose |
|-------|------|---------------|---------|
| `semantic_graph_fn` | `SemanticGraphFn` | touring-cognitive | Plan similarity via concept nodes |
| `pheromone_fn` | `PheromoneUpdateFn` | touring-simd | ACO template selection RL |
| `cognitive_nexus_fn` | `CognitiveNexusFn` | touring-cognitive | Cross-session plan similarity |
| `wiring_gate_fn` | `WiringGateFn` | touring-analysis + syn | Orphan export gate (hard block) |
| `wasm_sandbox_fn` | `WasmSandboxFn` | touring-wasm | WASM defense-in-depth validator |
| `mcts_eval_fn` | `MctsEvalFn` | touring-cognitive | MCTS synthesis scoring |
| `dspy_sig_fn` | `DspySigFn` | touring-cortex | DSPy signature execution |
| `knowledge_upsert_fn` | `KnowledgeUpsertFn` | touring-hooks | Post-commit FileKnowledgeDB upsert |
| `session_start_fn` | `SessionStartFn` | touring-server | Auto touring session start at verify |
| `session_checkpoint_fn` | `SessionCheckpointFn` | touring-server | Auto checkpoint at speculate |
| `session_assess_fn` | `SessionAssessFn` | touring-server | Auto session assess at commit |
| `decompose_create_fn` | `DecomposeCreateFn` | touring-server | Create task in decompose DAG |
| `decompose_update_fn` | `DecomposeUpdateFn` | touring-server | Update subtask status in DAG |

## VGP Cache Stats

```rust
let stats: VgpCacheStats = engine.cache_stats();
// VgpCacheStats { hits, misses, size, hit_rate, has_index_fast_path }
```

## How to add a new GeneratorKind

1. Add variant to `src/generator/kinds.rs` enum
2. Add `template_name()` match arm
3. Add `label()` match arm
4. Create `templates/<name>.tera` template file
5. Register in `src/template/engine.rs` `templates()` function
6. Add to `template_names()` static slice
7. Add to `all_kinds()` in `tests/e2e_pipeline.rs`
8. Run `cargo test -p touring-generator` to verify

## F-9 modularization (in progress, 2026-06-21)

`src/core/context.rs` (drifted to ~4500 LOC) is being split into cohesive
sibling modules under `src/core/`, each re-exported from `context.rs` via a
`pub use` shim so every `core::context::*` path (and the `lib.rs` re-exports)
resolves unchanged — **zero downstream ripple**. First extraction done:
`context_fuzzy.rs` (`FuzzyMatcher` / `NoopFuzzyMatcher` / `BkTreeFuzzyAdapter` /
`FuzzySuggestion` + BK-tree + tests). When extracting, tests that touch private
internals must move into the new module (same-module private access).

## How to add a new production adapter

1. Add `#[cfg(feature = "your-feature")]` module in `src/core/context.rs` (or a
   `context_*.rs` sibling module re-exported from `context.rs`)
2. Implement the relevant trait (`FuzzyMatcher`, `RlRewardSink`, `TelemetrySink`, etc.)
3. Add `into_closure()` method returning the appropriate `*Fn` type alias
4. Wire in `touring-server/src/tools/generator_tools.rs::make_context()`
5. Add `#[cfg(feature = "your-feature")]` test module
6. Add feature to `Cargo.toml` `[features]` section and `full` composite

## File layout

| Path | Purpose |
|------|---------|
| `src/core/context.rs` | GeneratorContext + 10 adapters + 13 closures (~2400 LOC) |
| `src/executor/typestate.rs` | Typestate pipeline + session/decompose auto-calls |
| `src/vgp/engine.rs` | VGP engine + VgpCacheStats |
| `src/template/engine.rs` | Tera template engine (29 templates) |
| `src/plan/schema.rs` | GeneratorPlan JSON schema |
| `tests/e2e_pipeline.rs` | Main E2E test suite (138 tests) |
| `tests/e2e_cross_audit.rs` | Cross-audit test suite (41 tests) |
| `ARCHITECTURE.md` | Full architecture documentation |

## Skill

Claude Code skill at `~/.claude/skills/touring-generator/SKILL.md` auto-invokes the generator when code artifact creation is needed.
