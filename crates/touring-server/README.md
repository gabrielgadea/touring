# touring-server

> The MCP bridge and Cortex CLI surface — how editors, agents, and the shell talk
> to Touring. Master Plan D.W3.P2.

## Purpose

`touring-server` exposes Touring's capabilities through two channels: the **MCP
server** (structured tools for editors/agents) and the **Cortex CLI** (the
`touring <subcommand>` handlers). It is the thin, schema'd layer over the daemon
RPC — it routes and validates, it does not reimplement the intelligence.

## Architecture

| Area | What |
|---|---|
| MCP server | ~26 `touring_*` tools advertised over `touring serve` (stdio↔socket bridge) |
| Cortex CLI | ~97 handlers (H1–H97) backing the `touring` subcommands |
| Hooks | ~68 lifecycle hooks dispatched via the bridge |
| reasoning / visual / session | Leaf modules split out for layering (W9) |

## Key entry points

```bash
touring serve                 # start the MCP bridge (one per editor session)
touring doctor -j             # daemon component health
touring status -j             # dashboard + composite_health_score
```

MCP tools appear to clients as `mcp__touring__*` and follow the token-efficient
workflow (`touring_minimal_context` → `detail_level='minimal'` → `_next_tools`).

## Example

```bash
# From the shell: read-only queries route through the CLI handlers (<10ms)
touring e2e -j

# From an editor: structured write-ops route through MCP (~200ms)
#   mcp__touring__decompose_create, mcp__touring__memory_store, …
```

## Caveats

- **Two channels, different latencies.** Prefer the CLI for read-only queries
  (<10ms); use MCP for writes and structured tool-calls (~200ms).
- Interactively-authenticated MCP servers may be **absent in headless/cron**
  runs — keep capabilities reachable via the CLI/daemon path too.
- Large crate (~61k LOC); the layered split (reasoning/visual/session) is W9 and
  `src/snapshot/` is dead code slated for removal (W1).
- The binary is named `touring` (not `touring-server`). Tests look for
  `target/debug/touring` or `target/release/touring`.

## Build

```bash
# Build the touring binary (all 17 default features ON)
cargo build -p touring-server

# Release binary
cargo build -p touring-server --release

# Heap-profiling build (mutually exclusive with prod-allocator)
cargo build -p touring-server --no-default-features --features dhat-heap,...
```

The binary is `touring` (not `touring-server`). The library crate name is
`touring_server`.

## Tests

```bash
# Build first — binary_e2e tests spawn the touring binary
cargo build -p touring-server
cargo test -p touring-server                          # 502 tests (339 unit + 163 integration + binary_e2e)
cargo clippy -p touring-server -- -D warnings
```

The binary E2E suite (`tests/binary_e2e.rs`) spawns the compiled `touring`
binary and exercises CLI subcommands end-to-end. MCP tool tests cover all
~26 `touring_*` tools; CLI handler tests cover the ~97 Cortex CLI handlers
(H1–H97).

## Contributing

`touring-server` co-evolves with `touring-cortex` (handler implementations),
`touring-hooks` (hook lifecycle), and the MCP tool catalog in `src/server/`.
When adding a new CLI subcommand or MCP tool: (1) add the handler in
`src/cli/` or `src/server/`, (2) wire it into the command table
(`src/cli/command_table.rs`), (3) add unit + integration tests, (4) run
`touring-quality score crates/touring-server --fail-below 0.80`.

The crate is the thin schema'd routing layer — intelligence lives in
`touring-cortex`, `touring-hooks`, and the daemon. Keep handlers small:
validate inputs, delegate to the daemon RPC, return structured output.

## License

Part of the Touring workspace; see the workspace root for licensing.
