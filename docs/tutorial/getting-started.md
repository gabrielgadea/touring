# Getting Started with Touring

> A **tutorial** (Diátaxis): learning-oriented, honest about current maturity.
> Master Plan D.W4.P1. For reference material see `docs/reference/`.

Touring is an **agentic code harness**: it indexes your source, verifies symbols
before code is generated (VGP), sandboxes code execution (CEG X0–X9), and learns
which actions work (RL). This guide takes you from zero to a working index.

## Honest prerequisites

- **Rust toolchain** (stable). Touring is a Rust workspace.
- **~6 minutes to compile** the release build on a warm cache (measured: a full
  `--release` build is ~6 min; a cold build is longer). A prebuilt binary is on
  the roadmap (Master Plan B.W1) — until then you compile from source.
- **Linux or macOS.** The Code Execution Gateway's kernel enforcement
  (landlock) is Linux-only; it degrades *loud*, not silent, elsewhere.
- `python3` (for the bundled `docs/*.py` quality gates).

## 1. Build

```bash
cd ~/.claude/rust
cargo build --release            # ~6 min warm; produces touring, touring-hook, touring-daemon
```

Or, if you manage the canonical symlinks, the one-shot pipeline:

```bash
update-touring                   # build + install symlinks + restart daemon + verify
```

## 2. Verify health

```bash
touring doctor -j                # daemon socket, knowledge DB, circuit breaker, predictor
touring status -j                # symbol count, orphan count, composite_health_score
```

A healthy system reports the daemon components `healthy` and a
`composite_health_score`. If `doctor` shows the daemon down, it auto-spawns on
the next command (or run `touring daemon-ctl status`).

## 3. Index a workspace

```bash
touring index status             # is this workspace known?
touring index rebuild "$PWD"     # build the local symbol index
```

The index is **local** (no cloud round-trip) and is the navigation substrate for
everything else — symbol lookup, wiring/blast analysis, VGP.

## 4. Your first queries

```bash
touring index find <Symbol>              # does this symbol exist? (the VGP primitive)
touring ast meta <file> --depth summary  # blast radius, quality, cognitive score
touring wiring cycles --min-depth 2      # dependency cycles (Tarjan SCC)
touring wiring orphans -j                # pub symbols with no consumers
```

## 5. Measure the workspace (dogfooding)

Touring keeps its own metrics honest with deterministic generators:

```bash
python3 docs/sync_metrics.py             # crates / LOC / test-fns / health
python3 docs/sync_metrics.py --check     # CI gate: fails if ARCHITECTURE.md drifts
python3 docs/gen_reference.py --validate  # CI gate: reference docs in sync
python3 docs/file_size_gate.py --check   # CI gate: no new file-bloat
```

## Where to go next

- **How-to guides** (`docs/how-to/`) — task-oriented recipes (add a language,
  extend the generator, debug the CEG).
- **Explanation** (`docs/explanation/`) — the 4-layer architecture and the *why*.
- **Reference** (`docs/reference/`) — generators, MCP tools, hooks, quality gates.
- **`docs/tutorial/first-hook.md`** — write your first lifecycle hook.

## Known gaps (honesty first)

Touring is maturing from single-user infrastructure toward a multi-model
platform. Today: compilation from source (no prebuilt binary yet), LSP resolution
is syntactic (no cross-file type inference — Master Plan A.W4), and the public
extension contract (RFC-006) is planned. See the diagnostic
(`docs/2026-06-04-touring-diagnostico-elite-mercado.md`) for the full picture.
