# Plano Estratégico: Deep Code Analysis Engine para Touring

> **TACO v6.0** | **Touring v30.1** | **Data**: 05/04/2026
> **Gerado por**: 3 scouts + 2 architects + 2 auditors + Context7 best practices
> **Status**: IMPLEMENTADO — Fases 1-6 completas, cross-audit aprovada, 90 testes passando

---

## Sumário Executivo

Este plano define a criação do **`touring-analysis`** — um novo crate Rust que unifica e eleva
a infraestrutura de análise profunda de código do Touring a nível de excelência. O plano aborda
5 bugs P0/P1 descobertos na infraestrutura atual, cria uma pipeline unificada de análise com
blast radius máximo, wiring map completo, E2E profundo, e um Code Health Score com intervalos
de confiança Wilson.

### Números-Chave do Codebase Analisado

| Métrica | Valor |
|---------|-------|
| Crates | 13 |
| Arquivos .rs | 470 |
| Linhas de código | 202.629 |
| Testes | 5.179 (#[test] + #[tokio::test]) |
| Símbolos indexados | 6.725.192 |
| Hooks daemon | 68 |
| Cortex handlers | 97 |
| `.unwrap()` calls | 2.776 |
| `unsafe` blocks | 27 |
| God objects (>1K LOC) | 10 arquivos |

---

## Parte 1: Bugs P0/P1 — Correções Prioritárias

### Bug 1 (P0): E2E queries tabela inexistente `memory_entries`

**Sintoma**: E2E fase 5 (knowledge) e fase 8 (memory) sempre retornam 0 entries.
**Causa raiz**: `cli_e2e.rs` linhas 593 e 842 querem `memory_entries`, mas a tabela real em
memory.db é `rlm_entries` (MEMORY_SCHEMA_V8).

**Fix**: 2 replacements em `crates/touring-hooks/src/cli_e2e.rs`:
```
Linha 593: "SELECT COUNT(*) FROM memory_entries" → "SELECT COUNT(*) FROM rlm_entries"
Linha 842: "SELECT COUNT(*) FROM memory_entries" → "SELECT COUNT(*) FROM rlm_entries"
```

**Esforço**: S (< 2h) | **Risco**: Nenhum | **Independente**: Sim

### Bug 2 (P0): Schema divergence no `wiring_map`

**Sintoma**: Databases criados com `KNOWLEDGE_SCHEMA_V8` têm colunas erradas no `wiring_map`,
causando falha em TODAS as queries de wiring.

**Causa raiz**: `touring-core/src/schema/knowledge.rs` define:
`wiring_map(symbol_name, defined_in, used_in, usage_count, integration_score)`

Mas o schema operacional em `touring-hooks/src/knowledge.rs` usa:
`wiring_map(module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at)`

**Também divergem**: `module_ecosystem`, `bash_outcomes` (columns), `edit_history` vs `file_edit_history` (nome),
`gotchas` vs `file_gotchas` (nome), + 4 tabelas ausentes do V8 DDL.

**Fix**: Reescrever `KNOWLEDGE_SCHEMA_V8` para match exato do schema operacional.
O schema operacional é o ground truth (2.500+ linhas de queries escritas contra ele).

**Esforço**: M (2-8h) | **Risco**: Médio — grep por nomes antigos antes de aplicar | **Depende de**: Bug 4

### Bug 3 (P1): Tabelas `functional_signatures` e `functional_chains` nunca criadas

**Sintoma**: `functional_wiring.rs` faz INSERT em tabelas que não existem em DBs novos.
E2E fase 3 (wiring) T6 sempre retorna 0 chains.

**Causa raiz**: As tabelas foram documentadas como parte do v6→v7 migration mas o DDL
nunca foi commitado nem em `ensure_schema()` nem em `migrate_schema()`.

**Fix**: Adicionar `CREATE TABLE IF NOT EXISTS` para ambas tabelas em `knowledge.rs::migrate_schema()`.
Idempotente — DBs existentes não são afetados.

```sql
CREATE TABLE IF NOT EXISTS functional_signatures (
    file_path TEXT PRIMARY KEY,
    module_purpose TEXT,
    domain TEXT,
    symbols_json TEXT DEFAULT '[]',
    content_hash TEXT,
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS functional_chains (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_module TEXT NOT NULL,
    source_symbol TEXT NOT NULL,
    source_output TEXT,
    sink_module TEXT NOT NULL,
    sink_symbol TEXT NOT NULL,
    sink_input TEXT,
    chain_type TEXT NOT NULL DEFAULT 'Sequential',
    confidence REAL DEFAULT 0.0,
    created_at TEXT DEFAULT (datetime('now'))
);
```

**Esforço**: S (< 2h) | **Risco**: Nenhum (idempotente) | **Independente**: Sim

### Bug 4 (P2): `functional_chain_signal` ausente do `pre_edit.rs`

**Sintoma**: Functional chain context só é injetado em `pre_write.rs:188`, não em `pre_edit.rs`.
ENHANCEMENT-PLAN.md marca como "✅ E1" mas o código não tem a chamada.

**Fix**: Adicionar em `pre_edit.rs::compose_edit_context_scored()` após Signal 13:
```rust
if let Some(fc_signal) = crate::functional_wiring::functional_chain_signal(db, file_path) {
    signals.push(fc_signal);
}
```

**Esforço**: S (< 2h) | **Risco**: Baixo (aditivo) | **Depende de**: Bug 3

### Bug 5 (P2): E2E fase 2 (AST) blast radius é stub

**Sintoma**: `phase_ast` apenas verifica se `symbol_store` existe — não computa blast radius real.

**Fix**: Será resolvido pela criação do touring-analysis com E2E phases reescritas.

**Esforço**: Incluído na Parte 2 | **Depende de**: touring-analysis

---

## Parte 2: Arquitetura do Deep Analysis Engine

### Posição no Grafo de Dependências

```
Layer 0 (Foundation)
  touring-core ─────────────────────────────────────────────┐
  touring-simd ─────────────────────────────────────────────┤
                                                            ↓
Layer 1 (Core Intelligence)
  touring-ast ──── touring-core, touring-simd               │
                                                            ↓
Layer 2 (Learning)
  touring-learning ── touring-core, touring-simd, touring-ast
  touring-index ───── touring-ast                           │
                                                            ↓
Layer 2.5 (Analysis) ← NOVO
  touring-analysis ── touring-core, touring-simd,           │
                       touring-ast, touring-learning         │
                                                            ↓
Layer 4 (Hook Runtime)
  touring-hooks ───── touring-analysis (+ existentes)       │
                                                            ↓
Layer 5-6 (Application)
  touring-cortex ──── touring-analysis (+ existentes)       │
  touring-server ──── touring-analysis (+ existentes)       │
```

### Princípios de Design

| Princípio | Implementação |
|-----------|---------------|
| **Stateless** | Recebe connections/indexes como parâmetros, não possui estado |
| **Trait-based** | `BlastRadiusStrategy`, `QualityAnalyzer` plugáveis |
| **Schema guard** | Constantes únicas para nomes de tabelas — single source of truth |
| **Budget-enforced** | `AnalysisConfig::hook_path()` aborta em >50ms retornando parcial |
| **Wrap, don't copy** | Delega para implementações existentes em touring-ast |
| **Wilson confidence** | Score composto com intervalos de confiança estatísticos |

### Estrutura de Módulos

```
crates/touring-analysis/src/
├── lib.rs                      — Re-exports, feature gates
├── engine.rs                   — AnalysisConfig (hook_path/standard/deep)
├── blast_radius/
│   ├── mod.rs                  — BlastRadiusStrategy trait + BlastRadiusEngine
│   ├── bfs.rs                  — BFS exato (wraps SymbolIndex::blast_radius_with_depth)
│   ├── weighted.rs             — Dijkstra ponderado por co-edit
│   └── hnsw.rs                 — cfg(feature="ann") HNSW aproximado
├── quality/
│   ├── mod.rs                  — QualityPipeline + QualityReport
│   ├── antipatterns.rs         — SIMD antipattern scan (move de hooks/shared/)
│   ├── complexity.rs           — Cyclomatic + cognitive complexity
│   ├── unwrap_audit.rs         — NOVO: .unwrap() counter + risk scoring
│   ├── error_coverage.rs       — NOVO: Result/? coverage ratio
│   └── test_proxy.rs           — NOVO: test file ratio como proxy de coverage
├── wiring/
│   ├── mod.rs                  — WiringAnalyzer trait + WiringReport
│   ├── orphan.rs               — Orphan detection (move de hooks/wiring.rs)
│   ├── functional_chains.rs    — Chain tracing, broken chain detection
│   └── cross_crate.rs          — cfg(feature="cross-crate") análise cross-boundary
├── health/
│   ├── mod.rs                  — CodeHealthReport + HealthScorer
│   ├── dimensions.rs           — Enum de dimensões com pesos
│   └── confidence.rs           — Wilson confidence intervals
├── e2e/
│   ├── mod.rs                  — E2eAnalyzer::run()
│   ├── phases.rs               — 8+ phase impls com schema_guard
│   └── schema_guard.rs         — CRÍTICO: constantes de nomes de tabelas
└── temporal/
    ├── mod.rs                  — cfg(feature="temporal") trends
    └── trends.rs               — Velocity, churn, quality delta
```

### Tipos Core

```rust
// ── engine.rs ──
pub struct AnalysisConfig {
    pub blast_depth: usize,        // 0 = unlimited
    pub quality_sample: usize,     // 0 = all files
    pub cross_crate: bool,
    pub budget_ms: Option<u64>,    // hard-abort for hook path
}

impl AnalysisConfig {
    pub fn hook_path() -> Self;    // blast_depth=5, sample=1, budget=40ms
    pub fn standard() -> Self;     // blast_depth=0, sample=30, no budget
    pub fn deep() -> Self;         // unlimited, cross-crate, temporal
}

// ── blast_radius/mod.rs ──
pub trait BlastRadiusStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn compute(&self, start_file: &str, config: &AnalysisConfig) -> BlastRadiusResult;
    fn latency_tier(&self) -> LatencyTier; // Fast(<1ms), Medium(<10ms), Slow(>10ms)
}

pub struct BlastRadiusEngine {
    strategies: Vec<Box<dyn BlastRadiusStrategy>>,
}

// ── quality/mod.rs ──
pub struct QualityReport {
    pub file_path: String,
    pub antipatterns: Vec<Antipattern>,
    pub complexity: ComplexityMetrics,
    pub unwrap_count: usize,
    pub error_handling_coverage: f64,
    pub score: f64,  // [0.0, 1.0]
}

pub struct QualityPipeline { config: AnalysisConfig }

impl QualityPipeline {
    pub fn analyze_file(&self, path: &str, source: &str) -> QualityReport;
    pub fn analyze_batch(&self, files: &[(&str, &str)]) -> Vec<QualityReport>; // rayon
}

// ── health/mod.rs ──
pub struct CodeHealthReport {
    pub blast_radius: HealthDimension,
    pub wiring: HealthDimension,
    pub quality: HealthDimension,
    pub error_handling: HealthDimension,
    pub test_coverage_proxy: HealthDimension,
    pub composite_score: f64,
    pub confidence_lower: f64,  // Wilson 95% CI
    pub confidence_upper: f64,
    pub status: HealthStatus,   // Healthy/Degraded/Critical
}

// ── e2e/schema_guard.rs ──
pub const TABLE_EDIT_HISTORY: &str = "edit_history";
pub const TABLE_BASH_OUTCOMES: &str = "bash_outcomes";
pub const TABLE_GOTCHAS: &str = "gotchas";
pub const TABLE_WIRING_MAP: &str = "wiring_map";
pub const TABLE_FUNCTIONAL_CHAINS: &str = "functional_chains";
pub const MEMORY_TABLE_RLM_ENTRIES: &str = "rlm_entries";
```

### Feature Flags

```toml
[features]
default = ["blast-radius", "quality", "wiring"]
blast-radius = []
quality = []
wiring = []
deep = ["blast-radius", "quality", "wiring", "cross-crate", "temporal"]
cross-crate = []
temporal = []
```

### Exposição CLI + MCP

| Interface | Comando | Comportamento |
|-----------|---------|---------------|
| CLI | `touring analyze deep [--depth quick\|standard\|deep]` | Executa E2eAnalyzer, retorna CodeHealthReport JSON |
| MCP | `touring_deep_analysis(depth, target?)` | Tool #27, retorna CodeHealthReport |
| Hook | Via `pre_edit` signal pipeline | AnalysisConfig::hook_path(), <50ms |

---

## Parte 3: DAG de Implementação

### Fase 1 — P0 Bug Fixes (Independentes, Safe)

| ID | Task | Arquivo | Esforço | Deps |
|----|------|---------|---------|------|
| F1.1 | Fix `memory_entries` → `rlm_entries` | cli_e2e.rs L593, L842 | S | — |
| F1.2 | Add functional tables DDL | knowledge.rs migrate_schema() | S | — |
| F1.3 | Reconcile KNOWLEDGE_SCHEMA_V8 | touring-core/schema/knowledge.rs | M | F1.2 |
| F1.4 | Add functional_chain_signal to pre_edit | pre_edit.rs | S | F1.2 |

**Acceptance**: `cargo test --workspace --exclude touring-python` passa. E2E retorna counts > 0 para memory e functional chains.

### Fase 2 — Crate Skeleton + Schema Guard

| ID | Task | Esforço | Deps |
|----|------|---------|------|
| F2.1 | Criar `touring-analysis/Cargo.toml` + `lib.rs` stubs | S | F1.3 |
| F2.2 | Criar `e2e/schema_guard.rs` com todas as constantes | S | F2.1 |
| F2.3 | Adicionar ao workspace Cargo.toml | S | F2.1 |
| F2.4 | Verificar: `cargo check -p touring-analysis` | S | F2.3 |

**Acceptance**: Crate compila sem erros. Schema guard constants verificados contra DDL real.

### Fase 3 — Quality Pipeline Extraction

| ID | Task | Esforço | Deps |
|----|------|---------|------|
| F3.1 | Move antipatterns.rs para touring-analysis | M | F2.4 |
| F3.2 | Criar unwrap_audit.rs (memchr scan) | M | F2.4 |
| F3.3 | Criar error_coverage.rs (Result/? ratio) | M | F2.4 |
| F3.4 | Criar complexity.rs (delegates to ast_bridge) | S | F2.4 |
| F3.5 | Criar QualityPipeline::analyze_file() + analyze_batch() | M | F3.1-F3.4 |
| F3.6 | Re-export em hooks/shared/antipatterns.rs | S | F3.1 |

**Acceptance**: 20+ unit tests. Antipatterns cobrem 8 linguagens. unwrap_audit detecta .unwrap() corretamente.

### Fase 4 — Blast Radius Unification

| ID | Task | Esforço | Deps |
|----|------|---------|------|
| F4.1 | Criar BlastRadiusStrategy trait + BfsStrategy | M | F2.4 |
| F4.2 | Criar WeightedStrategy (wraps Dijkstra) | S | F4.1 |
| F4.3 | Criar BlastRadiusEngine (strategy dispatch) | M | F4.1-F4.2 |
| F4.4 | Budget enforcement (<50ms abort com resultado parcial) | S | F4.3 |

**Acceptance**: 15+ tests. BFS depth=5 matches resultado direto de SymbolIndex. Budget truncation funciona.

### Fase 5 — Wiring Analyzer

| ID | Task | Esforço | Deps |
|----|------|---------|------|
| F5.1 | Criar orphan detection (move de hooks/wiring.rs) | M | F2.4 |
| F5.2 | Criar functional_chains.rs (chain tracing) | M | F5.1, F1.2 |
| F5.3 | Criar WiringAnalyzer::analyze() | M | F5.1-F5.2 |

**Acceptance**: 10+ tests. Orphan detection matches resultado de `touring wiring orphans`. Broken chains detectadas.

### Fase 6 — E2E Extraction + Health Score

| ID | Task | Esforço | Deps |
|----|------|---------|------|
| F6.1 | Extrair 8 phases de cli_e2e.rs para touring-analysis/e2e/phases.rs | L | F3.5, F4.3, F5.3 |
| F6.2 | Criar E2eAnalyzer::run() | M | F6.1 |
| F6.3 | Criar CodeHealthReport + HealthScorer | M | F6.2 |
| F6.4 | Wilson confidence intervals | S | F6.3 |
| F6.5 | Refatorar cli_e2e.rs para delegar ao E2eAnalyzer | M | F6.2 |

**Acceptance**: E2E existentes passam. CodeHealthReport produz score com confidence. Todas as queries usam schema_guard.

### Fase 7 — CLI + MCP Exposure

| ID | Task | Esforço | Deps |
|----|------|---------|------|
| F7.1 | Add `cli-analyze-deep` handler em cli_handlers.rs | M | F6.5 |
| F7.2 | Add ao hook_registry.rs (68 → 69 hooks) | S | F7.1 |
| F7.3 | Add `analyze` subcommand em cli/common.rs | S | F7.1 |
| F7.4 | Add `touring_deep_analysis` MCP tool em server/mod.rs | M | F6.5 |

**Acceptance**: `touring analyze deep --depth standard` retorna JSON. MCP tool retorna CodeHealthReport.

### Fase 8 — Cross-Crate + Temporal (Feature-gated)

| ID | Task | Esforço | Deps |
|----|------|---------|------|
| F8.1 | Criar cross_crate.rs (feature flag analysis) | L | F5.3 |
| F8.2 | Criar temporal/trends.rs (velocity, churn) | L | F6.3 |
| F8.3 | Integrar ao CodeHealthReport como dimensões opcionais | M | F8.1-F8.2 |

**Acceptance**: `touring analyze deep --depth deep` inclui cross-crate e temporal dimensions.

### Diagrama DAG

```
F1.1 ─────────────┐
F1.2 ────┬────────┤
         ↓        ↓
F1.3 ── F1.4    F2.1 → F2.2 → F2.3 → F2.4
                                        │
         ┌──────────────────────────────┤
         ↓              ↓               ↓
      F3.1-F3.4      F4.1-F4.2      F5.1-F5.2
         ↓              ↓               ↓
       F3.5           F4.3            F5.3
         ↓              ↓               ↓
         └──────── F6.1 ←──────────────┘
                    ↓
              F6.2 → F6.3 → F6.4
                    ↓
                  F6.5
                    ↓
         ┌─────────┴──────────┐
         ↓                    ↓
    F7.1 → F7.2,F7.3      F7.4
                    ↓
         F8.1 ── F8.2 → F8.3
```

---

## Parte 4: Performance Budget

| Operação | Budget | Estratégia |
|----------|--------|------------|
| Hook-path blast radius | <5ms | BFS depth=5, warm index |
| Hook-path quality | <15ms | Single file, SIMD antipatterns |
| Hook-path total | <40ms | Budget enforcement, truncation parcial |
| CLI standard | <2s | 30-file sample, rayon parallel |
| CLI deep | <30s | All files, cross-crate, temporal |
| MCP tool | <10s | Configurable via depth parameter |

---

## Parte 5: Métricas de Sucesso

### Quality Gates

| Gate | Critério |
|------|----------|
| **Functional** | Todos os testes passam (existentes + 80 novos) |
| **Robust** | Zero `.unwrap()` em código de produção do touring-analysis |
| **Readable** | Nenhum arquivo > 500 LOC no novo crate |
| **Documented** | Docstrings em toda API pública |
| **Secure** | Path validation em analyze_batch (prevent traversal) |
| **No Regression** | `cargo test --workspace --exclude touring-python` green |
| **Performance** | Hook-path <50ms (benchmark validation) |

### KPIs Pós-Implementação

| KPI | Baseline (atual) | Target |
|-----|-------------------|--------|
| E2E accuracy | ~40% (3+ phases broken) | 100% (all phases query correct tables) |
| Wiring orphan detection | 0 (schema broken for fresh DBs) | Accurate count |
| Quality dimensions | 2 (antipatterns + complexity) | 6 (+ unwrap, error coverage, test proxy, confidence) |
| Blast radius APIs | 3 disconnected | 1 unified with strategy dispatch |
| Code Health Score | Não existe | Composite 0-1 com Wilson CI |
| New tests | 0 | 80+ |

---

## Parte 6: Riscos e Mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|-------|---------------|---------|-----------|
| Schema reconciliation quebra queries existentes | MED | HIGH | Grep por nomes antigos antes. Testes E2E existentes como safety net |
| Extração de antipatterns quebra callers em hooks | LOW | MED | Re-export pattern (`pub use touring_analysis::*`) mantém compatibilidade |
| Hook-path budget excedido | LOW | MED | Truncation parcial com nota. Benchmark em CI |
| touring-analysis cria ciclo de dependências | LOW | HIGH | Layer 2.5 verificado: não depende de hooks/cortex/server |
| God objects em touring-server crescem | MED | LOW | Novo MCP tool é 1 método, delegates para touring-analysis |

---

## Parte 7: Estimativa de Esforço Total

| Fase | Esforço | Tasks |
|------|---------|-------|
| Fase 1: P0 Fixes | S+S+M+S = ~1-2d | 4 tasks |
| Fase 2: Crate Skeleton | S+S+S+S = ~0.5d | 4 tasks |
| Fase 3: Quality Pipeline | M+M+M+S+M+S = ~2-3d | 6 tasks |
| Fase 4: Blast Radius | M+S+M+S = ~1-2d | 4 tasks |
| Fase 5: Wiring | M+M+M = ~1-2d | 3 tasks |
| Fase 6: E2E + Health | L+M+M+S+M = ~3-5d | 5 tasks |
| Fase 7: CLI + MCP | M+S+S+M = ~1-2d | 4 tasks |
| Fase 8: Cross-crate | L+L+M = ~3-5d | 3 tasks |
| **TOTAL** | **~12-21d** | **33 tasks** |

---

## Parte 8: Arquivos Essenciais (Referência)

### Arquivos a MODIFICAR

| Arquivo | Mudança |
|---------|---------|
| `crates/touring-hooks/src/cli_e2e.rs` | Fix table names + delegate to touring-analysis |
| `crates/touring-hooks/src/knowledge.rs` | Add functional tables DDL |
| `crates/touring-core/src/schema/knowledge.rs` | Rewrite KNOWLEDGE_SCHEMA_V8 |
| `crates/touring-hooks/src/pre_edit.rs` | Add functional_chain_signal |
| `crates/touring-hooks/src/shared/antipatterns.rs` | Re-export from touring-analysis |
| `crates/touring-hooks/Cargo.toml` | Add touring-analysis dep |
| `crates/touring-server/src/server/mod.rs` | Add touring_deep_analysis MCP tool |
| `crates/touring-server/src/cli/common.rs` | Add analyze subcommand |
| `crates/touring-hooks/src/cli_handlers.rs` | Add cli-analyze-deep handler |
| `crates/touring-hooks/src/hook_registry.rs` | Add cli-analyze-deep (68→69) |
| `Cargo.toml` (workspace) | Add touring-analysis member |

### Arquivos a CRIAR

| Arquivo | Propósito |
|---------|-----------|
| `crates/touring-analysis/Cargo.toml` | Crate definition + features |
| `crates/touring-analysis/src/lib.rs` | Public re-exports |
| `crates/touring-analysis/src/engine.rs` | AnalysisConfig |
| `crates/touring-analysis/src/blast_radius/*.rs` | 4 arquivos (mod, bfs, weighted, hnsw) |
| `crates/touring-analysis/src/quality/*.rs` | 6 arquivos (mod, antipatterns, complexity, unwrap, error, test_proxy) |
| `crates/touring-analysis/src/wiring/*.rs` | 4 arquivos (mod, orphan, functional, cross_crate) |
| `crates/touring-analysis/src/health/*.rs` | 3 arquivos (mod, dimensions, confidence) |
| `crates/touring-analysis/src/e2e/*.rs` | 3 arquivos (mod, phases, schema_guard) |
| `crates/touring-analysis/src/temporal/*.rs` | 2 arquivos (mod, trends) |

### Arquivos de REFERÊNCIA (não modificar, mas consultar)

| Arquivo | Por que |
|---------|---------|
| `crates/touring-hooks/src/hook_runtime.rs` | Padrão HookRuntime decomposition |
| `crates/touring-hooks/src/shared/signal_pipeline.rs` | Padrão SignalLayer trait |
| `crates/touring-ast/src/graph/mod.rs` | blast_radius_with_depth() para wrapping |
| `crates/touring-ast/src/graph/blast_radius.rs` | BlastRadiusOutput enum |
| `crates/touring-hooks/src/wiring.rs` | Queries de wiring para extraction |
| `crates/touring-hooks/src/functional_wiring.rs` | Chain detection logic |
| `crates/touring-simd/src/simd_utils/ops.rs` | SIMD patterns (pulp::WithSimd) |
| `crates/touring-learning/src/bandit/linucb.rs` | LinUCB para Wilson confidence |

---

*TACO v6.0 | 7 Fases Completas | 3 Scouts + 2 Architects + Context7 | touring-analysis Pln2*
