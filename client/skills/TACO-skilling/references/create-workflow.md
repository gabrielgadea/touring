# CREATE — Building a New Skill, Phase by Phase

Five phases. Layer A (TACO/Touring) brackets the skill-creator core (Phase 3).
Announce the mode, then work the phases in order. Do not skip Phase 1.

## Table of contents

- [Phase 0 — Health & intake](#phase-0--health--intake)
- [Phase 1 — Discover](#phase-1--discover)
- [Phase 2 — Design](#phase-2--design)
- [Phase 3 — Draft + eval (delegate)](#phase-3--draft--eval-delegate)
- [Phase 4 — Touring enrichment](#phase-4--touring-enrichment)
- [Phase 5 — Validate & persist](#phase-5--validate--persist)
- [Worked example](#worked-example)

## Phase 0 — Health & intake

Run the TACO FASE 0 health gate, then capture intent.

```bash
touring doctor -j        # daemon health; degraded → continue, mark daemon_degraded
```

Capture four things from the user (ask only what the conversation has not already
answered):

1. **What** should the skill enable Claude to do?
2. **When** should it trigger — what phrases, what contexts?
3. **Output** — what artifact or result does a successful run produce?
4. **Eval** — does the skill have objectively verifiable output (file transforms,
   data extraction, code generation)? If yes, plan test cases. If the output is
   subjective (writing style, design), plan to skip formal evals and rely on
   qualitative review. Suggest the default; let the user decide.

## Phase 1 — Discover

The dedup gate. **Mandatory** — it is Hard Rule #1.

```bash
python3 ~/.claude/skills/TACO-skilling/scripts/discover.py "<intent>"
```

The report answers three questions:

- **Does a skill already do this?** If an existing skill overlaps ≥ ~70%, do not
  create — recommend extending it (a REFINE pass) or composing it.
- **Is this task actually repeated?** The script scans session transcripts for
  prior occurrences of the workflow. Rule #1: a skill is worth its permanent
  context cost only if the task recurs. One-off → recommend a plain prompt.
- **What past lessons apply?** `touring memory recall` hits are summarized so the
  draft starts from accumulated knowledge.

Decide and state the outcome: **create new** / **extend existing** / **compose
existing** / **decline (one-off)**.

## Phase 2 — Design

Before any drafting, make the design decisions that the rubric demands:

- **Internal structure** — which steps are deterministic and repeated? Those
  become `scripts/` (Rule #2). Which knowledge is large or variant-specific? That
  becomes `references/`. What does the SKILL.md body itself need to hold?
- **Invocation flags** — decide user-invocable and model-invocable deliberately.
  See `quality-rubric.md`. High-risk skills (anything that deploys, sends, or
  deletes) should not be left model-invocable by default.
- **Complexity** — if the skill itself is a multi-step build, create a DAG:
  `touring decompose create skill "<name>"`.

Write the design down as a short brief. It is the input to Phase 3.

## Phase 3 — Draft + eval (delegate)

Invoke `document-skills:skill-creator` via the `Skill` tool and follow its
workflow. Hand it the Phase 1–2 brief so it does not re-interview the user:

- the captured intent (what / when / output / eval),
- the discovery outcome (create new, and why not extend),
- the design brief (scripts to bundle, references to split, invocation flags).

skill-creator owns: the SKILL.md draft, test-case prompts, and `package_skill.py`.
When it returns a draft, resume here at Phase 4.

**Do not run the skill-creator eval loop by default.** Its `run_loop` / `run_eval`
tests triggering by invoking `claude -p` once per query — hundreds of paid LLM
sessions per run. That is the anti-pattern this skill exists to avoid: code
analyses, the model synthesises. Validate the draft with `scripts/quality_gate.py`
(structure + hygiene, zero LLM); audit triggering with `scripts/triggering_audit.py`
(real session history + Touring intelligence, zero LLM). Run the LLM eval loop
only when the user has explicitly accepted the cost.

If `document-skills:skill-creator` is not installed, degrade: produce the draft
following `skill-writer` conventions (frontmatter + structured body) and tell the
user the skill shipped without the skill-creator scaffolding.

## Phase 4 — Touring enrichment

Apply what skill-creator cannot:

1. **VGP** — for every command, symbol, or file path the draft SKILL.md cites,
   verify it exists: `touring index find <symbol>`. Anything unverified is either
   corrected or removed. A skill that tells Claude to run a non-existent command
   is worse than no skill.
2. **Generate the bundled scripts** — via `Touring-native tooling`, never the Write tool
   (edição-com-gate, enforced by the `Touring-native tooling-guard` hook):
   ```bash
   Write tool (script Python) --path <skill>/scripts/<name>.py \
     --intent "<what the script does>"
   ```
3. **Embed the refinement stub** — copy `assets/refine-stub.sh` into the new
   skill's `scripts/`, and insert the `## Refinement` section from
   `assets/skill-template.md` into its SKILL.md. This wires the skill into the
   hybrid self-improvement loop (see `refinement-loop.md`).
4. **Hygiene gate** — run the quality gate on the new skill:
   ```bash
   python3 ~/.claude/skills/TACO-skilling/scripts/quality_gate.py <skill-dir>
   ```
   It must pass REGRA #13 (name ≤ 64, description ≤ 1024, body < 500 lines) and
   the structural checks before the skill ships.

## Phase 5 — Validate & persist

1. **Package** — `python -m scripts.package_skill <skill-dir>` from the
   skill-creator directory (validates frontmatter + structure, produces `.skill`).
2. **Persist the lesson**:
   ```bash
   touring memory store "skill:create:<name>" "<one-line summary of what + why>" --tier semantic
   touring learning reward orchestrate 1.0 "TACO-skilling: created <name>"
   touring diary write taco-skilling "Created skill <name>" --aaak
   ```
3. **Report** to the user: the skill path, what it does, its scripts, its
   invocation flags, and how to refine it later (`refine the <name> skill`).

## Worked example

> User: "I keep asking Claude to convert our meeting notes into the standard
> action-item format. Can we make that a skill?"

- **Phase 0** — intent: convert notes → action-item table; triggers on "action
  items", "meeting notes"; output: a markdown table; verifiable → plan evals.
- **Phase 1** — `discover.py "meeting notes action items"` finds no overlapping
  skill, finds the task 6× in transcripts → real repetition → **create new**.
- **Phase 2** — one deterministic step (the table formatting) → bundle a
  `format_actions.py` script; user-invocable yes, model-invocable yes (low risk).
- **Phase 3** — skill-creator drafts `meeting-action-items/SKILL.md`, writes 3
  test cases, runs the eval loop, benchmarks.
- **Phase 4** — VGP confirms no exotic commands cited; `Touring-native tooling` generates
  `format_actions.py`; refine-stub embedded; hygiene gate passes (body 80 lines).
- **Phase 5** — packaged, lesson stored, reward logged, user told how to refine.
