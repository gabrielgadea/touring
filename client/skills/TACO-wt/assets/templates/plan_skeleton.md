---
plan: <plan-name>            # kebab-case, used in directory paths
title: <Human-Friendly Title>
authored: 2026-MM-DD
status: DRAFT                # DRAFT | ACTIVE | COMPLETE | ARCHIVED
total_waves: 12
total_engineer_days: 120
critical_path:               # ordered list of waves on the critical path
  - W01
  - W04
  - W07
  - W12
wave_weights:                # optional, default 1.0 for every wave
  W01: 1.0
  W11: 2.0                   # double-weight a critical wave
  W14: 0.5                   # half-weight a documentation wave
quality_dimensions:          # canonical 9 dimensions from pln2_generator
  - precision
  - scalability
  - performance
  - functionality
  - code_quality
  - detail
  - integration
  - dependencies
  - potentiation
---

# <Human-Friendly Title>

> **Intent**: <one-paragraph statement of what this plan accomplishes>
> **Motivation**: <why now? what does the team gain?>
> **Success metric**: <one measurable thing — e.g., "75% coverage" or "0 cycles">

---

## Overview

<2-4 paragraphs explaining the high-level shape of the plan: the waves, the
order, the riskiest bits, and where the parallelism opportunities are.>

---

## Wave dependency DAG

```
W01 ──► W02 ──► W04 ──┐
            │          ├──► W07 ──► W11 ──► W12
            └► W03 ──► W05 ──► W06 ┘
                      │
                      └──► W08 ──► W09 ──► W10
```

`plan_validator.py --check-cycles` enforces this DAG (Kahn's topological sort).

---

## Waves

### W01 — <Wave title>

| Field | Value |
|-------|-------|
| Status | PENDING |
| Engineer-days | 5 |
| Critical path? | yes |
| Depends on | — |
| Severity | P0 |
| Sub-scripts | `W01_discover.py`, `W01_apply.py`, `validate_W01.py` |
| Quality dimensions | precision, integration |

**Objective**: <one sentence>

**Outcomes**:
- <bullet, measurable>
- <bullet, measurable>

**Premises to verify** (per L6, every wave re-measures its premises):
- <premise — verified by `W01_discover.py`>

**Risks**:
- <risk — mitigation>

---

### W02 — <Wave title>

| Field | Value |
|-------|-------|
| Status | PENDING |
| Engineer-days | 8 |
| Critical path? | no |
| Depends on | W01 |
| Severity | P1 |
| Sub-scripts | `W02_discover.py`, `W02_apply.py`, `validate_W02.py` |
| Quality dimensions | functionality, code_quality |

**Objective**: <one sentence>

<...continue for every wave...>

---

## Cross-audit gates

The plan is considered COMPLETE when:

| Gate | Tool | Criterion |
|------|------|-----------|
| Compilation | `cargo check --workspace` | 0 errors |
| Lint | `ruff check`, `cargo clippy -D warnings` | 0 |
| Tests | `pytest scripts/<plan>/`, `cargo test` | all pass |
| Wave validators | `validate_W<N>.py` per wave | all `PASS` |
| Cross-audit composite | `cross_audit.py` (normal mode) | ≥ 0.8 |
| Evidence completeness | `evidence_collector.py --strict` | 0 missing |
| TOON checkpoint | `toon_checkpoint.py emit --phase plan-complete` | valid hash chain |

---

## Lessons applied from prior plans

The plan author MUST document which TACO-wt lessons (L1-L10) inform decisions
here. Example:

- **L4**: cross-audit uses `--baseline` mode for the first 30 days; transitions to normal mode once W01-W03 are PASS.
- **L6**: every wave's first sub-script is `discover` — never an `apply`.
- **L9**: `--apply` flag is opt-in; runner defaults to dry-run.

---

## References

- TACO-wt skill: `~/.claude/skills/TACO-wt/SKILL.md`
- Lessons: `~/.claude/skills/TACO-wt/references/lessons.md`
- Pipeline patterns: `~/.claude/skills/TACO-wt/references/pipeline-patterns.md`
- Cross-audit protocol: `~/.claude/skills/TACO-wt/references/cross-audit-protocol.md`
- Orchestration patterns: `~/.claude/skills/TACO-wt/references/orchestration-patterns.md`
