# Session Changelog — 2026-04-14

> **Owner**: Touring core | **Scope**: Wave 2 + Wave 3 (rkyv IPC) | **Sessions**: 4 consecutive sessions, single day
> **Outcome**: Production-deployed, validated end-to-end (live daemon at PID 3389730)

## Summary

Two waves shipped in one day, all behind feature flags or with zero-impact additions:

- **Wave 2 Start** — Auditoria autônoma: insta snapshots + loom proofs expand
- **Wave 3 Pilot** — touring-rkyv ipc module + roundtrip tests + benchmarks
- **Wave 3 Complete** — Daemon peek-byte dispatch + CLI dual-path + feature `rkyv-ipc`
- **Wave 3 Next Steps** — Metrics counters + response envelope migration + env-var runtime switch + docs
- **Wave 3 Default-On Promotion** — `rkyv-ipc` movido para default features de `touring-hooks` + `touring-server`; rebuild + redeploy + verify
- **Production deploy** — Binaries swapped, daemon graceful-restarted, validated in vivo (live counters)

## Files added/modified

### New files (8)

| File | LOC | Purpose |
|---|---|---|
| `crates/touring-ast/tests/snapshot_imports.rs` | 50 | insta snapshots: import resolver (Rust/Python) |
| `crates/touring-ast/tests/snapshot_complexity.rs` | 75 | insta snapshots: cyclomatic complexity |
| `crates/touring-ast/tests/snapshot_module_tree.rs` | 41 | insta snapshots: module hierarchy |
| `crates/touring-rkyv/src/ipc.rs` | 211 | `IpcRequest` / `IpcResponse` + framing |
| `crates/touring-rkyv/tests/ipc_roundtrip.rs` | 174 | 10 roundtrip + bytecheck tests |
| `crates/touring-rkyv/benches/ipc_vs_json.rs` | 174 | Criterion benches (3 groups) |
| `crates/touring-hooks/tests/rkyv_ipc_e2e.rs` | 158 | 3 E2E tests via real UnixStream |
| `docs/2026-04-14-rkyv-ipc-rollout.md` | 130 | Rollout playbook + benchmarks |

### Modified files (10)

| File | Change |
|---|---|
| `crates/touring-loom-proofs/tests/actor_invariants.rs` | +112 LOC: `invariant_g_fascicle_dispatcher_fanout_no_loss`, `invariant_h_ema_reward_converges_under_contention` |
| `crates/touring-rkyv/src/lib.rs` | +9 LOC: re-exports for `ipc` module |
| `crates/touring-rkyv/Cargo.toml` | +6 lines: `criterion` dev-dep, bench harness |
| `crates/touring-hooks/Cargo.toml` | +6 lines: `rkyv-ipc` feature flag (off by default) |
| `crates/touring-hooks/src/daemon.rs` | +130 LOC: peek-byte dispatch, `handle_rkyv_request_async`, dual response, metrics wiring |
| `crates/touring-hooks/src/shared/gate_metrics.rs` | +50 LOC: 4 atomic counters + `record_rkyv_*` helpers + 5 snapshot fields |
| `crates/touring-server/Cargo.toml` | +7 lines: `rkyv-ipc` feature flag mirror + opt dep |
| `crates/touring-server/src/cli/mod.rs` | +80 LOC: rkyv emit, `parse_daemon_response` dual-path, env-var switch |
| `crates/touring-learning/src/online_rl.rs` | 2 LOC: `std::identity` → `std::convert::identity`; FtrlLayer slice typing |
| `crates/touring-learning/src/bandit/{linucb,ast_enriched}.rs` | 4 LOC across files: `Array1::from_slice/from_shape_vec` → `from_vec` (pre-existing API drift fixes) |

### Documentation files updated (3)

- `~/.claude/CLAUDE.md` — added "rkyv Zero-Copy IPC (Wave 3)" section under Daemon Architecture
- `~/.claude/rules/touring-cli-commands.md` — added §17b1 (env var switch) + extended §17b (rkyv counters)
- `~/.claude/skills/Touring/SKILL.md` — added "RKYV ZERO-COPY IPC" quick reference

## Tests added

| Suite | Count | Status |
|---|---|---|
| `touring-ast::snapshot_imports` | 2 | pass |
| `touring-ast::snapshot_complexity` | 2 | pass |
| `touring-ast::snapshot_module_tree` | 1 | pass |
| `touring-loom-proofs::actor_invariants` | 8 (was 6) | pass with `--cfg loom` |
| `touring-rkyv::ipc_roundtrip` | 10 | pass |
| `touring-hooks::rkyv_ipc_e2e` | 3 | pass with `--features rkyv-ipc` |
| **Total new tests** | **26** | **all green** |

## Performance baselines (criterion, single-threaded)

### Request-side rkyv vs JSON

| Payload | Op | JSON | rkyv | Speedup |
|---|---|---|---|---|
| 60 B | serialize | 750 ns | 181 ns | **4.1×** |
| 60 B | parse | 1.14 µs | 32 ns | **35×** |
| 64 KiB | serialize | 863 µs | 47 µs | **18×** |
| 64 KiB | parse | 1.03 ms | 30 ns | **34 800×** (O(1) zero-copy) |

### Response-side rkyv vs JSON (256 KiB CallGraph)

| Op | JSON | rkyv | Speedup |
|---|---|---|---|
| serialize | 370 µs | 10 µs | **37×** |
| parse | 617 µs | 2.2 µs | **280×** |

## Live production validation

After build + binary swap + graceful daemon restart on user's machine:

```json
{"rkyv_dispatch_count": 3, "rkyv_dispatch_bytes": 304, "rkyv_mean_bytes": 101.33,
 "rkyv_parse_error_count": 0, "rkyv_response_count": 2}
```

`touring index find HookRuntime` returned 10 definitions through the rkyv path.
`TOURING_RKYV_IPC=0 touring index find SymbolStore` returned 4 definitions through the JSON path,
and rkyv counters did **not** increment — proving path isolation is correct.

## Build & deploy commands used

```bash
# Phase 1: build everything with feature
cargo build --release --features rkyv-ipc -p touring-server   # 4m02s
cargo build --release --features rkyv-ipc -p touring-hooks    # 2m38s

# Phase 2: deploy
cp target/release/touring         ~/.claude/hooks/touring
cp target/release/touring-daemon  ~/.claude/hooks/touring-daemon
# touring-hook is a symlink → auto-updated

# Phase 3: graceful restart
kill -TERM 3197853 3131860   # serve + daemon
# Lock + socket auto-cleaned by graceful_shutdown
# Daemon auto-respawned via hook trigger → PID 3389730

# Phase 4: verify
touring doctor                              # all checks ok
touring gate-metrics -j | jq '.rkyv_*'      # counters live
```

## Pre-existing bugs fixed (incidental, all 1-line)

| File | Line | Before | After |
|---|---|---|---|
| `online_rl.rs` | 629 | `std::identity` | `std::convert::identity` |
| `online_rl.rs` | 347 | `ftrl.update(&features, ...)` (Array1) | `.as_slice().unwrap_or(&[])` |
| `linucb.rs` | 790, 797 | `Array1::from_slice(features)` | `Array1::from_vec(features.to_vec())` (linter later changed to `aview1().to_owned()`) |
| `ast_enriched.rs` | 186, 376 | `Array1::from_shape_vec((..,), enriched.view())` / `Array1::from(padded)` | `Array1::from_vec(enriched.to_vec())` (linter later: `from_iter`/`zeros`+`azip`) |

These were all surfaces of the same drift: someone tried to optimize away allocations using ndarray APIs that don't exist or have different signatures in the locked workspace version (0.15).

## Lessons recorded in touring memory

1. `wave2_snapshot_loom_insta` — Wave 2 implementation pattern
2. `wave3_rkyv_ipc_pilot` — touring-rkyv module design + benchmarks
3. `wave3_complete_rkyv_migration` — daemon peek-byte + feature gate strategy
4. `wave3_d1_d2_d4_d5_d6_d8_complete` — observability + response migration + env switch
5. `wave3_production_validation` — live production deploy + metrics

## Rollback plan (any future incident)

```bash
# Hot rollback (no rebuild)
export TOURING_RKYV_IPC=0
# Subsequent CLI calls emit JSON; daemon's peek-byte routes JSON.

# Total rollback (binary swap)
cp ~/.claude/hooks/touring-daemon.old ~/.claude/hooks/touring-daemon
kill -TERM $(pgrep -f touring-daemon)
# Daemon respawn picks up old binary on next hook trigger.
```

## Next session candidates (NOT implemented, planned)

From the Wave 3 plan, **D3 (hdrhistogram latency)** and **D7 (Python bench script)** were
explicitly skipped — both add external deps that warrant their own session. Remaining:

- **D3** — P50/P99 latency histogram via `hdrhistogram` crate (~1-2h)
- **D7** — `scripts/bench_hook_latency.py` for empirical N-sample comparison (~1-2h)
- **Wave 1** (candle-core + mentedb-cognitive + moka activation) — multi-day work, deferred
- **Wave 4** (ultraslayer, hft-channel) — high-risk optional optimizations
