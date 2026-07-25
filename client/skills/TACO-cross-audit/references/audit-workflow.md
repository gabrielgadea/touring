# Cross-Audit Workflow — The Seven Phases

The full pipeline. Phases 1-4 are read-only discovery, phase 5 modifies code,
phases 6-7 prove and record. Announce each phase as you enter it.

## Table of contents

- [Phase 1 — MAP](#phase-1--map)
- [Phase 2 — PURPOSE AUDIT](#phase-2--purpose-audit)
- [Phase 3 — DEBT SCAN](#phase-3--debt-scan)
- [Phase 4 — HARMONY CHECK](#phase-4--harmony-check)
- [Phase 5 — FIX & POTENTIALIZE](#phase-5--fix--potentialize)
- [Phase 6 — E2E PROOF](#phase-6--e2e-proof)
- [Phase 7 — REPORT](#phase-7--report)
- [Delegating to touring-auditor](#delegating-to-touring-auditor)

## Phase 1 — MAP

Establish the full relation map of the target tree. You cannot audit what you
have not mapped, and you cannot leave in harmony connections you never saw.

```bash
touring ast workspace-info                 # crates, features, cross-crate deps
touring wiring chains                      # source→sink module graph
touring ast blast <entry-file>             # full dependency tree from the entry point
```

For each significant file, `touring ast meta <file> --depth summary -j` gives
blast_radius and quality. Record: the module graph, the public API surface, the
entry points, and which files have blast_radius > 10 (audit those first — a
defect there propagates furthest).

Output of this phase: a relation map naming every module, its exports, its
consumers, and the flows that cross them.

## Phase 2 — PURPOSE AUDIT

For each symbol / module / crate, compare **documented purpose** against **real
behavior**. The documented purpose is found in docstrings, the module name, the
README, the type's doc comment. The real behavior is what the code actually does.

Classify each unit:

| Verdict | Meaning |
|---------|---------|
| `FULFILLS` | behavior matches the documented purpose |
| `PARTIAL` | does some of what it claims; gaps named |
| `VIOLATES` | behavior contradicts the stated purpose |
| `UNDOCUMENTED` | no purpose stated — cannot audit fidelity; document it, then audit |
| `UNVERIFIED` | could not be executed/inspected — never report as passing |

A `PARTIAL` or `VIOLATES` verdict feeds Phase 5. An `UNDOCUMENTED` unit is itself
a defect — a symbol with no stated purpose cannot orchestrate anything reliably.

This is the phase to delegate to `touring-auditor` for large trees — see below.

## Phase 3 — DEBT SCAN

Run the deterministic debt scanner over the whole tree:

```bash
python3 ~/.claude/skills/TACO-cross-audit/scripts/scan_debt.py <target-dir>
```

It finds every: `allow(unused)` / `allow(dead_code)`, `TODO` / `FIXME` / `HACK` /
`XXX`, `unimplemented!()` / `todo!()` / `panic!("not …")`, `pending` / `WIP`
marker, and planned-but-absent feature reference. Each hit is a phase-5 task —
nothing on this list is allowed to survive the audit (Hard Rule #4).

## Phase 4 — HARMONY CHECK

Verify every connection is sound:

```bash
python3 ~/.claude/skills/TACO-cross-audit/scripts/harmony_map.py <target-dir>
```

It aggregates `touring wiring orphans`, `touring wiring audit`, and
`touring wiring cycles` into one report: orphan pub symbols (exported, no
consumer), modules scoring < 1.0, broken or unintended dependency cycles, unused
imports. Every disharmony is a phase-5 task.

If `touring` is degraded, the script falls back to `grep`-based orphan detection
and marks the report `daemon_degraded`.

## Phase 5 — FIX & POTENTIALIZE

Resolve every finding from phases 2-4. The direction is fixed by REGRA #0 — see
[potentialization.md](potentialization.md). In short: integrate, wire, implement;
never delete capability to silence a warning.

Apply every code change through `Edit tool` . Pre-existing
errors are fixed here too — origin is irrelevant (Hard Rule #5). Show the user the
diff before applying anything substantial.

After phase 5, re-run the phase 3 and phase 4 scans — they must come back clean.

## Phase 6 — E2E PROOF

Create end-to-end tests that exercise the **integrated** flow, not isolated
functions. Then **run them** and show the run. See [e2e-proof.md](e2e-proof.md).
Prove the invariants:

```bash
python3 ~/.claude/skills/TACO-cross-audit/scripts/prove_invariants.py <target-dir>
touring e2e -j                              # composite system health
```

A test written but not executed is not proof (Hard Rule #7).

## Phase 7 — REPORT

Produce the audit report from `assets/audit-report-template.md`. It carries the
**executed evidence** — commands run and their output — not assertions. Persist:

```bash
touring memory store "cross-audit:<target>:<date>" "<verdict + key findings>" --tier semantic
touring learning reward orchestrate <value> "cross-audit of <target>"
touring diary write taco-cross-audit "<summary>" --aaak
```

## Delegating to touring-auditor

For a large tree, Phase 2 (and a deep pass of Phase 6) can be delegated to the
`touring-auditor` subagent via the `Agent` tool. It runs the per-symbol
purpose-fidelity audit and E2E proof at scale and returns raw JSON. Use it when
the tree is too large to audit symbol-by-symbol inline; keep the orchestration,
the phase ordering, and the REGRA #0 direction here in this skill. The subagent
audits; this skill decides and proves.
