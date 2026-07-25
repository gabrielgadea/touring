# AGENTS.md — Loop bundle maintenance manual (OpenKB-style)

> Read at runtime by the loop when writing or reconciling the OKF bundle. This is
> the "instruction manual for maintaining the wiki" (OpenKB pattern): edit it and
> the change takes effect on the next phase-close — no recompilation.

## What the bundle is

A **loop run** produces one **OKF bundle** rooted at its plan dir
(`docs/plans/<date>-<slug>/`). It is a directory tree of OKF `.md` documents plus
typed Knowledge Abstracts. It is the human-readable, diffable mirror of the DAG +
memory. Treat it as **append-mostly**: prefer new phase reports + `log.md` entries
over rewriting history.

## Every `.md` is an OKF document

Required frontmatter on **every** `.md` in the bundle:

```yaml
---
type: <LoopBundle | Plan | PhaseReport | Diagnostic | Strategy | Doc>
title: <human-readable>
description: <one sentence>
plan_id: <task_id>            # ← the cross-ref anchor (step 17). MANDATORY.
tags: [loop, <slug>, ...]
timestamp: <ISO 8601>
okf_version: "0.1"
---
```

`plan_id` is the invariant that ties every document to its plan — the
`loop_doc_link_gate.py` gate FAILS any bundle `.md` missing it.

## Cross-linking rules

- Use **bundle-relative absolute** links: `[the plan](/plan.md)`,
  `[phase P3](/phases/P3.md)` — stable under moves. Prefer these.
- Entity/concept references use `[[wikilinks]]` (Obsidian/Hyper-Extract compatible).
- Link semantics live in the **prose**, not the link (OKF convention).
- A broken link to unwritten knowledge is tolerated (it marks a gap) — but a
  broken link to a doc that *should* exist is a lint finding.

## Reserved files (bundle root)

- **`index.md`** — the bundle listing: `okf_version`, `type: LoopBundle`, a table
  of every document with its `type` + one-line description (progressive
  disclosure). Update it when a new doc is added.
- **`log.md`** — chronological history, newest first, ISO 8601 + prose. One entry
  per phase-close and per human-gate decision. This is the audit trail.

## Page types (OpenKB synthesis, at CLOSE)

When compiling the wiki (step 16), maintain three page kinds:

- **Summary pages** — one per phase (`phases/P<n>.md`): what was done, evidence,
  gates passed.
- **Concept pages** — synthesise across phases (e.g. "convergence", "doc-pipeline").
- **Entity pages** — one per durable entity (a key symbol, a decision, a gotcha),
  with `[[wikilinks]]` in and out.

## Knowledge Abstracts (Hyper-Extract structuring, at phase-close)

Each phase emits `knowledge/P<n>.json` — a typed hypergraph:

```json
{
  "entities": [
    {"entity_id": "<canonical-name>", "type": "decision|learning|gotcha|symbol|phase", "description": "..."}
  ],
  "relations": [
    {"relation_id": "{source}|{type}|{target}", "source": "...", "type": "produces|blocks|refines|touches", "target": "..."}
  ]
}
```

IDs are **deterministic** (derived from canonical name, not order/memory —
REGRA #17). Re-running an extraction on the same input yields the same ids, so
the graph is diffable and mergeable across runs (this is what compounds in L4).

## Lint rules (OpenKB `lint` — contradiction detection)

`loop_doc_link_gate.py` flags:

- **Orphan doc** — a bundle `.md` with no `plan_id` or not listed in `index.md`.
- **Broken link** — a bundle-relative link that does not resolve.
- **Contradiction** — two docs asserting conflicting facts about the same
  `entity_id` (e.g. a phase reported `done` in `log.md` but `pending` in the DAG).
  Contradictions are the drift signal (co-evolution law) — surface, don't hide.

## Maintenance discipline

1. New doc → add OKF frontmatter (with `plan_id`) → register in `index.md` → append `log.md`.
2. Phase-close → write `phases/P<n>.md` + `knowledge/P<n>.json` + `log.md` entry.
3. Never rewrite `log.md` history; append only.
4. Run `loop_doc_link_gate.py` before declaring the loop converged (step 17).
