# How to author an ADW flow

> Task-oriented. Concepts, the fragment kit and the full spec schema live in
> [`docs/explanation/adw-flow-portfolio.md`](../explanation/adw-flow-portfolio.md).

## Goal

Create a durable agent workflow for one task, composed from existing pieces,
proven before it costs an agent call.

## Context: compose, never copy

Copying a flow forks its bugs. `touring adw new` composes from the kit of 11
fragments, so a fix to a shared step reaches every flow that uses it. The command
refuses to start until prior art has been judged — creating blind is the failure
it exists to prevent.

## Steps

```bash
# 1. See what already exists, by INTENT rather than by filename
touring portfolio "validar migração de schema antes do deploy"
touring adw fragments                       # what is composable

# 2. Create. --verdict is mandatory: reuse | extend | supersede | create_new
touring adw new validar-migracao-schema \
  --intent "validar migração de schema antes do deploy com rollback verificado" \
  --verdict create_new \
  --when-to-use "mudança de schema que toca dados de produção" \
  --when-not-to-use "mudanças sem impacto em dados de produção" \
  --use recall-pack:recall \
  --job planner:agent \
  --use critic-panel:panel \
  --use human-approve:sign

# 3. Read it as ONE graph — composition is never the only representation
touring adw explain validar-migracao-schema

# 4. Run it
touring adw run validar-migracao-schema --var topic=schema --var artifact=diff
```

`--use <fragment>[:alias]` composes a kit piece · `--job <name>[:type]` adds a step
of your own · order is significant (`--order` overrides) · fragment inputs default
to `{{vars.<input>}}`, so the flow is runnable as generated.

## Verify

```bash
touring adw lint validar-migracao-schema    # exit 0, and read the warnings
touring adw test validar-migracao-schema    # walks the graph, agents mocked
```

`adw test` reports a `synthesized` list — the nodes it stood in for because no
recording existed. A synthesized walk proves the **graph connects**, never that
the agent behaves. Record real behaviour with `touring adw run … --record`.

## Pitfall: never interpolate a variable into a shell string

```toml
# WRONG — the value lands inside `bash -c`
command = ["bash", "-c", "touring memory recall \"{{vars.symptom}}\""]

# RIGHT — positional argument
command = ["bash", "-c", "touring memory recall \"$1\"", "--", "{{vars.symptom}}"]
```

The library shipped the first form for ten days, to every project that
instantiated it. A structural guard now scans the deployed library, not only the
repo copy.

## Pitfall: a gate that hands work back must hand its verdict too

An `on_fail` edge returning to an agent whose prompt never references
`{{nodes.<gate>.summary}}` retries blind — the agent cannot know what rejected it.
The lint warns; flows created by `adw new` wire it automatically.

## Pitfall: fan-out declarations have no safe defaults

`merge`, `on_branch_fail` and `max_branches` are all required on a `parallel`
node. Each one, left implicit, is a way for the block to lose evidence quietly —
see the explanation doc §5 and [ADR 0003](../adr/0003-adw-read-only-fanout-explicit-policies.md).
