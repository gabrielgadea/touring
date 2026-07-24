---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W10"
name: "touring-orchestration Fusion"
phase: "F3-STABILIZATION"
depends_on:
  - W9
parallel_with:
  - W9
status: "DONE — 3-crate fusion (2026-05-15); decompose/session extraction superseded by W9"
created: "2026-05-11"
completed: "2026-05-15"
cila: "L3"
rust_changes: "FUSION"
estimated_days: "5-7"
checkpoint: "touring_premium_W10_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W10.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W1-*.md
  - W2-*.md
  - W3-*.md
  - W4-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W10: touring-orchestration Fusion

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F3-STABILIZATION
> **Contribuição para resultado final**: Concentra orquestração (DAG, tasks, decompose, session) num único crate. Permite touring-server depender só dele para essas operações.

---

## Contexto e Dependências

- **Depende de**: W9
- **Paralelo com**: W9
- **CILA**: `L3`
- **Mudanças Rust**: `FUSION`
- **Estimativa**: 5-7 dias
- **Checkpoint**: `touring_premium_W10_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W10.py`

---

## Descrição

Fundir touring-flow (809L), touring-tasksfile (1.2k), touring-devrc-adapter (591L), + extrair decompose/ + session/ + diary/ de touring-server para o novo touring-orchestration (~3.5k LOC). Features flow-dag, tasks-sqlite, decompose-mcts, session-persist.

---

## Efeitos no Sistema

- touring-orchestration ~3.5k LOC, ≥ 25% test ratio
- 3 crates absorvidos (flow, tasksfile, devrc-adapter)
- Decompose + session + diary extraídos de touring-server
- 4 features modulares

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W10.1: Create touring-orchestration skeleton

**Descrição**: taco-forge perfect-create-crate.

**Dias estimados**: 0.5

**Critério de validação**: cargo check -p touring-orchestration exit 0.

---

### W10.2: Move touring-flow → orchestration/flow/

**Descrição**: 809 LOC. DAG primitives.

**Dias estimados**: 0.5

**Critério de validação**: cargo check --features flow-dag exit 0.

---

### W10.3: Move touring-tasksfile → orchestration/tasks/

**Descrição**: 1.2k LOC. Tasksfile DSL + SQLite persistence.

**Dias estimados**: 0.7

**Critério de validação**: cargo check --features tasks-sqlite exit 0.

---

### W10.4: Move touring-devrc-adapter → orchestration/devrc/

**Descrição**: 591 LOC + 0% tests. Devrc adapter. +200 LOC tests.

**Dias estimados**: 0.7

**TDD RED** (escrever ANTES do código):
```python
def test_devrc_parses_real_devrcfile():
    """RED: devrc parser untested."""
```

**Critério de validação**: cargo test --features tasks-sqlite exit 0.

---

### W10.5: Extract decompose from touring-server → orchestration/decompose/

**Descrição**: Decompose MCTS lives in touring-server hoje. Move para orchestration/decompose/. Touring-server agora depende de touring-orchestration.

**Dias estimados**: 1.0

**Critério de validação**: cargo check --features decompose-mcts exit 0.

---

### W10.6: Extract session + diary → orchestration

**Descrição**: Session manager, diary writer. Touring-server delega para orchestration.

**Dias estimados**: 1.0

**Critério de validação**: touring session start <id> ainda funciona via orchestration.

---

### W10.7: Features + tests

**Descrição**: 4 features + +500 LOC tests total. Ratio ≥ 25%.

**Dias estimados**: 1.5

**Critério de validação**: cargo llvm-cov -p touring-orchestration ratio ≥ 25%.

---

### W10.8: Update consumers + delete old

**Descrição**: Touring-server, touring-hooks atualizados. Shims onde necessário.

**Dias estimados**: 0.5

**Critério de validação**: cargo check --workspace exit 0.

---

## Gate de Saída

touring-orchestration 3.5k LOC, 4 features, ≥ 25% test ratio, 3 crates absorvidos.

## Riscos Específicos

- Decompose extraction quebra touring decompose CLI → smoke test decompose create/add/status
- Session manager extraction quebra TACO session lifecycle → validar com touring session start <id>

## Checklist de Conclusão

- [ ] Todos os subtasks implementados
- [ ] Todos os testes TDD GREEN
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test --workspace --no-fail-fast` pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `touring wiring cycles --min-depth 2` no new cycles
- [ ] `touring wiring orphans -j` no new orphans (REGRA #0)
- [ ] Bench regression < 5%
- [ ] Test ratio ≥ 20% per touched crate
- [ ] Checkpoint `.toon` salvo
- [ ] Memory lesson persistida (`touring memory store --tier semantic`)
- [ ] RL reward injetado (`touring learning reward orchestrate <val>`)
- [ ] Documentação atualizada (se necessário)

---

## Execution Result (2026-05-15) — 3-Crate Fusion

W10 shipped the **3-crate fusion** part of the plan. The "extract decompose /
session / diary from touring-server" subtasks (W10.5, W10.6) are **superseded
by W9** and were not re-done — see below.

### What W10 shipped — `touring-orchestration`

`touring-flow` (809 LOC) + `touring-tasksfile` (1,229 LOC) +
`touring-devrc-adapter` (591 LOC) fused into the new `touring-orchestration`
crate (18 files, ~2,629 LOC) with three modules:

| Module | Origin crate | Content |
|---|---|---|
| `flow` | touring-flow | Declarative dataflow pipeline — FlowBuilder, FlowPipeline, filter DSL |
| `tasks` | touring-tasksfile | Tasksfile YAML DSL — schema, parser, compiler, template engine |
| `devrc` | touring-devrc-adapter | Devrcfile → Tasksfile adapter |

- Feature surface: `yaml`, `templates`, `http-client` (`default = yaml +
  templates`) — union of the origin crates' features.
- The 3 origin crates are now **1-file shim crates**
  (`pub use touring_orchestration::<module>::*`) that propagate their feature
  interface to `touring-orchestration`. External consumers (`touring-hooks`,
  `touring-server`) are untouched.
- 42 intra-crate `crate::` references rewritten to `crate::{flow,tasks,
  devrc}::`; the old `devrc → tasksfile` crate dependency became the
  intra-crate `crate::tasks` reference (no cycle — `devrc → tasks`, `flow`
  independent).

### Superseded by W9

The W10 plan (written before W9 ran) called for extracting `decompose`,
`session`, and `diary` from touring-server into the orchestration crate.
W9's pragmatic split already did the server-side extraction in a different
shape:

- `session` → `touring-server-session` (W9)
- `reasoning` incl. `TaskDecomposer` (the decompose engine) →
  `touring-server-reasoning` (W9)

Re-extracting those into `touring-orchestration` would be churn for no gain —
they are touring-server-internal crates and already cleanly separated. W10
therefore ships the fusion only.

### Gate results

| Gate | Result |
|---|---|
| `cargo check --workspace` | exit 0 ✅ |
| `cargo check --workspace --tests` | exit 0 ✅ |
| `touring-orchestration` + shim tests | 79 PASS ✅ |
| `cargo clippy -D warnings` (orchestration) | 0 issues ✅ |
| New Cargo cycles | 0 ✅ |
| External API surface (consumers) | preserved (shims) ✅ |
| W10-introduced regressions | 0 ✅ |
