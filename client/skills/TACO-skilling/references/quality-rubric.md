# Quality Rubric — What "Excellent" Means

A skill ships only when it clears every gate here. The rubric is the operational
definition of "specific, assertive, high quality" — the bar TACO-skilling exists
to enforce. `scripts/quality_gate.py` checks the mechanical gates; the judgment
gates need a human eye, prompted by this file.

## Table of contents

- [Gate 1 — Description triggers correctly](#gate-1--description-triggers-correctly)
- [Gate 2 — Progressive disclosure](#gate-2--progressive-disclosure)
- [Gate 3 — Hygiene (REGRA #13)](#gate-3--hygiene-regra-13)
- [Gate 4 — Layer 3 present](#gate-4--layer-3-present)
- [Gate 5 — Invocation flags set deliberately](#gate-5--invocation-flags-set-deliberately)
- [Gate 6 — Verified, not invented](#gate-6--verified-not-invented)
- [Gate 7 — Instructions explain why](#gate-7--instructions-explain-why)
- [Ship checklist](#ship-checklist)

## Gate 1 — Description triggers correctly

The `description` is the single most important field — it is the only thing
Claude reads to decide whether to use the skill. It must:

- state **what** the skill does AND **when** to use it (contexts, phrases);
- be **specific** — "build a fast internal-metrics dashboard" not "help with data";
- be **slightly pushy** — Claude tends to under-trigger skills. Add an explicit
  cue: "Use this whenever the user mentions X, Y, or Z, even if they do not say
  'skill'.";
- be audited against **real session history** with `scripts/triggering_audit.py`
  — it measures what prompts actually triggered the skill, deterministically and
  with zero LLM calls. (The skill-creator `run_loop.py` optimizer simulates
  triggering with paid `claude -p` calls — hundreds per run; do not use it as the
  default. Code analyses recorded data; the model only reads the report.)

A vague description is the most common reason a good skill never fires.

## Gate 2 — Progressive disclosure

The three loading tiers must be respected:

- **SKILL.md body holds the spine, not the encyclopedia.** Workflow, decisions,
  pointers — yes. Long reference tables, variant-specific detail — no, those go to
  `references/`.
- **Reference files are pointed to clearly** from the body, with a one-line "read
  this when …" cue. A reference nobody is told to read is dead weight.
- **References over ~300 lines carry a table of contents.**

## Gate 3 — Hygiene (REGRA #13)

Mechanical, checked by `quality_gate.py`:

| Limit | Value |
|-------|-------|
| `name` length | ≤ 64 characters |
| `description` length | ≤ 1024 characters |
| SKILL.md body | < 500 lines |
| frontmatter | valid YAML, `name` + `description` present |

Over a limit is not a warning — it is a failed gate. The fix is extraction to a
reference, never deletion of substance.

## Gate 4 — Layer 3 present

If the skill has a step that is **deterministic and repeated**, that step is a
script in `scripts/`, not prose in the body. Prose asks Claude to re-derive the
same logic every run — burning tokens, risking variance. A script gives the same
output every time, for free. This is Rule #2: the leverage is in layer 3.

Signal that a script is missing: the skill's instructions describe an algorithm
(parse this, count that, format the other) step by step. That algorithm wants to
be code.

## Gate 5 — Invocation flags set deliberately

Two independent axes control who can run a skill. Decide both per skill — never
leave them to default:

- **User-invocable** — appears in the slash menu, the user can run it directly.
  Set to *false* for agent-only plumbing skills the user should never think about.
- **Model-invocable** — Claude can trigger it autonomously. Set to *false* for
  high-risk skills: anything that deploys, sends a message, spends money, or
  deletes data. Those should require an explicit human action.

> Note: the exact frontmatter field names for these flags depend on the current
> Claude Code skill schema — confirm against the live documentation or the
> `skill-creator` / `skill-writer` skill before writing them. The *decision* is
> mandatory regardless of the field name. (Confidence on field names: ~0.8.)

The default (both true) is correct for most skills. The point of the gate is that
it was a *decision*, not an omission.

## Gate 6 — Verified, not invented

Every command, CLI flag, symbol, or file path the SKILL.md cites must exist. VGP:

```bash
touring index find <symbol>
```

A skill that instructs Claude to run a hallucinated command is actively harmful —
it teaches a confident wrong move. If a citation cannot be verified, correct it or
remove it before shipping.

## Gate 7 — Instructions explain why

A judgment gate, not mechanical. Skills written as a wall of `ALWAYS` / `NEVER` in
caps are brittle — Claude follows them rotely and breaks on the first case the
rule did not foresee. Strong skills explain the *reasoning*: "do X, because Y" lets
Claude generalize correctly to situations the author never imagined. If the draft
is full of unexplained imperatives, that is a yellow flag — reframe each one as a
reason. (This is also the skill-creator's own guidance.)

## Ship checklist

A skill is excellent — and ready to ship — when:

- [ ] Description states what + when, is specific, slightly pushy, trigger-tested.
- [ ] SKILL.md body < 500 lines; references split out and pointed to.
- [ ] `name` ≤ 64, `description` ≤ 1024; frontmatter valid.
- [ ] Every repeated deterministic step is a bundled script.
- [ ] User-invocable and model-invocable were decided, not defaulted.
- [ ] Every cited command/symbol passed VGP.
- [ ] Instructions explain *why*, not just *what*.
- [ ] (If verifiable output) the eval loop ran and the benchmark is acceptable.
- [ ] `quality_gate.py` exits clean.
