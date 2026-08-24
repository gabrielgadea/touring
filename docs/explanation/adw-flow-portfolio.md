# ADW Flow Portfolio

**Authoring, composing and proving durable agent workflows.**

An ADW (Agent Durable Workflow) is a declarative graph of typed nodes that the
`touring adw` runner walks. The runner — never the model — decides when the work
is done: gates produce verdicts, the journal is fsync'd so a `kill -9` resumes
without redoing paid work, and a narrative that disagrees with a gate is recorded
as a divergence rather than believed.

This document is the reference for **writing** flows. For the runner's internals
see `~/.claude/skills/Touring/SKILL.md`; for why the design is what it is, see the
ADRs linked at the end. Task-oriented steps: `docs/how-to/author-an-adw-flow.md`.

---

## 1. Orientation

| You want to | Do this |
|---|---|
| Run an existing flow | `touring adw from-template <name>` then `touring adw run <name> --var k=v` |
| See what pieces exist | `touring adw fragments` |
| Create a flow for your own task | `touring adw new …` (§3) |
| Read a composed flow as one graph | `touring adw explain <name>` |
| Check a flow before running it | `touring adw lint <name>` |
| Walk the graph without paying for agents | `touring adw test <name>` (§8) |

**Shipped flows** (`~/.claude/skills/Touring/adw-library/`): `audit` · `bugfix` ·
`chore` · `explore-plan` · `feature` · `hotfix` · `scout-perpetuo` ·
`strategy-loop`. Each declares `[purpose]`, so `touring portfolio "<intent>"`
retrieves them by what they are FOR rather than by filename.

---

## 2. Anatomy of a spec

A flow is one TOML file at `<project>/.touring/adw/<name>.toml`.

```toml
[adw]
name = "bugfix"
entry = "recall"            # the node the run starts at
budget_tokens = 0           # 0 = unbounded; the lint sums node budgets against it

[purpose]                   # what makes the flow findable by INTENT
intent = "Fix a reproducible bug with institutional memory and a verifying gate"
when_to_use = ["a defect you can describe and a command that proves it fixed"]
when_not_to_use = ["a production incident — use hotfix, then bugfix for the cause"]
inputs = ["symptom", "target", "verify_cmd"]
produces = ["a gate-verified fix"]
tags = ["#kind:flow", "#domain:debugging"]

[node.recall]
type = "code"
command = ["bash", "-c", "touring memory recall \"$1\"", "--", "{{vars.symptom}}"]
timeout_ms = 30000
idempotent = true           # safe to replay on resume
on_pass = "fix"
on_fail = "fix"
```

### `[purpose]` — the discovery contract

`when_not_to_use` is **required in practice** and is the field that matters most:
it is what lets the portfolio *rule a flow out* instead of recommending the
closest candidate. The miner composes the indexable document from `[purpose]`
first, then the description and steps, and only then the header comment — the
600-character cap therefore falls on boilerplate (`Instantiate:`, `Run: --var …`)
rather than on the curated prose. A flow created by `touring adw new` cannot omit
it; a hand-written one is warned by the lint.

### Node types

| Type | Runs | Notes |
|---|---|---|
| `code` | a command | deterministic; mark `idempotent` when replay is safe |
| `agent` | a headless Claude session | `tier`, `allowed_tools`, `session`, `persona` |
| `gate` | a command whose verdict routes the graph | with `verdict_contract = true` it speaks PASS/REJECT/ESCALATE |
| `loop` | a body until a dryness signal | `NEW_FINDINGS=0` ends it |
| `human` | a durable pause | resume with `--approve <node>`; `zte = true` allows a conformal bypass |
| `parallel` | a read-only fan-out with a barrier | §5 |

### Edges

`on_pass` · `on_fail` · `on_dry` · `on_escalate`, plus the terminals `__end__`
and `__fail__`. **A gate that hands work back to an agent must pass its verdict
along** — the agent's prompt has to reference `{{nodes.<gate>.summary}}`, or the
retry is blind and the lint says so.

### Templates

`{{vars.k}}` (from `--var k=v`) and `{{nodes.X.summary}}` (a previous node's
output) resolve at run time. Inside a fragment, `{{inputs.k}}` resolves at compose
time. Inside a dynamic fan-out template, `{{branch.value}}` and `{{branch.index}}`
resolve per clone.

> **Never interpolate a variable into a shell string.** Pass it as a positional
> argument (`bash -c '…"$1"…' -- "{{vars.x}}"`), as every shipped flow does. The
> library once interpolated directly and shipped a shell-injection path to every
> project that instantiated it.

---

## 3. Creating a flow

`touring adw new` refuses to create anything before prior art has been judged.

```bash
touring adw new validar-migracao-schema \
  --intent "validar migração de schema antes do deploy com rollback verificado" \
  --verdict create_new \
  --when-to-use "mudança de schema que toca dados de produção" \
  --when-not-to-use "mudanças sem impacto em dados de produção" \
  --use recall-pack:recall \
  --job planner:agent \
  --use critic-panel:panel \
  --use human-approve:sign
```

- `--verdict` is mandatory and must be one of `reuse | extend | supersede |
  create_new`. The command prints the prior art it found — including the gaps it
  does **not** cover — and refuses to proceed on `reuse`.
- `--use <fragment>[:alias]` composes a kit piece; `--job <name>[:type]` adds a
  step of your own. Order is significant; `--order` overrides it.
- `--bind alias.input=value` binds a fragment input (default: `{{vars.<input>}}`,
  so the flow is runnable as generated).

The result is born lint-clean, with `[purpose]` filled and the gate→agent feedback
already wired.

---

## 4. Fragments — composition without a new engine

A fragment is a mini-spec that declares its seams:

```toml
[fragment]
description = "Institutional recall: memory + gotchas for one topic"
inputs = ["topic", "area"]
entry = "memory"
```

`[[use]]` inlines it under a namespace, so a composed flow resolves to a **flat**
graph — the engine, journal, resume and lint never learn that composition exists:

```toml
[[use]]
module = "recall-pack"
as     = "recall"                     # nodes become recall.memory, recall.gotcha
with   = { topic = "{{vars.symptom}}" }
```

Fragments leave through the seams `__exit__` / `__exit_fail__`, which the host
rewrites to the next real node. `touring adw explain <name>` prints the resolved
flat graph — composition is never the only representation of a flow.

### The kit (11 pieces)

| Fragment | Shape | Gives you |
|---|---|---|
| `recall-pack` | code | memory + gotchas for a topic |
| `prior-art` | code | portfolio lookup by purpose |
| `diagnose-pack` | code | investigate + blast + read, before acting |
| `fanout-lenses` | parallel (dynamic) | sectioning: N runtime lenses over one target |
| `critic-panel` | parallel + gate | voting: blind critics, distinct lenses, code-counted quorum |
| `gate-rust` | gate | check + test + clippy on the touched scope |
| `gate-quality50` | gate | 50-dimension score with the six P0 blocks |
| `conflict-guard` | code | serialise declared write-sets between concurrent workers |
| `human-approve` | human | durable pause before the irreversible |
| `phase-close` | code | memory store + reward + OKF report |
| `converge` | gate | `loop_converged.py` — exit 0 is the only "done" |

---

## 5. Fan-out

A `parallel` node runs **read-only** branches concurrently and joins at a real
barrier. Branches that can write are rejected by the lint: parallel writers race,
and no journal ordering can reconstruct who clobbered whom.

### Static — the branches are named

```toml
[node.panel]
type = "parallel"
branches = ["correctness", "security", "reproducibility"]
merge = "concat"
on_branch_fail = "ignore"
max_branches = 3
on_pass = "quorum"
```

### Dynamic — the branch COUNT is decided at run time

```toml
[node.sweep]
type = "parallel"
branches = "{{vars.lenses}}"   # resolves to a list: JSON array, or comma/newline separated
template = "lens"              # cloned once per value
merge = "concat"
on_branch_fail = "all"
max_branches = 8
```

Each clone is named `<template>:<value>` and binds `{{branch.value}}` and
`{{branch.index}}`. This is the shape a gauntlet actually has (N critics from
runtime input) and the only expression of the canonical *orchestrator-workers*
pattern, where the subtasks are decided by the run rather than by the author.

### The three declarations that are not optional

| Field | Values | Why it has no silent default |
|---|---|---|
| `merge` | `collect` · `tally` · `concat` | N branches share one result slot; without a declared join, all but one result is lost — last-write-wins |
| `on_branch_fail` | `all` (fail-closed) · `any` · `ignore`/`best_effort` · `quorum:N` | a failed branch would otherwise be indistinguishable from a passing one, and the graph would advance on partial evidence |
| `max_branches` | integer ≥ 1 | a runtime-sized fan-out otherwise multiplies the per-branch cost by a number nobody declared |

`max_branches` is checked **before any branch runs**: exceeding it refuses the
block rather than truncating the work and reporting success on part of it.
`quorum:N` answers "did enough branches run clean" — a different question from
the one a downstream verdict gate answers ("what did the critics conclude"), so
neither substitutes for the other. A quorum larger than the branch count is
refused at lint: it is a gate that could only ever fail.

There is no `policy` field. The pass condition lives in `on_branch_fail` alone.

---

## 6. Persona — declaring posture, not hoping for it

A node's specialisation has three axes. `tier` chooses the model, `allowed_tools`
chooses what it can touch, and `[node.X.persona]` declares **how it positions
itself** — the axis that separates a builder from a sceptic. It is compiled to
`--agents '{…}' --agent <role>`, inline, so the flow stays portable with no global
agent catalogue.

```toml
[node.critic.persona]
role = "critic-security"
stance = "reject_by_default"
lens = "security — untrusted input, injection, secrets, privilege"
scope = "the artifact under review and the bar it must clear"
bar = "{{inputs.bar}}"
burden = "Assume every input is hostile until the artifact bounds it."
refuses = ["editing the artifact", "fixing what you find"]
forbids = ["waving through a shell string built from a variable"]
blind_to = ["the author's own account of whether it works"]
escalate_when = "the artifact's trust boundary is not discoverable"
emits = "VERDICT=PASS|REJECT|ESCALATE followed by REASON=<one line>"
```

The lint holds a critic to a critic's structure: a parseable `emits`, never
`session = "resume_on_fail"` (inheriting the context you exist to distrust), and
— in a panel — lenses that actually differ. A dynamic template whose lens is
fixed is refused: N clones of one lens buy one opinion N times.

---

## 7. The verification contract

A gate with `verdict_contract = true` speaks three verdicts, not two, and must
declare `on_escalate`:

| Verdict | Meaning | Routing |
|---|---|---|
| `PASS` | the check ran and cleared the bar | `on_pass` |
| `REJECT` | the check ran and the bar was not met | `on_fail`, consuming a retry |
| `ESCALATE` | the check **could not run** | `on_escalate`, **without** consuming a retry |

The third exists because a broken environment is otherwise indistinguishable from
a real rejection, and both spend the retry budget re-invoking an agent against
something no agent can fix. **Silence is REJECT**: an unparseable verdict never
clears a gate.

Related runtime guards: stagnation detection (opt-in via `stagnation_rounds`, so
it never overrides an author's explicit `max_retries`), a measured cost ceiling
(`budget_usd`, from the driver's own accounting), and a per-run kill switch — a
`STOP` file in the run directory stands the run down at the next node boundary.

---

## 8. Proving a flow before it costs anything

```bash
touring adw lint <name>     # structure: edges, cycles, budgets, fan-out, persona
touring adw test <name>     # walk the graph with agents mocked
touring adw run <name> --resume-run <id>   # replay after a crash
```

`touring adw test` walks a graph that never ran: an agent with no recording is
stood in for by a stub, and a human gate is auto-approved. Both are **named in
the output** under `synthesized` — a synthesized walk proves the graph connects,
never that the agent behaves, and must never read as a replayed one. A spec that
*declares* `driver = "mock"` still demands its recording, so synthesis never
leaks into a real run.

### Common lint errors

| Message | What to do |
|---|---|
| `merge must be declared` | pick `collect`/`tally`/`concat`; there is no safe default |
| `max_branches must be >= 1` | declare the ceiling; the budget lint needs it |
| `cycle without exit: x → y` | the loop has no edge that leaves it |
| `branch may use ['Edit'…]` | fan-out is read-only; move the write after the join |
| `gate → agent … blind to the verdict` | reference `{{nodes.<gate>.summary}}` in the agent's prompt |
| `template has a fixed lens` | bind it to `{{branch.value}}` |
| `\`policy\` is not a field` | the pass condition is `on_branch_fail` |

---

## 9. Design decisions

| Decision | ADR |
|---|---|
| Composition by load-time inlining, not runtime subgraphs | [0002](../adr/0002-adw-fragment-composition-by-inlining.md) |
| Fan-out is read-only, with merge and failure policy declared | [0003](../adr/0003-adw-read-only-fanout-explicit-policies.md) |
| Three verdicts (PASS/REJECT/ESCALATE), REJECT by omission | [0004](../adr/0004-adw-three-verdict-verification-contract.md) |
| Subtask claiming is a conditional UPDATE, not a read | [0005](../adr/0005-decompose-atomic-subtask-claim.md) |

## 10. References

- Runner + full CLI surface — `~/.claude/skills/Touring/SKILL.md`
- Analysis, sources and measurements behind the design —
  `docs/plans/2026-08-18-graph-engineering-flow-portfolio/cartografia-de-fluxos.html`
- Implementation record with executed gates —
  `docs/plans/2026-08-18-graph-engineering-flow-portfolio/plan.md`
- Convergence discipline that consumes these flows —
  `~/.claude/skills/loop-engineering/SKILL.md`
