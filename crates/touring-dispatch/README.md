# touring-dispatch — Daemon Dispatch Layer

The **daemon's nervous system**, carved from `touring-hooks` on 2026-06-10
(**Phase D** of the daemon-lib-rearch plan —
`~/.claude/plans/daemon-lib-rearch/plan.md`).

## Purpose

This crate is everything that *dispatches*: it owns `HookRuntime` (the
boundary object every handler receives), the hook registry, the daemon actor
and all the Claude Code lifecycle handlers. It sits **above**
`touring-hooks-core` (the data/intelligence engines) and **below** the
`touring-hooks` façade that hosts the `touring-hook` / `touring-daemon`
binaries.

| Group | Contents |
|---|---|
| Runtime | `hook_runtime` (HookRuntime, 2.9k — the God-object, next decomposition target), `runtime/`, `hook_registry` (dispatch table), `daemon` (actor + socket) |
| Lifecycle hooks | `hooks/` — every pre_*/post_* handler (pre_read 3.8k, post_edit 2.8k, …), `lifecycle/`, `instructions_loaded`, `stop`, `team_hooks` |
| CLI handlers | `cli/` (55 files, ~19.7k — 176 fns over HookRuntime), `cli_handlers*` (`#[path]`-mapped), `cli_suggester`, `cli_e2e` |
| Dispatch glue | `ceg_adapter` (CEG hook driver), `wiring` (graph/impact engine — the persistence layer lives in core's `knowledge_wiring`), `hook_memory`, `mcts_materializer`, `shared/` (signals, quality, reindex…), `schemas`, `workflow`, `suggesters`, `bidirectional` |

## Layering contract

- Depends on `touring-hooks-core` and re-exports its modules at the
  historical `crate::X` paths (`knowledge`, `tantivy_index`, …) so the moved
  code is byte-identical to the monolith.
- MUST NOT be depended on by `touring-hooks-core` (cycle).
- The `touring-hooks` façade does `pub use touring_dispatch::*;` — consumers
  never name this crate directly.

## Features

Mirror of the historical `touring-hooks` set; the façade forwards every one
(declared there with `default-features = false` so feature control actually
reaches this crate). Core-gated features forward again to
`touring-hooks-core` (`tantivy-fts`, `session-hooks`, …).

> `--no-default-features` does not compile (7 unresolved gated re-exports) —
> **parity with the pre-split monolith**, which had the same latent breakage
> (B.4 "stubs-only" drifted long before the carve).
