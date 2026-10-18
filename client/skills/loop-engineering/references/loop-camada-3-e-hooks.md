<!-- loop-camada-3-e-hooks.md — movido do SKILL.md de `loop-engineering` em 08/10/2026 (compactação 65). Conteúdo VERBATIM: a conservação é provada por estas linhas aparecerem aqui sem alteração. -->

## Scripts (Layer-3 — deterministic composition, `scripts/`)

| Script (real CLI) | Replaces (atomic sequence) | Contract |
|---|---|---|
| `loop_diagnose.py --scope <path> [--topic <s>] [--bundle <dir>] [--plan-id <id>]` | `touring status`+`touring-quality --workspace`+`wiring orphans`+`memory recall`+`touring map` | one-shot diagnostic → `{health, quality50, wiring, memory, structure}` JSON + an OKF `Diagnostic` doc in `<bundle>/diagnostics/` |
| `loop_converged.py --task <id> --scope <path> [--bundle <dir>] [--rust-full]` | the 6 convergence clauses above | exit 0 (converged) / 1 (continue) + JSON `{converged, clauses, unmet[], next_action}`. **Fail-CLOSED**: `dag_done` + applicable Rust clauses never pass on missing evidence |
| `loop_phase_close.py --task <id> --phase <Pn> --summary "…" [--status done] [--bundle <dir>] [--gates <json>] [--abstract <json>] [--reward <f>]` | decompose update + memory store + learning reward + OKF report + typed abstract + log | closes a phase; writes `phases/<Pn>.md` (OKF `PhaseReport`) + `knowledge/<Pn>.json` (Hyper-Extract hypergraph, deterministic ids) + appends `log.md`. **O phase curto ("P2") é resolvido para o subtask_id completo da DAG** (`resolve_subtask_id`, 25/08 — `decompose update` não casa prefixo: sem a resolução o fechamento declarava sucesso e a DAG não andava; provado por mutação 0→1→0 no campo `dag_updated`) |
| `variant_archive.py record\|sample\|stats\|prune <target>` | discarding what failed the gate | the stepping-stone archive: `w = s·h` weighted sampling, seeded and replayable, capped with pruning by score then age. 13 tests |
| `judge_attest.py [--attest --why "…"] [--json]` | reading the graders' source and trusting it | the judge of record: sha256 per grader + the clause inventory (read by AST). exit 0 clean / 1 blocking drift. 12 tests, each guard mutation-proven |
| `loop_doc_link_gate.py --bundle <dir> [--strict]` | OKF frontmatter/link validation + OpenKB contradiction lint | exit 0 clean / 1 blocking + JSON `{missing_type[], missing_plan_id[], broken_links[], orphan_docs[], contradictions[]}` |

Every script: `--help`, `--json`, `--quiet`; fail-open when the daemon is degraded. Prefer the script over re-deriving its N-call sequence by hand.

---

## Hooks (`settings.json`, registered)

- **Stop** → `scripts/hooks/loop_stop_guard.py` — resolves the **per-project** active marker (scoped to the session cwd); only when the daemon POSITIVELY confirms the task exists with pending subtasks AND `loop_converged.py` exits 1 does it emit `{"decision":"block","reason":"…next_action…"}` (converge-or-continue). On convergence it **archives** the marker; an orphaned/missing DAG **fails OPEN** (release + archive). Capped at `MAX_CONTINUATIONS=30` per run (runaway guard).
- **PreCompact** → `scripts/hooks/loop_snapshot.py` — snapshots pending subtasks to memory (key from `loop_marker.state_key`) + `log.md` so the loop resumes after compaction (the checkpointer analog).
- **SessionStart + PostCompact** → `scripts/hooks/loop_resume.py` — the READER the checkpointer lacked until 2026-08-02 (state was written and never read back, so the marker kept enforcing a loop the next context had no memory of). Prefers **live evidence** — the artifact gate and `decompose get` recomputed now — over the stored snapshot, which enters only as enrichment; injects the unmet artifacts / pending subtasks plus the exact next command. Silent when nothing is owed.
- **UserPromptSubmit** → `scripts/hooks/loop_outer_arm.py` — arms the per-project marker with `status: "outer"` BEFORE any DAG exists. Two tiers: an explicit invocation (`/loop-engineering`, `/goal` → `strategy-outer`; `/TACO-cross-audit` → `cross-audit`), **or** — since 2026-08-02 — any substantive engineering prompt → `work-outer`, the DEFAULT flow. `work-outer` owes only the two deterministic OUTER artifacts (diagnostic + CCE ledger, no strategy doc), caps at 2 continuations, derives a stable per-day bundle, never downgrades an explicitly-invoked flow, and is disabled by `TOURING_WORK_OUTER_DISABLED=1`. Detection is an imperative-mood heuristic tuned for precision: prose mentions and questions must never arm (`test_prose_never_arms`, `test_default_flow_stays_out_of_conversation`). From that moment the Stop hook verifies the flow's artifact manifest (`hooks/flow_manifests.json` via `hooks/loop_outer_gate.py`, capped at the manifest's `max_continuations`) instead of the DAG, and every evaluation appends one record to `~/.claude/loop-engineering/compliance.jsonl` (the flow-KPI feed). A live loop (real task, `status: active`) is never clobbered by arming.
- All three share `scripts/hooks/loop_marker.py` — the per-(project, session) marker helper (scope/read/write/archive/TTL/session-ownership).

All are **loop-scoped** (no-op unless *this project's* marker exists) and **fail-open** (any error, an undeterminable daemon, or an orphaned DAG → exit 0, never block the session). Registered by appending to the existing `.hooks.Stop` / `.hooks.PreCompact` / `.hooks.UserPromptSubmit` arrays (never replacing) — **always as `python3 <path>`, never a bare script path**: a direct path depends on the execute bit, and a silently missing `+x` made every UserPromptSubmit fail with Permission denied on 23/07/2026 (`test_registered_hook_commands_are_runnable` now guards every registered command as-registered).

## tdd-enforcer integration (Rank #2 do plano `docs/plans/2026-09-01-skill-aprimoramento/`, added 2026-09-01)

INNER step 12 (Execute phase) runs the `tdd-enforcer` protocol (mechanism E + G — pre-conditional + procedural RED-GREEN). The agent runs `enforce.py --mode red` before writing production code; **no hook enforces it** (0 registrations, verified 18/10/2026), so a skipped check goes unnoticed.

**Pre-conditional gate (G)** — before Edit/Write can run on production code, the agent MUST show a failing test that exercises the planned change. No failing test = no edit. This closes the universal gap (E) RED-GREEN-REFACTOR.

**Sequential cycle (E)** — verbatim from obra `superpowers:test-driven-development`:

1. Write the failing test (RED)
2. Run test — MUST FAIL (proves the test is real, not vacuous)
3. Apply minimal code change (GREEN)
4. Run test — MUST PASS
5. Revert fix → run test (MUST FAIL — proves the test catches the regression)
6. Restore fix → run test (MUST PASS — proves the fix is correct)
7. Run full test suite — no regressions
8. Phase-close with verification evidence (test output before/after)

**Cross-pollination sources**:
- obra `superpowers:test-driven-development` (RED-GREEN-REFACTOR cycle verbatim)
- mattpocock/skills `tdd` ("No test is written at an unconfirmed seam")
- Universal MUST E (Marcel Point #1) — same MUST lives in 7 skills

**Skip conditions** (gating exempts):
- Documentation-only changes (`.md` files, comments, docstrings)
- Pure refactors without behavior change (verified by `git diff --stat` showing no semantic diff)
- Generated files (under `_generated/`, `target/`, `node_modules/`)
- Configuration changes (non-executable files)

**Invocation pattern**:

```bash
# Pre-edit gate (before any Edit/Write on production code)
python3 ~/.claude/skills/tdd-enforcer/scripts/enforce.py \
  --mode red --scope <path> --phase <phase_id>
# exit 0 = failing test exists, proceed with Edit/Write
# exit 1 = no failing test, BLOCK until one is written

# Post-edit verification (after Edit/Write)
python3 ~/.claude/skills/tdd-enforcer/scripts/enforce.py \
  --mode green --scope <path> --phase <phase_id>
# exit 0 = test now passes (red→green transition verified)
# exit 1 = test still fails (regression or incomplete fix)
```

The `tdd-enforcer` skill (criado como PHASE B candidato 4) owns the scripts; loop-engineering only references the contract. The agent invokes it; no hook does (registering it as a PreToolUse gate, shadow first, is item T1 of the ECC plan, `docs/plans/2026-10-17-analise-ecc/`). See `tdd-enforcer/SKILL.md` for the full protocol.

---

**Touring Diagnostic Arsenal** (`~/.claude/skills/Touring/scripts/`, shared) feeds the loop's diagnose (step 2) and multi-axis convergence: `systemic_diag_v2.py [scope]` (50-dim × arch-blast × security fused → the integrated risk the convergence gate scores), `crate_50dim_matrix.py <crate>` (lossless per-dim evidence for a phase target), `workspace_arch_diag.py` / `crate_arch_diag.py` (cycles + God-objects, the architecture clauses), `clone_blocks.py <file>` (classify a dedup phase before acting). Each is scope-able (crate/dir/file) so a phase can converge on its own target. Set `DIAG_OUT=<bundle>/diagnostics` to file the matrices with the run. **Reporting Contract (MANDATORY)**: every arsenal diagnostic run is relayed as the full 7-section elite audit report (never a single-lever summary) — spec + enforcement in `~/.claude/skills/Touring/scripts/report_contract.py`, printed as each digest's footer.

## The INNER now has parts, not just prose (2026-08-19)

The flow portfolio makes two of the loop's own steps composable instead of hand-run.
Reach for the piece before writing the phase by hand:

| Loop step | Piece | What it buys |
|---|---|---|
| INNER 12 execute | `touring adw new <name> --use <frag>…` | a flow born lint-clean, with gate feedback already wired and prior art judged (`--verdict` is mandatory) |
| INNER 13 cross-audit | `critic-panel` fragment | N **blind** critics with distinct lenses, `session = "fresh"`, quorum counted by code — the judge that step 13 lacked, because until now the context that produced the work also graded it |
| INNER 13 facts (W6 S-6.4) | probe `FACT=` + `loop_phase_close.py --facts` | os críticos julgam contra fatos ENDEREÇADOS (chave/valor/run_id) em vez de narrativa — a Lei L3 desce do turno para a fase; sem `--facts` o relatório da fase carrega a seção visível "Afirmações sem endereço" |
| INNER 13 breadth | `fanout-lenses` fragment | sectioning: N lenses over one target, `branches` resolved at RUNTIME (`{{vars.lenses}}` + a cloned `template`), merged by concatenation |
| OUTER 10 plan | `plan-pack` fragment | the planning node bound to `taco-planning` — the craft travels with the node, not just the posture |
| any step | `skilling-pack` fragment | when a procedure RECURRED (a `code` node checks ≥2, so it is arithmetic and not enthusiasm), capture it under `TACO-skilling` |
| any step | `touring adw explain <name>` | the FLAT resolved graph — composition is never the only representation |

Two invariants worth carrying into any phase that fans out: `max_branches` is checked
**before** a single branch runs (exceeding it refuses the block rather than billing for
partial work), and a dynamic template whose persona has a fixed `lens` is rejected —
N clones of one lens buy one opinion N times.

**A node now carries its CRAFT, not only its posture (2026-08-20).** An `agent` node
compiles to a headless `claude -p` with a persona: WHO acts, under which refusals.
`skill = "<name>"` adds WHICH CRAFT — the runner injects `Skill` into `allowed_tools`
and prefixes a deterministic instruction, and the lint refuses a skill that does not
exist or is `off` in `skillOverrides`, because both fail silently at run time and read
exactly like success. Measured before: `Skill` appeared ZERO times in the runner and
zero times across the ten library specs, so every planning agent re-derived
`taco-planning` and every auditing agent re-derived `TACO-cross-audit`.

**The asymmetry this fixes is the loop's own.** Step 12 executes and step 13 audits,
and both were the same context with the same posture — exactly what Isenberg names as
the commonest failure. `critic-panel` gives step 13 a judge that never saw step 12.

## Context enrichment inside the loop (2026-08-08)

The loop's value depends on what reaches each phase. Two of the nine enrichment
strategies are now **structural** in this harness — canonical body: `~/.claude/skills/Touring/references/context-enrichment.md`.

- **The `portfolio` lens is automated (E1+E2).** `explore`'s CCE contract only
  counts a round as dry once **every** lens in `AUTOMATED_LENSES` ran, so
  prior-art-by-purpose is consulted before any goal converges. Nothing to
  remember; skipping it is not reachable.
- **Creation ADWs carry prior art (E2).** `feature.toml`'s `recall` node and
  `chore.toml`'s `prior_art` node call `touring portfolio`, and their summary
  already feeds the scout prompt — the enrichment arrives by construction.

When a phase produces or consumes an injection, hold it to the honesty axes:
**E3** three sections + verdict · **E4** absence displayed · **E5** never
fabricate · **E6** no false gap claim · **E8** derived values · **E9** no silent
staleness. Each is provable by execution, which is what the convergence gate
wants anyway.

At phase close, record the verdict — `touring portfolio verdict … --why …` —
so the next run starts from the decision this one reached (the ACO pheromone).

### Hashtag library (v30.4, 2026-08-12) — the loop's output is catalogued by construction

Every phase **consumes** the faceted library and **produces** for it:

- **Recall is facet-aware**: `touring memory recall "<goal> #kind:lesson"` /
  `touring memory query "#kind:snippet #domain:<d>"` — conjunctive facet filter
  before RRF; `tag_filter.relaxed` in the payload keeps the filter honest.
  Prior-art by purpose: `touring portfolio "<intent> #kind:script"`.
- **Generated code is anchored**: every reusable snippet/fn a phase writes
  carries the codetag `// #tags: kind:snippet lang:… purpose:… domain:…` on its
  first line — `post_write`/`post_edit` harvest it automatically (removing the
  anchor tombstones the memory; code is the source of truth). Batch:
  `touring memory sync-tags --dir <path>`.
- **Phase-close persists with facets**: `--summary` memories and decisions go
  in with `--tag "#kind:…" --tag "#process:<fase>" --tag "#domain:…"`, and
  related entries are linked (`touring memory link <new> <old> --rel supersedes`)
  — so the next loop's recall finds the trail, and `touring memory moc <domain>`
  renders the domain's emergent map.

Guide + facet vocabulary: `~/projects/touring/docs/memory-hashtag-library.md`.

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

## Documentation pipeline — OKF · Hyper-Extract · OpenKB

Three open standards stack into one knowledge pipeline (details + conventions in `AGENTS.md`):

- **OKF** (Open Knowledge Format) — the **substrate**. Every `.md` the loop writes is an OKF document: YAML frontmatter (`type`, `title`, `description`, `tags`, `timestamp`) + body + **bundle-relative cross-links** (`/plan.md`). The plan dir is an **OKF bundle** (`index.md` listing + `log.md` history). Satisfies step 17 with a real standard.
- **Hyper-Extract** — the **structuring** engine. At phase-close, findings become a **typed Knowledge Abstract** (a hypergraph: nodes = decision/learning/gotcha/symbol/phase; edges = typed relations) with **deterministic** ids (`entity_id`, `relation_id = {source}|{type}|{target}` — aligns with REGRA #17), exported as `[[wikilinks]]`.
- **OpenKB** — the **synthesis** layer. At close, compile the bundle into an interlinked wiki (summary/concept/entity pages + auto cross-refs) and **lint for contradictions** (aligns with the co-evolution/drift law). This is also the KB that **compounds across runs** (L4).

**MVP policy**: adopt OKF fully; implement Hyper-Extract + OpenKB **patterns natively** in the scripts (no external Python deps). Integrating the real tools (Hyper-Extract's 10+ engines, OpenKB's `watch`/`lint` pipeline) is a post-MVP phase.

## Grilling integration (Marcel Point #2 do Gabriel, 2026-09-01)

Before any plan/strategy recommendation in this skill, invoke the `grilling` primitive (frontier-drain interview) to drain unknowns before declaring convergence or requesting human approval. Pattern: enumerate bounded questions, ask one at a time, integrate, repeat.

**Trigger conditions in this skill**:
- **Step 6 (Strategy consolidation)** — before naming the intent, list the unknowns that would change it
- **Step 9 (HUMAN GATE)** — before presenting the strategy, enumerate unknowns the human should resolve
- **Step 14 (Phase-close)** — before claiming "phase done", drain unknowns that would invalidate the verdict
- **Loop convergence** — before `loop_converged.py` reports PASS, drain the unknowns that the convergence gate didn't check

**Skip conditions**: routine reconnaissance (mapping, search, recall) — read-only with no decision to recommend.

**Companion**: `decision-canvas` (structured form for plan-authoring; grilling = conversational form). Cross-reference: `~/.claude/skills/grilling/SKILL.md`. Universal primitive — applied transversalmente em 5 skills per Marcel Point #2.

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

## Activation & resume — the `thread_id`

A loop is **active** iff a marker exists for this **(project, session)** pair at
`~/.claude/loop-engineering/active-<sha1(cwd)[:12]>-<sha1(session)[:8]>.json` —
*never* a global singleton, so concurrent loops in different projects never
clobber each other and a Stop event in project B never gates on project A's loop.

Since 2026-08-02 the key also carries the **session**: N Claude Code sessions open
on the SAME project used to share one marker, so session B's Stop was held by A's
unmet manifest, B free-rode on A's artifacts once A completed, the `continuations`
cap was consumed jointly, and A's convergence archived the marker out from under
B. Identity comes from the hook payload's `session_id`, else
`CLAUDE_CODE_SESSION_ID`/`TOURING_SESSION_ID` (Claude Code exports both into every
hook process, so stdin-less hooks resolve the same id the arming hook used). A
marker stamped for another session is never "mine" even if its path is reached;
a pre-2026-08-02 marker carries no stamp and is **claimed once** by the first
session that evaluates it (migration, so a live loop is not silently un-gated);
with no resolvable identity the old project-wide behavior is kept exactly. ADW run
dirs are session-keyed for the same reason (`adw.py::new_run_id`) — the old
`<name>-<epoch>` collided when two sessions started one ADW in the same second.

Write it with the helper (guarantees `cwd` + timestamps + `status`), never by hand:

```bash
scripts/hooks/loop_marker.py write --task <task_id> \
    --scope "<path scored for convergence>" --bundle "<OKF bundle dir>"
# fields: {task, scope, bundle, cwd, status, continuations, created_at, updated_at}
```

- **OUTER arming is automatic** — invoking the skill writes the marker with `status: "outer"` (no task yet; `flow` selects the manifest); `strategy-loop` refreshes it with the bundle, and step 11 upgrades it to `status: "active"` with the real DAG task. An outer marker obeys the same TTL and goes quiet once its manifest is complete (`outer_complete: true`). **Since 2026-08-02 no invocation is required**: a substantive engineering prompt arms `work-outer` on its own, so the deterministic OUTER is the default modus operandi rather than an opt-in mode (CLAUDE.md §"MODUS OPERANDI = LOOP").
- **Start** a loop → `loop_marker.py write …` (after the plan+DAG exist). The Stop hook now enforces convergence; PreCompact now snapshots.
- **Resume** after compaction/`/clear` → `touring decompose ready <task>` + `touring memory recall "loop-state:<task>"` reconstitute the state; the marker keeps the hooks live.
- **End** a loop → convergence archives the marker automatically; or `loop_marker.py archive` to abandon a run. A marker not updated in **24h (TTL)** or stamped `CONVERGED`/`ARCHIVED` is inert — no future session's Stop is ever held by a stale run.
