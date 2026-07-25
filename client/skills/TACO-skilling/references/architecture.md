# Architecture — Two Layers, One Pipeline

TACO-skilling is a *composition*. It owns an intelligence layer and delegates the
proven skill-construction core. Understanding the split prevents the most common
failure mode of meta-skills: reimplementing what `skill-creator` already does
well, then drifting from it as Anthropic improves it.

## Table of contents

- [The two operational layers](#the-two-operational-layers)
- [The three internal layers of any skill](#the-three-internal-layers-of-any-skill)
- [Progressive disclosure](#progressive-disclosure)
- [How the composition runs in practice](#how-the-composition-runs-in-practice)
- [Why compose instead of fork](#why-compose-instead-of-fork)

## The two operational layers

### Layer A — TACO/Touring (owned by this skill)

Everything `skill-creator` has no access to:

| Capability | Why it matters | Backed by |
|------------|----------------|-----------|
| **Discovery & dedup** | Refuses to create a skill that overlaps an existing one (Rule #3). | `scripts/discover.py` |
| **Memory recall** | Surfaces lessons from past skill work so mistakes are not repeated. | `touring memory recall` |
| **VGP verification** | Every command/symbol cited in a SKILL.md is checked to actually exist. | `touring index find` |
| **Transcript mining** | Reads real `~/.claude/projects/*.jsonl` history for repetition + post-skill feedback. | `scripts/mine_transcripts.py` |
| **Hygiene enforcement** | REGRA #13 line/length gates, run on create AND on every refine. | `scripts/quality_gate.py` |
| **Script generation** | Bundled scripts produced via `Touring-native tooling`, never hand-written. | `Write tool (script Python)` |
| **Learning loop** | Closes the feedback loop so the next run starts smarter. | `touring learning reward` + `memory store` |

### Layer B — skill-creator core (delegated)

The `document-skills:skill-creator` skill owns the construction pipeline and is
invoked, not copied: draft SKILL.md, write test prompts, baseline comparison, and
`package_skill.py`. TACO-skilling passes it an *enriched design* (the discovery
report + rubric decisions) and consumes its outputs. It does not duplicate any of
that machinery.

**Not adopted from skill-creator: the LLM eval loop.** skill-creator's
`run_loop.py` / `run_eval.py` and benchmark optimizer test triggering by
invoking `claude -p` once per query — hundreds of paid LLM sessions per run.
TACO-skilling deliberately does *not* run that by default: triggering is audited
deterministically from recorded session history by `scripts/triggering_audit.py`
(zero LLM). Code analyses the data; the model only synthesises. The eval loop is
opt-in, gated on the user explicitly accepting the cost.

```
        user request
             │
   ┌─────────▼──────────┐
   │  TACO-skilling     │  Layer A — intelligence
   │  Phases 0,1,2      │  discover · memory · design
   └─────────┬──────────┘
             │ enriched design
   ┌─────────▼──────────┐
   │  skill-creator     │  Layer B — construction
   │  Phase 3           │  draft · eval · benchmark · package
   └─────────┬──────────┘
             │ draft + eval artifacts
   ┌─────────▼──────────┐
   │  TACO-skilling     │  Layer A — enrichment
   │  Phases 4,5        │  VGP · script-gen · hygiene · persist
   └────────────────────┘
```

## The three internal layers of any skill

Independent of TACO-skilling's two *operational* layers, every skill it produces
has three *internal* layers. Keep the vocabulary straight:

1. **Description** (frontmatter `description:`) — the triggering mechanism. Claude
   reads it to decide whether to activate the skill. Always in context.
2. **Instructions** (SKILL.md body) — the playbook. Loaded when the skill triggers.
3. **Bundled resources** — `scripts/` (executable, deterministic), `references/`
   (docs loaded on demand), `assets/` (templates/fonts used in output).

The popular framing calls layer 3 "tools" and conflates it with MCP
function-calling tools. They are different: a skill's layer 3 is *files in the
skill folder*; MCP tools are *endpoints exposed by an MCP server*. A skill may
instruct Claude to call an MCP tool, but the MCP tool is not part of the skill.

## Progressive disclosure

The three internal layers map to three loading tiers — this is what keeps a rich
skill cheap:

| Tier | Content | Cost |
|------|---------|------|
| 1 | name + description | always in context (~100 words) |
| 2 | SKILL.md body | in context only when the skill triggers (< 500 lines) |
| 3 | references / scripts | loaded on demand; scripts execute without ever loading |

A skill that puts everything in the SKILL.md body pays tier-2 cost for content
that belongs in tier 3. The CREATE workflow's Phase 4 hygiene gate enforces the
split; the REFINE workflow re-checks it on every change.

## How the composition runs in practice

When CREATE reaches Phase 3, invoke the skill-creator skill (`Skill` tool,
`document-skills:skill-creator`) and follow its workflow for the draft + eval
loop. Feed it the discovery report and design decisions from Phases 1–2 so it
does not re-ask questions you already answered. When it returns, resume at
Phase 4 to apply the Touring enrichment. See `create-workflow.md` for the
hand-off detail.

## Why compose instead of fork

- **Updates are free.** When Anthropic improves skill-creator (better eval loop,
  new benchmark stats), TACO-skilling inherits it with zero work.
- **No drift.** A forked pipeline silently diverges from the upstream best
  practice. Composition cannot drift — it always runs the current upstream.
- **It is Rule #3 applied to ourselves.** A meta-skill that violated "composable,
  not custom" while preaching it would be incoherent.

The single cost: TACO-skilling depends on `document-skills:skill-creator` being
installed. If it is absent, CREATE degrades to a `skill-writer`-level draft
(structure + frontmatter, no eval loop) and says so explicitly.
