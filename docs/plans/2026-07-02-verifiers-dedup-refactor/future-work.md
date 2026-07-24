---
type: FutureWork
title: Data-driven verifier redesign (F1.3 Gold) — deferred option (a)
description: The only path to a legitimate dir-F1.3 ≥ Gold; deferred from Loop run #1 as an L5 architectural redesign.
plan_id: task_1783004397291527687
tags: [loop, future-work, touring-quality, F1_3, redesign, deferred]
timestamp: 2026-07-02T15:00:00Z
okf_version: "0.1"
---

# Future work — data-driven verifier redesign (option a)

Part of the [bundle](/index.md). Deferred from run #1 (see [/log.md](/log.md)).

## Context (what run #1 established)

Loop run #1 removed all *extractable* cross-file duplication in the 50 verifiers
(`strip_rust_comments_and_strings` 2→1, the `finish` tail 50→1,
`is_detector_own_source` 35→1) — behavior-preserving, **344 tests green, 0
warnings**. The residual `dir`-F1.3 = **28.9% (80 clone blocks)** is the
**inherent structural similarity** of 50 parallel `impl Verification` blocks.
F1.3-`ScopeNative` reports this **correctly** — it is a deliberate cross-file
Type-1 detector (W4 2026-07-02), not a bug. Per-file every verifier is
F1.3 = 1.0 (Diamond). So `dir`-Silver is the honest, correct state.

## The deferred option (a)

The only path to a legitimate `dir`-F1.3 ≥ Gold is to remove the structural
repetition **at the source**: redesign the 50 verifiers from 50 hand-written
`impl Verification` blocks into a **data-driven / declarative** form:

- a `verifier!` macro that generates the `impl` + `check` glue from a spec, **or**
- a table of `(DimId, analyze_fn, AggKind)` rows dispatched by ONE generic runner,
  so each dim contributes only its unique `analyze_*` logic (no repeated impl
  scaffolding, no repeated fallback / `lang_from_ext` / early-return shapes).

## Why it was deferred (not done in run #1)

- **Size**: L5 architectural redesign, not "dedup boilerplate" (run #1's scope).
- **Trade-off**: macro/table indirection can make individual verifiers harder to
  read and debug — the explicit-per-dim design was a deliberate readability choice.
- **Value is debatable**: F1.3 flagging a *family* of 50 similar verifiers is
  arguably a false alarm for this code shape; the similarity is intentional
  structure, not copy-paste debt.

## Lighter-weight alternative (recommended to evaluate first)

Before a full redesign, consider a **family-aware F1.3 exemption**: when a scope
is a directory of sibling trait-impls (a verifier family / plugin registry),
apply a higher duplication threshold or exclude the shared `impl` scaffold from
the corpus. This is a **harness calibration refinement** (part of the 9-wave
`docs/plans/2026-07-02-harness-50dim-reform/`), far cheaper than redesigning the
verifiers, and it keeps the explicit-per-dim readability.

## Entry point for the future session

- DAG task (this run): `task_1783004397291527687` (P1–P4 done, P5 = accept-(c)).
- Touring memory: `future-work:verifiers-data-driven-redesign:2026-07-02`,
  `correction:f1_3-scopenative-is-deliberate:2026-07-02`.
- Files: `crates/touring-quality/src/verifications/` (50 `fX_Y_*.rs` + `mod.rs`),
  `crates/touring-quality/src/aggregate.rs` (`AGG_TABLE`).
