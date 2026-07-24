# touring-hooks-core — Data/Intelligence Engine Layer

The **engine layer** of the Touring Neural Hooks system, carved from
`touring-hooks` on 2026-06-10 (**Phase C** of the daemon-lib-rearch plan —
`~/.claude/plans/daemon-lib-rearch/plan.md`).

## Purpose

`touring-hooks` had grown into a monolith mixing two very different layers:

1. **Engines** — stateful data/intelligence subsystems (SQLite knowledge
   graph, Tantivy FTS, health-delta tracking, AST/ACO/cognitive bridges)
   with **zero** coupling to the daemon dispatch machinery.
2. **Dispatch** — `HookRuntime` (the God-object), `hook_registry`, `cli/`
   handlers, the daemon actor and the pre/post hook handlers.

This crate is layer 1: **48 modules / ~30k LOC** with the invariant that
nothing here names `HookRuntime`, `hook_registry` or `cli/` (verified by the
partition classifier — comments and string-literals stripped).

## Module map

| Group | Modules |
|---|---|
| Knowledge | `knowledge` (4.4k SQLite WAL graph), `async_knowledge`, `knowledge_wiring` (wiring_map CRUD — inherent `impl FileKnowledgeDB`), `knowledge_symbol_bridge` |
| Search | `tantivy_index` (feature `tantivy-fts`), `sandbox_output_store` |
| Quality | `health_delta`, `health_delta_audit`, `mutation_test`, `conformal`, `error_predictor` |
| Bridges | `ast_bridge`, `aco_bridge`, `cognitive_bridge`, `nlp_bridge` (feature `nlp-enrichment`) |
| Safety | `circuit_breaker`, `circuit_state_machine`, `branch_fs`, `panic_log`, `approval_store` |
| Session | `shared::session_bus`, `session_guide`, `session_insights` (feature `session-hooks`), `cortex_dispatcher` |
| Infra | `ipc`, `throttle`, `compression_profiles`, `output_capture`, `dependency_cache`, `ecosystem`, `proc_identity`, … |

## Consumers

`touring-hooks` re-exports **every** module here at its historical path
(`pub use touring_hooks_core::knowledge;` …), so `touring_hooks::knowledge`,
`crate::tantivy_index` (inside touring-hooks) and every cross-crate consumer
(touring-server × 50+ imports) resolve unchanged.

## Feature flags

All feature names mirror `touring-hooks`, which forwards each one
(`tantivy-fts`, `shadow-workspace`, `capnp-server`, `session-hooks`,
`nlp-enrichment`, `utilities`, `inventory-registry`, `mpatch-fuzzy`,
`quantization`). This crate's `default = []` — the façade decides.

> Run `cargo test -p touring-hooks-core --all-features` to exercise the
> gated modules (605 tests; 485 without features).

## Layering contract (do not violate)

- This crate MUST NOT depend on `touring-hooks` (the façade) — that would be
  a cycle. The dispatch layer calls *down* into the engines, never the
  reverse.
- `shared::session_bus` lives here (not in `touring-hooks-shared`) because it
  consumes `ann_memory` from `touring-hooks-prediction`, which already
  depends on the leaf — relocating it to the leaf would cycle.
- `knowledge_wiring` holds the inherent `impl FileKnowledgeDB` carved from
  `touring-hooks/src/wiring.rs`: Rust requires inherent impls to live in the
  crate that defines the type. The wiring *engine* (impact BFS, Tarjan,
  repair) stays in the dispatch layer.
