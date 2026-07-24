---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W2"
name: "Tooling Foundation"
phase: "F1-PREP"
depends_on:
  - W0
  - W1
parallel_with: []
status: "PENDING"
created: "2026-05-11"
cila: "L2"
rust_changes: "REFACTOR"
estimated_days: "4-5"
checkpoint: "touring_premium_W2_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W2.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W1-*.md
  - W3-*.md
  - W4-*.md
  - W5-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W2: Tooling Foundation

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F1-PREP
> **Contribuição para resultado final**: Sem isso, cada crate vira ilha de configuração. Atualizar versão de uma dep externa exige tocar 42 Cargo.toml. Esta wave estabelece single source of truth.

---

## Contexto e Dependências

- **Depende de**: W0, W1
- **Paralelo com**: Nenhuma
- **CILA**: `L2`
- **Mudanças Rust**: `REFACTOR`
- **Estimativa**: 4-5 dias
- **Checkpoint**: `touring_premium_W2_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W2.py`

---

## Descrição

Centralizar dependências externas em [workspace.dependencies], metadados em [workspace.package], lints em [workspace.lints]. Configurar cargo-deny, cargo-machete, cargo-mutants. CI gates para todos. Preparar terreno para todas as waves seguintes.

---

## Efeitos no Sistema

- [workspace.dependencies] com ~60 deps centralizadas
- [workspace.package] com license/edition/MSRV 1.83 partilhados
- [workspace.lints] strict (deny warnings + pedantic + nursery)
- cargo-deny config (bans, advisories, sources, licenses)
- cargo-machete CI gate (0 unused deps)
- cargo-mutants per-crate threshold (initial 50%, target 80% em W11)
- GitHub Actions workflow para deny+machete+mutants+msrv

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W2.1: Centralize external deps in [workspace.dependencies]

**Descrição**: Listar todas deps externas únicas (~60 nomes). Adicionar a [workspace.dependencies] no Cargo.toml raiz com versão fixa. Cada crate passará a usar .workspace = true.

**Dias estimados**: 1.5

**DISCOVER obrigatório**:
  - cargo metadata --format-version 1 | jq '.packages[].dependencies[].name' | sort -u
  - touring memory recall 'workspace.dependencies'

**Critério de validação**: [workspace.dependencies] tem ≥ 60 entradas; nenhuma versão duplicada.

---

### W2.2: Centralize [workspace.package] metadata

**Descrição**: license = 'MIT OR Apache-2.0', edition = '2021', rust-version = '1.83', authors, version. Permite per-crate herdar via license.workspace = true.

**Dias estimados**: 0.5

**Critério de validação**: [workspace.package] presente com 5+ campos.

---

### W2.3: Update 42 Cargo.toml: <dep>.workspace = true

**Descrição**: Para cada crate, substituir 'serde = "1"' por 'serde.workspace = true'. Mesmo para todas as deps comuns. Manter overrides locais quando necessário.

**Dias estimados**: 1.5

**TDD RED** (escrever ANTES do código):
```python
def test_no_inline_dep_versions():
    """RED: nenhum crate deve ter version literal em deps comuns."""
```

**Critério de validação**: grep -rn 'serde = "' crates/*/Cargo.toml retorna 0 hits para deps já em workspace.dependencies.

---

### W2.4: [workspace.lints] strict

**Descrição**: Deny warnings + clippy::pedantic + clippy::nursery + rustdoc::broken_intra_doc_links. Per-crate override apenas com justificativa documentada.

**Dias estimados**: 0.5

**Critério de validação**: cargo clippy --workspace -- -D warnings exit 0.

---

### W2.5: cargo-deny config

**Descrição**: deny.toml com [bans], [advisories], [sources], [licenses] strict. Only allow MIT, Apache-2.0, BSD, MPL. Block GPL, AGPL contagious.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - cargo install --locked cargo-deny
  - touring memory recall 'cargo-deny licenses'

**Critério de validação**: cargo deny check exit 0.

---

### W2.6: cargo-machete (0 unused deps)

**Descrição**: Auditar e remover deps declaradas mas não usadas. Adicionar machete.toml com ignore-list para deps feature-gated não detectadas automaticamente.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - cargo install --locked cargo-machete

**Critério de validação**: cargo machete exit 0 OR justified ignore-list.

---

### W2.7: cargo-mutants per-crate config

**Descrição**: [workspace.metadata.mutants] threshold inicial 50%. Por-crate override para crates com fixture-heavy tests. Não bloqueia em W2 — apenas baseline.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - cargo install --locked cargo-mutants

**Critério de validação**: cargo mutants --baseline workspace exit 0 (não enforça threshold).

---

### W2.8: CI workflow: deny + machete + mutants + msrv

**Descrição**: .github/workflows/quality.yml: 4 jobs em matriz. cargo-msrv para verificar 1.83 não regride. cargo-deny + machete bloqueiam PR. cargo-mutants warn-only.

**Dias estimados**: 1.0

**Critério de validação**: Push para branch + observe workflow green.

---

## Gate de Saída

[workspace.dependencies] populated; 42 Cargo.toml usam .workspace=true; cargo-deny + machete clean; CI workflow ativo.

## Riscos Específicos

- Dep com features divergentes entre crates → manter inline com override
- cargo-deny pode bloquear deps pre-existentes com license unusual → documentar exceções em deny.toml [licenses] allowlist

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
