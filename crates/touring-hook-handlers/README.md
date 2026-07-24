# touring-hook-handlers — Pre/Post Lifecycle Hook Handlers

The **hook handler layer**, carved from `touring-dispatch` on 2026-06-10
(Wave H, PoNR #5 — the final cut of the daemon-lib-rearch plan). After this
carve the former 169k/36% monolith's largest fragment (touring-dispatch) is
**37.5k = 7.8%** — third largest in the workspace.

## Purpose

Owns every Claude Code lifecycle hook handler the dispatch table routes to:

| Group | Modules |
|---|---|
| Pre-hooks (`pre-hooks` feature) | `pre_{bash,edit,edit_prevention,glob,grep,read,tool_use,write}` |
| Post-hooks (`post-hooks`) | `post_{bash,edit,edit_rule_engine,read,tool_batch,tool_failure,tool_rl,tool_use,write}` |
| Session (`session-hooks`) | `session_hooks` |
| Always-on | `permission_request`, `post_compact_handler`, `stop`, `instructions_loaded`, `hooks_task_lifecycle`, `team_hooks` |
| Companions | `hook_decompose_bridge`, `mcts_materializer` (single-consumer chains) |
| Hook-only shared engines | `shared::{metadata_collector, signal_pipeline}` |

## Layering

```
touring-dispatch (hook_registry · daemon · lifecycle/)   37.5k = 7.8%
   ↓ depends on + re-exports at historical paths
touring-hook-handlers (this crate)                       26.2k = 5.5%
   ↓ depends on
touring-cli (cli handlers) → touring-hook-runtime → touring-hooks-core → leaves
```

## Wave H inversions (pre-carve)

- `ceg_adapter`, `task_digest`, `suggesters`+`bidirectional` moved down to
  touring-hook-runtime (consumers on both sides of the cut)
- `shared::tantivy_stream` moved to the runtime layer (daemon.rs + post hooks),
  now properly `tantivy-fts`-gated (the monolith's default-on feature masked it)
- runtime gained `txn_lock_enforcement` (2 ceg_adapter tests) and the
  `capability`/`gate_metrics` re-exports

## Features

`pre-hooks` · `post-hooks` (=pre) · `session-hooks` (→core) · `tantivy-fts`
(→core+runtime+cli) · `mpatch-fuzzy` (→core) · `nlp-enrichment` (→core) ·
`resource-monitor` (→foundation) — all forwarded by touring-dispatch.
Tests: 633 full-features / 103 no-default (parity holds).
