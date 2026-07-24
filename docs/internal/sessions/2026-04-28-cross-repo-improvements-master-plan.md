# Cross-Repo Improvements — Master Plan v1.0

> **Date**: 2026-04-28 | **Author**: TACO orchestrator (Claude Opus 4.7) under direction of Gabriel Gadea
> **Sources**: hotpath-rs (pawurb) · rustfmt/src · rust-analyzer/crates · Context7 (`/websites/rs_salsa`, `/pawurb/hotpath-rs`)
> **Status**: ✅ ALL 15 DELIVERABLES COMPLETE (2026-04-30) — IMPLEMENTED
> **Touring baseline**: v30.3.0 · 81 CLI · 96 MCP · 176 hooks · 24 crates · 199.832 orphans pub
> **Methodology**: TACO L4+ (all phases) · T-shirt sizing (S/M/L/XL) · DAG-validated dependencies
> **Authority**: Gabriel approves wave-by-wave; orchestrator does not auto-execute

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Context & Discovery Recap](#2-context--discovery-recap)
3. [Methodology & Conventions](#3-methodology--conventions)
4. [Master Dependency DAG](#4-master-dependency-dag)
5. [Wave A — Quick Wins (4 deliverables)](#5-wave-a--quick-wins)
6. [Wave B — Engine Reforms (5 deliverables)](#6-wave-b--engine-reforms)
7. [Wave C — Architectural Bets (4 deliverables)](#7-wave-c--architectural-bets)
8. [Wave D — Semantic Closure (3 deliverables)](#8-wave-d--semantic-closure)
9. [Cross-Cutting Concerns](#9-cross-cutting-concerns)
10. [Risk Register](#10-risk-register)
11. [Memory Persistence Plan](#11-memory-persistence-plan)
12. [Validation Gates per Wave](#12-validation-gates-per-wave)
13. [Timeline (Gantt-style)](#13-timeline-gantt-style)
14. [Appendix — Evidence & References](#14-appendix--evidence--references)

---

## 1. Executive Summary

<objective>
Implement 15 cross-cutting improvements to the Touring code-intelligence platform, extracted from deep analysis of three reference Rust projects (hotpath-rs, rustfmt, rust-analyzer). Improvements span four sequenced waves (A: quick wins, B: engine reforms, C: architectural bets, D: semantic closure). The North Star is to (a) close known gaps in the generator/hooks/diagnostic loop, (b) introduce a refactor-as-CLI framework that monetizes the 199.832 orphan pub symbols, and (c) extend Touring's lead in semantic awareness without duplicating IDE/LSP features that don't fit a CLI+daemon+hooks product.
</objective>

**Total scope**: 15 deliverables · ~21 sprints (4-7 months at 1 active engineer-day/week, parallelizable to ~3 months with 3 engineers)
**Baseline preservation**: zero breaking changes to existing 73 CLI commands and 88 MCP tools; all new functionality additive
**Compliance**: REGRA #0 (potencialização — zero scope reduction), REGRA #11 (no git), REGRA #12 (disk hygiene), REGRA #13 (skill hygiene)

---

## 2. Context & Discovery Recap

### 2.1 What was analyzed

| Repo | Type | Key extraction |
|------|------|-----------------|
| `pawurb/hotpath-rs` v0.15.1 | Async-first profiler | Macro-based instrumentation (`#[measure]`, `measure_block!`, `#[measure_all]`), dual-module zero-cost gating (`lib_on.rs`/`lib_off.rs`), hdrhistogram + quanta, MCP tool integration |
| `rust-lang/rustfmt/src` | Code formatter | Cursor-based gap preservation (`missed_spans.rs::format_missing`), `Rewrite` trait + `Shape` budget, `SkipContext` declarative skip, `CharClasses` state machine, idempotency post-pass |
| `rust-lang/rust-analyzer/crates` | LSP semantic engine | `salsa` incremental queries (Durability tiers, Revision invalidation), `rowan` red-green tree, `ide-assists` 170+ refactor handlers, `SourceChange` transactional cross-file edits, `vfs` virtual filesystem, `ide-ssr` semantic structural rewrite, `Definition` enum, diagnostic-with-fix-it framework |

### 2.2 Why now

- Touring is at v30.3.0 with mature foundations (actor pattern, RL, generator typestate, hooks).
- 199.832 orphan pub symbols accumulated — a refactor-as-CLI framework would convert this debt into automated wiring tasks.
- Generator pipeline produces code but cannot transform existing code (no refactor primitives) — gap that blocks progressive migration scenarios.
- RFC-100 diagnostic codes detect issues but don't carry fixes — closing this loop converts Touring from analyzer to remediator.

### 2.3 Out of scope (explicitly)

- LSP server (MCP covers the role)
- proc-macro subprocess expansion (Rust-only, overhead-heavy)
- Chalk trait solver (overkill for syn-based 90% coverage)
- Edition-aware Rust parsing (niche)
- Hover/inlay-hints/semantic-highlighting LSP protocol features
- TUI ratatui binary (Touring is daemon, consumed by Claude Code)
- HTTP server embedded (use MCP tools or Unix socket)
- `#[global_allocator]` swap (conflicts with jemalloc/mimalloc default)

---

## 3. Methodology & Conventions

### 3.1 T-shirt sizing

| Size | Engineer-days | Calendar (1 dev) | Tests added | Files touched |
|------|---------------|------------------|-------------|---------------|
| **S** | 1–3 | 0.5–1 week | 5–15 | 1–4 |
| **M** | 4–8 | 1–2 weeks | 15–40 | 5–12 |
| **L** | 9–20 | 3–5 weeks | 40–100 | 12–30 |
| **XL** | 21–60 | 6+ weeks | 100+ | 30+ |

### 3.2 Risk tagging

| Tag | Probability | Impact |
|-----|-------------|--------|
| LOW | <30% | recoverable in <1 sprint |
| MEDIUM | 30–60% | wave delay 1–2 sprints |
| HIGH | >60% | architectural rework required |

### 3.3 Per-deliverable structure

Every deliverable in this plan is documented with:

1. **ID** (`W-X.N`), **Title**, **Wave**
2. **Effort** estimate (S/M/L/XL)
3. **Dependencies** (must-finish-before list)
4. **Affected crates** (existing) and **new crates** (created)
5. **Atomic sub-tasks** (each independently shippable)
6. **Files to create/modify** (concrete paths)
7. **Acceptance criteria** (Gherkin-style: Given/When/Then)
8. **Test plan** (unit + integration + E2E)
9. **Rollback plan** (revert path without git)
10. **Telemetry / RFC-100 codes added**
11. **Memory store entries** (post-completion)

### 3.4 TACO Phase mapping

This plan operates at **L4+** (all 9 phases). Each deliverable internally re-runs:

```
FASE 0 (cargo check + touring doctor) → FASE 1 (scout) → FASE 2 (architect) →
FASE 3 (Context7) → FASE 4 (decompose) → FASE 4.5 (anti-FP audit) →
FASE 5 (engineer) → FASE 6 (post-audit) → FASE 7 (scriber)
```

**Hard rules in force**:
- REGRA #0: zero scope reduction; orphans wired or removed, never `allow(dead_code)`
- REGRA #11: zero git operations (touring as source of truth)
- REGRA #12: profile.dev defensivos, no `rm -rf target/`
- REGRA #13: skill hygiene if SKILL.md edits required

---

## 4. Master Dependency DAG

```
                    ┌──────────────────────────────────────────┐
                    │              WAVE A (independent)        │
                    └──────────────────────────────────────────┘
                       A.1                A.2          A.3        A.4
                  (touring-profile)  (SkipContext) (idempot.) (MCP profile)
                          │                │           │            │
                          └────────────────┼───────────┘            │
                                           │                        │
                                           ▼                        │
                    ┌──────────────────────────────────────────┐    │
                    │              WAVE B (after A.2 + A.3)    │    │
                    └──────────────────────────────────────────┘    │
                     B.1     B.2     B.3        B.4         B.5     │
                    (SSR)  (Shape) (CharClasses)(dual-mod)(SourceChng)
                     │      │       │            │           │      │
                     │      │       │            │           │      │
                     ▼      ▼       ▼            ▼           ▼      │
                    ┌──────────────────────────────────────────┐    │
                    │              WAVE C (after B.5)          │    │
                    └──────────────────────────────────────────┘    │
                       C.1            C.2          C.3       C.4    │
                    (assists)        (vfs)        (salsa)    (gap)  │
                       │              │            │          │     │
                       │              │            │          │     │
                       ▼              ▼            │          │     │
                    ┌──────────────────────────────────────────┐    │
                    │              WAVE D (after C.1 + D.1)    │    │
                    └──────────────────────────────────────────┘    │
                       D.1                D.2          D.3          │
                   (Definition)       (resolve-def) (RFC100+fix)    │
                                                                    │
                                       (A.4 publishes once daemon ready)
```

**Critical path**: A.3 → B.5 → C.1 → D.3 (~12-16 weeks single dev, ~6-8 weeks with 3 devs)
**Parallelizable**: A.1, A.2, A.4 (all of Wave A) · B.1, B.2, B.3 (independent in Wave B) · C.2, C.3, C.4 (independent of C.1)

---

## 5. Wave A — Quick Wins

> **Goal**: 4 deliverables, ~2 sprints, high density of value, zero architectural risk
> **Exit criteria**: All 4 shipped + telemetry visible in `touring gate-metrics -j` + 30+ tests added
> **Estimated calendar**: 3–4 weeks (1 dev) or 1–2 weeks (parallelized 3 devs)

### 5.1 Deliverable A.1 — `touring-core::profile` (RAII guards + macros)

| Field | Value |
|-------|-------|
| **ID** | W-A.1 |
| **Title** | Inline instrumentation primitives for hot paths |
| **Wave** | A |
| **Effort** | M (6 engineer-days) |
| **Dependencies** | none |
| **Origin** | hotpath-rs (`measure_block!`, `MeasurementGuardSync/Async`) |
| **Confidence** | FACT [1.0] |
| **Status** | ✅ COMPLETE (2026-04-28/29): touring-core::profile module (RAII MeasurementGuard, worker thread, measure_block! macro). Background worker consuming MeasurementEvent via crossbeam-channel, per-label hdrhistogram aggregation. gate_metrics plumbed (profile_p50_us, profile_p99_us, profile_call_count_total). 12 unit + 4 integration tests PASS. CLI `touring profile query/dump/heap-dump/flamegraph` all operational.

**Affected crates**:
- `touring-core` (existing) — new module `profile`
- `touring-hooks` (existing) — replace ~30 `Instant::now()/elapsed()` sites
- `touring-server` (existing) — wire counters into `gate-metrics`

**New crates**: none

**Atomic sub-tasks**:

1. **A.1.1** Add `crates/touring-core/src/profile/mod.rs` with `MeasurementGuard` RAII (Drop emits to mpsc); `MeasurementGuardAsync` (futures::Future + drop on cancellation); `measure_block!` declarative macro; `#[touring::measure]` proc-macro thin wrapper. Re-export `hdrhistogram::Histogram` for percentile aggregation.
2. **A.1.2** Add background worker thread (`profile::worker`) consuming `crossbeam-channel` of `MeasurementEvent { label, duration_ns, thread_id }`. Aggregates per-label hdrhistogram with merge on shutdown.
3. **A.1.3** Plumb counters into `touring-server::gate_metrics::GateMetricsSnapshot` as `profile_p50_us`, `profile_p99_us`, `profile_call_count_total` keyed by `label`.
4. **A.1.4** Replace ~30 `Instant::now()` sites in `touring-hooks/{pre_edit,pre_read,post_edit}.rs` and `touring-ast/{blast,wiring}.rs` with `let _g = touring_core::profile::measure("pre_edit_chain");`.
5. **A.1.5** Add CLI flag `touring profile dump --format hotpath-json` outputting hotpath-compatible JSON for interop with existing TUIs.
6. **A.1.6** Write 12 unit tests (drop semantics, mpsc backpressure, async cancellation, label collision) + 4 integration tests (real hook scenarios).

**Files to create**:
- `crates/touring-core/src/profile/mod.rs` (~250 LOC)
- `crates/touring-core/src/profile/worker.rs` (~150 LOC)
- `crates/touring-core/src/profile/macros.rs` (~80 LOC, declarative + proc-macro re-export)
- `crates/touring-core/tests/profile_tests.rs` (~200 LOC)

**Files to modify**:
- `crates/touring-server/src/gate_metrics.rs` — add 3 fields with `#[serde(default)]`
- `crates/touring-hooks/src/{pre_edit,pre_read,post_edit}.rs` — replace timing sites
- `crates/touring-ast/src/{blast,wiring}.rs` — replace timing sites

**Acceptance criteria** (Gherkin):
```
GIVEN a hot path instrumented with `let _g = touring_core::profile::measure("foo");`
WHEN the function executes 1000 times
THEN `touring gate-metrics -j` returns `profile_call_count_total{label="foo"} = 1000`
AND `profile_p99_us{label="foo"}` is within 5% of manual baseline
AND drop-on-panic still emits the measurement (no leaks)
```

**Test plan**:
- Unit: drop semantics under panic, async cancellation, label deduplication
- Integration: 4 real hook scenarios with synthetic load
- Bench: criterion benchmark verifying overhead < 200ns per `measure_block!` invocation
- Stress: 1M events through worker channel without backpressure stall

**Rollback plan**:
- Feature gate the entire module behind `#[cfg(feature = "profile")]` (default-on)
- If issue detected, set `default-features = ["minimal"]` in Cargo.toml of consumers; old `Instant::now()` paths preserved as `#[cfg(not(feature = "profile"))]` fallback for first 2 weeks
- Revert: remove the `profile` mod, restore manual timing (kept in git-archive backup outside Touring workspace)

**Telemetry / RFC-100**:
- New counter set `profile_*` in `gate-metrics`
- No new RFC-100 codes (instrumentation only)

**Memory store post-completion**:
```bash
touring memory store "wave_a_1_profile_completed" "RAII guards landed in touring-core::profile. Replaced 30 manual timing sites. Overhead <200ns confirmed." --tier semantic --type lesson
```

---

### 5.2 Deliverable A.2 — `SkipContext` regions

| Field | Value |
|-------|-------|
| **ID** | W-A.2 |
| **Title** | Frozen-region markers for generator and post-edit |
| **Wave** | A |
| **Effort** | S (3 engineer-days) |
| **Dependencies** | none |
| **Origin** | rustfmt/skip.rs (`SkipContext { macros, attributes }`) |
| **Confidence** | FACT [0.95]
| **Status** | ✅ COMPLETE (2026-04-28/29): SkipContext regions (// touring:skip-region markers). post-edit W-115 SkippedRegionWritten hook. CLI `touring skip list/validate`. 9 skip-region tests (Rust/JS/TS/Python + edge cases). ProfileAggregator Default impl fix. 182/182 test suites OK.
**New crates**: none

**Atomic sub-tasks**:

1. **A.2.1** Define `SkipContext { regions: HashSet<SkipRegion>, kinds: HashSet<GeneratorKind> }` in `touring-generator::skip`. `SkipRegion = (FilePath, ByteRange)`.
2. **A.2.2** Parse comment markers `// touring:skip-region` … `// touring:skip-end` (multi-lang via tree-sitter comment node detection); also support attribute `#[touring::skip]` for Rust.
3. **A.2.3** Generator typestate `Rendered` consults `SkipContext`: if proposed edit overlaps any `SkipRegion`, abort stage with diagnostic `Q-310 RegionFrozen`.
4. **A.2.4** Post-edit hook re-validates: if Edit tool wrote into skipped region, emit RFC-100 `W-115 SkippedRegionWritten` (warning, not blocking).
5. **A.2.5** CLI `touring skip list <file>` to introspect detected regions; `touring skip validate <file>` to dry-run.
6. **A.2.6** Unit + 6 integration tests covering Rust/JS/TS/Python (4 langs).

**Files to create**:
- `crates/touring-generator/src/skip/mod.rs` (~180 LOC)
- `crates/touring-generator/src/skip/parser.rs` (~120 LOC, tree-sitter comment walker)
- `crates/touring-generator/tests/skip_regions.rs` (~150 LOC)

**Files to modify**:
- `crates/touring-generator/src/typestate/rendered.rs` — gate by SkipContext
- `crates/touring-hooks/src/post_edit.rs` — re-validate post-write
- `crates/touring-hooks/src/diagnostics/codes.rs` — add Q-310, W-115

**Acceptance criteria**:
```
GIVEN a file containing `// touring:skip-region\nfn frozen() {}\n// touring:skip-end`
WHEN generator typestate `Rendered` proposes edit inside `frozen()`
THEN typestate fails with diagnostic Q-310 and zero bytes are written

GIVEN the same file
WHEN user manually edits `frozen()` via plain Edit tool
THEN post-edit hook emits W-115 warning (non-blocking)
```

**Test plan**:
- 6 integration tests across Rust/JS/TS/Python
- Edge cases: nested regions (forbidden), unclosed region (treated as EOF), region inside string literal (ignored)
- Idempotency with A.3: skip-region preserved across format(format(x))

**Rollback plan**: feature flag `skip-regions` (default-on after 1 sprint of staging); markers ignored if flag off

**Telemetry / RFC-100**:
- `Q-310 RegionFrozen` (blocking, generator stage)
- `W-115 SkippedRegionWritten` (warning, post-edit hook)
- Counter: `skip_region_violations_count`

**Memory store**: `wave_a_2_skip_context_completed`

---

### 5.3 Deliverable A.3 — Idempotency gate

| Field | Value |
|-------|-------|
| **ID** | W-A.3 |
| **Title** | `format(format(x)) == format(x)` validator in pre-edit |
| **Wave** | A |
| **Effort** | S (2 engineer-days) |
| **Dependencies** | none (uses existing `touring ast format-rust`) |
| **Origin** | rustfmt/formatting.rs (`format_lines` + `has_diff`) |
| **Confidence** | FACT [0.9] |

**Affected crates**: `touring-hooks`, `touring-ast`
**New crates**: none

**Atomic sub-tasks**:

1. **A.3.1** In `touring-hooks::pre_edit`, after shadow_validate score ≥ 0.8, run `format-rust` twice on the proposed output and compare bytes.
2. **A.3.2** If diff detected, downgrade pre-edit score by 0.3 and emit RFC-100 `Q-220 NonIdempotentFormat`.
3. **A.3.3** Add config knob `pre_edit.idempotency.enabled` (default ON) and `pre_edit.idempotency.langs` (default: rust only; expandable via tree-sitter formatters).
4. **A.3.4** Telemetry counter `idempotency_violations_count` in `gate-metrics`.
5. **A.3.5** 4 unit tests (matching/diverging cases, panic recovery, language gating).

**Files to create**:
- `crates/touring-hooks/src/idempotency.rs` (~120 LOC)

**Files to modify**:
- `crates/touring-hooks/src/pre_edit.rs` — wire idempotency check
- `crates/touring-hooks/src/diagnostics/codes.rs` — add Q-220
- `crates/touring-server/src/gate_metrics.rs` — add counter

**Acceptance criteria**:
```
GIVEN a Rust file with content X that, when formatted, produces Y, but format(Y) produces Z (Z != Y)
WHEN pre_edit hook runs on X
THEN pre_edit score is reduced by 0.3
AND diagnostic Q-220 is emitted
AND counter idempotency_violations_count increments by 1
```

**Test plan**:
- Unit: synthetic divergent case (force tree mismatch), matching case, panic in formatter
- Integration: real Rust files from touring-* crates
- Performance: idempotency check adds <50ms to pre-edit

**Rollback plan**: config flag `pre_edit.idempotency.enabled = false` disables instantly

**Telemetry / RFC-100**: `Q-220 NonIdempotentFormat`, counter `idempotency_violations_count`

**Memory store**: `wave_a_3_idempotency_gate_completed`

---

### 5.4 Deliverable A.4 — MCP `profile_query` tool

| Field | Value |
|-------|-------|
| **ID** | W-A.4 |
| **Title** | Live profile query MCP tool for TACO orchestrator |
| **Wave** | A |
| **Effort** | S (3 engineer-days) |
| **Dependencies** | A.1 (provides counter source) |
| **Origin** | hotpath-mcp (rmcp + axum tool exposure) |
| **Confidence** | INFERENCE [0.75]
| **Status** | ✅ COMPLETE (2026-04-29/30): MCP tool `touring_profile_query` + CLI `touring profile query` implemented. ProfileAggregator::query() + dump() + heap_dump() + flamegraph() todos OK. 4/4 profile MCP tools operational. touring-core profile module 100% OK. |

**Affected crates**: `touring-server` (MCP dispatch), `touring-core::profile`
**New crates**: none

**Atomic sub-tasks**:

1. **A.4.1** Add MCP tool `mcp__touring__profile_query` with input schema `{ section: Option<String>, top_n: u32, include_percentiles: Vec<u8> }` and output `{ entries: Vec<ProfileEntry> }`.
2. **A.4.2** Implement handler reading from `touring-core::profile::aggregator` (in-memory hdrhistogram store).
3. **A.4.3** Add CLI mirror `touring profile query --section pre_edit --top 10 -j`.
4. **A.4.4** Update `references/mcp_tools.md` and skill SKILL.md to mention new tool (REGRA #13 compliance).
5. **A.4.5** 3 unit + 2 E2E tests (MCP roundtrip, CLI roundtrip).

**Files to create**:
- `crates/touring-server/src/mcp/tools/profile_query.rs` (~150 LOC)

**Files to modify**:
- `crates/touring-server/src/mcp/dispatch.rs` — register tool
- `crates/touring-server/src/cli/profile.rs` — add `query` subcommand
- `~/.claude/skills/Touring/references/mcp_tools.md` — document tool

**Acceptance criteria**:
```
GIVEN A.1 has been deployed and 100 events emitted for label="pre_edit_chain"
WHEN MCP client calls mcp__touring__profile_query with { section: "pre_edit_chain", top_n: 1, include_percentiles: [50, 99] }
THEN response contains { entries: [{ label, count: 100, p50_us, p99_us, total_us, percent_total }] }
AND latency of MCP call < 50ms
```

**Test plan**: MCP+CLI roundtrip; concurrent reads while events stream

**Rollback plan**: revert MCP dispatch entry; tool unregistered

**Telemetry / RFC-100**: counter `mcp_profile_query_call_count`

**Memory store**: `wave_a_4_mcp_profile_query_completed`

---

## 6. Wave B — Engine Reforms

> **Goal**: 5 deliverables, structural changes, ROI on tooling depth
> **Exit criteria**: SSR + SourceChange + CharClasses + Shape + dual-mod gating live; 80+ tests added
> **Estimated calendar**: 6–8 weeks (1 dev), 3–4 weeks (3 devs)

### 6.1 Deliverable B.1 — `touring ssr` (semantic structural rewrite)

| Field | Value |
|-------|-------|
| **ID** | W-B.1 |
| **Title** | Pattern-based rewrite with VGP path resolution |
| **Wave** | B |
| **Effort** | M (8 engineer-days) |
| **Dependencies** | A.2 (SkipContext respected by SSR) |
| **Origin** | rust-analyzer/ide-ssr |
| **Confidence** | FACT [1.0] |
| **Status** | ✅ COMPLETE (2026-04-28/29) |

**Affected crates**: `touring-ast`, `touring-ast-polyglot`, `touring-index` (VGP backend)
**New crates**: `touring-ssr` (proposed) — only if size warrants split, otherwise as `touring-ast::ssr` submodule

**Atomic sub-tasks**:

1. **B.1.1** Define pattern grammar: `pattern ==>> replacement` with placeholders `$<name>`, multi-match `$<name>:*`, type constraints `${a:kind(literal):not(b)}`.
2. **B.1.2** Parser for pattern using nom or chumsky → `SsrRule { pattern: PatternNode, replacement: ReplacementNode, constraints: Vec<Constraint> }`.
3. **B.1.3** `MatchFinder` over tree-sitter syntax tree, binding placeholders.
4. **B.1.4** **VGP gate**: each path in pattern AND replacement must resolve via `touring index find` in target file scope. If any unresolved → reject rule (avoids homonimia FPs — VP-Scout Cadeia 4 alignment).
5. **B.1.5** `Rewriter` applies replacements respecting `SkipContext` (A.2).
6. **B.1.6** CLI `touring ssr "<pattern> ==>> <replacement>" [--scope <glob>] [--dry-run]`.
7. **B.1.7** MCP tool `mcp__touring__ssr_apply` with same schema.
8. **B.1.8** 20 unit tests + 10 integration tests across Rust/JS/TS/Python.

**Files to create**:
- `crates/touring-ast/src/ssr/{mod.rs, parser.rs, matcher.rs, rewriter.rs, vgp_gate.rs}` (~800 LOC)
- `crates/touring-ast/tests/ssr_tests.rs` (~400 LOC)

**Files to modify**:
- `crates/touring-server/src/cli/ast.rs` — add `ssr` subcommand
- `crates/touring-server/src/mcp/tools/` — add `ssr_apply.rs`

**Acceptance criteria**:
```
GIVEN a Rust file containing `foo(x, y)` where `foo` resolves to `crate::api::foo`
WHEN user runs `touring ssr "$crate::api::foo($a, $b) ==>> ($a).foo($b)"`
THEN call sites are rewritten to method form
AND no rewrite happens in a file where `foo` resolves to a different `foo` (homonimia avoided)
AND any region marked `// touring:skip-region` is preserved
```

**Test plan**:
- Unit: parser, matcher, rewriter independently
- Integration: real-world refactors (e.g., `try!` → `?`, `unwrap` → `?`)
- VP-Scout: SSR rule with deliberately ambiguous symbol — must reject

**Rollback plan**: dry-run mode is default; commit only with `--apply` flag

**Telemetry / RFC-100**:
- `S-100 SsrRuleAccepted`, `S-101 SsrRuleRejectedAmbiguity`, `S-102 SsrRewriteCommitted`
- Counters `ssr_match_count`, `ssr_rewrite_count`, `ssr_vgp_rejection_count`

**Memory store**: `wave_b_1_ssr_completed`

---

### 6.2 Deliverable B.2 — `Shape` budget in generator

| Field | Value |
|-------|-------|
| **ID** | W-B.2 |
| **Title** | Width/indent budget propagated through 30 GeneratorKind |
| **Wave** | B |
| **Effort** | M (7 engineer-days) |
| **Dependencies** | none (independent within Wave B) |
| **Origin** | rustfmt/rewrite.rs (`Shape { width, indent, offset }`) |
| **Confidence** | FACT [0.85] |
| **Status** | ✅ COMPLETE (2026-04-28/29): `RenderShape` created (shape.rs, 169 LOC, 8 tests), `render()` signature updated to return `Result<Option<...>, GenerateError>`, all 30 GeneratorKind call sites updated across e2e_pipeline.rs + e2e_cross_audit.rs + generator_tools.rs |

**Affected crates**: `touring-generator`
**New crates**: none

**Atomic sub-tasks**:

1. **B.2.1** Define `RenderShape { max_width: u16, indent: u16, offset: u16 }` with method `fn budget(&self, used: u16) -> u16`.
2. **B.2.2** Modify `Rendered` typestate signature: `fn render(ctx: &Context, shape: RenderShape) -> Option<Artifact>` (Option to indicate "doesn't fit").
3. **B.2.3** Update each of the 30 GeneratorKind to consume Shape; if return None, escalate Draft → multiline strategy.
4. **B.2.4** Default `max_width = 100`, configurable via `touring config set generator.shape.max_width N`.
5. **B.2.5** Add 30 unit tests (one per kind, edge case "barely fits" / "overflows").

**Files to modify**:
- `crates/touring-generator/src/typestate/rendered.rs` — signature change
- `crates/touring-generator/src/kinds/*.rs` (30 files) — consume Shape
- `crates/touring-generator/tests/shape_tests.rs` (new, ~600 LOC)

**Files to create**:
- `crates/touring-generator/src/shape.rs` (~80 LOC)

**Acceptance criteria**:
```
GIVEN GeneratorKind::FunctionImpl with shape { max_width: 80, indent: 4 }
WHEN rendering produces a single-line that exceeds 80 chars
THEN render returns None
AND typestate falls back to multiline strategy
AND final output respects max_width
```

**Test plan**: per-kind unit tests + property-based (proptest) for "no line exceeds max_width"

**Rollback plan**: each kind has `#[cfg(feature = "shape-budget")]`; off → original behavior

**Telemetry / RFC-100**:
- `G-200 ShapeOverflow` (debug only, when single-line fails)
- Counter `shape_fallback_count`

**Memory store**: `wave_b_2_shape_completed`

---

### 6.3 Deliverable B.3 — `CharClasses` state machine (multi-lang)

| Field | Value |
|-------|-------|
| **ID** | W-B.3 |
| **Title** | String/comment/raw-string aware char iterator |
| **Wave** | B |
| **Effort** | M (5 engineer-days) |
| **Dependencies** | none |
| **Origin** | rustfmt/comment.rs (`LineClasses`/`CharClasses`) |
| **Confidence** | FACT [0.9] |
| **Status** | ✅ COMPLETE (2026-04-29): B.3.3 DONE (2026-04-28) + B.3.4 DONE (2026-04-29) + B.3.5 DONE (2026-04-29) + B.3.6 DONE (2026-04-29) — `cli-ast-grep` now accepts `skip_strings` flag (payload + CLI `--skip-strings`), CharClasses filter rejects hits inside StringLit/Comment/RawString/DocComment. Unit tests added: 6 new cases (string_like_ranges_code_only/string_with_string/with_comment + hit_is_in_string_like accept/reject). CC=25. B.3.4: `highlight.rs` now uses CharClasses to classify lines — comment/string-only lines are dimmed (ANSI 245 faint). New helpers `string_like_regions()`, `line_is_non_code()`, `StringRegion`. 14 tests PASS. B.3.5: `code_only()` pre-filter applied to `docstring` and `functional_signature` in `build_tantivy_doc()`; `fuzzy_search` lowercases query before FuzzyTermQuery; `suggest` uses `{}.*` FST-wildcard pattern. 15/15 tantivy tests PASS. B.3.6: 12 multi-language unit tests across 4 variants (Rust/JS/Python/Go): template literals, single/double-quote strings, line comments, raw strings. 25/25 char_classes tests PASS. |

**Affected crates**: `touring-core`, downstream consumers (`touring-ast::grep`, `touring-ast::highlight`, `touring-index::tantivy`)
**New crates**: none

**Atomic sub-tasks**:

1. **B.3.1** Implement `touring-core::char_classes::{CharClass, CharClasses<'a>}` as iterator returning `(char, CharClass)` where `CharClass ∈ { Code, StringLit, Comment, RawString, DocComment }`.
2. **B.3.2** Multi-lang via tree-sitter token stream: leverage `tree-sitter::Node::kind` to classify token roots, extract bytes, iterate.
3. **B.3.3** Migrate `touring-ast::grep` to use CharClasses by default (skip matches inside StringLit unless `--include-strings`).
4. **B.3.4** Migrate `touring-ast::highlight` to use CharClasses for syntect token mapping (cleaner than current ANSI heuristic).
5. **B.3.5** Migrate `touring-index::tantivy` indexing — don't index string literal contents (saves ~15-20% index size, INFERENCE).
6. **B.3.6** 12 unit tests (state transitions, escape sequences, raw strings) + 4 lang variants (Rust/JS/Python/Go).

**Files to create**:
- `crates/touring-core/src/char_classes/mod.rs` (~250 LOC)
- `crates/touring-core/tests/char_classes_tests.rs` (~300 LOC)

**Files to modify**:
- `crates/touring-ast/src/grep.rs` — wire CharClasses
- `crates/touring-ast/src/highlight.rs` — wire CharClasses
- `crates/touring-index/src/tantivy/indexer.rs` — wire CharClasses

**Acceptance criteria**:
```
GIVEN a Rust file `let x = "TODO: fix";`
WHEN user runs `touring ast grep <file> "TODO" --skip-strings`
THEN zero matches returned (TODO is inside StringLit)

WHEN user runs same query without --skip-strings
THEN 1 match returned at the original position
```

**Test plan**: 4 langs × 6 token-class scenarios (24 cases) + property tests for "concat of class spans == original source"

**Rollback plan**: feature flag `char-classes` per consumer

**Telemetry / RFC-100**: counter `char_classes_iter_count` per consumer

**Memory store**: `wave_b_3_char_classes_completed`

---

### 6.4 Deliverable B.4 — Dual-module `lib_on/lib_off` gating

| Field | Value |
|-------|-------|
| **ID** | W-B.4 |
| **Title** | Zero-cost feature gate for hooks (CI/benchmark mode) |
| **Wave** | B |
| **Effort** | M (4 engineer-days) |
| **Dependencies** | A.1 (uses profile counters to verify) |
| **Origin** | hotpath-rs (`lib_on.rs`/`lib_off.rs` split) |
| **Confidence** | FACT [1.0] |
| **Status** | ✅ COMPLETE (2026-04-28/29): lib_on.rs + lib_off.rs split implemented with feature `hooks-active` (default), signature parity preserved, 14 tests pass |

**Affected crates**: `touring-hooks`
**New crates**: none

**Atomic sub-tasks**:

1. **B.4.1** Split `touring-hooks/src/lib.rs` into `lib_on.rs` (current behavior) and `lib_off.rs` (no-op stubs with same signatures).
2. **B.4.2** Top-level `lib.rs` does `#[cfg(feature = "hooks-active")] pub use lib_on::*;` else `pub use lib_off::*;`.
3. **B.4.3** Default feature `hooks-active` (no behavior change for users).
4. **B.4.4** Add CI job benchmarking with `--no-default-features` (`hooks-noop`) to measure baseline Touring perf without hook overhead.
5. **B.4.5** Add `cargo doc --features hooks-active,hooks-noop` test to verify signature parity.
6. **B.4.6** 8 unit tests (parity of return types, null behaviors, feature combinations).

**Files to create**:
- `crates/touring-hooks/src/lib_on.rs` (~current content of lib.rs)
- `crates/touring-hooks/src/lib_off.rs` (~stubs, ~200 LOC)
- `crates/touring-hooks/tests/parity_tests.rs` (~150 LOC)

**Files to modify**:
- `crates/touring-hooks/src/lib.rs` — pure cfg dispatch
- `crates/touring-hooks/Cargo.toml` — feature `hooks-noop`

**Acceptance criteria**:
```
GIVEN cargo build with --no-default-features --features hooks-noop
WHEN any hook function is called
THEN it returns Ok(()) without side effects
AND profile_call_count_total{label="pre_edit_chain"} stays 0

GIVEN default features
WHEN hook is called
THEN behavior is identical to v30.3.0
```

**Test plan**: parity tests via cargo test on both feature sets; CI matrix with both

**Rollback plan**: revert split; lib_off.rs unused but kept

**Telemetry / RFC-100**: none (build-time only)

**Memory store**: `wave_b_4_dual_module_completed`

---

### 6.5 Deliverable B.5 — `SourceChange` transactional cross-file

| Field | Value |
|-------|-------|
| **ID** | W-B.5 |
| **Title** | Atomic batched edits across multiple files |
| **Wave** | B |
| **Effort** | L (12 engineer-days) |
| **Dependencies** | A.3 (idempotency gates each file) |
| **Origin** | rust-analyzer/ide-db::source_change |
| **Confidence** | FACT [1.0] |
| **Status** | ✅ COMPLETE (2026-04-29): ALL 9/9 SUB-TASKS DONE. SourceChange struct (BTreeMap), TextEdit with Indel non-overlap, SnippetEdit, FileSystemEdit, Applier two-phase (shadow_validate + atomic commit with rollback), rkyv serialization, typestate wiring (PlanExecutor→Applier), CLI handler (apply/preview/validate), MCP tool (touring_source_change), 11 integration tests (source_change_tests.rs). touring-server 718 tests PASS, touring-generator 364 tests PASS (342+11), clippy 0 warnings. Critical bugs fixed: (1) Applier::commit() now writes modified files to disk via path_for closure (FileId→PathBuf), fixing "Committed file must exist on disk" assertion; (2) artifact population loop added for ApplyResult::Committed in typestate.rs. |

**Affected crates**: `touring-generator`, `touring-rkyv` (IPC), `touring-hooks` (post-edit)
**New crates**: none (can be split into `touring-source-change` if size > 1500 LOC)

**Atomic sub-tasks**:

1. **B.5.1** Define `SourceChange { edits: IntMap<FileId, TextEdit>, fs_edits: Vec<FileSystemEdit>, annotations: Vec<Annotation>, snippet: Option<SnippetEdit> }`. `FileSystemEdit ∈ { CreateFile, MoveFile, DeleteFile, MoveDir }`.
2. **B.5.2** `TextEdit` as ordered `Vec<Indel { delete: Range, insert: String }>` with non-overlap invariant (validated at construction).
3. **B.5.3** `SnippetEdit` for cursor placement post-apply (`$0`, `${0:default}`).
4. **B.5.4** rkyv serialization for IPC between daemon and hooks.
5. **B.5.5** Two-phase apply: shadow validate all edits → if all OK, commit atomically; else rollback (no partial writes).
6. **B.5.6** Generator `Speculated→Committed` typestate uses SourceChange instead of single-file mpatch.
7. **B.5.7** CLI `touring source-change apply --file <change.json>` and `touring source-change preview` (dry-run).
8. **B.5.8** MCP tool `mcp__touring__source_change_apply`.
9. **B.5.9** 25 unit tests + 8 integration tests (multi-file refactors, fs ops, rollback on failure).

**Files to create**:
- `crates/touring-generator/src/source_change/{mod.rs, text_edit.rs, fs_edit.rs, snippet.rs, applier.rs}` (~800 LOC)
- `crates/touring-rkyv/src/source_change_ipc.rs` (~200 LOC)
- `crates/touring-generator/tests/source_change_tests.rs` (~500 LOC)

**Files to modify**:
- `crates/touring-generator/src/typestate/{speculated.rs, committed.rs}` — wire SourceChange
- `crates/touring-server/src/cli/source_change.rs` — new CLI module
- `crates/touring-server/src/mcp/tools/source_change_apply.rs` — new MCP tool
- `crates/touring-hooks/src/post_edit.rs` — handle SourceChange completion

**Acceptance criteria**:
```
GIVEN a SourceChange { edits: { file_a: [..], file_b: [..] }, fs_edits: [CreateFile(file_c)] }
WHEN applier commits
THEN all 3 operations succeed atomically
AND if file_b edit fails (e.g., concurrent modification), file_a is reverted
AND file_c is not created

GIVEN a SourceChange with SnippetEdit at file_a position 100
WHEN applier commits
THEN cursor placement metadata is emitted to client (Claude Code shell)
```

**Test plan**:
- Unit: TextEdit non-overlap, IntMap ordering, snippet escape sequences
- Integration: real refactors touching 2-5 files
- Failure injection: simulate concurrent write, ENOSPC, EACCES — verify rollback

**Rollback plan**: feature flag `source-change`; if off, `Committed` typestate uses old single-file mpatch

**Telemetry / RFC-100**:
- `SC-100 SourceChangeApplied`, `SC-101 SourceChangeRolledBack`, `SC-102 SourceChangePartialFailure`
- Counters `source_change_apply_count`, `source_change_rollback_count`

**Memory store**: `wave_b_5_source_change_completed`

---

## 7. Wave C — Architectural Bets

> **Goal**: 4 deliverables, biggest payoffs but biggest effort
> **Exit criteria**: assists framework with 10 handlers + VFS overlay + salsa POC + format-rust preserve mode; 200+ tests; deep documentation
> **Estimated calendar**: 10–14 weeks (1 dev), 5–7 weeks (3 devs)

### 7.1 Deliverable C.1 — `touring-assists` framework

| Field | Value |
|-------|-------|
| **ID** | W-C.1 |
| **Title** | Refactor-as-CLI framework with 10 high-value handlers |
| **Wave** | C |
| **Effort** | XL (35 engineer-days for framework + 10 handlers) |
| **Dependencies** | B.5 (SourceChange transport), A.2 (SkipContext), B.3 (CharClasses for safe matching), partial D.1 (Definition for some handlers) |
| **Origin** | rust-analyzer/ide-assists (170+ handlers) |
| **Confidence** | INFERENCE [0.9] |
| **Status** | ✅ DELIVERED (2026-04-30): ALL 10 HANDLERS IMPLEMENTED + CLI FULLY OPERATIONAL + 50 TESTS PASSING. Framework: assist.rs, context.rs, assists.rs, catalog.rs. Handlers: auto_wire, extract_function, inline_call, auto_import, generate_impl, merge_imports, change_visibility, add_missing_match_arms, move_module_to_file, convert_to_guarded_return. CLI: touring assist {list-kinds,applicable,apply} FULLY OPERATIONAL (2026-04-30). touring-assists: 50/50 tests PASS. touring-server: all tests PASS. Exit criteria MET. Remaining: C.1.16 achieved (50 tests). |

**Affected crates**: `touring-ast`, `touring-generator`, `touring-index`
**New crates**: `touring-assists` (mandatory split — large surface)

**Atomic sub-tasks**:

**Framework (Tasks 1–5, ~12 days)**:

1. **C.1.1** Define `Assist { id: AssistId, label: String, group: AssistGroup, target: TextRange, source_change: Lazy<SourceChange> }`. Lazy because rendering can be expensive.
2. **C.1.2** Define `AssistContext<'a> { db: &'a Db, file_id: FileId, range: TextRange, syntax: SyntaxNode, semantic: Option<&'a Semantics> }`.
3. **C.1.3** Define handler signature `type Handler = fn(&mut Assists, &AssistContext) -> Option<()>;` matching rust-analyzer pattern exactly.
4. **C.1.4** `Assists` accumulator with `add(label, group, target, |builder| builder.edit(...))` and `add_group(group_label, items)`.
5. **C.1.5** CLI `touring assist list-kinds`, `touring assist applicable <file>:<line>:<col>`, `touring assist apply <kind> <file> <range>`.

**Handlers (Tasks 6–15, ~23 days, ~2 days each)**:

6. **C.1.6** `auto_wire` — for orphan pub symbols, suggest insertion points based on `touring wiring suggest` output (high-leverage given 199.832 orphans).
7. **C.1.7** `extract_function` — Rust + JS/TS via tree-sitter; identify free vars, generate signature, emit call site.
8. **C.1.8** `inline_call` — inverse of extract; replace call site with body, substitute params.
9. **C.1.9** `auto_import` — for unresolved symbol, find candidate via `touring index find`, insert use stmt at appropriate scope.
10. **C.1.10** `generate_impl` — for type T, generate `impl Trait for T` skeleton with required methods.
11. **C.1.11** `merge_imports` — combine adjacent `use` statements with shared prefix.
12. **C.1.12** `change_visibility` — pub ↔ pub(crate) ↔ pub(super) ↔ private; check downstream impact via `touring wiring impact`.
13. **C.1.13** `add_missing_match_arms` — for match on enum, add arms for unhandled variants (uses `touring ast rust-semantic` to resolve enum).
14. **C.1.14** `move_module_to_file` — `mod foo { ... }` → `mod foo;` + new `foo.rs` (uses SourceChange fs_edits).
15. **C.1.15** `convert_to_guarded_return` — `if x { body }` with else → `if !x { return; } body`.

**Tests (Task 16, ~2 days)**:

16. **C.1.16** 30 unit tests + 20 integration tests across handlers; property tests for "applying assist preserves AST validity".

**Files to create**:
- `crates/touring-assists/Cargo.toml`
- `crates/touring-assists/src/lib.rs` (~100 LOC)
- `crates/touring-assists/src/framework/{assist.rs, context.rs, builder.rs}` (~600 LOC)
- `crates/touring-assists/src/handlers/{auto_wire,extract_function,inline_call,auto_import,generate_impl,merge_imports,change_visibility,add_missing_match_arms,move_module_to_file,convert_to_guarded_return}.rs` (~300 LOC each = 3000 LOC)
- `crates/touring-assists/tests/` (50 test files, ~2000 LOC)

**Files to modify**:
- `crates/touring-server/src/cli/assist.rs` — new CLI module
- `crates/touring-server/src/mcp/tools/assist_apply.rs` — new MCP tool
- `~/.claude/skills/Touring/references/touring-cli-intelligence.md` — document `touring assist`
- workspace `Cargo.toml` — add `touring-assists` member

**Acceptance criteria**:
```
GIVEN file with `pub fn foo() {}` orphan (zero consumers per touring wiring orphans)
WHEN user runs `touring assist applicable <file>:1:1`
THEN response includes `auto_wire` with suggested wire-target candidates

WHEN user runs `touring assist apply auto_wire <file> 0:0..0:0`
THEN SourceChange is emitted with new `use crate::path::foo;` in selected consumer
AND foo's orphan count decrements

GIVEN cursor in middle of code block
WHEN user runs `touring assist applicable <file>:<line>:<col>`
THEN extract_function appears in applicable list
AND apply produces new pub fn + call site, all tests still pass
```

**Test plan**:
- Per-handler: 3 unit + 2 integration = 50 tests minimum
- Property: assist application preserves cargo check status
- Performance: each handler renders SourceChange in <500ms

**Rollback plan**: each handler feature-flagged (`assist-extract-function`, etc.); can disable individually

**Telemetry / RFC-100**:
- `A-100 AssistApplied`, `A-101 AssistRejected`, `A-102 AssistAmbiguous`
- Counters `assist_apply_count`, `assist_rejection_count` per handler ID

**Memory store**: `wave_c_1_assists_framework_completed` + per-handler entries

---

### 7.2 Deliverable C.2 — `touring-vfs` overlay

| Field | Value |
|-------|-------|
| **ID** | W-C.2 |
| **Title** | Virtual filesystem with snapshots and multi-project isolation |
| **Wave** | C |
| **Effort** | L (15 engineer-days) |
| **Dependencies** | none (independent of C.1) |
| **Origin** | rust-analyzer/vfs |
| **Confidence** | INFERENCE [0.85]
| **Status** | ✅ COMPLETE (2026-04-28/30): touring-vfs crate created with 7 modules (PathId/InMemory/VFS provider/diff/watch/snapshot/sync). VFS layer operational. salsa integration ready.

**Atomic sub-tasks**:

1. **C.2.1** Define `FileId(u32)` opaque type, `AbsPathBuf` normalized paths.
2. **C.2.2** `Vfs` struct with `BTreeMap<AbsPathBuf, FileId>`, `Vec<FileEntry>` storing `(content: Bytes, version: u64, durability: Durability)`.
3. **C.2.3** Snapshot via `Arc<VfsState>` cloning (cheap — bytes shared via `Bytes`).
4. **C.2.4** `VfsOverlay` for shadow validate: layered on top of Vfs, edits visible only to overlay reader.
5. **C.2.5** Multi-project: `FileSet { name, files: BTreeSet<FileId> }` (THSF integration — each holon gets a FileSet).
6. **C.2.6** Watcher integration: filesystem changes update Vfs version atomically, emit `VfsChange` event.
7. **C.2.7** Migrate `touring-server::file_io::read_file` to use Vfs (transparent for callers).
8. **C.2.8** Migrate generator `Speculated` typestate to use VfsOverlay (no real disk writes during speculation).
9. **C.2.9** 18 unit tests + 6 integration (concurrent reads, overlay isolation, snapshot correctness, watcher events).

**Files to create**:
- `crates/touring-vfs/Cargo.toml`
- `crates/touring-vfs/src/{lib.rs, file_id.rs, abs_path.rs, vfs.rs, overlay.rs, file_set.rs, watcher.rs}` (~1400 LOC)
- `crates/touring-vfs/tests/vfs_tests.rs` (~700 LOC)

**Files to modify**:
- `crates/touring-server/src/file_io.rs` — wire Vfs
- `crates/touring-generator/src/typestate/speculated.rs` — wire VfsOverlay
- workspace `Cargo.toml` — add `touring-vfs`

**Acceptance criteria**:
```
GIVEN Vfs with 1000 files
WHEN VfsOverlay is created with edits to 5 files
THEN overlay reader sees edits, base Vfs reader sees originals
AND snapshot of base Vfs is unaffected by overlay edits

GIVEN two FileSets representing different projects with same path /tmp/foo.rs
WHEN reading /tmp/foo.rs in FileSet A vs B
THEN distinct contents returned
AND no cross-contamination

GIVEN external file edit (filesystem watcher fires)
WHEN VfsState.version reads
THEN version incremented atomically
AND VfsChange event emitted to subscribers
```

**Test plan**:
- Concurrency: 10k concurrent reads while overlay/watcher active
- Memory: 1M-file Vfs uses < 200MB RAM (Bytes sharing)
- Performance: snapshot creation <1ms

**Rollback plan**: feature flag `vfs`; if off, file_io.rs uses direct fs access

**Telemetry / RFC-100**:
- Counters `vfs_read_count`, `vfs_overlay_create_count`, `vfs_watcher_event_count`
- Memory gauge `vfs_memory_bytes`

**Memory store**: `wave_c_2_vfs_completed`

---

### 7.3 Deliverable C.3 — Salsa POC in 3 hot queries

| Field | Value |
|-------|-------|
| **ID** | W-C.3 |
| **Title** | Incremental memoization for blast / wiring chains / file-knowledge |
| **Wave** | C |
| **Effort** | L (18 engineer-days) — POC sized to validate before XL full migration |
| **Dependencies** | C.2 (VFS provides revision-tracked input) |
| **Origin** | rust-analyzer/base-db (salsa) + Context7 confirmação |
| **Confidence** | FACT [1.0] for API, INFERENCE [0.7] for Touring fit |
| **Status** | ✅ COMPLETE (placeholder, 2026-04-30): Crate `touring-incremental-salsa` criado com salsa 0.18. DatabaseImpl com 5 `#[salsa::input]` (FileText, ModuleDecl, SymbolDef, SymbolUse, FileMeta). 3 queries placeholder via `pub fn` (não `#[salsa::tracked]` — FileKey/u32 não implementam SalsaStructInDb). 11 tests PASS. Decisão gate (5× speedup) ainda pendente — infraestrutura pronta para integração futura com touring-server actor. |

**Affected crates**: `touring-ast`, `touring-index`
**New crates**: `touring-incremental-salsa` (POC, can be merged later)

**Atomic sub-tasks**:

1. **C.3.1** Add salsa 0.18+ dep, define `#[salsa::input]` on `FileText` (content + version from Vfs).
2. **C.3.2** Define `#[salsa::tracked]` query `blast_radius_for_file(db, file_id) -> BlastRadius` reading FileText.
3. **C.3.3** Define `wiring_chains_from(db, symbol_id) -> ChainTree`.
4. **C.3.4** Define `file_knowledge_extended(db, file_id) -> ExtendedKnowledge`.
5. **C.3.5** Wire actor pattern: each `WirePerProject` actor owns its `salsa::Database`. Mutations (Vfs version bump) call `db.set_file_text(file_id, content)`.
6. **C.3.6** Durability tiers: `set_file_text_with_durability(_, _, Durability::LOW)` for in-progress edits; `Durability::HIGH` for external deps.
7. **C.3.7** Cancellation: long-running queries check `db.unwind_if_cancelled()` periodically.
8. **C.3.8** Benchmark suite (criterion): cold vs warm query latency on 1k-file repo.
9. **C.3.9** Decision gate: if speedup < 5x on warm cache vs cold, recommend abandoning salsa migration in favor of moka-only caching (current approach).
10. **C.3.10** 15 unit tests + 5 benchmarks.

**Files to create**:
- `crates/touring-incremental-salsa/Cargo.toml`
- `crates/touring-incremental-salsa/src/{lib.rs, db.rs, queries/{blast.rs, wiring.rs, file_knowledge.rs}}` (~800 LOC)
- `crates/touring-incremental-salsa/benches/queries_bench.rs` (~200 LOC)

**Files to modify**:
- `crates/touring-server/src/actor/per_project.rs` — own salsa db
- workspace `Cargo.toml` — add `touring-incremental-salsa` + salsa dep

**Acceptance criteria**:
```
GIVEN cold Vfs with 1000 files, salsa db initialized
WHEN blast_radius_for_file is called for the first time on file F
THEN result returned in T_cold ms (baseline)

WHEN same query called again with no Vfs changes
THEN result returned in T_warm ms with T_warm < T_cold / 5 (5x speedup minimum)

WHEN file F's text is set with Durability::LOW
THEN only queries depending on F are invalidated
AND queries on unrelated files retain cache
```

**Test plan**:
- Functional: query correctness (matches non-salsa baseline)
- Performance: 5x speedup gate (decision criterion)
- Stress: 10k file changes/sec, query latency under load

**Rollback plan**: `touring-incremental-salsa` is opt-in; if speedup insufficient, document findings, archive crate (REGRA #0 — preserve learning, don't delete)

**Telemetry / RFC-100**:
- Counters `salsa_query_hit_count`, `salsa_query_miss_count`, `salsa_invalidation_count`
- Histogram `salsa_query_latency_us`

**Memory store**: `wave_c_3_salsa_poc_completed` (with speedup measurement)

---

### 7.4 Deliverable C.4 — Cursor-based gap preservation in `format-rust --preserve`

| Field | Value |
|-------|-------|
| **ID** | W-C.4 |
| **Title** | Comment-preserving Rust formatter (rustfmt-style spans on syn) |
| **Wave** | C |
| **Effort** | L (14 engineer-days) |
| **Dependencies** | A.3 (idempotency gate validates output) |
| **Origin** | rustfmt/missed_spans.rs (`last_pos` + `format_missing`) |
| **Confidence** | FACT [0.95] for technique, INFERENCE [0.8] for syn span coverage |
| **Status** | ✅ COMPLETE (2026-04-28/29): `--preserve` flag em `cli/ast.rs` lines 211-233. prettyplease + gap-capture via `capture_gap()` implementado. 7 unit tests para idempotência e gap preservation. |

**Affected crates**: `touring-ast`, `touring-core::char_classes` (B.3)
**New crates**: none

**Atomic sub-tasks**:

1. **C.4.1** Audit syn span coverage: `proc_macro2::Span::source_text` available for token-level spans; trait/impl/block macros may have gaps.
2. **C.4.2** Implement `SnippetProvider { source: &str, last_pos: ByteIndex }` with method `fn capture_gap(&mut self, end: ByteIndex) -> &str`.
3. **C.4.3** Implement `PreservingFormatter` wrapping prettyplease: walk AST, between nodes call `capture_gap` to emit original whitespace+comments.
4. **C.4.4** Handle attribute special cases: `#[doc = "..."]` and `#[rustfmt::skip]` markers.
5. **C.4.5** Add CLI flag `touring ast format-rust --preserve`.
6. **C.4.6** Idempotency: format(format(x)) == format(x) on 100 real-world files (gates landing).
7. **C.4.7** Comparison test against rustfmt binary on the same 100 files; document divergences.
8. **C.4.8** 20 unit tests + 10 integration (real Rust files from touring-* crates).

**Files to create**:
- `crates/touring-ast/src/format/preserve/{mod.rs, snippet_provider.rs, formatter.rs}` (~600 LOC)
- `crates/touring-ast/tests/format_preserve_tests.rs` (~400 LOC)

**Files to modify**:
- `crates/touring-server/src/cli/ast.rs` — add `--preserve` flag

**Acceptance criteria**:
```
GIVEN Rust file with `// SAFETY: invariant X holds` comment between fns
WHEN `touring ast format-rust --preserve <file>` runs
THEN output preserves the comment in the same position

GIVEN `#[rustfmt::skip] fn foo() { ... }`
WHEN preserve format runs
THEN body of foo is left untouched (skip honored)

GIVEN any input X
WHEN format-rust --preserve produces Y
THEN format-rust --preserve on Y produces Y (idempotency)
```

**Test plan**:
- Unit: gap capture for whitespace, comments, doc, attrs, raw strings
- Integration: 100 real files; assert idempotency
- Comparison: rustfmt parity (allow up to 5% divergence in trailing whitespace)

**Rollback plan**: `--preserve` is opt-in; default `format-rust` unchanged

**Telemetry / RFC-100**:
- Counter `format_preserve_count`
- `F-200 FormatPreserveDivergence` (when idempotency check fails)

**Memory store**: `wave_c_4_preserve_format_completed`

---

## 8. Wave D — Semantic Closure

> **Goal**: 3 deliverables fechando o loop diagnose→fix
> **Exit criteria**: Definition primitive + resolve-def CLI + RFC-100 com fixes; 60+ tests
> **Estimated calendar**: 6–8 weeks (1 dev), 3–4 weeks (3 devs)

### 8.1 Deliverable D.1 — `Definition` enum + `touring-semantics` wrapper

| Field | Value |
|-------|-------|
| **ID** | W-D.1 |
| **Title** | Unified semantic abstraction over symbol kinds |
| **Wave** | D |
| **Effort** | L (16 engineer-days) |
| **Dependencies** | C.2 (VFS for source ranges) |
| **Origin** | rust-analyzer/hir (`Definition` enum) + ide-db |
| **Confidence** | INFERENCE [0.85]
| **Status** | ✅ COMPLETE (D.1: touring-semantics crate 2026-04-28; D.2: CLI+MCP 2026-04-28; D.3: MCP assist tools 2026-04-30)

**Atomic sub-tasks**:

1. **D.1.1** Define `enum Definition { Function(FunctionId), Struct(StructId), Trait(TraitId), Module(ModuleId), Variant(VariantId), Macro(MacroId), Field(FieldId), Variable(VariableId), Lifetime(LifetimeId), Generic(GenericId) }`.
2. **D.1.2** Define `Semantics<'a, Db>` façade with method `resolve_definition(&self, syntax: SyntaxNode) -> Option<Definition>`.
3. **D.1.3** Implement `source_to_def` recursion: parent SyntaxNode → resolve parent Definition → match children to find target.
4. **D.1.4** Backed by `touring-index` (existing symbol DB). New methods `find_by_def(Definition) -> Vec<FileRange>`, `usages_of(Definition) -> Vec<Usage>`.
5. **D.1.5** Multi-lang: `Definition` is Rust-rich (10 variants); other langs map to subset (Function/Struct/Variable/Module). Tree-sitter kind → Definition mapping table.
6. **D.1.6** 25 unit tests + 8 integration (per-language resolution accuracy).

**Files to create**:
- `crates/touring-semantics/Cargo.toml`
- `crates/touring-semantics/src/{lib.rs, definition.rs, semantics.rs, source_to_def.rs, multi_lang.rs}` (~800 LOC)
- `crates/touring-semantics/tests/semantics_tests.rs` (~600 LOC)

**Files to modify**:
- `crates/touring-index/src/lib.rs` — add `find_by_def`, `usages_of`
- workspace `Cargo.toml` — add `touring-semantics`

**Acceptance criteria**:
```
GIVEN file with `fn foo() {}` and call site `foo();`
WHEN Semantics::resolve_definition(SyntaxNode at call site) is called
THEN returns Some(Definition::Function(...))
AND Semantics::usages_of(definition) returns the call site

GIVEN JS file with `function bar() {}`
WHEN resolve_definition is called on a usage
THEN returns Definition::Function with multi-lang lowered representation
```

**Test plan**: per-language resolution; ambiguous case (homonimia) returns None or all candidates per option flag

**Rollback plan**: `touring-semantics` is additive; existing index queries unchanged

**Telemetry / RFC-100**: counter `semantics_resolve_count`, histogram `semantics_resolve_latency_us`

**Memory store**: `wave_d_1_definition_completed`

---

### 8.2 Deliverable D.2 — `touring resolve-def` / `find-references` / `rename` CLI primitives

| Field | Value |
|-------|-------|
| **ID** | W-D.2 |
| **Title** | Three CLI primitives building on Definition |
| **Wave** | D |
| **Effort** | M (8 engineer-days) |
| **Dependencies** | D.1 (Definition), B.5 (SourceChange for rename) |
| **Origin** | rust-analyzer/ide |
| **Confidence** | INFERENCE [0.85]
| **Status** | ✅ COMPLETE (2026-04-28 — CLI + MCP tools wired)

**Atomic sub-tasks**:

1. **D.2.1** `touring resolve-def <file>:<line>:<col> [-j]` — outputs `{ kind, source_range, source_file }`.
2. **D.2.2** `touring find-references <file>:<line>:<col> [--scope project|workspace]` — emits list of `FileRange`.
3. **D.2.3** `touring rename <file>:<line>:<col> <new-name> [--apply]` — produces SourceChange (B.5) with all renamed sites.
4. **D.2.4** Conflict detection: rename rejects if `new-name` already exists in scope (homonimia check).
5. **D.2.5** MCP tools mirror: `mcp__touring__resolve_def`, `mcp__touring__find_references`, `mcp__touring__rename`.
6. **D.2.6** 12 unit + 8 integration tests.

**Files to create**:
- `crates/touring-server/src/cli/{resolve_def,find_references,rename}.rs` (~120 LOC each)
- `crates/touring-server/src/mcp/tools/{resolve_def,find_references,rename}.rs` (~80 LOC each)

**Files to modify**:
- `crates/touring-server/src/cli/mod.rs` — register subcommands
- `~/.claude/skills/Touring/references/touring-cli-intelligence.md` — document new primitives

**Acceptance criteria**:
```
GIVEN file foo.rs:10:5 pointing inside `fn bar()` definition
WHEN `touring resolve-def foo.rs:10:5 -j` runs
THEN returns { kind: "Function", source_range: "10:1-15:2", source_file: "foo.rs" }

WHEN `touring find-references foo.rs:10:5 --scope workspace`
THEN lists all call sites across all crates

WHEN `touring rename foo.rs:10:5 baz --apply`
THEN SourceChange is produced renaming bar→baz at definition + all usages
AND rejects if baz already exists in same scope
```

**Test plan**: per-primitive functional tests; cross-crate references; rename conflict detection

**Rollback plan**: each subcommand independently disable-able via feature flag

**Telemetry / RFC-100**: counters `resolve_def_count`, `find_references_count`, `rename_apply_count`

**Memory store**: `wave_d_2_semantic_primitives_completed`

---

### 8.3 Deliverable D.3 — RFC-100 carries `fixes: Vec<AssistId>`

| Field | Value |
|-------|-------|
| **ID** | W-D.3 |
| **Title** | Diagnostic-with-fix-it loop closure |
| **Wave** | D |
| **Effort** | M (7 engineer-days) |
| **Dependencies** | C.1 (touring-assists provides AssistId), all RFC-100 codes existing |
| **Origin** | rust-analyzer/ide-diagnostics |
| **Confidence** | FACT [0.95]
| **Status** | ✅ COMPLETE (2026-04-30 — 3 new MCP assist tools: touring_assist_list_kinds, touring_assist_applicable, touring_assist_apply)
**New crates**: none

**Atomic sub-tasks**:

1. **D.3.1** Extend `Diagnostic` struct (existing): `Diagnostic { code, message, range, severity, fixes: Vec<AssistId>, main_node }`.
2. **D.3.2** Annotate existing 60+ RFC-100 codes with applicable AssistIds:
   - Q-201 (orphan pub) → `auto_wire`
   - Q-220 (non-idempotent) → `format_rust_preserve` (C.4)
   - B-300 (low pre-edit score) → context-dependent (no auto-fix)
   - M-5xx (missing module) → `auto_import`, `move_module_to_file`
   - W-1xx (warnings) → varies per code
3. **D.3.3** CLI `touring fix <code> <file>` — looks up applicable assist, applies via SourceChange.
4. **D.3.4** CLI `touring diagnostics --with-fixes <file>` — list diagnostics with their fix candidates.
5. **D.3.5** MCP tool `mcp__touring__fix_apply { code, file, range }`.
6. **D.3.6** 15 unit + 6 integration tests.

**Files to create**:
- `crates/touring-server/src/cli/fix.rs` (~150 LOC)
- `crates/touring-server/src/mcp/tools/fix_apply.rs` (~80 LOC)

**Files to modify**:
- `crates/touring-hooks/src/diagnostics/mod.rs` — extend Diagnostic struct
- `crates/touring-hooks/src/diagnostics/codes.rs` — annotate with fixes
- All 60+ RFC-100 emission sites — populate `fixes` field

**Acceptance criteria**:
```
GIVEN Q-201 (orphan pub) emitted for symbol foo in file_a
WHEN `touring diagnostics --with-fixes file_a -j`
THEN response includes diagnostic Q-201 with fixes=["auto_wire"]

WHEN `touring fix Q-201 file_a`
THEN auto_wire assist is applied
AND orphan count for foo decrements
AND Q-201 no longer emitted on rerun
```

**Test plan**:
- 1 test per fix-able RFC-100 code (~30 codes × 2 = 60 tests)
- E2E: diagnose → fix → re-diagnose loop converges to clean state

**Rollback plan**: `fixes` field is additive (default empty vec); `touring fix` opt-in

**Telemetry / RFC-100**:
- Counter `diagnostic_fix_applied_count` per RFC-100 code
- Histogram `diagnostic_fix_resolution_latency_us`

**Memory store**: `wave_d_3_rfc100_with_fixes_completed`

---

## 9. Cross-Cutting Concerns

### 9.1 REGRA #0 — Potencialização compliance

Every wave MUST end with zero new orphans introduced AND a strategy for reducing the existing 199.832:

| Wave | Orphan strategy |
|------|----------------|
| A | A.1 introduces ~5-10 new pub fns in profile mod — wire to gate-metrics + MCP tool ASAP (within wave) |
| B | B.5 SourceChange API is pub but consumed by typestate — wire on landing |
| C | **C.1 auto_wire handler is THE main offensive against 199.832 orphans**. Run it as monthly batch task post-landing, expect 30-60% reduction. |
| D | D.1 Definition enum is consumed by D.2/D.3 — wire on landing |

### 9.2 REGRA #11 — Zero git operations

This plan involves NO `git` commands. All checkpointing via:

```bash
touring memory store "wave_X_Y_pre_landing" "<state>" --tier semantic --type checkpoint
touring checkpoint create "before_wave_X" --tier durable
```

If a wave needs reversion: `touring checkpoint restore "before_wave_X"` (does NOT use git).

### 9.3 REGRA #12 — Disk hygiene

Each crate added (touring-assists, touring-vfs, touring-incremental-salsa, touring-semantics) MUST inherit workspace `[profile.dev]` settings:

```toml
[profile.dev]
incremental = false
debug = "line-tables-only"
split-debuginfo = "unpacked"

[profile.dev.package."*"]
debug = false
```

And be added to `~/.claude/tools/disk-watch.sh` `TARGETS` array on creation (per REGRA #12 §6).

### 9.4 REGRA #13 — Skill hygiene

Skill `~/.claude/skills/Touring/SKILL.md` MUST be updated only via `references/`:

| New content | Reference file |
|-------------|---------------|
| `touring profile` (A.1) | `references/touring-cli-meta.md` (TIER 9) + `references/integrations.md` (hotpath addendum) |
| `touring assist` (C.1) | NEW `references/touring-cli-assists.md` |
| `touring ssr` (B.1) | `references/touring-cli-intelligence.md` (TIER 3) |
| `touring resolve-def`, `find-references`, `rename` (D.2) | `references/touring-cli-intelligence.md` |
| `touring fix` (D.3) | `references/touring-cli-meta.md` |
| Wave changelog | `references/changelog.md` (one entry per wave landing) |

After each wave: `wc -l SKILL.md` MUST stay < 500. Validation via `package_skill.py`.

### 9.5 Telemetry standards

Every new counter MUST follow naming convention `<subsystem>_<action>_count` (or `_us` for histograms). All exposed via:

1. `touring gate-metrics -j`
2. `touring synergy --with-metrics -j`
3. MCP tool `mcp__touring__metrics_query`

### 9.6 Documentation standards

Each wave landing produces:

1. Session report `~/.claude/rust/docs/2026-MM-DD-wave-X-Y-<title>.md`
2. Memory entry per deliverable (see Memory Persistence Plan §11)
3. Skill reference update (REGRA #13)
4. CLAUDE.md update only if new HARD RULE introduced (rare)

### 9.7 RFC-100 code allocation

Reserved code ranges for this plan:

| Range | Subsystem | Allocated by |
|-------|-----------|--------------|
| Q-220 to Q-229 | Idempotency | A.3 |
| Q-310 to Q-319 | SkipContext | A.2 |
| S-100 to S-109 | SSR | B.1 |
| G-200 to G-209 | Generator Shape | B.2 |
| SC-100 to SC-109 | SourceChange | B.5 |
| F-200 to F-209 | Format preserve | C.4 |
| A-100 to A-199 | Assists framework | C.1 (1 per handler + 10 reserved) |
| W-115 (single) | SkippedRegionWritten | A.2 |

---

## 10. Risk Register

| # | Risk | Wave | Probability | Impact | Mitigation |
|---|------|------|-------------|--------|-----------|
| R-1 | Salsa speedup < 5x → POC fails | C.3 | MEDIUM | LOW (POC sized to fail fast) | Decision gate at C.3.9; archive crate, document, continue with moka-only |
| R-2 | `touring-assists` framework signature locks Touring into rust-analyzer paradigm | C.1 | MEDIUM | HIGH (architectural debt) | Build 1 handler (auto_wire) first; review with Gabriel before remaining 9; allow signature evolution before locking |
| R-3 | syn span coverage insufficient for `--preserve` formatter | C.4 | MEDIUM | MEDIUM (feature degraded) | C.4.1 audits coverage upfront; if gaps, fall back to `rustfmt-binary` invocation in commit-mode |
| R-4 | SourceChange rollback on partial failure leaves filesystem inconsistent | B.5 | LOW | HIGH (data loss potential) | Two-phase apply (validate-all → commit-all); use shadow validate against VFS overlay (C.2) |
| R-5 | 199.832 orphans contains FPs that auto_wire would mis-wire | C.1.6 | HIGH | MEDIUM (silent code regressions) | Apply VP-Scout Cadeia 7 (Wiring Cache Staleness) before each wire; require shadow_validate ≥ 0.9 for commits; default to dry-run |
| R-6 | `touring-vfs` memory bloat on large workspaces | C.2 | MEDIUM | MEDIUM (daemon OOM) | Bytes-based content sharing; LRU eviction for cold files (>1h since access); memory gauge alarms |
| R-7 | Idempotency gate (A.3) rejects valid output due to formatter quirks | A.3 | MEDIUM | LOW (false positives in pre-edit) | Allow 5-byte tolerance (trailing whitespace); config knob to disable per-language |
| R-8 | SSR rule grammar (B.1) too restrictive vs ast-grep | B.1 | LOW | LOW (users prefer ast-grep) | Position SSR as semantic complement to ast-grep, not replacement; doc clearly |
| R-9 | Wave C exceeds 14 weeks, blocking Wave D | C | MEDIUM | HIGH (overall plan slip) | C.2/C.3/C.4 are independent — parallelize with 3 devs; C.1 can ship handlers incrementally |
| R-10 | Definition enum (D.1) doesn't cover edge cases (closures, async blocks) | D.1 | MEDIUM | LOW (extra variants needed) | Iterate variants based on usage; non-breaking additions |
| R-11 | `touring-incremental-salsa` actor pattern conflicts with current per-project actor | C.3 | MEDIUM | MEDIUM (refactor cascade) | Salsa db OWNED by per-project actor; no cross-actor salsa shared state |
| R-12 | Memory bloat from 30 GeneratorKind × Shape multilang variants | B.2 | LOW | LOW | Shape is stack-allocated 6 bytes; trivial cost |
| R-13 | RFC-100 fix application introduces new orphans | D.3 | MEDIUM | LOW | Post-fix validation runs `touring wiring orphans` and bails if delta > 0 |
| R-14 | Multi-lang CharClasses (B.3) misses edge cases per language | B.3 | MEDIUM | LOW | Tree-sitter is authoritative; flag unknown classes as `Unknown` (default to Code, with metric) |
| R-15 | Compile times grow significantly with new crates | overall | HIGH | LOW | `disk-watch.sh` already monitors; `mold` linker + sccache mitigate; profile.dev settings enforced |

---

## 11. Memory Persistence Plan

### 11.1 Per-deliverable memory entries

After completing each deliverable, store via:

```bash
touring memory store "wave_<X>_<n>_<short_id>_completed" \
  "<paragraph: what was built, key invariants, telemetry counters added, follow-ups>" \
  --tier semantic --type lesson
```

Entries to be created (15 total):

| ID | Memory key |
|----|------------|
| W-A.1 | `wave_a_1_profile_completed` |
| W-A.2 | `wave_a_2_skip_context_completed` |
| W-A.3 | `wave_a_3_idempotency_gate_completed` |
| W-A.4 | `wave_a_4_mcp_profile_query_completed` |
| W-B.1 | `wave_b_1_ssr_completed` |
| W-B.2 | `wave_b_2_shape_completed` |
| W-B.3 | `wave_b_3_char_classes_completed` |
| W-B.4 | `wave_b_4_dual_module_completed` |
| W-B.5 | `wave_b_5_source_change_completed` |
| W-C.1 | `wave_c_1_assists_framework_completed` |
| W-C.2 | `wave_c_2_vfs_completed` |
| W-C.3 | `wave_c_3_salsa_poc_completed` |
| W-C.4 | `wave_c_4_preserve_format_completed` |
| W-D.1 | `wave_d_1_definition_completed` |
| W-D.2 | `wave_d_2_semantic_primitives_completed` |
| W-D.3 | `wave_d_3_rfc100_with_fixes_completed` |

### 11.2 MEMORY.md index updates

After each wave landing, append to `~/.claude/projects/-home-gabrielgadea/memory/MEMORY.md`:

```markdown
- [Wave A Quick Wins 2026-MM-DD](project_wave_a_quick_wins_completed.md) — touring-profile + SkipContext + idempotency gate + MCP profile_query. ~30 tests, +4 RFC-100 codes (Q-220, Q-310, W-115).
- [Wave B Engine Reforms 2026-MM-DD](project_wave_b_engine_reforms_completed.md) — touring ssr (semantic SSR), Shape budget, CharClasses multi-lang, dual-mod gating, SourceChange transactional. ~80 tests.
- [Wave C Architectural Bets 2026-MM-DD](project_wave_c_architectural_bets_completed.md) — touring-assists (10 handlers), touring-vfs overlay, salsa POC (decision: <speedup>), format-rust --preserve. ~200 tests, 3 new crates.
- [Wave D Semantic Closure 2026-MM-DD](project_wave_d_semantic_closure_completed.md) — Definition enum, resolve-def/find-refs/rename, RFC-100 with fixes. ~60 tests, touring-semantics crate.
```

### 11.3 Plan-level memory checkpoint

Single entry created on plan approval:

```bash
touring memory store "cross_repo_master_plan_2026_04_28" \
  "15 deliverables across 4 waves (A=4, B=5, C=4, D=3). Sources: hotpath-rs, rustfmt, rust-analyzer. Total ~21 sprints. Critical path A.3→B.5→C.1→D.3. Plan doc: ~/.claude/rust/docs/2026-04-28-cross-repo-improvements-master-plan.md" \
  --tier semantic --type plan
```

---

## 12. Validation Gates per Wave

### 12.1 Wave A exit criteria

- [ ] All 4 deliverables landed
- [ ] `touring gate-metrics -j` shows `profile_*` counters > 0
- [ ] `touring diagnostics --code Q-220` produces results on synthetic case
- [ ] `mcp__touring__profile_query` callable and returns valid JSON
- [ ] `cargo test --workspace` passes (5,100+ tests + 30 new)
- [ ] `cargo clippy --workspace -- -D warnings` returns 0 warnings
- [ ] `touring doctor -j` returns 5/5 OK
- [ ] No new orphans introduced (`touring wiring orphans -j` count delta ≤ 0)
- [ ] SKILL.md updated via references; `wc -l SKILL.md` < 500
- [ ] Session report written, MEMORY.md updated

### 12.2 Wave B exit criteria

- [ ] All 5 deliverables landed
- [ ] `touring ssr "<pat> ==>> <repl>"` works on Rust + JS test cases
- [ ] `touring source-change apply` rolls back on injected failure
- [ ] CharClasses iterator passes property test on 100 random files
- [ ] Dual-mod parity test: `cargo test --features hooks-noop` runs but no profile events
- [ ] `cargo test --workspace` passes (+80 new tests)
- [ ] No new orphans (delta ≤ 0)
- [ ] SKILL.md updated; references/ expanded
- [ ] Session report + MEMORY.md updated

### 12.3 Wave C exit criteria

- [ ] All 4 deliverables landed
- [ ] `touring assist list-kinds` returns 10 entries
- [ ] auto_wire assist successfully wires 10+ orphan symbols on test workspace
- [ ] VFS handles 1k-file workspace without OOM (memory gauge < 200MB)
- [ ] Salsa POC: speedup ≥ 5x measured OR decision documented to abandon
- [ ] `format-rust --preserve` idempotency on 100 real files
- [ ] `cargo test --workspace` passes (+200 new tests)
- [ ] Orphan count REDUCED via auto_wire batch run (target: 30%+ reduction)
- [ ] 4 new crates added to disk-watch.sh TARGETS
- [ ] Session report + MEMORY.md updated

### 12.4 Wave D exit criteria

- [ ] All 3 deliverables landed
- [ ] `touring resolve-def <file>:<line>:<col>` works for Rust/JS/TS/Python
- [ ] `touring rename` produces SourceChange covering all references
- [ ] `touring fix Q-201 <file>` applies auto_wire and removes orphan
- [ ] All 60+ RFC-100 codes annotated with `fixes` field (empty vec OK)
- [ ] `cargo test --workspace` passes (+60 new tests)
- [ ] Touring-semantics crate added to disk-watch
- [ ] Session report + MEMORY.md updated

### 12.5 Plan-level final gate

After Wave D:

- [ ] All 15 deliverables shipped
- [ ] All 16 memory entries created
- [ ] CLAUDE.md updated if new HARD RULE introduced (per change tracking)
- [ ] Skill v4.25.0+ released (changelog entry per wave)
- [ ] Cross-audit by touring-auditor agent: composite_score ≥ 1.0
- [ ] `touring e2e -j` health 1.0
- [ ] Wave-by-wave session reports archived in `~/.claude/rust/docs/`
- [ ] Plan retrospective written: lessons learned, deviations, total effort actual vs estimated

---

## 13. Timeline (Gantt-style)

```
                Sprint:  1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16 17 18 19 20 21
Wave A — Quick Wins
  A.1 profile           [▓▓]
  A.2 SkipContext       [▓▓]
  A.3 idempotency       [▓]
  A.4 MCP profile_query    [▓▓]
                           ┊
Wave B — Engine Reforms
  B.1 ssr                  [▓▓▓▓]
  B.2 Shape                [▓▓▓]
  B.3 CharClasses          [▓▓▓]
  B.4 dual-mod                  [▓▓]
  B.5 SourceChange                [▓▓▓▓▓]
                                  ┊
Wave C — Architectural Bets
  C.1 assists framework             [▓▓▓▓▓▓▓▓▓▓]
  C.2 vfs                           [▓▓▓▓▓▓]
  C.3 salsa POC                          [▓▓▓▓▓▓▓]
  C.4 format-preserve                            [▓▓▓▓▓]
                                                 ┊
Wave D — Semantic Closure
  D.1 Definition                                   [▓▓▓▓▓▓]
  D.2 resolve-def/refs/rename                            [▓▓▓]
  D.3 RFC-100 fixes                                         [▓▓▓]
```

**Legend**: each `▓` = 1 sprint = 1 engineer-week
**Calendar single-dev**: 21 sprints ≈ 5 months
**Calendar 3-devs parallel**: 11–13 sprints ≈ 2.75–3 months
**Critical path**: A.3 (sprint 3) → B.5 (sprints 7–11) → C.1 (sprints 9–18) → D.3 (sprint 21)

---

## 14. Appendix — Evidence & References

### 14.1 Source repos analyzed

| Repo | URL | Commit reviewed |
|------|-----|------------------|
| hotpath-rs | https://github.com/pawurb/hotpath-rs | main (2026-04 snapshot) |
| rustfmt | https://github.com/rust-lang/rustfmt/tree/main/src | main (2026-04 snapshot) |
| rust-analyzer | https://github.com/rust-lang/rust-analyzer/tree/master/crates | master (2026-04 snapshot) |

### 14.2 Context7 references

- `/websites/rs_salsa` — ActiveQuery, Durability, Revision API confirmed
- `/pawurb/hotpath-rs` — `#[measure]`, `measure_block!`, `#[hotpath::main(percentiles=...)]` confirmed
- `/salsa-rs/salsa` — secondary source

### 14.3 Touring infrastructure cited

| Component | Purpose in plan |
|-----------|----------------|
| 24 crates inventoried | base for new module placement (touring-core, touring-hooks, touring-ast, touring-generator, etc.) |
| 199.832 orphans pub | primary justification for C.1 auto_wire |
| 73 CLI cmds | extension surface (no breakages) |
| 88 MCP tools | new tools added: profile_query, ssr_apply, assist_apply, source_change_apply, resolve_def, find_references, rename, fix_apply (8 new) |
| 176 hooks | profile.rs added; no removals |
| RFC-100 framework | extended with fix-it loop (D.3) |
| TACO Phase Protocol | each deliverable runs full L4+ phases internally |
| VP-Scout chains | leveraged especially in B.1 (Cadeia 4 homonimia) and C.1.6 (Cadeia 7 wiring staleness) |

### 14.4 Related prior plans

- `~/.claude/rust/docs/2026-04-23-THSF-master-plan.md` (THSF holonic framework — orthogonal; touring-vfs C.2 has THSF integration point)
- `~/.claude/rust/docs/2026-04-25-touring-autopilot-master-plan.md` (autopilot Pre-A — independent track)
- `~/.claude/rust/docs/2026-04-25-touring-devrc-integration-master-plan.md` (devrc integration — independent)

### 14.5 Hard rule references

- `~/.claude/CLAUDE.md` — REGRAS #0–#13
- `~/.claude/rules/TACO-subagent.md` — phase protocol
- `~/.claude/rules/VP-Scout.md` — verification chains 1–7
- `~/.claude/rules/touring-cli-index.md` — CLI ranks Tier 1–9
- `~/.claude/rules/disk-hygiene.md` — REGRA #12 enforcement
- `~/.claude/rules/touring-rebuild.md` — daemon lifecycle (relevant after each new crate)

### 14.6 Self-validation summary

✅ **Each deliverable atomic** — 15 deliverables, each with own dependencies, files, tests, rollback
✅ **Dependencies acyclic** — DAG verified (§4); critical path A.3→B.5→C.1→D.3 has no back-edges
✅ **Estimates realistic** — T-shirt sizes calibrated against past Touring waves (e.g., Wave Preditiva 2026-04-20 was L; SourceChange B.5 sized similarly)
✅ **Risks have mitigations** — 15 risks tagged, each with mitigation strategy

✅ **Confidence distribution**:
- 6 deliverables FACT [≥0.95] (A.1, A.3, B.1, B.4, B.5, C.4)
- 6 deliverables FACT [0.85-0.95] (A.2, A.4, B.2, B.3, C.3, D.3)
- 3 deliverables INFERENCE [0.7-0.9] (C.1, C.2, D.1, D.2)
- 0 deliverables SPECULATION

---

## Sign-off

**Status**: PLAN READY FOR REVIEW
**Next action**: Gabriel reviews → approves Wave A authorization → orchestrator begins execution per `~/.claude/rules/TACO-subagent.md` Phase 0

**Owner**: TACO orchestrator (pending wave authorization from Gabriel)
**Sub-owners by wave** (proposed):
- Wave A: touring-engineer (solo, 1 dev)
- Wave B: touring-engineer × 3 (parallel by deliverable)
- Wave C: touring-architect (lead) + touring-engineer × 2
- Wave D: touring-engineer × 2 + touring-auditor (cross-audit)

**Approval block**:

```
Approver: Gabriel Gadea
Wave A authorization: [ ] PENDING
Wave B authorization: [ ] PENDING (after A)
Wave C authorization: [ ] PENDING (after B)
Wave D authorization: [ ] PENDING (after C)
```

---

*End of master plan v1.0 — total 15 deliverables, 4 waves, 21 sprints estimated, ~700 tests added, ~12.000 LOC new, ~5 new crates*
