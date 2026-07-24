# Plan — Wave A — ZERO-CODE Reflexos Cardinais. Wire 9 missing Claude Code hook events in ~/.claude/settings.json to existing touring-cortex handlers: TeammateIdle (run_teammate_idle_gate at team_hooks.rs:52), ConfigChange (lifecycle.rs:1011 area), WorktreeCreate (lifecycle.rs:905), WorktreeRemove, StopFailure (StopFailureHandler at lifecycle.rs:542), Notification (lifecycle.rs:1149), Elicitation (lifecycle.rs:1255), ElicitationResult (lifecycle.rs:1315), PermissionDenied. ZERO LOC Rust necessary — handlers already exist in touring-cortex/src/handlers/lifecycle.rs. Validation via touring gate-metrics total_invocations delta after triggering each event.

> **Generated**: 2026-05-03T19:14:34.541774Z | **Task ID**: `task_1777835634668240326` | **CILA Level**: 3 | **Generator**: `taco-forge plan --quality high` v1.14.0
> **Provenance**: every section traces to a Touring CLI invocation. **Re-render is idempotent** — modify the underlying DAG, not this file.

## 01. Quality dimensions enforced

| Dim | Aspect | Mechanism in this plan |
|-----|--------|------------------------|
| **a** | Precision | VGP-verified symbols + Touring CLI evidence |
| **b** | Scalability | DAG decomposition; per-phase isolation |
| **c** | Performance | discover ~3s; VGP cached via memory store |
| **d** | Applicability | 31 generator kinds; 10 assist handlers |
| **e** | Code Quality | TDD enforced; clippy 0; tdg >= B |
| **f** | Detail | validation script per phase; cross-audit final |
| **g** | Systemic Integration | wiring orphans delta == 0; cycles 0 |
| **h** | Dependencies | cargo update + workspace-info checked |
| **i** | Potentialization | REGRA #0 — orphans wired; deliverables max scope |

## 02. Final goal

Wave A — ZERO-CODE Reflexos Cardinais. Wire 9 missing Claude Code hook events in ~/.claude/settings.json to existing touring-cortex handlers: TeammateIdle (run_teammate_idle_gate at team_hooks.rs:52), ConfigChange (lifecycle.rs:1011 area), WorktreeCreate (lifecycle.rs:905), WorktreeRemove, StopFailure (StopFailureHandler at lifecycle.rs:542), Notification (lifecycle.rs:1149), Elicitation (lifecycle.rs:1255), ElicitationResult (lifecycle.rs:1315), PermissionDenied. ZERO LOC Rust necessary — handlers already exist in touring-cortex/src/handlers/lifecycle.rs. Validation via touring gate-metrics total_invocations delta after triggering each event.

## 03. Consequences (impacts on multiple perspectives)

- Codebase consequences: deliverables added/modified per phase; orphan delta == 0.
- Testing consequences: test files generated BEFORE impl (TDD); validation scripts run per phase.
- Memory consequences: outcome persisted via `touring memory store --tier semantic`.
- RL consequences: reward injected per phase + final audit (closes feedback loop).
- Documentation consequences: plan + validators + audit script tracked under plan/.

## 04. Success criteria

Default gate (TACO Delivery Checklist):
- All phases completed (`touring decompose status` → 100%)
- Cross-audit script PASSES (`audit-plan-completion.sh` exit 0)
- VGP: zero `BLOCKED` symbols
- Wiring: zero new orphan pub symbols (REGRA #0)
- E2E: `touring e2e --depth standard` composite ≥ 0.7

## 05. DISCOVER snapshot

| Signal | Value | Source |
|--------|-------|--------|
| Daemon healthy | `True` | `touring doctor -j` |
| Composite health score | `0.00` | `touring status -j` |
| Symbol count | `0` | `touring status -j` |
| Orphan pub symbols | `201,564` | `touring wiring orphans -j` |
| Cycle count | `2` | `touring wiring cycles` |
| EMA reward (RL) | `0.0000` | `touring status -j` |
| Drift alert | `degraded` | `touring evolution drift -j` |
| E2E composite | `0.00` | `touring e2e --depth standard -j` |
| Synergy wired pairs | `50` | `touring synergy --with-metrics -j` |
| Workspace packages | `0` | `touring ast workspace-info` |

**Known gotchas active**: 20 pitfall(s) flagged

## 06. VGP — Verified Generation Protocol report

**Tally**: verified=4, not_found=9, blocked=0, skipped=0

| Symbol | Status | Blast | Files | Evidence |
|--------|--------|-------|-------|----------|
| `ZERO` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: FOUND: ZERO at projects/analise/pac |
| `CODE` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: CODE |
| `TeammateIdle` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: TeammateIdle |
| `ConfigChange` | `verified` | 0 | — | VGP V1+V2 PASS; blast_radius=0. V2 evidence: NOT FOUND: ConfigChange |
| `WorktreeCreate` | `not_found` | 0 | — | touring index find returned no results |
| `WorktreeRemove` | `not_found` | 0 | — | touring index find returned no results |
| `StopFailure` | `not_found` | 0 | — | touring index find returned no results |
| `StopFailureHandler` | `not_found` | 0 | — | touring index find returned no results |
| `ElicitationResult` | `not_found` | 0 | — | touring index find returned no results |
| `PermissionDenied` | `not_found` | 0 | — | touring index find returned no results |
| `run_teammate_idle_gate` | `not_found` | 0 | — | touring index find returned no results |
| `team_hooks` | `not_found` | 0 | — | touring index find returned no results |
| `total_invocations` | `not_found` | 0 | — | touring index find returned no results |

## 07. Phases overview

| ID | Title | Deps | Status | Effort | Validator |
|----|-------|------|--------|--------|----------|
| `P1` | Bootstrap (placeholder) | — | `pending` | 4.0h | `validate-phase-P1.sh` |

## 08.P1 — Bootstrap (placeholder)

**Contribution to final goal**: Establish the working baseline. Replace this phase with concrete decomposition via `touring decompose add` + re-run `taco-forge plan --quality high`.

**Impacts**:
- scope discovery
- baseline metrics captured

**Deliverables**:
- `plan/ — generated artifacts`
- `validators/ — per-phase validators`
- `checkpoints/ — TOON state snapshots`

**Validation gate** (deep mode): `cargo check`=False, `cargo test`=False, TDG ≥ `B`, orphan delta ≤ 0, E2E `quick` ≥ 0.5, TDD gates: red=False green=False refactor=False

## 09. Validation scripts (one per phase)

Each phase ships with a deep validation script. Run individually:

- `bash validators/validate-phase-P1.sh`

Run all in dependency order:

```bash
for f in validators/validate-phase-*.sh; do
  echo "=== $f ==="
  bash "$f" || { echo "FAIL at $f"; exit 1; }
done
```

## 10. Cross-audit (final gate)

After all phases complete, run the generated cross-audit script:

```bash
bash audit-plan-completion.sh
```

Audits performed:
- All phases finalized via `touring decompose status`
- VGP symbols still verified (re-run V1+V2 batch)
- Zero new orphan pub symbols vs DISCOVER baseline
- `cargo check --workspace` exit 0
- `cargo test --workspace` exit 0
- `touring e2e --depth standard` composite ≥ 0.7
- All `validate-phase-*.sh` exit 0
- Memory persists final outcome via `touring memory store`
- Diary entry via `touring diary write --aaak`

## 11. Reproduce / re-render

```bash
~/.claude/tools/taco-forge/workflows/plan.sh \
  --intent "Wave A — ZERO-CODE Reflexos Cardinais. Wire 9 missing Claude Code hook events in ~/.claude/settings.json to existing touring-cortex handlers: TeammateIdle (run_teammate_idle_gate at team_hooks.rs:52), ConfigChange (lifecycle.rs:1011 area), WorktreeCreate (lifecycle.rs:905), WorktreeRemove, StopFailure (StopFailureHandler at lifecycle.rs:542), Notification (lifecycle.rs:1149), Elicitation (lifecycle.rs:1255), ElicitationResult (lifecycle.rs:1315), PermissionDenied. ZERO LOC Rust necessary — handlers already exist in touring-cortex/src/handlers/lifecycle.rs. Validation via touring gate-metrics total_invocations delta after triggering each event." \
  --cila-level=3 \
  --quality high \
  --out plan/plan.md
```

---

_Code-First plan v1.14.0 — `task_1777835634668240326`. Modify the DAG, not this file._
