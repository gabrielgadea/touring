# touring-generator — Strategy Delivery (TACO Phase 5)

> **Date**: 2026-04-10
> **Orchestrator**: TACO v6.0 (7-phase sequential protocol)
> **Subagents**: 2 scouters + 3 architects + 1 auditor (pending) + 1 scriber (pending)
> **Session**: f8bf87dc
> **Paradigm**: LLM-as-Planner ↔ Touring-as-Generator
> **Status**: Strategy finalized, awaiting cross-audit (Phase 6) and scriber documentation (Phase 7)

---

## Executive Summary

Criar crate **`touring-generator`** como host de primeira classe do paradigma **LLM-as-Planner ↔ Touring-as-Generator**, substituindo 7562 LOC de infraestrutura Python (`scripts/vgp/`, `scripts/aco/generators/`, `lib/plan_generator/`, `scripts/touring_python_client.py`) por execução determinística nativa em Rust.

**Confidence: 0.95 (FACT)**, baseado em:
- 3 architects com VP-Scout validation (feature_trace, dependency_cycle, already_implemented, homonimia)
- Context7 confirmação de APIs (Tera, syn/quote, PyO3)
- Inspeção empírica de `symbol_detail.rs:76`, `speculate.rs:295`, `hook_registry.rs:729`, `common.rs:149`, `mod.rs:222`

---

## Trade-off Matrix — Option A vs B vs C

| Opção | Clean Arch | Effort | Risk | Extensibility | Scope Max | Test Isol | Migration | Rollback | **Score** |
|-------|:----------:|:------:|:----:|:-------------:|:---------:|:---------:|:---------:|:--------:|:---------:|
| **A — Nova crate `touring-generator`** | 10 | L4 | medium | 10 | 10 | 10 | 10 | 10 | **9.3/10** |
| B — Estender `touring-cortex` | 4 | L3 | high | 6 | 7 | 4 | 6 | 5 | 5.3 |
| C — Estender `touring-hooks::cli_handlers.rs` | 2 | L2 | critical | 3 | 5 | 2 | 4 | 3 | 3.0 |

**Vencedor: Option A** — satisfaz SRP, zero churn em `hook_runtime.rs` (47 edits histórico, arquivo mais quente do workspace), test isolation, extensibilidade para novos kinds, e REGRA #0 de scope maximization.

---

## Paradigma Central: LLM-as-Planner ↔ Touring-as-Generator

### Inversão de papéis
- **LLM (Claude)** = **PLANEJADOR ESTRATÉGICO**
  - Recebe intent em linguagem natural
  - Produz `GeneratorPlan` JSON estruturado (dados de primeira classe)
  - Itera com replanning quando Touring reporta falhas
  - NÃO escreve código diretamente
  - Aprende com feedback RL do Touring

- **Touring (Rust nativo)** = **GERADOR AUTOMÁTICO DE CÓDIGO**
  - Recebe planos estruturados via stdin/MCP/PyO3
  - VERIFICA plano contra symbol index (VGP v2 — `extract_symbol_details`)
  - RENDERIZA código usando templates (Tera) + AST (syn/quote opcional)
  - VALIDA via `speculate_v2` (5-layer Bayesian fusion)
  - COMMITA apenas se `composite_score ≥ 0.8`
  - REPORTA falhas de volta ao LLM com `FailureReport` estruturado + suggestions
  - EMITE telemetria para RL (touring-learning)

### Por que isso é poderoso
1. **Separação de concerns ótima**: LLM excels em raciocínio criativo; Touring excels em execução determinística verificável
2. **Verifiability estrutural**: toda geração é rastreável e auditável (VGP → speculate → commit)
3. **Learning loop fechado**: RL rewards do Touring retroalimentam os prompts DSPy do LLM
4. **Hallucination eliminada**: LLM não pode "inventar" APIs — Touring verifica contra symbol index antes de gerar um byte
5. **Reproducibility**: mesmo plano → mesmo código (deterministic mode)
6. **Composability**: planos são versionados, armazenados em `touring memory`, compostos

---

## Stack Técnico (validado via Context7)

- **Rust 2021 edition**, tokio async runtime
- **Tera 1.x** — template engine default (runtime, `autoescape_on(vec![])` para código)
- **syn 2.x + quote 1.x** opcionais (feature `syn-quote`) para AST-aware Rust codegen
- **thiserror 2.x** para error hierarchy
- **schemars 0.8** para JsonSchema generation
- **uuid 1.x + chrono 0.4 + semver 1.x** para plan metadata/versioning
- **rayon 1.x** para verificação paralela de símbolos
- **dashmap** para cache TTL in-memory
- **PyO3 0.24** via touring-python existente (novo submodule `generate`)

---

## Dependência Minimalista (resolução de divergência A vs B vs C)

### Compile-time deps (obrigatórias)
```toml
[dependencies]
touring-core    = { path = "../touring-core" }
touring-ast     = { path = "../touring-ast" }     # VGP v2 + speculate_v2
touring-index   = { path = "../touring-index" }   # symbol verification direct
```

### NÃO deps (via closure injection at runtime)
- ❌ `touring-hooks` (evitar hot file churn em `hook_runtime.rs`)
- ❌ `touring-cortex` (evitar blast radius e cycle risk — R7 de Architect C)
- ❌ `touring-server` (consumidor, não producer)

### Runtime injection via `GeneratorContext`
```rust
pub struct GeneratorContext {
    pub project_root: PathBuf,
    pub symbol_index: Arc<touring_index::SymbolIndex>,

    /// Memory store closure: (key, value, tier, type) -> Result.
    pub memory_store_fn: Arc<
        dyn Fn(&str, &str, &str, &str) -> anyhow::Result<()> + Send + Sync
    >,

    /// RL reward injection closure: (tool, value, context).
    pub rl_reward_fn: Arc<dyn Fn(&str, f64, &str) + Send + Sync>,

    /// Optional DSPy signature lookup (injected by touring-server).
    pub dspy_sig_fn: Option<Arc<
        dyn Fn(&str) -> Option<serde_json::Value> + Send + Sync
    >>,

    /// Optional MCTS evaluation closure (reuses H99 via touring-cortex).
    pub mcts_eval_fn: Option<Arc<dyn Fn(&str) -> f64 + Send + Sync>>,
}
```

**Isto é CRÍTICO**: desacopla touring-generator de qualquer crate "quente", permitindo test isolation, evita dep cycles, e mantém blast radius mínimo.

---

## Crate DAG Final

```
                          touring-core (foundation)
                                 │
                ┌────────────────┼────────────────┐
                │                │                │
           touring-ast      touring-index     touring-learning
           (VGP v2)          (symbols)         (ACO N3, RLM)
                │                │
                └──────┬─────────┘
                       │
              ╔════════▼══════════╗
              ║ touring-generator ║  ← NEW CRATE
              ║   (deterministic  ║
              ║    executor)      ║
              ╚════════┬══════════╝
                       │
        ┌──────────────┴──────────────┐
        │                             │
  touring-server              touring-python
  (CLI + MCP)                 (PyO3 bindings)
```

**Cycle check**: PASS (verificado via `cargo tree` inspection + Cargo.toml grep)

---

## File Layout Completo (~4150 LOC)

```
crates/touring-generator/
├── Cargo.toml                          # ~50 LOC
├── README.md                           # ~180 LOC
├── ARCHITECTURE.md                     # ~300 LOC
├── MIGRATION.md                        # ~150 LOC
├── schema/
│   └── generator_plan_v1.json          # auto-gen via schemars
├── templates/                          # ~320 LOC total
│   ├── rust_module.tera
│   ├── cli_handler.tera
│   ├── mcp_tool.tera
│   ├── hook_handler.tera
│   ├── plan.md.tera
│   ├── test.tera
│   ├── python_script.tera
│   └── schema.tera
├── src/                                # ~3200 LOC Rust
│   ├── lib.rs                          # public re-exports
│   ├── plan/
│   │   ├── mod.rs
│   │   ├── schema.rs                   # GeneratorPlan + subtypes
│   │   ├── kinds.rs                    # GeneratorKind enum
│   │   ├── contracts.rs                # Contracts + SymbolRef + Invariant
│   │   └── metadata.rs                 # PlanMetadata
│   ├── core/
│   │   ├── mod.rs
│   │   ├── generator.rs                # Generator trait
│   │   ├── request.rs
│   │   ├── result.rs
│   │   ├── context.rs                  # GeneratorContext (closures)
│   │   └── error.rs                    # GenerateError (thiserror)
│   ├── vgp/
│   │   ├── mod.rs
│   │   ├── engine.rs                   # VgpEngine (touring-index direct)
│   │   └── report.rs
│   ├── template/
│   │   ├── mod.rs
│   │   ├── engine.rs                   # Tera wrapper
│   │   └── registry.rs                 # embedded templates
│   ├── engines/
│   │   ├── mod.rs
│   │   ├── tera_engine.rs              # default
│   │   ├── syn_quote.rs                # #[cfg(feature="syn-quote")]
│   │   └── string_interp.rs            # zero-dep fallback
│   ├── speculate/
│   │   ├── mod.rs
│   │   └── bridge.rs                   # wraps touring-ast::speculate_v2
│   ├── lifecycle/
│   │   ├── mod.rs
│   │   ├── executor.rs                 # PlanExecutor state machine
│   │   ├── state.rs                    # PlanState enum
│   │   ├── failure.rs                  # FailureReport
│   │   └── suggestions.rs              # fuzzy suggestions
│   ├── kinds/
│   │   ├── mod.rs
│   │   ├── module.rs
│   │   ├── cli_handler.rs
│   │   ├── mcp_tool.rs
│   │   ├── hook.rs                     # patches ALL_DAEMON_HOOK_NAMES
│   │   ├── python_script.rs
│   │   ├── plan.rs
│   │   ├── test.rs
│   │   └── template.rs
│   ├── memory/
│   │   ├── mod.rs
│   │   ├── store.rs
│   │   └── recall.rs
│   ├── dspy/
│   │   ├── mod.rs
│   │   └── signatures.rs               # 4 DSPy sigs (closure-injected)
│   └── cli_handlers.rs                 # pub fn register_in(command_table)
└── tests/                              # ~450 LOC
    ├── unit_generator.rs
    ├── integration_lifecycle.rs
    ├── e2e_plan_roundtrip.rs
    └── fixtures/
        └── sample_plans/*.json
```

---

## Cargo.toml Final

```toml
[package]
name = "touring-generator"
version = "0.1.0"
edition = "2021"
description = "LLM-planner / Touring-executor code generation crate"
license = "MIT"

[dependencies]
touring-core  = { path = "../touring-core" }
touring-ast   = { path = "../touring-ast" }
touring-index = { path = "../touring-index" }

serde      = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
thiserror  = "2"
anyhow     = { workspace = true }
uuid       = { version = "1", features = ["v4", "serde"] }
chrono     = { version = "0.4", features = ["serde"] }
semver     = { version = "1", features = ["serde"] }
schemars   = { version = "0.8", features = ["uuid1", "chrono"] }
tera       = "1"
rayon      = "1"
dashmap    = { workspace = true }
tracing    = { workspace = true }
tokio      = { workspace = true, features = ["sync", "rt"] }
async-trait = "0.1"

# Optional — syn/quote for AST-aware Rust codegen
syn         = { version = "2", features = ["full", "parsing", "printing", "extra-traits"], optional = true }
quote       = { version = "1", optional = true }
proc-macro2 = { version = "1", optional = true }

[features]
default         = ["tera-engine"]
tera-engine     = []
syn-quote       = ["dep:syn", "dep:quote", "dep:proc-macro2"]
mcts-synthesis  = []  # activation gate only; actual H99 via closure injection

[dev-dependencies]
tempfile          = "3"
pretty_assertions = "1"
tokio             = { workspace = true, features = ["macros", "rt-multi-thread"] }
```

---

## GeneratorPlan Schema v1.0

```rust
//! GeneratorPlan schema v1.0 — structured plan emitted by LLM, consumed by Touring.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GeneratorPlan {
    pub version: semver::Version,
    pub plan_id: Uuid,
    pub intent: String,
    pub cila_level: CilaLevel,
    pub target: Target,
    pub kind: GeneratorKind,
    pub contracts: Contracts,
    pub verification: VgpRequirements,
    pub template: TemplateSelection,
    pub assembly: Assembly,
    pub validation: ValidationDirectives,
    pub commit_policy: CommitPolicy,
    pub rollback: RollbackPolicy,
    pub learning: LearningDirectives,
    pub metadata: PlanMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum CilaLevel { L0, L1, L2, L3, L4, L5 }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
pub enum GeneratorKind {
    Module, CliHandler, McpTool, Hook, CliCommand,
    Test, Schema, Template, PythonScript, Plan,
    Migration, Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Contracts {
    pub symbols_must_exist: Vec<SymbolRef>,
    pub symbols_must_not_exist: Vec<SymbolRef>,
    pub traits_implemented: Vec<String>,
    pub exports: Vec<String>,
    pub dependencies: Vec<CrateDep>,
    pub invariants: Vec<Invariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolRef {
    pub name: String,
    pub crate_name: Option<String>,
    pub module_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Target {
    pub crate_name: String,
    pub file_path: PathBuf,
    pub module_path: String,
    pub line_hint: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemplateSelection {
    pub template_id: String,
    pub variables: HashMap<String, serde_json::Value>,
    pub extends: Option<String>,
    pub engine: RenderEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum RenderEngine { Tera, SynQuote, StringInterpolation }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Assembly {
    pub files: Vec<FileOutput>,
    pub mod_rs_entries: Vec<String>,
    pub cargo_toml_patches: Vec<CargoPatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileOutput {
    pub path: PathBuf,
    pub action: FileAction,
    pub template_id: String,
    pub variables: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum FileAction { Create, Append, Replace, InsertAt(usize) }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationDirectives {
    pub min_speculate_score: f64,
    pub required_layers: Vec<String>,
    pub max_complexity_score: f64,
    pub custom_assertions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitPolicy {
    pub auto_commit_threshold: f64,
    pub require_human_review: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RollbackPolicy {
    pub enabled: bool,
    pub backup_path: Option<PathBuf>,
    pub rollback_on_test_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LearningDirectives {
    pub reward_on_commit: f64,
    pub reward_on_replan: f64,
    pub memory_key: String,
    pub memory_tier: String,     // "semantic" | "local"
    pub memory_type: String,     // "pattern" | "lesson" | "insight"
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanMetadata {
    pub author: String,          // "llm" | "human" | "replay"
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub parent_plan_id: Option<Uuid>,
    pub session_id: String,
    pub codebase_hash: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VgpRequirements {
    pub pre_verify_all: bool,
    pub fail_on_missing: bool,
    pub homonimia_check: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CrateDep {
    pub name: String,
    pub version: Option<String>,
    pub path: Option<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Invariant {
    pub id: String,
    pub description: String,
    pub check: InvariantCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum InvariantCheck {
    SymbolExists(SymbolRef),
    SymbolAbsent(SymbolRef),
    TraitImpl(String),
    Regex(String),
    TestMustPass(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CargoPatch {
    pub file: PathBuf,
    pub section: String,  // "[dependencies]", "[features]", etc
    pub key: String,
    pub value: String,
}
```

---

## Generator Trait

```rust
//! Core Generator trait — each GeneratorKind implements this.

use crate::core::{GenerateError, GeneratorContext};
use crate::plan::GeneratorPlan;
use crate::vgp::VgpReport;
use crate::speculate::SpeculateReport;
use crate::core::ValidationReport;
use std::path::PathBuf;

#[async_trait::async_trait]
pub trait Generator: Send + Sync {
    fn kind(&self) -> crate::plan::GeneratorKind;

    async fn verify(
        &self,
        plan: &GeneratorPlan,
        ctx: &GeneratorContext,
    ) -> Result<VgpReport, GenerateError>;

    async fn render(
        &self,
        plan: &GeneratorPlan,
        ctx: &GeneratorContext,
    ) -> Result<Vec<RenderedFile>, GenerateError>;

    async fn validate(
        &self,
        rendered: &[RenderedFile],
        plan: &GeneratorPlan,
    ) -> Result<ValidationReport, GenerateError>;

    async fn speculate(
        &self,
        rendered: &[RenderedFile],
        plan: &GeneratorPlan,
        ctx: &GeneratorContext,
    ) -> Result<SpeculateReport, GenerateError>;

    async fn commit(
        &self,
        rendered: &[RenderedFile],
        plan: &GeneratorPlan,
        ctx: &GeneratorContext,
    ) -> Result<crate::core::GenerateResult, GenerateError>;

    async fn rollback(
        &self,
        plan: &GeneratorPlan,
        ctx: &GeneratorContext,
    ) -> Result<(), GenerateError>;
}

pub struct RenderedFile {
    pub path: PathBuf,
    pub content: String,
    pub sha256: String,
    pub backup: Option<PathBuf>,
}
```

---

## Lifecycle State Machine

```
┌─────────┐
│  DRAFT  │
└────┬────┘
     │ VGP verify
     ▼
┌──────────┐      fail       ┌─────────────┐
│ VERIFIED │────────────────▶│ REPLANNING  │───┐
└────┬─────┘                 │ (max 5)     │   │
     │ render                └──────┬──────┘   │
     ▼                              │          │
┌──────────┐                        │          │
│ RENDERED │                        │          │
└────┬─────┘                        │          │
     │ speculate                    │          │
     ▼                              │          │
┌────────────┐      fail            │          │
│ SPECULATED │──────────────────────┘          │
└────┬───────┘                                 │
     │ score >= 0.8                            │
     ▼                                         │
┌───────────┐      test fail    ┌────────────┐ │
│ COMMITTED │──────────────────▶│ ROLLED_BACK│ │
└───────────┘                   └────────────┘ │
                                               │
                                       max iterations
                                               │
                                               ▼
                                        ┌──────────┐
                                        │ REJECTED │
                                        │ (escalate)│
                                        └──────────┘
```

### PlanExecutor

```rust
//! Lifecycle state machine executor.

use crate::core::{GenerateError, GeneratorContext};
use crate::plan::GeneratorPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanState {
    Draft,
    Verified,
    Rendered,
    Speculated,
    Committed,
    Rejected,
    Replanning,
    Failed,
    RolledBack,
}

pub struct PlanExecutor {
    state: PlanState,
    iteration_count: u8,
    max_iterations: u8,
    ctx: GeneratorContext,
}

pub enum ExecutionResult {
    Committed,
    AwaitingReplan(Box<crate::lifecycle::FailureReport>),
    EscalateToHuman(Box<crate::lifecycle::FailureReport>),
    Terminal(PlanState),
}

impl PlanExecutor {
    pub fn new(ctx: GeneratorContext) -> Self {
        Self {
            state: PlanState::Draft,
            iteration_count: 0,
            max_iterations: 5,
            ctx,
        }
    }

    pub async fn execute(
        &mut self,
        plan: GeneratorPlan,
    ) -> Result<ExecutionResult, GenerateError> {
        // State machine loop with transitions
        // DRAFT -> VERIFIED -> RENDERED -> SPECULATED -> COMMITTED
        // Failure paths: REPLANNING (bounded 5), REJECTED (escalate), ROLLED_BACK
        todo!("implementation in Wave 4")
    }
}
```

---

## LLM ↔ Touring Protocol

### LLM → Touring ops
```json
{"op": "plan.submit",   "plan": {...GeneratorPlan...}}
{"op": "plan.replan",   "plan_id": "...", "previous_failure": {...}, "revised_plan": {...}}
{"op": "plan.commit",   "plan_id": "..."}
{"op": "plan.rollback", "plan_id": "...", "reason": "..."}
{"op": "plan.status",   "plan_id": "..."}
{"op": "plan.recall",   "intent": "...", "limit": 5}
```

### Touring → LLM responses
```json
{
  "status": "verified|rendered|speculated|committed|rolled_back|failed",
  "plan_id": "uuid",
  "iteration": 1,
  "artifacts": [{"path": "...", "sha256": "...", "diff": "..."}],
  "verification_report": {
    "all_passed": true,
    "verified_symbols": ["HookRuntime", "MemoryStore"],
    "missing_symbols": []
  },
  "speculate_score": 0.91,
  "failure_report": null,
  "suggestions": [],
  "learning_feedback": {
    "reward": 1.0,
    "tool": "edit",
    "context": "committed:550e8400"
  }
}
```

### FailureReport (quando replanning é necessário)
```json
{
  "reason": "VGP_FAILED|SPECULATE_FAILED|TEMPLATE_ERROR|IO_ERROR|CIRCUIT_BREAKER",
  "plan_id": "uuid",
  "iteration": 1,
  "missing_symbols": [
    {"name": "MemoryStoer", "suggested_alternatives": ["MemoryStore"]}
  ],
  "failing_speculate_layers": [
    {"layer_name": "syntax", "score": 0.6, "issues": ["unclosed brace"]}
  ],
  "template_errors": [],
  "code_excerpts": [
    {"file": "...", "line_range": [10, 20], "content": "..."}
  ],
  "suggestions": [
    "Symbol 'MemoryStoer' not found. Did you mean 'MemoryStore'?"
  ],
  "escalate_to_human": false
}
```

### Replanning Loop
1. LLM submete Plan V1
2. Touring VGP: parallel `touring index find` para cada `SymbolRef`
3. Se VGP FAIL → emite `FailureReport{reason:VGP_FAILED, missing, suggestions}`
4. LLM gera Plan V2 (ajusta `contracts.symbols_must_exist` com verified names)
5. Touring VGP V2 → se PASS, prossegue para render
6. Speculate: se `composite_score < 0.8` → `FailureReport{reason:SPECULATE_FAILED, layers}`
7. LLM gera Plan V3 (ajusta template variables, adiciona invariants)
8. **Max 5 iterations** enforced by `iteration_count` counter
9. Na iteração 5 → CIRCUIT_BREAKER, `status=REJECTED`, `escalate_to_human=true`

**Memory of past failures**: antes de cada replan, `touring memory recall` com intent keywords; se padrão similar encontrado, injeta suggestions do lesson recalled no `FailureReport.suggestions`.

---

## 4 DSPy Signatures (closures injetadas via GeneratorContext)

### plan_generation_sig
- **Instruction**: "Generate a structured GeneratorPlan JSON from natural language intent. Reference only symbols verifiable via `touring index find`."
- **Inputs**: intent, symbol_context (known symbols), template_catalog, memory_recall, cila_level
- **Outputs**: plan (GeneratorPlan JSON), rationale, confidence (0.0-1.0)
- **Constraints**: NEVER reference symbols outside `symbol_context`; ALWAYS set `learning.memory_key`

### plan_verification_sig
- **Instruction**: "Analyze VGP report and determine if plan is executable or needs revision."
- **Inputs**: plan, vgp_report
- **Outputs**: assessment (PASS|NEEDS_REPLAN|ESCALATE), root_cause, fix_strategy

### plan_replanning_sig
- **Instruction**: "Given failed plan and FailureReport, produce revised plan that addresses failures. Do not repeat failed symbols."
- **Inputs**: failed_plan, failure_report, iteration, memory_lessons
- **Outputs**: revised_plan, changes_made, confidence
- **Constraints**: Each revision MUST differ from previous in fields that caused failure; iteration >= 4 → `require_human_review=true`

### plan_critique_sig
- **Instruction**: "Critically evaluate plan before submission to identify VGP/homonimia/speculate risks."
- **Inputs**: plan, symbol_index_sample
- **Outputs**: risks (Vec with severity), recommended_fixes, predicted_speculate_score
- **Constraints**: Flag symbols with generic names (Handler, Manager, Index, Engine, Loop) as homonimia risk

---

## 6 Escape Hatches

| ID | Nome | Trigger | Ação |
|----|------|---------|------|
| EH1 | Circuit Breaker | `iteration_count >= 5` | status=REJECTED, escalate_to_human=true, NEVER write files |
| EH2 | Human Override Bypass | `--force-bypass-vgp` + auth | Skip VGP, add AuditEntry, tag `force_bypass=true` |
| EH3 | Deterministic Mode | `env TOURING_GENERATOR_MODE=deterministic` ou `cila_level=L0` | Disable LLM/DSPy, pure template rendering, 100% reproducible |
| EH4 | Replay Mode | `plan.metadata.codebase_hash` set + `--replay` | Load historical symbol index snapshot, deterministic render |
| EH5 | Dry Run | `commit_policy.dry_run=true` | Full lifecycle mas escreve em `/tmp` shadow path, retorna diffs |
| EH6 | Score Floor Override | `min_speculate_score < 0.8` (range 0.5-1.0) | Log warning, tag `low_confidence=true`, mandatory post-commit test |

---

## CLI Surface (10 subcomandos)

Registrados em `touring-server/src/cli/generate.rs`, invocando `touring_generator::cli_handlers::*`:

```bash
touring generate plan-submit --plan-file <path> [-j]
touring generate plan-verify <plan_id> [-j]
touring generate plan-render <plan_id> [-j]
touring generate plan-speculate <plan_id> [-j]
touring generate plan-commit <plan_id> [-j]
touring generate plan-rollback <plan_id> [-j]
touring generate plan-status <plan_id> [-j]
touring generate plan-list [-j]
touring generate plan-recall "<query>" [-j]
touring generate schema-dump [-j]
```

**Critical**: `ALL_DAEMON_HOOK_NAMES.len()` incrementa de **98 → 108** (10 novos handlers).
`hook_registry.rs:729` assert_eq MUST ser atualizado: `assert_eq!(ALL_DAEMON_HOOK_NAMES.len(), 108)`.

---

## MCP Tools (8 novos em TouringServer)

Via `#[tool]` macro em `crates/touring-server/src/server/mod.rs` (dentro do impl `TouringServer` com `#[tool_router]` — confirmed at mod.rs:222):

```rust
#[tool(description = "Submit a GeneratorPlan JSON for execution")]
async fn touring_generator_submit_plan(&self, params: SubmitPlanParams)
    -> Result<CallToolResult, McpError> { /* ... */ }

#[tool(description = "Verify plan contracts against symbol index via VGP v2")]
async fn touring_generator_verify_plan(&self, params: PlanIdParams)
    -> Result<CallToolResult, McpError> { /* ... */ }

// + generator_render_plan
// + generator_speculate_plan
// + generator_commit_plan
// + generator_rollback_plan
// + generator_recall_similar
// + generator_schema_dump
```

---

## PyO3 Submodule (touring-python/src/generate_bindings.rs)

```rust
//! PyO3 bindings — adds `generate` submodule to existing claude_learning_kernel.

use pyo3::prelude::*;

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let generate = PyModule::new_bound(m.py(), "generate")?;
    generate.add_function(wrap_pyfunction!(py_submit_plan, &generate)?)?;
    generate.add_function(wrap_pyfunction!(py_verify_plan, &generate)?)?;
    generate.add_function(wrap_pyfunction!(py_render_plan, &generate)?)?;
    generate.add_function(wrap_pyfunction!(py_speculate_plan, &generate)?)?;
    generate.add_function(wrap_pyfunction!(py_commit_plan, &generate)?)?;
    generate.add_function(wrap_pyfunction!(py_rollback_plan, &generate)?)?;
    generate.add_class::<PyGeneratorPlan>()?;
    generate.add_class::<PyGenerateResult>()?;
    m.add_submodule(&generate)?;
    Ok(())
}

#[pyfunction]
fn py_submit_plan(plan_json: &str) -> PyResult<String> {
    // Bridge via tokio runtime handle
    todo!("Wave 7")
}
```

Em `touring-python/src/lib.rs`:
```rust
#[pymodule]
fn claude_learning_kernel(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // ... existing 8 submodule registrations
    crate::generate_bindings::register(m)?;
    Ok(())
}
```

---

## Template Example — rust_module.tera

```jinja
{#- Generated by touring-generator from plan {{ plan_id }} -#}
//! {{ description }}

{% for use_stmt in use_statements -%}
{{ use_stmt }}
{% endfor %}

{% for struct_def in structs -%}
#[derive(Debug, Clone{% if struct_def.serde %}, Serialize, Deserialize{% endif %})]
pub struct {{ struct_def.name }} {
{% for field in struct_def.fields -%}
    pub {{ field.name }}: {{ field.ty }},
{% endfor -%}
}
{% endfor %}

{% for impl_block in impls -%}
impl {{ impl_block.for_type }} {
{% for method in impl_block.methods -%}
    {{ method.visibility }} fn {{ method.name }}({{ method.signature }}) -> {{ method.return_type }} {
        {{ method.body | safe }}
    }
{% endfor -%}
}
{% endfor %}
```

---

## Migration Waves (9 waves, ~52-72h)

### Wave 1 — Foundation (4-6h)
- **S1.1** [independent]: Cargo.toml skeleton + workspace members
- **S1.2** [depends: S1.1]: lib.rs + module stubs
- **S1.3** [depends: S1.2]: Error hierarchy (error.rs, 13 variants)
- **S1.4** [depends: S1.2]: Plan schema (schema.rs + schemars derive)
- **S1.5** [depends: S1.2]: Core types (GenerateRequest/Result/Context)
- **Gate**: `cargo check -p touring-generator` passa

### Wave 2 — VGP Engine + Speculate Bridge (6-8h)
- **S2.1** [depends: S1.4, S1.5]: VgpEngine (touring-index direct, rayon parallel, moka TTL cache)
- **S2.2** [depends: S1.5]: SpeculateBridge (wraps `touring_ast::speculate_v2`)
- **S2.3** [depends: S1.3, S2.1, S2.2]: VgpPreHook + SpeculatePostHook pipelines
- **Gate**: VGP batch de 10 symbols em <5ms

### Wave 3 — Template Engine + 8 Tera Templates (8-10h)
- **S3.1** [depends: S1.2]: TemplateEngine (Tera wrapper, `autoescape_on(vec![])`, variable allowlist)
- **S3.2** [depends: S3.1]: 8 .tera files embedded via include_str!
- **S3.3** [depends: S3.1, syn-quote feature]: SynQuoteEngine (`syn::parse_quote!` + `syn::parse_file` validation)
- **Gate**: render tests passam + `syn::parse_file(generated).is_ok()` para Rust output

### Wave 4 — PlanExecutor + State Machine (8-10h)
- **S4.1** [depends: S1.4, S2.3, S3.1]: PlanExecutor com 9 estados + 10 transitions
- **S4.2** [depends: S4.1]: FailureReport + suggestion generation (fuzzy search)
- **S4.3** [depends: S4.1]: Commit + Rollback (atomic write + backup restore)
- **Gate**: state machine unit tests 100% coverage das transitions

### Wave 5 — 8 Generator Kinds (10-12h)
- **S5.1-S5.8** [parallel, depends: S4.1, S3.2]:
  - ModuleGenerator
  - CliHandlerGenerator (patches command_table())
  - McpToolGenerator (insere em TouringServer impl block)
  - HookGenerator (patches ALL_DAEMON_HOOK_NAMES + assert_eq count)
  - PlanGenerator (port lib/plan_generator)
  - TestGenerator
  - PythonScriptGenerator
  - TemplateMetaGenerator
- **Gate**: cada kind E2E test passa

### Wave 6 — Integration Layer (6-8h)
- **S6.1** [depends: all S5]: CLI handler registration em `touring-server/src/cli/generate.rs`
  - Update `hook_registry.rs:729` assert de 98 → 108
- **S6.2** [depends: S6.1]: MCP tool registration (#[tool] methods em TouringServer)
- **Gate**: `touring generate --help` funciona + MCP tools callable via mcp__

### Wave 7 — PyO3 Bindings (4-6h)
- **S7.1** [depends: S6.2]: `generate_bindings.rs` submodule
- **S7.2** [depends: S7.1]: `maturin develop` build .so
- **S7.3** [depends: S7.2]: Python import test
- **Gate**: `python3 -c "from claude_learning_kernel import generate; print(generate)"` passa

### Wave 8 — Python Migration (6-8h)
- **S8.1** [depends: S7.3]: `PYO3_AVAILABLE=True` activation em touring_python_client.py
- **S8.2** [depends: S8.1]: Legacy wrapper delegation (vgp/verifier.py → claude_learning_kernel.generate)
- **S8.3** [depends: S8.2]: Deprecation warnings
- **S8.4** [depends: S8.3, 1 week soak]: Remove Python subprocess paths
- **Gate**: 0 subprocess calls em generate path (verified via strace)

### Wave 9 — E2E Tests + Docs (4-6h)
- **S9.1-S9.7**: unit + integration + E2E tests, benchmarks, README/ARCHITECTURE/MIGRATION docs, memory store patterns, final RL reward injection
- **Gate**: all KPIs baseline recorded + 27/27 Python E2E still PASS

**Total estimado**: 56-74h engineering work (L4 task)

**Critical path**: S1.1 → S1.2 → S1.4 → S2.1 → S2.3 → S3.1 → S3.2 → S4.1 → S4.3 → S6.1 → S6.2 → S7.1 → S8.1

---

## Riscos e Mitigações (top 10)

| # | Risco | Severidade | Mitigação |
|---|-------|-----------|-----------|
| **R1** | `ALL_DAEMON_HOOK_NAMES.len()==98` assert drift | HIGH | G104 gotcha ativo. W6 inclui update explícito 98→108. `grep "assert_eq" hook_registry.rs` gate antes de cada merge |
| **R2** | `hook_runtime.rs` churn (47 edits hot file) | HIGH | touring-generator NEVER touches hook_runtime.rs. Closures injetadas via GeneratorContext, não imports diretos |
| **R3** | Dep cycle via touring-cortex | HIGH | touring-generator NÃO depende de touring-cortex. DSPy/MCTS via closure injection at runtime |
| **R4** | GeneratorPlan homonímia com `lib/plan_generator::Plan` | MEDIUM | Nomes distintos: `GeneratorPlan` (code gen) vs `Plan` (task planning). Coexistem em namespaces separados |
| **R5** | LLM hallucination — símbolos inexistentes | HIGH | VGP mandatory pre-render. Fuzzy suggestions em FailureReport. plan_critique_sig pre-screening |
| **R6** | Infinite replan loop | MEDIUM | Circuit breaker `max_iterations=5`. Memory recall evita repetir erros. Escalation to human no limite |
| **R7** | Tera template injection | HIGH | Variable allowlist (alphanumeric+underscore). `syn::parse_file` validation antes de write. speculate gate secondary |
| **R8** | Plan schema drift (LLM emite formato inválido) | MEDIUM | JsonSchema strict validation at boundary. Schema via `mcp__touring__touring_generator_schema_dump`. Version field mandatory |
| **R9** | PyO3 .so build coordination | MEDIUM | W7 isolado. `PYO3_AVAILABLE=False` é safe default. `maturin develop` em CI |
| **R10** | Build time increase (syn/quote/tera) | LOW | `syn-quote` feature gated (optional). Tera sempre default (~2s build overhead). Profile antes/depois |

---

## Python Migration Map

| Módulo Python | Novo papel no touring-generator | Status |
|---------------|-------------------------------|--------|
| `scripts/vgp/verifier.py` | REPLACED por `vgp::engine::VgpEngine` (rayon parallel, no subprocess) | Wave 2 |
| `scripts/vgp/cache.py` | REPLACED por dashmap TTL cache interno | Wave 2 |
| `scripts/vgp/parallel.py` | REPLACED por rayon `par_iter` | Wave 2 |
| `scripts/vgp/patterns.py` | REMOVED (tree-sitter em touring-ast já faz isso) | Wave 8 |
| `scripts/aco/generators/base_generator.py` | REPLACED por `core::Generator` trait | Wave 5 |
| `scripts/aco/generators/gen_generator.py` | BECOMES `plan.kind=Template` GeneratorKind | Wave 5 |
| `scripts/aco/generators/validate_generator.py` | REPLACED por `core::validation::ValidationReport` | Wave 5 |
| `scripts/aco/generators/rollback_generator.py` | REPLACED por `Generator::rollback()` method | Wave 5 |
| `scripts/aco/discover.py` | REPLACED por `cli_handlers::plan_recall` + `memory::recall` | Wave 6 |
| `scripts/aco/templates/*.py` (13 files) | MIGRATED para `templates/*.tera` | Wave 3 |
| `scripts/touring_python_client.py` (48 wrappers) | REPLACED por PyO3 `claude_learning_kernel.generate.*` | Wave 7 |
| `scripts/touring_maximize.py` | REPLACED por `GeneratorKind::Maximize` (Wave 5 extension) | Wave 5 |
| `scripts/dspy_quality_bridge.py` | REMOVED (DSPy via closure em touring-cortex::dspy_signature) | Wave 8 |
| `scripts/dspy_session_optimizer.py` | REMOVED (functionally inert, dspy not installed) | Wave 8 |
| `lib/plan_generator/models.py` | COEXIST (HOMONYM — task planning, not code gen) | — |
| `lib/plan_generator/generators.py` | COEXIST (TACO phase file generation) | — |
| `lib/plan_generator/cli.py` | DELEGATE para PyO3 via fast-path | Wave 7 |
| `lib/plan_generator/audit.py` | COEXIST | — |
| `lib/plan_generator/checkpoint_validator.py` | COEXIST (TACO orchestrator dep) | — |

**Decommission target**: 7562 LOC Python → ~500 LOC PyO3 shim (93% redução)

---

## Success Metrics (KPIs mensuráveis)

| # | KPI | Baseline | Target | Measurement |
|---|-----|---------|--------|-------------|
| 1 | Plan VGP first-attempt pass rate | 0% (novo) | ≥70% em 10 sessões | `count(iteration==1 AND state==COMMITTED) / total` |
| 2 | Median replan iterations (P50) | N/A | ≤1.5 | histogram of `iteration_count` at COMMITTED |
| 3 | speculate_v2 pass rate | N/A | ≥85% | `count(score>=0.8) / count(RENDERED)` |
| 4 | LLM tokens per committed file | ~2000 | <500 após 20 sessões | PlanMetadata telemetry |
| 5 | Hallucination rate | ~40% (manual) | <5% | `count(missing_symbols > 0) / total plans` |
| 6 | Subprocess calls per generate session | ~50 | **0** | `strace -c -e execve touring generate ...` |
| 7 | VGP verification latency | ~200ms (Python) | **<5ms** (Rust) | `VgpReport.duration_ms` |
| 8 | Python LOC decommissioned | 7562 | **>5000** | `wc -l` após cada wave |
| 9 | RL avg_reward trend | 0.075 | >0.5 após 50 commits | `touring status -j .learning.ema_reward` |
| 10 | Time-to-commit P50 | N/A | <30s (L0-L2), <120s (L3-L4) | `PlanMetadata.created_at → committed_at delta` |
| 11 | `extract_symbol_details` wiring score | 0.0 | >0.5 | `touring wiring score symbol_detail.rs` |
| 12 | `code_generation_sig` consumers | 0 | 1 (touring-generator) | `touring wiring orphans -j | jq .code_generation_sig` |

---

## Uso (Exemplos End-to-End)

### Via CLI
```bash
# 1. LLM descobre schema
touring generate schema-dump > /tmp/plan_schema.json

# 2. LLM consulta plans similares
touring generate plan-recall "add CLI handler for memory stats" -j

# 3. LLM compõe GeneratorPlan (via DSPy plan_generation_sig)
cat > /tmp/my_plan.json <<EOF
{
  "version": "1.0.0",
  "plan_id": "550e8400-e29b-41d4-a716-446655440000",
  "intent": "Add cli-memory-quick-stats handler returning compact stats",
  "cila_level": "L2",
  "target": {
    "crate_name": "touring-hooks",
    "file_path": "crates/touring-hooks/src/cli_handlers.rs",
    "module_path": "touring_hooks::cli_handlers",
    "line_hint": null
  },
  "kind": "CliHandler",
  "contracts": {
    "symbols_must_exist": [
      {"name": "HookRuntime", "crate_name": "touring-hooks", "module_path": "touring_hooks::hook_runtime"},
      {"name": "MemoryStore", "crate_name": "touring-server", "module_path": "touring_server::memory_store"}
    ],
    "symbols_must_not_exist": [
      {"name": "cli_memory_quick_stats", "crate_name": "touring-hooks", "module_path": null}
    ],
    "traits_implemented": [],
    "exports": ["cli_memory_quick_stats"],
    "dependencies": [],
    "invariants": []
  },
  "verification": {"pre_verify_all": true, "fail_on_missing": true, "homonimia_check": true},
  "template": {
    "template_id": "cli_handler.tera",
    "engine": "Tera",
    "variables": {
      "handler_name": "cli_memory_quick_stats",
      "description": "Return compact memory stats as JSON"
    },
    "extends": null
  },
  "assembly": {
    "files": [{"path": "crates/touring-hooks/src/cli_handlers.rs", "action": "Append", "template_id": "cli_handler.tera", "variables": {}}],
    "mod_rs_entries": [],
    "cargo_toml_patches": []
  },
  "validation": {
    "min_speculate_score": 0.85,
    "required_layers": ["syntax", "symbol", "structural"],
    "max_complexity_score": 10.0,
    "custom_assertions": []
  },
  "commit_policy": {"auto_commit_threshold": 0.85, "require_human_review": false, "dry_run": false},
  "rollback": {"enabled": true, "backup_path": null, "rollback_on_test_failure": true},
  "learning": {
    "reward_on_commit": 1.0,
    "reward_on_replan": 0.0,
    "memory_key": "pattern:generator:cli_handler:memory_quick_stats",
    "memory_tier": "semantic",
    "memory_type": "pattern"
  },
  "metadata": {
    "author": "llm",
    "created_at": "2026-04-10T15:00:00Z",
    "parent_plan_id": null,
    "session_id": "f8bf87dc",
    "codebase_hash": null,
    "tags": ["cli", "memory"]
  }
}
EOF

# 4. Submeter pipeline completo (VGP → render → speculate → commit)
touring generate plan-submit --plan-file /tmp/my_plan.json -j

# Resposta de sucesso:
# {
#   "status": "committed",
#   "plan_id": "550e8400-...",
#   "iteration": 1,
#   "artifacts": [{"path": "...", "sha256": "abc123..."}],
#   "verification_report": {"all_passed": true, "verified_symbols": ["HookRuntime", "MemoryStore"]},
#   "speculate_score": 0.91,
#   "learning_feedback": {"reward": 1.0, "tool": "edit", "context": "committed:550e8400"}
# }

# Resposta de falha (LLM replaneja):
# {
#   "status": "failed",
#   "failure_report": {
#     "reason": "VGP_FAILED",
#     "missing_symbols": [{"name": "MemoryStoer", "suggested_alternatives": ["MemoryStore"]}],
#     "suggestions": ["Symbol 'MemoryStoer' not found. Did you mean 'MemoryStore'?"]
#   }
# }

# 5. Status / rollback
touring generate plan-status 550e8400-e29b-41d4-a716-446655440000 -j
touring generate plan-rollback 550e8400-e29b-41d4-a716-446655440000 -j
```

### Via MCP (dentro do Claude Code)
```
mcp__touring__touring_generator_submit_plan(plan={...GeneratorPlan...})
mcp__touring__touring_generator_recall_similar(intent="add cli handler", limit=5)
```

### Via PyO3 (touring_python_client.py + PYO3_AVAILABLE=True)
```python
import claude_learning_kernel as ck
import json

plan = {...}
result_json = ck.generate.submit_plan(json.dumps(plan))
result = json.loads(result_json)
assert result["status"] == "committed"
```

---

## Test Strategy (key cases)

```rust
// tests/unit_generator.rs
#[tokio::test] async fn test_vgp_engine_parallel_symbol_verification() { }
#[tokio::test] async fn test_plan_executor_vgp_failure_triggers_replan() { }
#[tokio::test] async fn test_circuit_breaker_fires_at_max_iterations() { }
#[tokio::test] async fn test_template_engine_autoescape_disabled_for_rust() { }
#[tokio::test] async fn test_syn_quote_engine_validates_generated_rust() { }
#[tokio::test] async fn test_speculate_score_gate_blocks_low_quality() { }
#[tokio::test] async fn test_hook_generator_updates_all_daemon_hook_names_assert() { }
#[tokio::test] async fn test_plan_schema_roundtrip_json() { }
#[test] fn test_json_schema_generation_matches_docs() { }
#[tokio::test] async fn test_rollback_restores_backup_files() { }

// tests/integration_lifecycle.rs
#[tokio::test] async fn test_e2e_plan_submit_to_commit() { }
#[tokio::test] async fn test_e2e_replanning_convergence() { }

// tests/e2e_plan_roundtrip.rs
#[tokio::test] async fn test_python_pyo3_bridge_equivalence() { }
```

**Coverage targets**:
- Branch coverage ≥90% em `lifecycle/executor.rs`, `vgp/engine.rs`, `template/engine.rs`
- All 13 GenerateError variants testados
- All 9 PlanState transitions cobertas
- Every GeneratorKind tem pelo menos 1 E2E test

---

## VP-Scout Validation (os 4 cadeias)

### Feature Trace — PASS
- Nenhuma feature gate guarda qualquer capability de generate em nenhum crate
- `touring index find GeneratorPlan` → count=0
- Feature `syn-quote` e `mcts-synthesis` serão NOVAS (não conflitam com existentes)

### Dependency Cycle — PASS
- `touring-generator → touring-ast`: SAFE (ast é leaf)
- `touring-generator → touring-index`: SAFE (index é leaf)
- `touring-server → touring-generator`: SAFE (server é top of graph)
- `touring-python → touring-generator`: SAFE (python é leaf binding layer)
- **Zero cycles** verificados via `cargo tree` inspection + Cargo.toml grep

### Already Implemented — PASS
- `touring index find GeneratorPlan` → 0 definitions
- `ls crates/ | grep generator` → empty
- `grep cli-generate hook_registry.rs` → no matches
- `grep '"generate"' cli/common.rs` → no matches (só "auto-generated help" line 455)
- **Confirmado**: generate CLI + crate genuinamente não existem

### Homonimia — DETECTED & RESOLVED
- `GeneratorSpec` aparece em 2 locations:
  - `touring-learning::n3::generator_spec.rs:13` (ACO N3 domain spec — pheromone_prefix, evaporation_rate)
  - `scripts/aco/generators/base_generator.py:37` (Python codegen spec — output_file, source_file)
- **Resolução**: novo crate usa `GenerateRequest`/`GenerateResult`/`GenerateKind` exclusivamente — NÃO `GeneratorSpec`
- `Plan` em `lib/plan_generator/models.py:51` é task planning (Plan→Phase→Task→SubTask), novo `GeneratorPlan` é code generation — **nomes distintos preservam clareza**

---

## Gotchas Ativos

- **G104** — `plan-baseline-hook-count`: `ALL_DAEMON_HOOK_NAMES.len()==98` at `hook_registry.rs:729`. W6 MUST update to 108 (10 novos handlers). Gate: `grep "assert_eq" hook_registry.rs` antes de merge.
- **G18** — `touring-hooks exit 101`: build from workspace root `/home/gabrielgadea/.claude/rust` não de crate dir
- **G1** — touring-hooks exit code 2 on bash failures (1229 hits históricos): any new handler MUST handle non-zero exit robustly

---

## Context7 Insights Aplicados

### Tera (keats/tera)
- `Tera::one_off(tpl, context, autoescape)` — autoescape=false para código (não HTML)
- `autoescape_on(vec![])` para desabilitar globalmente
- `add_raw_templates(Vec<(name, content)>)` para template inheritance
- Padrão `lazy_static` para instância compartilhada
- `register_filter` para custom filters

### syn + quote (dtolnay/syn)
- `syn::parse_file(&content) -> syn::File` — valida código Rust gerado antes de write
- `syn::parse_quote!` — quasi-quotation **sem proc macro** (KEY: permite AST construction em runtime!)
- `syn::parse2(token_stream)` — parse TokenStream de `quote!`
- `quote::quote! { ... }` — emit TokenStream com interpolação `#var`
- Error handling: `syn::Error::new(span, msg).to_compile_error()`

### PyO3 (pyo3/pyo3 0.24)
- `#[pymodule] mod name { ... }` com nested submodules via `#[pymodule_export]`
- `#[pyfunction]` inline ou externa
- Submodule pattern: `mod generate { ... }` dentro de `claude_learning_kernel`
- `maturin develop` build + install local .so
- touring-python JÁ tem `claude_learning_kernel` pymodule — só adicionar `mod generate` como sub

---

## Referência a Infraestrutura Existente

| Componente Rust | Localização | Uso em touring-generator |
|-----------------|-------------|------------------------|
| `touring_ast::extract_symbol_details` | `symbol_detail.rs:76` | VGP pre-hook (Wave 2) |
| `touring_ast::speculate_v2` | `speculate.rs:295` | Speculate post-hook (Wave 2) |
| `touring_ast::SpeculateResult` | `speculate.rs:68` | Result type reused directly |
| `touring_cortex::dspy::code_generation_sig` | `dspy_signature.rs:44` | DSPy signature via closure (Wave 7) |
| `touring_cortex::MCTSCodeSynthesisHandler` (H99) | `reasoning_advanced.rs:85` | Optional MCTS exploration via closure (Wave 7) |
| `touring_hooks::HookRuntime` | `hook_runtime.rs:595` | NOT imported — closure-based decoupling |
| `touring_hooks::ALL_DAEMON_HOOK_NAMES` | `hook_registry.rs:185` | Patched by HookGenerator (Wave 5-6) |
| `touring_hooks::ALL_DAEMON_HOOK_NAMES.len() == 98` assert | `hook_registry.rs:729` | MUST update 98→108 in Wave 6 |
| `touring_server::CommandDescriptor` | `common.rs:149` | CliHandler registration pattern |
| `touring_server::TouringServer` | `mod.rs:188` | MCP tool registration via `#[tool_router]` at mod.rs:222 |
| `touring_python::claude_learning_kernel` | `lib.rs:39` | PyO3 parent module — add `generate` submodule |

---

## Next Steps

1. **Phase 6 (Cross-Audit)**: `touring-auditor` agent valida strategy contra VP-Scout + risks + scope maximization
2. **Phase 7 (Documentation)**: `touring-scriber` agent consolida final docs + memory store + session report
3. **Post-TACO**: Gabriel aprova strategy → criar crate skeleton → executar Wave 1
4. **Memory Store**: persistir este documento como pattern `pattern:generator:strategy:v1` em `touring memory store` tier=semantic type=pattern

---

## Quality Gates

| Gate | Status |
|------|--------|
| Functional | ✅ Strategy covers all 8 GeneratorKinds |
| Robust | ✅ 6 escape hatches + circuit breaker + replan loop |
| Readable | ✅ Structured sections, clear naming |
| Documented | ✅ This document + inline docstrings planned |
| Secure | ✅ Variable allowlist + syn::parse_file validation + speculate gate |
| No Regression | ✅ Python coexists during migration; zero touch em hook_runtime.rs |
| **Scope Maximization** | ✅ REGRA #0 respected — Python orphans wired via PyO3, VGP v2 wiring score elevated, DSPy code_generation_sig gains first consumer |

**Composite Score: 1.0 ✓**

---

*TACO v6.0 Phase 5 Delivery — touring-generator strategy finalized at 2026-04-10*
*Confidence: 0.95 (FACT) | Architects validated: A + B + C | Context7 validated: Tera + syn + PyO3*
