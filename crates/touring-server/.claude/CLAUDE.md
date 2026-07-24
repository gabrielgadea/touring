# touring-server — Crate Instructions

## What this crate does

The `touring` binary — CLI + MCP server + hook runtime. Drives the touring daemon, exposes 86 MCP tools, implements all CLI subcommands, and runs as the touring hooks runtime.

## Default Features (all always-on since 2026-04-17)

All 17 features are in `default`:

| Feature | Purpose |
|---------|---------|
| `wasm-plugins` | WASM plugin runner via touring-wasm |
| `l7b-alpha` | L7-B inferlets, job spawning, health gates |
| `async-memory` | Async pattern clustering via touring-learning |
| `scip-emit` | SCIP index emission for code intelligence |
| `simd-fuzzy` | SIMD fuzzy matching via touring-simd |
| `rl-integration` | RL reward sink via touring-learning |
| `mcts-synthesis` | MCTS synthesis via touring-cognitive |
| `syn-quote` | Syn quote macro for code generation |
| `cognitive-nexus` | Cognitive nexus via touring-cognitive |
| `analysis-gate` | Wiring + quality gates via touring-analysis |
| `nlp-reranking` | NLP reranking via touring-antt |
| `observability` | Tracing telemetry, console, OTLP |
| `memory-integration` | Memory provider for generator |
| `generator-wasm-sandbox` | WASM sandbox adapter for generator |
| `generator-zero-copy` | Zero-copy snapshot via rkyv |
| `ebpf-telemetry` | eBPF syscall telemetry (Linux only) |
| `prod-allocator` | Mimalloc global allocator (touring-core/mimalloc-allocator) |
| `console` | tokio-console instrumentation (port 6669) |
| `otlp` | OpenTelemetry OTLP export |
| `file-logs` | Daily-rotated tracing-appender logs |
| `rkyv-ipc` | rkyv zero-copy IPC (default ON, bypass: TOURING_RKYV_IPC=0) |

## Features NOT in default (intentionally optional)

| Feature | Reason |
|---------|--------|
| `dhat-heap` | Mutually exclusive with `prod-allocator` — use `--no-default-features --features dhat-heap,...` for heap profiling |
| `build-info` | vergen-gix 1.x API breaking change (Emitter + AddEntries dance) — not yet resolved; comment says "will fix in future update" |

## TODO/FIXME (pre-existing, não blocos)

- `build.rs` + line 78-82: `build-info` feature scaffolded but commented out — `vergen-gix` 1.x has breaking API change (Emitter + AddEntries). Not blocking since feature is intentionally off.
- All other TODO/FIXME occurrences in source are test data strings (e.g., "TODO: Add tests for edge cases" in test fixtures) — not implementation TODOs.

## Runtime Model (2026-04-21)

`main.rs` no longer uses `#[tokio::main]`. Instead, a sync `main()` builds
the rayon global pool FIRST, then a tokio multi-thread runtime explicitly,
then dispatches to `async_main()`. Three env vars tune pool sizes without
recompiling:

| Env var | Default | Effect |
|---|---|---|
| `TOURING_MCP_WORKERS` | `num_cpus::get_physical()` | Tokio worker threads (default is physical cores, not logical/SMT — CPU-bound AST parsing + SIMD benefit from 1 worker per physical core because SMT siblings compete for L1/L2 cache) |
| `TOURING_BLOCKING_WORKERS` | 512 | `spawn_blocking` pool cap (SQLite, Tantivy, python3 bootstrap) |
| `TOURING_RAYON_THREADS` | `num_cpus::get_physical() / 2` | Rayon global pool (pre_edit signals, quality analysis) — isolated from tokio workers to prevent cross-pool starvation |

Thread stack size is raised to **4 MiB** (from Rust default 2 MiB) for AST
recursion headroom. Runtime name prefix is `touring-mcp-worker-*` and
`touring-rayon-*` (visible in `tokio-console`, `pidstat -t`, and OTLP
traces).

**Anti-pattern fixed (S1, 2026-04-21)**: `streaming_mcts_search` in
`src/server/tools_infra.rs` previously created a nested
`Builder::new_current_thread()` inside `spawn_blocking` + busy-waited via
`std::hint::spin_loop()`, pinning 1 core at 100% during the 20ms deadline.
Now uses `spawn_blocking` + `std::thread::yield_now()` directly — the
internal rayon pool of `StreamingMCTS` does the multi-core work.

## Boot Behaviour (2026-04-21)

Two recurring boot warnings were silenced:

1. **eBPF init**: `touring-telemetry` now logs at `info!` (was `warn!`)
   when the compiled `.bpf.o` bytecode is absent — this is the expected
   state on workstations. Only real faults (kernel headers, map access)
   stay at `warn!`.
2. **SymbolRefresh**: `src/server/mod.rs` background task now checks
   `script.exists()` before spawning `python3` on
   `<project_root>/scripts/touring_bootstrap_symbols.py` — absent
   script → `debug!` + skip cycle, not `warn!` loop every 30 min.

## How to run tests

```bash
cargo build -p touring-server        # Build binary first (required for binary_e2e tests)
cargo test -p touring-server         # 502 tests (339 unit + 163 integration + binary_e2e)
cargo clippy -p touring-server -- -D warnings  # must be 0
```

**Important**: The binary is named `touring` (not `touring-server`). Tests look for `target/debug/touring` or `target/release/touring`.

## Binary name

`touring` — not `touring-server`. The package name is `touring-server` but the binary is `touring`.

## File layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | Binary entry point |
| `src/cli/` | CLI subcommand implementations |
| `src/server/` | MCP server + tools (86 tools) |
| `src/plugins/` | WASM plugin runner |
| `src/tools/` | Tool implementations (generator_tools, cluster_tools, etc.) |
| `src/telemetry_init.rs` | Observability stack initialization |
| `src/scip_emit.rs` | Feature-gated SCIP emission |
| `src/context_compiler.rs` | Context compression for LLM prompts |
| `build.rs` | Build metadata (vergen-gix, build-info) |
| `tests/binary_e2e.rs` | E2E tests that spawn the touring binary |