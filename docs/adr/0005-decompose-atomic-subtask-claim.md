# 0005 — Taking a subtask is a conditional UPDATE, not a read

- Status: accepted
- Date: 2026-08-18
- Deciders: Gabriel Gadea (plan `2026-08-18-graph-engineering-flow-portfolio`, C2)

## Context

`touring decompose ready <task>` filters correctly by status and **only reads**.
No operation in the crate claimed a subtask in the same step — no `claim`,
`take_next` or `acquire`. The pattern was read-then-write with an open window, so
two concurrent sessions polling `ready` received the same subtask and both marked
it in progress. `ready_subtasks` has 66 call sites, which made changing it risky.

The same defect had already been fixed elsewhere in the system (the per-session
loop marker, August 2026), and the remedy is stated by the Wayfinder source: the
frontier is *open ∩ unblocked ∩ **unclaimed***, and a session claims before it
works.

## Decision

Add claiming as a **new** operation rather than changing `ready`:

```
touring decompose claim <task> --owner <id> [--lease-secs N]
touring decompose release <task> <subtask> --owner <id>
```

`claim` is a conditional `UPDATE … WHERE status = 'pending' AND claimed_by IS
NULL`. SQLite serialises writers, so exactly one of N racing claims reports a
changed row; the losers receive `claimed: false` with a reason. The claim carries
an expiring lease, so a session that dies mid-work frees its subtask.

The 66 existing call sites are untouched and migrate by choice.

## Consequences

- Verified under real concurrency: six simultaneous claims against two subtasks
  produced exactly two claims and four refusals, in each of the three consuming
  projects after the 30.4.1 propagation.
- `ready` keeps its read-only semantics, so nothing that only inspects the DAG
  changed behaviour. The cost is that a caller must *choose* `claim`; `ready`
  alone remains unsafe for concurrent workers, which the CLI help says plainly.
- Companion decision (same delivery): `frontier` reports fog as `unknown` when a
  ticket was never assessed. Defaulting to `clear` made the frontier answer "no
  uncertainty here" about work nobody had looked at — a fail-open on the one axis
  the feature exists to surface. `kind` still defaults to `implementation`,
  because an unlabelled ticket genuinely *was* implementation work before this
  existed; unmeasured fog has no such prior meaning.

## References

- `crates/touring-cli/src/cli/handlers/decompose.rs` — `cli_decompose_claim`, `cli_decompose_frontier`
- `crates/touring-hooks/tests/cli_handlers_e2e.rs` — `two_sessions_claiming_never_receive_the_same_subtask`, `unassessed_fog_reports_as_unknown_not_clear`
