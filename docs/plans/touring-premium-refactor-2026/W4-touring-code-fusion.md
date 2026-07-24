---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W4"
name: "touring-code Fusion"
phase: "F2-FUSIONS"
depends_on:
  - W3
parallel_with: []
status: "DONE"
created: "2026-05-11"
completed: "2026-05-15"
cila: "L4"
rust_changes: "FUSION"
estimated_days: "12-15"
checkpoint: "touring_premium_W4_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W4.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W1-*.md
  - W2-*.md
  - W3-*.md
  - W5-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W4: touring-code Fusion

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F2-FUSIONS
> **Contribuição para resultado final**: Elimina duplicação intencional documentada (ast-polyglot extends ast). Reduz 4 crate-boundaries para 1, simplificando o grafo. Features modulares permitem usuário escolher engine de parsing (tree-sitter p/ polyglot, syn p/ Rust deep, ast-grep p/ structural rewrite).

---

## Contexto e Dependências

- **Depende de**: W3
- **Paralelo com**: Nenhuma
- **CILA**: `L4`
- **Mudanças Rust**: `FUSION`
- **Estimativa**: 12-15 dias
- **Checkpoint**: `touring_premium_W4_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W4.py`

---

## Descrição

Fundir 4 crates relacionados (touring-ast 23k, touring-ast-polyglot 769L, touring-language 558L, touring-semantics 1072L) num único touring-code (~26k LOC). Sub-modules: parsers/{tree_sitter,ast_grep,syn} + languages + semantics + graph + format + complexity + incremental. Features lang-* (7 idiomas) + parser-* (3 engines). Re-export shims preservam consumidores por 2 versões.

---

## Efeitos no Sistema

- touring-code crate criado (~26k LOC src, ~6k tests, ratio ≥ 23%)
- 4 crates deletados (ast, ast-polyglot, language, semantics)
- 38 consumidores atualizados: touring_ast::X → touring_code::ast::X
- Features lang-rust (default), lang-typescript, lang-python, lang-go, lang-ruby, lang-java, lang-cpp
- Features parser-tree-sitter (default), parser-ast-grep, parser-syn
- Bench parsing: regressão < 5% (gate)
- Re-export shim 'pub use touring_code::ast::* as touring_ast' por 2 versões

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W4.1: Create touring-code skeleton + Cargo.toml

**Descrição**: cargo new --lib crates/touring-code via taco-forge perfect-create-crate. Cargo.toml com features lang-* + parser-*. Adicionar a workspace members.

**Dias estimados**: 0.5

**DISCOVER obrigatório**:
  - taco-forge perfect-create-crate --name touring-code --intent '...'
  - touring memory recall 'create-crate workflow'

**Critério de validação**: cargo check -p touring-code exit 0; crate registrado em workspace.

---

### W4.2: Move touring-ast/src/* → touring-code/src/parsers/tree_sitter/ + ast deep

**Descrição**: Mover 23k LOC. Manter touring-ast namespace via pub mod ast. Refatorar imports internos (use crate::ast::X).

**Dias estimados**: 2.0

**DISCOVER obrigatório**:
  - touring ast workspace-info | jq '.packages[] | select(.name=="touring-ast")'
  - wc -l crates/touring-ast/src/**/*.rs

**Critério de validação**: cargo check -p touring-code exit 0; pub mod ast exposto.

**🛑 BLOCKING**: Esta subtarefa bloqueia as posteriores se falhar.

---

### W4.3: Move touring-ast-polyglot/* → touring-code/src/parsers/ast_grep/

**Descrição**: 769 LOC. Feature 'parser-ast-grep' opt-in. Wire em touring_code::polyglot module.

**Dias estimados**: 1.0

**Critério de validação**: cargo check -p touring-code --features parser-ast-grep exit 0.

---

### W4.4: Move touring-language/* → touring-code/src/languages/

**Descrição**: 558 LOC. Tier matrix + capability tables. Sem feature gate.

**Dias estimados**: 0.5

**Critério de validação**: touring_code::languages::Lang exposto.

---

### W4.5: Move touring-semantics/* → touring-code/src/semantics/

**Descrição**: 1072 LOC. Definition enum + source_to_def + multi_lang. Atualizar import use touring_ast::languages::Lang → use crate::languages::Lang.

**Dias estimados**: 0.5

**Critério de validação**: touring_code::semantics::Definition exposto.

---

### W4.6: Define features lang-* + parser-*

**Descrição**: [features] em Cargo.toml: lang-rust (default), lang-typescript, lang-python, lang-go, lang-ruby, lang-java, lang-cpp; parser-tree-sitter (default), parser-ast-grep, parser-syn; semantic-search, incremental-salsa.

**Dias estimados**: 0.5

**Critério de validação**: cargo check --no-default-features -p touring-code exit 0; cargo check --all-features -p touring-code exit 0.

---

### W4.7: Update 25 consumers: touring_ast::X → touring_code::ast::X

**Descrição**: Identificar 25 consumers via touring wiring impact. Atualizar imports. Re-export shim 'pub use touring_code::ast::* as touring_ast' em crate stub touring-ast por 2 versões.

**Dias estimados**: 3.0

**DISCOVER obrigatório**:
  - touring wiring impact 'touring_ast' --depth 2
  - grep -rln 'use touring_ast' crates/*/src/

**TDD RED** (escrever ANTES do código):
```python
def test_consumers_use_touring_code():
    """RED: grep 'use touring_ast' should drop to 0 (or shim-only)."""
```

**Critério de validação**: grep 'use touring_ast::' crates/ → apenas em touring-ast shim crate.

**🛑 BLOCKING**: Esta subtarefa bloqueia as posteriores se falhar.

---

### W4.8: Update 8 polyglot consumers

**Descrição**: touring_ast_polyglot::X → touring_code::polyglot::X em ~8 consumers.

**Dias estimados**: 1.0

**Critério de validação**: grep 'touring_ast_polyglot' crates/ → apenas em shim.

---

### W4.9: Update 3 language consumers

**Descrição**: touring_language::X → touring_code::languages::X.

**Dias estimados**: 0.5

**Critério de validação**: grep 'touring_language' crates/ → apenas em shim.

---

### W4.10: Update 2 semantics consumers

**Descrição**: touring_semantics::X → touring_code::semantics::X.

**Dias estimados**: 0.5

**Critério de validação**: grep 'touring_semantics' crates/ → apenas em shim.

---

### W4.11: Bench parsing — regression < 5%

**Descrição**: cargo bench --workspace --baseline pre-refactor-<DATE> | grep 'change' | grep -v 'within noise'. Comparar parsing benches: rust syn, ts/py tree-sitter, polyglot ast-grep.

**Dias estimados**: 1.0

**DISCOVER obrigatório**:
  - touring memory recall 'bench regression budget'

**TDD RED** (escrever ANTES do código):
```python
def test_parsing_bench_within_5pct_of_baseline():
    """RED: any bench > 5% slower than baseline FAILS."""
```

**Critério de validação**: Nenhum bench mais lento que -5%. Idealmente alguns ficam mais rápidos (cache compartilhado dentro do crate).

**🛑 BLOCKING**: Esta subtarefa bloqueia as posteriores se falhar.

---

### W4.12: Tests pass + cycle re-check

**Descrição**: cargo test --workspace exit 0. touring wiring cycles --min-depth 2: cycle count monotonicamente não-crescente.

**Dias estimados**: 1.0

**Critério de validação**: cargo test workspace exit 0; cycle count ≤ baseline W3.

---

### W4.13: Delete old crates (ast, ast-polyglot, language, semantics)

**Descrição**: Remover diretórios + workspace members. Manter shim crates touring-ast/etc com pub use re-exports.

**Dias estimados**: 0.5

**Critério de validação**: ls crates/touring-{ast,ast-polyglot,language,semantics}/src/ → apenas lib.rs com pub use re-exports.

---

### W4.14: Update workspace members

**Descrição**: Cargo.toml [workspace] members lista touring-code + shims.

**Dias estimados**: 0.2

**Critério de validação**: grep '"crates/touring-code"' Cargo.toml; cargo check exit 0.

---

## Gate de Saída

touring-code 26k LOC, 6+3 features funcionais, ≥ 23% test ratio, 0 cycle regression, < 5% perf regression, 38 consumers atualizados, shim crates por 2 versões.

## Riscos Específicos

- Consumer pode usar pub item interno de touring-ast não exposto em touring-code::ast → identificar via cargo check antes de delete
- Cargo features feature unification entre touring-code e consumers → testar todas combinações via cargo hack --feature-powerset
- Bench regression > 5% em parsing rust syn (single-thread bottleneck) → investigar e mitigar antes de gate

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
