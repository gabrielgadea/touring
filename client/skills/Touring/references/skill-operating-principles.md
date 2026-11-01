# Skill Operating Principles — the five a TACO/Touring skill must instantiate

> **Load on demand** | **Version**: v1.0 | **Date**: 2026-08-20 | **Authority**: Gabriel Gadea
> **Canonical**: this file. A skill DERIVES its own instance from here and links back —
> it never restates the principle, and never carries a generic banner
> (`rules/touring-4-pillars.md`: *"A generic banner does not induce — it is debt"*).
> **Enforced by**: `~/.claude/skills/TACO-skilling/scripts/skill_principle_gate.py`

Five principles, each with a **real mechanism** behind it. A principle without a command
is an opinion; every row below names the command that makes it true, and every command
below was executed on 2026-08-20 to confirm it exists and its flags are as written.

| # | Principle | Mechanism (verified) | The failure it prevents |
|---|---|---|---|
| **P1** | **Compose, never copy** | `[[use]]` + `touring adw fragments` (13 live) | the copy that drifts from its original |
| **P2** | **Convergence is measured** | `loop_converged.py` — exit 0 is the only "done" | "done" as self-assessment |
| **P3** | **The gauntlet judges blind** | `critic-panel` fragment, code-counted quorum | the author grading their own work |
| **P4** | **The graph is flat and inspectable** | `touring adw explain <name>` | a workflow nobody can read before running |
| **P5** | **Wayfinder: claim, ticket, frontier** | `touring decompose claim\|ticket\|frontier` | two sessions doing the same subtask |

---

## P1 — Compose, never copy (ADW)

`[[use]] module/as/with` **inlines** a fragment under a namespace (`recall.memory`), so one
correction to the fragment reaches every flow that uses it. The kit is **13 fragments**, not
the 11 the constitution still names (measured `touring adw fragments`, 20/08/2026):

| module | entry | inputs |
|---|---|---|
| `recall-pack` | `memory` | topic, area |
| `prior-art` | `portfolio` | intent |
| `diagnose-pack` | `investigate` | symptom, target |
| `graph-pack` | `relations` | symbol, domain |
| `fanout-lenses` | `sweep` | target, lenses |
| `gate-rust` | `rust_gate` | crate |
| `gate-quality50` | `quality_gate` | target, floor |
| `conflict-guard` | `conflict_check` | write_set |
| `human-approve` | `approve` | message |
| `phase-close` | `persist` | key, summary, domain |
| `converge` | `converged` | task, scope |
| `critic-panel` | `panel` | artifact, bar, quorum |
| `worker-critic-pair` | `work` | task, artifact, bar |

- **MUST** — a skill whose procedure is already a fragment cites the fragment. Restating it
  in prose creates a second implementation that no correction will ever reach.
- **SHOULD** — new flows are built with `touring adw new --use <module>[:<as>]`, whose
  prior-art step demands an explicit verdict before a new flow is admitted.
- **Evidence, not intent**: `touring adw explain <name>` prints the graph AFTER inlining.

> **Origin (measured)**: a mass copy on 25/07/2026 produced a mirror nobody compared. The
> shell-injection fix then reached `client/` and the instantiated flows but **never the
> deployed library** `adw from-template` copies from. Copies do not inherit corrections.

## P2 — Convergence is measured, never asserted (loop-engineering)

`loop_converged.py --task <id> --scope <path>` is the judge. **Exit 0 is the only "done".**
Its clauses, read out of `_gather_clauses` by AST (not from this prose — the list drifted
once already and nothing reconciled it):

`judge_intact` · `dag_done` · `quality_gold` · `no_p0_fail` · `measured_whole_scope` ·
`orphans_base` · `cargo_green` · `cross_audit`

- `judge_intact` runs **first**: every other clause is worth exactly what the grader that
  scored it is worth (arXiv:2505.22954 App. H — an agent scored *perfect* by deleting the
  markers its detector counted). Deliberate change is one dated command:
  `judge_attest.py --attest --why "…"`.
- **MUST** — a skill that declares completion names the gate that decides it. A checklist
  marked by self-assessment is the exact failure mode Law L3 exists to eliminate.
- **MUST** — a gate registered as a hook declares its own worst case and is registered with
  a timeout that **dominates** it (`loop_stop_guard.SELF_BUDGET_SECONDS`, guarded by
  `test_registered_stop_timeout_dominates_the_guard_budget`).

> **Origin (measured 20/08/2026)**: the Stop guard was registered with `timeout: 20` while
> the gate it ran took **24.0 s**. It was killed before printing a verdict on every Stop —
> for 54 consecutive evaluations — and looked perfectly healthy when run by hand, which has
> no timeout. **A gate slower than its own timeout is not a gate.** The fix was also a
> measurement: the blocking half is settled by `touring decompose ready` in **0.00 s**, so
> the expensive gate now runs only where it decides something (24.01 s → 0.03 s).

## P3 — The gauntlet judges blind (critic-panel)

The `critic-panel` fragment runs N critics with **distinct lenses**, each in a fresh session
(`session = "fresh"` — a critic that saw the work being made is not blind), and the verdict
is a **quorum counted by code**, never a narrative synthesis.

- **MUST** — a skill that AUDITS routes its verdict through ≥2 distinct lenses with a
  code-counted quorum. Diversity, not redundancy: N identical critics find one failure mode.
- **SHOULD** — per-unit work uses `worker-critic-pair` (one worker, its own blind critic,
  iterating to a declared bar) instead of a panel at the end.
- **The panel carries its own warning**: it *only pays off AFTER the brief is right*. A
  gauntlet over a wrong brief buys N confident answers to the wrong question.

## P4 — The graph is flat and inspectable (graph engineering)

- `touring adw explain <name>` renders the **flat resolved graph** — what will actually run
  after every `[[use]]` is inlined. `--cost` adds the estimate.
- `[purpose].when_not_to_use` is the field that lets the portfolio **discard** a flow rather
  than recommend the nearest one; the miner indexes it BEFORE the header so the 600-char
  ceiling falls on boilerplate, not on the discriminating text.
- Fan-out has a **static** form (`branches = [...]`) and a **dynamic** one
  (`branches = "{{vars.x}}"` + `template`, cloned by value at runtime); `max_branches` is
  checked **before** any branch runs.
- **MUST** — a skill that fans out declares `on_branch_fail`
  (`all|any|ignore|best_effort|quorum:N`). An undeclared policy is silently `all`.

## P5 — Wayfinder: claim, ticket, frontier

| Command | What it does | Why it is not optional |
|---|---|---|
| `touring decompose claim <task> --owner <id> [--lease-secs N]` | conditional UPDATE — exactly one owner wins, lease expires if the session dies | `ready` only **reads**: two sessions polling it receive the **same** subtask |
| `touring decompose ticket <task> <subtask> --kind decision\|implementation --subtype research\|prototype\|grilling\|task --autonomy hitl\|afk --fog clear\|hazy\|unknown --origin-ticket <id>` | classifies whether the work is here to DECIDE or to BUILD, and how much fog surrounds it | unclassified work is planned as if its unknowns were resolved |
| `touring decompose frontier <task>` | partitions unblocked work into open **decisions** and the implementation they gate | open decisions **block** the implementation frontier; unevaluated fog reports `unknown`, never `clear` |

- **MUST** — a skill that hands work to a worker (subagent, session, ADW node) claims it.
  Every field of `ticket` is optional so a ticket is **refined as fog lifts**.
- **MUST NOT** — report `clear` for fog that was never evaluated. `unknown` is a state.

---

## How a skill instantiates a principle (the anti-banner rule)

A **derived instance** names the skill's own commands, symbols and failure modes:

```markdown
> **P5 (Wayfinder)** — this skill's phases are claimed, not read:
> `touring decompose claim <task> --owner engineer-<n>` before FASE 5 dispatches an
> engineer, so two orchestrators never dispatch the same phase. See
> `references/skill-operating-principles.md#p5`.
```

A **banner** is the same sentence pasted into fifteen files. It induces nothing, it is not
specific to anything, and it is the exact duplication this reference exists to remove.

| Requirement | Rule |
|---|---|
| MUST | the instance names a command/symbol **belonging to that skill** |
| MUST | the instance links back here (`skill-operating-principles.md`) |
| MUST NOT | identical instance text in 2+ skills (measured by `skill_overlap.py`) |
| SHOULD | be placed where the procedure lives, not in a preamble section |

## Cross-references

| Topic | Local |
|---|---|
| Four pillars (code-mode · master-cli · learning-memory · intelligence) | `~/.claude/rules/touring-4-pillars.md` |
| Loop protocol (OUTER/INNER/CLOSE, the three laws) | `~/.claude/skills/loop-engineering/SKILL.md` |
| Flow portfolio + fragments bundle | `docs/plans/2026-08-18-graph-engineering-flow-portfolio/` |
| Wayfinder + atomic claim (workspace rule 9) | `~/projects/touring/CLAUDE.md` |
| Pin gate / overlap / purpose instruments | `~/.claude/skills/TACO-skilling/scripts/` |

---

_v1.0 — 2026-08-20 | Five principles, five mechanisms, every command executed before it was written down._
