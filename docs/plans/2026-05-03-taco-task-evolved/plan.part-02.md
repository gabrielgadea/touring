# Potencialização Touring (TACO-task 2026-05-03) — Part 2 of 3

> **Nav**: [← Part 1](plan.part-01.md) | [↑ Index](plan.md) | [Part 3 →](plan.part-03.md)

---

## 07. Phases overview

| ID | Title | Deps | Status | Effort | Validator |
|----|-------|------|--------|--------|----------|
| `W1` | quick win 7 Cargo.toml edits | — | `pending` | 1.0h | `validate-phase-W1.sh` |
| `W2` | foundation comrak+TouringError dedup+queries.scm | `W1` | `pending` | 20.0h | `validate-phase-W2.sh` |
| `W3` | MarkdownDocument+CLI+anyhow elimination | `W2` | `pending` | 28.0h | `validate-phase-W3.sh` |
| `W4` | miette expansion+snapshots | `W3` | `pending` | 16.0h | `validate-phase-W4.sh` |
| `W5` | prelude+plan-detector upgrade | `W4` | `pending` | 8.0h | `validate-phase-W5.sh` |


## 08.W1 — quick win 7 Cargo.toml edits

> **Phase ID**: `W1` | **Effort**: 1.0h | **CILA**: L3 | **Status**: `pending` | **Checks**: 8 | **Validator**: `validate-phase-W1.sh`

**Contribution to final goal**: quick win 7 Cargo.toml edits

**Deliverables**:
- `7 Cargo.toml edits`

**Cross-references**: `W2`, `W3`, `W4`, `W5`

**Definition of Done** (acceptance checklist):

- [ ] All 1 declared deliverables landed and validated
- [ ] `validate-phase-W1.sh` exits 0
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test` for the affected package(s) passes
- [ ] `touring wiring orphans` shows zero new orphans (REGRA #0)
- [ ] `touring ast tdg <changed-files>` grade ≥ B
- [ ] `touring memory store --tier semantic plan:<task>:phase:W1:outcome` persisted
- [ ] `touring learning reward orchestrate 1.0 phase_W1_validated` injected

**Validation gate** (deep mode): `cargo check`=True, `cargo test`=True, TDG ≥ `B`, orphan delta ≤ 0, E2E `standard` ≥ 0.7, TDD gates: red=True green=True refactor=True


## 08.W2 — foundation comrak+TouringError dedup+queries.scm

> **Phase ID**: `W2` | **Effort**: 20.0h | **CILA**: L3 | **Status**: `pending` | **Checks**: 8 | **Validator**: `validate-phase-W2.sh`

**Contribution to final goal**: foundation comrak+TouringError dedup+queries.scm

**Deliverables**:
- `comrak`
- `TouringError dedup`
- `queries.scm`

**Dependencies**: `W1`

**Cross-references**: `W1`, `W3`, `W4`, `W5`

**Definition of Done** (acceptance checklist):

- [ ] All 3 declared deliverables landed and validated
- [ ] `validate-phase-W2.sh` exits 0
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test` for the affected package(s) passes
- [ ] `touring wiring orphans` shows zero new orphans (REGRA #0)
- [ ] `touring ast tdg <changed-files>` grade ≥ B
- [ ] `touring memory store --tier semantic plan:<task>:phase:W2:outcome` persisted
- [ ] `touring learning reward orchestrate 1.0 phase_W2_validated` injected

**Validation gate** (deep mode): `cargo check`=True, `cargo test`=True, TDG ≥ `B`, orphan delta ≤ 0, E2E `standard` ≥ 0.7, TDD gates: red=True green=True refactor=True


## 08.W3 — MarkdownDocument+CLI+anyhow elimination

> **Phase ID**: `W3` | **Effort**: 28.0h | **CILA**: L3 | **Status**: `pending` | **Checks**: 8 | **Validator**: `validate-phase-W3.sh`

**Contribution to final goal**: MarkdownDocument+CLI+anyhow elimination

**Deliverables**:
- `MarkdownDocument`
- `CLI`
- `anyhow elimination`

**Dependencies**: `W2`

**Cross-references**: `W1`, `W2`, `W4`, `W5`

**Definition of Done** (acceptance checklist):

- [ ] All 3 declared deliverables landed and validated
- [ ] `validate-phase-W3.sh` exits 0
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test` for the affected package(s) passes
- [ ] `touring wiring orphans` shows zero new orphans (REGRA #0)
- [ ] `touring ast tdg <changed-files>` grade ≥ B
- [ ] `touring memory store --tier semantic plan:<task>:phase:W3:outcome` persisted
- [ ] `touring learning reward orchestrate 1.0 phase_W3_validated` injected

**Validation gate** (deep mode): `cargo check`=True, `cargo test`=True, TDG ≥ `B`, orphan delta ≤ 0, E2E `standard` ≥ 0.7, TDD gates: red=True green=True refactor=True


## 08.W4 — miette expansion+snapshots

> **Phase ID**: `W4` | **Effort**: 16.0h | **CILA**: L3 | **Status**: `pending` | **Checks**: 8 | **Validator**: `validate-phase-W4.sh`

**Contribution to final goal**: miette expansion+snapshots

**Deliverables**:
- `miette expansion`
- `snapshots`

**Dependencies**: `W3`

**Cross-references**: `W1`, `W2`, `W3`, `W5`

**Definition of Done** (acceptance checklist):

- [ ] All 2 declared deliverables landed and validated
- [ ] `validate-phase-W4.sh` exits 0
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test` for the affected package(s) passes
- [ ] `touring wiring orphans` shows zero new orphans (REGRA #0)
- [ ] `touring ast tdg <changed-files>` grade ≥ B
- [ ] `touring memory store --tier semantic plan:<task>:phase:W4:outcome` persisted
- [ ] `touring learning reward orchestrate 1.0 phase_W4_validated` injected

**Validation gate** (deep mode): `cargo check`=True, `cargo test`=True, TDG ≥ `B`, orphan delta ≤ 0, E2E `standard` ≥ 0.7, TDD gates: red=True green=True refactor=True


## 08.W5 — prelude+plan-detector upgrade

> **Phase ID**: `W5` | **Effort**: 8.0h | **CILA**: L3 | **Status**: `pending` | **Checks**: 8 | **Validator**: `validate-phase-W5.sh`

**Contribution to final goal**: prelude+plan-detector upgrade

**Deliverables**:
- `prelude`
- `plan-detector upgrade`

**Dependencies**: `W4`

**Cross-references**: `W1`, `W2`, `W3`, `W4`

**Definition of Done** (acceptance checklist):

- [ ] All 2 declared deliverables landed and validated
- [ ] `validate-phase-W5.sh` exits 0
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test` for the affected package(s) passes
- [ ] `touring wiring orphans` shows zero new orphans (REGRA #0)
- [ ] `touring ast tdg <changed-files>` grade ≥ B
- [ ] `touring memory store --tier semantic plan:<task>:phase:W5:outcome` persisted
- [ ] `touring learning reward orchestrate 1.0 phase_W5_validated` injected

**Validation gate** (deep mode): `cargo check`=True, `cargo test`=True, TDG ≥ `B`, orphan delta ≤ 0, E2E `standard` ≥ 0.7, TDD gates: red=True green=True refactor=True


## 09. Validation scripts (one per phase)

Each phase ships with a deep validation script. Run individually:

- `bash validators/validate-phase-W1.sh`
- `bash validators/validate-phase-W2.sh`
- `bash validators/validate-phase-W3.sh`
- `bash validators/validate-phase-W4.sh`
- `bash validators/validate-phase-W5.sh`

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


---

> **Nav**: [← Part 1](plan.part-01.md) | [↑ Index](plan.md) | [Part 3 →](plan.part-03.md)
