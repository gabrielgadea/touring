# 0004 — A gate speaks three verdicts, and silence is REJECT

- Status: accepted
- Date: 2026-08-18
- Deciders: Gabriel Gadea (plan `2026-08-18-graph-engineering-flow-portfolio`, B6)

## Context

The runner decided a gate by `exit_code == 0`. Under that reading a check that
**could not run** — broken environment, missing dependency, network down — is
indistinguishable from a check that ran and rejected. Both consume the retry
budget, and the first spends every attempt re-invoking an agent against something
no agent can fix.

The practice is canonical elsewhere: a reference implementation of loop
engineering names "treating possible output as proved output" as the principal
production failure, and prescribes a verifier that rejects until shown otherwise
plus an escalation path distinct from failure.

## Decision

A gate with `verdict_contract = true` emits `VERDICT=PASS|REJECT|ESCALATE` and
**must** declare `on_escalate`.

- `PASS` — ran, cleared the bar → `on_pass`.
- `REJECT` — ran, did not clear → `on_fail`, consuming a retry.
- `ESCALATE` — could not run → `on_escalate`, **without** consuming a retry.

**The default verdict is REJECT.** An unparseable or absent verdict never clears a
gate; approval is never granted by omission.

Three runtime guards accompany it: stagnation detection over the gate ledger,
a measured cost ceiling from the driver's own accounting, and a per-run kill
switch (a `STOP` file stands the run down at the next node boundary).

## Consequences

- A broken environment now routes aside instead of burning the budget, and the
  distinction is visible in the journal.
- **Stagnation detection is opt-in** (`stagnation_rounds`, default 0). A gate that
  fails silently — `exit 1`, no output — is trivially "stagnant"; on by default it
  would cut a run at two attempts although its author wrote `max_retries = 5`, a
  silent default overriding an explicit declaration. That is precisely the failure
  class the contract exists to remove, so it must not be introduced by the fix.
- Authoring cost: a contract gate must emit a parseable line. That is deliberate —
  it is what makes termination a decision of code rather than a reading of prose.

## References

- `adw.py` — `parse_verdict`, `DEFAULT_VERDICT`, `_next_edge`, `_is_stagnant`
- `docs/explanation/adw-flow-portfolio.md` §7
