# OKF (Open Knowledge Format) — summary for the loop

> Source: Google `knowledge-catalog/okf/SPEC.md`. This is the substrate the loop
> uses for every `.md` it writes. Full conventions in `../AGENTS.md`.

## What it is

An open, human- and agent-friendly format: **markdown + YAML frontmatter**.
Readable by humans, parseable by agents, diffable in VCS, portable — no central
schema, no special tooling.

## Bundle

A **bundle** is a directory tree of markdown files. Reserved files:

- `index.md` — directory listing (progressive disclosure).
- `log.md` — chronological update history.
- concept documents — any other `.md`.

Declare the bundle version in the root `index.md`: `okf_version: "0.1"`.

## Frontmatter

- **Required**: `type` — short string identifying the kind of concept.
- **Recommended**: `title`, `description` (one sentence), `resource` (URI),
  `tags` (list), `timestamp` (ISO 8601).
- Consumers **must preserve unknown keys** → the loop adds `plan_id` (its cross-ref
  anchor) without breaking OKF compatibility.

## Body sections (conventional headings)

- `# Schema` — structured field/column description.
- `# Examples` — usage examples in code blocks.
- `# Citations` — external sources backing claims.

## Cross-linking

- **Absolute (bundle-relative)** — begin with `/`: `[plan](/plan.md)`. Recommended
  (stable under moves).
- **Relative** — `[neighbour](./other.md)`.
- Link **semantics live in the prose**, not the link.
- Broken links to unwritten knowledge are tolerated (they mark gaps).

## IDs & versioning

- **Concept ID** = file path minus `.md` (`phases/P3.md` → `phases/P3`).
- **Version** = `<major>.<minor>`; minor = backward-compatible, major = breaking.

## Example

```markdown
---
type: PhaseReport
title: P3 — convergence gate
description: One row per convergence clause with its evidence.
plan_id: task_1782996878252842489
tags: [loop, phase]
timestamp: 2026-07-02T12:00:00Z
okf_version: "0.1"
---

# Schema
| Clause | Result | Evidence |
|--------|--------|----------|
| decompose ready empty | ✅ | `touring decompose ready` → [] |

Part of the [bundle](/index.md).
```
