---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W1"
name: "Dead Code Purge"
phase: "F1-PREP"
depends_on:
  - W0
parallel_with: []
status: "PENDING"
created: "2026-05-11"
cila: "L2"
rust_changes: "DELETION"
estimated_days: "3-4"
checkpoint: "touring_premium_W1_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W1.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W2-*.md
  - W3-*.md
  - W4-*.md
  - W5-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W1: Dead Code Purge

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F1-PREP
> **Contribuição para resultado final**: Sinaliza compromisso com hygiene desde o início. Reduz baseline para 42 crates antes de fusões maiores. Elimina 1 dos 2 ciclos.

---

## Contexto e Dependências

- **Depende de**: W0
- **Paralelo com**: Nenhuma
- **CILA**: `L2`
- **Mudanças Rust**: `DELETION`
- **Estimativa**: 3-4 dias
- **Checkpoint**: `touring_premium_W1_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W1.py`

---

## Descrição

Eliminar 4 crates mortos/órfãos identificados na auditoria: touring-semantic-spike (66L archived) e touring-wasm-{client,common,server} (0 LOC cada). Fix Cycle #1 (file_tools↔project_tools intra-server). Atualizar workspace members. Zero impacto em consumidores reais.

---

## Efeitos no Sistema

- −4 crates do workspace (semantic-spike + 3 wasm 0-LOC)
- −1 ciclo de dependência (depth 2, intra-server)
- Workspace Cargo.toml members atualizado
- Pub use de crates removidos limpos em toda tree

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W1.1: DELETE touring-semantic-spike

**Descrição**: Remover crates/touring-semantic-spike/ inteiro. 66 LOC archived per ARCHITECTURE.md; 0 pub symbols. Remover entrada de [workspace] members em Cargo.toml.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - touring index find 'touring_semantic_spike' (esperado 0 hits)
  - grep -rn 'touring-semantic-spike' Cargo.toml crates/*/Cargo.toml

**Critério de validação**: cargo check --workspace exit 0; nenhuma referência restante.

---

### W1.2: DELETE touring-wasm-{client,common,server}

**Descrição**: Remover 3 crates de 0 LOC: touring-wasm-client, touring-wasm-common, touring-wasm-server. Atualizar [workspace] members + remover dev-deps órfãos.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - wc -l crates/touring-wasm-{client,common,server}/src/*.rs
  - grep -rn 'touring-wasm-client\|touring-wasm-common\|touring-wasm-server' Cargo.toml crates/*/Cargo.toml

**Critério de validação**: cargo check --workspace exit 0; touring wiring orphans -j no new orphans.

---

### W1.3: Audit + clean dead reexports

**Descrição**: grep por pub use referenciando crates removidos. Limpar em touring-server/src/lib.rs, touring-hooks façade, etc. Atualizar tests que importavam.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - touring tantivy search 'pub use touring_semantic_spike'
  - touring tantivy search 'pub use touring_wasm_client'

**Critério de validação**: cargo check --workspace --all-targets exit 0; 0 unused warnings novos.

---

### W1.4: Fix Cycle #1 (file_tools ↔ project_tools intra-server)

**Descrição**: Cycle de depth 2 detectado em crates/touring-server/src/tools/file_tools.rs → project_tools.rs. Refatorar: extrair tipos comuns para tools/shared.rs OU inverter direção via trait.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - touring ast blast crates/touring-server/src/tools/file_tools.rs
  - touring ast blast crates/touring-server/src/tools/project_tools.rs
  - touring wiring impact file_tools::* --depth 2

**TDD RED** (escrever ANTES do código):
```python
def test_no_cycle_file_project_tools():
    """RED: cycle detector should report 0 cycles in tools/."""
```

**Critério de validação**: touring wiring cycles --min-depth 2: Cycle #1 GONE; cycle_count = 1 (only macrociclo of depth 618 remains).

**🛑 BLOCKING**: Esta subtarefa bloqueia as posteriores se falhar.

---

### W1.5: Update workspace + validate

**Descrição**: Atualizar [workspace] members em Cargo.toml. Remover 4 crates. Validar com cargo check --workspace + touring wiring orphans -j (deve estar estável).

**Dias estimados**: 0.5

**Critério de validação**: cargo check --workspace exit 0; cargo test --workspace --no-run exit 0; orphan delta ≤ 0.

---

## Gate de Saída

4 crates removidos; Cycle #1 eliminado; cargo check + test --no-run exit 0; orphans não aumentaram.

## Riscos Específicos

- Algum crate consumer-só-em-test poderia estar usando 0-LOC wasm crates como placeholder → revisar tests cuidadosamente

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
