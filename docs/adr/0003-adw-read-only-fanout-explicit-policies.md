# 0003 — Fan-out is read-only, and its merge and failure policies are declared

- Status: accepted
- Date: 2026-08-19
- Deciders: Gabriel Gadea (plan `2026-08-18-graph-engineering-flow-portfolio`, B4)

## Context

The runner walked one node at a time (`current: str`, `_next_edge() -> str`),
which made the diamond — split, work in parallel, check, merge — inexpressible.
Measurement showed fan-out is **not** a performance win here: the `audit` flow's
three independent nodes take 0.1–0.2 s in total, and `strategy-loop` is dominated
by one 30 s node, so parallelism buys ~15 %. The argument for it is expressiveness
— several lenses where there is one, a panel of critics where there is a binary
gate — not wall-clock.

Every fan-out the sources actually ask for is **read-only**: researchers with
different lenses, blind critics, exploration lenses. None writes.

## Decision

1. **Read-only branches.** A branch whose `allowed_tools` include a write tool is
   a lint error. Parallel writers race and no journal ordering can reconstruct who
   clobbered whom. Concurrent writing already has two answers in the repo —
   `race` (copy per lane) and `conflict-check` (declared write-sets).
2. **`merge` is mandatory** (`collect|tally|concat`) with no default. Results are
   stored under the node's name; N branches sharing one slot is the
   last-write-wins that LangGraph documents as its default, and it loses evidence
   silently.
3. **`on_branch_fail` is mandatory in effect** — `all` (fail-closed) · `any` ·
   `ignore`/`best_effort` · `quorum:N`. The LangGraph docs do not specify what a
   failed branch means; Law L2 requires that we do.
4. **`max_branches` is required and checked before any branch runs.** A
   runtime-sized fan-out with no ceiling multiplies the per-branch cost by a
   number nobody declared, and the budget lint would under-count by that factor.
   Exceeding it refuses the block rather than truncating the work.
5. **Class-D is per branch.** The single `last_agent` slot became a race under
   fan-out, degrading Law L3 exactly when more agents are in play. A `parallel`
   block replaces the standing narrative claim only if it *made* one.
6. **No `policy` field.** The design document carries both `policy` and
   `on_branch_fail` for the same decision, and its own example contradicts itself
   (`policy = "quorum:2"` beside `on_branch_fail = "all"`). Accepting both would
   ship that contradiction, so the lint refuses `policy` and names the field that
   owns the semantics.

## Consequences

- The canonical *parallelization* variants are both expressible: sectioning
  (`merge = "concat"`) and voting (`merge = "tally"` plus a quorum). With dynamic
  branches (`branches = "{{vars.x}}"` + `template`) *orchestrator-workers* is too.
- `quorum:N` on `on_branch_fail` answers "did enough branches run clean", which is
  a different question from what a downstream verdict gate answers. Both exist.
- Writing fan-out stays deliberately verbose. That is the point: each declaration
  removed a way for the block to fail silently.

## References

- `adw.py` — `run_parallel_node`, `_branch_passed`, `_lint_parallel`, `_track_class_d`
- `docs/explanation/adw-flow-portfolio.md` §5
