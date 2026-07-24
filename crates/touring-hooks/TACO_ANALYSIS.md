# TACO Analysis — `touring-hooks` Deep Dive
> Gerado em: 2026-03-26 | TACO Orchestrator N₂ v4.0 | Touring v22.0.0 (S0-S8)
>
> **STATUS PÓS-IMPLEMENTAÇÃO (v25.0.0):** 14/15 estratégias implementadas, 2.840→3.040 testes,
> SCHEMA_VERSION 4→5, auditoria E2E 14/14 aprovadas. Ver `IMPLEMENTATION_PLAN.md` para detalhes.

---

## 1. Estrutura Atual

**20.519 linhas · 37 módulos · 644 testes · 2 binários**

### File Tree

| Arquivo | Linhas | Responsabilidade |
|---|---|---|
| `knowledge.rs` | 2.477 | `FileKnowledgeDB` — SQLite WAL, gotchas, bash outcomes, edit history, gotcha decay |
| `prompt_enhance.rs` | 1.416 | Prompt enhancement pipeline — CILA injection, reasoning techniques |
| `runtime.rs` | 1.409 | `HookRuntime` god object — 19 campos, init SQLite + RL + AST |
| `shadow_v2.rs` | 1.374 | Copy-on-write file snapshots para safe edits (BranchFs) |
| `classifier.rs` | 900 | `IntentClassifier` — CILA L0-L6, weighted scoring, RegexSet |
| `post_edit.rs` | 834 | Post-edit tracking: edit history, symbol indexing incremental |
| `session_insights.rs` | 677 | Trend analysis, session metrics, insight generation |
| `pii.rs` | 662 | PII scanner — 15+ patterns, redaction |
| `ast_bridge.rs` | 634 | Bridge para `touring-ast` — symbol find/overview/edit |
| `pre_edit.rs` | 627 | Context injection para edits — blast radius, anti-patterns |
| `output_capture.rs` | 624 | Captura de output para `post-bash` |
| `aco_bridge.rs` | 616 | ACO tracker, quality assessment, `HookResultCache` |
| `dependency_cache.rs` | 612 | petgraph-backed call graph, Tarjan SCC, topological sort |
| `main.rs` | 560 | Entry point — dispatch, `DAEMON_HOOKS`, `try_daemon_request` |
| `daemon.rs` | 358 | Servidor daemon — accept loop, `dispatch_request` (CC=29) |
| `aco_wiring.rs` | 364 | `AcoWiringState` — bus + bridge + multi_obj + session_predictor |
| `pre_read.rs` | ~300 | Pre-read context injection — gotchas, dependents, bash failures |
| `pre_bash.rs` | ~250 | Pre-bash context — command history, failure patterns |
| `session_hooks.rs` | ~220 | session-start / session-stop handlers |
| `team_hooks.rs` | 253 | N1: teammate-idle, task-completed → ACO wiring |
| `ipc.rs` | 62 | `DaemonRequest/Response`, `daemon_socket_path()`, `daemon_lock_path()` |
| `daemon_main.rs` | 49 | Entry point do daemon binário |

### Struct HookRuntime — 19 campos

```rust
pub struct HookRuntime {
    pub knowledge: FileKnowledgeDB,           // SQLite WAL — sempre presente
    pub classifier: IntentClassifier,         // RegexSet stateless — sempre presente
    pub pii_scanner: PIIScanner,              // 15+ patterns — sempre presente
    pub project_root: PathBuf,                // sempre presente
    pub quality_assessment: Option<HookQualityAssessment>, // ACO metrics
    pub result_cache: HookResultCache,        // LRU cache de respostas
    pub linucb: Option<LinUCBBandit>,         // LinUCB bandit (rkyv serializado)
    pub bandit: Option<Box<dyn ContextualBandit>>, // Polymorphic bandit
    pub online_rl: Option<OnlineRLEngine>,    // Online RL engine
    pub symbol_store: Option<SymbolStore>,    // touring-ast symbol DB
    pub symbol_index: Option<SymbolIndex>,    // In-memory cross-project index
    pub pipeline: Option<SharedPipeline>,     // Incremental AST pipeline
    session_turn: AtomicUsize,                // Contador de turns por sessão
    pub predictor: Option<TinyTransformerPredictor>, // Tool sequence predictor
    pub crdt_graph: Option<CrdtSemanticGraph>, // CRDT multi-agent knowledge
    pub cognitive: Option<CognitiveRuntime>, // Cognitive engine
    pub aco_wiring: Mutex<AcoWiringState>,   // ACO bus+bridge+predictor
    pub dependency_cache: Option<DependencyCache>, // petgraph call graph
    // (+ campos privados menores)
}
```

### Arquitetura IPC

```
Claude Code
    │
    ▼
touring-hook (thin client, ~8.8MB)
    │ DaemonRequest { hook, payload, project_root }
    │ newline-delimited JSON over Unix socket
    │ connect timeout: 100ms | read/write timeout: 3000ms
    ▼
/tmp/touring-daemon-{uid}.sock
    │
    ▼
touring-daemon (~8.7MB, persistente)
    │ RuntimeMap: HashMap<PathBuf, HookRuntime>
    │ Single-threaded accept loop
    │ Lazy init por project_root
    ▼
DaemonResponse { output, success }
    │
    ▼
stdout → Claude Code (additionalContext)
```

### Dependências Externas do Crate

```toml
# Internas (outros crates touring)
touring-core, touring-ast, touring-learning, touring-cognitive
touring-rules (optional), touring-antt (optional)

# Externas principais
serde_json, rusqlite (WAL mode), rkyv (zero-copy snapshots)
petgraph (call graph), regex, tracing, libc
```

---

## 2. Pipeline de Hooks

### Cobertura por Categoria

| Categoria | Hooks | Tratamento |
|---|---|---|
| **Daemon (warm cache)** | pre-read, pre-bash, pre-edit, pre-edit-prevention, post-read, post-bash, post-edit, post-tool-rl, session-start, session-stop | Handler dedicado + daemon |
| **Daemon (handler existe, fora do DAEMON_HOOKS)** | teammate-idle, task-completed | ⚠️ Handler em `dispatch_request` MAS não em `DAEMON_HOOKS` → sempre standalone |
| **Stateless** | prompt-enhance, qa-syntax | Sem `HookRuntime`, sem daemon |
| **Lifecycle semi-inteligente** | subagent-start, subagent-stop | Em `dispatch_request` — apenas `record_access()` |
| **Lifecycle zero-inteligência** | file-changed, cwd-changed, pre-compact, worktree-create/remove, notification, config-change, permission-request, instructions-loaded, setup, elicitation × 2, post-tool-failure, session-end, post-compact | `run_lifecycle_event()` — apenas INSERT no DB |

### Análise por Hook

| Hook | Latência esperada | Error handling | Issues |
|---|---|---|---|
| `pre-read` | ~2ms (warm) | ✅ fallback Allow | Token budget ausente |
| `pre-bash` | ~2ms (warm) | ✅ fallback Allow | — |
| `pre-edit` | ~3ms (warm) | ✅ fallback Allow | blast_radius sem limite |
| `post-edit` | ~5ms (warm) | ✅ silencioso | — |
| `post-tool-rl` | ~3ms (warm) | ✅ silencioso | Reward binário (GAP-RL1) |
| `session-start` | ~50ms (cold SQLite) | ✅ silencioso | Head-of-line blocking |
| `teammate-idle` | ~10ms (standalone) | ✅ fire-and-forget | Fora do DAEMON_HOOKS (GAP-A3) |
| `task-completed` | ~10ms (standalone) | ✅ fire-and-forget | Fora do DAEMON_HOOKS (GAP-A3) |
| `subagent-start/stop` | ~5ms | ✅ | Apenas record_access (GAP-A4) |
| `file-changed` | ~1ms | ✅ | Zero inteligência (GAP-A4) |
| `pre-compact` | ~1ms | ✅ | Zero inteligência — perdendo flush rkyv (GAP-A4) |

---

## 3. Diagnóstico de Gaps (7 Dimensões)

### Dimensão A — Arquitetura

**GAP-A1 — Complexidade Ciclomática Crítica**
- `dispatch_request()` em `daemon.rs`: match gigante com ~15 arms + cfg flags → CC estimado ~25-29
- Impossível unit-testar arms individuais isoladamente
- Adicionar hook = editar 3+ lugares (dispatch_request + DAEMON_HOOKS + standalone match)

**GAP-A2 — God Object: HookRuntime**
- 19 campos, muitos `Option<>`, CC=16 em `HookRuntime::new()`
- Um hook simples (pre-bash) carrega o mesmo runtime que session-start (que precisa de RL + cognitive + CRDT)
- Inicialização lazy preserva performance, mas API surface é enorme

**GAP-A3 — DAEMON_HOOKS dessincronizado** *(confirmado no código)*
- `DAEMON_HOOKS` em `main.rs:68` contém apenas 10 hooks
- `teammate-idle` e `task-completed` têm handlers completos em `dispatch_request()` mas **não estão em `DAEMON_HOOKS`**
- Resultado: ACO wiring do Agent Teams sempre executa standalone (sem warm cache, sem lazy state)

**GAP-A4 — 17 Lifecycle Hooks com Zero Inteligência**
- `run_lifecycle_event()` apenas faz `record_access()` no DB
- Oportunidades desperdiçadas (listadas em S4)

### Dimensão P — Performance

**GAP-P1 — Daemon Single-threaded com Head-of-line Blocking**
- Accept loop é single-threaded em `daemon.rs`
- `session-start` pode levar ~50ms (SQLite init para novo projeto)
- Bloqueia todos os hooks na fila durante esse tempo

**GAP-P2 — Timeout Excessivo no Fallback IPC** *(confirmado: `main.rs:450`)*
- `set_read_timeout(3000ms)` + `set_write_timeout(3000ms)`
- Sem circuit breaker: daemon sobrecarregado = todos os hooks esperam 3s antes do fallback
- 200ms adicional de retry para daemon start (`main.rs:436`)

**GAP-P3 — Sem Token Budget no Context Injection**
- `compose_high_signal_context()` não tem limite de tokens
- Arquivo com muitos gotchas injeta KB de contexto
- Sem ranking por relevância no momento da composição

### Dimensão Q — Qualidade do Contexto

**GAP-Q1 — CILA Level não influencia o Contexto**
- `classifier.rs` classifica e retorna `CILAResult` com nível, técnicas, confidence
- Pre-hooks não recebem o `CILAResult` da sessão atual
- L0 (chat simples) recebe mesmo contexto rico que L4 (agent loop)

**GAP-Q2 — Gotchas sem Staleness Decay**
- Gotchas acumulam sem expiração
- Sem campo `resolved_at`, `decay_score`, ou `last_occurrence`
- Problemas resolvidos continuam sendo injetados semanas depois

### Dimensão RL — Feedback Loop

**GAP-RL1 — Reward Signal Grosseiro**
- `ImmediateReward` usa `accepted: bool` (binário)
- Não captura `context_utility` — se o contexto injetado ajudou no resultado
- LinUCB aprende se a ferramenta foi usada com sucesso, não se o contexto contribuiu

**GAP-RL2 — Sem Correlação Pre → Post Hook**
- Contexto injetado no pre-hook não é rastreado até o resultado no post-hook
- LinUCB não distingue "contexto útil que preveniu erro" de "contexto que foi ignorado"

### Dimensão T — Testes

**GAP-T1 — Lifecycle Hooks sem Testes**
- 17 lifecycle hooks executam apenas `run_lifecycle_event()` — sem testes de comportamento
- Handlers futuros (S4) precisam de infra de teste

### Dimensão E — Extensibilidade

**GAP-E1 — Registro de Hooks Descentralizado**
- 3 lugares para sincronizar ao adicionar hook: `DAEMON_HOOKS`, `dispatch_request()`, standalone match
- Macro `hook_registry!` resolveria isso

### Dimensão O — Observabilidade

**GAP-O1 — Métricas por Hook Ausentes**
- Sem `invocations`, `avg_latency_ms`, `context_bytes_injected`, `cache_hits` por hook
- Health check endpoint inexistente (`S9` mitiga)

---

## 4. Best Practices da Indústria (Context7)

### Tokio — Async Patterns Aplicáveis
- `spawn` por connection no accept loop elimina head-of-line blocking (→ S3)
- `spawn_blocking` para operações SQLite pesadas sem bloquear o runtime
- `select!` para timeouts adaptativos por tipo de hook

### Tower — Service/Layer Pattern
- `ServiceBuilder + Layer` = modelo exato do que os hooks precisam
- Um hook = `service_fn(|payload| async { ... })`
- Middleware layers: timeout por hook, circuit breaker, cache, logging, metrics
- Composição sem modificar o handler (→ S1, S10)

### Tracing — Já Bem Usado
- `#[instrument]` presente nos hooks — correto
- `Span::or_current().instrument()` para propagação em async tasks
- Campos typed (`fields(hook = "pre-read")`) — já implementado
- Oportunidade: exportar spans para OpenTelemetry (→ S11)

### SQLite + deadpool — Optimization Patterns
- WAL mode já ativo — correto
- Batch insert com transações explícitas para gotchas/outcomes em volume
- FTS5 para busca semântica em `knowledge_context` sem Tantivy
- Index em `(file_path, created_at DESC)` para queries de gotcha recente

### IPC Performance
- Framing atual (newline-delimited JSON) é correto e simples
- Zero-copy upgrade: `bincode` ou `rkyv` para wire format — 3-5x speedup
- Mas: breaking change — requer versioning do protocolo antes

---

## 5. Estratégias de Excelência (15 estratégias)

### SPRINT 1 — Quick Excellence (2-3 dias úteis)

**S4a [P0, 30min] — teammate-idle/task-completed no DAEMON_HOOKS**
- Problema: GAP-A3 — ACO do Agent Teams sempre standalone
- Solução: Adicionar `"teammate-idle"` e `"task-completed"` ao array `DAEMON_HOOKS` em `main.rs:68`
- Já tratados em `dispatch_request()` — mudança é apenas no DAEMON_HOOKS
- Validação: os dois hooks passam pelo daemon path
- ROI: maior por hora trabalhada neste plano inteiro

**S14 [P0, 2h] — Circuit Breaker File-Based para IPC**
- Problema: GAP-P2 — 3,1s de timeout silencioso por hook quando daemon sobrecarregado
- Solução: Arquivo de estado `/tmp/touring-circuit-{uid}.state` com `failure_count + last_failure_ts`
- Se ≥3 falhas em 60s → skip daemon por 60s (fast fallback em <1ms)
- Reset automático após 60s sem falhas
- Preserva Exit 0 — apenas muda quando tenta o daemon
- Benefício: latência do fallback de 3.100ms → <5ms

**S5 [P0, 2h] — Token Budget + Ranking no Context Injection**
- Problema: GAP-P3 — contexto sem limite degrada LLM
- Solução: `compose_high_signal_context(db, file_path, max_tokens: usize = 800)`
- Score por sinal: `recency_score × relevance_weight × severity`
  - `recency_score = 1.0 / (1.0 + days_since_last_occurrence.max(0.1))`
  - Gotchas: weight=2.0, bash failures: weight=1.5, dependents: weight=1.0
- Ordenar por score DESC, truncar ao budget
- Benefício: contexto conciso, >2x signal-to-noise

**S13 [P1, 4h] — Graceful Shutdown do Daemon**
- Problema: `process::exit(0)` no watchdog pode abortar SQLite write
- Solução: `ShutdownChannel` via `std::sync::mpsc::channel()`
- Watchdog idle envia `ShutdownSignal`; accept loop drena requests pendentes (timeout 2s)
- Flush explícito: `linucb.save_rkyv()`, `crdt_graph.save()`, `session_insights.flush()`
- Depois: `process::exit(0)` normal
- Benefício: zero risco de corrupção SQLite/rkyv durante shutdown

**S9 [P2, 2h] — Health Check Endpoint**
- Nova variant: `DaemonRequest::Health` / CLI: `touring-hook --daemon-health`
- Retorna: `{ uptime_secs, requests_served, projects_loaded, avg_latency_ms, cache_hit_rate, last_error }`
- Usado por scripts de monitoramento e pelo circuit breaker (S14) para diagnóstico

**S11 [P2, 2h] — Métricas Estruturadas por Hook Event**
- `HookEventMetrics { hook_name, invocations, avg_latency_ms, p99_latency_ms, context_bytes_injected, cache_hits, fallback_count }`
- Mantido em `RuntimeMap` (por projeto) + global no daemon
- Exposto via health endpoint (S9) e em `session-stop` report
- Benefício: observabilidade para debugging e otimização

---

### SPRINT 2 — Intelligence Upgrade (3-5 dias úteis)

**S1 [P1, medium] — Dispatch Table (CC 29 → 5)**
- Problema: GAP-A1 — `dispatch_request()` com CC alto, inextensível
- Solução:
  ```rust
  type HookFn = fn(&mut HookRuntime, &Value) -> String;
  static HOOK_DISPATCH: OnceLock<HashMap<&'static str, HookFn>> = OnceLock::new();

  fn init_dispatch() -> HashMap<&'static str, HookFn> {
      let mut m: HashMap<&'static str, HookFn> = HashMap::new();
      m.insert("pre-read", |rt, v| pre_read::run_returning(rt, v).to_json());
      m.insert("post-edit", |rt, v| { let _ = post_edit::run(rt, v); String::new() });
      // ... uma linha por hook
      m
  }
  ```
- Adicionar hook = 1 linha. CC de dispatch cai para ~3.

**S4 [P1, medium] — Inteligência nos Lifecycle Hooks**
- Problema: GAP-A4 — 17 hooks com zero inteligência
- Implementar handlers inteligentes para os 5 mais impactantes:

  | Hook | Ação Inteligente |
  |---|---|
  | `file-changed` | Invalidar `result_cache` para o arquivo + trigger `IncrementalEditResult` |
  | `cwd-changed` | Pre-warm knowledge DB (top gotchas do novo diretório) |
  | `subagent-start` | Injetar context snapshot do projeto (top 5 gotchas + recent failures) |
  | `pre-compact` | Flush `linucb.rkyv` + `qtable.rkyv` + `SessionInsights` + `CrdtGraph` snapshot |
  | `worktree-create` | Sincronizar `DependencyCache` com novo worktree |

**S6 [P1, medium] — Intent-Aware Context Injection**
- Problema: GAP-Q1 — CILA level não usado nos pre-hooks
- Solução: `session-start` classifica intent e armazena `session_cila_level: u8` no `result_cache`
- Pre-hooks leem e ajustam:
  - L0-L1: apenas gotchas com `risk ≥ 0.7` (contexto mínimo)
  - L2-L3: gotchas + bash failures (comportamento atual)
  - L4-L6: contexto completo (gotchas + failures + dependents + blast_radius + cognitive)
- `ReminderBandit` (já implementado em S8/touring-hooks) decide adaptativamente

**S7 [P1, medium] — Context-Utility Feedback Loop**
- Problema: GAP-RL1 e GAP-RL2 — reward binário sem correlação
- Solução:
  1. Pre-hook: salvar `(session_turn, file_path, context_hash)` em `result_cache`
  2. Post-tool-rl: recuperar contexto da mesma turn; avaliar utilidade:
     - Tool succeeded + context avisou sobre o padrão que ocorreu → `utility = 1.0`
     - Tool failed mas context não mencionou → `utility = 0.2`
     - Tool succeeded sem relação com context → `utility = 0.5`
  3. Reward LinUCB: `reward = base × 0.7 + context_utility × 0.3`
- Benefício: LinUCB aprende QUAL contexto é útil, não apenas se a ferramenta funcionou

**S8 [P2, low] — Staleness Decay para Gotchas**
- Problema: GAP-Q2 — gotchas nunca expiram
- Schema migration (SCHEMA_VERSION → 5):
  ```sql
  ALTER TABLE file_gotchas ADD COLUMN decay_score REAL NOT NULL DEFAULT 1.0;
  ALTER TABLE file_gotchas ADD COLUMN last_occurrence TEXT;
  ALTER TABLE file_gotchas ADD COLUMN resolved_at TEXT;
  ```
- `decay_score = 1.0 / (1.0 + weeks_since_last_occurrence)`
- Auto-resolve: 5 edits com sucesso no arquivo após o gotcha → `resolved_at = now()`
- Filtro na query: `WHERE decay_score > 0.1 AND resolved_at IS NULL`

---

### SPRINT 3 — Architecture Upgrade (1-2 semanas)

**S3 [P1, high] — Daemon Multi-threaded**
- Problema: GAP-P1 — head-of-line blocking no accept loop
- Solução: Thread pool bounded (max 4 threads via `rayon`) por connection
  ```rust
  let pool = ThreadPoolBuilder::new().num_threads(4).build()?;
  loop {
      let (stream, _) = listener.accept()?;
      let runtime = Arc::clone(&runtime);
      pool.spawn(move || handle_connection(stream, runtime));
  }
  ```
- `RuntimeMap: Arc<Mutex<HashMap<PathBuf, HookRuntime>>>` — já thread-safe com Mutex
- ⚠️ Auditoria `Send` necessária: `rusqlite::Connection` é `Send + !Sync` — seguro dentro de Mutex
- `HookRuntime` precisa ser `Send`: verificar `Box<dyn ContextualBandit>` e `CognitiveRuntime`
- Benefício: session-start (50ms) não bloqueia pre-read (2ms) de outra sessão

**S2 [P2, high] — Decomposição do HookRuntime**
- Problema: GAP-A2 — god object com 19 campos
- Solução: Sub-runtimes agrupados por responsabilidade:
  ```rust
  pub struct HookRuntime {
      pub context: ContextRuntime,        // knowledge + classifier + pii + result_cache
      pub learning: Option<LearningRuntime>, // linucb + bandit + online_rl + qtable
      pub cognitive: Option<CognitiveRuntime_ext>, // predictor + crdt + cognitive_rt
      pub infra: InfraRuntime,            // symbol_store + index + pipeline + dep_cache
      pub aco_wiring: Mutex<AcoWiringState>,
      pub project_root: PathBuf,
      session_turn: AtomicUsize,
  }
  ```
- Inicialização lazy preservada em cada sub-runtime
- Hooks simples acessa apenas `context` sem carregar `learning` ou `cognitive`

**S10 [P2, medium] — Hook Registry Centralizado**
- Problema: GAP-E1 — 3 lugares para sincronizar
- Solução: Macro `hook_registry!` que gera `DAEMON_HOOKS` + dispatch HashMap + standalone match
  ```rust
  hook_registry! {
      daemon {
          "pre-read"  => pre_read::run_returning,
          "post-edit" => post_edit::run,
          "teammate-idle" => team_hooks::run_teammate_idle,
          // ...
      }
      standalone {
          "prompt-enhance" => prompt_enhance::run,
          "qa-syntax"      => qa_syntax::run,
      }
      lifecycle {
          "file-changed"   => lifecycle::handle_file_changed,
          "pre-compact"    => lifecycle::handle_pre_compact,
          // ...
      }
  }
  ```
- Adicionar hook = 1 linha no registry. Zero dessincronização.

**S12 [P2, medium] — Pre-warming do Cache em session-start**
- Solução: Em `run_session_start()`, após carregar sessão:
  1. Query `top 20 files by read_count DESC` do knowledge DB
  2. Para cada arquivo: `compose_high_signal_context(db, file, max_tokens=800)`
  3. Popular `result_cache` antecipadamente
  4. Usar `TinyTransformerPredictor` (já em `HookRuntime`) para priorizar arquivos
- Benefício: ~80% das primeiras invocações de pre-read = cache hits

---

### SPRINT 4 — Horizon (1 mês+)

**S15 [P2, high] — Cross-project Knowledge via CrdtDelta**
- `CrdtSemanticGraph` + `CrdtDelta` (já em touring-learning, S8 sprints anteriores)
- Daemon mantém CrdtGraph global (além dos per-project)
- Gotchas sobre padrões Rust genéricos propagam para todos os projetos Rust do usuário
- Delta sync por projeto: apenas `new_nodes + new_edges` desde último sync

**Plugin System WASM para Hooks Customizados**
- Usar `InferletPool` (touring-wasm, já implementado) para executar hooks WASM
- Interface: WASM module exporta `hook_run(payload_json: &str) -> *const u8`
- Usuário coloca `.claude/hooks/custom.wasm`, registra em `settings.json`
- Zero recompilação do workspace Touring

**IPC Wire Format Upgrade**
- JSON atual → `bincode` para wire format: ~3-5x speedup na serialização
- `DaemonRequest/DaemonResponse` já são `Serialize/Deserialize`
- Requer versioning do protocolo (magic bytes + version byte no início do frame)

---

## 6. Validação de Invariantes

| Estratégia | Exit 0 | Clippy | Tests verde | Schema migration |
|---|---|---|---|---|
| S4a | ✅ | ✅ | +2 testes | Não |
| S14 | ✅ | ✅ | +3 testes | Não |
| S5 | ✅ | ✅ | +2 testes | Não |
| S13 | ✅ | ✅ | +3 testes | Não |
| S9 | ✅ | ✅ | +2 testes | Não |
| S11 | ✅ | ✅ | +2 testes | Não |
| S1 | ✅ | ✅ | refactor only | Não |
| S4 | ✅ | ✅ | +8 testes | Não |
| S6 | ✅ | ✅ | +4 testes | Não |
| S7 | ✅ | ✅ | +5 testes | Não |
| S8 | ✅ | ✅ | +3 testes | **SCHEMA_VERSION → 5** |
| S3 | ✅ | ⚠️ Auditoria Send | +4 testes | Não |
| S2 | ✅ | ✅ | Large refactor | Não |
| S10 | ✅ | ✅ | refactor only | Não |
| S12 | ✅ | ✅ | +3 testes | Não |
| S15 | ✅ | ✅ | +5 testes | Não |

---

## 7. Resumo Executivo

O `touring-hooks` é uma base sólida com excelente design de fallback (exit 0 invariant) e pipeline de feedback inteligente. Os **5 gaps mais urgentes**:

1. **GAP-A3** — `teammate-idle`/`task-completed` fora do `DAEMON_HOOKS` (30min para corrigir)
2. **GAP-P2** — Circuit breaker ausente: 3,1s de timeout silencioso (2h para corrigir)
3. **GAP-P3** — Context injection sem token budget (2h para corrigir)
4. **GAP-A4** — 17 lifecycle hooks desperdiciando oportunidades de inteligência (3 dias)
5. **GAP-A1** — `dispatch_request()` CC=29, inextensível (1 dia de refactor)

**ROI por hora** (Quick Wins):
- S4a: 30min → impacto imediato no Agent Teams ACO
- S14: 2h → elimina degradação silenciosa de 3,1s
- S5: 2h → aumenta signal-to-noise do contexto LLM

**Arquivos-alvo por estratégia:**
- `main.rs:68` → S4a, S10
- `ipc.rs` + `main.rs:426` → S14
- `pre_read.rs`, `pre_bash.rs`, `pre_edit.rs` → S5, S6, S7
- `daemon.rs:176` → S1, S3, S9, S11
- `runtime.rs` → S2, S12
- `session_hooks.rs` → S6, S12, S13
- `team_hooks.rs` → S4a
- `knowledge.rs` → S8
- `lib.rs` (novo) → S10

---

*Análise gerada por TACO Orchestrator N₂ v4.0 — código-fonte lido diretamente, zero inferência de campos.*
