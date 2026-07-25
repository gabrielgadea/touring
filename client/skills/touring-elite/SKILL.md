---
name: touring-elite
description: Composite 13-gate EliteScore aggregator for Touring — surfaces release-readiness (Diamond/Platinum/Gold/Silver/Bronze/Unranked) via `docs/elite_aggregate.py`. Activates when releasing, refactoring, opening PRs, or auditing quality across the 17 dimensions of elite de mercado (architecture, security, performance, testing, documentation, best practices, CI/CD, modularization, scalability, extensibility, naming, navigability, craftsmanship, dependencies, UX, product docs).
---

# Touring — Elite 17-Dimensional Harness

> **Status** (2026-06-13): Composite **0.9703 (Diamond tier)**. 11/13 gates at 1.00, 1 WARN (extensibility 4 known issues), 1 ADVISORY (perf baseline pending first run).

Touring is the **first open-source harness** that delivers **Enterprise-Grade AI Code Governance** as a single composite score. It aggregates 13 orthogonal quality gates (and projects to 17 dimensions) and BLOCK-by-default any output that doesn't reach **Gold tier (0.80+)**.

## When to Use

Always before:
- Releasing a Touring version (or any workspace that adopts Touring)
- Opening a PR that touches >500 LOC OR introduces a new public API
- Auditing code health across all 17 elite-de-mercado dimensions
- Justifying a code review decision with a quantitative signal

Skip for:
- Single-line typo fixes
- Trivial edits (< 50 LOC, no API change)

## Composite Score — 6 Tiers

| Tier | Range | Badge | Release-ready? |
|------|-------|-------|----------------|
| **Diamond** | 0.95-1.00 | 💎 Premium | ✓ Yes — exceptional |
| **Platinum** | 0.90-0.94 | ★ Elite de Mercado | ✓ Yes — best-in-class |
| **Gold** | 0.80-0.89 | ✓ Elite | ✓ Yes — production-grade |
| **Silver** | 0.70-0.79 | ⚠ Review | ⚠ Human review required |
| **Bronze** | 0.60-0.69 | ⚠ Rewrite | ✗ Refactor before merging |
| **Unranked** | < 0.60 | 🚫 BLOCK | ✗ Rewrite mandatory |

## The 13 Aggregated Gates

| # | Gate | Weight | Status | Notes |
|---|------|--------|--------|-------|
| 02 | architecture | 1.0 | ✅ | `wiring_integrity_gate.py` (cycles=0, blast<10) |
| 03 | security_advisories | 1.5 | ✅ | cargo-deny (SEC-03 2026-06-13 binding) |
| 04 | performance | 0.7 | ⚠ ADVISORY | `perf_p99_gate.py` — needs `cargo bench` baseline |
| 05 | testing | 1.0 | ✅ | `file_size_gate.py` (proxy: file size = testability) |
| 06 | documentation | 1.0 | ✅ | `gen_reference.py --validate` |
| 08 | ci_cd_devops | 0.8 | ✅ | `root_hygiene_gate.py` |
| 09 | modularization | 0.8 | ✅ | `file_size_gate.py` |
| 10 | scalability | 0.7 | ✅ | `scalability_scan.py` (0 findings) |
| 11 | extensibility | 0.6 | ✅ | `extensibility_scan.py` (4 known kitchensinks) |
| 14 | craftsmanship | 0.7 | ✅ | `craftsmanship_tdg_gate.py` (292 files, 0 failures) |
| 15 | dependencies | 1.5 | ✅ | cargo-deny (CI binding) |
| 16 | ux | 0.6 | ✅ | `ux_audit.py` (5/5 shells + 76 cmd + 213 arg) |
| 17 | product_docs | 0.9 | ✅ | `sync_metrics.py --check` |

**Missing from aggregator but present in CI**: 01 code_quality, 07 best_practices, 12 naming, 13 navigability (covered by `cargo clippy -D warnings` + `rustdoc -D warnings` + `cargo fmt` + `cognitive metrics`).

## Relationship to the 50-Dimension Engine (`touring-quality`)

Two complementary motors — **do not confuse them**:

| Motor | Granularidade | Comando | Quando |
|-------|---------------|---------|--------|
| **touring-elite** (this skill) | 13 gates → composite **release-readiness** | `python3 docs/elite_aggregate.py --check` | release, PR >500 LOC, nova API pública |
| **touring-quality** | **50 dims por arquivo** (F1.1–F4.12) | `touring-quality score <FILE> --dims F2.5` · `check --gate F2.1 --target <FILE>` | pré/pós-edit, auditoria por dimensão |

The 13 elite gates **project onto** the 50 dims: gate `02_architecture` ↔ F1.7/F1.8/F1.12, `03_security` ↔ F2.1/F2.4/F2.5, `06_documentation` ↔ F3.8-F3.13, etc. (full mapping: strategy doc §2). Use `touring-quality` to find WHICH dimension dragged a gate down, then remediate that dim (`~/.claude/skills/touring-elite/references/quality/D{nn}.md`) and re-run `elite_aggregate.py`.

**Same 6-tier scale** (Diamond/Platinum/Gold/Silver/Bronze/Unranked) on both. Keystone catalog: `~/.claude/rules/elite-50-quality.md`. Real commands only — `touring-quality` is a **standalone binary** (hyphen), there is no `touring quality` subcommand, no `score --gate`, no `--enforce`, no `generator de qualidade dedicado (inexistente)` (PLANNED W7 → use `Edit tool`).

## Quick Usage

```bash
# Get current EliteScore
python3 docs/elite_aggregate.py --check

# Machine-readable JSON
python3 docs/elite_aggregate.py --json

# Per-gate breakdown
python3 docs/elite_aggregate.py --check | head -16

# Run a single gate
python3 docs/perf_p99_gate.py --check
python3 docs/scalability_scan.py --check
python3 docs/extensibility_scan.py --check
python3 docs/ux_audit.py --check
python3 docs/craftsmanship_tdg_gate.py --check
python3 docs/root_hygiene_gate.py --check
```

## CI/CD Integration

All 6 new gates are wired into `.github/workflows/ci.yml` under the `gates` job:

```yaml
- name: perf_p99 — P99 benchmark regression guard
  run: python3 docs/perf_p99_gate.py --check || echo "perf_p99 advisory (no baseline yet)"
- name: scalability — scan for shared mutable global state
  run: python3 docs/scalability_scan.py --check
- name: extensibility — flag kitchen-sink string-dispatch matches
  run: python3 docs/extensibility_scan.py --check
- name: ux — shell-completions coverage
  run: python3 docs/ux_audit.py --check
- name: craftsmanship — TDG grade ≥ B + cognitive_score ≤ 0.7
  run: python3 docs/craftsmanship_tdg_gate.py --check || echo "craftsmanship advisory (touring binary unavailable)"
- name: elite_aggregate — composite EliteScore
  run: python3 docs/elite_aggregate.py --check
```

**Composite score is BLOCK-by-default**: any BLOCK-tier FAIL fails the build.

## Architecture

```
                ┌─────────────────────────────────────┐
                │  L4 — Strategy (Human + LLM)        │
                │    - Goal definition (Diamond)      │
                │    - DAG via touring decompose      │
                └─────────────────┬───────────────────┘
                                  ↓
                ┌─────────────────────────────────────┐
                │  L3 — Generation (LLM worker)       │
                │    - TACO subagent (VGP + symbol    │
                │      verification table)            │
                │    - 12-factor agent (own context)  │
                │    - Constraint: NOT Edit/Write      │
                │      direct → Touring-native tooling perfect-*  │
                └─────────────────┬───────────────────┘
                                  ↓
       ┌──────────────────────────┴──────────────────────────┐
       │  L2 — Harness Gate Layer (13 gates in parallel)    │
       │                                                      │
       │   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
       │   │ architecture│  │  security   │  │ performance │  │
       │   └─────────────┘  └─────────────┘  └─────────────┘  │
       │   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
       │   │  testing    │  │documentation│  │ best_pract. │  │
       │   └─────────────┘  └─────────────┘  └─────────────┘  │
       │   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
       │   │ ci_cd       │  │modularization│ │ scalability │  │
       │   └─────────────┘  └─────────────┘  └─────────────┘  │
       │   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
       │   │extensibility│  │ craftsmanship│ │ dependencies│  │
       │   └─────────────┘  └─────────────┘  └─────────────┘  │
       │   ┌─────────────┐                                       │
       │   │ ux (16)     │  +  01/07/12/13 (clippy+rustdoc)     │
       │   └─────────────┘                                       │
       │   ┌─────────────┐                                       │
       │   │product_docs │                                       │
       │   └─────────────┘                                       │
       └──────────────────────────┬──────────────────────────────┘
                                  ↓
                ┌─────────────────────────────────────┐
                │  L1 — Observability (gate-metrics) │
                │    - composite_health_score        │
                │    - per-gate counters              │
                │    - RL reward per gate             │
                │    - Memory store (lessons)         │
                └─────────────────────────────────────┘
```

## Lessons (2026-06-13)

- **Always verify in loco** (FACT [1.0] via execution) before asserting state. My initial diagnosis cited 8 wrong facts (13 crates vs 46, cargo-deny ABSENT vs ROBUST, etc.). Cure: 8-command forensic checklist before declaring done.
- **Bash `$?` after pipe** captures the LAST command's exit, not the python script's. Use `python3 ... > /dev/null 2>&1; echo $?` for real exit code.
- **YAML breaks with `:` in quoted strings**. Use plain text or block scalar.
- **Scripts without `--json` need fallback detection**. Auto-detect + retry with script's native flag.
- **Whitelist refinements** reduce 96% of false positives (RefCell, Regex, Lazy, global allocators).
- **Composite score 0-1** surfaces overall health without noise. Single number for release-readiness.

## References

- `~/.claude/rules/touring-decision-matrix.md` — 12-task taxonomy C01-C12
- `~/.claude/rules/VP-Scout.md` — 7 verification chains
- `~/.claude/rules/TACO-subagent.md` — sequential phase protocol v6.2
- `~/.claude/CLAUDE.md` — TACO constitution
- `docs/elite_aggregate.py` — composite scorer
- `docs/scalability_scan.py` — gate 10
- `docs/extensibility_scan.py` — gate 11
- `docs/craftsmanship_tdg_gate.py` — gate 14
- `docs/perf_p99_gate.py` — gate 04
- `docs/ux_audit.py` — gate 16
- `docs/root_hygiene_gate.py` — gate 08
- `.github/workflows/ci.yml` — CI integration (6 new steps)
