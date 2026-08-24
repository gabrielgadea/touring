# Architecture Decision Records (ADRs)

This directory holds **Architecture Decision Records** — short documents capturing
a significant architectural decision, its context, and its consequences. Format:
[MADR](https://adr.github.io/madr/) (lightweight).

## Why

Decisions without a record get re-litigated or silently violated. An ADR captures
the *why* that the code alone can't express, so a future contributor (or LLM) can
understand a constraint without archaeology.

## How

- One decision per file: `NNNN-kebab-title.md` (zero-padded, monotonic).
- Status: `proposed` → `accepted` → (`superseded by NNNN` | `deprecated`).
- An accepted ADR is **immutable**; to change a decision, write a new ADR that
  supersedes it (and set the old one's status).

## Relationship to RFCs and the Constitution

The 5 constitutional RFCs (`RFC-001..005`) + `CONSTITUTION-v8.md` live in `docs/`
and govern the system-wide contract. ADRs here record narrower, crate- or
subsystem-level decisions.

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-web-dashboard-loopback-default.md) | touring-web dashboard binds loopback by default | accepted |
| [0002](0002-adw-fragment-composition-by-inlining.md) | ADW fragments compose by load-time inlining, not runtime subgraphs | accepted |
| [0003](0003-adw-read-only-fanout-explicit-policies.md) | Fan-out is read-only, and its merge and failure policies are declared | accepted |
| [0004](0004-adw-three-verdict-verification-contract.md) | A gate speaks three verdicts, and silence is REJECT | accepted |
| [0005](0005-decompose-atomic-subtask-claim.md) | Taking a subtask is a conditional UPDATE, not a read | accepted |
