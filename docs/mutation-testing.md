# Mutation Testing — Wave T1+T2 Playbook

> **Status**: Operational since 2026-04-25 | **Wrapper**: `touring mutation-test` (T1) | **CI**: `mutation-baseline` job + `mutants-incremental` job (T2) | **KPI**: `touring.mutation.kill_rate` (R2)

---

## Why mutation testing

Line/branch coverage tells you which lines a test *touched*; mutation testing
tells you which mutations of those lines a test *would have caught*. A
function with 100% line coverage and 0% mutation kill rate is not actually
verified — the tests run the code but never assert on its behavior. Mutation
testing is a **stronger** signal than coverage and complements it as the
"how much does the suite actually verify" metric.

The wrapper exists so the same `touring mutation-test` invocation runs
locally (developer iterating), in CI (advisory weekly job), inside the holon
audit (`run_full_audit.sh`), and feeds back into the executive dashboard
(`touring repo-score` Testing category).

---

## Quickstart

```bash
# Install (one-time, ~3 min)
cargo install cargo-mutants

# Run a single package (fastest path — populates cache)
touring mutation-test --package touring-ast --threshold 70 --timeout 120

# Read cached result without re-running (zero cost)
touring mutation-test --cache-only

# Force re-run (bypass 7-day cache window)
touring mutation-test --package touring-ast --force

# Whole workspace (slow — usually only in scheduled CI)
touring mutation-test --threshold 50

# Inspect the cache file directly
cat ~/.claude/rust/.touring-cache/mutation-test/touring-ast.json
```

---

## Wrapper output schema

```jsonc
{
  "ok": true,
  "cached": false,                  // true when served from disk cache
  "package": "touring-ast",         // null when whole-workspace
  "mutants_total": 100,             // = killed + survived + timeout + unviable
  "mutants_killed": 80,             // detected (test failed) — good
  "mutants_survived": 15,           // missed (no test failed) — gap in coverage
  "mutants_timeout": 3,             // timed out — counts as killed for kill_rate
  "mutants_unviable": 2,            // didn't compile — excluded from denominator
  "kill_rate": 84.69,               // (killed + timeout) / (killed + timeout + survived)
  "elapsed_secs": 142,
  "passed_threshold": true,
  "threshold": 80.0,
  "cargo_mutants_version": "26.1.2"
}
```

Failure envelope (binary missing, parse error, etc.):

```jsonc
{
  "ok": false,
  "error": "cargo-mutants binary not found ...",
  "kind": "binary_not_found"        // also: exit_failed, outcomes_missing, outcomes_parse, io
}
```

---

## Threshold ramp-up

The CI job and the R2 commitment use a deliberately conservative starting
threshold to avoid breaking the gate before the test suite has been
audited for mutation gaps. Bump the threshold in **two** places when each
milestone is reached:

| Phase    | Threshold | Where to update                                                                                     |
|----------|-----------|-----------------------------------------------------------------------------------------------------|
| Week 1-2 | **50%**   | `docs/ci-template.yml` (`mutation-baseline`) + `docs/kpi/commitments.yaml` (`touring.mutation.kill_rate`) |
| Week 3-4 | 65%       | same two files                                                                                      |
| Month 2  | 75%       | same two files                                                                                      |
| Month 3  | **80%**   | same two files — promote `mutation-baseline` from `continue-on-error: true` to required             |

Bumping the threshold is a **deliberate** action — never raise it
implicitly via auto-merge of a "improve mutation kill rate" PR. The R2 KPI
commitment is the canonical declaration; the CI threshold mirrors it.

---

## Architecture (T1 + T2)

```
┌────────────────────────────────────────────────────────────────────┐
│  Developer / CI / holon audit                                      │
│         │                                                          │
│         ▼                                                          │
│  touring mutation-test [--package P] [--threshold T] [--cache-only]│
│         │                                                          │
│         │ (daemon socket — rkyv or JSON)                           │
│         ▼                                                          │
│  cli_handlers_mutation_test::cli_mutation_test                     │
│         │                                                          │
│         │  ─── cache hit?  ──── yes ──► return cached envelope     │
│         │                                                          │
│         ▼ no                                                       │
│  mutation_test::run_mutation_test                                  │
│         │                                                          │
│         ├──► spawns `cargo mutants -p P --output <dir> ...`         │
│         │                                                          │
│         └──► parse_outcomes_json(<dir>/mutants.out/outcomes.json)  │
│                  │                                                 │
│                  ▼                                                 │
│         compute_kill_rate(killed, survived, timeout)               │
│                  │                                                 │
│                  ▼                                                 │
│         cache_store(<workspace>/.touring-cache/mutation-test/...)  │
│                  │                                                 │
│                  ▼                                                 │
│         MutationReport JSON envelope (returned to caller)          │
│                                                                    │
│  ─── consumers ────────────────────────────────────────────────    │
│  • touring repo-score   → Testing category source = mutation_test  │
│  • touring kpi --check  → touring.mutation.kill_rate commitment    │
│  • run_full_audit.sh    → advisory gate (skip if cargo-mutants ⊥)  │
│  • CI artifact          → mutation-report.json + mutants.out/      │
└────────────────────────────────────────────────────────────────────┘
```

---

## CI integration

Two **complementary** jobs run cargo-mutants in different modes:

| Job                      | Trigger              | Scope                          | Duration  | Purpose                                          |
|--------------------------|----------------------|--------------------------------|-----------|--------------------------------------------------|
| `mutants-incremental`    | every PR             | `cargo mutants --in-diff`      | ~10-15min | Catch regressions on changed code                |
| `mutation-baseline` (T2) | weekly cron + manual | full package via wrapper       | up to 60m | Refresh whole-package baseline + populate cache  |

The wrapper job:
- Runs `cargo build --release --bin touring --bin touring-hook` (the wrapper
  needs the binary to exec `cargo mutants` and write the cache).
- Starts the daemon (the wrapper writes to `<ws>/.touring-cache/...`
  via the daemon, not directly).
- Calls `touring mutation-test --package touring-ast --threshold 50 --force`.
- Uploads `mutation-report.json` + the raw `mutants.out/` tree as artifacts.
- `continue-on-error: true` until month 3 (then promote to required).

---

## Local audit gate

`tools/holon/tests/run_full_audit.sh` includes a **skip-aware** gate:

```bash
gate "mutation-test (touring-ast, advisory) [skip if absent]" \
    bash -c '
        command -v cargo-mutants >/dev/null 2>&1 || { echo "skip"; exit 0; }
        command -v touring        >/dev/null 2>&1 || { echo "skip"; exit 0; }
        out=$(touring mutation-test --package touring-ast --threshold 50 --cache-only 2>/dev/null || true)
        # cache_miss or wrapper error → advisory pass
        # passed_threshold=false       → fail
        # passed_threshold=true        → pass
        ...
    '
```

The gate consults `--cache-only` (zero-cost read) instead of running mutation
testing — the **scheduled** CI job is responsible for populating that cache.
Local devs see "skip" on machines without `cargo-mutants`, or "no cache yet"
on a fresh checkout, neither of which blocks the audit.

---

## Cache layout

```
<workspace>/.touring-cache/mutation-test/
├── _workspace.json          ← whole-workspace run (--package omitted)
├── touring-ast.json
├── touring-hooks.json
└── ...
```

- One file per package filter (or `_workspace.json` for unfiltered runs).
- 7-day stale window (configurable via `MutationConfig` — see `mutation_test.rs`).
- Atomic write: tempfile + `fs::rename` (no half-written cache).
- Format: pretty-printed `MutationReport` JSON.

To inspect the most recent run for a package:

```bash
jq . ~/.claude/rust/.touring-cache/mutation-test/touring-ast.json
```

To wipe the cache (force next run to re-execute):

```bash
rm ~/.claude/rust/.touring-cache/mutation-test/touring-ast.json
# or
touring mutation-test --package touring-ast --force
```

---

## Interpreting outcomes

| Field              | Good if…                                                                 |
|--------------------|--------------------------------------------------------------------------|
| `mutants_killed`   | High vs total. Each killed mutant = one bug your test suite *would* catch. |
| `mutants_survived` | Low. Each survivor is a real coverage gap — the test ran but didn't assert. |
| `mutants_timeout`  | Low (≤ 5% of total). High counts mean tests have non-deterministic perf. |
| `mutants_unviable` | Variable. Indicates mutations the compiler rejected — not a quality signal. |
| `kill_rate`        | ≥ threshold. The headline metric; what the gate enforces.                |

A drop in `kill_rate` between baselines is a **regression signal**: either
new code shipped without sufficient tests, or an existing test was weakened
(e.g. an assertion replaced with `assert!(true)`).

---

## Troubleshooting

### "cargo-mutants binary not found"
```
{ "ok": false, "kind": "binary_not_found", "error": "cargo-mutants binary not found ..." }
```
Install: `cargo install cargo-mutants`. The wrapper degrades cleanly — CI
gates are advisory, the daemon never panics.

### "outcomes_missing"
```
{ "ok": false, "kind": "outcomes_missing", "error": "outcomes.json missing at ..." }
```
`cargo-mutants` crashed before writing the artifact. Check
`<output_dir>/mutants.out/log/baseline.log` for the build error — usually
the unmutated baseline fails to compile (workspace already broken).

### "exit_failed"
```
{ "ok": false, "kind": "exit_failed", "error": "cargo-mutants exit failed ..." }
```
`cargo-mutants` itself errored AND `outcomes.json` is absent. This is
distinct from "tests caught a mutant" (which exits non-zero but still
writes outcomes — handled cleanly). Inspect stderr, often a cargo
toolchain issue.

### CI timeout (60min)
- Reduce scope: pass `--package <single_crate>` instead of whole workspace.
- Increase `--timeout` (per-mutant) only if individual test runs are slow;
  do **not** increase the CI `timeout-minutes` past 60 (runner cost).
- Consider sharding: `cargo mutants --shard 1/4` (and run 4 jobs). Wrapper
  doesn't expose `--shard` yet — see follow-up T3.

### Kill rate stuck below threshold
- Inspect surviving mutants: `cat target/mutants/<package>/mutants.out/missed.txt`
- Each line is `path:line:col: replace X with Y` — write a test that
  distinguishes the original from the mutation.

---

## Source map

| Concern                      | File                                                                 |
|------------------------------|----------------------------------------------------------------------|
| Lib (config + parser + cache) | `crates/touring-hooks/src/mutation_test.rs`                          |
| Daemon handler                | `crates/touring-hooks/src/cli_handlers_mutation_test.rs`             |
| CLI subcommand                | `crates/touring-server/src/cli/mutation_test.rs`                     |
| Hook registry wire            | `crates/touring-hooks/src/hook_registry.rs` (`cli-mutation-test`)    |
| R1 Testing category           | `crates/touring-hooks/src/cli_handlers_repo_score.rs::score_testing` |
| R2 KPI commitment             | `docs/kpi/commitments.yaml` (`touring.mutation.kill_rate`)           |
| CI baseline job               | `docs/ci-template.yml` (`mutation-baseline`)                         |
| CI PR diff job                | `docs/ci-template.yml` (`mutants-incremental`)                       |
| Audit gate                    | `tools/holon/tests/run_full_audit.sh` (advisory, skip-aware)         |

---

## Related skills

- `~/.claude/skills/Touring/SKILL.md` — TIER 4 row mentions `touring mutation-test`
- `crates/touring-server/.claude/CLAUDE.md` — workspace conventions
