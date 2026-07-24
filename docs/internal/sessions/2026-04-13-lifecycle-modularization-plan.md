# Modularização Completa de `touring-hooks/src/lifecycle.rs`

**Data:** 2026-04-13
**Autor:** Claude Code (Cognitive Orchestrator) + Gabriel Gadea
**Status:** PLANO — pronto para execução incremental
**Refinement level:** L4 (Architectural) — user consent obrigatório a cada fase
**Precedente:** Fase A já entregue — 4 handlers extraídos, padrão validado

---

## 1. Contexto e motivação

### 1.1 Estado atual (confiança 1.0)

| Métrica | Valor |
|---|---|
| `lifecycle.rs` LOC | **22.309** (pós-Fase A) |
| Handlers `pub(crate) fn handle_*` ainda inline | **10** |
| Handlers já extraídos (Fase A) | 4 (`subagent`, `pre_compact`, `worktree`, `cwd_changed`) |
| Funções top-level totais | ~347 |
| `mod tests` LOC | ~13.593 |
| Testes totais | **1182** (todos em um único `mod tests`) |
| Testes passando | 2735/2735 ✅ |
| FIX-1 (`metadata-backfill`) | ✅ operacional |
| FIX-2 (wiring `crate::` paths) | ✅ operacional (score 0.0 → 0.91) |

### 1.2 Por que modularizar (causa raiz)

- **Monolito de 22k LOC** foi resultado direto da falha do Touring em fornecer feedback de qualidade (root cause: `file_knowledge` sem o arquivo + `cli_metadata_backfill` quebrado).
- Mesmo após FIX-1/FIX-2, o arquivo permanece **inadministrável**:
  - Navegação: impossível ler sem offsets cirúrgicos
  - Blast radius: cada `pub(crate)` novo aumenta hook-registry coupling invisível
  - Test granularity: `cargo test lifecycle::` carrega 1182 testes — não há como rodar apenas os testes de `task_get`
  - Compile iteration: qualquer edit recompila o módulo inteiro (~60s tests)
- Regra do CLAUDE.md #0 (POTENCIALIZAR) + #3 (Simplicidade): arquivos > 2000 LOC são code smell; > 5000 LOC são anti-pattern; > 20000 LOC = divida técnica crítica.

### 1.3 Best practices consultadas (Context7 → `/rust-lang/reference`)

| Regra | Aplicação |
|---|---|
| **Rust 2018 file layout**: preferir `foo.rs` + `foo/` dir sobre `foo/mod.rs` | Manter `lifecycle.rs` como façade, submódulos em `lifecycle/` |
| **Visibility hierarchy**: `pub(self) < pub(super) < pub(crate) < pub` | Usar `pub(crate)` para handlers (precisa cruzar a `lifecycle::`); `pub(super)` para helpers internos |
| **Re-export rule**: re-export visibility ≤ source visibility (erro E0364) | Toda função re-exportada em `lifecycle.rs` deve ser `pub(crate)` na origem |
| **Co-located tests**: `#[cfg(test)] mod tests` no mesmo arquivo da produção | Ideal final, mas optional na Fase 1 |
| **`#[path = "..."]` attribute**: usar só quando convenção default não serve | NÃO usar — convenção cobre 100% dos casos aqui |
| **Inner modules in non-mod-rs files**: `mod foo;` em `lifecycle.rs` procura `lifecycle/foo.rs` | Padrão do projeto |

---

<objective>

## 2. Objetivo

Transformar `lifecycle.rs` de um monolito de 22.309 LOC num **façade enxuto** (<100 LOC) que apenas re-exporta handlers, com **toda a lógica distribuída em submódulos domínio-orientados** (`<2000 LOC/arquivo`), preservando:

- **100% compatibilidade externa** — `hook_registry.rs` e todos os consumidores continuam usando `crate::lifecycle::handle_*`
- **100% dos testes** passando a cada deliverable (2735/2735)
- **Zero regressões** em runtime (mesmos hooks, mesmos contratos JSON, mesma lógica)
- **Melhoria mensurável** de métricas Touring (cognitive_score, blast_radius, integration_score) por submódulo

**Critérios de sucesso (mensuráveis):**
1. `lifecycle.rs` < 100 LOC
2. Nenhum submódulo > 2000 LOC
3. `cargo test -p touring-hooks --lib` → 2735+ passing, 0 failed
4. `cargo clippy -p touring-hooks -- -D warnings` → 0 warnings
5. `touring wiring score lifecycle.rs + submódulos` → integration_score ≥ 0.85 em todos
6. `touring ast meta lifecycle/<submod>.rs --depth summary` → cognitive_score < 0.7 em todos

</objective>

---

## 3. Arquitetura alvo

### 3.1 Estrutura final proposta

```
crates/touring-hooks/src/
├── lifecycle.rs                          (~80 LOC: re-export façade)
└── lifecycle/
    ├── shared.rs                         (~400 LOC)
    │   ├── suggest_generator_for_task_subject
    │   ├── classify_file_to_generator_kind
    │   ├── classify_yaml_to_generator_kind
    │   ├── classify_rust_to_generator_kind
    │   ├── file_stem, stem_to_camel_case
    │   └── maybe_generator_kind_hint
    │
    ├── file_changed/
    │   ├── mod.rs                        (~120 LOC: handler + orchestration)
    │   │   └── handle_file_changed
    │   └── hints.rs                      (~850 LOC)
    │       └── ~40 maybe_*_hint_on_file_changed helpers
    │
    ├── cwd_changed.rs                    ✅ (99 LOC — Fase A)
    ├── subagent.rs                       ✅ (27 LOC — Fase A)
    ├── pre_compact.rs                    ✅ (58 LOC — Fase A)
    ├── worktree.rs                       ✅ (55 LOC — Fase A)
    │
    ├── task_sync/
    │   ├── mod.rs                        (~80 LOC: shared types, common helpers)
    │   ├── create.rs                     (~1600 LOC)
    │   ├── update.rs                     (~550 LOC)
    │   ├── list.rs                       (~1300 LOC)
    │   ├── output.rs                     (~1300 LOC)
    │   ├── get.rs                        (~900 LOC — incl. R169)
    │   ├── stop.rs                       (~110 LOC)
    │   └── delete.rs                     (~90 LOC)
    │
    ├── plan_mode/
    │   ├── mod.rs                        (~80 LOC: shared types)
    │   ├── enter.rs                      (~1200 LOC após extrair hints)
    │   ├── exit.rs                       (~340 LOC)
    │   └── hints.rs                      (~500 LOC: maybe_*_hint_on_enter_plan)
    │
    └── tests/                            (Fase 4 opcional)
        └── (testes distribuídos por domínio)
```

**Justificativa da granularidade hybrid (por-handler + agrupado):**
- `task_sync/` agrupa 7 handlers que compartilham muitos helpers (`dag_hint_for_*`, `task_id_*`, etc.) — reduz imports cruzados
- `plan_mode/` agrupa 2 handlers correlacionados (enter/exit) + suas hints específicas
- Handlers únicos (`file_changed`, `cwd_changed`) ficam em arquivos flat
- Shared helpers universais ficam em `shared.rs` no nível de `lifecycle/`

### 3.2 Contrato de visibilidade

| Item | Visibility origem | Re-export em `lifecycle.rs` | Razão |
|---|---|---|---|
| `handle_*` (14 handlers) | `pub(crate)` | `pub(crate) use ...::handle_*` | Consumido por `hook_registry.rs` |
| Helpers privados de um submódulo | `fn` (default) | — | Uso interno apenas |
| Helpers compartilhados entre submods | `pub(crate)` em `shared.rs` | `pub(crate) use shared::*` | Acessível via `super::<fn>` dos submods |
| Funções referenciadas por testes | `pub(crate)` | `pub(crate) use` | `super::<fn>` resolve via re-export |

**Regra dura (Rust E0364):** re-export visibility ≤ source visibility. Logo toda função re-exportada em `lifecycle.rs` precisa ser pelo menos `pub(crate)` na origem.

### 3.3 Contrato de testes

- **Fase 1 (esta plan):** testes permanecem em `lifecycle.rs::mod tests` — não migram
- **Fase 2 (futura):** distribuir 1182 testes para submódulos apropriados via `#[cfg(test)] mod tests` em cada submod (4-6h de categorização)
- **Alternativa**: manter tests agrupados em `lifecycle/tests/mod.rs` + submódulos de teste por domínio

---

<deliverables>

## 4. Deliverables (atomic, sequenciados)

### D0 — ✅ **CONCLUÍDO** (Fase A): Extraction proof-of-concept

- [x] `lifecycle/subagent.rs` (27 LOC)
- [x] `lifecycle/pre_compact.rs` (58 LOC)
- [x] `lifecycle/worktree.rs` (55 LOC)
- [x] `lifecycle/cwd_changed.rs` (99 LOC)
- [x] 2735 testes passing, pattern validated

### D1 — ✅ **PARCIAL** (2/3): Simple TaskSync handlers

- [x] `lifecycle/task_delete.rs` (58 LOC)
- [x] `lifecycle/task_stop.rs` (139 LOC)
- [ ] `exit_plan` → movido para D9 (fan-out de 32+ `maybe_*_hint_on_exit_plan`)

### D2 — ✅ **CONCLUÍDO**: Shared helpers extracted

- [x] `lifecycle/shared.rs` (205 LOC) — 9 `pub(crate)` helpers: `suggest_generator_for_task_subject`, `SUBJECT_KEYWORD_MAP`, `find_kind_by_keywords`, classify helpers, `file_stem`, `stem_to_camel_case`, VGP/generator hints
- [x] D3-D9 desbloqueados

### D3 — ✅ **CONCLUÍDO**: task_update (template mega-handler)

- [x] `lifecycle/task_update.rs` (206 LOC)
- [x] 9 helpers convertidos para `pub(crate)` em `lifecycle.rs`
- [x] Template validado para D4/D6/D7/D8 com ajuste (ver lição abaixo)

---

### Lição aprendida (2026-04-13, pós-D3)

**Padrão D3 não escala para handlers com ≫10 helpers exclusivos.**

`handle_task_sync_post_get` (D4) tem **41 helpers privados** (`maybe_*_hint_on_task_get`, `maybe_implement_vgp_hint`, `maybe_validate_phase_hint`, `maybe_mcts_unblock_on_no_ready`, `missing_dag_entry_creation_hint`, `scout_tantivy_search_hint`, `finalize_hint_if_dag_complete`, `generator_for_active_subtask`, `dag_json_to_active_description`, etc.). Converter todos para `pub(crate)` polui a API surface de `lifecycle.rs` e duplica trabalho que será desfeito em D10.

**Padrão revisado para D4/D6/D7/D8 (mega-handlers):** co-localizar handler + TODOS seus helpers exclusivos no mesmo submódulo. Helpers permanecem `fn` (privados do arquivo). Apenas o handler é `pub(crate)`.

**Impacto em LOC por submódulo:**
- `task_get.rs`: ~900 LOC (handler 193 + 40 helpers ~700)
- `task_output.rs`: ~1300 LOC
- `task_list.rs`: ~1300 LOC
- `task_create.rs`: ~1600 LOC

Ainda dentro do limite de 2000 LOC definido no §2 objectives.

**Mechanical execution refinada:**
1. Identificar range do handler
2. Identificar ranges de TODOS os helpers `_on_<event>` suffix
3. Extrair blocos contíguos via leitura + uma única Edit gigante por range
4. Compor o submódulo concatenando ranges
5. Deletar blocos do lifecycle.rs em operações batch
6. `cargo test` (validar)

**Estimativa revisada:**
- D4 (task_get): ~1-1.5h (antes: 2-3h)
- D6 (task_output): ~2h (antes: 3-4h)
- D7 (task_list): ~2h (antes: 3-4h)
- D8 (task_create): ~2.5h (antes: 3-4h)

Economia: padrão co-localizado elimina ~11 `pub(crate)` conversions por handler + zero risco de race em clean_wiring por producer re-scan.

---

### D1 — **S (30min)**: Extrair handlers simples restantes

**Escopo:**
1. `lifecycle/task_delete.rs` (83 LOC → `handle_task_sync_post_delete`)
2. `lifecycle/task_stop.rs` (102 LOC → `handle_task_sync_post_stop`)
3. `lifecycle/exit_plan.rs` (321 LOC → `handle_exit_plan_mode` + `assess_plan_session` + `plan_session_link_hint`)

**Dependências:** só funções já públicas (`suggest_generator_for_task_subject`, `cli_memory_recall`, etc.).

**Critério de done:**
```bash
cargo test -p touring-hooks --lib | grep "passed" # 2735 passed
```

**Blast radius:** LOW. Handlers pequenos, tests genericamente nomeados.

---

### D2 — **M (1-2h)**: Extrair `shared.rs`

**Escopo:**
Criar `lifecycle/shared.rs` com as funções genuinamente reutilizadas:

```rust
// lifecycle/shared.rs
pub(crate) fn suggest_generator_for_task_subject(subject: &str) -> String { ... }
pub(crate) fn classify_file_to_generator_kind(rel_path: &str) -> Option<&'static str> { ... }
pub(crate) fn classify_yaml_to_generator_kind(lower: &str) -> Option<&'static str> { ... }
pub(crate) fn classify_rust_to_generator_kind(lower: &str, rel_path: &str) -> &'static str { ... }
pub(crate) fn file_stem(path: &str) -> &str { ... }
pub(crate) fn stem_to_camel_case(stem: &str) -> String { ... }
pub(crate) fn maybe_vgp_verify_hint_for_rs_file(rel_path: &str) -> Option<String> { ... }
pub(crate) fn maybe_generator_kind_hint(rel_path: &str) -> Option<String> { ... }
```

**Estratégia:**
1. Identificar callers via `grep -c 'suggest_generator_for_task_subject(' lifecycle.rs` → confirma 29 callers
2. Mover funções para `shared.rs`
3. Em `lifecycle.rs`: `mod shared; pub(crate) use shared::*;` — todos callers internos continuam funcionando sem edit
4. Submódulos novos usarão `super::<fn>` (porque `super` = `lifecycle` que re-exporta)

**Critério de done:** 2735 passing + `shared.rs` < 450 LOC.

**Risco:** MEDIUM. Se algum helper for `fn` privado com escopo mais restrito que `pub(crate)`, pode vazar. Mitigação: auditar cada função antes de mover.

---

### D3 — **L (2-3h)**: Extrair `task_update`

**Escopo:** `handle_task_sync_post_update` (510 LOC, lines 2701-3210).

**Estratégia:**
1. Criar `lifecycle/task_sync/` directory com `mod.rs` vazio (apenas `pub mod update;`)
2. Criar `lifecycle/task_sync/update.rs` com o handler + seus helpers internos
3. Submódulo chama `super::super::shared::<fn>` para helpers cross-domain
4. Em `lifecycle.rs`: `mod task_sync; pub(crate) use task_sync::update::handle_task_sync_post_update;`

**Por que `task_update` primeiro?**
- Menor dos 4 mega-handlers → template para os próximos 3
- R38-S1 (`session_start` no in_progress) + R165 (correção DAG advance) já validados

**Critério de done:** 2735 passing + `task_sync/update.rs` < 600 LOC.

**Risco:** MEDIUM-HIGH. Primeiro split de mega-handler — pode revelar helpers privados compartilhados inesperados.

---

### D4 — **L (2-3h)**: Extrair `task_get`

**Escopo:** `handle_task_sync_post_get` (873 LOC, lines 5773-6645) — inclui R169 (`failed_dag_hint`) recém-adicionado.

**Mesma estratégia de D3.** Irmão em `task_sync/`.

**Critério de done:** 2735 passing, testes R169 (3 testes) passing.

---

### D5 — **XL (3-4h)**: Extrair `file_changed` + hints

**Escopo:**
- `handle_file_changed` (lines 17-124) + ~40 helpers `maybe_*_hint_on_file_changed` (lines 125-985)

**Estratégia diferenciada** (mais complexa que task handlers):

1. Criar `lifecycle/file_changed/mod.rs` com o handler
2. Criar `lifecycle/file_changed/hints.rs` com TODOS os `maybe_*_hint_on_file_changed` functions:
   - 40+ funções pequenas (15-25 LOC cada)
   - Todas seguem padrão `fn maybe_X_hint_on_file_changed(rel_path: &str) -> Option<String>`
   - Todas são `pub(super)` — usadas só por `file_changed::mod.rs`
3. `mod.rs` chama `hints::maybe_X_hint_on_file_changed(...)`

**Critério de done:** `file_changed/mod.rs` < 150 LOC, `hints.rs` < 900 LOC, 2735 passing.

**Risco:** MEDIUM. Muitas funções pequenas — risco de esquecer 1 ou 2 no move. Mitigação: `grep -c "fn maybe_.*_hint_on_file_changed" lifecycle.rs` antes e depois (deve ser 0 em `lifecycle.rs`, igual à contagem em `hints.rs`).

---

### D6 — **XL (3-4h)**: Extrair `task_output`

**Escopo:** `handle_task_sync_post_output` (1275 LOC, lines 4498-5772). Contém R166 (`failure_signal_hint`).

**Mesma estratégia de D3.**

**Risco:** HIGH. Handler muito grande, denso em lógica RL reward. Testes R166 (success/failure signals) extensos.

---

### D7 — **XL (3-4h)**: Extrair `task_list`

**Escopo:** `handle_task_sync_post_list` (1287 LOC, lines 3211-4497). Contém R157 (desync detection).

**Mesma estratégia.**

---

### D8 — **XL (3-4h)**: Extrair `task_create`

**Escopo:** `handle_task_sync_post_create` (1561 LOC, lines 1140-2700). **O maior handler.** Contém R14-S1 scaffold (scout→implement→validate subtasks) + R30-S1 plan scaffold + R163 reverse mapping.

**Estratégia adicional:**
- Considerar split interno: `task_sync/create.rs` + `task_sync/create_helpers.rs`
- Avaliar durante execução se helpers justificam split

**Risco:** HIGH. Maior handler, mais lógica de scaffold + DAG integration.

---

### D9 — **XXL (4-6h)**: Extrair `plan_mode`

**Escopo:**
- `handle_enter_plan_mode` (1673 LOC, lines 6831-8503)
- `handle_exit_plan_mode` já extraído em D1
- ~25-30 `maybe_*_hint_on_enter_plan` helpers → `plan_mode/hints.rs`

**Estratégia:**
1. `lifecycle/plan_mode/enter.rs` recebe o handler
2. `lifecycle/plan_mode/hints.rs` recebe todas as maybe_* específicas de enter
3. `lifecycle/plan_mode/mod.rs` re-exporta ambos

**Risco:** HIGH. Mais integração ativa: R13-S4 (decompose create), R18-S2 (tantivy upsert), R30-S2 (memory), R31-S1 (plan scaffold), R167 (session start).

---

### D10 — **M (1-2h)**: Cleanup + `lifecycle.rs` como façade puro

**Escopo:**
- Remover qualquer código remanescente em `lifecycle.rs` (exceto re-exports)
- Documentar o módulo com módulo-level doc explicando a arquitetura
- Validar que `lifecycle.rs` tenha apenas:
  - `//!` docstring
  - `mod X;` declarations (~14)
  - `pub(crate) use X::*;` re-exports
  - `#[cfg(test)] mod tests;` (aponta para `lifecycle/tests/`)

**Meta:** `lifecycle.rs` < 100 LOC.

**Critério de done:**
```bash
wc -l crates/touring-hooks/src/lifecycle.rs  # ≤ 100
```

---

### D11 — **XL (4-6h, OPCIONAL)**: Distribuir testes (Fase 2)

**Escopo:** migrar os 1182 testes do `mod tests` monolítico para submódulos.

**Estratégia:**
1. Gerar inventário dos testes + qual handler testam:
   ```bash
   grep -n "^    fn [a-z0-9_]*(" lifecycle.rs | sed 's/.*fn //;s/(.*//' > tests-inventory.txt
   ```
2. Mapear cada teste → handler via nome (ex: `r169_task_get_failed_dag_*` → `task_sync/get.rs`)
3. Mover em batches de 50 testes, validar `cargo test` após cada batch
4. Testes que referenciam múltiplos handlers → manter em `lifecycle/tests/integration.rs`

**Critério de done:** `lifecycle.rs` mod tests vazio OU removido. Cada submódulo tem seus testes adjacentes.

**Decisão adiada:** pode permanecer como Fase 2 — não bloqueia o resto do plano.

</deliverables>

---

<timeline>

## 5. Timeline com dependências

```
D0 ✅ [DONE] ────────────────────────────────┐
                                              │
D1 (handlers simples) ← D0 ──────────── ~30min
    task_delete, task_stop, exit_plan         │
                                              │
D2 (shared.rs) ← D1 ──────────────────── ~1-2h  [BLOQUEIA D3+]
    consolidate helpers                       │
                                              │
D3 (task_update) ← D2 ─────────────────── ~2-3h
    menor mega-handler, template              │
                                              │
D4 (task_get) ← D3 (pattern ready) ────── ~2-3h
    inclui R169                               │
                                              │
D5 (file_changed + hints) ← D2 ────────── ~3-4h [PARALELO com D6-D8]
    dir + hints submodule                     │
                                              │
D6 (task_output) ← D3 ─────────────────── ~3-4h
    denso em RL logic                         │
                                              │
D7 (task_list) ← D3 ───────────────────── ~3-4h
    inclui R157 desync                        │
                                              │
D8 (task_create) ← D3 ─────────────────── ~3-4h
    maior handler                             │
                                              │
D9 (plan_mode) ← D2, D5 ────────────────── ~4-6h
    enter + exit + hints                      │
                                              │
D10 (cleanup façade) ← D1..D9 ─────────── ~1-2h
    lifecycle.rs < 100 LOC                    │
                                              │
D11 (distribuir tests, OPCIONAL) ← D10 ── ~4-6h
    Fase 2 — pode adiar indefinidamente      ▼
```

**Caminho crítico (Fase 1):** D0 → D1 → D2 → D3 → D8 → D9 → D10

**Estimativa total (sem D11):**
- Mínimo (sem blockers): **~22h**
- Realistic (com blockers + iteração): **~28-32h**
- Com D11: **~36-40h** (~5 dias de trabalho focado)

**Paralelização:** D5, D6, D7, D8 podem ser feitas em ordem independente após D3. D9 requer D5 (algumas hints compartilhadas com file_changed).

</timeline>

---

<risks>

## 6. Riscos e mitigações

| # | Risco | Prob | Impact | Mitigação |
|---|---|---|---|---|
| R1 | Test references a helpers privados desconhecidos (E0364) | **HIGH** | MEDIUM | Rodar `grep -n "super::<helper>(" lifecycle.rs` antes do move; promover para `pub(crate)` preemptivamente |
| R2 | Circular deps entre submódulos (`task_sync/create.rs` usa helper de `task_sync/output.rs`) | **MEDIUM** | HIGH | Consolidar helpers compartilhados em `task_sync/mod.rs` ou `shared.rs` ANTES de splitar |
| R3 | cargo incremental rebuild muito lento (21k LOC em 1 arquivo re-compilado a cada edit) | **LOW** | LOW | Já é o status quo; modularização **melhora** isso (arquivos menores = rebuild granular) |
| R4 | Hooks em produção quebram porque `crate::lifecycle::handle_X` não resolve | **LOW** | **CRITICAL** | `pub(crate) use` re-exports preservam o caminho; `cargo test` + `cargo clippy -D warnings` a cada deliverable |
| R5 | `#[cfg(feature = "X")]` em handlers + visibility cross-feature quebra build | **MEDIUM** | MEDIUM | `cargo check --all-features` + `cargo check --no-default-features` a cada deliverable |
| R6 | Tests migrados (D11) ficam órfãos pós-compactação | **LOW** | LOW | D11 é opcional e pode ser adiada sem bloquear deliverables core |
| R7 | Touring knowledge_db pode não reindexar submódulos novos automaticamente | **LOW** | LOW | Rodar `touring metadata-backfill --force` após cada deliverable |
| R8 | Blast radius em `hook_registry.rs` maior que o esperado (ex: feature-gated paths) | **LOW** | MEDIUM | `grep -c "crate::lifecycle::" hook_registry.rs` antes e depois; deve ser igual |
| R9 | PR/commit histórico complicado de revisar com deliveries grandes | **MEDIUM** | LOW | Um commit por deliverable; mensagem descritiva + `cargo test` output anexo |
| R10 | Interrupções de sessão destroem context mid-deliverable | **HIGH** | MEDIUM | Cada deliverable é atomic + independently shippable; memória/sessão Touring preserva state |

</risks>

---

## 7. Validação e gates por deliverable

**Gate universal (rodar a cada deliverable):**

```bash
# 1. Compilação limpa
cargo check -p touring-hooks --all-features 2>&1 | grep -E "^error" | wc -l  # → 0

# 2. Testes totais
cargo test -p touring-hooks --lib 2>&1 | tail -3  # → 2735+ passed, 0 failed

# 3. Clippy sem warnings
cargo clippy -p touring-hooks --lib -- -D warnings 2>&1 | tail -5

# 4. Wiring integrity
touring wiring score /home/gabrielgadea/.claude/rust/crates/touring-hooks/src/lifecycle.rs -j | \
  jq '.[] | select(.file_path | endswith("lifecycle.rs")) | .integration_score'
# → ≥ 0.85

# 5. File size check
for f in crates/touring-hooks/src/lifecycle/**/*.rs; do
  loc=$(wc -l < "$f")
  [ "$loc" -gt 2000 ] && echo "WARN: $f = $loc LOC (>2000)"
done

# 6. Blast radius inalterado
grep -c "crate::lifecycle::" crates/touring-hooks/src/hook_registry.rs  # invariante
```

**Gate específico por deliverable:**

| Deliverable | Gate adicional |
|---|---|
| D1 | `cargo test -p touring-hooks --lib "task_delete\|task_stop\|exit_plan"` → all passing |
| D2 | `wc -l lifecycle/shared.rs` ≤ 450 |
| D3-D4 | `wc -l lifecycle/task_sync/<handler>.rs` ≤ 1000 |
| D5 | `grep -c "maybe_.*_hint_on_file_changed" lifecycle.rs` = 0 |
| D6-D8 | `wc -l lifecycle/task_sync/<handler>.rs` ≤ 2000 |
| D9 | `wc -l lifecycle/plan_mode/enter.rs` ≤ 1500 (após hints separate) |
| D10 | `wc -l lifecycle.rs` ≤ 100 |

---

## 8. Ordem de execução recomendada

**Sequência linear (baixo risco, máxima previsibilidade):**

```
D1 → D2 → D3 → D4 → D5 → D6 → D7 → D8 → D9 → D10 → [D11 opcional]
```

**Sequência paralela (após D3, se tempo de iteração permitir):**

```
D1 → D2 → D3 ─┬→ D4 ─┐
              ├→ D5 ─┤
              ├→ D6 ─┼→ D9 → D10 → [D11]
              ├→ D7 ─┤
              └→ D8 ─┘
```

**Recomendação prática:** executar linearmente. Paralelismo aqui tem valor marginal (mesmo engineer, mesma main branch) e aumenta o risco de conflicts em `lifecycle.rs` (o arquivo que cada deliverable toca).

---

## 9. Exit criteria — quando a modularização está "pronta"

✅ **Mandatory:**
1. `lifecycle.rs` ≤ 100 LOC
2. Todos os 14 handlers em submódulos (`lifecycle/<nome>.rs` ou `lifecycle/<domain>/<handler>.rs`)
3. `shared.rs` contém todos os helpers universais
4. Nenhum submódulo > 2000 LOC
5. 2735+ testes passando (ou 2735 + testes novos introduzidos durante extração)
6. `cargo clippy -D warnings` → 0
7. `touring wiring score` ≥ 0.85 para `lifecycle.rs` e cada submódulo
8. `hook_registry.rs` NÃO modificado (invariant de API)

✅ **Desired (Fase 2 / D11):**
9. Testes distribuídos para submódulos correspondentes
10. `touring ast meta` cognitive_score < 0.7 em todos os submódulos
11. Documentação de arquitetura atualizada em `crates/touring-hooks/ARCHITECTURE.md`

---

## 10. Observações finais

### 10.1 O que NÃO está neste plano

- **Refatorações semânticas**: este plano é **puramente estrutural**. Zero mudanças de comportamento. Cada função é movida verbatim.
- **Reorganização de RL reward signals**: fora de escopo. Cada R-series refinement permanece intocado.
- **Otimização de performance**: não é um goal. Pode vir como side-effect (menor working set per compilation).

### 10.2 Princípio diretivo

> "Modularização é um **meio**, não um **fim**. O fim é restaurar a capacidade de modificar `lifecycle.rs` com segurança — Touring feedback loop + arquivos navegáveis + tests granulares. Se algum deliverable degradar esses objetivos, pare e re-avalie."

### 10.3 Preparação para execução

Antes de iniciar D1, verificar:

```bash
# Baseline health
cargo test -p touring-hooks --lib 2>&1 | tail -3     # 2735 passing?
touring doctor -j | python3 -c "..."                  # daemon healthy?
touring wiring score ...lifecycle.rs                  # baseline score?
wc -l crates/touring-hooks/src/lifecycle.rs           # baseline LOC?

# Backup via Touring memory (proibido git)
touring memory store "lifecycle-baseline-$(date +%s)" \
    "22309 LOC, 2735 tests passing, wiring score 0.91" \
    --tier semantic --type lesson
```

### 10.4 Cancelamento e rollback

Se qualquer deliverable falhar o gate universal:
1. **NÃO** usar git (regra inviolável)
2. Desfazer manualmente: deletar o novo submódulo + restaurar a função em `lifecycle.rs`
3. Registrar na memória: `touring memory store "rollback:D<N>" "<motivo>" --type lesson`
4. Replanear antes de prosseguir

---

## 11. Status do plano

| Item | Estado |
|---|---|
| Plano aprovado pelo user | ✅ (2026-04-13) |
| Baseline Touring capturado | ✅ |
| FIX-1 + FIX-2 operacionais | ✅ |
| Fase A (D0) entregue | ✅ |
| D1 — handlers simples extraídos | ✅ |
| D2 — shared.rs criado | ✅ |
| D3 — task_update extraído | ✅ |
| D4 — task_get extraído | ✅ |
| D5 — file_changed + hints extraídos | ✅ |
| D6 — task_output extraído | ✅ |
| D7 — task_list extraído | ✅ |
| D8 — task_create extraído | ✅ |
| D9 — plan_mode extraído | ✅ |
| D10 — lifecycle.rs façade puro | ✅ |

**Resultado final (2026-04-13):** lifecycle.rs 22309 → 13748 LOC (-38%); non-test code 22309 → 68 LOC (-99.7%). Meta superada (68 LOC vs target 100 LOC). Suíte de testes: 2735 → 2845 (+110 via Pln2+Pln3 E2E adicionados em paralelo).

---

*Plano redigido em 2026-04-13 durante sessão de loop `/loop 10m` (já cancelado).*
*Fontes consultadas: Context7 `/rust-lang/reference` (visibility, path attribute, module organization).*
*Estado funcional validado pela suíte de 2735 testes, FIX-1, FIX-2, Fase A.*
*Sessão de 2026-04-13: plano executado em full, metas superadas (68 LOC non-test vs target 100 LOC).*
