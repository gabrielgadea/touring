---
type: LoopBundle
title: Loop Engineering MVP — knowledge bundle
description: OKF bundle for the Loop Engineering MVP build (skill + scripts + hooks) and its dogfood validation run.
plan_id: task_1782996878252842489
tags: [loop, loop-engineering-mvp, okf]
timestamp: 2026-07-02T00:00:00Z
okf_version: "0.1"
---

# Loop Engineering MVP — OKF bundle

This directory is the **OKF bundle** for the Loop Engineering MVP. Every `.md`
here is an OKF document carrying `plan_id: task_1782996878252842489`. Progress of
record lives in the Touring DAG (`touring decompose status task_1782996878252842489`)
and memory (`touring memory recall "loop-engineering"`); this bundle is the
human-readable, diffable mirror.

## Documents

| Document | Type | Description |
|---|---|---|
| [/plan.md](/plan.md) | Plan | The 8-phase MVP plan, rendered from the DAG. |
| [/log.md](/log.md) | Log | Chronological history (phase-closes + human-gate decisions). |
| [/phases/](/phases/) | PhaseReport | One OKF report per phase (written at phase-close). |
| [/knowledge/](/knowledge/) | KnowledgeAbstract | Per-phase typed hypergraph (Hyper-Extract style). |
| [/diagnostics/](/diagnostics/) | Diagnostic | Deep-diagnostic digests (`loop_diagnose.py`). |
| [/checkpoints/](/checkpoints/) | Provenance | taco-forge TOON checkpoints (discover / plan_spec). |

## Phases (DAG `task_1782996878252842489`)

| Phase | Deliverable | Depends on |
|---|---|---|
| P1 | OKF bundle + skill scaffold + `AGENTS.md` | — |
| P2 | `loop_diagnose.py` | P1 |
| P3 | `loop_converged.py` (convergence gate) | P1 |
| P4 | `loop_phase_close.py` | P1, P3 |
| P5 | `loop_doc_link_gate.py` | P1 |
| P6 | Stop + PreCompact hooks (`settings.json`) | P3 |
| P7 | `SKILL.md` body wire-all | P2,P3,P4,P5,P6 |
| P8 | Dogfood validation run (50-verifier boilerplate → Silver→Gold) | P7 |

## Skill

The engine itself lives at `~/.claude/skills/loop-engineering/`
([SKILL.md](file:///home/gabrielgadea/.claude/skills/loop-engineering/SKILL.md),
[AGENTS.md](file:///home/gabrielgadea/.claude/skills/loop-engineering/AGENTS.md)).
