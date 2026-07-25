---
name: TACO-skilling
description: Meta-skill for engineering high-quality Claude Code Agent Skills. Use whenever the user wants to create, build, author, design, scaffold, or bootstrap a new skill — or to refine, improve, audit, sharpen, or update an existing one. Also triggers on "turn this workflow into a skill", "criar uma skill", "build me a skill for X", "why does my skill keep getting Y wrong", and any request to capture a repeated task as reusable procedural knowledge. Composes document-skills:skill-creator (draft, test, eval, benchmark, package) with a Touring intelligence layer — memory recall, VGP symbol verification, transcript mining of past Claude Code sessions, REGRA #13 hygiene gates, deterministic script generation, and a self-improvement loop. Prefer this over hand-writing SKILL.md files; skill engineering should always go through here.
---

# TACO-skilling — Skill Engineering with Touring Intelligence

You are building or refining a **skill**: a folder of procedural knowledge that
Claude loads on demand. This meta-skill makes that process rigorous, grounded in
real evidence, and self-improving.

**Core stance — composition, not replacement.** TACO-skilling does not reinvent
the skill pipeline. It *orchestrates* `document-skills:skill-creator` for the
proven core (draft → test → eval → benchmark → package) and wraps it in a
**Touring intelligence layer** the bare skill-creator lacks: deduplication
against existing skills, memory of past lessons, symbol verification, mining of
your real Claude Code session history, hygiene enforcement, and a refinement
loop. This is Rule #3 (composable, not custom) applied to the tool itself.

## Two modes

| Mode | Trigger | Entry point |
|------|---------|-------------|
| **CREATE** | "create/build/author a skill", "turn this into a skill" | [references/create-workflow.md](references/create-workflow.md) |
| **REFINE** | "improve/refine/audit the X skill", "X keeps doing Y wrong" | [references/refinement-loop.md](references/refinement-loop.md) |

Decide the mode from the request, announce it ("Using TACO-skilling to
create/refine ..."), then follow the matching workflow. The reference file holds
the full step detail — this body holds the spine.

---

## The mental model — two layers

A skill, opened up, has **three internal layers**: description, instructions,
tools. TACO-skilling has **two operational layers** that produce them:

```
  TACO/Touring layer  ── discovery · memory · VGP · transcript mining
       (own)             hygiene · script-gen · learning · diary
          │
          ▼
  skill-creator core  ── draft SKILL.md · test cases · eval loop
     (delegated)         benchmark · viewer · package_skill.py
```

Full architecture, and exactly what each layer owns, is in
[references/architecture.md](references/architecture.md). Read it before your
first CREATE run in a session.

---

## The Four Rules (the foundation — and where the popular framing stops short)

These four rules are the distilled best practice for skill engineering.
TACO-skilling **enforces** them and patches the three places the popular framing
is incomplete — because an incomplete rule, followed literally, does damage.

1. **Skills, not prompts.** Capture *repeated* work as a skill; leave one-off and
   exploratory work to plain prompts. They coexist — a skill is not "the correct
   way to prompt", it is the correct way to store a *recurring* procedure. CREATE
   Phase 1 inspects real session history for actual repetition before you commit
   a skill, so you do not pay permanent context cost for a one-time task.

2. **Skills are more than prompts — the leverage is in layer 3.** Description and
   instructions are necessary; the *scripts and references* are where most value
   hides. A deterministic step run by a script is cheaper, faster and repeatable
   versus spending tokens to re-derive it. Note the conflation the popular framing
   makes: layer 3 of a skill is **bundled scripts + reference docs**, which is
   distinct from MCP function-calling tools. See
   [references/quality-rubric.md](references/quality-rubric.md).

3. **Composable, not custom.** Many small focused skills beat one monolith. But
   composition has a cost the popular framing ignores: every skill description
   sits in context permanently and competes for triggering precision. CREATE
   Phase 1 therefore *dedups* — it refuses to create a skill that overlaps an
   existing one, and prefers extending or composing what already exists.

4. **Smarter every session — WITH pruning.** A skill improves by being updated
   after each use. The part the popular framing omits: updates without pruning
   cause bloat, and bloat *degrades* adherence (longer files lower instruction
   following). TACO-skilling pairs every REFINE addition with a hygiene gate
   (REGRA #13) — if the skill crosses the line limit, the refinement *must*
   extract content to a reference in the same pass. Add and prune together, or
   the compounding loop quietly becomes entropy.

---

## CREATE — building a new skill

Five phases. The TACO layer brackets the skill-creator core (Phase 3).

| # | Phase | What happens | Owner |
|---|-------|--------------|-------|
| 0 | **Health & intake** | `touring doctor -j`; capture intent — what it enables, when it triggers, output format, whether it needs test cases | TACO |
| 1 | **Discover** | `scripts/discover.py` — dedup vs existing skills, `touring memory recall`, scan transcripts for real repetition | TACO |
| 2 | **Design** | Apply the quality rubric; decide which steps become scripts (Rule #2); decide invocation flags | TACO |
| 3 | **Draft** | Delegate to `skill-creator` for the SKILL.md draft + test-case design. Its eval loop (`run_loop`/`run_eval`) invokes `claude -p` per query — **LLM-costly, opt-in only** (see note) | skill-creator |
| 4 | **Touring enrichment** | VGP-verify cited symbols; generate scripts via `Touring-native tooling`; embed `refine-stub.sh`; run hygiene gate | TACO |
| 5 | **Validate & persist** | `package_skill.py`; `touring memory store`; `touring learning reward`; `touring diary write` | both |

Run `scripts/discover.py "<intent>"` first — its report tells you in Phase 1
whether to create, extend, or compose. The full step-by-step — including how to
invoke the skill-creator workflow and hand it the enriched design — is in
[references/create-workflow.md](references/create-workflow.md).

**Generating the new skill's scripts (Phase 4):** never hand-write `.py`/`.sh`
files with the Write tool — edição-com-gate and the `Touring-native tooling-guard` hook block it.
Use `Write tool (script Python)` for Python, `Write tool`
for other code. Bundling a script *is* Rule #2 in action.

**On the skill-creator eval loop (Phase 3):** its `run_loop`/`run_eval` tests
triggering by invoking `claude -p` once per query — hundreds of paid LLM
sessions per run. Do **not** run it by default. Validate instead with
`scripts/quality_gate.py` (structure + hygiene) and, for triggering, with
`scripts/triggering_audit.py` — which analyses *real session history*
deterministically, zero LLM. Use the eval loop only when the user explicitly
accepts the cost.

---

## REFINE — the self-improvement loop

Rule #4 made operational, grounded in **your real session history** rather than
in memory of the last chat.

| # | Phase | What happens |
|---|-------|--------------|
| 1 | **Mine** | `scripts/mine_transcripts.py <skill>` scans `~/.claude/projects/*/*.jsonl` for sessions where the skill ran, plus the local telemetry `refine-stub.sh` collected |
| 2 | **Diagnose** | Classify the signal — recurring errors, uncovered edge cases, steps the user always corrects, post-skill re-prompts |
| 3 | **Route the learning** | Decide *where* each lesson belongs — SKILL.md, a reference file, a new script, `touring memory` (cross-skill lesson), or the gotcha DB. Not everything belongs in the skill |
| 4 | **Propose** | Show the user a concrete diff. Never apply blind |
| 5 | **Apply + gate + persist** | Apply via `Touring-native tooling`; run the hygiene gate (ADD *and* PRUNE); re-run eval if the skill has one; `touring memory store` + `learning reward` |

The hybrid refinement model: the engine (`scripts/`) lives only here; every skill
TACO-skilling generates carries a lightweight `refine-stub.sh` that records local
usage telemetry and delegates back to this skill. Full detail — the transcript
signal taxonomy and the learning-routing decision tree — is in
[references/refinement-loop.md](references/refinement-loop.md).

---

## Quality bar — what "excellent" means

A skill ships only when it clears the rubric in
[references/quality-rubric.md](references/quality-rubric.md). The headline gates:

- **Description triggers correctly** — specific, slightly pushy, covers what +
  when; audited against *real session history* with `scripts/triggering_audit.py`
  (deterministic, zero LLM) — never by paid `run_loop` simulation.
- **Progressive disclosure respected** — SKILL.md < 500 lines; references loaded
  on demand; scripts execute without loading into context.
- **Hygiene (REGRA #13)** — name ≤ 64 chars, description ≤ 1024 chars, body < 500
  lines.
- **Layer 3 present** — repeated deterministic work is a script, not prose.
- **Invocation flags set deliberately** — user-invocable and model-invocable
  decided per skill, not left to default; high-risk skills restricted.
- **Verified, not invented** — every cited command/symbol passes VGP.

---

## Touring integration

The intelligence layer the bare skill-creator has no access to:

| Phase | Touring capability | Command |
|-------|--------------------|---------|
| Discover | past lessons | `touring memory recall "<intent>"` |
| Discover | symbol / pattern search | `touring tantivy search "<query>"` |
| Discover | known pitfalls | `touring gotcha match <file>` |
| Design | task DAG for complex skills | `touring decompose create` |
| Enrich | VGP — does the symbol exist? | `touring index find <symbol>` |
| Enrich | generate scripts | `Write tool (script Python)` |
| Persist | store the lesson | `touring memory store --tier semantic` |
| Persist | reward the outcome | `touring learning reward` |
| Persist | agent diary | `touring diary write` |
| Refine | pattern drift | `touring evolution insights -j` |

Exact invocations, memory tiers, and the daemon-down fallback are in
[references/touring-integration.md](references/touring-integration.md). If
`touring doctor -j` reports the daemon down, the workflow still runs — fall back
to filesystem scans and mark the affected fields `daemon_degraded`.

---

## Scripts — this skill's own layer 3

| Script | Purpose |
|--------|---------|
| `scripts/discover.py` | Dedup vs existing skills, memory recall, "is this task actually repeated?" scan |
| `scripts/mine_transcripts.py` | Scan session `.jsonl` for a skill's usage + feedback signal |
| `scripts/quality_gate.py` | Validate structure + REGRA #13 hygiene + quality score |
| `scripts/triggering_audit.py` | Audit a skill's real triggering from session history + Touring intelligence (zero LLM) — for description tuning |
| `scripts/lib.py` | Shared helpers — `.jsonl` parsing, `touring` CLI wrappers (imported, not run) |

Run any executable script with `--help`. They are deterministic — prefer them
over re-deriving the same analysis by hand (Rule #2).

---

## Hard rules

1. **Discover before creating.** Never create a skill that duplicates an existing
   one — `discover.py` Phase 1 is mandatory. Overlap → extend or compose instead.
2. **Generate code through `Touring-native tooling`.** edição-com-gate: `.py`/`.sh` and other code
   are created via `Write tool*`, never the Write tool.
3. **Refine prunes.** Every REFINE that adds content runs the hygiene gate; over
   the limit → extract to a reference in the same pass.
4. **Verify, never invent.** Cited commands/symbols pass VGP (`touring index find`)
   before they enter a SKILL.md.
5. **Generated skills are in English** — body and instructions; descriptions may
   carry bilingual trigger keywords for reliable activation.
6. **The user sees the diff.** REFINE never applies changes blind; CREATE never
   skips the user's sign-off on test cases.
7. **Code analyses, the model synthesises.** Never invoke `claude -p` / `run_loop`
   / any LLM in bulk for work a script can do. Session history is recorded data —
   analyse it deterministically (`triggering_audit.py`, `mine_transcripts.py`, the
   `touring` CLI), never simulate it with paid LLM calls. The model reads the
   script's report and decides; the script does the bulk work.

---

## Reference map

| Topic | File |
|-------|------|
| Two-layer architecture + skill-creator composition | [references/architecture.md](references/architecture.md) |
| CREATE pipeline, phase by phase | [references/create-workflow.md](references/create-workflow.md) |
| REFINE loop, transcript mining, learning routing | [references/refinement-loop.md](references/refinement-loop.md) |
| Quality rubric, hygiene, invocation flags | [references/quality-rubric.md](references/quality-rubric.md) |
| Touring command integration + fallback | [references/touring-integration.md](references/touring-integration.md) |
| skill-creator (delegated core pipeline) | the `document-skills:skill-creator` skill |
