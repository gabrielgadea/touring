# Touring Quality Gates

> The gates the Touring workspace enforces on itself ("a cura está dentro" —
> dogfooding). Each is **deterministic and zero-LLM**, so it runs identically in
> CI, in a pre-commit hook, or by hand. Master Plan tracks: C.W1/C.W2/C.W3.

## The gates

The single reference table. Every gate is deterministic and zero-LLM; the "Wired"
column states the honest truth about *where* it runs today.

| Gate | Command | Enforces | Fail condition | Wired |
|------|---------|----------|----------------|-------|
| **Compile** | `cargo check --workspace` | the whole workspace builds | any crate fails to build | local / pre-commit |
| **Lint** | `cargo clippy --workspace -- -D warnings` | zero clippy warnings (deny-all) | any warning surfaces | local / pre-commit + CI |
| **Missing-docs ratchet** (D.W2.P3.T6) | `cargo rustc -p touring-generator --lib -- -W missing-docs` | touring-generator doc debt only shrinks (baseline **340**, 2026-06-11); touring-hooks façade is already `#![deny(missing_docs)]` | count grows past the baseline | CI (`missing-docs ratchet` step) |
| **No-unwrap (gateway)** | `cargo clippy -p touring-hooks -- -D warnings` (deny is in-source) | the CEG production code never `.unwrap()`s | a non-`#[cfg(test)]` `.unwrap()`/`.expect(...)` appears in `gateway/` (`#![cfg_attr(not(test), deny(clippy::unwrap_used))]`) | local / pre-commit (compile-enforced) |
| **Metrics drift** | `python3 docs/sync_metrics.py --check` | `ARCHITECTURE.md` numbers match the measured workspace | crate count drifts beyond the ~5% tolerance | local / pre-commit |
| **Doc-gen drift** | `python3 docs/gen_reference.py --validate` | `docs/reference/*.md` stay in sync with source of truth | a generated reference doc is stale | local / pre-commit |
| **File-size** | `python3 docs/file_size_gate.py --check` | no `.rs` file bloats; hotspots can only shrink | a non-whitelisted `.rs` exceeds 5,000 LOC, or a whitelisted hotspot grows past its dated cap | local / pre-commit |
| **Wiring integrity** | `python3 docs/wiring_integrity_gate.py --check` | the workspace dependency DAG stays acyclic (no-back-edge) | `touring wiring cycles --min-depth 2` reports `cycle_count > 0` (fail-open if daemon down) | local / pre-commit |
| **Schema drift** | `cargo test -p touring-storage` (migration suite) | the on-disk DB `SCHEMA_VERSION` matches the embedded migrations (C.W2.P3) | a migration is added/removed without bumping `SCHEMA_VERSION`, or the test corpus detects an un-migrated schema | local / pre-commit (test-enforced) |
| **Health-delta** (advisory) | `touring health-delta status` | per-path quality trend — flags files regressing across edits | a path's `regression_streak ≥ 3` (`STREAK_ALERT_THRESHOLD`); advisory like orphan count, fail-open if daemon down | local / advisory |
| **Fuzz GC** (C.W3.P2.T13) | `scripts/fuzz-gc.sh` (→ `fuzz/gc.sh`) | cargo-fuzz corpora stay under the 200MB cap (REGRA #12) | corpora exceed the cap (historic high: 4.3GB) | local weekly cron via `safe-clean.sh incremental` |

## Run them all

```bash
cd ~/.claude/rust
cargo check --workspace                          && \
cargo clippy --workspace -- -D warnings          && \
python3 docs/sync_metrics.py --check             && \
python3 docs/gen_reference.py --validate         && \
python3 docs/file_size_gate.py --check           && \
python3 docs/wiring_integrity_gate.py --check    && \
echo "ALL GATES GREEN"
```

The two compile/test-enforced gates (no-unwrap and schema drift) run as part of the
`cargo` steps above — they have no separate invocation because their failure *is* a
compile or test failure.

## Why these exist

The diagnostic (`docs/2026-06-04-touring-diagnostico-elite-mercado.md`, §3.2)
documented a live regression: `lifecycle.rs` grew from a 168-LOC hub to 19,444 LOC
in 7 weeks **because no gate enforced a size budget**. The file-size gate freezes
the three known hotspots (`cli_handlers.rs`, `lifecycle.rs`, `wiring.rs`) behind a
dated whitelist — each cap can only shrink — and makes *new* bloat impossible.

The metrics/doc-gen gates close the documentation-drift finding (A03): the system
that calls itself "auditable" now generates its own numbers and reference docs
from the index/CLI, so they cannot lie. See `docs/sync_metrics.py` and
`docs/gen_reference.py`.

The wiring-integrity gate is the purest case of "a cura está dentro": Touring
detects dependency cycles for every workspace it indexes, yet never enforced
acyclicity on *itself*. This session cleared a phantom depth-683 back edge (A05 +
a wiring rebuild); the gate now runs `touring wiring cycles --min-depth 2` and
fails the build if the cycle ever returns. It reports orphan count for information
only — failing on orphans is a known false signal (thousands of intentional
feature-gated / generated pub symbols). See `docs/wiring_integrity_gate.py`.

The no-unwrap and schema-drift gates are enforced *in the source itself*: the CEG
carries `#![cfg_attr(not(test), deny(clippy::unwrap_used))]` so a stray production
`.unwrap()` is a clippy error, and the `touring-storage` migration suite refuses to
build a DB whose `SCHEMA_VERSION` drifts from the embedded migrations (C.W2.P3).

The health-delta gate (C.W3.P3.T14) is advisory, mirroring the orphan-count signal:
the daemon tracks a per-file quality trend across edits (`pre_edit`/`post_edit`
deltas) and raises a regression streak once a path degrades `STREAK_ALERT_THRESHOLD`
(=3) times in a row. `touring health-delta status` surfaces those streaks so a human
can intervene before a file rots; it is intentionally non-blocking (a transient dip
during a refactor is normal) and fails open when the daemon is offline. Reset a
path's streak after a deliberate checkpoint with `touring health-delta reset <path>`.

## Wiring it into CI

These gates are platform-agnostic (plain `cargo` + `python3`). Add the "Run them
all" block as a CI step, a pre-push hook, or a `make check` target. No GitHub-only
assumptions — the Touring repo's git is managed manually.

**Since 2026-06-11 they are also wired into `.github/workflows/ci.yml`** (jobs:
`check` = cargo check+clippy, `test` = `cargo test --workspace --lib`, `gates` =
the Python gates above + a root-hygiene check). The workflow activates the moment
the repo is published to GitHub (B-W1). Every gate exits non-zero on failure,
takes no interactive input, and the daemon-dependent steps (`sync_metrics`,
`wiring_integrity_gate`) **fail open** when the touring daemon is unavailable, so
an offline CI runner never breaks on infrastructure rather than on real defects.
Note: CI `test` intentionally runs `--lib` only — the integration test
`touring-server/tests/graph_service_e2e.rs` has a known deterministic hang
(finding 2026-06-11).
