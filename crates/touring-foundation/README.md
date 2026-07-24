# touring-foundation

> **Foundation crate** of the Touring workspace. All other crates depend on this.
> Status: production. MSRV: 1.80. Wave: W13.1 partial (2026-05-23).

## Purpose

`touring-foundation` provides the **shared substrate** that every Touring crate
needs: error types, configuration, circuit breakers, health events, telemetry
init hooks, the failover coordinator, and primitives that have no business
living in any one downstream crate.

Changes here have **high blast radius** — every crate in the workspace is
affected. Edit with `touring ast blast crates/touring-foundation/src/<file>`
first.

## Public API surface (41 modules)

### Core infrastructure

| Module | Purpose | Re-exports |
|--------|---------|------------|
| `alloc` | Global memory allocator (mimalloc) — MUST be first | — |
| `error` | Unified `TouringError` (thiserror-derived) | `TouringError` |
| `config` | `TouringConfig` runtime configuration + layered loader (W12.4) | `TouringConfig` |
| `types` | Common types: `CILALevel`, `MemoryTier`, `truncate_str` | `CILALevel`, `MemoryTier`, `truncate_str` |
| `schema` | DB schema definitions + migration helpers | — |
| `migration` | Cross-DB consolidation primitives | — |
| `shutdown` | Graceful shutdown coordination | — |

### Reliability / observability

| Module | Purpose |
|--------|---------|
| `health` | Composite health score (`composite_health_score`) |
| `health_events` | Pub/sub for `HealthDeltaEvent` |
| `failover` | `Failover`, `FailoverCoordinator`, retry + circuit-breaker chain |
| `sentinel` | PSI (Pressure Stall Information) integration on Linux |
| `telemetry` | OpenTelemetry / OTLP / tracing init |
| `governor` | Rate limiting + back-pressure |
| `drift` | Quality drift detection + alert levels |
| `feedback` | `FeedbackPattern`, `FeedbackSignal`, `PatternFeedback` |
| `profile` | RAII hotpath instrumentation (`touring profile query/dump`) |
| `activity` | Activity recorder for session reports |

### Crypto / hash / parsing

| Module | Purpose |
|--------|---------|
| `hash` | Blake3 helpers + content-addressed digests |
| `security` | Path canonicalization + traversal-prevention |
| `chunker` | Byte/line/token chunking primitives |
| `char_classes` | Unicode char classification + skip-region detection |

### Conflict + cgm + mvkl + semantic

| Module | Purpose |
|--------|---------|
| `conflict` | Merge conflict detection + resolution |
| `cgm` | Code Graph Model (cross-crate symbol graph) |
| `mvkl` | Multi-version key-value layer (RLM persistence) |
| `semantic` | Definition resolver + source-to-def mapping |
| `embedding` | GPU embedding client (W3.2 boundary) |

### Misc

| Module | Purpose |
|--------|---------|
| `checkpoint` | Session checkpoint snapshots |
| `diagnostic` | RFC-100 diagnostic codes (B-301, B-302, M-5xx, W-1xx) |
| `plugin` | Plugin registry (ProviderPlugin trait) |
| `shared` | Internal building blocks (circuit_breaker, domain_circuit, pool) |
| `rules` | Rule-loading helpers |

## Quick start

```rust
use touring_foundation::{TouringConfig, TouringError};

let cfg = TouringConfig::detect_layered()?;
println!("cache_size = {}", cfg.cache_size);
```

The 4-layer config precedence (W12.4):
1. Hardcoded defaults
2. `/etc/touring/config.toml` (system)
3. `~/.touring/config.toml` (user, written by `touring toolchain init`)
4. `<project>/.touring/touring.toml` (project, written by `touring init-project`)

## Tests

```bash
cargo test -p touring-foundation              # 469 tests pass
cargo llvm-cov -p touring-foundation --json   # ~78% line coverage (W11 baseline)
```

## Blast radius

```bash
touring ast blast crates/touring-foundation/src/<file>
# Most files: blast_radius > 50 (foundation depended on by 13+ crates)
```

Before editing this crate:
1. `touring ast meta <file> --depth summary -j`
2. `touring ast blast <file>`
3. `touring pre-edit` (score ≥ 0.8)
4. Verify changes don't break consumers via `cargo check --workspace`

## License

Same as the workspace root.

## Reference

- Workspace plan: `~/.claude/rust/docs/plans/touring-premium-refactor-2026/`
- W11 coverage measurement: `09-CHANGELOG.md` § `[W11-2026-05-15]`
- W12.4 layered config: `09-CHANGELOG.md` § `[W12.4-2026-05-23]`
- W13.1 partial (this README): `09-CHANGELOG.md` § `[W13.1-partial-2026-05-23]`
- Touring CLI ranks: `~/.claude/rules/touring-cli-index.md`
