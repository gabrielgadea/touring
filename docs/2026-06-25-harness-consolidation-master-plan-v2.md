# Master Plan v2.0 — Consolidação do Harness (REUSE-FIRST)

**Data**: 2026-06-25
**Autor**: TACO (Touring Agentic Code Orchestrator)
**Status**: ⏸️ PLANEJAMENTO v2.0 (nenhuma modificação executada)
**Supersedes**: `2026-06-25-harness-consolidation-master-plan.md` (v1.0)
**Mudança principal**: **REUSE-FIRST** — ao invés de criar novos hooks pre/post, **estender os hooks existentes** + reaproveitar infraestrutura (touring-intelligence::rl, touring-offensive, touring-orchestration, touring-hook-handlers).

---

## TL;DR das Mudanças v1 → v2

| Item | v1.0 (criar novo) | **v2.0 (REUSE)** |
|------|------------------|------------------|
| **Pre-write hook** | Criar novo `touring-hooks/src/quality_pre_write.rs` | **Estender** `touring-hook-handlers/src/hooks/pre_write.rs` (já existe, 300+ LOC) |
| **Post-write hook** | Criar novo `touring-hooks/src/quality_post_write.rs` | **Estender** `touring-hook-handlers/src/hooks/post_write.rs` (já existe, 400+ LOC) |
| **RL bridge** | Criar novo `touring_quality::rl` | **Reusar** `touring-intelligence::rl::streaming_hook_integration::HookStatsConsumer` (já existe) |
| **RL reward** | Criar novo `learning_signals` | **Reusar** `touring-intelligence::rl::learning_signals::emit_advantage` (já existe) |
| **Online learning** | Criar novo `online_rl` | **Reusar** `touring-intelligence::rl::online_rl::process_reward` (já existe) |
| **Score history** | Criar `~/.claude/touring/quality-history.jsonl` | **Reusar** `touring-harness::history::ScoreHistory` (movido para touring-quality em W1) |
| **Workflow template engine** | Criar novo | **Reusar** `touring-orchestration::tasks::template_engine` (Tera, já existe) |
| **NLP argument mining** | Criar novo | **Reusar** `touring-offensive::erickson` (já existe, Claims/Evidence/Warrant) |
| **CWE patterns** | Criar novo | **Reusar** `touring-offensive::vuln::cwe_patterns` (já existe, 10 CWE detectors) |
| **Event registry** | Criar novo | **Reusar** `touring-dispatch::hook_registry::build_dispatch_table` (já existe, 100+ hooks) |
| **Tasksfile DSL** | Criar novo | **Reusar** `touring-orchestration::tasks::TasksfileCompiler` (já existe) |

**Net effect**: W6 reduziu de **6 tasks / 9 subtasks** para **5 tasks / 11 subtasks** (mais granular mas com menos código novo).

---

## 0. Inventário da Infraestrutura Existente (Descoberta via Diagnóstico)

### 0.1 `touring-hooks` — Façade + 3 crates reais

```
touring-hooks/                    ← thin façade, re-exporta de touring-dispatch
touring-dispatch/                 ← hook_registry, dispatch table, daemon, lifecycle
touring-hooks-core/               ← engines: knowledge, search, health, bridges, session, ACO
touring-hook-handlers/            ← TODOS os 33+ hooks (pre_write, post_write, post_tool_rl, ...)
touring-hooks-shared/             ← CILA, hook_events, signal_pipeline
touring-hooks-prediction/         ← classifier, PII, llm_judge, layer7_prediction
touring-hook-runtime/             ← HookRuntime, HookDispatchError, triad_hook
```

**Reuse**: **`touring-hook-handlers/src/hooks/{pre,post}_write.rs`** (já implementados, com speculative validation, anti-pattern, wiring prediction, block gate). Só **estender** para incluir `touring_quality::score_target` como uma das validações.

### 0.2 `touring-intelligence` — RL + Reasoning + ANN + Index

```
touring-intelligence/
  rl/                    ← QTable, LinUCB, bandit, evolution, semantic, online_rl,
                           rl/, streaming_hook_integration, learning_signals,
                           metacognitive_pipeline, online_learning, bandit
  reasoning/             ← MCTS, GoT, ACO, pensieve, BM25, session graph
  ann/                   ← ANN index, reranker, semantic chunker
  index/                 ← incremental symbol indexing, file watcher
```

**API crítica para integração**:

| Struct/Function | Path | Purpose |
|-----------------|------|---------|
| `HookStatsConsumer` (trait) | `rl::streaming_hook_integration` | Bridge hook → RL subsystems |
| `HookQualitySummary` | `rl::streaming_hook_integration` | DTO com 11 dim scores (composite, precision, coverage, etc.) |
| `emit_advantage(ActorAdvantage)` | `rl::learning_signals` | Policy gradient update |
| `emit_td_error(TdErrorSignal)` | `rl::learning_signals` | Value baseline update |
| `OnlineRLEngine::process_reward(ImmediateReward)` | `rl::online_rl` | O(1) per-tool reward |
| `MCTS` | `reasoning::mcts` | Optimal remediation sequence |

### 0.3 `touring-offensive` — NLP + CWE + Solvers

```
touring-offensive/
  bug_bounty.rs          ← CVE tracker + CVSS scoring
  concolic.rs + solver/  ← z3/cvc5 symbolic execution
  erickson/              ← Claims/Evidence/Warrant NLP markers
  vuln/cwe_patterns.rs   ← 10 CWE detectors (SQLi, XSS, CMDi, ...)
```

**Reuse para harness**:
- `touring_offensive::vuln::cwe_patterns::SqlInjectionPattern` etc. — **já integrado** em `touring_quality::f2_1_owasp` via `SecurityAnalyzer`
- `touring_offensive::erickson::extract(text)` → pode detectar Claims em **doc comments** (F3.8 enhancement) e **upgrade recommendations** em comentários (F4.4 modernization heuristic)
- `touring_offensive::concolic` → path exploration (F2.1 deeper analysis, ADVISORY tier)

### 0.4 `touring-orchestration` — Tasks + Flow + Devrc

```
touring-orchestration/
  tasks/
    compiler.rs         ← Tasksfile YAML → CompiledTasksfile
    parser.rs           ← YAML parser + deps validation
    template_engine.rs  ← Tera template ({{ params }}, {{ secrets }})
    schema.rs           ← TasksfileRoot types
  flow/
    flow_pipeline.rs    ← dataflow pipeline runner
    stages.rs           ← pipeline stages
  devrc/
    converter.rs        ← Devrcfile adapter
```

**Reuse**:
- `touring_orchestration::tasks::template_engine::render(template, ctx)` → output format para `touring quality fix <dim> <file>` (gera script de remediação formatado)
- `touring_orchestration::tasks::TasksfileCompiler` → compilar `quality-check.tasks.yml` em workflow determinístico (Q de Gabriel: workflow combinado com ast/graph/lsp/hooks)
- `touring_orchestration::flow::FlowPipeline` → pipeline: `ast → graph → lsp → hooks → history` (sinal de qualidade propagando pelos estágios)

---

## 1. Master Plan (refinado)

> **Touring terá UM harness unificado**, organizado em **3 subsistemas com responsabilidade clara** + **REUSE máximo** da infra existente:
>
> 1. **`touring-analysis`** (engine layer, 50 dim detectors reais) — **inalterado**
> 2. **`touring-quality`** (verifier + composite + gates + change + history + report) — **a casa unificada** do harness, com novo módulo `gates.rs` que faz o rollup 50→17
> 3. **`touring-ceg`** (sandbox, concern separado por design) — X7 DECISION passa a incluir o 6º sinal **W_QUALITY=0.20** do composite de 50 dim
>
> **APIs REUSADAS** (não-criadas):
> - **pre_write / post_write / post_tool_rl** hooks: estender (não criar)
> - **touring-intelligence::rl::streaming_hook_integration**: bridge hook → RL
> - **touring-intelligence::rl::learning_signals**: emit_advantage / emit_td_error
> - **touring-intelligence::rl::online_rl**: process_reward O(1)
> - **touring-offensive::erickson**: NLP markers em doc comments
> - **touring-offensive::vuln::cwe_patterns**: 10 CWE (já integrado em F2.1)
> - **touring-orchestration::tasks::template_engine**: Tera templates para `quality fix`
> - **touring-orchestration::flow::FlowPipeline**: pipeline ast → graph → hooks → history
> - **touring-dispatch::hook_registry::build_dispatch_table**: 1 novo event "quality-check"

---

## 2. Topologia Pós-Consolidação (refinada — REUSE em destaque)

```
                            ┌────────────────────────────────────────────┐
                            │       touring-cli  (shell entry point)     │
                            │   $ touring quality <sub> ...                │
                            └─────────────────┬──────────────────────────┘
                                          │
                  ┌───────────────────────┼─────────────────────────────────┐
                  │                       │                                 │
          ┌───────▼─────────┐      ┌───────▼──────────┐           ┌──────────▼──────────┐
          │ touring-server  │      │ touring-cli      │           │ touring-quality     │
          │ (MCP server)    │      │ (shell)          │           │ (bin: standalone   │
          │ 90+ tools + 5   │      │ quality <sub>    │           │  CLI too)           │
          │ elite tools     │      │                  │           │                     │
          │ (migrated from  │      │                  │           │ quality <sub>...    │
          │  harness-mcp)   │      │                  │           │                     │
          └───────┬─────────┘      └──────────────────┘           └─────────┬───────────┘
                  │                                                          │
                  │ uses score_target                                         │ exposes:
                  │ uses run_quality_pipeline                                 │ - score_target()
                  │                                                            │ - run_quality_pipeline()
                  │                                                            │ - aggregate_to_gates()
                  │                                                            │ - emit_report()
                  │                                                            │ - quality_history.jsonl
                  │                                                            │
                  └─────────────────────────────────────┬──────────────────────┘
                                                        │
                  ┌─────────────────────────────────────┼──────────────────────┐
                  │                                     │                      │
          ┌───────▼──────────┐                ┌──────────▼─────────┐  ┌─────────▼─────────┐
          │ touring-analysis │                │ touring-quality    │  │ touring-ceg       │
          │ (engine layer)  │  uses analyze_X │ (UNIFIED HOME)     │  │ (SANDBOX)         │
          │                 │ ──────────────> │                   │  │                   │
          │ 50 dim engines  │  uses score_X    │ verifications/    │  │ X0..X9 pipeline  │
          │ polyglot        │                  │ gates.rs [NEW]    │  │ X7 composite:     │
          └─────────────────┘                  │ composite.rs       │  │   W_QUALITY=0.20  │
                                              │ aggregate.rs       │  │   W_STATIC=0.20   │
                                              │ change.rs    [MOVED]│  │   W_VGP=0.15      │
                                              │ history.rs   [MOVED]│  │   W_PREDICT=0.10 │
                                              │ report.rs    [MOVED]│  │   W_SANDBOX=0.15  │
                                              │ runner.rs    [MOVED]│  │   W_GATE=0.20     │
                                              │ tier.rs             │  └───────────────────┘
                                              └────────────────────┘
                                                            ▲
                                                            │
                          ┌─────────────────────────────────┴────────────────────────┐
                          │   EXISTING INFRA REUSED (NO NEW CODE HERE)              │
                          │                                                          │
                          │  ┌──────────────────────────────────────────────────┐  │
                          │  │ touring-hook-handlers/src/hooks/                  │  │
                          │  │   - pre_write.rs (extend with quality score)    │  │
                          │  │   - post_write.rs (extend with HookQualitySummary)│  │
                          │  │   - post_tool_rl.rs (extend with rewards)        │  │
                          │  └──────────────────────────────────────────────────┘  │
                          │                                                          │
                          │  ┌──────────────────────────────────────────────────┐  │
                          │  │ touring-intelligence::rl                        │  │
                          │  │   - streaming_hook_integration (HookStatsConsumer)│  │
                          │  │   - learning_signals (emit_advantage/td_error)  │  │
                          │  │   - online_rl (process_reward O(1))              │  │
                          │  │   - reasoning::mcts (remediation sequence)       │  │
                          │  └──────────────────────────────────────────────────┘  │
                          │                                                          │
                          │  ┌──────────────────────────────────────────────────┐  │
                          │  │ touring-offensive                                │  │
                          │  │   - erickson (NLP markers for doc comments)      │  │
                          │  │   - vuln::cwe_patterns (10 CWE detectors)        │  │
                          │  └──────────────────────────────────────────────────┘  │
                          │                                                          │
                          │  ┌──────────────────────────────────────────────────┐  │
                          │  │ touring-orchestration                            │  │
                          │  │   - tasks::template_engine (Tera for fix output) │  │
                          │  │   - tasks::TasksfileCompiler (workflow DSL)      │  │
                          │  │   - flow::FlowPipeline (dataflow propagation)    │  │
                          │  └──────────────────────────────────────────────────┘  │
                          │                                                          │
                          │  ┌──────────────────────────────────────────────────┐  │
                          │  │ touring-dispatch::hook_registry                  │  │
                          │  │   - build_dispatch_table (add "quality-check")  │  │
                          │  └──────────────────────────────────────────────────┘  │
                          └──────────────────────────────────────────────────────────┘

DELETED (after W5):
  ✗ crates/touring-harness/         (Q1: dissolve into touring-quality)
  ✗ crates/touring-harness-mcp/     (Q2: tools migrated to touring-server)
  ✗ target/release/touring-elite    (Q1: standalone binário)
  ✗ target/release/touring-harness-mcp (Q2: daemon MCP)
```

---

## 3. QuickAction Card (atualizado)

```
╔══════════════════════════════════════════════════════════════════╗
║  HARNESS CONSOLIDATION — MASTER PLAN v2.0 (REUSE-FIRST)        ║
║  Goal: 1 unified harness. 50 dim. 17 gates. Diamond tier.      ║
║  Strategy: ESTENDER infra existente (não criar nova)           ║
╠══════════════════════════════════════════════════════════════════╣
║  W0 (NOW): baseline (5 tasks, 15-25 min)                        ║
║  W1: foundation migration (Change/History/Report → quality)    ║
║  W2: gates.rs + 14 stubs deleted + single composite             ║
║  W3: `touring quality` CLI + 5 tools → touring-server            ║
║  W4: CEG X7 W_QUALITY=0.20                                     ║
║  W5: delete touring-harness + touring-harness-mcp              ║
║  W6: EXTEND existing hooks + wire RL bridge (reuse infra)      ║
║  W7: 50/50 Diamond acceptance                                   ║
╠══════════════════════════════════════════════════════════════════╣
║  Pre-flight:  cargo check --workspace && touring doctor -j      ║
║  Acceptance: cargo test --workspace + clippy + diamond score   ║
║  Quality:    50 dims Diamond. P0 BLOCK fail-closed.             ║
╚══════════════════════════════════════════════════════════════════╝
```

---

## 4. Wave 6 — REESCRITA (EXTEND ao invés de CREATE)

### W6 — Estender Hooks Existentes + Wire RL Bridge

**Goal**: REUSE os hooks existentes em `touring-hook-handlers`. Adicionar harness signals via **extensão cirúrgica** (não criar novos handlers). Bridge para RL via `streaming_hook_integration` (já existe).

| Task | Subtask | File | CHANGE vs v1 |
|------|---------|------|--------------|
| W6.T1 | W6.T1.1 | `crates/touring-hook-handlers/Cargo.toml` | add `touring-quality = { path = "../touring-quality" }` |
| W6.T1 | W6.T1.2 | `crates/touring-hook-handlers/src/hooks/pre_write.rs` | **EXTEND** `run_returning` with quality scoring step (not create new file) |
| W6.T1 | W6.T1.3 | `crates/touring-hook-handlers/src/hooks/pre_write.rs` | call `touring_quality::score_target(content)` for proposed content |
| W6.T1 | W6.T1.4 | `crates/touring-hook-handlers/src/hooks/pre_write.rs` | if composite < 0.80: add issue to issues list |
| W6.T1 | W6.T1.5 | `crates/touring-hook-handlers/src/hooks/pre_write.rs` | if any P0 BLOCK dim fails: escalate to HookResponse::Deny |
| W6.T1 | W6.T1.6 | `crates/touring-hook-handlers/src/hooks/pre_write_tests.rs` | add test: P0 BLOCK triggers Deny (5 new tests) |
| W6.T2 | W6.T2.1 | `crates/touring-hook-handlers/src/hooks/post_write.rs` | **EXTEND** with `HookQualitySummary::from_dims(...)` |
| W6.T2 | W6.T2.2 | `crates/touring-hook-handlers/src/hooks/post_write.rs` | call `HookStatsConsumer::consume_hook_quality(summary)` for RL feed |
| W6.T2 | W6.T2.3 | `crates/touring-hook-handlers/src/hooks/post_write.rs` | dim improvement → `emit_advantage(ActorAdvantage { advantage: +0.1 })` |
| W6.T2 | W6.T2.4 | `crates/touring-hook-handlers/src/hooks/post_write.rs` | dim regression → `emit_td_error(TdErrorSignal { td_error: -0.1 })` |
| W6.T2 | W6.T2.5 | `crates/touring-hook-handlers/src/hooks/post_write.rs` | **REUSE** `touring_intelligence::rl::online_rl::process_reward` |
| W6.T2 | W6.T2.6 | `crates/touring-hook-handlers/src/hooks/post_write.rs` | fail-open if touring-quality / RL unavailable (graceful degradation) |
| W6.T3 | W6.T3.1 | `crates/touring-dispatch/src/hook_registry.rs` | **REUSE** `build_dispatch_table` — add NEW entry `"quality-check"` |
| W6.T3 | W6.T3.2 | `crates/touring-dispatch/src/hook_registry.rs` | quality-check handler invokes `touring_quality::score_target` on demand |
| W6.T4 | W6.T4.1 | `crates/touring-quality/src/fix/mod.rs` (NEW sub-module, not new crate) | use `touring_orchestration::tasks::template_engine` for output formatting |
| W6.T4 | W6.T4.2 | `crates/touring-quality/src/fix/templates/quality-fix-{dim}.tera` | Tera templates per dim (e.g., `quality-fix-F2.4.tera` for secrets) |
| W6.T4 | W6.T4.3 | `crates/touring-quality/src/fix/mod.rs` | `pub fn render_fix(dim_id: DimId, context: &HashMap) -> String` |
| W6.T5 | W6.T5.1 | `crates/touring-analysis/src/quality/f3_8_inline_doc.rs` | **REUSE** `touring_offensive::erickson::extract` on doc comments |
| W6.T5 | W6.T5.2 | `crates/touring-analysis/src/quality/f3_8_inline_doc.rs` | detect Claims/Evidence/Warrant markers as "documentation quality" signal |
| W6.T5 | W6.T5.3 | `crates/touring-analysis/src/quality/f4_4_modernization.rs` | detect "should upgrade X" Claims in code comments (heuristic) |
| W6.T5 | W6.T5.4 | `crates/touring-quality/src/verifications/f2_1_owasp.rs` | **REUSE** `touring_offensive::vuln::cwe_patterns` as FALLBACK for languages SecurityAnalyzer doesn't cover |
| W6.T5 | W6.T5.5 | `crates/touring-quality/src/verifications/f2_1_owasp.rs` | add `is_detector_own_source()` allowlist (already pattern in f2_4) |
| W6.T6 | W6.T6.1 | `crates/touring-lsp/src/quality_diagnostics.rs` (NEW file, smallest addition) | `QualityDiagnostics::from_dim_score(score) -> Vec<lsp_types::Diagnostic>` |
| W6.T6 | W6.T6.2 | `crates/touring-lsp/src/server.rs` | on save/change: re-score and publish diagnostics |
| W6.T6 | W6.T6.3 | `crates/touring-lsp/src/severity.rs` | map DimStatus → LSP Severity (Block→Error, Warn→Warning, Advisory→Hint) |
| W6.T7 | — | `cargo test --workspace` | 0 fail (NEW + existing tests pass) |
| W6.T8 | — | `cargo clippy --workspace --all-targets -- -D warnings` | 0 warnings |
| W6.T9 | — | end-to-end: write a file via Claude Code, verify post-write hook feeds RL via HookQualitySummary | demo: PASS |
| W6.T10 | — | end-to-end: LSP diagnostic appears in editor on save | demo: PASS |
| W6.T11 | — | end-to-end: `touring quality fix F2.4 src/foo.rs --dry-run` outputs Tera-rendered remediation | demo: PASS |

**Change Log vs v1**:

| v1 | v2 (this) | Reason |
|----|----------|--------|
| W6.T1 create `touring-hooks/src/quality_pre_write.rs` | W6.T1.2 EXTEND `touring-hook-handlers/src/hooks/pre_write.rs` | REGRA #0 — não duplicar |
| W6.T2 create `touring-hooks/src/quality_post_write.rs` | W6.T2.1 EXTEND `touring-hook-handlers/src/hooks/post_write.rs` | same |
| W6.T3 create LSP diagnostic publisher | W6.T6 create `quality_diagnostics.rs` adapter (1 file, smallest) | LSP is genuinely new |
| W6.T4 wire RL reward (custom impl) | W6.T2.3-4 REUSE `emit_advantage` / `emit_td_error` (já existe) | avoid duplication |
| W6.T5 add quality-history.jsonl watcher | REUSE `touring-harness::history::ScoreHistory` (movido W1) | single source |
| W6.T6 wire quality status | REUSE `HookStatsConsumer` trait (já existe) | bridge already wired |

---

## 5. Decisões de Arquitetura (atualizadas)

### 5.1 Onde mora o `Change` (conceito do antigo touring-harness)

**v1**: Mover Change/History/Report/Runner de touring-harness para touring-quality/src/{change,history,report,runner}.rs.
**v2**: SAME — confirmado em W1. Mas **re-exportado** em `touring-quality::history::ScoreHistory` para compat com qualquer consumidor existente.

### 5.2 Como o RL é alimentado

**v1**: Criar `touring_quality::rl` module com bridge custom.
**v2**: **REUSE** `touring-intelligence::rl::streaming_hook_integration::HookStatsConsumer::consume_hook_quality(HookQualitySummary)`. Bridge já existe, já wired para LinUCB + SelfOptimizer + RingBuffer.

```rust
// In post_write::run_returning (extended, not new):
use touring_intelligence::rl::streaming_hook_integration::{HookQualitySummary, HookStatsConsumer};

let summary = HookQualitySummary::from_dims(
    session_id,
    total_hooks_fired,
    avg_latency_ms,
    max_latency_ms,
    fast_hooks_ratio,
    precision_score,   // dim-aggregated
    coverage_score,    // dim-aggregated
    latency_score,
    knowledge_score,
    context_score,
    reliability_score,
    integration_score,
    security_score,
    evolution_score,
    composite_score,   // from touring_quality::score_target
);
// Already-existing bridge — just call it
touring_intelligence::rl::streaming_hook_integration::consume_hook_quality(summary);
```

### 5.3 Como o Erickon's NLP markers entram no F3.8 / F4.4

**v1**: Criar detector próprio.
**v2**: **REUSE** `touring_offensive::erickson::extract(text)` para detectar Claims/Evidence/Warrant em doc comments e code comments. Erikston's 5 patterns (Claim/Evidence/Warrant/Backing/Rebuttal) são heurísticas para o F3.8 inline doc e F4.4 modernization.

```rust
// In f3_8_inline_doc.rs (extended, not new):
use touring_offensive::erickson::{extract, NLPPattern};

fn detect_doc_comment_claims(doc_text: &str) -> usize {
    extract(doc_text).iter()
        .filter(|e| matches!(e.pattern, NLPPattern::Claim | NLPPattern::Evidence | NLPPattern::Warrant))
        .count()
}
```

### 5.4 Como o `touring quality fix` renderiza saída

**v1**: Tera templates criados manualmente.
**v2**: **REUSE** `touring_orchestration::tasks::template_engine::render(template, ctx)`. Templates `quality-fix-{dim}.tera` registrados uma vez. Output renderizado via `render(template, ctx)`.

```rust
// In touring-quality/src/fix/mod.rs (NEW submodule, not new crate):
use touring_orchestration::tasks::template_engine::render;

pub fn render_fix(dim_id: DimId, context: &HashMap<String, String>) -> Result<String> {
    let template_name = format!("quality-fix-{}.tera", dim_id.as_str());
    let template = TEMPLATES.get(&template_name).ok_or(Error::TemplateNotFound)?;
    render(template, context)
}
```

### 5.5 Onde mora o Tasksfile DSL para workflows de qualidade

**v1**: Workflows criados ad-hoc.
**v2**: **REUSE** `touring_orchestration::tasks::TasksfileCompiler`. Compilar `quality-check.tasks.yml` (declarativo) em `CompiledTasksfile` (executável). O usuário pode escrever:

```yaml
# quality-check.tasks.yml
version: "1.0"
metadata:
  name: "quality-check"
  description: "Run all 50 dims + 17 gates on a target"
tasks:
  check:
    command: "touring quality check {{ params.target }}"
    depends_on: []
  gate-rollup:
    command: "touring quality gate {{ params.target }}"
    depends_on: ["check"]
  fix:
    command: "touring quality fix {{ params.dim }} {{ params.target }}"
    depends_on: ["gate-rollup"]
hooks:
  pre: "touring quality check {{ params.target }} --fail-below 0.80"
```

E compilar com `TasksfileCompiler::compile()`.

---

## 6. Acceptance Final do Plano v2 (atualizado)

| Item | Acceptance |
|------|------------|
| **1 unified harness** | `touring-quality` é a casa. `touring-harness` e `touring-harness-mcp` deletados. |
| **50 dim engines** | 50 engines reais em `touring-analysis/src/quality/`. tp/fp-validados. |
| **17 gates via rollup** | `touring-quality/src/gates.rs::aggregate_to_gates` mapeia 50→17. |
| **Single composite** | 50-dim weighted avg (Q4). Sem paralelo. |
| **Unified CLI** | `touring quality <sub>` com 15 subcomandos. |
| **MCP surface** | 5 tools em `touring-server`. `touring-harness-mcp` deletado. |
| **CEG X7 integration** | W_QUALITY=0.20. 6 sinais. Fail-closed. |
| **Hooks REUSED** | `pre_write` + `post_write` + `post_tool_rl` estendidos (não criados) |
| **RL bridge** | `HookStatsConsumer::consume_hook_quality` + `emit_advantage/td_error` REUSED |
| **NLP enhancement** | F3.8 + F4.4 usam `touring_offensive::erickson::extract` |
| **Templates** | `touring_orchestration::tasks::template_engine` REUSED para `quality fix` |
| **LSP diagnostics** | `touring-lsp/src/quality_diagnostics.rs` (1 novo file, smallest) |
| **Score history** | `~/.claude/touring/quality-history.jsonl` (Q7) |
| **Diamond tier** | 50/50 dims em Diamond no workspace |
| **Tests** | 0 fail. 0 warnings. 0 BLOCK violations |

---

## 7. Tasks Update — Diff v1 → v2

| Wave | v1 tasks | v2 tasks | Delta |
|------|----------|----------|-------|
| W0 | 5 | 5 | unchanged |
| W1 | 5 | 5 | unchanged |
| W2 | 6 | 6 | unchanged |
| W3 | 7 | 7 | unchanged |
| W4 | 6 | 6 | unchanged |
| W5 | 7 | 7 | unchanged |
| **W6** | 6 | **11** | **+5** (granular: EXTEND vs CREATE) |
| W7 | 10 | 10 | unchanged |
| **Total** | 52 | **57** | +5 |

---

## 8. Riscos & Mitigações (atualizados)

| # | Risco | Severidade | Mitigação |
|---|-------|------------|-----------|
| R1 | Mudança de score durante transição (W2) | 🔴 HIGH | Flag `--harness-backend=50dim\|17gate` opt-in por 1 wave. Rollout gradual. |
| R2 | Regressão em tests (W2) | 🟡 MED | Rodar `cargo test --workspace` antes de cada mudança. |
| R3 | Dependência circular (W1) | 🟡 MED | **Mover `GateId` para `touring-quality/src/gates.rs`**. |
| **R4 (NEW)** | **touring-intelligence::rl import cycle** (touring-intelligence já importa touring-offensive via existing dep) | 🟡 MED | Verificar se `touring-hook-handlers` pode importar `touring-intelligence` sem cycle. Sim — `touring-intelligence` é leaf (não importa de `touring-hook-handlers`). |
| **R5 (NEW)** | **touring-orchestration::tasks::template_engine já tem deps próprias** (tera, etc.) | 🟢 LOW | Verificar feature flags. Se conflict, alternative: usar `format!()` simples. |
| **R6 (NEW)** | **touring-offensive::erickson pode ter FPs em comentários técnicos** | 🟡 MED | Manter como **ADVISORY tier** (não P0/WARN). Heurística, não BLOCK. |
| R7 | Rollback problemático (W5) | 🟢 LOW | Wave 5 é ÚLTIMO. Até então, 100% reversível. |

---

## 9. Plano de Execução (ordenado por dependência)

```
W0 → W1 → W2 → W3 → W4 → W5 → W6 → W7
         │         │              │
         │         │              └─ EXTEND pre_write / post_write / post_tool_rl
         │         │                 REUSE HookStatsConsumer, emit_advantage, erickson
         │         │                 REUSE template_engine
         │         │                 CREATE 1 file: touring-lsp/src/quality_diagnostics.rs
         │         │
         │         ├─ CLI `touring quality` + 5 tools → touring-server
         │         │
         │         ├─ CEG X7 W_QUALITY=0.20
         │         │
         │         ├─ Delete touring-harness + harness-mcp
         │         │
         │         └─ gates.rs + 14 stubs deleted
         │
         └─ Move Change/History/Report → touring-quality
```

---

## 10. QuickAction Card (v2 — REUSE-FIRST)

```
╔══════════════════════════════════════════════════════════════════╗
║  HARNESS CONSOLIDATION — MASTER PLAN v2.0                      ║
║  Strategy: ESTENDER infra existente (não criar nova)           ║
╠══════════════════════════════════════════════════════════════════╣
║  W0 (NOW): baseline (5 tasks, 15-25 min)                        ║
║  W1: foundation migration (Change/History/Report → quality)    ║
║  W2: gates.rs + 14 stubs deleted + single composite             ║
║  W3: `touring quality` CLI + 5 tools → touring-server            ║
║  W4: CEG X7 W_QUALITY=0.20                                     ║
║  W5: delete touring-harness + touring-harness-mcp              ║
║  W6: EXTEND pre/post/post_tool_rl hooks + REUSE RL bridge     ║
║  W7: 50/50 Diamond acceptance                                   ║
╠══════════════════════════════════════════════════════════════════╣
║  REUSED (no new code):                                         ║
║   - touring-hook-handlers::hooks::{pre_write,post_write,        ║
║       post_tool_rl} (EXTEND, don't create)                     ║
║   - touring-intelligence::rl::streaming_hook_integration       ║
║   - touring-intelligence::rl::learning_signals                 ║
║   - touring-intelligence::rl::online_rl                        ║
║   - touring-offensive::erickson (NLP markers)                  ║
║   - touring-offensive::vuln::cwe_patterns                      ║
║   - touring-orchestration::tasks::template_engine              ║
║   - touring-orchestration::tasks::TasksfileCompiler           ║
║   - touring-dispatch::hook_registry                            ║
║                                                                ║
║  NEW (only what's truly new):                                  ║
║   - touring-quality/src/gates.rs (Q6)                          ║
║   - touring-quality/src/fix/mod.rs (use template_engine)         ║
║   - touring-lsp/src/quality_diagnostics.rs (1 file, smallest)  ║
║   - 1 new hook event name "quality-check" in hook_registry     ║
╚══════════════════════════════════════════════════════════════════╝
```

---

**Aguardando aprovação de Gabriel** para iniciar W0 (QuickAction). O plano está completo e REUSE-first; nenhuma modificação foi executada ainda.