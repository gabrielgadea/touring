---
name: TACO-cross-audit
description: Cross-audit a code directory for purpose-fidelity — prove, in practice, that what was implemented actually fulfills its documented purpose, not merely that it does not crash. Use whenever the user asks to audit implemented code, run a cross-audit / auditoria cruzada, prove the full flow works end-to-end, verify wiring / AST / blast-radius / imports / exports are in harmony, eliminate dead code and allow(unused) / allow(dead_code), resolve every TODO / FIXME / pending / unimplemented marker, wire orphan symbols, implement planned-but-missing features, fix pre-existing errors regardless of origin, or create E2E tests that prove integration. Triggers on "auditoria cruzada", "cross-audit", "prove it works", "verifique o fluxo completo", "me prove na prática", "audit everything implemented", "leave it in perfect harmony". Every correction must potentialize scope (REGRA #0), never reduce it.
---

# TACO-cross-audit — Purpose-Fidelity Auditing

You are auditing a body of implemented code. The question is not *"does it
crash?"* — that is a unit test's job. The question this skill answers is
**"does the code fulfill its documented purpose?"** — and it answers by
*proving it in practice*, with executed evidence, never assertion.

The framing: code is **an instrument for orchestrating process flows toward an
objective and a result**. Every symbol, function, module, crate, and flow is
audited against that — its fidelity to the final purpose, at every granularity.

## The core distinction

| | Unit test | Cross-audit |
|---|-----------|-------------|
| Asks | "does it crash?" | "does it do what its purpose says?" |
| Checks | one function in isolation | interface contracts, invariants, integration |
| Evidence | green / red | executed proof of the whole flow |
| Edge cases | the ones the author thought of | the ones the *purpose* implies |

A cross-audit verifies **interface contracts, invariants (e.g. exit 0 always),
edge-case behavior, and integration between components** — and proves the result
by running it. Detail in [references/purpose-fidelity.md](references/purpose-fidelity.md).

## The seven phases

Work them in order, announce each one. Full detail, with the per-phase Touring
commands and the `touring-auditor` delegation, is in
[references/audit-workflow.md](references/audit-workflow.md).

| # | Phase | Output |
|---|-------|--------|
| 1 | **MAP** | Full relation map of the target tree — AST, wiring, blast radius, imports/exports, module graph |
| 2 | **PURPOSE AUDIT** | Per symbol / module / crate: documented purpose vs real behavior |
| 3 | **DEBT SCAN** | Every dead-code / `allow(unused)` / TODO / FIXME / `unimplemented!` / pending marker / planned-not-built feature |
| 4 | **HARMONY CHECK** | Every connection sound — zero orphans, zero broken cycles, zero unused imports |
| 5 | **FIX & POTENTIALIZE** | Pre-existing errors fixed; orphans wired; pending features built; dead code *integrated* |
| 6 | **E2E PROOF** | E2E tests created and *run* — proving integration, exit 0, contracts, edge cases |
| 7 | **REPORT** | Audit report carrying the executed evidence |

Phases 1-4 are read-only discovery; phase 5 modifies code; phases 6-7 prove and
record. Never reorder — you cannot leave in harmony what you never mapped.

**Deterministic exit artifact (flow guard, 2026-07-23)**: the phase-7 report
MUST be written to `<scope>/docs/audits/cross-audit-<date>.md` — invoking this
skill arms a per-project marker (`loop_outer_arm.py`, flow `cross-audit`) and
the Stop hook refuses to end the turn until that file exists
(`flow_manifests.json` via `loop_outer_gate.py`): the audit's completion is
judged by the artifact on disk, never by the auditor's narrative. `memory
store` / `diary write` remain the institutional record; the file is the gate.

## REGRA #0 — potentialize, never reduce

This is the direction every correction must take. When the audit finds a defect,
the fix *expands* capability — it never deletes scope to silence a warning.

| Finding | WRONG (reduces) | RIGHT (potentializes) |
|---------|-----------------|------------------------|
| `allow(dead_code)` | keep the allow, or delete the symbol | wire the symbol to a real consumer |
| Orphan pub symbol | delete it | connect it to callers, or expand its purpose |
| Unused import | delete the import | integrate the module it brought in |
| Planned feature, not built | delete the TODO | implement the feature |
| Pre-existing error | `#[allow]` it away | fix the root cause |
| Failing edge case | narrow the input contract | handle the edge case |

If a fix would shrink what the code can do, it is the wrong fix. Find the one
that makes the code *more* of an orchestration instrument, not less. The full
strategy catalog is in [references/potentialization.md](references/potentialization.md).








## Hard rules

1. **Map before fixing.** Phases 1-4 (read-only) complete before phase 5 touches
   anything. You cannot leave in harmony what you never mapped.
2. **Prove, do not assert.** Every "it works" carries an executed command and its
   output. No output → mark `UNVERIFIED`.
3. **Potentialize, never reduce (REGRA #0).** A correction that shrinks scope is
   the wrong correction. Integrate, wire, implement — do not delete capability.
4. **Nothing pending.** No TODO / FIXME / `unimplemented!` survives the audit; no
   planned feature is left unbuilt. "Later" is not an audit outcome.
5. **Fix pre-existing errors too.** Origin is irrelevant — an error that exists
   gets fixed, regardless of who or when created it.
6. **Apply code fixes via `Touring-native tooling`.** edição-com-gate — `Edit tool` for changes,
   never raw Edit/Write on code files.
7. **E2E tests are run, not just written.** A test that was not executed is not
   proof.
8. **50-dim elite gate is part of "in harmony".** No tree is left audited until
   the 6 BLOCK dims (P0) pass and the delivery floor is Gold (0.80) —
   `touring-quality check --gate <dim>` + `touring-quality score --fail-below 0.80`.
   A P0 FAIL (secret / OWASP / CVE / config / deprecated / EOL pkg) is remediated
   via `Edit tool` (REGRA #0), never suppressed. Keystone:
   `~/.claude/rules/elite-50-quality.md`.

## Reference map

| Topic | File |
|-------|------|
| The 7 phases in detail + `touring-auditor` delegation | [references/audit-workflow.md](references/audit-workflow.md) |
| Purpose-fidelity — cross-audit vs unit test, contracts, invariants | [references/purpose-fidelity.md](references/purpose-fidelity.md) |
| REGRA #0 potentialization strategy catalog | [references/potentialization.md](references/potentialization.md) |
| Building E2E tests that prove integration | [references/e2e-proof.md](references/e2e-proof.md) |
| Deep per-symbol audit subagent | the `touring-auditor` agent |
---

## Referencia movida (compactação 65, 08/10/2026)

Esta skill ficou abaixo do corte de 20.000 caracteres do Claude Code, para que o procedimento inteiro chegue ao modelo depois de uma compactação. O material abaixo foi movido **verbatim** para [`references/cross-audit-camadas.md`](references/cross-audit-camadas.md) — nada foi perdido, apenas movido.

**No arquivo movido:** Disciplina de prova, elite 50-dim, scripts da camada 3, enriquecimento de contexto, integração Touring e as hard rules de causa-raiz.
