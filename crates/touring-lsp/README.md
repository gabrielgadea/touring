# touring-lsp

> **LSP server bridging Touring cross-file capabilities to editors** —
> feature-gated `tower-lsp` integration for go-to-references and symbol
> rename, with a pure mapping layer that compiles without any live server.

`touring-lsp` exposes Touring's working cross-file analysis (references,
rename) to editors over the Language Server Protocol. The crate is split
into two tiers:

- **Default build** — only the pure `mapping` layer and shared `types`/`error`
  modules compile. No `tower-lsp`, no `tokio`, no `touring-hooks`. Fully
  testable in isolation.
- **`--features lsp-bridge`** — adds the async `tower_lsp::LanguageServer`
  implementation in `server`, the binary `touring-lsp` (stdio transport), and
  the `touring-hooks` backend bridge.

This split ensures a default `cargo check --workspace` never pulls the heavy
async stack unless an editor integration is explicitly requested.

## Architecture

```text
  Editor (LSP client)
       │  stdio
       ▼
  touring-lsp (--features lsp-bridge)
  ┌─────────────────────────────────────────────┐
  │  server::TouringLanguageServer               │  ← tower-lsp LanguageServer impl
  │    └── delegates to touring-hooks backend    │    (feature = lsp-bridge)
  ├─────────────────────────────────────────────┤
  │  mapping  (always compiled, pure functions)  │  ← LSP shapes ↔ Touring payloads
  │  types    (always compiled)                  │
  │  error    (always compiled)                  │
  └─────────────────────────────────────────────┘
       │
       ▼
  touring-hooks  (cross-file refs + rename engine)
```

Dependency direction: `touring-lsp` → `touring-hooks` (never the reverse).
`touring-hooks` owns the `HookRuntime`; `touring-lsp` calls into it.

## Build

```bash
# Default — pure mapping layer only (no tower-lsp, no tokio)
cargo build -p touring-lsp

# With LSP bridge + editor binary
cargo build -p touring-lsp --features lsp-bridge
```

The binary is named `touring-lsp` (produced only with `--features lsp-bridge`).
The library crate name is `touring_lsp`.

## Usage

### Mapping layer (no feature required)

```rust
use touring_lsp::mapping::{self, LspPosition};

// Build the JSON payload expected by the Touring backend for a references query
let payload = mapping::references_request_payload(
    "src/auth.rs",
    LspPosition { line: 42, character: 8 },
);
// payload is serde_json::Value ready to send to the daemon
```

### Running the LSP server

```bash
# Start the LSP server over stdio (editors connect automatically via LSP config)
touring-lsp
```

Configure your editor to launch `touring-lsp` as a language server for Rust.
The server advertises `textDocument/references` and `textDocument/rename`
capabilities backed by Touring's cross-file index.

### Quality diagnostics (library)

`touring-lsp` integrates `touring-quality` to map 50-dim per-dimension status
to LSP `DiagnosticSeverity`, enabling live quality feedback in editors when
the quality harness is active.

## Tests

```bash
cargo test -p touring-lsp                 # unit tests (default features)
cargo test -p touring-lsp --features lsp-bridge  # includes server integration tests
cargo clippy -p touring-lsp -- -D warnings
```

The `mapping` module carries pure unit tests that run in any build
configuration (no live server required). The `server` module tests are
guarded by `#[cfg(feature = "lsp-bridge")]`.

## Contributing

`touring-lsp` enforces `#![deny(missing_docs)]`, `#![forbid(unsafe_code)]`,
and `#![cfg_attr(not(test), deny(clippy::unwrap_used))]`. All public items
must carry doc comments. Keep the `mapping` module free of optional
dependencies — it is the zero-cost, always-compiled surface.

When adding a new LSP capability: (1) add the payload builder in `mapping`,
(2) add unit tests covering both the happy path and edge cases, (3) wire the
capability in `server` (feature-gated), (4) run
`touring-quality score crates/touring-lsp --fail-below 0.80`.

## License

Part of the Touring workspace; see the workspace root for licensing.
