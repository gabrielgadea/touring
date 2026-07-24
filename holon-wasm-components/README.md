# holon-wasm-components

THSF Fase 4+5 — WebAssembly components for the Touring Holonic Symbiosis
Framework.

## What's here

| Path | Purpose |
|---|---|
| `spec-version/` | Proof-of-life component; returns the WIT package version |
| `blast-radius/` | Graph BFS over a pre-serialised reverse adjacency list |
| `quality-gate/` | Anti-pattern density scoring (Rust / Python) |
| `generator-health/` | Health-delta formatter — summary + alerts + metrics with `health_score` |
| `compose/` | WAC composition scripts (e.g. `aggregate.wac`) |
| `scripts/` | `build-all.sh` — one-shot builder for everything above |
| `.holon/manifest.toml` | Provider manifest exposing the four capabilities via `adapter=wasm` |

## Architecture — Option D (2026-04-25)

```
holon-wasm-components/     ← wasm32-wasip2 workspace (4 components)
├── spec-version/
├── blast-radius/
├── quality-gate/
├── generator-health/
├── compose/
└── scripts/

holon-wasm-runner/         ← x86_64-unknown-linux-gnu standalone host crate
└── src/main.rs            ← wasmtime 42 component runner + pretty-print output
```

**Option D**: `runner/` foi extraído como crate host separado
(`holon-wasm-runner/`, target x86_64-unknown-linux-gnu). Isso previne
que o workspace wasm32-wasip2 seja detectado pelo workspace pai
`~/.claude/rust/`. Cada crate builda independentemente — zero
cross-contamination.

## Quick start

```bash
# 1. Install toolchain (once)
rustup target add wasm32-wasip2
cargo install wac-cli --locked         # optional, for composition

# 2. Build WASM components (this workspace)
cd holon-wasm-components
./scripts/build-all.sh

# 3. Build host runner (separate crate)
cd ../holon-wasm-runner
cargo build --release

# 4. Invoke a component via the host runner
./target/release/holon-wasm-runner \
    ../holon-wasm-components/target/wasm32-wasip2/release/holon_spec_version.wasm \
    invoke spec-version '{}'

# 5. Invoke via the holon CLI (transport dispatch)
holon invoke --root . holon-wasm-components spec-version '{}'
```

## WIT contract (stable)

```wit
package holon:core@0.1.0;

interface capabilities {
    list-capabilities: func() -> list<string>;
    invoke: func(request: invoke-request) -> result<invoke-response, invoke-error>;
}

world holon-component { export capabilities; }
```

Full definition: `../crates/touring-wasm/wit/holon-core.wit`.

## Capabilities

### spec-version
Returns the WIT package version string.
```bash
holon-wasm-runner holon_spec_version.wasm invoke spec-version '{}'
```

### blast-radius
BFS reverse-adjacency graph traversal. Input: `{"graph": {...}, "target": "x.rs"}`.
```bash
holon-wasm-runner holon_blast_radius.wasm invoke blast-radius \
  '{"graph":{"a.rs":["b.rs","c.rs"],"b.rs":["c.rs"],"c.rs":[]},"target":"c.rs"}'
```

### quality-gate
Anti-pattern density scoring (unwrap, panic, todo, bare except).
```bash
holon-wasm-runner holon_quality_gate.wasm invoke quality-gate \
  '{"source":"fn main() { x.unwrap(); }","lang":"rust"}'
```

### generator-health
Pure-function formatter for `health_delta` snapshots. Input: counters + per_path.
Output: summary + alerts + metrics with `health_score` in [0, 1.2].
```bash
holon-wasm-runner holon_generator_health.wasm invoke generator-health \
  '{"counters":{"compute_count":20,"regression_count":2,"improvement_count":15,"recovery_count":3,"streak_alert_count":0,"alert_threshold":3},"per_path":[{"file_path":"src/foo.rs","regression_streak":3,"improvement_streak":0},{"file_path":"src/bar.rs","regression_streak":0,"improvement_streak":5}]}'
```

## E2E Tests

```bash
# Full E2E suite (14 tests — all 4 components + error paths)
./tests/e2e_run.sh

# Expected output:
#   14 / 14 tests passed
```

Test coverage:
- `spec-version`: list capabilities, invoke
- `blast-radius`: list, 2-level blast, isolated node
- `quality-gate`: list, clean code, unwrap, panic+todo, Python bare except
- `generator-health`: list, healthy state, critical regression
- Error handling: unknown capability error

## Why WASI 0.2 (not 0.3)

As of 2026-04 the Rust toolchain does not ship a `wasm32-wasip3` target.
Components compile for `wasm32-wasip2` (WASI 0.2 component model) which
is stable. WASI 0.3 async will arrive in a follow-up wave once the Rust
target stabilises.

## Cross-audit fixes applied (2026-04-25)

| Bug | Fix |
|---|---|
| `record_count` dead field in `generator-health` | Removed — was written but never read |
| Deprecated `post_return()` calls in runner | Removed — wasmtime 42 deprecated API |
| Output showed raw byte arrays (`[123, 34, ...]`) | Added `try_unwrap_invoke_response()` + `try_format_error()` helpers — auto-extract + pretty-print |
| Duplicate `[lib]` keys in 4 Cargo.toml files | Rewrote without duplicates |

## Related

- Runtime bench: `~/.claude/tools/holon/benchmarks/bench_d34.sh`
- Session reports:
  - `~/.claude/rust/docs/2026-04-23-thsf-fase4-wave4ab.md`
  - `~/.claude/rust/docs/2026-04-24-thsf-fase4-wave4c.md`
  - `~/.claude/rust/docs/2026-04-24-thsf-fase4-wave4d.md`
  - `~/.claude/rust/docs/2026-04-24-thsf-fase4-final.md`
  - `~/.claude/rust/docs/2026-04-24-thsf-fase5-generator-symbiotic.md`
  - `~/.claude/rust/docs/2026-04-25-thsf-fase4-5-wasm-restructure.md` (this session)
- THSF master plan: `~/.claude/rust/docs/2026-04-23-THSF-master-plan.md`