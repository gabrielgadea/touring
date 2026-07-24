# Master Plan v3.0 — Consolidação do Harness (FULL REUSE)

**Data**: 2026-06-25
**Autor**: TACO (Touring Agentic Code Orchestrator)
**Status**: ⏸️ PLANEJAMENTO v3.0 (nenhuma modificação executada)
**Supersedes**:
- `2026-06-25-harness-consolidation-master-plan.md` (v1.0)
- `2026-06-25-harness-consolidation-master-plan-v2.md` (v2.0)
**Mudança principal**: **FULL REUSE** — descoberta de `touring-cortex::handlers::quality` (já existe!) + 14 outras estruturas de infra que eliminam a necessidade de criar QUALQUER hook novo.

---

## TL;DR das Mudanças v1 → v2 → v3

| Item | v1 (criava) | v2 (reuse parcial) | **v3 (FULL REUSE)** |
|------|--------------|--------------------|--------------------|
| **Pre-write hook** | Criaria novo | Estenderia `touring-hook-handlers::hooks::pre_write.rs` | **Estender `touring-cortex::handlers::quality::CodeStandardsEnforcer`** (que JÁ EXISTE e é registrado) |
| **Post-write hook** | Criaria novo | Estenderia `touring-hook-handlers::hooks::post_write.rs` | **Estender `touring-cortex::handlers::quality::PostQualityGate`** (que JÁ EXISTE) |
| **Compliance/History** | Criaria | Reusaria streaming_hook_integration | **Reusar `touring-cortex::handlers::quality::ComplianceCollector`** (que JÁ EXISTE) |
| **F1.8 dep_cycles** | Implementava novo | Reusaria touring-analysis | **REUSE `touring-code::ast::graph::cycles`** (DFS 3-color) + `touring-cortex::call_graph` (Tarjan SCC) |
| **F1.7 boundaries** | Implementava novo | Idem | **REUSE `touring-code::ast::graph::blast_radius`** (BFS + HNSW ANN) |
| **Multi-lang quality** | Criaria | Idem | **REUSE `touring-code::ast::quality`** (14 langs com Wilson CI) |
| **LSP go-to-def** | Implementaria | Idem | **REUSE `touring-code::semantics::source_to_definition`** (tree-sitter) |
| **Before/after diff** | Criaria | Idem | **REUSE `touring-foundation::conflict::GraphImpact`** (3-tier) |
| **Schema versioning** | Criaria | Idem | **REUSE `touring-foundation::schema`** (3-domain DB, SCHEMA_V8) |
| **RL bridge** | Criaria | Reusaria streaming_hook_integration | **REUSE `touring-cortex::rl_mapping`** (já mapeia hook → RL) |
| **Multi-signal fusion** | Criaria | Idem | **REUSE `touring-cortex::signal_fusion`** (Bayesian) |
| **Remediation sequence** | Criaria | Idem | **REUSE `touring-intelligence::reasoning::cognitive_mcts`** (graph-informed MCTS) |
| **Co-edit predict** | Criaria | Idem | **REUSE `touring-intelligence::reasoning::coedit_predictor`** (RRF) |
| **Pattern detection** | Criaria | Idem | **REUSE `touring-foundation::semantic::SemanticClassifier`** (22 classes) |
| **PostToolRL** | Criaria | Estenderia post_tool_rl | **ESTENDER `touring-cortex::handlers::learning`** (que JÁ EXISTE) |
| **F1.1 complexity** | Implementava | Reusava touring-analysis | **FAST-PATH** via `touring-code::ast::quality::analyze_quality` (multi-lang 14) + BACKSTOP `touring-analysis::f1_1_complexity` (AST-precise) |

**Net effect v3**:
- Apenas **2 arquivos genuinamente novos** (vs 2 em v2):
  - `touring-quality/src/gates.rs` (Q6) — novo módulo
  - `touring-lsp/src/quality_diagnostics.rs` (1 file, smallest)
- **W6 reduzido de 11 tasks para 7 tasks** (integração via cortex é mais limpa)
- **Total de tasks reduzido de 57 para 53**

---

## 0. Inventário Completo de Infraestrutura (Descoberta v3)

### 0.1 `touring-cortex` — Centralized Hook Engine (84+ handlers)

```
touring-cortex/
  src/
    lib.rs                      ← facade: 84+ builtin handlers (H1-H84, H90-H97)
    pipeline.rs                 ← Pipeline executor with lazy handler instantiation
    handler.rs                  ← Handler trait (execute() → HandlerResult)
    context.rs                  ← CortexContext (shared mutable state)
    types.rs                    ← HookEvent, Decision, HandlerResult, CortexOutput
    call_graph.rs               ← petgraph: Tarjan SCC, callers/callees, hotspots
    signal_fusion.rs            ← Bayesian fusion (estimate + confidence → fused)
    rl_mapping.rs               ← RL state/action mapping
    cross_audit.rs              ← 19-strategy E2E validation
    runtime.rs                  ← CortexRuntime init
    scoring.rs                  ← relevance ranking
    enrichment.rs               ← context enrichment pipeline
    handlers/
      mod.rs                    ← register_all() registry
      quality.rs                ← 🆕 3 QUALITY HANDLERS (H51, H52, H53)
      learning.rs               ← 🆕 RL integration handlers
      intelligence.rs           ← ACO/DSPy/perception
      cognitive.rs              ← cognitive MCTS handlers
      drift.rs                  ← drift detection
      dspy.rs + dspy_compile.rs ← DSPy prompt compilation
      enforcement.rs            ← strategy enforcer
      enrichment.rs             ← enrichment handlers
      evolution.rs              ← evolution drift
      incremental_indexing.rs  ← incremental index
      integration.rs            ← integration handlers
      lifecycle.rs              ← lifecycle hooks
      mente.rs (feature)        ← MenteDB cognitive
      neural.rs                 ← 10 neural hook wrappers
      ranking.rs                ← ranking handlers
      rules.rs                  ← DSL rules
      session.rs                ← session handlers
      test_generation.rs        ← test gen
      tools.rs                  ← tool handlers
      wasm.rs                   ← WASM hooks
```

### 0.2 `touring-cortex::handlers::quality` (3 sub-handlers — já existe!)

```rust
// H51: CodeStandardsEnforcerHandler
//   - Events: PreToolUse[Write|Edit] (sync, CAN BLOCK)
//   - Flow: diff-based ruff lint (new content vs existing baseline)
//   - Cache: moka W-TinyLFU 10_000 entries
//   - Score: (errors * 1.0 + warnings * 0.2), blocks if > 2.0
//   - Persists to graph.db via ctx.persistence.log_hook_event
//   - Drift: drift_record("code_standards_score", score)
//
// H52: PostQualityGateHandler
//   - Events: PostToolUse[Write|Edit|MultiEdit] (async)
//   - Flow: format check + complexity estimate (lines/fn) + summary
//   - Tool matcher: "Write|Edit|MultiEdit"
//   - Skips if context_budget_remaining < 100
//   - Output: "QGate[file]: fmt:OK | complexity:OK(avg 15 lines/fn)"
//
// H53: ComplianceCollectorHandler
//   - Events: PostToolUse (async)
//   - Flow: metrics to compliance.jsonl + Wilson score
//   - Output: compliance metrics for governance
```

**Decision**: **ESTENDER esses 3 handlers** ao invés de criar novos. Eles já têm:
- Cache (moka 10K)
- Persistence (graph.db via `ctx.persistence.log_hook_event`)
- Drift tracking (`drift_record`)
- Async/sync correto
- Tool matching

### 0.3 `touring-code::ast` — AST + Graph + Quality (14 langs!)

```
touring-code/src/ast/
  mod.rs                        ← AST public API
  parser.rs                     ← tree-sitter parsers (rust/py/ts/js)
  quality.rs                    ← 🆕 analyze_quality(source, lang) — 14 langs
  graph/
    cycles.rs                   ← DFS 3-color cycle detection
    blast_radius.rs             ← BFS + HNSW ANN blast radius
    method_calls.rs             ← Method-call extraction for wiring
    imports.rs                  ← import edges for wiring
    pheromone.rs                ← ACO pheromone layer
    enriched.rs                 ← enriched graph
  symbols.rs                    ← symbol kind + parent + docstrings
  rust_semantic.rs              ← syn-based AST semantic (rust-only)
  speculate.rs                  ← AST-based speculation
  scope_map.rs                  ← scope resolution
  store.rs                      ← symbol store
  surgery.rs                    ← body replacement
  wiring.rs                     ← wiring analysis
  module_tree.rs                ← module hierarchy
touring-code/src/semantics/
  definition.rs                 ← Definition enum (FileRange, Usage, DefinitionKind)
  source_to_def.rs              ← recursive parent-walking → definition
  multi_lang.rs                 ← language-specific Definition mapping
  semantics.rs                  ← Semantics facade
```

**Reuse**:
- `touring_code::ast::quality::analyze_quality(source, lang) -> QualityReport` — **fast-path** para F1.x (multi-lang 14)
- `touring_code::ast::graph::cycles::SymbolIndex::detect_cycles()` — F1.8 dep_cycles
- `touring_code::ast::graph::blast_radius::BlastRadius` — F1.7 boundaries
- `touring_code::semantics::source_to_definition` — LSP go-to-definition
- `touring_code::ast::graph::method_calls::extract_method_calls` — wiring em runtime

### 0.4 `touring-foundation` — Foundation + Schema + Semantic + Conflict

```
touring-foundation/src/
  schema/                       ← 🆕 SCHEMA_VERSION=8 (3-domain DB)
    mod.rs                      ← ensure_all_schemas(knowledge, memory, graph)
    knowledge.rs                ← KNOWLEDGE_SCHEMA_V8
    memory.rs                   ← MEMORY_SCHEMA_V8
    graph.rs                    ← GRAPH_SCHEMA_V8 (hook_events table!)
  semantic/                     ← 🆕 22 SemanticClass categories
    classifier.rs               ← SemanticClassifier
    categories.rs               ← SemanticClass enum
    data.rs + data/             ← universal_rules.json (data-driven)
    rules.rs                    ← RuleEngine
    overrides.rs                ← per-language TOML overrides
    cli.rs                      ← touring definitions CLI
  conflict/                     ← 🆕 3-tier conflict detection
    ast_diff.rs                 ← < 100ms (sla)
    semantic.rs                 ← < 1s
    graph_impact.rs             ← < 5s (call-graph impact)
    sla.rs                      ← SlaSpec + SlaViolation
  drand_core, drift, gate_metrics, governor, hash, health, etc.
```

**Reuse**:
- `touring_foundation::schema::ensure_all_schemas` — quality events → graph.db
- `touring_foundation::semantic::SemanticClassifier` — F1.11 design patterns (22 classes)
- `touring_foundation::conflict::GraphImpactDetector` — before/after diff em post_write

### 0.5 `touring-intelligence::reasoning` — MCTS + Co-edit + Reasoning

```
touring-intelligence/src/reasoning/
  mcts.rs                       ← 🆕 Generic MCTS engine (UCT, pheromone layer)
  cognitive_mcts.rs             ← 🆕 SemanticGraph-informed MCTS (S6)
  coedit_predictor.rs           ← 🆕 RRF-based co-edit prediction
  gated_mcts.rs                 ← CEG-gated MCTS
  hybrid_engine.rs              ← MCTS + LinUCB
  mcts_streaming.rs             ← streaming MCTS
  adaptive_engine.rs            ← adaptive refinement
  agent_state_machine.rs        ← agent state
  got.rs                        ← Graph of Thoughts
  pensieve.rs                   ← MemoryBank
  reasoning_engine.rs           ← main facade
  ann_index.rs                  ← ANN integration
  bm25_tfidf.rs                 ← BM25 + TF-IDF retrieval
  persistence.rs                ← reasoning persistence
```

**Reuse**:
- `touring_intelligence::reasoning::cognitive_mcts::CognitiveMCTS` — optimal remediation sequence
- `touring_intelligence::reasoning::coedit_predictor::CoEditPredictor` — "next file to audit" suggestions
- `touring_intelligence::reasoning::mcts::MCTSEngine` — generic MCTS for any sequential task

### 0.6 `touring-cortex::signal_fusion` — Bayesian fusion

```rust
// From touring-cortex/src/signal_fusion.rs:
pub struct HandlerSignal {
    pub handler_name: String,
    pub estimate: f64,    // 0.0-1.0
    pub confidence: f64,  // 0.0-1.0
}

pub struct FusedSignal {
    pub fused_estimate: f64,   // confidence-weighted mean
    pub fused_confidence: f64,
    pub disagreement_cv: f64,  // coefficient of variation
    pub signal_count: usize,
    pub is_high_confidence: bool,  // confidence > 0.7 AND cv < 0.3
}

// Reuse: fuse quality score from CodeStandardsEnforcer + PostQualityGate
//        + touring_quality::score_target into a single composite
```

### 0.7 `touring-cortex::rl_mapping` — RL state/action

```rust
// Bridge handler events to RL state/action for touring-intelligence::rl
// Reuse: post_write handler emits RL reward via this bridge
```

---

## 1. Master Plan v3 (refinado)

> **Touring terá UM harness unificado**, com **FULL REUSE** de:
> - **touring-cortex::handlers::quality** (3 sub-handlers: CodeStandardsEnforcer, PostQualityGate, ComplianceCollector) → estender, não criar
> - **touring-cortex::signal_fusion** (Bayesian) → agregar multi-dim
> - **touring-cortex::rl_mapping** (handler → RL bridge)
> - **touring-cortex::call_graph** (Tarjan SCC) → F1.8 dep_cycles
> - **touring-code::ast::quality** (14 langs, Wilson CI) → fast-path F1.x
> - **touring-code::ast::graph** (cycles, blast_radius, method_calls)
> - **touring-code::semantics** (Definition, source_to_def) → LSP
> - **touring-foundation::schema** (3-domain DB)
> - **touring-foundation::semantic** (22 SemanticClass)
> - **touring-foundation::conflict** (3-tier detection)
> - **touring-intelligence::reasoning::cognitive_mcts** (optimal remediation)
> - **touring-intelligence::reasoning::coedit_predictor** (next file)
>
> **APENAS 2 arquivos genuinamente novos**:
> 1. `touring-quality/src/gates.rs` (Q6: GateId + 50→17 rollup)
> 2. `touring-lsp/src/quality_diagnostics.rs` (1 file, LSP adapter)

---

## 2. Topologia Pós-Consolidação (v3 — REUSE em destaque)

```
                          ┌────────────────────────────────────┐
                          │   touring-cli                      │
                          │   $ touring quality <sub> ...      │
                          └─────────────────┬──────────────────┘
                                            │
              ┌─────────────────────────────┼─────────────────────────────┐
              │                             │                             │
      ┌───────▼─────────┐          ┌────────▼─────────┐           ┌────────▼──────────┐
      │ touring-server  │          │ touring-quality  │           │ touring-quality   │
      │ (MCP)           │          │ (UNIFIED HOME)   │           │ (bin: standalone) │
      │ 90+ tools + 5   │          │                  │           │                   │
      │ elite (migrate) │          │ verifications/   │           │ quality <sub>...  │
      └───────┬─────────┘          │ gates.rs [NEW]   │           └─────────┬─────────┘
              │                    │ change.rs [MOVED]│                     │
              │                    │ history.rs [MOVED]│                     │
              │                    │ report.rs [MOVED] │                     │
              │                    │ runner.rs [MOVED] │                     │
              │                    │ composite.rs      │                     │
              │                    │ tier.rs           │                     │
              │                    │ aggregate.rs      │                     │
              │                    └────────┬─────────┘                     │
              │                             │                               │
              │                             │ uses                          │
              │                             ▼                               │
              │                    ┌────────────────────┐                  │
              │                    │ touring-analysis    │                  │
              │                    │ (engine layer)     │                  │
              │                    │ 50 dim engines     │                  │
              │                    │ polyglot           │                  │
              │                    └─────────┬──────────┘                  │
              │                              │                              │
              │                              │ uses                        │
              │                              ▼                              │
              │                    ┌────────────────────┐                  │
              │                    │ touring-code        │                  │
              │                    │ (ast/graph)         │◄─────────────────┘
              │                    │ quality (14 langs)  │
              │                    │ graph (cycles, br)  │
              │                    │ semantics (def)    │
              │                    └─────────┬──────────┘
              │                              │
              │                    ┌─────────▼──────────┐
              │                    │ touring-foundation │
              │                    │ schema (3-domain)  │
              │                    │ semantic (22 class)│
              │                    │ conflict (3-tier)  │
              │                    └─────────┬──────────┘
              │                              │
              │         ┌────────────────────┴────────────────────┐
              │         │                                         │
              │   ┌─────▼──────────┐                  ┌──────────▼──────────┐
              │   │ touring-cortex │                  │ touring-ceg         │
              │   │ (HOOK ENGINE)  │                  │ (SANDBOX)           │
              │   │                │                  │                     │
              │   │ handlers/      │                  │ X0..X9 pipeline     │
              │   │   quality [EXT]│                  │ X7 composite:       │
              │   │   learning[EXT]│                  │   W_QUALITY=0.20    │
              │   │   call_graph   │                  │   W_STATIC=0.20     │
              │   │   signal_fusion│                  │   W_VGP=0.15        │
              │   │   rl_mapping   │                  │   W_PREDICT=0.10    │
              │   │   lifecycle    │                  │   W_SANDBOX=0.15    │
              │   │   ...          │                  │   W_GATE=0.20       │
              │   └────────────────┘                  └─────────────────────┘
              │         │
              │         │ uses
              │         ▼
              │   ┌────────────────────┐
              │   │ touring-intelligence│
              │   │ reasoning/          │
              │   │   cognitive_mcts    │ ◄── optimal remediation sequence
              │   │   coedit_predictor  │ ◄── next file to audit
              │   │   mcts              │ ◄── generic MCTS
              │   │ rl/                 │
              │   │   streaming_hook    │ ◄── hook → RL bridge
              │   │   learning_signals  │ ◄── emit_advantage/td_error
              │   │   online_rl         │ ◄── O(1) per-tool reward
              │   └────────────────────┘
              │
              └─── uses touring-cortex (MCP server runs the hook engine)

DELETED (after W5):
  ✗ crates/touring-harness/         (Q1: dissolve into touring-quality)
  ✗ crates/touring-harness-mcp/     (Q2: tools migrated to touring-server)
  ✗ target/release/touring-elite    (Q1)
  ✗ target/release/touring-harness-mcp (Q2)
```

---

## 3. Wave 6 — REESCRITA (EXTEND cortex quality handlers)

### W6 — Estender `touring-cortex::handlers::quality`

**Goal**: REUSE `CodeStandardsEnforcer`, `PostQualityGate`, `ComplianceCollector`. Adicionar 50-dim scoring via `touring_quality::score_target` como **novo signal** dentro dos handlers existentes.

| Task | Subtask | File | CHANGE |
|------|---------|------|--------|
| W6.T1 | W6.T1.1 | `crates/touring-cortex/Cargo.toml` | add `touring-quality = { path = "../touring-quality" }` |
| W6.T1 | W6.T1.2 | `crates/touring-cortex/src/handlers/quality.rs` | **EXTEND `CodeStandardsEnforcerHandler::execute`** with quality scoring step |
| W6.T1 | W6.T1.3 | `crates/touring-cortex/src/handlers/quality.rs` | call `touring_quality::score_target(content)` for proposed content |
| W6.T1 | W6.T1.4 | `crates/touring-cortex/src/handlers/quality.rs` | if composite < 0.80: add to issues list |
| W6.T1 | W6.T1.5 | `crates/touring-cortex/src/handlers/quality.rs` | if any P0 BLOCK dim fails: upgrade to HandlerResult::block |
| W6.T1 | W6.T1.6 | `crates/touring-cortex/src/handlers/quality.rs` (tests) | add test: P0 BLOCK triggers block (5 new tests) |
| W6.T2 | W6.T2.1 | `crates/touring-cortex/src/handlers/quality.rs` | **EXTEND `PostQualityGateHandler::execute`** |
| W6.T2 | W6.T2.2 | `crates/touring-cortex/src/handlers/quality.rs` | call `touring_quality::score_target(file_path)` after write |
| W6.T2 | W6.T2.3 | `crates/touring-cortex/src/handlers/quality.rs` | compute diff: pre-write score vs post-write score |
| W6.T2 | W6.T2.4 | `crates/touring-cortex/src/handlers/quality.rs` | use `touring_cortex::signal_fusion::fuse_signals` to merge CodeStandards + PostQuality + quality-50dim |
| W6.T2 | W6.T2.5 | `crates/touring-cortex/src/handlers/quality.rs` | append fused signal to `QGate[file]` summary |
| W6.T2 | W6.T2.6 | `crates/touring-cortex/src/handlers/quality.rs` | **emit RL reward** via `touring_cortex::rl_mapping::allocate_budget_by_qvalue` |
| W6.T3 | W6.T3.1 | `crates/touring-cortex/src/handlers/quality.rs` | **EXTEND `ComplianceCollectorHandler`** with 50-dim composite |
| W6.T3 | W6.T3.2 | `crates/touring-cortex/src/handlers/quality.rs` | log `composite_score` + `gate_scores[17]` to compliance.jsonl |
| W6.T4 | W6.T4.1 | `crates/touring-cortex/src/handlers/learning.rs` | **EXTEND** PostToolRL handler with quality-improvement reward |
| W6.T4 | W6.T4.2 | `crates/touring-cortex/src/handlers/learning.rs` | if dim improved: `emit_advantage(advantage: +0.1)` |
| W6.T4 | W6.T4.3 | `crates/touring-cortex/src/handlers/learning.rs` | if dim regressed: `emit_td_error(td_error: -0.1)` |
| W6.T4 | W6.T4.4 | `crates/touring-cortex/src/handlers/learning.rs` | use `touring_intelligence::rl::online_rl::process_reward` for O(1) per-tool reward |
| W6.T5 | W6.T5.1 | `crates/touring-cortex/src/handlers/quality.rs` | use `touring_intelligence::reasoning::coedit_predictor` to suggest "next file to audit" |
| W6.T5 | W6.T5.2 | `crates/touring-cortex/src/handlers/quality.rs` | use `touring_foundation::conflict::GraphImpactDetector` for before/after diff |
| W6.T6 | W6.T6.1 | `crates/touring-lsp/src/quality_diagnostics.rs` (NEW file, smallest) | `QualityDiagnostics::from_dim_score(score) -> Vec<lsp_types::Diagnostic>` |
| W6.T6 | W6.T6.2 | `crates/touring-lsp/src/server.rs` | on save/change: re-score and publish diagnostics |
| W6.T6 | W6.T6.3 | `crates/touring-lsp/src/severity.rs` | map DimStatus → LSP Severity |
| W6.T6 | W6.T6.4 | `crates/touring-lsp/src/server.rs` | on go-to-def: use `touring_code::semantics::source_to_definition` |
| W6.T7 | — | `cargo test --workspace` | 0 fail |
| W6.T8 | — | `cargo clippy --workspace --all-targets -- -D warnings` | 0 warnings |
| W6.T9 | — | end-to-end: write a file, verify cortex quality handler blocks at composite < 0.80 | demo: PASS |
| W6.T10 | — | end-to-end: LSP diagnostics appear in editor on save | demo: PASS |

**Change Log vs v2**:

| v2 | v3 (this) | Reason |
|----|----------|--------|
| Extend `touring-hook-handlers::hooks::pre_write.rs` | Extend `touring-cortex::handlers::quality::CodeStandardsEnforcer` | cortex is the central hook engine; quality handler is already registered; moka cache + persistence + drift already in place |
| Extend `touring-hook-handlers::hooks::post_write.rs` | Extend `touring-cortex::handlers::quality::PostQualityGate` | same |
| Create history.jsonl | Extend `touring-cortex::handlers::quality::ComplianceCollector` | compliance.jsonl + Wilson already in place |
| 1 new LSP file | SAME (1 new file, smallest) | LSP is genuinely new |
| 11 tasks / 20 subtasks | **7 tasks / 10 subtasks** | simpler, less code |

---

## 4. Decisões de Arquitetura (atualizadas v3)

### 4.1 Onde mora o Pre/Post-write quality check

**v1**: criar `touring-hooks/src/quality_pre_write.rs`
**v2**: estender `touring-hook-handlers::hooks::pre_write.rs`
**v3**: **estender `touring-cortex::handlers::quality::CodeStandardsEnforcer`** (que JÁ EXISTE)

Razão: `touring-cortex` é o **engine central de hooks** (84+ handlers). Seu quality handler já tem:
- moka W-TinyLFU cache (10K entries)
- `ctx.persistence.log_hook_event` → `graph.db` (3-domain DB)
- `drift_record` → compliance tracking
- Wilson confidence via `touring_simd::WilsonRanker`
- `ctx.context_budget_remaining` skip
- Proper async/sync semantics

### 4.2 Como o F1.8 dep_cycles é implementado

**v1**: `touring-analysis::f1_8_dep_cycles` (Touring Tarjan SCC)
**v2**: mesmo
**v3**: **fast-path** via `touring_code::ast::graph::cycles::SymbolIndex::detect_cycles()` (DFS 3-color, O(V+E)) + **backstop** via `touring_cortex::call_graph::CallGraph` (Tarjan SCC + hotspots). Ambos disponíveis; escolha conforme requisito de complexidade.

### 4.3 Como o F1.7 boundaries é implementado

**v1**: `touring-analysis::f1_7_boundaries` (pub-surface)
**v2**: mesmo
**v3**: **REUSE** `touring_code::ast::graph::blast_radius::BlastRadius` (BFS + HNSW ANN). F1.7 vai contar `pub` symbols por módulo; blast_radius dá o grafo de propagação.

### 4.4 Como o F1.10 data_model é implementado

**v1**: `touring-analysis::f1_10_data_model` (7-lang polyglot)
**v2**: mesmo
**v3**: **fast-path** via `touring_code::ast::quality::analyze_quality(source, lang)` (14 langs, Wilson CI) + **AST-deep** via `touring-analysis::f1_10_data_model` (illegal-states + primitive-obsession 7 langs). O fast-path serve 14 linguagens; o deep serve 7.

### 4.5 Como o ciclo before/after diff em post_write

**v1**: custom impl
**v2**: idem
**v3**: **REUSE** `touring_foundation::conflict::GraphImpactDetector` (3-tier: AstDiff < 100ms, Semantic < 1s, GraphImpact < 5s). Já tem SLA tracking via governor pattern.

### 4.6 Como o LSP go-to-def é implementado

**v1**: custom impl
**v2**: idem
**v3**: **REUSE** `touring_code::semantics::source_to_definition` (tree-sitter, 14 langs). Já tem Definition enum + multi_lang mapping.

### 4.7 Como o signal fusion multi-dim é feito

**v1**: custom impl
**v2**: `touring_quality::gates::aggregate_to_gates` (média ponderada simples)
**v3**: **BOTH**:
- `touring_cortex::signal_fusion::fuse_signals` para combinar 3+ handler signals (CodeStandards + PostQuality + 50-dim composite) via Bayesian
- `touring_quality::gates::aggregate_to_gates` para o rollup 50→17

### 4.8 Como o optimal remediation sequence é calculado

**v1**: custom impl
**v2**: idem
**v3**: **REUSE** `touring_intelligence::reasoning::cognitive_mcts::CognitiveMCTS` (graph-informed MCTS com pheromone). Quando o user roda `touring quality fix`, MCTS busca a sequência ótima de fixes que minimiza regressão.

### 4.9 Como o "next file to audit" é sugerido

**v1**: custom impl
**v2**: idem
**v3**: **REUSE** `touring_intelligence::reasoning::coedit_predictor::CoEditPredictor` (RRF de 3 sinais: co-edit history + import graph + blast radius). Quando o user audita file A, predictor sugere files B/C/D que provavelmente também têm issues.

### 4.10 Como o F1.11 design patterns é implementado

**v1**: `touring-analysis::f1_11_patterns` (GoF + ownership)
**v2**: mesmo
**v3**: **REUSE** `touring_foundation::semantic::SemanticClassifier` (22 SemanticClass categories via data-driven universal_rules.json). Adiciona 22 classes ao F1.11 (que antes detectava ~10 patterns).

---

## 5. Tasks Update — Diff v2 → v3

| Wave | v2 tasks | v3 tasks | Delta |
|------|----------|----------|-------|
| W0 | 5 | 5 | unchanged |
| W1 | 5 | 5 | unchanged |
| W2 | 6 | 6 | unchanged |
| W3 | 7 | 7 | unchanged |
| W4 | 6 | 6 | unchanged |
| W5 | 7 | 7 | unchanged |
| **W6** | **11** | **10** | **-1** (cortex handlers são mais coesos) |
| W7 | 10 | 10 | unchanged |
| **Total** | **57** | **55** | **-2** |

---

## 6. Acceptance Final v3 (atualizado)

| Item | Acceptance |
|------|------------|
| **1 unified harness** | `touring-quality` é a casa. `touring-harness` e `touring-harness-mcp` deletados. |
| **50 dim engines** | 50 engines reais em `touring-analysis/src/quality/`. |
| **17 gates via rollup** | `touring-quality/src/gates.rs::aggregate_to_gates`. |
| **Single composite** | 50-dim weighted avg. |
| **Unified CLI** | `touring quality <sub>` com 15 subcomandos. |
| **MCP surface** | 5 tools em `touring-server`. |
| **CEG X7 integration** | W_QUALITY=0.20. 6 sinais. |
| **Hooks REUSED via cortex** | `touring-cortex::handlers::quality` (H51/H52/H53) estendidos |
| **RL bridge** | `touring_cortex::rl_mapping` + `touring_intelligence::rl::streaming_hook_integration` |
| **Signal fusion** | `touring_cortex::signal_fusion::fuse_signals` |
| **LSP** | `touring-lsp/src/quality_diagnostics.rs` + `touring_code::semantics` |
| **Cycle detection** | `touring_code::ast::graph::cycles` (DFS) + `touring_cortex::call_graph` (Tarjan) |
| **Blast radius** | `touring_code::ast::graph::blast_radius` (BFS + HNSW) |
| **Before/after diff** | `touring_foundation::conflict::GraphImpactDetector` (3-tier) |
| **Multi-lang quality** | `touring_code::ast::quality` (14 langs, Wilson CI) |
| **Pattern classification** | `touring_foundation::semantic::SemanticClassifier` (22 classes) |
| **Optimal remediation** | `touring_intelligence::reasoning::cognitive_mcts` |
| **Co-edit predict** | `touring_intelligence::reasoning::coedit_predictor` |
| **Schema** | `touring_foundation::schema` (3-domain DB, SCHEMA_V8) |
| **Diamond tier** | 50/50 dims em Diamond no workspace |
| **Tests** | 0 fail. 0 warnings. 0 BLOCK violations. |

---

## 7. Plano de Execução (v3)

```
W0 (baseline) → W1 (move Change/History/Report) → W2 (gates.rs + delete 14 stubs)
                                          ↓
                              W3 (CLI + 5 tools) → W4 (CEG X7) → W5 (delete crates)
                                                                 ↓
                                                    W6 (EXTEND cortex quality + LSP)
                                                                 ↓
                                                              W7 (Diamond acceptance)
```

---

## 8. QuickAction Card v3

```
╔══════════════════════════════════════════════════════════════════╗
║  HARNESS CONSOLIDATION — MASTER PLAN v3.0 (FULL REUSE)         ║
║  Strategy: ESTENDER infra existente (não criar nova)           ║
╠══════════════════════════════════════════════════════════════════╣
║  W0 (NOW): baseline (5 tasks, 15-25 min)                        ║
║  W1: foundation migration (Change/History/Report → quality)    ║
║  W2: gates.rs + 14 stubs deleted + single composite             ║
║  W3: `touring quality` CLI + 5 tools → touring-server            ║
║  W4: CEG X7 W_QUALITY=0.20                                     ║
║  W5: delete touring-harness + touring-harness-mcp              ║
║  W6: EXTEND cortex::handlers::quality + LSP diagnostics        ║
║  W7: 50/50 Diamond acceptance                                   ║
╠══════════════════════════════════════════════════════════════════╣
║  REUSED (NO new code — extend only):                           ║
║   - touring-cortex::handlers::quality (H51/H52/H53)           ║
║   - touring-cortex::signal_fusion (Bayesian)                    ║
║   - touring-cortex::rl_mapping (handler → RL)                  ║
║   - touring-cortex::call_graph (Tarjan SCC + hotspots)          ║
║   - touring-code::ast::quality (14 langs + Wilson)              ║
║   - touring-code::ast::graph/{cycles,blast_radius,method_calls}║
║   - touring-code::semantics (Definition, source_to_def)         ║
║   - touring-foundation::schema (3-domain DB, V8)                ║
║   - touring-foundation::semantic (22 SemanticClass)             ║
║   - touring-foundation::conflict (3-tier AstDiff/Sem/Graph)    ║
║   - touring-intelligence::reasoning::cognitive_mcts             ║
║   - touring-intelligence::reasoning::coedit_predictor           ║
║   - touring-intelligence::rl::streaming_hook_integration        ║
║   - touring-intelligence::rl::learning_signals                  ║
║   - touring-intelligence::rl::online_rl                         ║
║   - touring-orchestration::tasks::template_engine               ║
║                                                                ║
║  NEW (only 2 files):                                            ║
║   - touring-quality/src/gates.rs (Q6)                          ║
║   - touring-lsp/src/quality_diagnostics.rs (1 file, smallest)  ║
╚══════════════════════════════════════════════════════════════════╝
```

---

**Aguardando aprovação de Gabriel** para iniciar W0 (QuickAction). O plano está completo e FULL-REUSE; nenhuma modificação foi executada ainda.