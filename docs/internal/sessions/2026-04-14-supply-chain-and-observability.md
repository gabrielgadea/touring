# 2026-04-14 — Supply-Chain Governance + Observability Infrastructure

> **Scope**: session report for the supply-chain + observability pass that
> landed `cargo-deny`, `cargo-nextest`, `cargo-llvm-cov`, `cargo-machete`,
> `tokio-console`, OTLP export, `dhat-heap` profiling, loom proofs, workspace
> licensing, and 4 CVE fixes. All four `cargo deny check` gates pass.

## Final State

```
cargo deny check            → advisories ok, bans ok, licenses ok, sources ok
cargo test -p touring-loom-proofs (--cfg loom)  → 3 passed; 0 failed
cargo check --workspace     → Finished 29.28s
cargo check --features "console,otlp" -p touring-server  → Finished 25.38s
./target/release/touring --version  → touring 30.0.0
```

## Motivation

The workspace had grown to 19 crates / 5,154+ tests / 181k LOC without a
supply-chain gate. Quarterly audits surfaced:

- Wasmtime sandbox-escape CVE on aarch64 (RUSTSEC-2026-0096) — critical.
- Plus two other unpatched wasmtime CVEs (RUSTSEC-2026-0085, RUSTSEC-2026-0086).
- rustls-webpki CRL matching bug (RUSTSEC-2026-0049).
- OpenSSL + native-tls present in the graph via reqwest defaults despite
  `rustls` being the intended TLS path.
- 29 duplicate crate versions (approx, bitflags, hashbrown 4 versions, etc).
- 15 workspace crates without `license = "..."` field.
- `loom` attempted inside `touring-hooks` failed due to hyper-util
  transitive conflict.

No local tooling enforced any of this.

## What Landed

### 1. Supply-chain gates

| Gate | Tool | Config |
|---|---|---|
| Advisories | `cargo-deny` | `deny.toml` — 3 documented ignores for unmaintained (bincode, instant, paste) |
| Bans | `cargo-deny` | `multiple-versions = "deny"` + 43 entries in `skip` list with rationale |
| Licenses | `cargo-deny` | MIT/Apache-2.0/BSD/ISC/Zlib allow-list + `[workspace.package] license = "MIT OR Apache-2.0"` |
| Sources | `cargo-deny` | crates.io only; unknown-registry = deny |
| Unused deps | `cargo-machete` | Runs in CI; surfaced 14 crates with unused deps (follow-up) |
| Test runner | `cargo-nextest` | `.config/nextest.toml` — default + ci profiles, test-groups for SQLite serialization |
| Coverage | `cargo-llvm-cov` | CI threshold: **75% line coverage** |

### 2. Observability features (opt-in on touring-server)

| Feature | Purpose | Enablement |
|---|---|---|
| `console` | tokio-console async task inspection | `--features console` + `--cfg tokio_unstable` (set in .cargo/config.toml) |
| `otlp` | OpenTelemetry OTLP span export | `--features otlp` + `OTEL_EXPORTER_OTLP_ENDPOINT` env |
| `dhat-heap` | Heap profiling via dhat-rs | `--no-default-features --features dhat-heap,<...>` (mutually exclusive with prod-allocator) |

Centralized in `touring-server/src/telemetry_init.rs` — feature-composable
`Registry::default().with(fmt_layer).with(console?).with(otlp?)`.

### 3. CVEs eliminated

- `cargo update -p wasmtime` → 42.0.1 → **42.0.2** (3 CVEs fixed)
- `cargo update -p rustls-webpki` → 0.103.9 → **0.103.12** (1 CVE fixed)
- `reqwest` workspace dep → `default-features = false, features = ["json", "http2", "charset", "rustls-tls"]` (6 OpenSSL-chain crates removed from graph)

### 4. Compile-time invariants (`static_assertions`)

| Location | Assertion |
|---|---|
| `touring-simd/src/quantization.rs` | `assert_impl_all!(EmbeddingU4: Send, Sync, Clone)` + `const_assert_eq!(size_of::<EmbeddingU4>(), 40)` |
| `touring-hooks/src/shared/job_registry.rs` | `assert_impl_all!(JobState: Send)` |

### 5. Loom proofs (new isolated crate)

`crates/touring-loom-proofs/` — zero touring deps, 3 invariants proved:
- `invariant_a_concurrent_fetch_add_converges` — atomic counter no-lost-update
- `invariant_b_release_store_publishes_prior_writes` — Release/Acquire publication
- `invariant_c_mutex_protected_map_has_no_lost_update` — DashMap-pattern invariant

Run: `RUSTFLAGS="--cfg loom" cargo test -p touring-loom-proofs --release`

**Deliberately out of scope**: real `ProjectCommand` mpsc flow — loom 0.7's
`mpsc::channel` has destructor panics during model exploration.

## Trade-offs & Known Limitations

| Trade-off | Rationale |
|---|---|
| Loom doesn't model the real tokio channel | loom 0.7 mpsc has destructor panics; we prove the atomic backbone instead |
| `vergen` / build-info feature is scaffolded but disabled | vergen-gix 1.0.9 API (`Emitter + AddEntries`) needs follow-up polish |
| `dhat-heap` requires `--no-default-features` | mimalloc is the workspace default allocator; duplicate `#[global_allocator]` is a compile error (intended safeguard) |
| CI workflow in `docs/ci-template.yml` | local security hook blocks direct writes to `.github/workflows/`; promote manually after review |
| 43 skipped duplicate versions | Resolving each requires bumping transitive deps we don't control; each skip entry carries rationale for quarterly review |
| 14 crates with unused deps (cargo-machete) | follow-up cleanup pass — requires touching 14 Cargo.toml files; no security impact |

## Migration Notes for Consumers

1. **Local dev**: install `cargo-nextest`, `cargo-machete`, `tokio-console`
   via `cargo install --locked`. `cargo-deny` and `cargo-llvm-cov` already
   present.
2. **CI integration**: promote `docs/ci-template.yml` to `.github/workflows/ci.yml`
   after reviewing the injection-safety notes at the top of the file.
3. **Heap profiling session**: `cargo run --release --no-default-features --features "dhat-heap,wasm-plugins,l7b-alpha,scip-emit,simd-fuzzy,rl-integration,syn-quote,ebpf-telemetry" -p touring-server -- serve` then analyze `dhat-heap.json` at https://nnethercote.github.io/dh_view/dh_view.html
4. **tokio-console session**: `cargo run --release --features "console,otlp" -p touring-server -- serve` in one terminal, `tokio-console http://127.0.0.1:6669` in another.
5. **OTLP export**: set `OTEL_EXPORTER_OTLP_ENDPOINT=http://<collector>:4317` and `OTEL_SERVICE_NAME=touring-prod` before launching.

## Follow-ups

1. Re-enable `build-info` / vergen — refine the emitter chain for vergen-gix 1.0.
2. Resolve the 14 machete findings (commit per-crate).
3. Work through the 43 skipped duplicate versions quarterly — drop entries
   as transitive deps converge.
4. Extend loom proofs as the daemon actor evolves.
