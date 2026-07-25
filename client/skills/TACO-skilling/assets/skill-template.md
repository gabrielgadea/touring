<!--
  TACO-skilling SKILL.md template.
  TACO-skilling fills the {{PLACEHOLDERS}} during CREATE Phase 4 and removes this
  comment. Sections marked "(optional)" are kept only if they apply.
  Keep the finished body under 500 lines (REGRA #13) — push detail to references/.
-->
---
name: {{NAME}}
description: {{DESCRIPTION}}
---

# {{TITLE}}

{{ONE_PARAGRAPH_PURPOSE}}
<!-- One paragraph: what the skill does and why it exists. Plain, concrete. -->

## When to use this skill

{{TRIGGER_CONTEXTS}}
<!-- The contexts and phrasings that should make Claude reach for this skill.
     This mirrors the description but in prose, for the reader. -->

## Workflow

{{NUMBERED_STEPS}}
<!-- The playbook. Imperative form. Explain WHY each non-obvious step matters —
     reasons generalize; bare ALWAYS/NEVER imperatives break on the first
     unforeseen case. Point to references/ for anything bulky. -->

## Scripts (optional)

{{SCRIPT_TABLE}}
<!-- Keep this section only if the skill bundles scripts. A repeated
     deterministic step belongs here as code, not as prose in the workflow. -->

## Examples (optional)

{{EXAMPLES}}
<!-- Input → Output pairs. Concrete examples beat abstract description. -->

---

## Refinement

This skill is wired into the **TACO-skilling self-improvement loop**. When its
output is not what you wanted, that signal should make the skill better — not
vanish when the chat closes.

- **To improve it:** ask Claude **"refine the {{NAME}} skill"** — this runs
  TACO-skilling in REFINE mode, which mines real session history for what went
  wrong and proposes a concrete diff.
- **Telemetry:** the bundled `scripts/refine-stub.sh` records local usage
  breadcrumbs and delegates to the central refinement engine in
  `~/.claude/skills/TACO-skilling/`. The stub never duplicates that engine —
  one engine, every skill (Rule #3).
- **Pruning is built in:** every refinement runs a hygiene gate, so additions are
  paired with extraction to references. The skill improves without bloating.
