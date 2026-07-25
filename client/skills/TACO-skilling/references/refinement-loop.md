# REFINE — The Self-Improvement Loop

Rule #4 made operational. The popular framing says "update the skill after each
use". This loop adds the three things that framing omits: it grounds the update
in *real session evidence* rather than memory of the last chat, it *routes* each
lesson to the right home, and it *prunes* as it adds so the skill does not bloat.

## Table of contents

- [The hybrid model](#the-hybrid-model)
- [Phase 1 — Mine](#phase-1--mine)
- [Phase 2 — Diagnose](#phase-2--diagnose)
- [Phase 3 — Route the learning](#phase-3--route-the-learning)
- [Phase 4 — Propose](#phase-4--propose)
- [Phase 5 — Apply, gate, persist](#phase-5--apply-gate-persist)
- [The pruning rule](#the-pruning-rule)

## The hybrid model

The refinement engine (`scripts/`) lives only in TACO-skilling. Every skill
TACO-skilling generates carries a lightweight `refine-stub.sh` that records local
usage telemetry and delegates back here. So:

- **One engine, many skills.** A fix to the refinement logic propagates to every
  skill at once — no duplicated `refine.py` across N skills (Rule #3, REGRA #13).
- **Per-skill telemetry.** The stub gives each skill a local breadcrumb trail
  (when it ran, exit signal) that the engine reads alongside the transcripts.

To refine a skill: `refine the <skill-name> skill`, or run the phases below
directly.

## Phase 1 — Mine

```bash
python3 ~/.claude/skills/TACO-skilling/scripts/mine_transcripts.py <skill-name>
```

The script scans `~/.claude/projects/*/*.jsonl` (real Claude Code session
history) for sessions where the skill was activated, and extracts the **feedback
signal** — what happened *around* each use:

- the `Skill` invocation and the prompt that triggered it,
- what the user said in the next few turns (corrections, "no, I meant…",
  re-prompts of the same request),
- tool errors that occurred while the skill was active,
- the local telemetry the skill's `refine-stub.sh` recorded.

It also pulls `touring memory recall "skill:refine:<name>"` so prior refinements
inform this one. Output: a structured signal report, no daemon required.

## Phase 2 — Diagnose

Classify every signal in the report into one of four kinds:

| Signal kind | What it looks like | Typical cause |
|-------------|--------------------|---------------|
| **Recurring error** | the skill produces the same wrong output across sessions | a wrong/missing instruction |
| **Uncovered edge case** | the skill works, except for one input shape it never anticipated | missing branch in the playbook |
| **Always-corrected step** | the user edits the same part of the output every time | a default that does not match the user |
| **Post-skill re-prompt** | the user immediately re-asks, rephrasing | the description over-triggered, or the skill under-delivered |

For each, apply the Rule #4 question — *is this a one-time fix, or should the
skill handle it forever?* Only "forever" signals proceed to Phase 3.

## Phase 3 — Route the learning

The popular framing's binary ("one-time fix" vs "put it in the skill") is too
coarse. A lesson has **five** possible homes — route each one deliberately:

```
Is the lesson specific to THIS skill?
├── no, it applies when building/using ANY skill
│      └──▶ touring memory store  (tier semantic, key skill:lesson:<topic>)
│
└── yes, specific to this skill
    ├── it is a known pitfall others will hit
    │      └──▶ touring gotcha  (the pitfall DB)
    ├── it is a repeated deterministic step
    │      └──▶ a new/updated script in scripts/  (Rule #2)
    ├── it is bulky knowledge or a variant-specific detail
    │      └──▶ a reference file  (keeps the body lean)
    └── it is a short, central instruction
           └──▶ the SKILL.md body
```

Routing everything into the SKILL.md body is the failure mode. It is what makes
"smarter every session" decay into bloat.

## Phase 4 — Propose

Produce a concrete diff for the user — exact before/after for each file touched.
State, per change, which signal it answers and which home Phase 3 routed it to.
**Never apply blind.** The user knows their workflow; the diff is a proposal, not
a fait accompli. (Hard Rule #6.)

## Phase 5 — Apply, gate, persist

1. **Apply** — code changes via `Edit tool` ; markdown
   changes directly.
2. **Hygiene gate — ADD and PRUNE**:
   ```bash
   python3 ~/.claude/skills/TACO-skilling/scripts/quality_gate.py <skill-dir>
   ```
   If the SKILL.md body crossed 500 lines, the refinement is **not done**: in the
   same pass, extract the least-central section to a reference file and re-run
   the gate. See below.
3. **Re-eval** — if the skill has an `evals/` set, re-run it (delegate to
   skill-creator) to confirm the change did not regress.
4. **Persist**:
   ```bash
   touring memory store "skill:refine:<name>:<date>" "<what changed + why>" --tier semantic
   touring learning reward orchestrate 1.0 "TACO-skilling: refined <name>"
   touring diary write taco-skilling "Refined <name>: <summary>" --aaak
   ```

## The pruning rule

This is the correction at the heart of REFINE. The popular framing treats
"update the skill" as pure upside. It is not — Anthropic's own guidance is that
longer instruction files lower adherence. So:

> **Every refinement that adds content must leave the skill no less healthy than
> it found it.** If an addition pushes the body over 500 lines, the same
> refinement pass must extract content to a reference. Add and prune together.

A skill that only grows is a skill slowly going blind. The hygiene gate in
Phase 5 is non-negotiable for exactly this reason.
