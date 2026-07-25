---
name: loop-engineering
description: Use when running a long, multi-phase engineering task on the Touring workspace that must iterate until "complete AND perfect" — a hybrid-autonomy loop that recalls memory, runs a deep Touring diagnostic, plans+decomposes into a DAG, executes phases with cross-audit + 50-dim gates, persists OKF-compliant knowledge each phase, survives compaction, and only exits when a MEASURABLE convergence gate passes. Invoke for "run until done", phased refactors/migrations/audits, or any goal too large for one pass.
---

# Loop Engineering — an iterate-until-converged engine over Touring

> **Thesis** (LangChain, *The Art of Loop Engineering*): *"the potential in agents is in the loops you build around them."* Value accrues from **structural iteration**, not a single pass. This skill is the loop harness: it stacks a plan→act→observe→**verify**→**converge** cycle over Touring's primitives and only stops when "done" is *measured*, not felt.

## When to activate

- A goal spans **3+ phases** or is "run until complete and perfect".
- Refactors/migrations/audits that need repeated diagnose→act→verify.
- Any task where **convergence must be proven** (tests+quality+wiring), not asserted.

**Skip** for single-file edits, quick questions, or L0-L1 trivial tasks — the loop's overhead only pays off across phases.

## The four stacked loops (what this skill implements)

| Loop | Role | Touring realization |
|---|---|---|
| **L1 Agent** | plan → act → observe → iterate | master commands + `Edit`/`Write` + `touring decompose` |
| **L2 Verification** | score against a rubric; retry-on-feedback | `TACO-cross-audit` + `touring-quality` 50-dim + `touring wiring audit` |
| **L3 Event** | external triggers (optional) | `touring jobs` / cron (deferred; not in MVP) |
| **L4 Hill-climb** | traces improve the harness | `touring diary`/`learning`/`evolution drift` |

## Durable state — the checkpointer (survives compaction)

The loop is **resumable**. Its state is a `thread_id`-style ledger, triple-redundant so it survives `/clear`, compaction, and new sessions:

- **`touring decompose`** DAG = the task graph + per-subtask status (`pending|in_progress|done`) — the *authoritative* progress.
- **`touring memory`** (semantic tier) = decisions, learnings, gotchas, the convergence snapshot.
- **OKF bundle `.md`** (the plan dir) = human-readable, diffable mirror (`index.md`+`log.md`+phase reports).

On **PreCompact**, the `loop-snapshot` hook writes `{plan_id, phase, iteration, convergence_snapshot, next_ready}` so the next context resumes cleanly. On resume: `touring decompose ready <task>` + `touring memory recall "loop-state:<plan_id>"` reconstitute everything.

## The loop (outer → inner → close)

> **v2 reframe (F6.1, ADW plan 2026-07-19)**: the loop is the **convergence
> discipline INSIDE the stages of an ADW**, not a rival orchestrator. The OUTER
> stages 1-11 are the explore↔plan convergence pair — prefer `touring explore`
> (F1, CCE ledger) + `plan_refine.py` (F2, refine-to-plateau) as their engines.
> The INNER delegates execution to `touring adw run <name>` whenever a library
> ADW covers the phase (bugfix/chore/feature/hotfix/audit/explore-plan/
> scout-perpetuo); the manual INNER below remains the fallback for phases no
> ADW covers yet. Law L2 either way: code, never the LLM, ends the loop.
>
> **v3 enforcement (2026-07-23, origin `protocol-adherence-diagnosis`)**: the
> OUTER steps 1-5 are now ONE deterministic command — `touring adw run
> strategy-loop --var topic=… --var scope=… --var bundle=…` — and the whole
> OUTER is **gated**: invoking this skill arms a per-project marker
> (`loop_outer_arm.py`, UserPromptSubmit) and the Stop hook refuses to end a
> turn until the flow's artifact manifest (`hooks/flow_manifests.json`:
> diagnostic doc + CCE ledger + strategy doc) exists on disk
> (`loop_outer_gate.py`) — artifacts, never narrative (Law L3).

```
OUTER (1× per goal) — steps 1-5 are ONE command (deterministic, ADW-enforced):
  1-5  touring adw from-template strategy-loop 2>/dev/null;  # instantiate 1× per project
       touring adw run strategy-loop --var topic="<goal>" \
         --var scope="<project root>" --var bundle="<plan bundle dir>"
       (= arm outer marker → memory recall → loop_diagnose.py →
          touring explore --until-dry (CCE) → evidence report)
       Context7 stays a manual lens: mark it on the ledger with
       touring explore "<goal>" --mark-lens external:visited --note "<source>"
  6  Strategy (sequential-thinking) consolidate into an intent
  7  Persist strategy             touring memory store + OKF strategy-<date>-<slug>.md (bundle)
  8  Present strategy + objectives
  9  ██ HUMAN GATE ██             plan approval (LangGraph interrupt analog)
       (the Stop guard blocks any turn ending before diagnostic + ledger +
        strategy doc exist — hooks/flow_manifests.json, verdict by artifacts)
  10 Plan + decompose             /plan or taco-planning skill  → chunked plan.md
  11 Register DAG                 touring decompose add … --depends-on   (or --auto-populate)

INNER (repeat per phase until CONVERGED)
  →  next = touring decompose ready <task>   (topological, deps satisfied)
  12 Execute phase                Edit/Write + touring-engineer / touring index+ast+wiring
  13 Cross-audit + 50-dim         TACO-cross-audit + touring wiring audit + touring-quality
  ██ VERIFY/REFLECT gate ██       passed the rubric? if NO → retry with the failure as feedback
                                  + self-critique: "did this fulfil the INTENT? what is missing?"
  14 Phase-close                  scripts/loop_phase_close.py <task> <phase>
                                    → memory store + learning reward + decompose finalize
                                    + OKF phase report + Hyper-Extract typed abstract
  ██ CONVERGENCE gate ██          scripts/loop_converged.py <scope> <task>   (see below)
  15 → next phase, OR exit if converged
  ██ BUDGET/circuit-breaker ██    max iters/phase; diminishing-returns → stop + ask

CLOSE (1×)
  16 Documentation                touring-scriber → OKF docs (code/manual/architecture)
  17 Validate cross-ref           scripts/loop_doc_link_gate.py <bundle>   (every .md → plan)

META (optional, L4 hill-climb)
     mine traces → touring diary/learning/evolution → improve this harness
```

## The convergence gate — "complete AND perfect" is MEASURED

The loop exits **only** when `scripts/loop_converged.py` returns exit 0. All clauses must hold (grader-driven convergence — the blog's core discipline):

```
CONVERGED ⟺  touring decompose ready <task>        → empty (all subtasks done)
         AND touring-quality score <scope> --workspace --fail-below 0.80  → pass (≥ Gold)
         AND 0 dims P0 BLOCK in Fail                (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5)
         AND touring wiring orphans                 ≤ baseline (REGRA #0)
         AND cargo check + test + clippy            → green (Rust scopes)
         AND TACO-cross-audit purpose-fidelity      → clean (purpose fidelity)
```

Not converged → the script emits `{unmet:[…], next_action:"…"}`; the loop continues on the next ready subtask. This is what makes "perfect" terminate instead of looping forever.

## Human-in-the-loop — hybrid autonomy (3 gates)

Autonomous **within** a phase; pause for a human at:

1. **Strategy → plan** (step 9) — approve the decomposition before building.
2. **Before irreversible / outward actions** — deploy, `git` (REGRA #11 — never auto), external writes, `settings.json` changes, anything hard to reverse.
3. **Final sign-off** (optional) — before declaring converged & closing.

Everything else (new files, in-workspace edits, diagnostics, audits) runs autonomously.

## Documentation pipeline — OKF · Hyper-Extract · OpenKB

Three open standards stack into one knowledge pipeline (details + conventions in `AGENTS.md`):

- **OKF** (Open Knowledge Format) — the **substrate**. Every `.md` the loop writes is an OKF document: YAML frontmatter (`type`, `title`, `description`, `tags`, `timestamp`) + body + **bundle-relative cross-links** (`/plan.md`). The plan dir is an **OKF bundle** (`index.md` listing + `log.md` history). Satisfies step 17 with a real standard.
- **Hyper-Extract** — the **structuring** engine. At phase-close, findings become a **typed Knowledge Abstract** (a hypergraph: nodes = decision/learning/gotcha/symbol/phase; edges = typed relations) with **deterministic** ids (`entity_id`, `relation_id = {source}|{type}|{target}` — aligns with REGRA #17), exported as `[[wikilinks]]`.
- **OpenKB** — the **synthesis** layer. At close, compile the bundle into an interlinked wiki (summary/concept/entity pages + auto cross-refs) and **lint for contradictions** (aligns with the co-evolution/drift law). This is also the KB that **compounds across runs** (L4).

**MVP policy**: adopt OKF fully; implement Hyper-Extract + OpenKB **patterns natively** in the scripts (no external Python deps). Integrating the real tools (Hyper-Extract's 10+ engines, OpenKB's `watch`/`lint` pipeline) is a post-MVP phase.

## Scripts (Layer-3 — deterministic composition, `scripts/`)

| Script (real CLI) | Replaces (atomic sequence) | Contract |
|---|---|---|
| `loop_diagnose.py --scope <path> [--topic <s>] [--bundle <dir>] [--plan-id <id>]` | `touring status`+`touring-quality --workspace`+`wiring orphans`+`memory recall`+`touring map` | one-shot diagnostic → `{health, quality50, wiring, memory, structure}` JSON + an OKF `Diagnostic` doc in `<bundle>/diagnostics/` |
| `loop_converged.py --task <id> --scope <path> [--bundle <dir>] [--rust-full]` | the 6 convergence clauses above | exit 0 (converged) / 1 (continue) + JSON `{converged, clauses, unmet[], next_action}`. **Fail-CLOSED**: `dag_done` + applicable Rust clauses never pass on missing evidence |
| `loop_phase_close.py --task <id> --phase <Pn> --summary "…" [--status done] [--bundle <dir>] [--gates <json>] [--abstract <json>] [--reward <f>]` | decompose update + memory store + learning reward + OKF report + typed abstract + log | closes a phase; writes `phases/<Pn>.md` (OKF `PhaseReport`) + `knowledge/<Pn>.json` (Hyper-Extract hypergraph, deterministic ids) + appends `log.md` |
| `loop_doc_link_gate.py --bundle <dir> [--strict]` | OKF frontmatter/link validation + OpenKB contradiction lint | exit 0 clean / 1 blocking + JSON `{missing_type[], missing_plan_id[], broken_links[], orphan_docs[], contradictions[]}` |

Every script: `--help`, `--json`, `--quiet`; fail-open when the daemon is degraded. Prefer the script over re-deriving its N-call sequence by hand.

**Touring Diagnostic Arsenal** (`~/.claude/skills/Touring/scripts/`, shared) feeds the loop's diagnose (step 2) and multi-axis convergence: `systemic_diag_v2.py [scope]` (50-dim × arch-blast × security fused → the integrated risk the convergence gate scores), `crate_50dim_matrix.py <crate>` (lossless per-dim evidence for a phase target), `workspace_arch_diag.py` / `crate_arch_diag.py` (cycles + God-objects, the architecture clauses), `clone_blocks.py <file>` (classify a dedup phase before acting). Each is scope-able (crate/dir/file) so a phase can converge on its own target. Set `DIAG_OUT=<bundle>/diagnostics` to file the matrices with the run. **Reporting Contract (MANDATORY)**: every arsenal diagnostic run is relayed as the full 7-section elite audit report (never a single-lever summary) — spec + enforcement in `~/.claude/skills/Touring/scripts/report_contract.py`, printed as each digest's footer.

## Hooks (`settings.json`, registered)

- **Stop** → `scripts/hooks/loop_stop_guard.py` — resolves the **per-project** active marker (scoped to the session cwd); only when the daemon POSITIVELY confirms the task exists with pending subtasks AND `loop_converged.py` exits 1 does it emit `{"decision":"block","reason":"…next_action…"}` (converge-or-continue). On convergence it **archives** the marker; an orphaned/missing DAG **fails OPEN** (release + archive). Capped at `MAX_CONTINUATIONS=30` per run (runaway guard).
- **PreCompact** → `scripts/hooks/loop_snapshot.py` — snapshots pending subtasks to memory (`loop-state:<task>`) + `log.md` so the loop resumes after compaction (the checkpointer analog).
- **UserPromptSubmit** → `scripts/hooks/loop_outer_arm.py` — detects a gated-flow invocation (`/loop-engineering`, `/goal` → `strategy-outer`; `/TACO-cross-audit` → `cross-audit`) and arms the per-project marker with `status: "outer"` BEFORE any DAG exists. From that moment the Stop hook verifies the flow's artifact manifest (`hooks/flow_manifests.json` via `hooks/loop_outer_gate.py`, capped at the manifest's `max_continuations`) instead of the DAG, and every evaluation appends one record to `~/.claude/loop-engineering/compliance.jsonl` (the flow-KPI feed). A live loop (real task, `status: active`) is never clobbered by arming.
- All three share `scripts/hooks/loop_marker.py` — the per-project marker helper (scope/read/write/archive/TTL).

All are **loop-scoped** (no-op unless *this project's* marker exists) and **fail-open** (any error, an undeterminable daemon, or an orphaned DAG → exit 0, never block the session). Registered by appending to the existing `.hooks.Stop` / `.hooks.PreCompact` / `.hooks.UserPromptSubmit` arrays (never replacing) — **always as `python3 <path>`, never a bare script path**: a direct path depends on the execute bit, and a silently missing `+x` made every UserPromptSubmit fail with Permission denied on 23/07/2026 (`test_registered_hook_commands_are_runnable` now guards every registered command as-registered).

## Activation & resume — the `thread_id`

A loop is **active** iff a **per-project** marker exists at
`~/.claude/loop-engineering/active-<sha1(cwd)[:12]>.json` (keyed by the run's cwd
— *never* a global singleton, so concurrent loops in different projects never
clobber each other and a Stop event in project B never gates on project A's loop).

Write it with the helper (guarantees `cwd` + timestamps + `status`), never by hand:

```bash
scripts/hooks/loop_marker.py write --task <task_id> \
    --scope "<path scored for convergence>" --bundle "<OKF bundle dir>"
# fields: {task, scope, bundle, cwd, status, continuations, created_at, updated_at}
```

- **OUTER arming is automatic** — invoking the skill writes the marker with `status: "outer"` (no task yet; `flow` selects the manifest); `strategy-loop` refreshes it with the bundle, and step 11 upgrades it to `status: "active"` with the real DAG task. An outer marker obeys the same TTL and goes quiet once its manifest is complete (`outer_complete: true`).
- **Start** a loop → `loop_marker.py write …` (after the plan+DAG exist). The Stop hook now enforces convergence; PreCompact now snapshots.
- **Resume** after compaction/`/clear` → `touring decompose ready <task>` + `touring memory recall "loop-state:<task>"` reconstitute the state; the marker keeps the hooks live.
- **End** a loop → convergence archives the marker automatically; or `loop_marker.py archive` to abandon a run. A marker not updated in **24h (TTL)** or stamped `CONVERGED`/`ARCHIVED` is inert — no future session's Stop is ever held by a stale run.

## Bundle layout (the OKF bundle for a run)

```
docs/plans/<date>-<slug>/          ← OKF bundle root
  index.md                         ← bundle listing (okf_version, type: LoopBundle)
  log.md                           ← chronological history (ISO 8601 + prose)
  plan.md                          ← the plan (OKF doc; rendered from the DAG)
  phases/P<n>.md                   ← per-phase OKF reports (loop_phase_close)
  knowledge/P<n>.json              ← per-phase typed Knowledge Abstract (Hyper-Extract)
  diagnostics/<ts>.md              ← diagnostic digests (loop_diagnose)
  checkpoints/*.toon               ← phase-close provenance (loop_phase_close)
```

## Golden rules

1. **Convergence is measured, never asserted** — `loop_converged.py` exit 0 is the only "done".
2. **State is durable** — DAG + memory + OKF bundle; assume compaction will happen mid-loop.
3. **Verify after every phase** — cross-audit + 50-dim before phase-close (L2).
4. **Human-gate the irreversible** — plan, deploy, `git`, `settings.json`, external writes.
5. **Every `.md` is an OKF doc linked to the plan** — the doc-link gate enforces it (step 17).
6. **Persist the pheromone** — `memory store` + `learning reward` each phase (Learning Memory pillar).

## Cross-references

| Topic | Local |
|---|---|
| Bundle maintenance manual (OpenKB-style) | `AGENTS.md` |
| OKF format summary | `references/okf-format.md` |
| Touring master commands + decision matrix | `~/.claude/rules/touring-decision-matrix.md` · `~/.claude/skills/Touring/SKILL.md` |
| Purpose-fidelity cross-audit | `~/.claude/skills/TACO-cross-audit/SKILL.md` |
| 50-dim quality harness | `~/.claude/rules/elite-50-quality.md` |
| This MVP's plan + DAG | `~/projects/touring/docs/plans/2026-07-02-loop-engineering-mvp/` (`task_1782996878252842489`) |

---

_v0.3 (flow enforcement) — 2026-07-23 | Loop Engineering over Touring. The art is in the loop; the discipline is in the convergence gate; the guarantee is in the artifact manifest._
