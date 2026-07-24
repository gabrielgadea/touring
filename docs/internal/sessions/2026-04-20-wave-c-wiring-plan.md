# Wave C Wiring Completion — Plano Detalhado

> **Data**: 2026-04-20 | **Autor**: TACO (Claude Code em modo Touring) | **Status**: Aguardando aprovação Gabriel
> **Contexto**: fecha o ciclo das Waves C1 → C2 entregues em `2026-04-20`
> **Escopo**: 3 wirings inline restantes (C2-post_edit, C1.6-decomposer-bandit, C2→decomposer-cascade)

---

## Objective

### O quê
Completar 3 wirings inline que ativam o ciclo end-to-end
**edit → análise → reward → decomposição adaptativa**:

1. **C2-post_edit inline** — post_edit hook chama `analyze_rust_edit` após
   cada edit Rust e emite `tracing::warn` em High severity
2. **C1.6 — TaskDecomposer consome GranularityBandit** — `create_task_with_cila`
   consulta bandit via daemon query e auto-popula subtasks com split factor
3. **C2 → TaskDecomposer cascade bridge** — proposals de `CascadePlan.high_severity()`
   viram subtasks reais em DAG ativo via queue drain

### Por quê
As 7 waves anteriores (sessão 2026-04-20) entregaram 81 testes verdes e APIs
library-ready em 5 crates. Porém sem os consumers finais:

- `GranularityBandit` existe mas ninguém consulta → bandit nunca aprende
- `plan_api_cascade` existe mas post_edit não chama → cascade não fira
- Proposals de cascade não viram subtasks → decomposer fica estático

**Fechar o loop transforma código potencial em efeito observável em runtime.**

### Success Criteria (binário, testável)

| # | Critério | Teste |
|---|---|---|
| 1 | Editar `.rs` → `tracing::warn` emitido quando `Severity::High` detectada | `post_edit_rust_edit_emits_cascade_log` |
| 2 | MCP `touring_decompose create` L3+ Rust → subtasks auto-geradas com split factor do bandit | `decompose_with_hint_creates_split3_subtasks` |
| 3 | Edit remove API pública com callers → proposals viram subtasks reais no DAG ativo | `post_edit_to_decomposer_cascade_e2e` |
| 4 | `cargo check --workspace` PASS | workspace gate |
| 5 | `cargo clippy --workspace -- -D warnings` PASS | lint gate |
| 6 | Testes existentes 100% preservados (46→46 C1 arc + 23 C2 + 12 bridge) | regression gate |

---

## VGP Discovery — Ground Truth

Executado antes do planejamento (confidence 1.0):

| Claim | Evidência |
|---|---|
| `post_edit.rs:281` lê `file_content` via `std::fs::read_to_string` | `grep -n "read_to_string" crates/touring-hooks/src/post_edit.rs` |
| `post_edit.rs` tem 2806 LOC, CC alto em múltiplos handlers | `wc -l` + post-edit hook reports |
| `TaskDecomposer::new()` em `server/mod.rs:279`, wrapped `Arc<RwLock<_>>` | `grep -rn "TaskDecomposer::new"` |
| `create_task_with_cila` called from `tools_analysis.rs:250` (MCP entry) | mesma busca |
| Decomposer **não** importa `HookRuntime` (zero resultados em `reasoning/`) | `grep HookRuntime crates/touring-server/src/reasoning/` |
| `api_cascade_bridge::ApiSurfaceCache` entregue, 12 tests PASS (wave anterior) | wave memory store `wave_c2_wiring_bridge_2026_04_20` |

**Conclusão-chave**: Decomposer e HookRuntime vivem em crates desacoplados
(touring-server vs touring-hooks). Integração exige **query pattern** (não shared
state) — decomposer solicita hint via daemon query, recebe JSON de volta.

---

## Deliverables

### D1 — C2-wiring inline em post_edit

**Tamanho**: S (~30min) | **Dependências**: nenhuma (bridge já entregue) | **Risco**: MEDIUM

**Arquivos alterados**:
- `crates/touring-hooks/src/hook_runtime.rs` (+ 2 linhas)
- `crates/touring-hooks/src/post_edit.rs` (+ ~10 linhas num ponto isolado)
- `crates/touring-hooks/tests/post_edit_cascade_e2e.rs` (novo, ~80 linhas)

**Passos**:
1. Adicionar field `pub api_cascade_cache: ApiSurfaceCache` em struct
   `ContextRuntime` (próximo a linha 380)
2. Inicializar com `ApiSurfaceCache::new()` em `HookRuntime::new` (próximo a
   linha 790, mesmo bloco do `session_bus`)
3. Em `post_edit.rs:281` (imediatamente após `file_content` lido), invocar:
   ```rust
   if let Some(ref src) = file_content {
       let outcome = shared::api_cascade_bridge::analyze_rust_edit(
           file_path, src, &runtime.ctx.api_cascade_cache,
       );
       if let Some(plan) = outcome.plan() {
           shared::api_cascade_bridge::log_cascade_plan(file_path, plan);
       }
   }
   ```
4. Adicionar 3 E2E tests:
   - `post_edit_rust_edit_populates_cache` — cache cresce em 1
   - `post_edit_second_rust_edit_emits_cascade_plan` — diff produzido
   - `post_edit_non_rust_skips_cascade` — `.py` não toca cache

**Validação**: `cargo test -p touring-hooks --test post_edit_cascade_e2e`

---

### D2 — Query adapter para GranularityBandit no touring-server

**Tamanho**: S (~20min) | **Dependências**: nenhuma (HookRuntime wiring C1.5 entregue) | **Risco**: LOW

**Arquivos alterados**:
- `crates/touring-server/src/reasoning/granularity_adapter.rs` (novo, ~80 linhas)
- `crates/touring-hooks/src/cli_handlers.rs` (+ ~30 linhas)
- `crates/touring-hooks/src/hook_registry.rs` (+ 1 entry, count 143→144)

**Passos**:
1. Criar struct `GranularityHint` com fields:
   ```rust
   pub struct GranularityHint {
       pub split_factor: SplitFactor,
       pub score: f64,
       pub estimated_loc: usize,
       pub language: String,
       pub cila_level: u8,
   }
   ```
2. Adicionar handler `cli_granularity_hint(rt, payload)` em `cli_handlers.rs`:
   - Parse payload `{size_loc, language, cila_level}`
   - Call `rt.select_task_split(...)`
   - Retorna JSON `{split_factor, score, subtask_count}`
3. Registrar `"cli-granularity-hint"` em `hook_registry.rs` (3 locais: 2 arrays + dispatch map)
4. Adapter `query_granularity_hint(size_loc, lang, cila) -> Result<GranularityHint>`
   em `granularity_adapter.rs` envia hook query via `daemon_query`
5. Atualizar test `registry_has_expected_count` para 144
6. Tests adapter + handler E2E

**Validação**: `cargo test -p touring-hooks hook_registry + granularity_adapter`

---

### D3 — C1.6: TaskDecomposer consome GranularityHint

**Tamanho**: M (~1h) | **Dependências**: D2 | **Risco**: MEDIUM (decomposer.rs 2344 LOC)

**Arquivos alterados**:
- `crates/touring-server/src/reasoning/decomposer.rs` (+ ~20 linhas backward-compat)
- `crates/touring-server/src/server/tools_analysis.rs` (+ ~10 linhas)

**Passos**:
1. Adicionar método **novo** (não altera existente) em `TaskDecomposer`:
   ```rust
   pub(crate) fn create_task_with_cila_and_hint(
       &mut self,
       task_type: &str,
       description: &str,
       cila_level: u8,
       hint: Option<&GranularityHint>,
   ) -> String {
       let task_id = self.create_task_with_cila(task_type, description, cila_level);
       if let (Some(h), true) = (hint, cila_level >= 3) {
           self.bootstrap_subtasks_from_hint(&task_id, h);
       }
       task_id
   }
   ```
2. Adicionar helper privado `bootstrap_subtasks_from_hint` que:
   - Gera N subtasks placeholder nomeadas `sub_1..sub_N` onde N = `split_factor.subtask_count()`
   - Cada `sub_i` depende de `sub_{i-1}` (deps chain sequencial)
3. Em `tools_analysis.rs` MCP tool `touring_decompose`:
   - Antes de chamar decomposer, query granularity hint (opcional via flag `auto_decompose: bool`)
   - Se hint obtido + L3+, usar `create_task_with_cila_and_hint`
   - Senão fallback para `create_task_with_cila` existente (backward compat)
4. Schema MCP adiciona `auto_decompose: Option<bool>` (default `false`)
5. Tests:
   - `decompose_without_hint_creates_empty_task` — backward compat
   - `decompose_with_split3_hint_creates_3_subtasks_with_deps_chain`
   - `decompose_l1_ignores_hint` — L0-L1 não splitam
   - `decompose_hint_absent_falls_back_to_plain_create`

**Validação**: `cargo test -p touring-server reasoning::decomposer`

---

### D4 — Cascade queue entre post_edit e decomposer

**Tamanho**: M (~45min) | **Dependências**: D1 + D3 | **Risco**: LOW

**Arquivos alterados**:
- `crates/touring-hooks/src/shared/cascade_queue.rs` (novo, ~100 linhas)
- `crates/touring-hooks/src/shared/mod.rs` (+ 1 pub mod)
- `crates/touring-hooks/src/post_edit.rs` (+ 3 linhas — enqueue após log)
- `crates/touring-server/src/server/tools_analysis.rs` (+ ~15 linhas — drain action)

**Passos**:
1. Criar `CascadeQueue` com campos:
   ```rust
   pub struct CascadeQueue {
       inner: Mutex<VecDeque<PendingCascade>>,
       max_len: usize, // = 256
       ttl: Duration,  // = Duration::from_secs(3600)
   }
   pub struct PendingCascade {
       pub path: PathBuf,
       pub proposals: Vec<SubtaskProposal>,
       pub queued_at: SystemTime,
   }
   ```
2. APIs:
   - `push(path, plan)` — filtra para apenas `high_severity()`, drop se queue cheia
   - `drain_fresh()` — drena pending com `now - queued_at < ttl`, descarta stale
   - `len()`, `is_empty()`, `evict_stale()` (idempotente)
3. Adicionar field `pub cascade_queue: CascadeQueue` em `ContextRuntime`
4. Em `post_edit.rs` após `log_cascade_plan`, se plan tem High severity:
   ```rust
   runtime.ctx.cascade_queue.push(file_path, plan);
   ```
5. Nova MCP action `touring_decompose` sub-action `drain_cascades`:
   - Itera `drain_fresh()`
   - Para cada `PendingCascade`, se task ativo existe, adiciona subtask via `add_subtask`
   - Returns `{drained_count, subtasks_added, stale_evicted}`
6. Tests:
   - `queue_push_then_drain_roundtrip`
   - `queue_respects_max_len_capacity`
   - `stale_cascades_are_evicted_by_ttl`
   - `integration_post_edit_to_drain_creates_subtasks`

**Validação**: `cargo test -p touring-hooks shared::cascade_queue`

---

### D5 — CLI observability + MCP tool

**Tamanho**: S (~25min) | **Dependências**: D4 | **Risco**: none

**Arquivos alterados**:
- `crates/touring-hooks/src/cli_handlers.rs` (+ 2 handlers, ~40 linhas)
- `crates/touring-hooks/src/hook_registry.rs` (+ 2 entries, count 144→146)
- `crates/touring-server/src/cli/cascade.rs` (novo, ~50 linhas)
- `crates/touring-server/src/cli/mod.rs` (+ 1 pub mod)
- `crates/touring-server/src/cli/common.rs` (+ 1 CommandDescriptor)

**Passos**:
1. Handlers:
   - `cli_cascade_queue_status` — retorna `{pending_count, stale_count, oldest_age_secs}`
   - `cli_cascade_queue_drain` — drena via `drain_fresh()`, retorna resumo
2. Registrar em registry (count 144→146)
3. CLI subcommand:
   ```bash
   touring cascade queue          # status
   touring cascade drain          # explicit drain
   ```
4. MCP tool `touring_cascade_queue` via `#[tool]` macro em `TouringServer`
5. Atualizar `registry_has_expected_count` para 146
6. Tests: queue status JSON shape + drain side effects

**Validação**: `cargo test -p touring-hooks + cargo test -p touring-server cli::cascade`

---

### D6 — Integration E2E + docs

**Tamanho**: S (~30min) | **Dependências**: D1..D5 | **Risco**: none (test-only)

**Arquivos alterados**:
- `crates/touring-integration-tests/tests/wave_c_e2e.rs` (novo, ~150 linhas)
- `docs/2026-04-20-wave-c-wiring.md` (novo — session report)
- `MEMORY.md` + memory store

**Passos**:
1. Full-cycle E2E test:
   - Setup: `TempDir` com projeto Rust dummy (`Cargo.toml` + `src/lib.rs`)
   - Edit 1: define `pub fn foo()` + consumer `fn bar() { foo() }`
   - `HookRuntime::new` → cache populado (FirstObservation)
   - Edit 2: remove `pub fn foo()` (quebra consumer)
   - Verify: `api_cascade_cache.get()` retorna prior surface, plan tem `Severity::High`, queue tem 1 pending
   - Invoke MCP `touring_decompose create` com `auto_decompose=true`
   - Verify: task criada com N subtasks (via GranularityBandit)
   - Invoke MCP `touring_cascade_queue drain`
   - Verify: subtasks extras anexadas representando callers a atualizar
2. Session report documentando:
   - Arquitetura callback injection vs shared state (decisão)
   - Tabela de hook count evolution
   - Benchmarks latência (opcional: hdrhistogram em cascata full)
3. Memory store `wave_c_wiring_completion_2026_04_20`
4. Update MEMORY.md

**Validação**: `cargo test -p touring-integration-tests wave_c_e2e` + `touring memory recall`

---

## Timeline — Sequenciamento com DAG

```
┌─────────────────────────────────────────────────────────────────┐
│ Session 1 — Foundation (paralelo possível)                      │
│                                                                 │
│ ├─ FASE 0 — health gate (cargo check + touring doctor)         │
│ ├─ D1 (S, 30min) ─────── independente                           │
│ └─ D2 (S, 20min) ─────── independente                           │
│                                                                 │
│ Validação: D1 tests + D2 tests passando isoladamente            │
├─────────────────────────────────────────────────────────────────┤
│ Session 2 — Decomposer integration                              │
│                                                                 │
│ ├─ Requires: D2 merged                                          │
│ ├─ D3 (M, 1h) ─────── depende D2 (adapter)                      │
│ └─ Backward-compat tests + 3 novos scenarios                    │
│                                                                 │
│ Validação: decomposer tests preservados + 3 novos verdes        │
├─────────────────────────────────────────────────────────────────┤
│ Session 3 — Queue bridge + observability                        │
│                                                                 │
│ ├─ Requires: D1 + D3 merged                                     │
│ ├─ D4 (M, 45min) ─────── depende D1+D3 (queue + decomposer)     │
│ └─ D5 (S, 25min) ─────── depende D4 (CLI + MCP expose)          │
│                                                                 │
│ Validação: queue roundtrip + CLI shapes + MCP schema            │
├─────────────────────────────────────────────────────────────────┤
│ Session 4 — Validation + docs                                   │
│                                                                 │
│ ├─ Requires: D1..D5 merged                                      │
│ ├─ D6 (S, 30min) ─────── E2E full-cycle + session report        │
│ └─ Final gates: cargo test --workspace + clippy -D warnings     │
│                                                                 │
│ Memory store + MEMORY.md update                                 │
└─────────────────────────────────────────────────────────────────┘

DAG de dependências:
         ┌─── D1 ──┐
FASE 0 ──┤         ├─── D4 ── D5 ── D6
         └─── D2 ──┴── D3 ────┘
```

**Total estimado**: ~3.5h calendar time, ~4 sessões frescas para evitar context drift.

**Paralelizável**: Session 1 pode rodar D1+D2 em 2 `touring-engineer` agents paralelos com `mode=acceptEdits`. A partir de Session 2 é sequencial.

---

## Risks & Mitigations

| # | Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|---|
| R1 | Touching `post_edit.rs` quebra signal pipeline (CC alto em handlers adjacentes) | MEDIUM | HIGH | D1 adiciona apenas 10 linhas num ponto bem isolado (após `file_content` read). Test E2E executa post_edit full antes/depois. Rollback = 1 revert. |
| R2 | `decomposer.rs` mudança de assinatura quebra callers `pub(crate)` | MEDIUM | HIGH | D3 usa **novo método** (`create_task_with_cila_and_hint`) — método antigo intocado. Zero callers quebram. `cargo check --workspace` valida. |
| R3 | `HookRuntime ↔ Decomposer` cross-crate state race | LOW | MEDIUM | D2 usa query pattern (não shared state). Hook handler é Mutex-protected dentro do actor per-project. Read-only do ponto de vista do decomposer. |
| R4 | Cascade queue vaza memória sob load (proposals nunca drenadas) | MEDIUM | MEDIUM | TTL 1h + bounded capacity 256 cascades + `evict_stale` idempotente. Counter em `gate-metrics`. |
| R5 | `granularity_hint` latência (daemon round-trip) atrasa MCP response | LOW | LOW | Query cache (W17 pattern, 60s TTL) reutiliza resultado. Timeout 100ms com fallback para `SplitFactor::Monolithic`. |
| R6 | Session drift / context overflow em sessões longas | HIGH | MEDIUM | Split em 4 sessões conforme timeline. Cada sessão entrega 1-2 deliverables atomicamente. Memory store após cada session. |
| R7 | Decomposer subtasks geradas não refletem real complexity (bandit cold-start) | HIGH | LOW | Bandit já faz cold-start exploration (`COLD_ARM_THRESHOLD=3`). Primeiras 12 decisões são forçadas. Warm-up acontece naturalmente. Tests provaram convergência. |
| R8 | Tests E2E (D6) flakey em ambiente CI | MEDIUM | LOW | Use `tempfile::TempDir` (padrão). Evita paths absolutos. Assertions determinísticas (não timing). Sem `std::thread::sleep`. |
| R9 | `registry_has_expected_count` test breaks em cada adição de hook | LOW | LOW | Pattern conhecido (memória `project_wave24_hook_synergy_2026_04_18`). Cada D que adiciona hook atualiza 3 pontos simultaneamente: arrays + dispatch + count assertion. |

---

## Self-Validation

| Critério | Status | Evidência |
|---|---|---|
| Cada deliverable é atômico e independentemente shippable | ✅ | D1/D2 standalone; D3..D6 têm deps explícitas mas ship individual |
| Dependências explícitas e acíclicas | ✅ | DAG validada em Timeline section |
| Estimativas realistas | ✅ | Baseadas em tamanhos reais dos arquivos tocados (medidos via wc -l) |
| Riscos com mitigações | ✅ | 9 riscos tabulados, todos com mitigação específica |
| Success criteria binário | ✅ | 6 critérios testáveis declarados em Objective |
| Zero breaking changes | ✅ | D1-D5 aditivos (novos fields optional, novos métodos); D6 test-only |
| VGP verificado | ✅ | 6 claims cruzadas com source via grep/Read antes do plano |

---

## Recomendação de Execução

**Não implementar inline agora** — plano precisa aprovação Gabriel +
idealmente delegação a `touring-engineer` agents com `mode=acceptEdits` em
sessões frescas (evita context drift de ~7 waves acumuladas).

### Próxima ação sugerida

```bash
# 1. Registrar DAG formal no decompose
touring decompose create intent "Wave C wiring completion — D1..D6" \
    --origin=touring-cli --cila-level=4

# 2. Adicionar subtasks
touring decompose add <task> D1 "post_edit inline wiring"
touring decompose add <task> D2 "granularity query adapter"
touring decompose add <task> D3 "decomposer consumes hint" D2
touring decompose add <task> D4 "cascade queue bridge" D1,D3
touring decompose add <task> D5 "CLI + MCP observability" D4
touring decompose add <task> D6 "E2E integration + docs" D5

# 3. Validar DAG
touring decompose validate <task>

# 4. Session 1 — delegar D1 e D2 em paralelo
# Agent(subagent_type="touring-engineer", mode="acceptEdits",
#       prompt="executar D1 conforme docs/2026-04-20-wave-c-wiring-plan.md")
# Agent(subagent_type="touring-engineer", mode="acceptEdits",
#       prompt="executar D2 conforme docs/2026-04-20-wave-c-wiring-plan.md")
```

### Pontos de decisão para Gabriel

1. **Aprovar o plano completo (D1..D6) ou subset?**
   - Subset mínimo: D1 + D6 (observability only, sem decomposer integration)
   - Subset médio: D1 + D2 + D3 (sem cascade queue)
   - Completo: D1..D6 (recomendado)

2. **Executar em sessões dedicadas ou delegar a `touring-engineer` agents?**
   - Sessões dedicadas: mais controle, testes incrementais, tempo Gabriel maior
   - Delegação: throughput maior, mas exige review pós-facto

3. **Priorizar D1 (observability imediata) ou D3 (decomposer integration)?**
   - D1 gera valor observável em 30min
   - D3 é o coração da sinergia mas exige D2 primeiro

4. **Criar o DAG formal via `touring decompose create` agora ou aguardar aprovação?**

---

## Arquivos de Referência

| Tópico | Arquivo |
|---|---|
| Wave C1 GranularityBandit lesson | `~/.claude/projects/-home-gabrielgadea/memory/project_wave_c1_granularity_bandit_2026_04_20.md` |
| Wave C2 api_cascade lesson | `~/.claude/projects/-home-gabrielgadea/memory/wave_c2_api_cascade_2026_04_20` (memory store) |
| Wave C2 bridge lesson | `~/.claude/projects/-home-gabrielgadea/memory/wave_c2_wiring_bridge_2026_04_20` (memory store) |
| Bridge implementation | `crates/touring-hooks/src/shared/api_cascade_bridge.rs` |
| GranularityBandit impl | `crates/touring-learning/src/bandit/granularity.rs` |
| HookRuntime wiring | `crates/touring-hooks/src/hook_runtime.rs` (linhas 1380+) |
| Decomposer | `crates/touring-server/src/reasoning/decomposer.rs:913` |
| post_edit call site | `crates/touring-hooks/src/post_edit.rs:281` |

---

*Plano v1.0 — 2026-04-20 | TACO Phase Protocol v6.2 | Aguardando aprovação Gabriel.*
