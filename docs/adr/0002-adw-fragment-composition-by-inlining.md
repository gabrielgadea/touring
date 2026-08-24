# 0002 — ADW fragments compose by load-time inlining, not runtime subgraphs

- Status: accepted
- Date: 2026-08-18
- Deciders: Gabriel Gadea (plan `2026-08-18-graph-engineering-flow-portfolio`)

## Context

The ADW library was nine monolithic TOML specs. Reuse happened by copying, so a
correction to a shared step reached only the flows someone remembered to edit —
the mechanism by which a shell-injection fix reached the repo mirror and the
instantiated copies but never the deployed library that `from-template` reads.

LangGraph solves the same problem differently: a compiled subgraph is passed
directly as a node (`builder.add_node("sub", subgraph)`), keeping its own state
schema and checkpointer, with the checkpoint namespace extended for the nested
scope. That is composition **at run time**, and it is strictly more powerful.

## Decision

Compose by **inlining at load time**. `[[use]] module/as/with` expands a fragment
into the host's node table under a namespace (`recall.memory`), resolving
`{{inputs.x}}` at expansion; fragments leave through the seams `__exit__` /
`__exit_fail__`, which the host rewrites to real nodes.

After inlining the spec is **flat**. The engine, journal, resume and lint keep
operating on exactly the structure they operated on before — not one line of the
execution machinery knows composition exists.

`touring adw explain <name>` prints the resolved flat graph, so composition is
never the only representation of a flow.

## Consequences

- **Positive.** Zero risk to the durability guarantees: replay, `--resume-run`
  and Class-D detection are untouched because they never see a fragment. The lint
  sees the whole graph, so a composed flow is checked as strictly as a written
  one. A fragment is a text transform, which makes it cheap to review.
- **Negative — accepted.** A fragment cannot own persistent state or its own
  checkpointer, which the LangGraph shape would allow. Our stated problem is
  reuse of pieces, not isolation of state; the limit is worth accepting and worth
  reopening if a fragment ever needs its own checkpoint.
- **Node names change.** Namespacing renames nodes (`recall` → `recall.memory`),
  so a composed flow is not byte-identical to the monolith it replaces.
  Equivalence is therefore proven by **resolved graph + execution sequence**, not
  by an empty diff. For the same reason the eight shipped flows were **not**
  retro-migrated: renaming their nodes would change prompts and journal names of
  flows in production for a secondary gain.
- Two defects came from the seam between "string" and "list" in this expansion,
  both found by using the feature: the inliner iterated a dynamic fan-out's
  template string character by character, and the same confusion existed in the
  lint and the runtime. Anything that walks `branches` must first ask which shape
  it holds.

## References

- `~/.claude/skills/Touring/scripts/adw.py` — `resolve_uses`, `_inline_one`
- `docs/explanation/adw-flow-portfolio.md` §4
- `docs/plans/2026-08-18-graph-engineering-flow-portfolio/cartografia-de-fluxos.html` §05
