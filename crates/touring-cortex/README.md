# touring-cortex

> **Centralized hook execution engine (Cortex)** — 84+ builtin handlers, a
> typestate pipeline, context enrichment, and RL state/action mapping, extracted
> from `touring-server` as an autonomous crate (S4).

`touring-cortex` is the heart of the Touring hook system. Every lifecycle
event (`PreToolUse`, `PostToolUse`, `SessionStart`, `SessionEnd`, and more)
is routed through a `Pipeline` of `Handler` implementations. Handlers accumulate
state into a `CortexContext`, emit `Decision`s (block / allow / skip), and
produce a `CortexOutput` that carries context-line injections back to the caller.

The crate also ships Reciprocal Rank Fusion (`fusion`), directed call-graph
analysis via `petgraph` (`call_graph`), prompt-cache stratification
(`cache_strategy`), and the shared RL state/action mapping (`rl_mapping`) used
by both this crate and `touring-server`.

## Architecture

```text
  HookEvent
      │
      ▼
  Pipeline ──── registered handlers (H1–H97) ───► CortexContext (accumulates state)
      │                                                │
      │                                                ▼
      │                                        HandlerResult (per-handler)
      │                                                │
      └───────────────────────────────────────────────▼
                                              CortexOutput (full pipeline envelope)
                                                    │
                                                    ├─► context lines injected to CC
                                                    ├─► RL signals (rl_mapping)
                                                    └─► metrics (metrics.rs)
```

Key modules:

| Module | Role |
|---|---|
| `types` | `HookEvent`, `Decision`, `HandlerResult`, `CortexOutput`, `HookSpecificOutput` |
| `handler` | `Handler` trait — implement to add a custom handler |
| `context` | `CortexContext` — shared mutable state through the pipeline |
| `pipeline` | `Pipeline` — ordered execution with event/tool filtering |
| `runtime` | `CortexRuntime` — initialization and main entry point |
| `cache_strategy` | Prompt cache stratification (`StableSessionContext` / `VolatilePromptContext`) |
| `enrichment` | Context enrichment pipeline — combines scored signals |
| `rl_mapping` | RL state/action mapping (shared with `touring-server`) |
| `fusion` | Reciprocal Rank Fusion for combining ranked retrieval results |
| `call_graph` | Directed call-graph analysis (callees, callers, cycles, hotspots) |
| `handlers/` | 84+ builtin handlers organized by domain (H1–H97) |
| `fascicles` | Fascicle scoring (feature `fascicles`, default ON) |
| `scoring` | Composite scoring utilities |
| `metrics` | Pipeline observability counters |

## Build

```bash
# Default build (fascicles + cognitive-memory features ON)
cargo build -p touring-cortex

# Release
cargo build -p touring-cortex --release
```

The library crate name is `touring_cortex`. There is no standalone binary; the
crate is consumed by `touring-server` and other workspace members.

## Usage

```rust
use touring_cortex::{
    Pipeline, Handler, CortexContext,
    HookEvent, Decision, HandlerResult, CortexOutput,
};

// Build a pipeline with all builtin handlers
let mut pipeline = Pipeline::default();
touring_cortex::builtin_handlers::register_all(&mut pipeline);

// Route an event
// (CortexRuntime::handle is the top-level entry — see runtime::CortexRuntime)
```

### Implement a custom handler

```rust
use touring_cortex::{Handler, CortexContext, HandlerResult, HookEvent};

struct MyHandler;

impl Handler for MyHandler {
    fn name(&self) -> &str { "my-handler" }

    fn handle(&self, event: &HookEvent, ctx: &mut CortexContext) -> HandlerResult {
        // inspect event, mutate ctx, return verdict
        HandlerResult::default()
    }
}
```

### Prompt cache stratification

```rust
use touring_cortex::{StableSessionContext, VolatilePromptContext, compose_stratified_context};

let stable  = StableSessionContext::new(/* … */);
let volatile = VolatilePromptContext::new(/* … */);
let composed = compose_stratified_context(&stable, &volatile);
```

## Tests

```bash
cargo test -p touring-cortex              # unit tests across all modules
cargo clippy -p touring-cortex -- -D warnings
```

The handler suite exercises each of the 84+ builtin handlers via the
`handlers/` submodule tests. The `call_graph` and `fusion` modules carry
their own unit fixtures.

## Contributing

`touring-cortex` co-evolves with `touring-server` and `touring-hooks`: a new
handler added here must be registered in `builtin_handlers::register_all`,
exercised in `handlers/` tests, and reflected in the affected skill or D-rule
documentation. Run `touring-quality score crates/touring-cortex --fail-below
0.80` before submitting — the crate must stay at Gold tier or above.

The crate enforces `#![cfg_attr(not(test), deny(clippy::unwrap_used))]` and
`#![deny(missing_docs)]`; all public items must carry doc comments.

## License

Part of the Touring workspace; see the workspace root for licensing.
