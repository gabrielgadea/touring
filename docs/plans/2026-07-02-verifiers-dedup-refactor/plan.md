---
type: Plan
title: Verifiers dedup refactor — plan
description: Strategy + DAG to dedup the 50 verifiers' boilerplate, behavior-preserving (F1_3 → ≥ Gold).
plan_id: task_1783004397291527687
tags: [loop, plan, refactor]
timestamp: 2026-07-02T15:00:00Z
okf_version: "0.1"
---

# Plan — dedup the 50 touring-quality verifiers

Part of the [bundle](/index.md). History at [/log.md](/log.md).

## Problem

Each of the 50 verifiers in `crates/touring-quality/src/verifications/` carries
near-identical boilerplate. F1.3 (Type-1 clone detection over the concatenated
directory blob) sees every block repeated ~50× and scores duplication high enough
to Fail — capping the whole crate at **Silver** despite a **0.9512** composite.

Measured duplication (census):

| Boilerplate | Files | Occurrences |
|---|---|---|
| Test harness `write_temp` + `use std::io::Write` + `use tempfile::NamedTempFile` | ~48–50 | 79 / 53 / 50 |
| `DimScore { … latency_ms: 0 }` tail + `from_score` + `vec![auto_remediation(…)]` | 50 | 50–55 |
| `strip_rust_comments_and_strings` (own copy) | 2 (f4_3, f1_8) | 2 |

## Objective — measured convergence gate

```
CONVERGED ⟺  F1_3 Fail → Pass
         AND tier Silver → ≥ Gold          (loop_converged: tier ∈ {Gold,Platinum,Diamond})
         AND composite ≥ 0.95 preserved
         AND cargo check + test + clippy    → green
         AND 0 new orphans (baseline 5044)
         AND behavior IDENTICAL             (every test green today stays green)
```

## DAG (`task_1783004397291527687`)

| Phase | What | Proof |
|---|---|---|
| **P1** | Census + design the shared modules (`source_scan`, `testkit`, `finish` helper) | read-only + target structure |
| **P2** | Extract `strip_rust_comments_and_strings` → `verifications::source_scan`; rewire f4_3/f1_8 | cargo check + test green |
| **P3** | `fn finish(id, value, evidence, target) -> DimScore` in `mod.rs`; rewire 50 `check()` tails | cargo check + test green |
| **P4** | Extract `write_temp`/`NamedTempFile` → shared `#[cfg(test)] testkit`; rewire 50 test mods | cargo test green |
| **P5** | Converge + close: re-score, cargo full, cross-audit, phase-close OKF | `loop_converged.py` exit 0 |

Deps: `P1 → {P2 → P3 → P4} → P5`.

## Constraints (REFACTOR MODE)

- **Never change behavior — only structure.** Extracted helpers are literal moves.
- Edits via `taco-forge perfect-edit` (REGRA #14), not raw Edit.
- Each phase = one reviewable, behavior-preserving transformation; tests green between.
