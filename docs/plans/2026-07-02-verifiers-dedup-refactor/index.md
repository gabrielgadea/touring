---
type: LoopBundle
title: Verifiers dedup refactor — Loop Engineering run #1
description: Dedup boilerplate across the 50 touring-quality verifiers (F1_3 Fail → ≥ Gold), behavior-preserving.
plan_id: task_1783004397291527687
tags: [loop, refactor, touring-quality, dedup]
timestamp: 2026-07-02T15:00:00Z
okf_version: "0.1"
---

# Verifiers dedup refactor — Loop Engineering run #1

The **first real run** of the Loop Engineering engine. Scope:
`crates/touring-quality/src/verifications` (50 verifiers + `mod.rs`).

Goal: collapse the boilerplate the 50 verifiers each carry (test harness,
`DimScore` construction tail, comment/string stripping) into shared helpers,
**without changing any verifier's behavior** — driving F1.3 (duplication) from
Fail to Pass and the tier from Silver to ≥ Gold.

## Bundle contents

- [plan](/plan.md) — the strategy + DAG + measured convergence gate
- [log](/log.md) — chronological history
- [future-work](/future-work.md) — deferred option (a): data-driven verifier redesign for dir-F1.3 Gold

## Layout

```
/plan.md  /log.md  /phases/  /knowledge/  /diagnostics/
```

- `/phases/` — per-phase OKF reports (P1–P5), written by `loop_phase_close.py`
- `/knowledge/` — per-phase typed Knowledge Abstracts (Hyper-Extract hypergraphs)
- `/diagnostics/` — diagnostic digests, written by `loop_diagnose.py`
