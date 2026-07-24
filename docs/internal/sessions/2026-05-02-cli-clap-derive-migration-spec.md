---
title: "CLI clap derive Migration Spec — P3 Strategy 1.3"
date: 2026-05-02
status: COMPLETED (kickoff AND completion 2026-06-11 — same day; spec estimated 4-5 weeks. DAG task_1781176181409460634 W0..W7 all completed; SOAK waived by Gabriel. Final: 48 handlers migrated + completions.rs aggregator (49 subcommands, bash/zsh/fish/powershell/elvish/man — closes Master Plan B-W1.T5); lib tests 877→1,275 (+398), clippy 0, daemon_query payloads byte-identical across all waves; arg_or retained only as common.rs-internal builtin helper per NG5. Executed via 6 delegated touring-engineers + independent V2-V4 verification per wave. Tooling bug found 3×: taco-forge prettyplease format stage corrupts large/proc-macro files — gotcha #49, fix pending in touring-generator.)
priority: P3 (architectural cleanup, no immediate user-facing impact)
effort: XL (~25-30 engineer-days after 2026-06-11 inventory refresh, 6 waves + W7 completions aggregator)
risk: HIGH (touches 88 handler files, 26,631 LOC; regression surface broad)
authors: ["TACO Orchestrator", "Gabriel Gadea (approver)"]
related:
  - "lesson:wave_2026_05_02:decompose_cli_diagnostic_fixes"
  - "tech_debt:cli_clap_derive_migration"
  - "~/.claude/rules/taco-forge-canonical-workflows.md"
---

# CLI clap derive Migration Spec — P3 Strategy 1.3

## Executive Summary

Migrate the **64 CLI handlers** in `crates/touring-server/src/cli/` from manual
positional argument parsing (`arg_or(args, idx, default)`) to `clap` derive
macros. The current pattern is the architectural root cause of an entire class
of CLI bugs — including the **4 root-cause bugs fixed in Wave 2026-05-02**
(`decompose add` comma-magic, single-dep, trailing-comma, `decompose update`
silent no-op).

**Why now**: not now. This spec exists so that when the migration is finally
scheduled, there is a deterministic, low-risk plan. **Do NOT pick this up
opportunistically** — it requires a dedicated wave and a strategy of progressive
rollout to avoid catastrophic regression on the ~5,100-test workspace.

**Why ever**: every handler that uses `arg_or` is a latent CLI bug. Each one
reinvents argument parsing, each one is one typo away from a silent no-op, and
each one needs a hand-written `--help` block. clap derive eliminates the entire
class.

---

## 1. Background

### 1.1 Origin of this spec

Wave 2026-05-02 (Diagnostic Fixes) corrected 4 root-cause bugs in
`crates/touring-server/src/cli/decompose.rs`:

| Issue | Symptom | Root cause |
|---|---|---|
| ISSUE-2 | description with comma corrupted deps | `find(\|a\| ... contains(','))` scanned ALL args |
| ISSUE-3 | single-dep "S-01" became empty | predicate required `.contains(',')` |
| ISSUE-4 | `decompose update --depends-on` silent no-op | CLI never parsed flag; daemon never read field |
| ISSUE-5 | trailing comma "S-01," became empty string | no `.filter(\|s\| !s.is_empty())` |

All 4 are variants of the **same architectural pattern**: manual positional
parsing without a schema. clap derive prevents all 4 by construction:

- Type-safe arg binding (no string-to-type drift)
- `value_delimiter = ','` with built-in trim/filter semantics
- `help` auto-generated per subcommand
- Mutually-exclusive groups via `ArgGroup`
- Required vs optional enforced at parse time, not at runtime

### 1.2 Current state inventory (V2 verified 2026-05-02; **refreshed 2026-06-11 at kickoff**)

| Metric | 2026-05-02 | **2026-06-11 (kickoff)** | Source |
|---|---|---|---|
| Handler files | 64 | **88** | `ls crates/touring-server/src/cli/*.rs` |
| Total LOC | 16,771 | **26,631** | `wc -l crates/touring-server/src/cli/*.rs` |
| Total `arg_or(args, ...)` calls | 128 | **150** | `grep -c "arg_or(args,"` |
| Smallest handler | 20 LOC (`shadow.rs`) | unchanged | observed |
| Largest handler | 735 LOC (`wiring.rs`) | unchanged | observed |
| Workspace edition | 2021 | unchanged | `Cargo.toml` |
| `clap` in Cargo.toml | NOT present | **present**: workspace `clap = "4.5"` (line 435), v4.6.0 already in touring-server dep tree (transitive) | `grep clap Cargo.toml` + `cargo tree` |
| decompose.rs (pilot) | 178 LOC / 18 arg_or / 8 variants | **282 LOC / 10 arg_or / 11 variants** (+templates, +template, +ready --by-priority) | `wc -l` / `grep -c` |

**Kickoff resolutions (2026-06-11)**: Q1 = clap 4.5 workspace-pinned (already present). Q5 = clap
default (accepts both `--flag=value` and `--flag value` — maximizes backwards-compat). Q6 = daemon-side
handlers now live in `crates/touring-cli` (post daemon-lib-rearch) and receive JSON payloads, not argv —
**out of scope**. Q7 = parse from `args[1..]` (`try_parse_from(args.iter().skip(1))`) so clap treats
the subcommand name as program name; passing the full slice would make clap try to match `"decompose"`
as a subcommand variant and fail. New **W7** appended: `touring completions <shell>` aggregator via
`CommandFactory` + clap_complete/clap_mangen (closes Master Plan B-W1.T5) + `arg_or` deletion.
Bug-fix flag adopted in pilot: `--cila-level` on `create` (daemon already reads `cila_level` from the
payload — `crates/touring-cli/src/cli/decompose.rs:25-46` — but the manual parser silently swallowed
the flag into the description; REGRA #0 wiring, not a new feature).

### 1.3 Anatomy of a current handler

```rust
// crates/touring-server/src/cli/decompose.rs (post-2026-05-02 fix)
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let subcommand = arg_or(args, 2, "status");

    match subcommand {
        "create" => {
            let task_type = arg_or(args, 3, "general");
            let tail: Vec<&String> = args.get(4..)
                .map(|s| s.iter().collect()).unwrap_or_default();
            let mut origin = "touring-cli".to_string();
            let mut priority = "normal".to_string();
            let mut desc_parts: Vec<&str> = Vec::new();
            for arg in &tail {
                if let Some(val) = arg.strip_prefix("--origin=") {
                    origin = val.to_string();
                } else if let Some(val) = arg.strip_prefix("--priority=") {
                    priority = val.to_string();
                } else {
                    desc_parts.push(arg.as_str());
                }
            }
            let description = desc_parts.join(" ");
            // ... payload + daemon_query ...
        }
        // ... 7 more variants in same style ...
    }
    Ok(())
}
```

**Pain points**:

1. Every handler reimplements its own positional indexing
2. Every flag is parsed by `strip_prefix` — typos in flag names silently fall to
   the catch-all (description bucket)
3. Mutually exclusive flags are not enforceable
4. `--help` requires hand-written text (drift between behavior and doc)
5. Type coercion (i64, f64) requires manual `parse().ok()` chains
6. CSV parsing reinvented per handler (each with its own bug surface)

### 1.4 Dispatch architecture (preserved by this spec)

`crates/touring-server/src/main.rs:167-210` uses **table-driven dispatch**:

```rust
let table = cli::common::command_table();
if let Some(cmd) = table.iter().find(|c| c.name == subcommand) {
    match (cmd.handler)(&args) { ... }
}
```

Each handler exposes `pub fn run(args: &[String]) -> anyhow::Result<()>`.
**This spec does NOT change the dispatch layer** — it changes only what each
handler does internally with the `args` slice.

This decision (Confidence: 0.95) keeps the change surface bounded:
- `main.rs`, `cli/common.rs::command_table` untouched
- Builtins (`serve`, `--version`, `--help`) untouched
- Hook runtime invocation pattern untouched
- Error policies (`ExitOnError`, `HookSilent`) untouched

---

## 2. Goals and Non-Goals

### 2.1 Goals

| # | Goal | Verifiable by |
|---|---|---|
| G1 | Eliminate manual positional parsing in all 64 handlers | `grep -c "arg_or(args" crates/touring-server/src/cli/` → **0** |
| G2 | Each subcommand auto-generates `--help` from clap derive metadata | `touring decompose add --help` shows clap-formatted output |
| G3 | Type-safe arg binding (no `i64::from_str` boilerplate) | Handlers compile with `priority: i64` directly |
| G4 | CSV parsing centralized via `value_delimiter` | `parse_csv_deps` helper deleted; `Vec<String>` with `value_delimiter = ','` everywhere |
| G5 | Mutually exclusive flags enforced | Parse-time error on `--status=X --abort` (currently silent) |
| G6 | Existing JSON output schema preserved | All ~200 integration tests pass without payload-shape changes |
| G7 | Backwards-compatible CLI surface (no broken invocations) | Pre-migration `update` command syntax `update <task> <sub> <status>` still works |

### 2.2 Non-Goals

| # | Non-goal | Rationale |
|---|---|---|
| NG1 | Change dispatch layer or `command_table` | Out of scope; high risk; no user-visible benefit |
| NG2 | Migrate to async-clap or custom parser | clap 4 derive is sufficient and battle-tested |
| NG3 | Rewrite tests from scratch | Tests assert payload JSON, not CLI parsing — most pass unchanged |
| NG4 | Add new flags or behavior to handlers | This is a refactor, not a feature wave |
| NG5 | Eliminate `arg_or` from non-CLI code paths | `common::arg_or` may stay for non-clap builtins (no caller harm) |

---

## 3. Architecture

### 3.1 Approach selection

Two viable approaches:

#### Approach A — internal clap parser per handler (RECOMMENDED, 0.9)

Each handler declares its own `#[derive(Parser)]` struct, parses inside `run`,
keeps the existing `pub fn run(args: &[String])` signature.

```rust
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "decompose", disable_help_flag = false)]
struct DecomposeCli {
    #[command(subcommand)]
    cmd: DecomposeCmd,
}

#[derive(Subcommand, Debug)]
enum DecomposeCmd {
    /// Create a new task DAG container.
    Create {
        /// Task type (general, refactor, feature, ...)
        task_type: String,
        /// Origin marker for bidirectional flow.
        #[arg(long, default_value = "touring-cli")]
        origin: String,
        /// Priority bucket.
        #[arg(long, default_value = "normal")]
        priority: String,
        /// Free-form description (collected from remaining positional args).
        #[arg(trailing_var_arg = true)]
        description: Vec<String>,
    },
    /// Add a subtask to an existing task DAG.
    Add {
        task_id: String,
        subtask_id: String,
        /// Subtask description.
        description: Vec<String>,
        #[arg(long, default_value = "normal")]
        priority: String,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long = "parallel-group")]
        parallel_group: Option<String>,
        /// Comma-separated dependency list.
        #[arg(long = "depends-on", value_delimiter = ',')]
        depends_on: Vec<String>,
    },
    Update {
        task_id: String,
        subtask_id: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long = "depends-on", value_delimiter = ',')]
        depends_on: Vec<String>,
        #[arg(long)]
        priority: Option<i64>,
        #[arg(long = "quality-score")]
        quality_score: Option<f64>,
        /// Backwards-compat positional status (deprecated; emits warning).
        #[arg(hide = true)]
        legacy_status: Option<String>,
    },
    // ... rest of variants
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    // clap reads from std::env::args by default; we have a slice — use try_parse_from
    let cli = DecomposeCli::try_parse_from(args.iter())?;
    match cli.cmd {
        DecomposeCmd::Create { task_type, origin, priority, description } => {
            let payload = serde_json::json!({
                "task_type": task_type,
                "description": description.join(" "),
                "origin": origin,
                "priority": priority,
            });
            let output = daemon_query("cli-decompose-create", payload)?;
            println!("{output}");
            Ok(())
        }
        DecomposeCmd::Add { task_id, subtask_id, description, priority, deadline,
                            parallel_group, depends_on } => {
            let depends_on: Vec<String> = depends_on.into_iter()
                .filter(|s| !s.is_empty()).collect();
            let mut payload = serde_json::json!({
                "task_id": task_id,
                "subtask_id": subtask_id,
                "description": description.join(" "),
                "depends_on": depends_on,
                "priority": priority,
            });
            if let Some(d) = deadline { payload["deadline"] = serde_json::json!(d); }
            if let Some(g) = parallel_group { payload["parallel_group"] = serde_json::json!(g); }
            let output = daemon_query("cli-decompose-add", payload)?;
            println!("{output}");
            Ok(())
        }
        DecomposeCmd::Update { task_id, subtask_id, status, depends_on, priority,
                               quality_score, legacy_status } => {
            let mut payload = serde_json::json!({
                "task_id": task_id,
                "subtask_id": subtask_id,
            });
            // Honor explicit --status, fall back to legacy positional with warning.
            if let Some(s) = status {
                payload["status"] = serde_json::json!(s);
            } else if let Some(s) = legacy_status {
                eprintln!("warning: positional <status> is deprecated; use --status=<value>");
                payload["status"] = serde_json::json!(s);
            }
            let depends_on: Vec<String> = depends_on.into_iter()
                .filter(|s| !s.is_empty()).collect();
            if !depends_on.is_empty() {
                payload["depends_on"] = serde_json::json!(depends_on);
            }
            if let Some(p) = priority { payload["priority"] = serde_json::json!(p); }
            if let Some(q) = quality_score { payload["quality_score"] = serde_json::json!(q); }
            let output = daemon_query("cli-decompose-update", payload)?;
            println!("{output}");
            Ok(())
        }
        // ... 5 more variants ...
    }
}
```

**Pros**:
- Zero changes to `command_table` or `main.rs`
- Each handler is independently migratable (parallel waves possible)
- Type safety per-handler
- Auto-generated `--help` per subcommand
- `value_delimiter = ','` solves CSV parsing once

**Cons**:
- 64 separate `#[derive(Parser)]` structs (some boilerplate duplication)
- Cannot enforce **global** `--verbose`/`--timeout` via clap (those are parsed
  in `mod.rs` before dispatch — UNCHANGED in this design)

#### Approach B — top-level `Cli` enum (rejected, 0.6)

Single `enum Cli { Decompose(DecomposeCli), Wiring(WiringCli), ... }` parsed
at top level. Replaces `command_table` dispatch.

**Why rejected**:
- Breaks builtins handling (`serve`, `--help`, `--version` need bespoke logic)
- Forces all 64 migrations into a single landing wave (cannot stage)
- Increases binary size by deeper enum nesting
- Conflicts with hook-runtime dispatch which calls handler functions directly
  (not via clap)

### 3.2 Recommended approach

**Approach A** with these shared conventions:

1. Each handler defines its own `<Name>Cli` Parser struct in `<file>.rs`
2. Use `try_parse_from(args.iter())` so the existing `&[String]` slice is consumed
3. Argv `args[0]` is the binary name, `args[1]` is the subcommand — clap handles
   both via `try_parse_from`
4. Subcommands inside a handler use `#[derive(Subcommand)]`
5. CSV-style flags: `#[arg(long, value_delimiter = ',')]` + post-parse
   `.filter(|s| !s.is_empty())`
6. Backwards-compat positionals: `#[arg(hide = true)]` + warning emit
7. Help text in doc-comments (`///`) — auto-rendered by clap

### 3.3 New crate dependency

```toml
# crates/touring-server/Cargo.toml
[dependencies]
clap = { workspace = true, features = ["derive", "env"] }

# Cargo.toml (workspace)
[workspace.dependencies]
clap = "4.5"
```

Confidence on version: 0.85 (clap 4.5 is current as of 2025; check for newer
stable release at migration time).

---

## 4. Migration Strategy — 6 Waves

Sort by **risk × current bug history**: handlers with recent bug fixes go first
to establish the pattern; high-blast-radius handlers go last with extra QA.

### Wave 1 — Pilot (decompose) — 1 day, LOW risk

| Handler | LOC | `arg_or` count | Recent bugs |
|---|---|---|---|
| `decompose.rs` | 178 | 18 | 4 fixed 2026-05-02 ✅ |

- Deliverables:
  - `DecomposeCli` Parser struct + `Subcommand` enum
  - All 8 variants migrated (create, add, get, update, validate, status,
    finalize, ready)
  - `parse_csv_deps` helper deleted
  - 12 new unit tests using `clap::Parser::try_parse_from` directly
- Success criterion: pre-migration integration tests pass; new unit tests cover
  the 4 root-cause bugs + happy path

### Wave 2 — Foundations (4 simple handlers) — 1 day, LOW risk

| Handler | LOC | Variants |
|---|---|---|
| `shadow.rs` | 20 | 1 |
| `init.rs` | ~50 | 1 |
| `session.rs` | 60 | 4 |
| `workflow.rs` | 68 | 2 |

- Establishes the migration boilerplate as a pattern
- Each handler ≤80 LOC — fast iteration

### Wave 3 — Mid-complexity (10 handlers) — 2 days, MEDIUM risk

`memory.rs`, `learning.rs`, `gotcha.rs`, `evolution.rs`, `diary.rs`,
`flywheel.rs`, `health_delta.rs`, `mcts.rs`, `suggest.rs`, `e2e.rs`.

LOC range: 80–220.

### Wave 4 — Higher complexity (10 handlers) — 2 days, MEDIUM-HIGH risk

`tantivy.rs`, `tasksfile.rs`, `cognitive.rs`, `index.rs`, `gate_metrics.rs`,
`cascade.rs`, `assist.rs`, `granularity.rs`, `inferlets.rs`, `jobs.rs`.

LOC range: 220–400.

### Wave 5 — Heaviest (8 handlers) — 3 days, HIGH risk

`wiring.rs` (735 LOC), `synergy.rs` (550), `source_change.rs` (493),
`search_unified.rs` (475), `snapshot.rs` (426), `viz.rs` (310), `ast.rs` (~280),
`generate.rs` (~280).

These handlers have rich subcommand trees + many flag combinations. Recommend:
- Pair-program or solo with mandatory architectural review
- Extra E2E tests for each subcommand variant
- Run `touring e2e --depth standard` before AND after each handler

### Wave 6 — Long tail (~30 handlers) — 3 days, MEDIUM risk

Remaining handlers (50–200 LOC each). Can be parallelized across multiple
engineers if the pattern is well-established by Wave 5.

### Total estimate

| Metric | Value |
|---|---|
| Engineer-days | 12 (Wave 1) + 1 + 2 + 2 + 3 + 3 = **12 days** sequential |
| Calendar weeks | 3 (with QA buffer) |
| Engineers required | 1 with senior Rust + clap experience |
| Reviewer-days | +2 days for cross-audit (touring-auditor agent) |

---

## 5. Backwards Compatibility Matrix

For each handler, document which legacy invocations must continue to work.

### Example: decompose

| Legacy invocation | Status post-migration | Mechanism |
|---|---|---|
| `touring decompose update <task> <sub> <status>` | ✅ works (deprecated) | `legacy_status` positional + warning |
| `touring decompose add <task> <sub> <desc>` | ✅ works | unchanged positionals |
| `touring decompose add <task> <sub> "S-01,S-02"` (legacy comma deps) | ❌ no longer works | already deprecated 2026-05-02; warning was emitted |
| `touring decompose add <task> <sub> <desc> --depends-on=S-01` | ✅ works | new path |
| `touring decompose update <task> <sub> --depends-on=S-01` | ✅ works | new path (also new since 2026-05-02) |

**Policy**: any invocation that worked in **Touring v30.0.0 (2026-05-02)** must
continue to work after the migration, except where the pre-existing behavior
was the bug being fixed (`add <task> <sub> "deps,as,csv"` was already
documented as deprecated with stderr warning).

---

## 6. Testing Strategy

### 6.1 Test categories

| Category | Pre-migration count | Post-migration target |
|---|---|---|
| Unit tests (parsing) | 0 (parser was inline manual code) | 1 per Subcommand variant per handler ≈ **300+ new tests** |
| Integration tests (daemon roundtrip) | ~200 | unchanged (assert JSON payload shape) |
| E2E binary tests (`tests/binary_e2e.rs`) | 30 | +20 (legacy invocation matrix) |

### 6.2 Test patterns

```rust
// Pattern A: parsing-only test (fast, no daemon)
#[test]
fn decompose_add_parses_depends_on_csv() {
    let cli = DecomposeCli::try_parse_from([
        "touring", "decompose", "add", "task_1", "S-01",
        "title", "--depends-on=S-00,S-A,S-B",
    ]).unwrap();
    let DecomposeCmd::Add { depends_on, .. } = cli.cmd else { panic!() };
    assert_eq!(depends_on, vec!["S-00", "S-A", "S-B"]);
}

// Pattern B: backwards-compat smoke test
#[test]
fn decompose_update_accepts_legacy_positional_status() {
    let cli = DecomposeCli::try_parse_from([
        "touring", "decompose", "update", "task_1", "S-01", "completed",
    ]).unwrap();
    let DecomposeCmd::Update { legacy_status, status, .. } = cli.cmd else { panic!() };
    assert_eq!(legacy_status.as_deref(), Some("completed"));
    assert_eq!(status, None);
}

// Pattern C: regression test against root-cause bug
#[test]
fn decompose_add_description_with_comma_does_not_leak_into_deps() {
    let cli = DecomposeCli::try_parse_from([
        "touring", "decompose", "add", "task_1", "S-01",
        "Implement", "foo,", "bar",  // description with comma
    ]).unwrap();
    let DecomposeCmd::Add { description, depends_on, .. } = cli.cmd else { panic!() };
    assert_eq!(description.join(" "), "Implement foo, bar");
    assert!(depends_on.is_empty());  // NO leakage
}
```

### 6.3 Acceptance gate per wave

Each wave PASSES iff:

1. `cargo check --workspace` exit 0
2. `cargo test -p touring-server` ALL tests pass (no skipped)
3. `cargo test -p touring-hooks` ALL daemon-side tests pass
4. New unit tests cover all migrated variants (target ≥90% branch coverage)
5. Backwards-compat tests pass for every legacy invocation in §5
6. `touring e2e --depth standard` returns composite_health_score ≥ baseline
7. cross-audit by `touring-auditor` agent: `vgp_cross_verification` re-runs
   on ≥50% of cited symbols (per REGRA #15 / Wave TRM 2026-05-02)

---

## 7. Risk Assessment

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Silent JSON payload schema drift | MEDIUM (0.6) | HIGH | Snapshot tests on `daemon_query` payloads pre/post; diff in CI |
| Legacy invocations break | MEDIUM (0.5) | HIGH | Per-wave backwards-compat matrix (§5); explicit `legacy_*` fields |
| Hook runtime calls handlers directly (not via main.rs) | LOW (0.3) | MEDIUM | Hook handlers stay in `touring-hooks` crate, not migrated; CLI is a thin shell |
| `--help` output regression confuses operators | LOW (0.4) | LOW | Snapshot test `--help` output; commit clap-derived versions to repo |
| clap 4 minor version churn during migration | LOW (0.2) | LOW | Pin to specific minor version in `[workspace.dependencies]` |
| Build time increase from clap derive macros | LOW (0.3) | LOW | clap is already a transitive dep via other crates; marginal cost |
| Large diff makes review impractical | HIGH (0.8) | MEDIUM | Wave-based PRs (~10 handlers per PR); reviewer fatigue mitigated by smaller chunks |
| Edition 2021 limitations on clap features | LOW (0.2) | LOW | clap 4.5 supports edition 2021; verify before kickoff |

### 7.1 Killer risk: introducing a NEW silent bug

The whole point of this migration is to eliminate silent CLI bugs. If a
migration wave introduces a new one, it would defeat the purpose and erode
trust in the whole tooling stack.

**Mitigation**: Wave 1 (decompose) is a **mandatory dogfooding gate**. The
handler gets ≥1 week of soak time in production with telemetry on every
invocation before Wave 2 starts. Telemetry signals to watch:

- `touring gate-metrics -j | jq '.diagnostic_b302_emitted_count'` — should not
  spike
- `touring evolution drift -j` — alert level should stay `none`
- `touring learning status | jq '.ema_reward'` — should stay ≥ baseline

---

## 8. Effort and Timeline

### 8.1 Person-days

| Wave | Days | Cumulative |
|---|---|---|
| 0 (kickoff: spec + clap deps in Cargo.toml + harness) | 1 | 1 |
| 1 (decompose pilot) | 1 | 2 |
| Soak time (1 week in production) | — | 9 |
| 2 (4 simple handlers) | 1 | 10 |
| 3 (10 mid-complexity) | 2 | 12 |
| 4 (10 higher) | 2 | 14 |
| 5 (8 heaviest) | 3 | 17 |
| 6 (~30 long tail) | 3 | 20 |
| Post-migration cleanup (delete `arg_or`, doc updates) | 1 | **21 days** |

**Calendar**: 4–5 weeks with sequential execution and 1-week soak.

### 8.2 Cost-benefit

**Benefit**:
- Eliminates 100% of `arg_or`-class bugs going forward
- Reduces handler LOC by ~30% (parsing boilerplate eliminated)
- Auto-generated `--help` keeps docs in sync with behavior
- Easier to add new flags (just add field to derive struct)

**Cost**:
- 21 engineer-days
- One full week of soak monitoring after Wave 1
- Some risk of regression on heaviest handlers (Wave 5)

**Recommendation**: schedule for a dedicated quarter. Do NOT bundle with
feature work.

---

## 9. Success Criteria

| # | Criterion | Verification command |
|---|---|---|
| SC1 | Zero `arg_or(args, ...)` calls in `cli/` | `grep -rn "arg_or(args" crates/touring-server/src/cli/ \| wc -l` returns **0** |
| SC2 | All 64 handlers compile with clap derive | `cargo check --workspace` exit 0 |
| SC3 | All pre-migration integration tests pass | `cargo test --workspace` exit 0 |
| SC4 | New unit tests added per variant | `cargo test -p touring-server cli_` count ≥ 300 |
| SC5 | `--help` output snapshot tests in place | `tests/binary_e2e.rs` includes 64 help-snapshot assertions |
| SC6 | `touring e2e --depth standard` health ≥ baseline | `composite_health_score ≥ 0.85` |
| SC7 | Wiring orphan delta ≤ 0 | `touring wiring orphans -j` count not increased |
| SC8 | Memory persisted: lessons + RL rewards | `touring memory recall "wave_p3_1.3_clap_migration"` returns ≥ 6 entries |

---

## 10. Open Questions

| # | Question | Owner | Resolution before |
|---|---|---|---|
| Q1 | Pin clap to 4.5.x or `^4.5`? | architect | kickoff |
| Q2 | Migrate `cli/common.rs::arg_or` away or keep as no-op stub? | architect | Wave 6 |
| Q3 | Should `cli/common.rs::command_table` switch to clap-derived metadata? | architect | post-migration |
| Q4 | How to handle handlers that take **arbitrary** trailing args (e.g., `tantivy search "<query>"`)? | engineer | Wave 4 |
| Q5 | Should we deprecate `--flag value` (space-separated) and force `--flag=value`? | UX | kickoff |
| Q6 | Do hook-runtime handlers (in `touring-hooks/src/cli_handlers_*.rs`) need clap migration too? | architect | Wave 0 |
| Q7 | Will `try_parse_from` consume `args[0]` (binary name) correctly? | engineer | Wave 1 |

### Q7 specifically (worth resolving in spec)

clap's `try_parse_from` expects the first element to be the binary name. The
current `args` slice in handler `run(args)` already includes the binary name
at `args[0]` (it is `std::env::args().collect::<Vec<_>>()` from main.rs).
**Resolution**: pass the slice as-is; clap will treat `args[0]` as program
name and `args[1]` as subcommand. Verified in clap 4.5 docs.

---

## 11. References

| Item | Location |
|---|---|
| Wave 2026-05-02 fix wave (motivating context) | `~/.claude/projects/-home-gabrielgadea/memory/lesson:wave_2026_05_02:decompose_cli_diagnostic_fixes` |
| Tech-debt entry | `~/.claude/projects/-home-gabrielgadea/memory/tech_debt:cli_clap_derive_migration` |
| Current handler — golden reference | `crates/touring-server/src/cli/decompose.rs` (post-Wave 2026-05-02) |
| Daemon-side decompose | `crates/touring-hooks/src/cli_handlers_decompose.rs` |
| Dispatch table | `crates/touring-server/src/cli/common.rs::command_table` |
| main.rs dispatch | `crates/touring-server/src/main.rs:160-210` |
| clap 4 derive book | `https://docs.rs/clap/4/clap/_derive/index.html` |
| Symbol Verification Table (REGRA #15) | `~/.claude/rules/TACO-subagent.md` (CONSTITUTIONAL section) |
| File metadata first | `~/.claude/rules/file-metadata-first.md` |

---

## 12. Approval and Authorization

This spec is **PROPOSED** as of 2026-05-02. Authority to proceed lies with
**Gabriel Gadea** (per CLAUDE.md hierarchy). No work begins on Wave 0 until
Gabriel approves the timeline AND the pilot handler choice.

Recommended kickoff signal:

```bash
touring memory store "decision:wave_p3_1.3_kickoff_2026_QX" \
  '{"approved_by":"Gabriel","start_date":"YYYY-MM-DD","pilot_handler":"decompose","timeline_weeks":4}' \
  --tier semantic
```

After kickoff, each wave emits its own `decision:wave_p3_1.3_wN_complete` entry
with telemetry snapshot.

---

## 13. Appendix — Handler inventory by complexity

```
LOC    File                         arg_or  Variants  Wave
-----  ---------------------------  ------  --------  ----
20     shadow.rs                    1       1         2
50     init.rs                      ~       1         2
60     session.rs                   4       4         2
68     workflow.rs                  2       2         2
80     resolve_def.rs               3       1         3
80     repo_health.rs               4       2         3
92     repo_health.rs               4       2         3
110    suggest.rs                   5       3         3
112    repo_score.rs                5       2         3
122    saga.rs                      6       7         3
131    ssr.rs                       5       3         3
158    [diary,evolution,...]        ~       ~         3
178    decompose.rs                 18      8         1 ✅ WAVE 1
180    tasksfile.rs                 7       4         4
181    status.rs                    6       3         4
220    skip.rs                      6       3         4
266    tantivy.rs                   8       5         4
280    [ast.rs estimated]           ~       ~         5
310    viz.rs                       9       4         5
426    snapshot.rs                  12      6         5
475    search_unified.rs            14      7         5
493    source_change.rs             16      8         5
550    synergy.rs                   18      6         5
735    wiring.rs                    25      10        5
```

(numbers verified 2026-05-02 via `wc -l` and `grep -c "arg_or(args,"`)

---

_End of spec — version 1.0, status: PROPOSED, awaiting Gabriel approval._
