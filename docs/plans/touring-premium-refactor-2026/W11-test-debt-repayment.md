---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W11"
name: "Test Debt Repayment"
phase: "F4-QUALITY"
depends_on:
  - W6
  - W7
  - W8
  - W9
  - W10
parallel_with:
  - W12
status: "RE-SCOPED (2026-05-15) — plano premises stale pos-fusao; W11.1/W11.3 obsoletos (cov real 77-83%), W11.5 ja atingido (89 proptest); trabalho genuino = W11.6 fuzz + W11.4 advisory + W11.2 re-spec"
created: "2026-05-11"
rescoped: "2026-05-15"
cila: "L3"
rust_changes: "TESTS-ONLY"
estimated_days: "10-15 (original) -> 5-8 (re-scoped 2026-05-15)"
checkpoint: "touring_premium_W11_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W11.py"
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
# W11: Test Debt Repayment

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F4-QUALITY
> **Contribuição para resultado final**: Plano premium não tem espaço para test-debt. Esta wave fecha a brecha. Mutation kill rate ≥ 80% prova que tests não são meramente cosmeticos.

---

## Contexto e Dependências

- **Depende de**: W6, W7, W8, W9, W10
- **Paralelo com**: W12
- **CILA**: `L3`
- **Mudanças Rust**: `TESTS-ONLY`
- **Estimativa**: 10-15 dias
- **Checkpoint**: `touring_premium_W11_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W11.py`

---

## Descrição

Repagar test debt remanescente para garantir 20%+ ratio em TODOS os crates e mutation kill rate ≥ 80% workspace-wide. Inclui proptest para tipos chave (Identity, Plan, Definition) + fuzz targets para parsers e serializers. Wave 'invisible' mas crítica para premium quality gate.

---

## Efeitos no Sistema

- touring-intelligence (cortex herdado) 15% → 20%
- touring-bindings (web/python/desktop) 8% → 18%
- touring-foundation (sentinel/telemetry) 15% → 22%
- Mutation kill rate workspace ≥ 80%
- 50 proptest properties (Identity, Plan, Definition)
- 8 fuzz targets (parsers, serializers)
- NENHUM crate < 20% test ratio

---

## Discovery Updates (2026-05-15) — Ground-Truth Re-measurement

> **Origem**: 5ª invocação do `/goal`. Antes de escrever qualquer teste, Gabriel
> autorizou **re-medir e re-escopar W11**. As premissas de "test ratio" do plano
> foram escritas em 11/05, **antes** das fusões W4-W10. Mesmo padrão de premissa
> stale já registrado para W6.0 (premissa "0.56% test ratio" provada errada).

### Método

`cargo llvm-cov -p <crate> --json --summary-only`, agregando **apenas** os
arquivos próprios de cada crate (path contém `/crates/<crate>/`). O relatório
`-p` cru contamina o `TOTAL` com arquivos de crates-dependência
(`touring-storage`, `inferlets`) a 0% — esses foram excluídos.

### Cobertura real vs. premissa do plano

| Crate | Premissa plano | Cobertura REAL (arquivos próprios) | Veredicto |
|---|---|---|---|
| touring-intelligence | "15% → 20%" | **83,14%** linhas (29.338/35.287, 139 arquivos) | Premissa STALE — 4× o alvo |
| touring-foundation | "15% → 22%" | **77,73%** linhas (7.399/9.519, 69 arquivos) | Premissa STALE — 3,5× o alvo |
| touring-bindings | "8% → 18%" | não-mensurável como descrito (`default = []`) | Premissa precisa RE-SPEC |

`touring-bindings` é a crate-fusão da W7: `default = []`, com 6 bind-modules
opt-in (`bind-python`, `bind-wasm`, `bind-capnp`, `bind-web`, `bind-desktop`,
`bind-postgis`). Já existem **185 funções de teste** (14,5k LOC) mas só executam
sob a `bind-*` feature correspondente. `cargo llvm-cov` com features default só
compila `lib.rs` (3 linhas, 100%) — daí o "8%" enganoso.

### Baseline dos demais sub-alvos

| Sub-alvo | Premissa plano | Realidade medida |
|---|---|---|
| W11.5 proptest | "≥ 50 properties" | **89** property fns workspace-wide (19 blocos `proptest!`) — **JÁ ATINGIDO** |
| W11.6 fuzz | "8 fuzz targets" | **0** diretórios de fuzz — **gap real** |
| W11.4 mutation | "≥ 80% workspace" | `cargo-mutants` instalado; `w11_mutation_kill_rate_audit.py` pronto; run full-workspace = horas/dias |

### Re-escopo dos subtasks

| Subtask | Original | Veredicto 2026-05-15 | Ação |
|---|---|---|---|
| W11.1 intelligence 15→20% | 3,0d | **OBSOLETO** — real 83,14% | Drop. Opcional: 3 arquivos 0-cov (`ann/validation_status.rs`, `reasoning/aco_traits.rs`, `rl/data/telemetry.rs`) |
| W11.2 bindings 8→18% | 3,0d | **RE-SPEC** — crate feature-gated | Novo escopo: coverage por-feature `bind-*`; foco em `web/` (5.245 LOC / 25 tests) |
| W11.3 foundation 15→22% | 2,0d | **OBSOLETO** — real 77,73% | Drop. Opcional: 10 arquivos 0-cov (`failover/mod.rs`, `feedback.rs`, `hash.rs`, `mvkl/layer0-2`, `plugin/registry.rs`, `plugin/trait.rs`, +2) |
| W11.4 mutation ≥80% | 3,0d | **VÁLIDO — ADVISORY** | cargo-mutants baseline incremental; 70% mid-target conforme risco do plano |
| W11.5 proptest ≥50 | 1,5d | **JÁ ATINGIDO** — 89 > 50 | Verificar cobertura Identity/Plan/Definition; sem novo código obrigatório |
| W11.6 fuzz 8 targets | 2,5d | **VÁLIDO — gap real** | Único deliverable de código genuíno: 8 cargo-fuzz targets |

### Estimativa revisada

Original **10-15 dias** → re-escopado **5-8 dias**. W11.1/W11.3 (5d combinados)
eliminados; W11.5 (1,5d) já satisfeito. Trabalho genuíno restante: W11.6 fuzz
(~2,5d) + W11.4 mutation advisory (~1-2d) + W11.2 re-spec bindings (~2-3d) +
limpeza opcional de 13 arquivos 0-cov (~1d).

### Gate de Saída revisado

O gate original ("NENHUM crate < 20% test ratio") já está **satisfeito** para os
3 crates medidos (intelligence 83%, foundation 78%, bindings com 185 tests
feature-gated). Gate de saída W11 revisado:

1. **W11.6** — 8 fuzz targets criados + smoke 100 iterações verde
2. **W11.4** — cargo-mutants baseline advisory persistido em `.touring-cache/mutation-test/`
3. **W11.2** — cada `bind-*` feature com os testes existentes verdes sob `--features bind-<x>`
4. **0-cov cleanup** — 13 arquivos 0-cov testados OU justificados (REGRA #0)

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **Re-escopo 2026-05-15** — ver `## Discovery Updates` acima. Os subtasks
> abaixo são o texto **original** do plano; W11.1/W11.3 estão OBSOLETOS e W11.5
> JÁ ATINGIDO. A tabela de re-escopo é autoritativa.

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W11.1: touring-intelligence test ratio 15% → 20%

**Descrição**: Cortex pipeline + fusion + scoring + cross_audit precisam de mais cobertura. Foco em paths de fusão (handler dispatch + signal_fusion).

**Dias estimados**: 3.0

**TDD RED** (escrever ANTES do código):
```python
def test_signal_fusion_combines_3_layers():
    """RED: signal_fusion 3-layer combine untested."""
```

**Critério de validação**: cargo llvm-cov -p touring-intelligence ratio ≥ 20%.

---

### W11.2: touring-bindings test ratio 8% → 18%

**Descrição**: Web + python + desktop + postgis cobrem APIs externas.

**Dias estimados**: 3.0

**Critério de validação**: cargo llvm-cov -p touring-bindings ratio ≥ 18%.

---

### W11.3: touring-foundation test ratio 15% → 22%

**Descrição**: Sentinel (PSI) + telemetry (OTel) + plugin registry.

**Dias estimados**: 2.0

**Critério de validação**: cargo llvm-cov -p touring-foundation ratio ≥ 22%.

---

### W11.4: Mutation kill rate workspace ≥ 80%

**Descrição**: cargo mutants --workspace --threshold 0.80. Identificar mutations que sobrevivem; add tests focados.

**Dias estimados**: 3.0

**DISCOVER obrigatório**:
  - cargo install --locked cargo-mutants
  - touring memory recall 'cargo mutants kill rate'

**TDD RED** (escrever ANTES do código):
```python
def test_mutation_kill_rate_80pct():
    """RED: mutants kill rate < 80%."""
```

**Critério de validação**: cargo mutants exit 0; kill_rate ≥ 80%.

---

### W11.5: Proptest properties (Identity, Plan, Definition)

**Descrição**: 50 properties total: EntityId determinism (~10), Plan typestate transitions (~15), Definition resolution (~10), wire format roundtrip (~10), wiring graph invariants (~5).

**Dias estimados**: 1.5

**Critério de validação**: cargo test proptest:: exit 0; ≥ 50 properties.

---

### W11.6: Fuzz targets (parsers, serializers)

**Descrição**: 8 cargo-fuzz targets: rust syn parser, tree-sitter rust/py/ts/go, ast-grep pattern matcher, rkyv wire deserializer, tantivy query parser, JWT license verifier.

**Dias estimados**: 2.5

**DISCOVER obrigatório**:
  - cargo install --locked cargo-fuzz

**Critério de validação**: cargo fuzz list ≥ 8 targets; 100 iterations smoke pass.

---

## Gate de Saída

NENHUM crate < 20% test ratio; mutation kill rate workspace ≥ 80%; ≥ 50 proptest properties; ≥ 8 fuzz targets em CI.

## Riscos Específicos

- Mutation kill rate 80% pode levar muito tempo se tests são majoritariamente integration → aceitar 70% como mid-target
- Fuzz targets precisam corpus inicial → coletar de regression suite

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

## Execution Result — W11.6 Fuzz Targets (2026-05-15)

W11.6 (o único deliverable de código genuíno da W11 re-escopada) foi entregue:
uma crate `cargo-fuzz` na raiz do workspace com 8 fuzz targets.

### Entregue

| Item | Estado |
|---|---|
| Crate `fuzz/` (raiz do workspace, `exclude = ["fuzz"]` no Cargo.toml) | ✅ criada |
| 8 fuzz targets | ✅ `cargo +nightly fuzz build` exit 0; `cargo fuzz list` = 8 |
| `cargo check --workspace` | ✅ exit 0 (fuzz excluído, intacto) |

Os 8 targets: `fuzz_rust_syn`, `fuzz_rust_public_api` (parsing syn em
touring-code); `fuzz_polyglot_search_{rust,python,typescript,go}` +
`fuzz_polyglot_rewrite` (tree-sitter + ast-grep via touring-ast-polyglot);
`fuzz_rkyv_deserialize` (`touring_rkyv::check_archived_root::<IpcRequest>`).

JWT license verifier + tantivy query parser (da lista original do plano) **não
foram criados** — verificação de licença JWT ainda não existe (território W14)
e o query parser tantivy precisa de fixture de `Index`. Os 8 targets entregues
são todos VGP-verificados contra APIs públicas reais.

### O fuzzer funcionou — 5 bugs revelados

O propósito de fuzzing é achar bugs. Achou 5.

**CORRIGIDO nesta wave:**
- `touring-code::polyglot` — `search()` (search.rs) e `rewrite()` (rewrite.rs)
  construíam o matcher ast-grep com o construtor infalível `Pattern::new`, que
  faz `.unwrap()` interno → **panic em padrão malformado**. Ambas já retornam
  `Result`. Fix: `Pattern::new` → `Pattern::try_new` + `.map_err(Error::InvalidPattern)?`.
  2 testes de regressão adicionados (`search_rejects_malformed_pattern_without_panic`,
  `rewrite_rejects_malformed_pattern_without_panic`). `cargo test -p touring-code
  --lib polyglot` → 17 passed.

**ENCONTRADOS — bugs pré-existentes, deferidos (NÃO são regressões):**
- **B-FUZZ-001** (3 targets: search rust/python/typescript) — panic
  `"Ellipsis should be matched in parent level"` em `ast-grep-core
  match_tree/mod.rs:82`, alcançado de `polyglot/search.rs` `node.matches(&pat)`.
  Trigger: padrão ellipsis `$$$` solto. O panic é no **estágio de match**, após
  `Pattern::try_new` ter sucesso — o fix W11.6 acima NÃO cobre isto. Inputs
  minimizados de crash preservados em `fuzz/artifacts/`.
- **B-FUZZ-002** (1 target: search go) — panic `"LanguageError { version: 15 }"`
  em `ast-grep-core node.rs:73` — grammar tree-sitter-go ABI v15 incompatível
  com o `ast-grep-core =0.36.0` pinado.

B-FUZZ-001 e B-FUZZ-002 apontam para 1 follow-up: **avaliar upgrade do
`ast-grep-core`** do `=0.36.0` pinado (Cargo.toml:399, workspace dep) para
`0.38.7` ou `0.42.1` (ambos já no cache do registry). ast-grep-core mais novo
provavelmente traz suporte a ABI tree-sitter mais nova (B-FUZZ-002) e pode
corrigir o panic de match ellipsis (B-FUZZ-001). É um bump de dependência
workspace-wide → tarefa própria escopada, não um sub-fix da W11.6.

⚠️ **Correção de severidade B-FUZZ-001 — ver subseção "Atualização pós-guard"
no fim deste documento.** A primeira avaliação ("crash de produção real")
estava ERRADA: `match_tree/mod.rs:82` é um `debug_assert!`, não `panic!` — em
release (`debug-assertions = false`) é compilado fora; só dispara em debug/fuzz
builds. O bug de produção real é B-FUZZ-002 (Go).

### Resultados do smoke (≥4000 runs, sem workaround)

| Target | Smoke |
|---|---|
| fuzz_rust_syn | ✅ DONE limpo |
| fuzz_rust_public_api | ✅ DONE limpo |
| fuzz_rkyv_deserialize | ✅ DONE limpo |
| fuzz_polyglot_rewrite | ✅ DONE limpo |
| fuzz_polyglot_search_rust | ⚠️ crash → B-FUZZ-001 |
| fuzz_polyglot_search_python | ⚠️ crash → B-FUZZ-001 |
| fuzz_polyglot_search_typescript | ⚠️ crash → B-FUZZ-001 |
| fuzz_polyglot_search_go | ⚠️ crash → B-FUZZ-002 |

Os 4 artefatos de crash ficam em `fuzz/artifacts/` como seeds de regressão para
quando B-FUZZ-001/002 forem corrigidos.

### W11 restante (próxima sessão)

W11.4 (baseline advisory cargo-mutants) + W11.2 (coverage por-feature `bind-*` de
touring-bindings) + a avaliação do upgrade `ast-grep-core`. A infraestrutura de
fuzzing W11.6 está pronta e CI-ready (build-verificada; gate em CI com
`-max_total_time`).

---

## W11.6 — Atualização pós-guard (2026-05-15)

Após o W11.6 inicial, B-FUZZ-001 foi investigado a fundo — com **correção de
severidade**.

**Guard de input** — `is_degenerate_ellipsis_pattern` (`polyglot/search.rs`,
`pub(super)`) rejeita padrões de ellipsis degenerada com `Err(Error::InvalidPattern)`
antes do matcher. Aplicado em `search()` e `rewrite()`. +8 testes → **25 testes
polyglot**, todos verdes. `ast-grep-core` mantido em `=0.36.0` (o upgrade não foi
feito — quebras de API; o guard é mais cirúrgico e single-crate).

**Correção — B-FUZZ-001 NÃO é crash de produção.** `match_tree/mod.rs:82` é
`debug_assert!(false, "Ellipsis should be matched in parent level")` — **não**
`panic!`. O `[profile.release]` do touring (Cargo.toml:534) tem
`debug-assertions = false` (default) → o `debug_assert!` é **compilado fora**; em
produção o ramo `MV::Multiple` apenas retorna `Some(())`. B-FUZZ-001 só dispara
sob `debug-assertions = true` (builds debug + o build do cargo-fuzz). A primeira
avaliação ("crash de produção real") estava **errada** e fica retificada aqui.

**Re-validação fuzz (6000 runs/target):**

| Target | Resultado |
|---|---|
| rust_syn · rust_public_api · rkyv_deserialize · polyglot_rewrite | ✅ limpo |
| polyglot_search_rust · polyglot_search_typescript | ✅ limpo (guard) |
| polyglot_search_python | ⚠️ ainda dispara o `debug_assert!` — input `] \x00~]µµµ` (zero `$`): ast-grep cria o nó ellipsis no parse interno; guard textual não alcança. Debug-only. |
| polyglot_search_go | ⚠️ B-FUZZ-002 — `.expect("should parse")` em `node.rs:73` (Go grammar ABI v15) — **crash real** (release inclusive) |

**6/8 limpo.** O guard textual é incompleto por natureza para B-FUZZ-001 (o nó
ellipsis nasce do parse interno do ast-grep, não do texto `$$$`). **B-FUZZ-002 é
o único bug de produção real** — `Lang::Go` em `polyglot search`/`rewrite` →
`AstGrep::new` → `.expect("should parse")` aborta. Fix completo de ambos = upgrade
`ast-grep-core 0.36→0.42` (follow-up escopado próprio — o engineer adaptou o path
de `StrDoc` mas o bump completo cascateia em quebras de API).

**Circuit breaker** — W11.6 teve 5 rodadas de iteração fuzz. A infraestrutura
está entregue e provou seu valor (achou 3 bugs reais). O upgrade `ast-grep-core`
não foi perseguido em contexto degradado.
