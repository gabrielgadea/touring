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

> **L4 was named after the weaker algorithm; the archive closes the gap (2026-08-19).**
> arXiv:2505.22954 beats hill-climbing agentic design by keeping an ARCHIVE of every
> variant and sampling parents from it — 50.0% vs 39.7% on SWE-bench — because many paths
> to a good agent run through worse ones. `variant_archive.py` now keeps what fails the
> gate, **with its score**: `loop_phase_close.py --variant "<attempt>" --variant-target
> "<what it attempts>"` records it, and a partial gate becomes a gradient (five clauses of
> six → 0.83) rather than a zero. Selection is the paper's own weight, `w = s·h` — a
> sigmoid on the score times the novelty bonus `1/(1+children)` — so a leader already
> branched from five times yields to an unexplored peer, and a perfect variant is not
> sampled at all. Seeded, therefore replayable. What is NOT reproduced: the paper's full
> loop, which costs ~2 weeks per run on a cheap numeric benchmark we do not have.

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
       (= arm outer marker → `ground` DIAMOND: memory recall ‖ loop_diagnose.py
          in a read-only fan-out — by construction since 29/08 (M2,
          paralelização-agentes; the smaller branch leaves the clock:
          33.5s serial → 25.3s parallel) → touring explore --until-dry (CCE)
          → evidence report)
       Context7 stays a manual lens: mark it on the ledger with
       touring explore "<goal>" --mark-lens external:visited --note "<source>"
       The `portfolio` lens (2026-08-08) is AUTOMATED and therefore inescapable:
       a round only counts as dry once every lens ran, so prior-art-by-purpose
       is consulted before any goal converges (`touring portfolio "<goal>"`).
       VERIFY ARTIFACTS via the deterministic gate, never ad-hoc listings
       (lesson 2026-08-12, memory `lesson:artifact-verification-idiom:2026-08-12`):
         python3 scripts/hooks/loop_outer_gate.py --json   # manifest globs + mtime
       `ls -la DIR | tail` sorts ALPHABETICALLY — a new artifact can sort to
       the top and be cut by tail (the ledger "missing" that wasn't). Manual
       idioms when the gate is not the question: `ls -lat | head` (recency),
       `find <dir> -newermt '<ts>'` (since), `compgen -G '<glob>'` (existence).
  6  Strategy (sequential-thinking) consolidate into an intent
  7  Persist strategy             touring memory store + OKF strategy-<date>-<slug>.md (bundle)
  8  Present strategy + objectives
  9  ██ HUMAN GATE ██             plan approval (LangGraph interrupt analog)
       (the Stop guard blocks any turn ending before diagnostic + ledger +
        strategy doc exist — hooks/flow_manifests.json, verdict by artifacts)
  10 Plan + decompose             touring adw new <flow> --use plan-pack:plan   → the
       planning node is BOUND to the taco-planning craft (skill = "taco-planning"),
       so the plan is scored on 9 dims with VGP ground truth instead of re-derived
  11 Register DAG                 touring decompose add … --depends-on   (or --auto-populate)

INNER (repeat per phase until CONVERGED)
  →  next = touring decompose claim <task> --owner <session-id>   (topological, deps
       satisfied, ATOMIC — `ready` only READS, so two sessions polling it receive the
       SAME subtask and run the same phase twice; the lease returns it if one dies)
  12 Execute phase                **tdd-enforcer protocol** (red-green pre-conditional, run by
                                  the agent — no hook enforces it; see "tdd-enforcer integration") →
                                  Edit/Write + touring-engineer / touring index+ast+wiring
  13 Cross-audit + 50-dim         --use audit-pack:audit (skill = "TACO-cross-audit")
       + --use critic-panel:panel — ORTHOGONAL: audit-pack supplies the craft,
       critic-panel supplies the blindness. A skilled auditor who watched the
       work being made is still not blind. + touring wiring audit + touring-quality
       (the library's cross-audit flow composes the panel BY CONSTRUCTION since
        29/08 — verdict_gate → 3 blind critics → code-counted quorum; first live
        run green — so `touring adw run cross-audit` already carries it; the
        manual --use remains the route for NEW flows)
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

Este gate é a **encarnação do P2** — o princípio de que convergência é medida, nunca afirmada. As outras skills da família derivam a própria instância dele a partir do enunciado canônico (`Touring/references/skill-operating-principles.md`); esta o **implementa**: quando outra skill diz "o veredito é um exit code", é este exit code.
> **Caminho corrigido (08/10/2026):** `~/.claude/skills/Touring/references/skill-operating-principles.md` — o caminho relativo acima nao resolve nesta skill.

The loop exits **only** when `scripts/loop_converged.py` returns exit 0. All clauses must hold (grader-driven convergence — the blog's core discipline):

```
CONVERGED ⟺  judge_attest.py                       → the graders match the judge of record
         AND touring decompose ready <task>        → empty (all subtasks done)
         AND touring-quality score <scope> --workspace --fail-below 0.80  → pass (≥ Gold)
         AND 0 dims P0 BLOCK in Fail                (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5)
         AND the score covered the whole scope      (not a truncated prefix)
         AND touring wiring orphans                 ≤ baseline (REGRA #0)
         AND cargo check + test + clippy            → green (Rust scopes)
         AND TACO-cross-audit purpose-fidelity      → clean (purpose fidelity)
```

`judge_intact` is evaluated **first**, because every other clause is worth exactly
what the grader that scored it is worth — and until 2026-08-19 the graders were
writable by the agent they judge, with nothing recording when they changed. A
clause that VANISHES blocks (the rubric shrank, so the verdict is not the verdict
of record); a grader that merely CHANGED speaks without blocking, because a gate
whose failure has no cheap remedy is one people learn to route around. Declaring a
deliberate change is one dated command: `judge_attest.py --attest --why "…"`.
Origin: arXiv:2505.22954 App. H — an agent reached a *perfect* score by deleting
the markers its detector counted, while instructed not to touch them. The list
above is now enforced rather than declared: the clause names are read out of
`_gather_clauses` by AST and compared with the attestation (this block claimed six
while seven ran, and nothing reconciled the two).

Not converged → the script emits `{unmet:[…], next_action:"…"}`; the loop continues on the next ready subtask. This is what makes "perfect" terminate instead of looping forever.

## Human-in-the-loop — hybrid autonomy (3 gates)

Autonomous **within** a phase; pause for a human at:

1. **Strategy → plan** (step 9) — approve the decomposition before building.
2. **Before irreversible / outward actions** — deploy, `git` (REGRA #11 — never auto), external writes, `settings.json` changes, anything hard to reverse.
3. **Final sign-off** (optional) — before declaring converged & closing.

Everything else (new files, in-workspace edits, diagnostics, audits) runs autonomously.










## Golden rules

1. **Convergence is measured, never asserted** — `loop_converged.py` exit 0 is the only "done".
2. **State is durable** — DAG + memory + OKF bundle; assume compaction will happen mid-loop.
3. **Verify after every phase** — cross-audit + 50-dim before phase-close (L2).
4. **Human-gate the irreversible** — plan, deploy, `git`, `settings.json`, external writes.
5. **Every `.md` is an OKF doc linked to the plan** — the doc-link gate enforces it (step 17).
6. **Persist the pheromone** — `memory store` + `learning reward` each phase (Learning Memory pillar).
7. **MUST (E) — Verification before completion (transversal Marcel Point #1, 2026-09-01)** — NO COMPLETION CLAIMS WITHOUT FRESH EVIDENCE. Before stating "done", "fixed", "passes", "ready", "ship", or any success synonym, you MUST have run the verification command in **this turn** and seen the output. Red-green cycle for regressions: write test → run (pass) → revert fix → run (MUST FAIL) → restore → run (pass). Source: obra `superpowers:verification-before-completion` (Iron Law) cross-pollinated 2026-09-01; closes universal gap (E) RED-GREEN-REFACTOR across 7 skills (Marcel Point #1 do Gabriel — single MUST idêntico, single commit). Rationalization table (apply verbatim):

| Excuse | Reality |
|---|---|
| "Should work now" | RUN the verification |
| "I'm confident" | Confidence ≠ evidence |
| "Just this once" | No exceptions |
| "Linter passed" | Linter ≠ compiler |
| "Agent said success" | Verify independently |
| "I'm tired" | Exhaustion ≠ excuse |
| "Partial check is enough" | Partial proves nothing |

**Skip condition**: applies only to claims about code this skill produces/modifies/audits. Read-only reconnaissance (mapping, search, recall) is exempt.
---

## Referencia movida (compactação 65, 08/10/2026)

Esta skill ficou abaixo do corte de 20.000 caracteres do Claude Code, para que o procedimento inteiro chegue ao modelo depois de uma compactação. O material abaixo foi movido **verbatim** para [`references/loop-camada-3-e-hooks.md`](references/loop-camada-3-e-hooks.md) — nada foi perdido, apenas movido.

**No arquivo movido:** Scripts layer-3, hooks registrados, integração tdd-enforcer, adjudicação do INNER, enriquecimento de contexto, layout do bundle OKF, pipeline de documentação, grilling e cross-references.
