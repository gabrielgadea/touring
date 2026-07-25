# Purpose-Fidelity — Auditing What Code Is *For*

A unit test asks "does this function crash?". A cross-audit asks "does this code
do what its purpose says it does?". This file is how you answer the second
question rigorously.

## Table of contents

- [Why the distinction matters](#why-the-distinction-matters)
- [Finding the documented purpose](#finding-the-documented-purpose)
- [The four things a cross-audit checks](#the-four-things-a-cross-audit-checks)
- [Verdict criteria](#verdict-criteria)

## Why the distinction matters

Code can pass every unit test and still fail its purpose:

- A function `validate_config()` that always returns `Ok` passes its "does not
  crash" test — and violates its purpose (it validates nothing).
- A module documented as "orchestrates the import flow" can have every part
  green in isolation while the *flow* never actually runs end to end.
- An exported symbol can be correct and complete — and orphaned, so it serves no
  purpose at all because nothing reaches it.

The unit test verifies the part. The cross-audit verifies the part *is doing its
job in the whole*. Code is an instrument for orchestrating process flows toward a
result — fidelity is measured against that result, not against "no exception".

## Finding the documented purpose

Before you can audit fidelity you must know what the unit claims to do. Sources,
in priority order:

1. The doc comment / docstring on the symbol.
2. The symbol name and signature (a name is a contract: `parse_invoice` claims to
   parse invoices).
3. The module's own doc and name.
4. The README / design doc / the purpose stated in the SKILL or the project docs.

If none of these state a purpose, the verdict is `UNDOCUMENTED` — and that is
itself a defect. A symbol with no stated purpose cannot be a reliable part of an
orchestration; Phase 5 documents it (stating the purpose its behavior implies),
then it is audited.

## The four things a cross-audit checks

### 1. Interface contracts

Does the unit honor what its signature and docs promise to callers? Return types,
error variants, nullability, the meaning of each parameter. A function whose docs
promise a sorted result must return a sorted result for every caller.

### 2. Invariants

Properties that must hold on every path. The canonical one for a CLI / hook /
tool: **exit 0 always** (or the documented exit-code contract) — the program
never crashes the caller. Others: idempotence where claimed, no partial writes,
no state left behind on the error path. Prove an invariant by running the unit
across inputs and showing it holds — assertion is not proof.

### 3. Edge-case behavior

Not the edge cases the author happened to test — the edge cases the *purpose
implies*. A parser's purpose implies empty input, malformed input, the largest
realistic input. A merge's purpose implies the conflict case. List the edge cases
the purpose demands, then check each.

### 4. Integration between components

The flow across units. A → B → C: each may be individually correct while the
hand-off A→B drops a field, or B→C swaps an argument. The cross-audit follows the
data across the boundary and verifies the contract holds at every seam. This is
where `touring wiring chains` and the E2E proof do their work.

## Verdict criteria

| Verdict | Award when |
|---------|------------|
| `FULFILLS` | contract honored, invariants hold, purpose-implied edge cases handled, integrates cleanly — *and you ran it to confirm* |
| `PARTIAL` | core purpose met, but a named contract clause, invariant, or edge case is not — list each gap explicitly |
| `VIOLATES` | behavior contradicts the stated purpose |
| `UNDOCUMENTED` | no purpose stated — document it first, then audit |
| `UNVERIFIED` | could not execute or fully inspect — never substitute for `FULFILLS` |

`FULFILLS` is never awarded on reading alone. The proof discipline holds here: a
verdict of `FULFILLS` means a command was run and its output confirmed the
behavior. Anything you could only read, not run, is `UNVERIFIED`.
