# touring-hook-runtime — HookRuntime Substrate Layer

The **HookRuntime layer**, carved from `touring-dispatch` on 2026-06-10
(Wave R+C, PoNR #3 — `~/.claude/plans/daemon-lib-rearch/data/wave_runtime_cli_manifest.md`).

## Purpose

Owns the boundary object every hook and cli handler receives, plus the
services wired into it as fields:

| Group | Modules |
|---|---|
| Runtime | `hook_runtime` (the God-object, 2.2k), `runtime/` (traits + `impls_{aco,cognitive,context,hook,knowledge,rl,symbols}` — the domain decomposition) |
| IoC | `ceg_impls` (`LearnRuntime`/`CegRuntime` impls — orphan rule: they must live with the type — + the 3 runtime-service handlers `cli_{learning_reward,gotcha_add,memory_store}`) |
| Services | `inferlets{,_assets}` (WASM sandbox), `auto_save_hook`, `triad_hook`, `gotcha_loader`, `embeddings` (S-04 semantic chain) |
| Engines | `hook_memory`, `wiring` (+`hypergraph`), `shared::{reindex, signals, session_context, quality}` |
| Protocol | `daemon_protocol::ProjectCommand` (actor envelope), `schemas/` payload validation |

## Layering

```
touring-dispatch (hooks/ · hook_registry · daemon · cli/)
   ↓ depends on + re-exports at historical paths
touring-hook-runtime (this crate)
   ↓ depends on
touring-hooks-core (knowledge, tantivy, bridges) → leaves
```

`touring-cli` (PoNR #4, carved 2026-06-10) depends on this layer directly —
the cli/ ↔ dispatch entanglement is broken. This crate also received
`prompt_enhance` (2.3k) and `protocol/` (ACP shim, `acp-protocol` feature)
in the C2 inversion wave.

## Features

`tantivy-fts` (→core) · `inferlets-wasm` · `post-hooks` · `saga` ·
`semantic-embeddings` (→optional touring-storage, arctic-embed-m 768d) —
all forwarded by touring-dispatch (and transitively by the touring-hooks
façade).

## Build

```bash
cargo build -p touring-hook-runtime
# with every optional service enabled:
cargo build -p touring-hook-runtime \
  --features "tantivy-fts,inferlets-wasm,post-hooks,semantic-embeddings"
```

## Usage

`HookRuntime` is the boundary object every hook and cli handler receives. Build
it once per project root; handlers return a `HookResponse` that serializes to
the Claude Code hook wire format.

```rust
use std::path::Path;
use touring_hook_runtime::{HookResponse, HookRuntime};

// Built once per project root (fallible — opens the knowledge DB, wires services).
let mut rt = HookRuntime::new(Path::new("/path/to/project"))?;

// A handler decides on a response. `to_json()` renders the wire format as a
// string (tests, logging); `emit()` writes the same bytes to stdout and exits.
let resp = HookResponse::context_with_event("extra context for the model", "PreToolUse");
// → {"hookSpecificOutput":{"additionalContext":"…","hookEventName":"PreToolUse"}}
println!("{}", resp.to_json());
resp.emit(); // prints the same JSON, then std::process::exit(0)
```

`HookResponse::Allow` emits nothing (empty stdout signals "no action"); every
other variant (`Context`, `Deny`, `Block`, `Halt`, `ContextWithUpdatedInput`)
serializes through the single `to_json` source of truth — `emit` delegates to it,
so the wire format can never drift between the two.

## Tests

```bash
cargo test -p touring-hook-runtime \
  --features "tantivy-fts,inferlets-wasm,post-hooks,semantic-embeddings"
```

## Contributing

Part of the [`touring`](../../) workspace — follow the workspace conventions
(REGRA #11: never invoke `git` directly; use `update-touring` for rebuilds).
Any change to the `HookRuntime` boundary or `HookResponse` wire format must keep
`emit` and `to_json` in lockstep (they share one serialization source).

## License

Workspace-inherited (`license.workspace = true`).
