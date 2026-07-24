# touring-cli — CLI Daemon-Side Query Handlers

The **cli layer**, carved from `touring-dispatch` on 2026-06-10
(Wave C2, PoNR #4 — `~/.claude/plans/daemon-lib-rearch/data/wave_runtime_cli_manifest.md`).
This carve closed the elite goal: **no crate >15% of the workspace**
(dispatch 108.6k → 66.2k = 13.9%).

## Purpose

Owns every daemon-side cli handler the dispatch table calls:

| Group | Modules |
|---|---|
| Handler tree | `cli/` (55 files — kpi, evolution, repo_score, repo_health, polyglot, scout, memory, decompose, mpatch, acp, saga, execute, viz, health, …) |
| Handler hubs | `cli_handlers{,_decompose,_entity,_index,_file_knowledge,_wiring_repair,_semantics,_session,_mcp,_mutation_test}` (the `#[path = "cli/handlers/*.rs"]` decls) |
| Suggestion engine | `cli_suggester` (PreToolUse classifier — best Touring command per (tool, input)) |
| E2E | `cli_e2e` (comprehensive code analysis handler) |
| Workflow Intelligence | `workflow/` (CEG Pln2 P8 stage/antipattern/advise — moved with its single consumer, cli_suggester) |

## Layering

```
touring-dispatch (hooks/ · lifecycle/ · hook_registry · daemon)
   ↓ depends on + re-exports at historical paths (288 downward call sites)
touring-cli (this crate)
   ↓ depends on
touring-hook-runtime (HookRuntime substrate)
   ↓
touring-hooks-core (knowledge, tantivy, engines) → leaves
```

Cross-crate consumers (touring-server's 22 `touring_hooks::cli_handlers_*`
imports) reach this crate through the double façade
touring-hooks → touring-dispatch → touring-cli, unchanged byte-for-byte.

## Wave C2 inversions (pre-carve, all ship-green)

- `prompt_enhance.rs` (2.3k) + `protocol/` (ACP shim) moved **down to touring-hook-runtime**
- `emit_b302_if_low_confidence_expansion` moved **down to touring-hooks-core::health_delta**
- 30 `maybe_*_hint_on_task_create` matchers + dispatcher → **NEW touring-hooks-core::generator_hints**
- `workflow/` (P8) moved **into this crate** with its single consumer

## Features

`tantivy-fts` (→core+runtime) · `acp-protocol` (→runtime) · `templates`
(→touring-orchestration) · `mpatch-fuzzy` (→core) · `ann-blast` (pure gate) —
all forwarded by touring-dispatch. Tests: `cargo test -p touring-cli
--features "tantivy-fts,templates,mpatch-fuzzy,ann-blast,acp-protocol"` (217).
