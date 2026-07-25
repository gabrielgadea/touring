# REGRA #0 — Potentialize, Never Reduce

The direction of every correction. When the audit finds a defect, the fix must
make the code *more* capable as an orchestration instrument — never less. This
file is the strategy catalog, plus the honest boundary between potentialization
and scope creep.

## Table of contents

- [The principle](#the-principle)
- [Strategy catalog](#strategy-catalog)
- [How to wire an orphan](#how-to-wire-an-orphan)
- [How to integrate dead code](#how-to-integrate-dead-code)
- [The honest boundary — potentialization is not invention](#the-honest-boundary)

## The principle

A defect is evidence of unrealized capability, not of excess. `allow(dead_code)`
on a function means a capability was built and never connected — the loss is the
*disconnection*, not the function. The reducing fix (delete the function) throws
away the capability. The potentializing fix (wire the function to a consumer)
realizes it. Same warning silenced; opposite outcome.

Reducing scope to make a warning go away is always the wrong fix. It is the audit
lying to itself — the warning is gone, the capability is gone too, and the code
is a weaker instrument than before the audit "improved" it.

## Strategy catalog

| Finding | Reducing fix (forbidden) | Potentializing fix (required) |
|---------|--------------------------|-------------------------------|
| `allow(dead_code)` on a symbol | keep the allow / delete the symbol | wire the symbol to a real consumer |
| Orphan pub symbol | delete it | connect it to callers, or expand its role |
| Unused import | delete the import | integrate the module it pulled in |
| `unimplemented!()` / `todo!()` | delete the branch | implement the branch |
| Planned feature, only a TODO | delete the TODO | build the feature |
| Pre-existing compile error | `#[allow]` / comment it out | fix the root cause |
| Failing edge case | narrow the input contract | handle the edge case |
| Function with no caller | delete it | find the flow that needs it and wire it in |
| Half-built feature | revert it | finish it |

## How to wire an orphan

An orphan pub symbol is a capability with no consumer. To wire it:

1. Read the symbol's purpose (its doc, its name, its signature).
2. Find the flow that *should* use it — `touring wiring chains` and the Phase 1
   relation map show which module's purpose the symbol serves.
3. Connect it: call it from the consumer that the purpose implies, or — if the
   capability is genuinely broader than any current caller — expand a consumer's
   purpose to use it. Apply the edit via `Edit tool`.
4. Re-run `touring wiring orphans` — the symbol is no longer orphaned.

If, after honest analysis, an orphan truly serves no flow and no purpose, that is
the one case where removal is correct — but it must be *stated and justified* in
the report, not done silently.

## How to integrate dead code

`allow(dead_code)` is a capability the codebase decided to keep but never use.
Integration: identify what the dead symbol was *for* (its purpose), find the live
flow that purpose belongs to, and connect them. Often the dead code is the better
implementation of something the live code does worse — in which case integration
means routing the live flow through it. The `allow` attribute is removed only
*after* the symbol has a real consumer, never before.

## The honest boundary

Potentialization is **realizing the purpose the code already has** — not bolting
on features nobody asked for. The boundary:

- Wiring an orphan, implementing a `todo!()`, fixing an edge case, finishing a
  half-built feature — these complete an *existing, documented* intent. Always do
  them.
- Inventing a new capability the purpose never implied, gold-plating, adding
  configuration nobody needs — that is scope creep. It is not REGRA #0; it is a
  different (worse) failure.

The test: does the change make the code fulfill *its own stated purpose* more
completely? If yes, it potentializes — do it. If it adds a purpose the code never
claimed, stop and ask the user. REGRA #0 maximizes the realization of intent, not
the invention of it.
