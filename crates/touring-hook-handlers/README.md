# touring-hook-handlers — Pre/Post Lifecycle Hook Handlers

The **hook handler layer**, carved from `touring-dispatch` on 2026-06-10
(Wave H, PoNR #5 — the final cut of the daemon-lib-rearch plan). After this
carve the former 169k/36% monolith's largest fragment (touring-dispatch) is
**37.5k = 7.8%** — third largest in the workspace.

## Overview

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

## Installation

Internal workspace crate — not published. A consumer inside the workspace depends on it by path and
forwards the features it needs (touring-dispatch does exactly this):

```toml
touring-hook-handlers = { path = "../touring-hook-handlers", features = ["pre-hooks", "post-hooks", "session-hooks"] }
```

The binaries that run these handlers (`touring-hook`, `touring-daemon`) are built and installed only through
`update-touring`, and propagated to per-project toolchains with `scripts/propagate-release.sh <version>` —
never a standalone `cargo build` (the touring rebuild protocol: `update-touring` owns kill → build → install →
restart → verify).

## Usage

Each handler takes the shared runtime and the hook's JSON payload. The `_returning` forms give the response
back instead of printing and exiting — the form the daemon and the tests use:

```rust,ignore
use touring_hook_runtime::hook_runtime::HookRuntime;

let mut rt = HookRuntime::new(project_root)?;
let payload = serde_json::json!({"session_id": "…", "hook_event_name": "SessionStart"});
let response = touring_hook_handlers::session_hooks::run_session_start_returning(&mut rt, &payload);
println!("{}", response.to_json());
```

Synthetic sessions (`touring_foundation::synthetic_session::is_synthetic_session_id`, e.g. the analise
contract suite's `hooks-contract-*`) are observed but never persisted as learning by the session handlers.

## Development

- Feature-gated modules only compile with their feature: a test filter that matches **zero** tests is an
  absent measurement, not a green run (`session_hooks` needs `--features session-hooks,post-hooks`).
- Format only the files you touched (`rustfmt --edition 2024 <file>`); `rustfmt` on a `lib.rs` walks every
  `mod` it declares, which is the crate-wide `fmt` the workspace rules forbid.
- New public symbols must be wired to a consumer (REGRA #0); run `cargo clippy -p touring-hook-handlers
  --all-targets --all-features -- -D warnings` before committing.

## Testing

```bash
cargo test -p touring-hook-handlers --all-features        # the full handler surface
cargo test -p touring-hook-handlers --no-default-features # parity of the always-on handlers
cargo test -p touring-hook-handlers --features session-hooks,post-hooks --lib session_hooks
```

## License

MIT OR Apache-2.0 — inherited from the workspace (`license.workspace = true`).
