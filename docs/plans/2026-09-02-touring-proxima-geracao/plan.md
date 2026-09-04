---
type: Plan
plan_id: 2026-09-02-touring-proxima-geracao
plan: touring-proxima-geracao-harness-modular
title: "Touring próxima geração — harness modular com reinjeção medida (Pln2)"
description: "Plano L4 medição-primeiro: identidade de telemetria, régua por injeção com teto por turno no executor, contrato de módulo fora-de-processo + built-ins Rust, primeira filha analise-regulatorio, publicação."
okf_version: 0.1
tags: [plan, pln2, harness, modularizacao, reinjecao, telemetria]
timestamp: 2026-09-02T07:40:00-03:00
authored: 2026-09-02
level: L4
status: DRAFT
intent: |
  Fazer do Touring o Sistema 1 do agente: toda reinjeção de contexto carrega um id determinístico, respeita um teto de bytes por turno aplicado no executor e é medida pelo efeito no turno seguinte; um módulo novo entra por contrato público fora-de-processo com diff zero no núcleo; built-ins Rust implementam o mesmo trait; primeira filha: analise-regulatorio.
quality_dimensions: [precision, scalability, performance, functionality, quality, detail, integration, dependencies, potentiation]
ground_truth_ref: ground_truth.json
toolkit_version: taco-planning-v2.0
---

# Touring próxima geração — harness modular com reinjeção medida (Pln2)

> **Intent**: ver frontmatter e [`strategy-2026-09-02-touring-proxima-geracao.md`](/strategy-2026-09-02-touring-proxima-geracao.md) §1.
> **Level**: L4 (novo subsistema multi-crate) | **Authored**: 2026-09-02 | **Aprovado**: Gabriel, HUMAN GATE passo 9.
> **Composite goal**: every dimension ≥ 8, no dimension < 7.
> **DAG**: `task_1788344667602060021` (19 subtasks, `touring decompose ready task_1788344667602060021`); marker do loop armado; convergência por `loop_converged.py --task task_1788344667602060021`.
> **Convenção de citação**: crase = símbolo EXISTENTE verificado por VGP (`ground_truth.json`); **negrito** = símbolo NOVO a criar (não existe no índice por definição).

Parte do [bundle](/index.md). Concepção: [`criacao.md`](/criacao.md).

---

## 1. Ground Truth Summary

> Source — `ground_truth.json` (coletor + `vgp_augment.py`, `touring index find` nativo; o sandbox não alcança o índice). Diagnóstico: [`diagnostics/touring-20260902T062959.md`](/diagnostics/touring-20260902T062959.md).

| Field | Value | Confidence |
|-------|-------|------------|
| `touring doctor` overall | DEGRADED (project_actor sem resposta; index find nativo funciona) | FACT [1.0] |
| composite_health / quality 50-dim | 0,6864 / 0,9371 Platinum, 0 blockers | FACT [1.0] |
| Wiring orphan count (baseline REGRA #0) | **1.759** — medido 02/09 ~11:00, DEPOIS da wave 14 de outra sessão (v30.4.33 corrigiu 4 furos do detector de wiring: 5.535 → 1.754). O baseline do diagnóstico das 06:29 (5.530) está obsoleto | FACT [1.0] |
| Index symbol count | 680.522 (9.754 arquivos) | FACT [1.0] |
| hook_dispatch_latency | p50 0,65 ms · p90 133 ms · max 1,18 s (n=4.364) | FACT [1.0] |
| enrichment emitido | 81 emissões · 105.422 bytes · média 1.301 B (só agregado, sem id) | FACT [1.0] |
| Teto de injeção existente | POR CHAMADA e graduado por CILA: `cila_budget_read` 800/2.000/4.000 e `cila_budget_edit`/`write` 1.200/3.000/6.000 (cila.rs:33-51, override por env), truncado no executor em signal_pipeline.rs:139 e :195; alertas `ctx_budget_warning` 75% / `ctx_budget_alert` 90% em `record_enrichment_metrics` (pre_read.rs:392-401); `DEFAULT_CONTEXT_BUDGET`=3.200 (pre_read.rs:974) é fallback de compose/compactação/sessão. **NÃO há teto por TURNO** | FACT [1.0] |
| Atribuição existente | post_tool_rl.rs:58-76 reconhece a injeção pelo marcador `__context_injection_file__` (caminho de ARQUIVO, último gravador vence), soma 0,1 se bate e não houve erro; recompensa `context_injection_quality` :264. **Produtores do marcador: só pre_read.rs:362 e pre_write.rs:279 — `pre_edit` não grava, logo edição é inatribuível hoje** | FACT [1.0] |
| Implementações de `SignalLayer` | 16 structs em 11 arquivos (lista estática; sem registry) | FACT [1.0] |
| Taxonomia de eventos (3 vocabulários) | `ALL_DAEMON_HOOK_NAMES` hook_registry.rs:401 = **239 nomes**; tabela de despacho = **245 entradas distintas** (199 `cli-*` de telemetria; lifecycle: 9 pre, 9 post, 12 task, 3 subagent, 2 session, 2 teammate, 1 ceg); settings.json do Claude Code = 27 tipos de evento. Nenhum enum tipado, nenhum modo de despacho declarado | FACT [1.0] |
| `tool_use_id` no payload | PostToolUse: FACT (docs oficiais + fixture post_bash.rs:1119); PreToolUse: INFERENCE [0.8] (SDK doc: "tool use ID to correlate pre and post") | ver S-1 |
| Identidade que já existe | `ActionSignature::to_key()` action_signature.rs:139-146 → `outcome:<tool_class>:<intent_class>:<qualifier>`, emitida como `sig=` (cli_suggester.rs:5727) — identidade de **CLASSE**. `grep injection_id\|inject_id\|Uuid::new\|nanoid` nos 3 crates de hook = **0 linhas** | FACT [1.0] |
| Proxy de STR existente | `mean_enrichment_bytes = enrichment_context_bytes_total / enrichment_emit_count` (gate_metrics_snapshot.rs:665, :1368) — média global, não por módulo | FACT [1.0] |
| 4º sink de telemetria | `JournalEntry` journal.rs:82+ — **v2 desde a wave 14** (`ts, language, exit_code, failure_kind, duration_ms, code_hash_stdout_bytes, bytes_elided, source, file, harvest, brief, orchestrate, tmp_bytes`, todos `serde(default)`), agregado por `JournalAggregate` (consumido em kpi.rs:506). Continua **sem session_id, sem hook, sem turno** | FACT [1.0] |
| Facetas da memória | `enum Facet` tags.rs:189-205 (Kind, Purpose, Lang, Domain, Process, Artifact, Status) | FACT [1.0] |
| Memory lessons applied | 10 (ver ground_truth.memory_lessons) | FACT [1.0] |

### Symbols verified (VGP)

| Symbol | File | Line | Kind |
|--------|------|-----:|------|
| `SignalLayer` | crates/touring-hooks-shared/src/signal_layer.rs | 130 | trait (`name`, `enrich`, `should_run`) |
| `SignalContext` | crates/touring-hooks-shared/src/signal_layer.rs | 48 | struct (v2: `tool_name`, `proposed`, `analysable_text`) |
| `ProposedChange` | crates/touring-hooks-shared/src/signal_layer.rs | 21 | enum |
| `LayerMetrics` | crates/touring-hooks-shared/src/signal_layer.rs | 116 | struct |
| `SignalPipeline` | crates/touring-hook-handlers/src/shared/signal_pipeline.rs | 50 | struct (`add_layer` 69, `with_budget` 81, `execute` 99, `execute_with_metrics` 156) |
| `StaticSignalLayer` / `CilaGatedLayer` / `FnSignalLayer` | signal_pipeline.rs | 221 / 246 / 271 | adapters existentes |
| `BlastRadiusSignalLayer` | signal_pipeline.rs | 383 | struct (candidata a migrar, W2) |
| `QualityBaselineLayer` | crates/touring-hook-handlers/src/shared/quality_signal.rs | 73 | struct (candidata a migrar, W2) |
| `HookTraceLine` | crates/touring-hook-runtime/src/hook_trace.rs | 32 | struct (`ts_ms, pid, ppid, hook, route, exit_reason, stdin_state, stdin_bytes, elapsed_ms, session_id`) |
| `ENV_TRACE_FILE` / `build_line` / `append_line` | hook_trace.rs | 28 / 123 / 153 | const / fn / fn |
| `HookCallEntry` | crates/touring-code/src/sdk_signal_mirror.rs | 51 | struct (`ts, hook_name, duration_ms, success, origin`) |
| `MirrorOrigin` / `MirrorAggregate` / `augment_with_mirror` | sdk_signal_mirror.rs | 74 / 98 / 228 | enum / struct / fn |
| `cli_kpi` / `CouplingSignals` / `actuator_signals` | crates/touring-cli/src/cli/kpi.rs | 125 / 1620 / 1777 | fn / struct / fn |
| `EnrichmentData` / `Suggestion` / `RouteOffer` / `record_route_offer` / `claim_route_reward` | crates/touring-cli/src/cli_suggester.rs | 314 / 362 / 4030 / 4209 / 4555 | emissão de additionalContext em 3765 (advisory_response) |
| `DEFAULT_CONTEXT_BUDGET` / `compose_high_signal_context_budgeted` | crates/touring-hook-handlers/src/hooks/pre_read.rs | 974 / 1038 | const / fn |
| `compose_edit_context` | crates/touring-hook-handlers/src/hooks/pre_edit.rs | 585 | fn |
| `EntityId` | crates/touring-identity/src/types.rs | 25 | struct (REGRA #17 — derivação determinística; molde do id de injeção) |
| `CodeModePresentation` / `project_presentation` | crates/touring-foundation/src/code_mode.rs | 29 / 79 | enum / fn (molde de declaração por escopo) |
| `BestPracticesGate` | crates/touring-quality/src/builtins/best_practices.rs | 36 | struct (regra 4 `signal_use`; ganha regra 5) |
| `cila_budget_read` / `cila_budget_edit` / `cila_budget_write` | crates/touring-hooks-shared/src/cila.rs | 33 / 41 / 49 | fn (teto por chamada, graduado por CILA) |
| `ActionSignature::to_key` | crates/touring-hooks-shared/src/action_signature.rs | 139 | fn (molde de identidade — de classe) |
| `JournalEntry` | crates/touring-code/src/journal.rs | 82 | struct (4º sink) |
| `Facet` | crates/touring-intelligence/src/rl/memory/tags.rs | 189 | enum (7 facetas) |
| `ALL_DAEMON_HOOK_NAMES` | crates/touring-dispatch/src/hook_registry.rs | 401 | const (239 nomes) |
| `record_enrichment_metrics` | crates/touring-hook-handlers/src/hooks/pre_read.rs | 392 | fn (emite ctx_budget_warning/alert) |

### Past lessons applied

- **f3-posttool-use-sync-2026-09-01** — o mirror JSONL append-only crash-safe é o padrão de sink para o ledger de injeções (S-7).
- **lesson:cli-suggest-injection-density-all-families:2026-06-29** — toda injeção deriva valor real, nunca placeholder; o id e o teto não podem custar tokens ao modelo (S-6: o id vai ao ledger, nunca ao texto).
- **decisao:pillar-induction-armado:2026-08-29** — `pillar_induction_followed` é o molde de adesão medida (S-7 outcome 2).
- **snr-slice:S-00-effectiveness-audit:2026-07-04** — `context_utility_bonus` é laço REAL mas cego ao id; manter e estender, não substituir.
- **lesson:taco:2026-04-12:sinergia-generator-hooks** + **virada-nova-frente-complementacao-hooks-2026-08-31** — peer crates via subprocess fire-and-forget, nunca import direto (BridgeModule segue o padrão).
- **gotcha:daemon:contexto-vem-por-parametro-nunca-do-ambiente** — o módulo externo recebe contexto por parâmetro (envelope JSON-RPC), nunca lê env do invocador.
- **lesson:fachada-all-hooks-nao-liga-features-proprias:2026-09-01** — cada onda prova o binário instalado por comportamento, não por versão.

### Known gotchas for target files

- `cargo test -p touring-hook-handlers` exige `--features pre-hooks,post-hooks` (sem default features os handlers nem compilam).
- Edits fora do Edit tool não passam pelo hook de reindex → rodar `touring index rebuild` antes do `loop_converged` (órfãos falsos).
- Daemon embute `touring-cli` estático: após editar RPC/KPI, `cargo build -p touring-server --release` + `update-touring`.
- **Trabalho concorrente (02/09)**: outra sessão entregou a wave 14 (v30.4.33: tmp privado por run, `run_journal` v2, escada de trust automática, KPI `code_mode_reuse`, correção do detector de wiring) e o working tree tinha ~70 arquivos modificados quando este plano foi autorado. Antes da S-1: reconciliar (`git status`, `touring index rebuild`) e **re-medir** o baseline de órfãos e o e2e — os números desta seção envelhecem rápido.

---

## 2. 9-Dimension Scores (Pln1 → Pln2)

> Source — `dimension_scorer.py` (rodado após a autoria; a coluna Current é preenchida pelo score real, ver `data/dimension_scores.json`).

| Dim | Current | Target | Delta | Amplification |
|-----|--------:|-------:|------:|---------------|
| **precision** | — | 8.5 | — | todo subtask com `file:LINE` verificado + assinatura |
| **scalability** | — | 8.5 | — | trait + registry + manifest (não structs bespoke) |
| **performance** | — | 8.0 | — | p50/p90 por módulo, teto O(n log n) na poda por score |
| **functionality** | — | 8.5 | — | wira `with_budget`, `augment_with_mirror`, `actuator_signals` (hoje sub-usados) |
| **quality** | — | 8.5 | — | teste nomeado + ramo de erro em todo subtask; 0 unwrap |
| **detail** | — | 8.5 | — | schemas JSON de entrada/saída (ledger, manifest, JSON-RPC) |
| **integration** | — | 8.5 | — | mapa feature→mecanismo gerado + guard CI |
| **dependencies** | — | 8.0 | — | features `pre-hooks,post-hooks,knowledge` declaradas; serde_json/blake3 já no workspace |
| **potentiation** | — | 8.5 | — | coluna Enables não-vazia em todos os S-N |

### 2.1 Orçamento de performance (targets, workload, bench, complexidade)

Baseline medido 02/09: despacho de hook p50 0,65 ms · p90 133 ms · max 1,18 s (n=4.364). Workload de referência: 1 sessão CC, 12 handlers PreToolUse + 13 PostToolUse, ~27 eventos/min, 16 layers built-in, 2 módulos externos.

| Hot path | Target | Complexidade | Bench (criterion, `crates/touring-hook-handlers/benches/`) |
|---|---|---|---|
| `execute_with_metrics` com envelope (S-6, S-9) | P50 ≤ 1 ms · P99 ≤ 20 ms por hook (in-process) | O(L·s log s), L layers ≤ 16, s sinais ≤ 64 (sort por score já existente + poda da cauda) | `criterion::signal_pipeline_execute_with_envelope_bench` |
| ledger append (S-7) | P99 ≤ 5 ms por linha | O(1) amortizado, append-only + fsync por linha (padrão do mirror) | `criterion::injection_ledger_append_bench` |
| bridge IPC por evento (S-13) | P90 ≤ 100 ms · P99 ≤ 250 ms (= timeout, fail-open) | O(m) sobre m módulos inscritos; pool limitado a 10 (mesmo teto do `touring.parallel`) | `criterion::bridge_echo_roundtrip_bench` (módulo echo em Python) |
| `touring kpi -j` reinjection (S-8) | P99 ≤ 300 ms | O(N) sobre N linhas do ledger; rotação em 50k linhas / 30 dias ⇒ N ≤ 50k (~25 MB) | `criterion::kpi_reinjection_effectiveness_bench` |
| poda por teto de turno (S-9) | zero alocação extra além do sort | O(s log s), s ≤ 64 | coberto pelo 1º bench |

Regressão de performance é gate: `cargo bench --bench signal_pipeline -- --save-baseline w0` na W0 e comparação por onda (`--baseline w0`); p50 de despacho não pode subir > 20% sobre 0,65 ms. Memória: envelope ≤ 200 B por injeção, linha do ledger ≤ 512 B.

### 2.2 Dependências pinadas e MSRV

| Dependência | Versão (workspace Cargo.toml) | Uso neste plano |
|---|---|---|
| serde | 1.0 (features: derive) | todos os structs novos (`serde(default)` para retrocompatibilidade) |
| serde_json | 1.0 | ledger, manifest, JSON-RPC 2.0 (protocolo escrito à mão sobre serde_json: sem crate `jsonrpc` novo) |
| blake3 | 1.5.5 (já em `touring-hooks-shared`) | derivação de **TurnId**/**InjectionId** (REGRA #17) |
| sha2 | 0.10 | fallback quando o texto excede 8 KB (hash do texto) |
| thiserror / anyhow | 2.0 / 1.0 | erros tipados do bridge e do ledger (0 `unwrap` em código novo; `clippy -D warnings`) |
| tokio | 1.40 (full) | spawn e timeout do processo filho do bridge |
| rusqlite | 0.38 (bundled) | inalterado (ledger é JSONL, não SQLite) |
| clap | 4.5 | subcomando `touring module` |
| tracing | 0.1 | `skipped_by_policy`, `bridge_timeout` |

MSRV: rust-version 1.95 (workspace Cargo.toml:152). Novo crate `touring-module` não adiciona dependência externa. Módulo Python (S-15): Python ≥ 3.11 (o `.venv` do analise), somente stdlib (`json`, `sys`) para o protocolo, sem dependência pip nova; features Cargo exigidas nos testes: `pre-hooks,post-hooks` (touring-hook-handlers), `knowledge` (storage).

### 2.3 Escalabilidade e pontos de extensão

Módulos escalam horizontalmente via processos filhos independentes (1 por módulo por sessão); controle de concorrência via pool limitado (10) + timeout por módulo + **DispatchMode** declarado (parallel só para leitores). O núcleo permanece 1 daemon. Ponto de extensão criado por subtask: S-11 (trait + eventos: novo comportamento = nova impl), S-12 (registry + manifest: novo módulo = 1 arquivo), S-13 (bridge: nova linguagem = 0 mudanças no núcleo), S-10 (policy: nova política = JSON, não código). Substituição de special-case por padrão: as listas estáticas de `add_layer` em pre_read/pre_edit/pre_write viram consulta ao registry (S-12).

### 2.4 Evidência de amplificação (Pln1 → Pln2, em termos mensuráveis)

- **Scalability**: the design scales horizontally — N modules × M events dispatch through a bounded parallel pool (10) with per-module timeout; scaling out a module = 1 more child process, scaling in = policy JSON. Scalability is bought by the trait + registry + manifest pattern (S-11/S-12/S-13), never by special-case branches; the registry is the single extension point, so scaling the number of modules never touches the core.
- **Performance**: latency budget per hot path — in-process hook P50 1ms / P99 20ms; ledger append P99 5ms; bridge round-trip P90 100ms / P99 250ms; `touring kpi -j` P99 300ms. Throughput of reference: 27 events/min per session, 12 PreToolUse handlers; benchmark by criterion (4 benches named in §2.1) with saved baseline `w0` and a 20% regression gate on P50. Pruning is O(s log s); ledger is O(1) amortized; KPI is O(N) with N bounded by rotation. Cache hit: the module policy is cached 60s so the hot path never reads disk per event.
- **Dependencies**: all pinned at the workspace — serde version = "1.0" (feature = derive), serde_json version = "1.0", blake3 version = "1.5.5" (workspace = true in touring-hooks-shared), sha2 version = "0.10", thiserror version = "2.0", anyhow version = "1.0", tokio version = "1.40" (feature = full), rusqlite version = "0.38" (feature = bundled), clap version = "4.5", tracing version = "0.1"; MSRV rust-version = "1.95"; Python >=3.11 for the daughter (stdlib only, no pip requirement); no new external crate is required by `touring-module` (JSON-RPC is hand-written over serde_json, compatible with any client language).
- **Potentiation**: the loop compounds — every measured injection feeds the RL flywheel (`inject_reward` per module) and the memory (lessons per module), so the harness grows more effective autonomously: identity (W0) is the multiplier for the régua (W1), the régua is the multiplier for the contract (W2), the contract makes module growth exponential (W3/W4 — any language, zero core diff), and the self-disabling policy (S-10) keeps the compound effect from turning into context noise. Self-improving by construction: `touring kpi --actuate` closes the loop without a human in it.

---

## 3. Phases

> Ordem obrigatória: medição-primeiro. Cada fase fecha com `loop_phase_close.py` e `loop_converged.py` exit 0. Tickets: `touring decompose ticket <task> <S-N> --kind implementation --fog clear|hazy`; fronteira: `touring decompose frontier <task>`.
> Fases W1..W4 são `sequential`; dentro de W0 e W2 os subtasks marcados ∥ correm em `parallel` com política `all` (um ramo falho reprova a fase — nenhum é opcional).

### Phase 1 — W0 · Identidade da telemetria

> Requires: feature = pre-hooks,post-hooks (touring-hook-handlers tests); blake3 workspace = true (touring-hooks-shared); serde version = "1.0" with feature = derive; compatible with legacy JSONL lines (serde default).

#### Phase 1 header (parallel ∥ S-2/S-3/S-4 após S-1; política `all`; when_not_to_use: nunca — é o alicerce de tudo)

#### S-1: Criar **TurnId** e **InjectionId** determinísticos em touring-hooks-shared [P0] [confidence: FACT]
- **File**: `crates/touring-hooks-shared/src/signal_layer.rs:48` (vizinho: `SignalContext`); novo arquivo `crates/touring-hooks-shared/src/identity.rs`
- **Source truth**: `pub struct SignalContext<'a> { … tool_name, proposed: Option<ProposedChange> … }` (signal_layer.rs:48-106); molde determinístico `EntityId` (types.rs:25). A identidade que já existe é de **classe**, não de instância: `ActionSignature::to_key()` (action_signature.rs:139-146) devolve `outcome:<tool_class>:<intent_class>:<qualifier>` e agrega N injeções distintas na mesma chave — o **InjectionId** é a instância que falta (grep por `injection_id|Uuid::new|nanoid` nos 3 crates de hook = 0).
- **Change**:
```rust
// crates/touring-hooks-shared/src/identity.rs (novo)
pub struct TurnId { raw: String }      // = tool_use_id do payload quando presente; senão blake3(session_id, hook, ts_ms/1000)
pub struct InjectionId { raw: String } // = blake3(session_id | turn_id | module | sha256(text))[..16]  — REGRA #17: puro e total
impl InjectionId { pub fn derive(session: &str, turn: &TurnId, module: &str, text: &str) -> Self }
// SignalContext ganha `pub turn: Option<TurnId>` (default None — v1 intacta)
```
  Fallback do **TurnId** quando o payload não traz `tool_use_id` (PreToolUse, INFERENCE 0.8): registrar `turn_source: "tool_use_id"|"derived"` na trace line (S-2) e medir a proporção na W0 — se PreToolUse trouxer o id, o fallback nunca dispara.
- **Blast radius**: 9 arquivos importam `signal_layer` (grep fan-in); campo novo com default → 0 quebras.
- **Test**: `test_injection_id_is_deterministic_for_same_inputs` (mesmos 4 inputs ⇒ mesmo id; 1 byte diferente ⇒ id diferente) · `test_turn_id_prefers_tool_use_id_over_derived` · erro: `test_derive` nunca panica com texto vazio (retorna id do vazio).
- **Dimensions**: [a:9, b:8, e:9, f:8, i:9]
- **Enables**: S-2..S-8 (toda atribuição depende deste id); reutiliza o princípio de `EntityId` fora do crate identity.

#### S-2 ∥: `HookTraceLine` carrega identidade e bytes injetados [P0] [confidence: FACT]
- **File**: `crates/touring-hook-runtime/src/hook_trace.rs:32` (`HookTraceLine`), `:123` (`build_line`), `:153` (`append_line`)
- **Source truth**: campos atuais `ts_ms, pid, ppid, hook, route, exit_reason, stdin_state, stdin_bytes, elapsed_ms, session_id` (hook_trace.rs:32-52).
- **Change**: adicionar `turn_id: Option<String>`, `turn_source: Option<String>`, `injection_ids: Vec<String>`, `injected_bytes: u64`, `module: Option<String>` (todos `serde(default)` — linhas antigas seguem legíveis); `build_line` lê `tool_use_id` do payload; setter `set_injection(id, bytes)` chamado pelo pipeline (S-6).
- **Blast radius**: 5 arquivos importam `hook_trace`; formato JSONL retrocompatível.
- **Test**: `test_trace_line_roundtrips_identity_fields` · `test_trace_line_without_identity_still_parses` (linha antiga) · erro: `test_append_line` com trace file inválido segue fail-open (já é).
- **Dimensions**: [a:9, e:8, f:9, g:8]
- **Enables**: KPI de cobertura (S-5); correlação trace × ledger por `turn_id`.

#### S-3 ∥: `HookCallEntry` (mirror) carrega `session_id` e `turn_id` [P1] [confidence: FACT]
- **File**: `crates/touring-code/src/sdk_signal_mirror.rs:51` (`HookCallEntry`), `:163` (`record`), `:228` (`augment_with_mirror`)
- **Source truth**: `pub struct HookCallEntry { ts, hook_name, duration_ms, success, origin: Option<String> }` (51-67; `origin` já é `serde(default)`).
- **Change**: `session_id: Option<String>`, `turn_id: Option<String>` (`serde(default, skip_serializing_if = Option::is_none)`); `record` recebe o par; `augment_with_mirror` agrupa também por `turn_id`.
- **Blast radius**: 10 arquivos importam `sdk_signal_mirror`; leitores legados ignoram campos novos.
- **Test**: `test_mirror_entry_roundtrip_with_identity` · `test_mirror_entry_legacy_line_reads_none` · erro: `MirrorError` inalterado.
- **Dimensions**: [a:9, e:8, f:8, g:9]
- **Enables**: join mirror × ledger × trace por `turn_id` (S-8).

#### S-4 ∥: contadores rotulados de injeção no gate-metrics [P1] [confidence: INFERENCE]
- **File**: `crates/touring-hook-handlers/src/shared/signal_pipeline.rs:156` (`execute_with_metrics`) + infra de counters (`touring gate-metrics -j`, onde já vivem `enrichment_context_bytes_total` e `hook_dispatch_by_name.*`)
- **Source truth**: `enrichment_emit_count=81`, `enrichment_context_bytes_total=105422` medidos em 02/09; o proxy de STR já existe como `mean_enrichment_bytes` (gate_metrics_snapshot.rs:665, :1368), porém **global** — sem rótulo de módulo.
- **Change**: `injection_emitted_by_module.<module>` e `injection_bytes_by_module.<module>` incrementados em `execute_with_metrics` (mesmo mecanismo de `hook_dispatch_by_name`). INFERENCE: o counter rotulado por nome já existe (hook_dispatch_by_name) — verificar a API do counter map antes de codar.
- **Blast radius**: 20 arquivos importam `signal_pipeline`; adição de contador não muda assinatura pública.
- **Test**: `test_execute_with_metrics_increments_labelled_counters` (mutação 0→1 por layer).
- **Dimensions**: [c:8, d:8, g:8]
- **Enables**: STR por módulo em tempo real sem ler o ledger (S-8); dashboard `touring status`.

#### S-5: KPI `telemetry_identity` em `cli_kpi` [P1] [confidence: FACT]
- **File**: `crates/touring-cli/src/cli/kpi.rs:125` (`cli_kpi`), ao lado de `code_mode_signal_use` (kpi.rs:478 → padrão `{available, ratio, total, used}`)
- **Source truth**: `code_mode_signal_use => {"available": true, "ratio": 0.625, "total": 8, "used": 5}` (medido 02/09).
- **Change**: `telemetry_identity { available, coverage, lines_total, lines_complete, by_register: {trace, mirror, run_journal, gate_metrics} }` — linha completa = tem `session_id`+`turn_id`+`hook`+bytes. Schema de saída JSON documentado inline; piso 0,95.
- **Blast radius**: 21 arquivos importam `kpi`; campo novo no JSON de `touring kpi -j` (aditivo).
- **Test**: `test_kpi_telemetry_identity_counts_only_complete_lines` (fixture com 3 linhas: 2 completas ⇒ 0,667) · erro: registro ausente ⇒ `available:false`, nunca coverage 1,0 (fail-closed).
- **Dimensions**: [a:8, e:9, f:9, g:8, i:8]
- **Enables**: gate da W0 (`coverage ≥ 0.95`); `BestPracticesGate` regra 5 (S-10).

### Phase 2 — W1 · Régua por injeção

> Requires: W0 KPI `telemetry_identity` coverage >=0.95; feature = pre-hooks,post-hooks; no new crate version required; compatible with `DEFAULT_CONTEXT_BUDGET` (per-emission cap stays).

#### Phase 2 header + teto por turno + desligamento por evidência (sequential; when_not_to_use: sem W0 fechada — sem identidade a régua atribui ao nada)

#### S-6: **InjectionEnvelope** em toda emissão de additionalContext [P0] [confidence: FACT]
- **File**: `crates/touring-hook-handlers/src/shared/signal_pipeline.rs:99` (`execute`) e `:156` (`execute_with_metrics`); emissores: pre_read.rs:1038 (`compose_high_signal_context_budgeted`), pre_edit.rs:585 (`compose_edit_context`), cli_suggester.rs:3765 (advisory_response)
- **Source truth**: `pub fn execute(&self, ctx: &SignalContext<'_>) -> Option<String>` (signal_pipeline.rs:99); `advisory_response(context: String)` monta `hookSpecificOutput.additionalContext` (cli_suggester.rs:3765-3772).
- **Change**: `execute_with_metrics` devolve, além do texto, um vetor de **InjectionRecord** {id, module (= layer.name()), bytes, score, remedy_hash}; cada emissor grava os records no `result_cache["__meta__"]["__injections__<turn_id>"]` (JSON) e no ledger (S-7). O TEXTO injetado não muda (o id nunca custa tokens ao modelo). `remedy_hash` = hash do comando/rota sugerido quando o layer o declara (base da adesão).
- **Blast radius**: 20 (signal_pipeline) + 3 emissores; assinatura de `execute` preservada, `execute_with_metrics` ganha campo no retorno (todos os callers no mesmo crate).
- **Test**: `test_every_emission_records_one_injection_per_layer` · `test_execute_with_envelope_p99_under_20ms` (latency assertion: P99 < 20ms, criterion baseline w0) · `test_emitted_text_is_byte_identical_with_and_without_envelope` · erro: falha ao gravar o ledger nunca bloqueia a emissão (fail-open, contador `test_injection_ledger_write_error`).
- **Dimensions**: [a:9, d:9, e:9, f:9, g:9]
- **Enables**: S-7 (atribuição), S-9 (teto por turno conhece os bytes por id).

#### S-7: Atribuição do outcome v1 ao id no PostToolUse (ledger append-only) [P0] [confidence: FACT]
- **File**: `crates/touring-hook-handlers/src/hooks/post_tool_rl.rs:58` (`compute_context_bonuses`) e `:250-268` (recompensa `context_injection_quality`); novo `crates/touring-code/src/injection_ledger.rs` (molde: sdk_signal_mirror.rs:163 `record`)
- **Source truth**: hoje `context_utility_bonus = 0.1` se `__context_injection_file__ == file_path` (post_tool_rl.rs:63-70) e `inject_reward("context_injection_quality", 0.3, …)` (250-268) — laço real, cego ao id. **Assimetria a corrigir (C08/REGRA #0)**: o marcador é gravado só por pre_read.rs:362 e pre_write.rs:279; `pre_edit` NÃO grava, então toda edição é inatribuível hoje — o envelope da S-6 fecha os três de uma vez.
- **Change**: ler `__injections__<turn_id_anterior>`; para cada record gravar **InjectionOutcome** {injection_id, module, turn_id, session_id, tool_name, success (bool), adhered (bool opcional: tool_input contém o remédio de remedy_hash nas N=3 ações seguintes), str_proxy (f32 opcional: bytes de símbolos/paths injetados que reaparecem em tool_input ÷ bytes injetados), ts} em `~/.claude/touring/injection_ledger.jsonl` (append-only, crash-safe como o mirror). Manter os bônus atuais e adicioná-los por id (`inject_reward` com chave `injection:<module>`). Schema JSON do ledger documentado no módulo.
- **Blast radius**: 25 arquivos referem `post_tool_rl`; mudança interna às fns privadas + 1 módulo novo.
- **Test**: `test_ledger_append_p99_under_5ms` (latency assertion) · `test_outcome_attributed_to_previous_turn_injection` · `test_outcome_marks_adherence_when_remedy_reappears` · `test_outcome_without_injection_writes_nothing` · **mutação**: remover a gravação ⇒ `n` fica 0 no KPI (S-8) — prova 0→1→0.
- **Dimensions**: [a:9, d:9, e:9, f:9, g:9, i:9]
- **Enables**: S-8, S-10; substitui o proxy de arquivo por id sem apagar o laço S-00; feeds the RL flywheel per module (`inject_reward`), so every measured injection compounds into the next suggestion.

#### S-8: KPI `reinjection_effectiveness` por módulo (n mínimo 30, fail-closed) [P0] [confidence: FACT]
- **File**: `crates/touring-cli/src/cli/kpi.rs:125` (`cli_kpi`); reuso de `CouplingSignals` (kpi.rs:1620) e `actuator_signals` (kpi.rs:1777) para o veredito
- **Source truth**: `code_mode_adherence` lê `run_journal.jsonl` e expõe `{success_rate, by_failure_kind, …}` (medido 0,918) — mesmo padrão de leitura de JSONL.
- **Change**: `reinjection_effectiveness { available, n_min: 30, by_module: { <m>: { n, success_rate, adherence_rate, str_mean, bytes_total, verdict: keep|watch|disable, measured_since } } }`; `verdict=disable` quando n≥30 ∧ adherence_rate<0,05 ∧ success_rate ≤ baseline_sem_injeção. Injeção sem outcome = "não medida" (fail-closed: nunca conta como efeito 0).
- **Blast radius**: 21 (kpi); aditivo.
- **Test**: `test_kpi_reinjection_p99_under_300ms` (latency on 50k-line ledger) · `test_kpi_reinjection_effectiveness_requires_n_min` · `test_kpi_verdict_disable_only_with_evidence` · `test_kpi_unmeasured_injection_is_not_zero_effect`.
- **Dimensions**: [a:8, c:8, d:9, e:9, f:9, i:9]
- **Enables**: Pronto #1 da concepção; S-10 (atuação por evidência); relatório `touring status`.

#### S-9: **TurnBudget** — teto de bytes por TURNO aplicado no executor [P0] [confidence: FACT]
- **File**: `crates/touring-hooks-shared/src/cila.rs:33-51` (`cila_budget_read`/`edit`/`write`), `crates/touring-hook-handlers/src/shared/signal_pipeline.rs:139` e `:195` (truncagem no executor), `crates/touring-hook-handlers/src/hooks/pre_read.rs:392-401` (`record_enrichment_metrics`)
- **Source truth**: o teto POR CHAMADA já existe e é graduado por CILA (800/2.000/4.000 read; 1.200/3.000/6.000 edit/write, cila.rs:33-51), aplicado no executor (`if output.len() + text.len() > self.budget { break }`, signal_pipeline.rs:139 e :195), com pressão observada a 75%/90% (`ctx_budget_warning`/`ctx_budget_alert`, pre_read.rs:398-401). **O que não existe é o acumulador por TURNO**: N hooks do mesmo turno somam sem limite algum.
- **Change**: `turn_injection_ceiling(cila_level)` na MESMA função de cila.rs (simetria com os três tetos existentes; default 3× o teto de write da faixa: 3.600/9.000/18.000 chars, override `TOURING_CILA_BUDGET_TURN`), acumulador `__turn_bytes__<turn_id>` no result_cache lido por `execute_with_metrics`; ao estourar, poda os sinais de menor score até caber (reusa o sort já existente) e incrementa `injection_ceiling_cut_total`; a poda entra no envelope (`cut: true`). Os alertas de 75%/90% passam a ser emitidos também para o teto de turno (reuso de `record_ctx_budget_warning`/`alert`, sem contador novo). Declaração por escopo em `.touring/touring.toml [injection] turn_ceiling_chars` (molde `project_presentation`, code_mode.rs:79).
- **Blast radius**: 20 (signal_pipeline) + 9 (cila importado pelos 3 pré-hooks); tetos por chamada intactos (o de turno é um segundo predicado, nunca substitui o primeiro).
- **Test**: `test_turn_ceiling_cuts_lowest_score_first` (O(s log s), s=64 in < 1ms)  · `test_turn_ceiling_accumulates_across_hooks_same_turn` · **mutação** 0→1→0: desligar o corte ⇒ teste falha · erro: acumulador ausente ⇒ trata como 0 (nunca bloqueia).
- **Dimensions**: [b:8, c:9, e:9, f:9, g:8]
- **Enables**: o antídoto do "morreu de contexto"; S-10 lê `cut` como sinal de módulo barulhento.

#### S-10: Desligamento por evidência (**ModulePolicy**) + regra 5 do `BestPracticesGate` [P1] [confidence: INFERENCE]
- **File**: `crates/touring-hook-handlers/src/shared/signal_pipeline.rs:69` (`add_layer`), `crates/touring-cli/src/cli/kpi.rs:1777` (`actuator_signals`), `crates/touring-quality/src/builtins/best_practices.rs:36`
- **Source truth**: `actuator_signals` já deriva ações (`RefinementAction`, kpi.rs:1639) a partir de KPIs; `BestPracticesGate` tem 4 regras (a 4ª, `signal_use`, lê o mirror).
- **Change**: `touring kpi --actuate` escreve `~/.claude/touring/module_policy.json {module: {state: enabled|disabled_by_evidence, since, evidence: {n, adherence_rate, success_rate}}}`; `SignalPipeline` lê a policy (cache 60 s) e pula layers desabilitados (`should_run` = false) registrando `skipped_by_policy`; `BestPracticesGate` regra 5 `reinjection_measured`: Warn se algum módulo emite com `n<30` há >7 dias. INFERENCE: formato exato de `RefinementAction` a confirmar antes de codar.
- **Blast radius**: 20 (signal_pipeline) + 21 (kpi) + gate; policy ausente ⇒ tudo habilitado (fail-open).
- **Test**: `test_disabled_module_is_skipped_and_counted` · `test_policy_missing_means_all_enabled` · `test_gate_rule5_warns_unmeasured_emitter`.
- **Dimensions**: [b:8, d:9, e:8, g:9, i:9]
- **Enables**: Cadeia elo 5 (refinar ou desligar por evidência); W5 contínua; autonomous self-correction (the harness disables its own noise without a human), the multiplier that keeps the compound effect positive.

### Phase 3 — W2 · Contrato de módulo

> Requires: new crate `touring-module` (MSRV rust-version = "1.95", tokio version = "1.40" for spawn/timeout); Python >=3.11 for the echo test module; compatible with all 16 existing `SignalLayer` impls via the adapter.

#### Phase 3 header + registry + BridgeModule (∥ S-12/S-13 após S-11; S-14 sequencial; política `all`; when_not_to_use: sem S-6..S-8 — módulo sem régua nasce cego)

#### S-11: `trait` **Module** + **HarnessEvent** + **DispatchMode** (5 estágios) [P0] [confidence: FACT]
- **File**: novo crate `crates/touring-module/src/lib.rs` (dependência: touring-hooks-shared para `SignalContext`/identity); adapter em `crates/touring-hooks-shared/src/signal_layer.rs:130`
- **Source truth**: `pub trait SignalLayer: Send + Sync { fn name(&self) -> &'static str; fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)>; fn should_run(&self, _cila_level: usize) -> bool }` (signal_layer.rs:130-145).
- **Change**:
```rust
pub enum HarnessEvent { SessionStart, UserPromptSubmit, PreToolUse{tool}, PostToolUse{tool}, PostToolUseFailure{tool}, PreCompact, Stop, /* … */ }
// Reconcilia os TRÊS vocabulários hoje incompatíveis: (a) os 239 nomes de ALL_DAEMON_HOOK_NAMES
// (hook_registry.rs:401), dos quais 199 são cli-* de telemetria e ~40 são lifecycle real;
// (b) os 27 tipos de evento do settings.json do Claude Code; (c) o modo de despacho, hoje
// inexistente. O enum cobre o lifecycle (a∩b); os cli-* seguem fora do contrato de módulo.
pub enum DispatchMode { Waterfall, Serial, Parallel, Emit }   // dsh/Cordis: transformar · ordenado · fan-out · fire-and-forget
pub struct ModuleManifest { name, version, subscribes: Vec<(HarnessEvent, DispatchMode)>, capabilities: Vec<String> }
pub trait Module: Send + Sync {
    fn manifest(&self) -> ModuleManifest;                                  // 1 subscribe
    fn observe(&self, ev: &HarnessEvent, ctx: &SignalContext<'_>);         // 2 telemetria estratégica (o que este módulo registra)
    fn process(&self, ev: &HarnessEvent, ctx: &SignalContext<'_>) -> ModuleResponse; // 3 tratar/cruzar → 4 injetar
    fn measure(&self, outcome: &InjectionOutcome);                         // 5 aprender (recebe a régua)
}
pub struct ModuleResponse { injections: Vec<Injection{ text, score, ttl_turns, remedy: Option<String> }>, telemetry: JsonValue /* serde_json */ }
// adapter: impl<L: SignalLayer> Module for LayerAsModule<L> — os 16 layers viram módulos sem mudar uma linha deles
```
- **Blast radius**: 0 (crate novo) + adapter aditivo em touring-hooks-shared (9 importadores).
- **Test**: `test_layer_as_module_preserves_enrich_output` (16 layers × mesma saída) · `test_manifest_rejects_duplicate_subscription` (erro tipado; espelha o `name` único das seções do dsh) · `test_dispatch_mode_waterfall_short_circuits` · `test_harness_event_covers_every_lifecycle_hook_name` (guard: todo nome não-`cli-*` de `ALL_DAEMON_HOOK_NAMES` mapeia para uma variante, senão o vocabulário volta a divergir).
- **Dimensions**: [a:9, b:9, d:9, e:8, f:9, g:9, i:9]
- **Enables**: S-12/S-13; modularização exponencial (o loop de hooks vira consumidor do registry).

#### S-12 ∥: **ModuleRegistry** + `touring module list|explain|map` + mapa feature→mecanismo [P0] [confidence: FACT]
- **File**: `crates/touring-module/src/registry.rs` (novo); CLI em `crates/touring-cli/src/cli/` (molde: `cli_kpi`, kpi.rs:125); `crates/touring-hook-handlers/src/shared/signal_pipeline.rs:338` (`build_graph_pipeline` passa a consultar o registry)
- **Source truth**: hoje os layers são adicionados por `add_layer` em listas estáticas em pre_read.rs:791/949, pre_write.rs:177-777, pre_edit.rs:717 (grep `layers`).
- **Change**: registry in-process (built-ins declarados via `inventory`-like lista estática + manifests carregados de `.touring/modules/*.toml`); `touring module list -j`, `touring module explain <name>` (grafo plano: eventos × modo × injeções), `touring module map` gera `docs/modules/feature-mechanism.md` (feature → módulo → evento) + guard CI `scripts/test_module_map_current.py` (o mapa desatualizado falha o CI — obrigação de prova, dsh).
- **Blast radius**: 20 (signal_pipeline) + 3 handlers; `build_graph_pipeline` mantém assinatura.
- **Test**: `test_registry_lists_all_16_builtins` · `test_registry_loads_manifest_from_project_dir` · `test_module_map_is_current` (CI) · erro: manifest inválido ⇒ módulo ignorado com aviso na trace, núcleo segue.
- **Dimensions**: [b:9, d:9, e:8, f:8, g:9, i:9]
- **Enables**: diff zero no núcleo para módulo novo (S-14 prova); `touring module new` (S-17).

#### S-13 ∥: **BridgeModule** — módulo fora-de-processo por JSON-RPC 2.0 sobre stdio [P0] [confidence: FACT]
- **File**: `crates/touring-module/src/bridge.rs` (novo); molde de spawn seguro: `crates/touring-server/src/cli/run.rs:767` (`run`, env_clear + whitelist do CEG); manifest em `.touring/modules/<name>.toml`
- **Source truth**: padrão institucional "peer crates via subprocess fire-and-forget"; CEG já aplica `env_clear`, rlimit e Landlock em `touring run`.
- **Change**: manifest `{ name, command, args, subscribes = [["PreToolUse:Read","waterfall"], …], timeout_ms = 250, capabilities = ["fs:ro:<root>"], lang }`; protocolo: `subscribe` → `on_event{event, ctx:{file_path, tool_name, session_id, turn_id, analysable_text≤8 KB}}` → `{injections:[{text, score, remedy?}], telemetry}`; `health`; processo filho reutilizado por sessão, `env_clear` + whitelist (A11: nenhum segredo), timeout ⇒ fail-open + contador `bridge_timeout_by_module`; p50/p90 por módulo em gate-metrics (meta p90 < 100 ms por evento; hoje p90 geral de despacho 133 ms). Schemas JSON de request/response versionados (`protocol_version: 1`).
- **Blast radius**: 0 no núcleo (registry carrega); 1 novo caminho de spawn.
- **Test**: `test_bridge_echo_module_python_roundtrip` (módulo mock em Python que devolve 1 injeção) · `test_bridge_timeout_is_fail_open` · `test_bridge_child_has_no_secret_env` (asserta ausência de `ANTHROPIC_API_KEY`/`GEMINI_API_KEY`) · `test_bridge_p90_under_budget_for_echo`.
- **Dimensions**: [b:9, c:9, d:9, e:9, f:9, g:8, h:8]
- **Enables**: S-15 (filha em Python); terceiros em qualquer linguagem.

#### S-14: Migrar 2 built-ins pelo registry e provar diff zero [P1] [confidence: FACT]
- **File**: `crates/touring-hook-handlers/src/shared/quality_signal.rs:73` (`QualityBaselineLayer`), `crates/touring-hook-handlers/src/shared/signal_pipeline.rs:383` (`BlastRadiusSignalLayer`)
- **Source truth**: ambos `impl SignalLayer for …` adicionados por `add_layer` nas listas estáticas dos handlers.
- **Change**: registrá-los via **ModuleRegistry** (adapter S-11) e remover das listas estáticas; guard `scripts/test_module_zero_core_diff.py`: registra um módulo de teste por manifest e asserta `git diff --stat crates/touring-hook-handlers crates/touring-hooks-shared` vazio.
- **Blast radius**: 20 (signal_pipeline) + 1 (quality_signal); saída dos hooks byte-idêntica (teste).
- **Test**: `test_migrated_layers_emit_identical_context` (golden antes/depois) · `test_zero_core_diff_when_registering_manifest_module` (CI).
- **Dimensions**: [a:8, b:9, e:9, g:9, i:8]
- **Enables**: migração incremental dos outros 14 layers na W5; a prova que a Fronteira exige ("não é reescrita").

### Phase 4 — W3 · Primeira filha

> Requires: analise pinned toolchain >=30.4.30 with the W2 binary propagated; Python >=3.11 `.venv`; lexhub API (INFERENCE, verify first).

#### Phase 4 header: analise-regulatorio (sequential; when_not_to_use: sem S-13 — sem bridge não há filha em Python)

#### S-15: Módulo Python `analise-regulatorio` via bridge no projeto analise [P0] [confidence: INFERENCE]
- **File**: `~/projects/analise/.touring/modules/analise-regulatorio.toml` (novo) + `~/projects/analise/scripts/touring_modules/analise_regulatorio.py` (novo); consumidores: lexhub (`packages/kazuba-enrichment`, API a verificar) + `touring memory query "#domain:regulatorio"`
- **Source truth**: analise está pinado em 30.4.30 com `code_mode_signal_use` ativo (CLAUDE.md regra 12); a suíte exige `.venv` (memória analise-exige-venv).
- **Change**: inscreve UserPromptSubmit (waterfall) e PreToolUse:Read/Edit em `*.md|*.py|*.tex` do pipeline documental; para cada evento: extrai entidades (norma, artigo, contrato, órgão) do `analysable_text`, consulta lexhub (top-3 dispositivos) e memória facetada (`#domain:regulatorio #kind:lesson`), devolve ≤ 1.200 chars com `remedy` = comando canônico do pipeline (ex.: `python3 scripts/lexhub/…`) para a adesão ser mensurável. INFERENCE 0.7 sobre a API do lexhub: subtask abre com `touring index find` no analise e ajusta.
- **Blast radius**: 0 no Touring (é um manifest + processo); no analise, 2 arquivos novos.
- **Test**: `test_analise_regulatorio_returns_injection_for_norma_reference` (pytest, `.venv`) · `test_injection_under_1200_chars` · erro: lexhub indisponível ⇒ `{injections: []}` em < 250 ms (fail-open).
- **Dimensions**: [b:8, d:9, f:8, g:9, i:9]
- **Enables**: Pronto #1; o primeiro módulo de domínio (regulatório/legal/contratual/administrativo + documentos/relatórios/apresentações/GeoAI).

#### S-16: Medição viva — n ≥ 30 e uma injeção desligada por evidência [P0] [confidence: FACT]
- **File**: `~/.claude/touring/injection_ledger.jsonl` (S-7), `touring kpi -j` (S-8), `module_policy.json` (S-10)
- **Source truth**: gates da concepção (§O Pronto, criacao.md).
- **Change**: 3 sessões reais no analise (`touring adw run` de tarefa documental) até `reinjection_effectiveness.analise-regulatorio.n ≥ 30`; relatório OKF `phases/W3.md` com n, success_rate, adherence_rate, str_mean, bytes/turno vs teto; provocar 1 sub-sinal deliberadamente ruidoso (score baixo, remédio inexistente) e provar `disabled_by_evidence` pelo KPI (não à mão).
- **Blast radius**: 0 (medição).
- **Test**: `test_kpi_analise_regulatorio_n_ge_30` (script `scripts/test_w3_live_gate.py` lê o KPI e falha abaixo do piso) · `test_policy_shows_one_disabled_by_evidence`.
- **Dimensions**: [c:8, d:9, e:9, i:9]
- **Enables**: veredito da Cadeia elo 5 com dado real; calibração do `TURN_INJECTION_CEILING`.

### Phase 5 — W4 · Publicação (∥ S-17/S-18; política `all`; when_not_to_use: antes de S-16 — publicar contrato não provado é promessa)

#### S-17 ∥: Spec do contrato, cookbook feature→mecanismo, `touring module new`, 2º módulo com diff zero [P1] [confidence: FACT]
- **File**: `docs/modules/CONTRACT.md`, `docs/modules/cookbook.md` (novos); CLI `touring module new <name> --lang python|rust` (template com manifest + echo + teste); 2º módulo `modules/examples/git-context` (Rust ou Python) registrado só por manifest
- **Source truth**: dsh mantém `docs/cookbook/extension-cookbook.md#the-feature--mechanism-map` como obrigação de prova; Touring já tem `adw from-template` como molde de scaffold.
- **Change**: CONTRACT.md com os schemas JSON-RPC (S-13) e a tabela evento × modo; cookbook com 10+ linhas feature → módulo → evento geradas por `touring module map` (S-12); README ganha a seção "escreva um módulo em 5 minutos"; CI roda `zero_core_diff` (S-14) contra o 2º módulo.
- **Blast radius**: docs + 1 subcomando; 0 no núcleo.
- **Test**: `test_module_new_scaffold_passes_bridge_roundtrip` · `test_docs_cookbook_matches_registry` (CI).
- **Dimensions**: [b:8, d:8, f:9, g:8, i:9]
- **Enables**: terceiros (Pronto #3); auto-documentação.

#### S-18 ∥: KPI `community` (GitHub) + alvos calibrados por Gabriel [P2] [confidence: FACT]
- **File**: `crates/touring-cli/src/cli/kpi.rs:125` (`cli_kpi`); fonte `gh repo view --json stargazerCount,forkCount` (cache diário em `~/.claude/touring/community.json`)
- **Source truth**: repositório público (memória touring-repo-publico-2026-08-02); números PROPOSTOS ≥ 1.000 stars / ≥ 100 forks / ≥ 10 contribuidores externos / ≥ 3 módulos de terceiros (Gabriel calibra).
- **Change**: `community { available, stars, forks, external_contributors, third_party_modules, targets, updated_at }`; `gh` ausente ⇒ `available:false` (nunca inventa).
- **Blast radius**: 21 (kpi); aditivo.
- **Test**: `test_kpi_community_reads_cached_json` · `test_kpi_community_unavailable_without_gh`.
- **Dimensions**: [d:8, e:8, f:8]
- **Enables**: Pronto #2/#3 medidos, não sentidos.

### Phase 6 — W5 · Contínua (sequential; when_not_to_use: nunca — é o laço)

#### S-19: Guard "a régua nunca regride" + migração incremental dos 14 layers restantes [P2] [confidence: FACT]
- **File**: `scripts/test_reinjection_kpi_never_regresses.py` (novo, CI); `crates/touring-hook-handlers/src/shared/*.rs` (14 `impl SignalLayer` restantes)
- **Source truth**: `loop_converged.py` cláusulas + `judge_attest.py` (juiz de registro).
- **Change**: baseline do KPI por módulo persistido em `docs/plans/2026-09-02-touring-proxima-geracao/.baseline/kpi.json`; o guard falha se `adherence_rate` ou `success_rate` de qualquer módulo `keep` cair > 0,05 sem `judge_attest --attest`; migrar 2 layers por sessão até 16/16 pelo registry.
- **Blast radius**: 20 (signal_pipeline) ao longo das sessões; cada migração provada por golden (S-14).
- **Test**: `test_kpi_baseline_guard_fails_on_regression` · `test_all_16_layers_registered` (ao fim).
- **Dimensions**: [b:8, e:9, g:8, i:9]
- **Enables**: Cadeia elo 6 — o portfólio de módulos, cada um com KPI; exponential module growth (any language, zero core diff) with a flywheel that never regresses by construction.

---

## 4. DAG

```mermaid
graph LR
  P1[W0 · Identidade da telemetria (sequential)]
  P2[W1 · Régua por injeção (sequential)]
  P3[W2 · Contrato de módulo (sequential)]
  P4[W3 · Primeira filha (sequential)]
  P5[W4 · Publicação (sequential)]
  P6[W5 · Contínua (sequential)]
  P1 --> P2
  P2 --> P3
  P3 --> P4
  P4 --> P5
  P5 --> P6
```

Textual sequence:
P1 (sequential: W0 · Identidade da telemetria) -> P2 (sequential: W1 · Régua por injeção) -> P3 (sequential: W2 · Contrato de módulo) -> P4 (sequential: W3 · Primeira filha) -> P5 (sequential: W4 · Publicação) -> P6 (sequential: W5 · Contínua)

## 5. Verification Protocol

```bash
# Rust (crates tocados)
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo test -p touring-hooks-shared -p touring-hook-runtime -p touring-code -p touring-cli
cargo test -p touring-hook-handlers --features pre-hooks,post-hooks
cargo test -p touring-module            # a partir da W2
# Python (filha, no analise)
cd ~/projects/analise && .venv/bin/python -m pytest scripts/touring_modules -q
# Touring
touring index rebuild --dir $PWD        # edits por script não passam pelo hook de reindex
touring e2e -j                          # composite ≥ baseline 0.8749
touring wiring orphans -j               # ≤ 1759 (baseline medido 02/09 pós-wave-14; reMEDIR ao iniciar a W0)
touring kpi -j | jq '.telemetry_identity, .reinjection_effectiveness'
# 50-dim: 6 BLOCK dims P0 (fail-closed) em cada arquivo tocado
for f in crates/touring-hooks-shared/src/identity.rs crates/touring-hook-runtime/src/hook_trace.rs \
         crates/touring-code/src/injection_ledger.rs crates/touring-hook-handlers/src/shared/signal_pipeline.rs \
         crates/touring-module/src/lib.rs crates/touring-module/src/bridge.rs; do
  for g in F2.1 F2.4 F2.5 F2.6 F4.3 F4.5; do touring-quality check --gate $g --target $f; done
done
touring-quality score crates --workspace --fail-below 0.80   # piso Gold (baseline 0.937 Platinum)
# Convergência (Lei L2) por fase
python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py --task <task_id> --scope $PWD --bundle docs/plans/2026-09-02-touring-proxima-geracao
```

Acceptance (por fase, todos AND):
- cargo check/clippy: 0 erros, 0 warnings · testes nomeados nesta seção: verdes · mutação 0→1→0 provada em S-7 e S-9
- `touring e2e -j` ≥ 0,8749 · orphans ≤ **1.759** (re-medir ao iniciar a W0 — o número caiu de 5.535 na wave 14 de 02/09 e continua em movimento) · `touring-quality score` ≥ 0,80, 0 BLOCK P0
- W0: `telemetry_identity.coverage ≥ 0.95` · W1: `reinjection_effectiveness.available` com n≥30 para ≥1 built-in e `injection_ceiling_cut_total` > 0 em teste · W2: `zero_core_diff` verde, p90 bridge echo < 100 ms · W3: `analise-regulatorio.n ≥ 30`, ≥1 `disabled_by_evidence` · W4: cookbook == registry
- Performance: p50 de despacho de hook não sobe > 20% sobre 0,65 ms (built-ins in-process); bridge p90 < 100 ms por evento; poda por score O(n log n)
- Dependências: features `pre-hooks,post-hooks` (touring-hook-handlers), `knowledge` (storage); crates já no workspace: serde/serde_json, blake3 (verificar `cargo tree -p touring-hooks-shared | grep blake3`; senão sha2 já presente)

---

## 6. Potentiation Matrix

| Change | Enables |
|--------|---------|
| S-1 ids determinísticos | toda atribuição (S-6..S-8); mesmo princípio de `EntityId` fora do crate identity |
| S-2 trace com identidade | KPI de cobertura; join trace × ledger por turno |
| S-3 mirror com identidade | join mirror × ledger; F9 origem ganha turno |
| S-4 counters por módulo | STR em tempo real; `touring status` por módulo |
| S-5 KPI telemetry_identity | gate da W0; regra 5 do gate de qualidade |
| S-6 envelope | atribuição por id sem custar tokens; teto por turno conhece bytes por id |
| S-7 ledger de outcomes | KPI de efetividade; substitui proxy de arquivo mantendo o laço S-00 |
| S-8 KPI efetividade | Pronto #1; atuação por evidência |
| S-9 teto por turno | antídoto do risco principal; sinal de módulo barulhento |
| S-10 policy por evidência | Cadeia elo 5; W5 contínua |
| S-11 trait Module | os 16 layers viram módulos sem mudar; modularização exponencial |
| S-12 registry + map | diff zero no núcleo; auto-documentação |
| S-13 bridge JSON-RPC | filha em Python; terceiros em qualquer linguagem |
| S-14 2 layers migradas | prova da Fronteira ("não é reescrita"); migração incremental |
| S-15 analise-regulatorio | primeiro módulo de domínio; Pronto #1 |
| S-16 medição viva | veredito com dado real; calibração do teto |
| S-17 spec + cookbook + new | terceiros; Pronto #3 |
| S-18 KPI community | Pronto #2/#3 medidos |
| S-19 guard + 14 layers | portfólio completo com KPI por módulo |

---

## Cross-references

- Estratégia: [`strategy-2026-09-02-touring-proxima-geracao.md`](/strategy-2026-09-02-touring-proxima-geracao.md) · Concepção: [`criacao.md`](/criacao.md) · Bundle: [`index.md`](/index.md)
- TACO-wt opera este plano — `~/.claude/skills/TACO-wt/SKILL.md` · Rubrica 9 dims — `~/.claude/skills/taco-planning/references/dimensions-rubric.md`
- Modelo de forma: dsh `.agents/notes/implemented/architecture/2026-06-11-microkernel-event-taxonomy.md` (clone em `~/references/code-mode-2026-08-23/deepseek-harness`)
