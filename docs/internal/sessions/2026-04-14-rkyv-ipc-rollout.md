# rkyv IPC Rollout — Wave 3

> **Date**: 2026-04-14 | **Owner**: Touring core | **Status**: feature-complete, beta-ready

## Summary

Touring's hook↔daemon protocol now supports two wire formats over the
same Unix socket:

| Format | Default | Trigger | Path |
|---|---|---|---|
| Newline-delimited JSON | ✅ ON | first byte `{` | `serde_json::from_str` |
| rkyv zero-copy frame | OFF | first byte `R` (magic `RKYV`) | `bytecheck::access_safe` |

Both directions (request + response) mirror the inbound format. The daemon
peeks the first byte and dispatches; the CLI parses dual-path on response.

## Build

```bash
# Standard build — rkyv-ipc is a DEFAULT feature since 2026-04-14, no flag needed.
cargo build --release -p touring-server
cargo build --release -p touring-hooks   # daemon + hook binaries

# Opt-out build (rare — only for legacy interop testing) — drop default-features.
cargo build --release --no-default-features --features <minimal-set> -p touring-server
```

## Activation

| Scenario | Action |
|---|---|
| Want rkyv (default state) | nothing — it's on by default in every standard build |
| Hot-disable globally via env var | `export TOURING_RKYV_IPC=0` |
| Per-invocation JSON for one command | `TOURING_RKYV_IPC=0 touring index find Foo` |

## Observability

```bash
touring gate-metrics -j | jq '{
  rkyv_dispatch_count,
  rkyv_dispatch_bytes,
  rkyv_mean_bytes,
  rkyv_parse_error_count,
  rkyv_response_count
}'
```

| Counter | Meaning | Healthy signal |
|---|---|---|
| `rkyv_dispatch_count` | Successful inbound rkyv parses | ↑ when rkyv active |
| `rkyv_dispatch_bytes` | Cumulative body bytes — divide for mean payload | n/a |
| `rkyv_mean_bytes` | Pre-computed mean — sanity check vs JSON sizes | should match `serde_json::to_vec().len()` |
| `rkyv_parse_error_count` | Frames rejected (magic, truncated, bytecheck) | **0** under steady load. >0 = wire bug |
| `rkyv_response_count` | Outbound rkyv responses emitted | matches `dispatch_count` 1:1 |

## Performance baseline (criterion)

Measured on cargo bench `--quick`, single-threaded:

| Operation | Payload | JSON | rkyv | Speedup |
|---|---|---|---|---|
| Serialize | small (60 B) | 750 ns | 181 ns | **4.1×** |
| Parse | small (60 B) | 1.14 µs | 32 ns | **35×** |
| Serialize | large (64 KiB) | 863 µs | 47 µs | **18×** |
| Parse | large (64 KiB) | 1.03 ms | 30 ns | **34 800×** (O(1) zero-copy) |
| Response serialize | 256 KiB CallGraph | *bench D5* | *bench D5* | run locally |

Reproduce: `cargo bench -p touring-rkyv --bench ipc_vs_json`

## Rollback

The rollback path is **always available** at runtime:

```bash
# Hot-disable rkyv without rebuilding/restarting daemon
export TOURING_RKYV_IPC=0
# Subsequent CLI calls emit JSON; daemon's peek-byte dispatch routes JSON.
```

If the rkyv-enabled binary itself is suspect, swap to a JSON-only build
via symlink (no daemon restart needed since only the CLI binary changes):

```bash
ln -sf "$(which touring-json-only)" /usr/local/bin/touring
```

## Beta procedure

1. Build feature-enabled binary in staging:
   `cargo build --release --features rkyv-ipc -p touring-server`
2. Run E2E suite:
   `cargo test -p touring-hooks --features rkyv-ipc --test rkyv_ipc_e2e`
3. Deploy to ONE workspace, monitor `rkyv_parse_error_count` for 24h.
4. If `parse_error_count == 0` and `mean_bytes` looks reasonable → expand.
5. If errors > 0 → set `TOURING_RKYV_IPC=0`, capture sample frames, file bug.

## Files touched (Wave 3)

| File | Lines added | Purpose |
|---|---|---|
| `crates/touring-rkyv/src/ipc.rs` | 211 | `IpcRequest`/`IpcResponse` + framing |
| `crates/touring-rkyv/src/lib.rs` | 9 | Re-exports |
| `crates/touring-rkyv/tests/ipc_roundtrip.rs` | 174 | 10 roundtrip + bytecheck tests |
| `crates/touring-rkyv/benches/ipc_vs_json.rs` | 174 | Criterion benches (D5) |
| `crates/touring-rkyv/Cargo.toml` | 6 | dev-deps + bench harness |
| `crates/touring-hooks/Cargo.toml` | 6 | feature `rkyv-ipc` |
| `crates/touring-hooks/src/daemon.rs` | 130 | peek-byte dispatch + dual response |
| `crates/touring-hooks/src/shared/gate_metrics.rs` | 50 | 5 new counters |
| `crates/touring-hooks/tests/rkyv_ipc_e2e.rs` | 158 | 3 E2E socket tests |
| `crates/touring-server/Cargo.toml` | 7 | feature mirror + opt dep |
| `crates/touring-server/src/cli/mod.rs` | 80 | CLI emit + dual-path response parse + env switch |

**Test count**: 10 unit + 3 E2E = **13 new tests, all passing**.

**Bonus**: pre-existing bugs fixed during integration —
`std::identity` → `std::convert::identity` (online_rl.rs:629),
`Array1::from_slice` → `Array1::from_vec` (linucb.rs:790,797),
FtrlLayer feature-gated update typing (online_rl.rs:347).
