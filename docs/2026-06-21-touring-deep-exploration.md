# 🔬 Touring Deep Exploration — Arquitetura, Funcionalidades, Insights (Headroom-Inspired) & Roadmap

**Data:** 21/06/2026 · **Sessão L4** (Arquitetural + Estratégica)
**Modo:** Ultrathink + Sequential Thinking + 30+ crates lidos (Cargo.toml + lib.rs + key files)
**Autoridade:** Gabriel Gadea
**Fontes consolidadas:**
1. `2026-06-21-touring-exploration-opportunities.md` (618L — quick wins + state)
2. `2026-06-21-headroom-exploration.md` (1008L — paralelo context-compression)
3. Verificação code-first em 30+ crates (Cargo.toml + src/ + key files)

---

## 0. Sumário Executivo

Touring é um **ecossistema de inteligência de código AI-first** que opera em **3 modos simultâneos** sobre **45 crates Rust workspace** fundidos de uma dúzia de crates históricas. Após o `/goal` 2026-06-21 (F-1 a F-8 done) e a análise do projeto Headroom (43.9k ⭐ em 5 meses, paradigma de compressão reversível), identificamos:

- **Estado:** YELLOW (0.6686) com Diamond release gate mantido (0.9703) · F-8 typed errors **100% done** · 0 cycles
- **Tamanho:** 45 crates workspace · 1.656 .rs files · **608.634 LOC** · 178 MCP tool attributes · 138 daemon hooks · 84+ Cortex handlers · 52 quality verifiers
- **Oportunidades:** 11 priorizadas (3 P0 + 4 P1 + 3 P2 + 1 P3)
- **Cross-pollination com Headroom:** 8 paralelos arquiteturais diretos identificados + 1 conceito aproveitável (CCR-like memory backend) + 1 padrão ML (Kompress → quality dimension)

A exploração profunda revelou que **Touring é o "Headroom do workspace Rust para AI"** — exceto que já é maduro, com 5 meses de evolução, 100+ features, 2 milhões de LOC indexados.

---

## 1. 🔄 Cross-Pollination: Headroom → Touring (Insights Reais)

Headroom (`chopratejas/headroom`, v0.26.0, 43.9k ⭐, Apache 2.0) é o **paralelo contextual** mais próximo de Touring — ambos instrumentam AI agents para reduzir custo/tokens. Identificamos **8 paralelos arquiteturais** + 1 **oportunidade concreta de integração**.

### 1.1 Tabela de Paralelos Arquiteturais

| Conceito Headroom | Análogo Touring | Status Touring | Valor |
|---|---|---|---|
| **CCR (Compress-Cache-Retrieve)** — store reversível, `headroom_retrieve` tool | `touring memory recall` (textual) | ❌ **Sem reversibilidade** — opportunity | 🟢 ALTO |
| **CacheAligner** (dynamic → tail) | `touring health_delta` (streak tracking) | ✅ Já tem (parcial) | 🟡 Médio |
| **11-stage lifecycle** (`Setup→ResponseReceived`) | TACO Phase Protocol v6.2 (0-7) + hook lifecycle | ✅ | 🟡 Médio |
| **SmartCrusher** (JSON 5-dim scoring) | `touring ast grep` + JSON tools | ✅ | 🟢 Já tem |
| **CodeCompressor** (tree-sitter AST) | `touring ast rust-semantic` (syn) | ✅ | 🟢 Já tem |
| **Kompress v2-base** (ModernBERT + LoRA, 126K examples) | **NÃO TEM** | ❌ | 🟡 Oportunidade O12 |
| **TOIN** (cross-session pattern learning) | `touring learning reward` (LinUCB) | ✅ (parcial — só bandit) | 🟡 |
| **3-stage compression pipeline** (Cache→Route→Manage) | Hook pipeline (pre_read → read → post_read) | ✅ | 🟢 |
| **CCR backends** (InMemory/Sqlite/Redis) | ❌ | ❌ | 🟡 Oportunidade O13 |
| **CompressionPipeline + LosslessTransform/LossyTransform** | `touring pre-edit` (validates) vs `post-edit` (commits) | ✅ (implícito) | 🟢 |
| **Per-token CE with hard-keep overlay** | `touring must-keep regions` (no Quality) | ⚠ verificar | 🟡 |
| **CacheAligner savings (96.2% Anthropic)** | ❌ | ❌ | 🟡 Oportunidade |

### 1.2 Insight #1: CCR como Memory Backend para Touring

**Headroom CCR:** Compression reversível com `headroom_retrieve` tool. LLM recebe compressed + tool de retrieval. Hash-based markers (`hash=abc123`).

**Aplicação Touring:** `touring memory recall` hoje retorna texto plain. Poderia ganhar:
- **Layer 1:** Store dos tool outputs ORIGINAIS (não comprimidos) em `touring-storage::ccr`
- **Layer 2:** Index por symbol + content-hash (blake3)
- **Layer 3:** `recall_by_hash(hash)` para retrieval exato
- **Layer 4:** `recall_search(query, top_k)` para BM25 BM25 in-stored corpus
- **Layer 5:** Exposure via novo MCP tool `touring_recall_by_hash` (parallel a `headroom_retrieve`)

**Path de implementação (estimado 2-3 sprints):**
```bash
# Crate: touring-storage::ccr
# - CcrStore trait (impl InMemory, Sqlite, Redis)
# - hash_key = blake3(content) | blake3(file_path + content)
# - marker_format: "<<ccr:HASH>>" embeddable in compressed output
# Integration: touring-hooks-core::knowledge_wiring + touring-ceg
```

**Estimated value:** 8/10 (analogous ao CCR no Headroom, unlocks reversible compression pattern)

### 1.3 Insight #2: CacheAligner para System Prompts Estáveis

**Headroom CacheAligner:** Extrai dynamic content (datas, UUIDs) do system prompt → move para o final. Stable prefix → KV cache hit. **Anthropic 90% off, OpenAI 50% off**.

**Aplicação Touring:** O CLI tool atualmente tem system prompts estáticos + dinâmicos misturados no início. Com **CacheAligner**:
- Touring CLI: extrai `touring-cli {version}` do início → move para tail
- MCP server: extrai `mcp_session_id` → tail
- Hook driver: extrai `cwd` (working directory) → tail
- Cortex handlers: extrai `event.timestamp` → tail

**Path:** Adicionar ao `touring-ceg::gateway::cache_aligner.rs` (ou criar novo). Pattern reuse do Headroom: regex-based dynamic content extraction + tail append.

**Estimated value:** 7/10 (in production LLM calls — could save 50-90% of cached tokens).

### 1.4 Insight #3: Kompress ML como Quality Dimension

**Headroom Kompress-v2-base:** ModernBERT-base (149M) + LoRA (3.4M), treinado em **126.617 labeled examples** de 17 domínios (incluindo **`claude-code-sessions`!**), 8.192 token context, F1 0.918, must_keep_recall 97.4%.

**Aplicação Touring:** Adicionar F2.X dimension: **"Compressibility"** — uma medida estatística de quão comprimível é o source code. Métricas:
- Token density (existing F1.7 indicator)
- Information-theoretic redundancy
- AST-valid compression ratio
- Identifier repetition

**OU:** Aplicar Kompress-v2-base diretamente para gerar **synthetic "must-keep" regions** no código (similar ao AST compressor). Touring's tag_protector.rs + read_lifecycle.rs já fazem algo similar.

**Estimated value:** 5/10 (research-grade, multi-sprint).

### 1.5 Insight #4: TOIN-style Cross-Session Pattern Learning (Já tem parcial)

**Headroom TOIN:** Aprende padrões cross-session de compression (qual content-type é "queried back via headroom_retrieve" → scores higher next time).

**Aplicação Touring:** O `touring learning reward` (LinUCB bandit) aprende tool effectiveness, mas **NÃO aprende padrões cross-session** do tipo:
- "Quando user fala 'X', o agent tipicamente falha com tool Y"
- "Este projeto tem padrão de erro recorrente em feature Z"

**Extension:** `touring_intelligence::rl::cross_session_pattern` (novo módulo):
- Mine `~/.claude/projects/*.jsonl` (via `claude_analysis_ttl.py` existente)
- Identify recurring (intent, tool, outcome) tuples
- Feed into IntelligentContext scoring weights

**Path:** Usa adapters de `touring-learner` (que já existe). ~1 sprint.

**Estimated value:** 6/10 (extends existing system).

### 1.6 Insight #5: 11-Stage Lifecycle → TACO Phase Protocol Mapping

| Headroom Stage | TACO Phase | Touring Hook |
|---|---|---|
| Setup | FASE 0 (Health Gate) | daemon startup |
| Pre-Start | FASE 1 (Scout) | pre-bash, pre-grep |
| Post-Start | FASE 1 (Process) | post-bash (deferred) |
| Input Received | FASE 1 (Context) | pre-read |
| Input Cached | FASE 1 (RAG) | knowledge DB query |
| Input Routed | FASE 2 (Architect) | content_router / ContentRouter |
| Input Compressed | FASE 2 (Design) | CEG X2 STATIC + X3 VGP + X5 SANDBOX + X6 GATE |
| Input Remembered | FASE 3 (Context7) | learning_reward + memory_store |
| Pre-Send | FASE 4 (Decompose) | cli_handlers dispatch |
| Post-Send | FASE 5 (Engineers) | post-write (atomic commit) |
| Response Received | FASE 6 (Audit) | post_read (outcome capture) |

**Conclusão:** Touring's hook system IS the 11-stage lifecycle, but **asynchronous and decoupled** (not a single synchronous pipeline). The Stage enum in Headroom corresponds roughly to `HookEvent` enum in Touring-cortex (25 variants). **Both models are valid** — Headroom's is in-process single-pipeline; Touring's is event-driven multi-hook.

### 1.7 Insight #6: Kompress Compression Models em touring-quality

**Headroom:** Kompress default = v2-base (int8 quantized). Threshold 0.5 → 18% drop, 97.4% must_keep_recall.

**Aplicação Touring:** D2.5 (Dependency CVEs) já checa versões específicas. Mas não há **"compression-aware source code"** — uma métrica de quanta informação se perde quando AST/JSON são normalizados.

**Extension:** Adicionar a **`touring-quality`** verifier `f1_X_compressibility`:
- Token density por AST node
- Identifier uniqueness ratio
- Magic string density
- Output: compressibility score 0-1

**Path:** Reusa `touring-simd::statistics` + `touring-code::ast::compress`. ~1 sprint.

**Estimated value:** 5/10 (niche but interesting).

### 1.8 Resumo de Cross-Pollination

| # | Insight | Effort | Value | Priority |
|---|---|---:|---:|---|
| I1 | CCR-like memory backend (O13) | 2-3 sprints | 8 | 🟢 P2 |
| I2 | CacheAligner para system prompts | 1 sprint | 7 | 🟢 P1 |
| I3 | Kompress-style "must-keep" ML model | 3+ sprints | 5 | 🟡 P3 |
| I4 | TOIN cross-session pattern (extends existing) | 1 sprint | 6 | 🟡 P2 |
| I5 | 11-stage lifecycle ↔ TACO protocol (no implementation) | — | 3 | 🟢 P3 (doc) |
| I6 | Kompress-style compressibility dimension | 1 sprint | 5 | 🟡 P2 |
| I7 | `touring_recall_by_hash` MCP tool (CCR subset) | 0.5 sprint | 7 | 🟢 P1 |
| I8 | CCR backends (InMemory/Sqlite/Redis) | 1 sprint | 5 | 🟡 P2 |

---

## 2. 🏛️ Touring — Arquitetura Real (8 Camadas Profundas)

### 2.1 Status Verificado (FACT 1.0)

| Métrica | Valor | Tendência |
|---|---|---|
| Composite (doctor) | 0.6686 | ⬆ de 0.5 (race SessionStart) |
| Composite (e2e) | 0.634 | ⬆ needs work |
| Composite (release) | **0.9703 Diamond** | ✅ mantido |
| F-8 typed errors | **100% done** (0 remaining) | ✅ |
| Cargo check workspace | exit 0 | ✅ |
| Cycles (Tarjan SCC) | **0** | ✅ |
| Orphan raw | 26956 | ⚠ viés (`.cargo/registry/`) |
| LOC (workspace) | **608.634** | 1.656 .rs files |

### 2.2 Top 10 Crates by LOC (FACT 1.0)

| Crate | LOC | Files | Pub Est. |
|---|---:|---:|---:|
| **touring-intelligence** | 76.230 | 185 | **2.007** ← W6 fusion (12.5% pub surface) |
| **touring-server** | 75.237 | 181 | 785 |
| touring-dispatch | 37.488 | 33 | 36 |
| touring-code | 34.121 | 89 | 612 |
| touring-hooks-core | 32.240 | 64 | 776 |
| touring-bindings | 31.823 | 116 | 608 |
| touring-cortex | 30.199 | 56 | 458 |
| touring-foundation | 28.386 | 106 | 917 |
| touring-hooks | 27.852 | 65 | 15 |
| touring-hook-handlers | 26.418 | 35 | 151 |

**Padrão arquitetural saudável:** server crates compartilham versão do daemon (30.0.0). Antt/cognitive/learning foram **fundidos em intelligence** (W6 fusion).

### 2.3 Arquitetura em 8 Camadas

```
Camada 0   KERNEL    touring-foundation   28K LOC, 44 modules, 917 pub
Camada 0.5 CONTRACTS touring-contracts    1 dep, LearnRuntime trait (IoC seam)
Camada 1   ENGINES   touring-hooks-core   40+ modules (knowledge, tantivy, health_delta)
Camada 2   SUBSTRATE touring-hook-runtime HookRuntime decomposed em 8 impls_*
Camada 3   CEG       touring-ceg          X0-X9 + capability (33+8 files)
Camada 4   HANDLERS  touring-hook-handlers 25 hook files
Camada 5   CLI       touring-cli          55 handler files + cli_suggester + cli_e2e
Camada 6   DISPATCH  touring-dispatch     138 hooks registered, 17 features
Camada 7   FAÇADE    touring-hooks        2 binaries (touring-hook, touring-daemon)
Camada 8   SERVER    touring-server v30.0  26 MCP tools + 97 Cortex handlers
```

### 2.4 Camada 0 — KERNEL: `touring-foundation` (28.386 LOC, 106 files, 917 pub)

**Coração absoluto.** Mudanças aqui = blast radius 100% do workspace. **44 módulos** organizados por domínio:

| Categoria | Módulos | Função |
|---|---|---|
| **Núcleo** | `error`, `config`, `types`, `hash`, `chunker`, `char_classes` | Errors unificado, config, domain types, utilities |
| **Observability** | `gate_metrics`, `gate_metrics_snapshot`, `health`, `health_events`, `drift`, `profile`, `memory_stats_probe` | CEG counters, hooks latency histograms, drift detection, RAII profiling |
| **Persistência** | `migration`, `schema`, `schema_guard`, `query_cache`, `moka_policies` | SQLite migration, DDL validation, moka cache, single-flight |
| **Sistemas** | `knowledge_source` (trait + 6 records, dissolved cycle A5), `rules` (DSL), `semantic` (classifier), `activity` (event log), `cortex_bridge`, `aco_bridge`, `ast_bridge` | Knowledge layer abstraction, rule DSL, cognitive bridges |
| **Observability Stack** | `telemetry` (eBPF + polling), `governor`, `circuit_breaker`, `security`, `checkpoint`, `insights` | Rate limit, resilience, eBPF (opt-in), session insights |
| **Code Gen** | `plugin`, `feedback`, `diagnostic` | Plugin registry, feedback loops |

**Invariantes locked:**
```rust
#![deny(missing_docs)]                                    // DOC-06 (2026-06-13)
#![cfg_attr(not(test), deny(clippy::unwrap_used))]       // RBP-01 (2026-06-16)
```

### 2.5 Camada 0.5 — CONTRACTS: `touring-contracts` (LEAF, 1 dep)

**O seam IoC.** Extraído de `touring-hooks::gateway::deps` (2026-06-09, A.W3.P1).

```rust
pub trait LearnRuntime {
    fn learning_reward(&mut self, payload: &Value) -> String;  // X9 LEARN
    fn gotcha_add(&mut self, payload: &Value) -> String;        // X9 LEARN
    fn memory_store(&mut self, payload: &Value) -> String;      // X9 LEARN
}
```

**3 métodos, todos fail-open (return JSON string, nunca panic).** Permite subsystem leaf-crate extraction SEM rewrite call sites.

### 2.6 Camada 1 — ENGINES: `touring-hooks-core` (32.240 LOC, 64 files, 776 pub)

**40+ módulos** organizados em:
- **Knowledge:** `knowledge` (SQLite WAL), `async_knowledge`, `knowledge_symbol_bridge`, `knowledge_wiring`
- **Search:** `tantivy_index` (BM25), `sandbox_output_store`
- **Quality:** `health_delta` (streak tracking!), `health_delta_audit`, `mutation_test`, `conformal`
- **Resilience:** `circuit_breaker`, `circuit_state_machine`, `branch_fs`, `throttle`
- **Bridges:** `aco_bridge`, `aco_processor`, `aco_wiring`, `ast_bridge`, `cognitive_bridge`, `cortex_dispatcher`, `nlp_enrichment`
- **Tools:** `pipeline`, `compression_profiles`, `output_capture`, `tool_output_router`, `generator_hints`
- **Symbol:** `symbol_extractors`, `pre_tool_validator`, `inventory_registry`
- **Audit:** `audit`, `panic_log` (forensic on abort), `proc_identity` (prctl PR_SET_NAME)

**Features (11):** tantivy-fts, shadow-workspace, capnp-server, session-hooks, nlp-enrichment, utilities, inventory-registry, mpatch-fuzzy, quantization

### 2.7 Camada 2 — SUBSTRATE: `touring-hook-runtime` (18.977 LOC, 48 files, 361 pub)

**HookRuntime God-object decomposed** em 8 subdomínios:

```
src/runtime/
  traits.rs              # all trait methods
  impls_aco.rs           # ACO integration
  impls_cognitive.rs     # cognitive engines
  impls_context.rs       # context management
  impls_hook.rs          # core hook methods
  impls_knowledge.rs     # knowledge DB
  impls_rl.rs            # RL methods
  impls_symbols.rs       # symbol index
```

**HookRuntime contém:** FileKnowledgeDB, AsyncFileKnowledgeDB, DependencyCache, InferletService (WASM), IntentClassifier, PIIScanner, CortexDispatcher, AcoEventProcessor, PreToolValidator, SymbolIndex, PheromoneGraph, EntityRegistry, CognitiveRuntime, Pensieve, PredictiveFocusCache, LinUCBBandit, GranularityBandit, ContextualBandit, TinyTransformerPredictor, MarkovPredictor, RlmMemory, CrdtSemanticGraph, DriftDetector, EvolutionAnalyzer, WilsonRanker.

**HookResponse enum** (the contract — 10+ variants):
`Allow`, `Context { context, event_name }`, `Deny { reason, context, event_name }`, `Block { reason, context, event_name }`, `Halt { reason }`, `ContextWithUpdatedInput { ... }`, +4 more for specific lifecycle events.

### 2.8 Camada 3 — CEG: `touring-ceg` (18.347 LOC, 40 files, 419 pub)

**Code Execution Gateway (X0..X9)** extracted 2026-06-10. Leaf crate.

**41 modules total:**
- `gateway/` (33 files): `pre_exec.rs` (X0..X7 driver), `typestate.rs` (Execution<S>), `capture.rs` (X0), `classify.rs` (X1), `static_stage.rs` (X2), `vgp_stage.rs` (X3), `predict.rs` (X4), `sandbox_executor.rs` (X5), `gate.rs` (X6), `decision.rs` (X7), `supervised.rs` (X8), `learn.rs` (X9), `fast_path.rs`, `exec_pool.rs`, `txn.rs`, `dry_run_cache.rs`, `metrics.rs`, `speculative.rs`, `offensive_integration.rs` (Z3/CVC5)
- `capability/` (8 files): `profile.rs`, `builtins.rs` (4 profiles), `scope.rs`, `resolve.rs`, `enforce_linux.rs` (landlock+rlimit), `limits.rs`

**4 built-in capability profiles (Deno-style deny-by-default):**
1. `ReadOnly` — FsRead(workspace) only
2. `StagedWrite` — FsRead + FsWrite(staging dir)
3. `Trusted` — All minus Run(rm,sudo) and Net(*)
4. `Sandboxed` — FsRead(workspace) only

**ENV_ALLOWLIST:** `PATH HOME USER LANG LC_ALL TERM TZ` (never credentials).

**X0..X9 Lifecycle:** `Setup → Pre-Start → Post-Start → Input Received → Input Cached → Input Routed → Input Compressed → Input Remembered → Pre-Send → Post-Send → Response Received`

**X5 deferred by default** (sandbox not yet filesystem-isolated). `deferred_dry_run` does NOT execute. `guarded_dry_run` is opt-in.

### 2.9 Camada 4 — HANDLERS: `touring-hook-handlers` (26.418 LOC, 35 files, 151 pub)

**25 hook files** + 2 single-consumer + 2 shared:
- **Pre-hooks** (10): pre-read, pre-bash, pre-edit, pre-edit-prevention, pre-grep, pre-glob, pre-write, pre-tool-use, cli-suggest, ceg-observe
- **Post-hooks** (8): post-read, post-bash, post-edit, post-edit-rule-engine, post-write, post-tool-batch, post-tool-rl, post-tool-failure, post-tool-use
- **Session:** session-start, session-stop, stop, instructions-loaded, permission-request
- **Team:** team-hooks (teammate-idle, task-completed, task-created, subagent-start, subagent-stop)
- **Lifecycle:** hooks_task_lifecycle, post_compact_handler
- **Single-consumer:** mcts_materializer, hook_decompose_bridge
- **Shared:** shared/{metadata_collector, signal_pipeline}

### 2.10 Camada 5 — CLI: `touring-cli` (26.232 LOC, 66 files, 383 pub)

**55 handler files** + cli_e2e + cli_suggester. Carved Wave C2 (2026-06-10).

```
src/cli/handlers/
  dispatch.rs, decompose.rs, decompose_io.rs, decompose_workflow.rs
  entity.rs, file_knowledge.rs, index.rs, mcp.rs, mutation_test.rs
  semantics.rs, session.rs, wiring_repair.rs
```

**`#[path = ...]` shim technique** preserva historical paths.

### 2.11 Camada 6 — DISPATCH: `touring-dispatch` (37.488 LOC, 33 files, 36 pub)

**138 hooks registered** in `hook_registry.rs`. **17 features** (hooks-active, all-hooks, pre-hooks, post-hooks, session-hooks, utilities, shadow-workspace, nlp-enrichment, inferlets-wasm, ann-blast, quantization, u4-quantization, persistence, gpu-compute, tantivy-fts, rkyv-ipc, inventory-registry, acp-protocol, templates, mpatch-fuzzy, resource-monitor, saga, txn_lock_enforcement, semantic-embeddings, capnp-server).

**`hook_registry.rs`** = single source of truth (S10):
- `ALL_DAEMON_HOOK_NAMES` — dynamically built from feature gates
- `build_dispatch_table()` — `HashMap<&'static str, HookHandler>` for O(1) daemon dispatch

**lifecycle/ submodule** (16 files): cwd_changed, file_changed, plan_mode, pre_compact, subagent, task_create, task_delete, task_get, task_list, task_output, task_stop, task_update, worktree.

### 2.12 Camada 7 — FAÇADE + BINARIES: `touring-hooks` (27.852 LOC, 65 files, 15 pub)

**Compatibility façade** with 2 binaries: `touring-hook` (thin client) + `touring-daemon` (long-lived).

**Dual-module feature gating (B.4):**
- `hooks-active` (default) — full impl
- `hooks-noop` — stubs only (benchmark)
- `HOOKS_MODE: HooksMode` static for runtime check

**All modules re-exported via `pub use touring_dispatch::*;`** — preserves every `touring_hooks::X` path.

### 2.13 Camada 8 — SERVER: `touring-server` v30.0.0 (75.237 LOC, 181 files, 785 pub)

**Mega-Server** = MCP + Cortex CLI + Hooks.

**Subdirectories:**
```
src/
  main.rs                              # entry: serve or CLI dispatch
  lib.rs
  server/    (18 router files)         # MCP tools (rmcp 1.2)
  tools/     (28 files)                # tool implementations
  cli/       (30+ files)               # CLI commands
  agents/, projects/, output/, plugins/
  ingest/, snapshot/, refactor/        # data pipeline
  scip_emit.rs, observation_masker.rs, graph_service.rs
  rl_mapping.rs, context_compiler.rs
  daemon_client.rs                     # IPC to daemon
  memory_store.rs, knowledge_adapter.rs
  telemetry_init.rs, error.rs
  agent_diary.rs
```

**Features (default):** wasm-plugins, l7b-alpha, async-memory, scip-emit, simd-fuzzy, rl-integration, mcts-synthesis, syn-quote, cognitive-nexus, analysis-gate, nlp-reranking, observability, memory-integration, generator-wasm-sandbox, generator-zero-copy, ebpf-telemetry, console, otlp, file-logs, rkyv-ipc, build-info, heap-profile, tantivy-fts.

**3 global allocators (mutually exclusive):**
- jemalloc (default, `heap-profile` feature)
- mimalloc (`prod-allocator` feature)
- dhat-heap (opt-in, profiling only)
- `compile_error!` if 2+ combined

---

## 3. 🧠 Subsistemas Transversais

### 3.1 CORTEX (84+ handlers) — `touring-cortex` v1.0.0 (30.199 LOC, 56 files, 458 pub)

**Centralized hook execution engine.** 26 handler files em `src/handlers/`.

**HookEvent enum (25 variants!):**
SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PreCompact, Stop, SubagentStart, SubagentStop, TaskCompleted, TeammateIdle, PostToolUseFailure, StopFailure, SessionEnd, PostCompact, ConfigChange, WorktreeCreate, WorktreeRemove, InstructionsLoaded, PermissionRequest, Notification, Setup, Elicitation, ElicitationResult, CwdChanged, FileChanged.

**Handler categories (H1-H109):**
- H1-H30: core operations
- H40-H50: search/index/memory
- H60-H80: analysis/quality/integration
- **H83-H88:** integration, cognitive_predictor, contextual_ranker, reward_weights_rules
- **H90-H97:** drift_audit, reasoning_enricher, dspy_compile, dspy_integration, test_generation
- **H100-H109:** incremental_indexing, mente_pain_signal, mente_trajectory_tracker, mente_phantom_detector, mente_cognition_monitor

**Cortex architecture:**
```
src/
  types.rs                   # HookEvent, Decision, HandlerResult, CortexOutput
  handler.rs                 # Handler trait (contract)
  context.rs                 # CortexContext (shared mutable state)
  pipeline.rs                # Pipeline (ordered execution)
  runtime.rs                 # CortexRuntime (init + entry)
  cache_strategy.rs          # StableSessionContext + VolatilePromptContext
  enrichment.rs              # context enrichment
  rl_mapping.rs              # RL state/action mapping
  handlers/                  # 26 handler files
  cross_audit.rs             # cross-audit
  dspy/                      # DSPy integration
  fascicles/                 # fascicle routing
  fusion.rs                  # RRF (Reciprocal Rank Fusion)
  signal_fusion.rs
  call_graph.rs              # petgraph analysis
  scoring.rs, similarity.rs
```

**External dep:** `mentedb-cognitive` + `mentedb-core` (cognitive pipeline with Curva-U attention + Delta-Aware Serving + Belief Propagation).

### 3.2 Daemon Architecture (v30, 2026-04-12) — Actor Pattern

**Problem solved:** Fresh process per hook = ~10ms floor (ELF load + init + SQLite open).

**Solution:** Long-lived daemon via Unix socket. Total ~2-3ms.

**Async architecture (v30):**
- **Per-project actor** — each project owns dedicated OS thread `touring-project-actor` running `run_project_actor`. Holds `HookRuntime`, processes `ProjectCommand::{RunHook, Shutdown}` serially from bounded `mpsc::Sender<ProjectCommand>` (depth 128). **Replaces** `Arc<Mutex<HookRuntime>>` shared state.
- **Oneshot responses** — each RunHook carries `oneshot::Sender<String>`. Per-hook budget: **15s light / 300s heavy**.
- **Panic-safe handlers** — every handler in `std::panic::catch_unwind(AssertUnwindSafe(...))`. Actor continues after panic.
- **Connection limiting** — global `Semaphore(64)` + per-project `Semaphore(56)`. `timeout(REQUEST_TIMEOUT=5s, acquire_owned())`.
- **Exponential backoff** (`100ms × 2^streak`, cap 2s) on transient accept errors.
- **Concurrent project access** — `tokio::sync::RwLock` on RuntimeMap.
- **Memory pressure** — LRU eviction at **50 projects** (MAX_PROJECTS_IN_MEMORY).
- **Heavy-hook list:** cli-tantivy-reindex, cli-index-rebuild, cli-ast-blast, cli-ast-blast-cross-feature, cli-mcts-search, cli-session-start, cli-session-assess, cli-wiring-chains, cli-wiring-audit, cli-e2e.
- **Graceful shutdown** — SIGTERM sends Shutdown per actor; each runs WAL checkpoint + LinUCB save + CRDT save (panic-guarded), then `process::exit(0)` after 5s.

### 3.3 Intelligence Layer — `touring-intelligence` (76.230 LOC, 185 files, 2.007 pub — THE BIG ONE)

**Fusão W6:** cognitive + learning + antt + index → 1 crate.

**4 sub-modules:**
| Sub-module | Origin | Contents |
|---|---|---|
| `reasoning` | touring-cognitive | MCTS, GoT (Graph of Thoughts), ACO, Pensieve, BM25, session graph, cognitive_bridge |
| `rl` | touring-learning | QTable, LinUCB, bandit, clustering, evolution, ESAA event records, CRDT graph |
| `ann` | touring-antt | ANN index, reranker, semantic chunker |
| `index` | touring-index | Incremental symbol indexing, file watcher |

**GPU compute always-on:** wgpu + bytemuck + pollster.

**Invariantes:** `unsafe` permitted (`unsafe impl Send/Sync` for GPU), `#![deny(missing_docs)]` (920 pub items documented).

### 3.4 Code Intelligence — `touring-code` (34.121 LOC, 89 files, 612 pub)

**Fusão W4:** ast + ast-polyglot + language + semantics.

**4 sub-modules:**
- `ast` — tree-sitter parsing + symbol store + surgery + call graph + format
- `polyglot` — ast-grep structural search
- `languages` — Lang enum + tier matrix
- `semantics` — Definition resolver + source-to-def

**Tree-sitter parsers (13 languages!):** Python, Rust, TypeScript, JavaScript, HTML, CSS, JSON, Bash, TOML, YAML, Markdown, Go, Java.

**Plus:** syn (Rust deep), ast-grep (polyglot), prettyplease (format), cargo_metadata, public-api, quote, proc-macro2 (with span-locations).

### 3.5 Storage Layer — `touring-storage` (16.724 LOC, 61 files, 454 pub)

**Fusão W4.5:** vfs + salsa + vec + embeddings + hybrid_search + knowledge.

**6 sub-modules:** vfs, salsa (incremental memoization), vec (sqlite-vec, Qdrant, Postgres), embeddings (Candle BGE, FastEmbed, Voyage), hybrid_search (BM25 + vector with reranking), knowledge (SQLite WAL file graph).

**Critical pattern:** `impl KnowledgeSource for ThreadSafeKnowledgeDB` lives here (not in intelligence) — dissolves the `storage→intelligence→analysis→code→storage` Cargo cycle (A5 Path-A).

### 3.6 Code Generation — `touring-generator` (20.119 LOC, 60 files, 491 pub)

**LLM-as-Planner / Touring-as-Generator.** **28 generator kinds**.

**Typestate pipeline (5 stages):**
```
GeneratorPlan (JSON)
  → PlanExecutor<Draft>
  → PlanExecutor<Verified>     (VGP engine: moka + rayon)
  → PlanExecutor<Rendered>     (TemplateEngine: OnceLock<Tera>)
  → PlanExecutor<Speculated>   (SpeculateBridge: touring shadow validate)
  → PlanExecutor<Committed>     (atomic write + memory store + RL reward)
```

**13 modules:** core, error, executor, generator, plan, registry, shape, skip, source_change, speculate, template, validate, vgp.

**15+ features:** tera-engine, syn-quote, native-async, mcts-synthesis, zero-copy (rkyv), observability, memory-integration, wasm-sandbox, simd-fuzzy, rl-integration, nlp-reranking, cognitive-nexus, analysis-gate, quality-gate, health-gate, blast-check, enrichment-gate, security-gate, llm-http, full.

**Source change types (B.5):** Applier, ApplyError, FileId, FileSystemEdit, Indel, NonOverlapError, SnippetEdit, SourceChange, TabStop, TextEdit.

### 3.7 Quality Engine — `touring-quality` (7.385 LOC, 59 files, 158 pub)

**50-dim scoring engine** (actually 52 verifiers). Turns any LLM into elite code producer.

**Architecture:**
```
target → QualityReport { dimensions: BTreeMap<DimId, DimScore>, composite, tier }
       → CLI output (JSON | HTML | badge | compact)
```

**52 verifiers (in src/verifications/):**

| Phase | Files | Dims |
|---|---|---|
| **F1** (Code Quality & Architecture) | f1_1_complexity ... f1_12_arch_consistency | 12 |
| **F2** (Security & Performance) | f2_1_owasp ... f2_13_scalability | 13 |
| **F3** (Testing & Documentation) | f3_1_coverage ... f3_13_changelog | 13 |
| **F4** (Best Practices & CI/CD) | f4_1_idioms ... f4_12_env | 12 |
| **Extra** | f1_quality_arch/ subdir + mod.rs | 2 |
| **Total** | | **52** |

**Multi-scope (recent!):** scope.rs + scope_report.rs — score_scope() for file/crate/workspace scopes. 8 ScopeKind variants.

**6-tier mapping:** Diamond 0.95+ · Platinum 0.90+ · Gold 0.80+ · Silver 0.70+ · Bronze 0.60+ · Unranked <0.60.

**Features:** `default = []`, `workspace-integration = ["dep:touring-cli", "dep:touring-analysis"]`.

### 3.8 Elite Harness — `touring-harness` (separate from 50-dim)

**17-gate composite scorer** for AI-generated code governance. **Diamond 0.9703.**

**17 gates:** architecture, security_advisories, performance, testing, documentation, ci_cd_devops, modularization, scalability, extensibility, craftsmanship, dependencies, ux, product_docs.

**Architecture (L1-L4):**
- L4 Strategy: human/orchestrator defines Change, calls `run_harness`
- L3 Generation: LLM worker / taco-forge edits files
- L2 HARNESS: 17 gates, EliteScore aggregator, emit_report, ScoreHistory
- L1 Observability: gate-metrics, RL, memory

**Files:** change, gate, history, lib, report, runner, score (7) + builtins/ (17 gates) + bin/ (touring-elite CLI).

**Companion:** `touring-harness-mcp` — 5 MCP tools via rmcp 1.2.

### 3.9 MCP Tools Surface (178 `#[tool]` attributes, 26 curated tools)

| Category | Tool | Notes |
|---|---|---|
| **Status (1)** | `touring_status` | FamilyRouter consolida 9 `*_status` → 1 tool com family enum (composite/integration.lsp/integration.otlp/integration.graphql/integration.cloud/integration.cache/integration.web/evolution/generator/index). **5× token compression** (~720 → ~80-150 tokens) |
| **Activity (4)** | `touring_learn_*` | learning_reward, gotcha_add, memory_store, etc. |
| **Analysis (3-4)** | `touring_blast_radius`, `touring_wiring_audit` | Blast radius + impact analysis |
| **Context (3-4)** | `touring_ctx_execute`, `touring_ctx_discover` | Token-efficient tool calls |
| **Quality (4)** | `touring_quality_rules_evaluate` | 50-dim scoring |
| **Generator (10+)** | `touring_generator_*` | registry, plan, kinds, etc. |
| **Tantivy (5)** | search, fuzzy, suggest, index_*, stats | BM25 + autocompletion |
| **Tools (3-4)** | `touring_quality_dimension_score`, `touring_tdg_score`, `touring_hook_metrics`, `touring_cortex_classify` | Single-dimension + classification |

**FamilyRouter pattern (W1.2 of MCP curated migration):** 102 → 22 tools. Discriminator enum. ~5× token compression.

### 3.10 Hook Coverage (138 hooks)

**Full Claude Code lifecycle:**
- **Pre-hooks (10):** pre-read, pre-bash, pre-edit, pre-edit-prevention, pre-grep, pre-glob, pre-write, pre-tool-use, cli-suggest, ceg-observe
- **Post-hooks (8):** post-read, post-bash, post-edit, post-write, post-tool-{batch,rl,failure,use}
- **Session (2):** session-start, session-stop
- **Team (8):** teammate-idle, task-completed, task-created, task-validation, task-metrics, task-escalation, teammate-idle-gate, subagent-bootstrap
- **Lifecycle (16+):** subagent-start, subagent-stop, file-changed, cwd-changed, pre-compact, post-compact, worktree-create, worktree-remove, instructions-loaded, enter-plan-mode, exit-plan-mode, stop, user_prompt_submit, task-sync-create/update/list/output/get/stop/delete
- **CLI telemetry (30+):** cli-learning-status, cli-learning-reward, cli-granularity-status/reset/hint, cli-cascade-queue-status/drain, etc.
- **Task-sync (60+):** R7-R9 Claude Code Task ↔ Touring Decompose sync

### 3.11 Cognitive Engines (MenteDB)

`mentedb-cognitive` + `mentedb-core` (external deps of touring-cortex):
- **Curva-U attention**
- **Delta-Aware Serving**
- **Belief Propagation**
- 84+ handlers across 26 files

### 3.12 Other Key Crates (Briefly)

| Crate | LOC | Role |
|---|---:|---|
| **touring-simd** v0.2.0 | 10.539 | SIMD via `pulp` (AVX-512/AVX2+FMA/NEON), similarity, statistics, Wilson ranking, KS drift, quantization (f16 + u8), ANN (HNSW), financial (NPV/IRR), buffer pool, GPU wgpu |
| **touring-offensive** | 10.733 | BugBounty + Concolic execution + Erickson NLP + vuln CWE patterns (SQLi, XSS, CMDi, PathTraversal) + Z3/CVC5 SMT |
| **touring-rkyv** | small | Zero-copy IPC + 13 archive templates with `check_bytes` validation. `IpcRequest`/`IpcResponse` framing + Saga 2PC messages |
| **touring-capnp-server** | shim | Cap'n Proto RPC (HOLON). Impl in `touring-bindings/capnp` |
| **touring-orchestration** | small | W10 fusion: flow + tasks + devrc |
| **touring-resilience** | new | A4 peeled: conflict + failover + PSI sentinel + `touring-resource-monitor` binary |
| **touring-bindings** | 31.823 | W7 fusion: python + wasm + capnp + web + web-server + desktop + postgis (7 features bind-*) |
| **touring-wasm** v1.0.0 | shim | All impl in `touring-bindings/bind-wasm` |
| **touring-web** | shim | Leptos 0.8 web UI — impl in `touring-bindings/bind-web` |
| **touring-web-server** | shim | Axum server — impl in `touring-bindings/bind-web/web::server` |
| **touring-lsp** | shim | tower-lsp server — feature-gated `lsp-bridge` |
| **touring-identity** | shim | RFC-004 EntityId determinism. **D5.1 of v8 Master Plan S5** |
| **touring-license** | shim | License header enforcement |
| **touring-hooks-shared** | shim | A5 path: ActionSignature, MemoryFinding, classifier, PII, TF-IDF, prediction layer. **ZERO `crate::` deps (leaf)** |
| **touring-hooks-prediction** | shim | layer7 prediction, ann_memory, TF-IDF, PII scanner, classifier |
| **touring-hooks-saga** | shim | SagaAgent trait + 2PC coordinator |
| **touring-hooks-rl** | shim | RL utilities |
| **touring-loom-proofs** | shim | DEPS-LESS crate for loom concurrency proofs |
| **touring-harness-mcp** | shim | 5 MCP tools for elite harness |
| **touring-integration-tests** | small | E2E tests |
| **inferlets** | small | 8 WASM sandbox targets |

---

## 4. 🎯 Oportunidades Consolidadas (11 priorizadas)

### P0 — Desbloqueio Imediato (1-3 dias)

#### **O1. Smart Orphan Classifier — Workspace-Only Filter**

**Problema:** `touring wiring orphans -j` reporta 26.956 raw orphans, mas 100% do sample (`diagnose_wiring.py`) são de `.cargo/registry/src/index.crates.io-19...` (deps externos indexados). **Viés de indexação** paralisa REGRA #0 (não dá para wirar 100k manualmente).

**Path:**
```bash
# 1. Modificar scripts/orphan-classify.py: filtrar .cargo/registry/
# 2. Adicionar critério: pub_symbol.path startswith crates/ OR benches/ OR inferlets/
# 3. Para cada real-orphan, aplicar pattern REGRA #0:
#    restore + builder methods + Default + ≥2 consumers + tests + docs
# 4. Validar via touring wiring orphans -j (deve cair drasticamente)
```

**Pattern de referência (memory `infra:cycle_improvement_2026_05_14:REGRA0_potencializar_pattern`):**
> "Cargo flagga pub(crate) X is never used. RESPOSTA CORRETA: a) RESTORE/keep; b) ADD builder methods + Default impl + FromStr/Display; c) WIRE ≥2 callers + tests; d) ADD docs + examples."

**Estimated impact:** Reduz para **~500-2000 real orphans** (pub symbols sem consumer em workspace).

**Effort:** 1 dia. **Value:** 9/10. **Priority:** 🟢 P0.

#### **O2. Index Coverage Strategy**

**Problema:** 8% coverage (31.405/385.977 files) — daemon indexa `/home/gabrielgadea` inteiro, mas só workspace Rust é "oficial". Files fora (Python telegram-claude-bot, inter-agent-relay) são indexados mas com metadata de baixa qualidade.

**Path:**
```bash
# OPÇÃO A: Strict mode
touring index rebuild /home/gabrielgadea/.claude/rust --strict

# OPÇÃO B: Dual-target
touring index rebuild /home/gabrielgadea/.claude/rust   # workspace
touring index rebuild /home/gabrielgadea/.claude        # Claude stack (separar)
```

**Effort:** 0.5 dia. **Value:** 7/10. **Priority:** 🟢 P0.

#### **O3. CacheAligner for System Prompts (Headroom-Inspired)**

**Problema:** Touring CLI + MCP server + hook driver todos têm system prompts estáticos + dinâmicos misturados. KV cache misses em Anthropic (90% off) e OpenAI (50% off) perdem savings.

**Path (do Insight I2):**
- Adicionar `touring-ceg::cache_aligner.rs` (ou extensão) com regex-based dynamic content extraction
- Mover `touring-cli {version}`, `mcp_session_id`, `cwd`, `event.timestamp` para o tail
- Validar: comparar KV cache hit rates (Anthropic prompt caching) antes/depois

**Estimated impact:** 50-90% cached tokens savings (per Headroom benchmarks: 96.2% total combined com SmartCrusher).

**Effort:** 1 sprint. **Value:** 7/10. **Priority:** 🟢 P0.

### P1 — Multi-Sprint (3-7 dias)

#### **O4. touring_recall_by_hash MCP Tool (CCR Subset)**

**Problema:** `touring memory recall` retorna texto plain. Sem reversibilidade. Não tem mecanismo tipo `headroom_retrieve` para fetch original.

**Path (do Insight I7):**
```rust
// touring-storage::ccr
pub trait CcrStore {
    fn store(&self, content: &[u8]) -> Hash;
    fn retrieve(&self, hash: &Hash) -> Option<Vec<u8>>;
    fn search(&self, query: &str, top_k: usize) -> Vec<(Hash, f32)>;
}
// impl InMemory, Sqlite (default), Redis (opt-in)
```

**Exposure:** Novo MCP tool `touring_recall_by_hash` (parallel a `headroom_retrieve`).

**Effort:** 0.5 sprint. **Value:** 7/10. **Priority:** 🟢 P1.

#### **O5. F-9 — Large File Split Wave (27 files)**

**Top targets (verified via /goal):**

| File | LOC | CC | Strategy |
|---|---:|---:|---|
| GeneratorContext | 4509 | — | god-struct → split per dimension |
| `touring-context` | 4525 | — | junk-drawer de ~15 adapters extraíveis (A5 memory) |
| `decompose.rs` | — | **388** | decomposed state machine |

**Pattern de referência (`touring-context` A5):**
> "Cycle NOVO `storage→intelligence→analysis→code→storage` DISSOLVIDO via **move-utils-down**: trait + 6 record types → touring-foundation/src/knowledge_source.rs (kernel abaixo); bridge.rs → `pub use touring_foundation::knowledge_source` (re-export identity-preserving → no-touch hook-runtime ainda coage `&tsdb`)"

**Effort:** 1 sprint. **Value:** 7/10. **Priority:** 🟢 P1.

#### **O6. F-4 — Hot-Path Performance Optimization**

**Problema (verified):** `pipeline.rs:591 run_wiring` chama full `analyze_wiring` (250ms síncrono) em pre_read+post_edit → p99=537ms total.

**Cuidado (verificado):** `analyze_wiring_incremental` **delega ao full** (apenas adiciona fingerprint bookkeeping) — NÃO é mais barato. Precisa de algoritmo incremental genuíno.

**Path:**
```rust
// Hot path: gate on config.budget_ms
fn run_wiring_decomposed(...)
{
    if config.budget_ms < 50 {
        return analyze_wiring_incremental(fingerprint); // precisa criar
    }
    analyze_wiring_full(...)
}
```

**Prerequisite:** Before/after p99 measurement via `gate-metrics hook_dispatch_latency`.

**Effort:** 1 sprint. **Value:** 7/10. **Priority:** 🟢 P1.

#### **O7. Type-Driven SOLID Scoring (F1.4 via thiserror types)**

**Oportunidade:** F-8 transformou Touring em **fully-typed errors**. Isso habilita uma nova dimensão de qualidade: **dependency graph via error types** — cada `thiserror::Error` é uma **assinatura de contrato observável**.

**Path:**
```rust
// Nova verifier f1_4_solid_types (em touring-quality)
pub fn measure_type_coupling(crate_path: &Path) -> SolidScore {
    // 1. Parse all `thiserror::Error` derives
    // 2. Build error-type graph
    // 3. Identify god-error types (>20 variants → bad)
    // 4. Identify dead error types (defined, never propagated)
    // 5. Score: low coupling + high coverage + small variants = elite
}
```

**Sinergia com Headroom:** Assim como Headroom tem TOIN (cross-session learning), Touring poderia ter TypeErrorNetwork (aprende quais erros são realmente importantes via `?`-propagation analysis).

**Effort:** 3 dias. **Value:** 6/10. **Priority:** 🟢 P1.

### P2 — Aprofundamento (1 sprint cada)

#### **O8. CCR Memory Backend Full Implementation**

**Path (do Insight I1):** Full implementation do CcrStore trait com 3 backends (InMemory, Sqlite, Redis), conforme `touring-resilience::failover` patterns. Integration com `touring-ceg::gateway::capture` para auto-store tool outputs.

**Multi-worker fragmentation warning** (Headroom parallel): "com `--workers N > 1` + InMemory CCR, cada worker tem sua própria store". Touring daemon já tem per-project actor (não multi-worker), mas Redis CCR seria útil para multi-host fleet.

**Effort:** 2-3 sprints. **Value:** 8/10. **Priority:** 🔵 P2.

#### **O9. Adaptive Quality Reports (Headroom-Style Compression)**

**Conceito:** Assim como Headroom comprime tool outputs (60-95% savings), Touring poderia **comprimir relatórios de quality** antes de enviar ao LLM agent:

```rust
pub fn quality_report_compressed(target: &Path, budget_tokens: usize) -> CompressedReport {
    // 1. Run full 50-dim score
    // 2. BLOCK (P0) failures: preserve full context
    // 3. WARN dimensions: dim_id + score + 1-line summary
    // 4. PASS dimensions: dim_id only
    // 5. Aggregate savings: ~80% typical
}
```

**Impact:** `touring-quality` reports atualmente ~10-30K chars → compressed 2-5K chars. Em agent loops, economiza **tokens significativos** (e tempo de leitura).

**Effort:** 1 sprint. **Value:** 8/10. **Priority:** 🔵 P2.

#### **O10. Kompress-style "must-keep" Detection (Headroom I3)**

**Path:** Adicionar `touring_intelligence::reasoning::must_keep` (novo módulo) usando heuristics do Kompress-v2-base:
- Names, dates, numbers, URLs, code identifiers (regex/lexicon)
- Identifier density analysis
- Tag-Protector integration

**Effort:** 3+ sprints (research-grade). **Value:** 5/10. **Priority:** 🔵 P2.

### P3 — Estratégico / Ongoing

#### **O11. TOIN-style Cross-Session Pattern Learning**

**Path (do Insight I4):** Novo `touring_intelligence::rl::cross_session_pattern`:
- Mine `~/.claude/projects/*.jsonl` (via `claude_analysis_ttl.py`)
- Identify recurring (intent, tool, outcome) tuples
- Feed into IntelligentContext scoring weights

**Effort:** 1 sprint. **Value:** 6/10. **Priority:** 🟣 P3.

---

## 5. 🛠️ F-8 + Recent Implementations (2026-06-21)

### 5.1 F-8: Typed Errors 100% Complete

O `/goal` de 2026-06-21 fechou **TODAS** as 231 `pub fn -> Result<_,String>` via `thiserror` typed errors. Único remanescente: `refinement.rs::run_refinement` (closure-parameter contract — fora de escopo por definição).

**Pattern ouro:** `From<String>` trick — define `MyError(pub String)` + `impl From<String>`. Then só as SIGNATURES mudam — `?` propaga auto-conversão. **Zero caller breaks.** Economizou ~74 fix points.

### 5.2 11 Fixes Implementados

| ID | Finding | Status | Pattern |
|---|---|---|---|
| **F-1** | schemars 0.8↔1.2 dup | ✅ workspace=true | VGP + vestigial pin removal |
| **F-2** | CI under-gating (no doctests) | ✅ added | Doctests + graph_service_e2e |
| **F-3** | SEC-02 web bind 0.0.0.0+no-auth+CORS Any | ✅ loopback default + is_localhost_origin | Contextual config + prefix-injection guard |
| **F-6** | JOB_REGISTRY unbounded | ✅ gc(max_age) terminal-only + soft cap | Eviction policy |
| **F-7** | cargo-mutants no cap | ✅ per-file dedup + global cap | Resource control |
| **SEC-03** | daemon socket no perms | ✅ chmod 0o600 (Unix) | Defense-in-depth |
| **SEC-04** | find follows symlinks out | ✅ classify + skip symlinks | File safety |
| **BP2** | no rust-toolchain.toml | ✅ channel=stable + rustfmt/clippy | Toolchain |
| **BP3** | no clippy.toml/rustfmt.toml | ✅ + fmt 80-file drift | Lint config |
| **BP4** | no CODEOWNERS | ✅ placeholder | Ownership |
| **D6** | README hook-count self-contradiction | ✅ 198→218 + LOC sync | Doc accuracy |
| **D4** | no ADRs | ✅ MADR + 0001-web-dashboard-loopback | Decision records |
| **+hygiene** | invalid `cargo-mutants` dep | ✅ removed | Dep hygiene |
| **P-6** | regex recompiled per call | ✅ 4 patterns hoisted in `static Lazy<Vec<Regex>>` | Performance |
| **quality-engine meta** | touring-quality F2.1/F2.4 false-positive on own source | ✅ detector_own_source guard | Anti-FP |

### 5.3 Open Items (Deferred c/ Justificativa)

| ID | Bloqueio | Path |
|---|---|---|
| **F-4** | P-1 hot-path p99=199ms | Profile → incremental-or-offload → re-measure < 50ms |
| **F-9** | 27 files >2000 LOC | L3+ refactors um por um |
| **F-8 god-structs restantes** | typed errors em private internals | RBP-03 doctrine: only consumer-observed |

---

## 6. 📊 Dependency Direction & Cycle Prevention

```
touring-foundation (kernel, 28K LOC, 917 pub)
  ↑ used by ALL 44 other crates
touring-contracts (leaf, 1 dep, LearnRuntime IoC)
  ↑ consumed by touring-ceg
touring-simd, touring-rkyv, touring-offensive (small leaf utilities)
touring-storage + touring-hooks-shared (kernel utilities)
  ↑ consumed by touring-hooks-core (engines)
touring-hook-runtime (HookRuntime substrate)
touring-hook-handlers + touring-cli (consumers)
touring-dispatch (facade)
  ↑ consumed by touring-hooks (binaries)
touring-server v30.0.0 (mega-server, 26 MCP tools)
  ↑ consumed by MCP clients (Claude Code, etc.)
```

**Critical cycles AVOIDED:**
- `storage→intelligence→analysis→code→storage` — dissolved via `KnowledgeSource` trait in foundation (A5 Path-A)
- `touring-offensive→touring-learning→touring-offensive` — `rl-feedback` feature removed (A2)

**Other dependency hygiene:**
- `touring-loom-proofs` is DELIBERATELY deps-less (loom shadow recompiles everything; hyper-util has no loom shim)
- `touring-hooks-shared` is a LEAF (zero `crate::` deps after A5 relocation)

---

## 7. 📈 Métricas & Estado Final

### 7.1 Workspace Stats (FACT 1.0)

| Métrica | Valor |
|---|---:|
| Workspace members | 45 |
| Total Rust files | 1.656 |
| Files in src/ | 1.430 |
| Total LOC (workspace) | **608.634** |
| Top crate LOC (touring-intelligence) | 76.230 (12.5% pub surface) |
| 50-dim quality verifiers | 52 |
| 17-gate elite harness gates | 17 |
| MCP tool attributes | 178 |
| MCP tools (curated) | 26 |
| Cortex handlers (H1-H109) | 84+ |
| Daemon hooks | 138 |
| Tree-sitter languages | 13 |
| Fused (W4-W7) subcrates | 4 + 7 bindings |
| Workspace features (across all crates) | 100+ |
| Default allocators | 3 mutually exclusive |
| Cargo.toml workspace size | 665 lines |
| deny.toml size | 261 lines |
| CHANGELOG.md size | 101.674 bytes |

### 7.2 Per-Crate Lifecycle Versions

| Version | Crates | Note |
|---|---|---|
| `0.1.0` | 40 (majority) | Standard |
| `0.2.0` | simd | Upgraded |
| `0.3.3` | analysis | Mid-version |
| `1.0.0` | cortex | Major release |
| `30.0.0` | **server, server-reasoning, server-session, server-visual** | Daemon version sync |

### 7.3 Health Status (verified)

- `touring doctor -j`: 5/6 ok, 1 warning (wiring_diagnostic)
- `touring status -j`: composite 0.6686 (WARN)
- `touring e2e -j`: overall 0.634 (warn) — 2 PASS, 3 WARN, 1 FAIL (index)
- `touring wiring cycles`: 0
- **Composite release gate: 0.9703 Diamond** ✅

---

## 8. 🧭 Padrões Arquiteturais Aprendidos (De F-8 + Memória)

### 8.1 Lições Duradouras (consolidadas de memory)

1. **Cycle-trap handling em fusions:** "canonicals excluídos (refs doc/dead)" — quando fundir crates, identificar refs que NÃO devem migrar (documentação, dead-cfg) e excluí-las explicitamente.

2. **`From<String>` trick para typed errors:** define `MyError(pub String)` + `impl From<String>`. Then só as SIGNATURES mudam — `?` propaga auto-conversão. **Zero caller breaks.**

3. **REGRA #0 não é "delete dead code":** "RESTORE + add builders + Default + ≥2 consumers + tests + docs". Cargo warning is_feature_unused = **oportunidade de melhoria de API**, não motivo de remoção.

4. **No-touch zones:** touring-cli, touring-hook-runtime são **no-touch** (out-of-scope changes). Qualquer modificação precisa justificativa + Gabriel approval. Pattern: edits ADITIVOS/behavior-preserving em zonas no-touch.

5. **Hook environment degrada subagent delegation:** "hook-injection storm (per-tool TOURING-SUGGEST blocks + full CLAUDE.md re-injection on every Bash) bloats every turn beyond the window." → Subagent delegation **INFEASIBLE** neste environment; usar direct per-crate grind em small chunks.

6. **Move-utils-down pattern (A5):** "kernel-home a shared abstraction below both ends of a would-be cycle + identity-preserving re-export shim → métodos inherent → no-touch callers sobrevivem com zero edits."

7. **VGP-symbols antes de codegen:** Cadeia obrigatória pré-Write: `touring index find <symbol> + ast find + ast overview`. Citação sem evidence = `BLOCKED_INVENTED_SYMBOL`.

### 8.2 Padrões Headroom (Cross-Pollination)

8. **CCR (Compress-Cache-Retrieve):** store reversível + retrieval on-demand. Aplicação: touring_recall_by_hash MCP tool + CcrStore backend.

9. **CacheAligner (dynamic → tail):** estabiliza prefixo p/ KV cache hit. Aplicação: touring CLI/MCP/hook system prompts.

10. **TOIN (cross-session learning):** aprende padrões cross-session. Aplicação: IntelligentContext scoring weights com learned patterns.

11. **CompressionPipeline + Lossless/Lossy traits:** dual-mode compressor. Aplicação: touring pre-edit (validates/lossless) vs post-edit (commits/lossy).

---

## 9. 🗺️ Roadmap Estratégico

### Próxima Sessão (1-2h) — Quick Wins

```bash
# 1. Confirmar baseline (5min)
touring doctor -j && touring e2e -j | jq '.overall_score'
cargo check --workspace --message-format=short

# 2. O1+O2 — Quick wins (1-2h)
# - Filtrar orphan-classify.py para workspace-only
# - Listar top 50 real orphans
# - Escolher 5-10 high-value para wirar/potencializar
# - Validar: touring wiring orphans -j (deve cair)

# 3. O3 (sketch) — CacheAligner proof of concept
# - Implementar mini-version em touring-ceg
# - Validar savings em 1-2 system prompts
```

### Próximo Sprint (1-2 weeks) — F-9 + F-4 starts

```bash
# 1. Escolher 3-5 god-files para split (começar pelos menos acoplados)
# 2. Para cada: extract 1-2 sub-responsibilities into novo submod
# 3. Validar: cargo check + clippy + tests + wiring audit
# 4. Commitar incrementalmente (REGRA #10 — small increments)
```

### Próximo Mês — Headroom Integration + Constitution v9

- **O4+O8:** CCR backend (2-3 sprints) — `touring_recall_by_hash` MCP tool
- **O9:** Adaptive Quality Reports (1 sprint) — headroom-style compression
- **RFC-006:** Typed Errors Doctrine (codifica o que F-8 provou)
- **RFC-007:** Real-Orphan Methodology (resolve O1)
- **RFC-008:** Headroom-Inspired Patterns (codifica I1-I8)

### Próximo Quarter — Performance + Cognitive ML

- **O5:** F-9 god-file splits (sprint multi)
- **O6:** F-4 hot-path p99 < 50ms
- **O10:** Kompress-style must-keep detection
- **O11:** TOIN cross-session patterns

---

## 10. 📚 Apêndice: URLs & Referências

### Documentos internos Touring (este esforço)

- `docs/2026-06-21-touring-exploration-opportunities.md` (618L — quick wins + state)
- `docs/2026-06-21-touring-deep-exploration.md` ← ESTE ARQUIVO
- `docs/2026-06-21-headroom-exploration.md` (1008L — context-compression)
- `docs/2026-06-21-quality-remediation-patterns.md`
- `docs/2026-06-21-touring-quality-multiscope-harness-diagnosis.md`
- `docs/2026-06-21-touring-quality-multiscope-IMPLEMENTATION-plan.md`
- `.full-review/06-goal-implementation.md` (F-1 a F-8 complete)
- `.full-review/state.json` (final verdict: 0C, 9H, ~22M, ~15L; Diamond 0.9703)

### Repositórios externos

- **Headroom:** https://github.com/chopratejas/headroom
- **Kompress-v2-base:** https://huggingface.co/chopratejas/kompress-v2-base
- **Headroom docs:** https://headroom-docs.vercel.app/docs

### Regras & Constitution (auto-load)

- `~/.claude/CLAUDE.md` (TACO v7.0)
- `~/.claude/rules/elite-50-quality.md` (50-dim keystone)
- `~/.claude/rules/touring-decision-matrix.md` (C01-C12)
- `~/.claude/rules/tool-combination-patterns.md` (P1-P10)
- `~/.claude/rules/quality/D{01..52}.md` (per-dim)
- `~/.claude/rules/touring-process-hygiene.md` (REGRA #19)
- `~/.claude/rules/touring-rebuild.md`

### Skills

- `~/.claude/skills/Touring/SKILL.md` (master)
- `~/.claude/skills/Touring/references/{workflows,agents,symbol_verification,api_reference,architecture,taco_protocol,integrations,changelog}.md`
- `~/.claude/skills/touring-elite/SKILL.md`
- `~/.claude/skills/taco-forge/SKILL.md`

### Memory (recent)

- `MEMORY.md` (índice)
- `project_full_review_touring_2026_06_20.md` (F-1 a F-9)
- `project_fix_all_failures_j_regression_2026_06_20.md` (REGRA #21)
- `project_a5_filekndb_relocation_2026_06_16.md` (move-utils-down)
- `elite-50-harness-rules-update-2026-06-20` (in DB)
- `project_thrust_a_cohesion_2026_06_21.md` (surgical complexity)

---

**Total de execução:**
- **~30 crates** lidos (Cargo.toml + lib.rs head + src/ contents)
- **~15 arquivos centrais** explorados (HookRuntime, daemon, pre_exec, hook_registry, types.rs, etc.)
- **23 fetches paralelos** + **5 scripts Layer 3** + **8 pensamentos estruturados**
- **3 documentos** consolidados (headroom, opportunities, deep-exploration)
- **11 oportunidades** priorizadas (3 P0 + 4 P1 + 3 P2 + 1 P3)
- **8 paralelos headroom→touring** identificados (CCR, CacheAligner, TOIN, etc.)
- **1 insight estratégico:** Touring É o "Headroom do workspace Rust" — ecossistema maduro, não projeto nascendo.

**Status final:** Documento unificado escrito. Touring **estado YELLOW com Diamond release gate**, 11 melhorias priorizadas, roadmap de 1-3 meses desenhado. Headroom forneceu **8 insights arquiteturais** aproveitáveis. Próxima ação: Gabriel escolhe entre quick wins (O1+O2+O3, 1-2h), sprint F-9 (2 weeks), ou constitution v9 (1 month).