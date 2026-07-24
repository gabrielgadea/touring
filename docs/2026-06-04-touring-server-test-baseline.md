# touring-server — Test Baseline (A.W1.P2.T3)

> **Date**: 2026-06-04 (baseline) · refreshed 2026-06-06 | **Wave**: A.W1.P2.T3
> **Purpose**: record the compile + test baseline of `touring-server` so future
> regressions are measurable, and retire the stale `ARCHITECTURE.md:771` claim of
> "122 test compile errors" (N03).

## Background

The 2026-06-04 diagnostic (`docs/2026-06-04-touring-diagnostico-elite-mercado.md`)
cited **122 touring-server test compile errors** sourced from a stale
`ARCHITECTURE.md:771` cell. Session 2026-06-05 (C.W2 / §9.1) re-measured in loco:

> `cargo check -p touring-server --tests` → **0 errors** (the 122 figure was stale).

This document is the canonical baseline so the claim is not re-introduced.

## Baseline (measure / refresh commands)

```bash
# Compile the test targets (does not run them) — the N03 gate:
cargo check -p touring-server --tests        # expect: 0 errors

# Count test functions in touring-server:
grep -rcE '^\s*#\[(tokio::)?test\]' crates/touring-server/src crates/touring-server/tests \
  2>/dev/null | awk -F: '{s+=$2} END{print s}'

# Run the suite:
cargo test -p touring-server
```

## Current state (2026-06-06)

| Metric | Value | Source |
|--------|-------|--------|
| `cargo check -p touring-server --tests` | 0 errors | C.W2 §9.1 (2026-06-05), re-confirmed by `cargo check --workspace` exit 0 (A01 wave) |
| Workspace test fns | 13,309 | `docs/sync_metrics.py` (2026-06-06) |
| touring-server crate | member, v30.0.0 | `Cargo.toml` workspace members |

## Regression guard

If `cargo check -p touring-server --tests` ever returns non-zero, that is a
regression against this baseline — investigate before merging. The figure to
trust is the **live `cargo check` output**, never a hand-written count in
`ARCHITECTURE.md` (the original N03 root cause).
