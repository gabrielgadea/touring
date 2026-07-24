# Análise Completa: touring-generator + Estratégias de Integração

> **Data**: 12/04/2026 | **Versão**: v1.0 | **Autor**: TACO Orchestrator
> **Escopo**: Análise profunda do touring-generator Rust crate, mapeamento dos Python scripts predecessores,
> estratégias de integração com Claude Code, e proposta de unificação com o sistema de tarefas touring.

---

## 1. Estado Atual — touring-generator (Rust)

### 1.1 Métricas

| Métrica | Valor |
|---------|-------|
| Source LOC | 5.671 |
| Test LOC | 3.728 |
| Total LOC | 9.399 |
| Generator Kinds | 30 |
| Tera Templates | 29 (pre-compiled OnceLock) |
| Feature-Gated Adapters | 10 (todos ativados) |
| CLI Subcommands | 24 (em touring-server) |
| MCP Tools | 20 (em touring-server) |
| Tests | ~203 (32 unit + 130 E2E + 41 cross-audit) |
| Benchmarks | 3 (vgp_engine, template_engine, speculate_bridge) |

### 1.2 Módulos Principais

| Arquivo | LOC | Propósito |
|---------|-----|-----------|
| `src/core/context.rs` | 2.369 | GeneratorContext + 10 feature-gated adapters |
| `src/executor/typestate.rs` | 496 | Pipeline typestate (Draft→Committed) |
| `src/vgp/engine.rs` | 579 | VGP verification engine (moka + rayon) |
| `src/plan/schema.rs` | 322 | GeneratorPlan JSON schema (schemars) |
| `src/plan/result.rs` | 234 | RenderedFile, CommitReport, VgpReport |
| `src/template/engine.rs` | 169 | TemplateEngine (OnceLock Tera, 29 templates) |
| `src/speculate/bridge.rs` | 121 | SpeculateBridge (touring shadow validate) |
| `src/generator/kinds.rs` | 170 | GeneratorKind enum (30 variants) |
| `src/error.rs` | 158 | GenerateError |

### 1.3 Typestate Pipeline (compile-time safety)

```
Draft ──verify()──► Verified ──render()──► Rendered ──speculate()──► Speculated ──commit()──► Committed
         │               │                    │                          │
      VgpEngine      Templates            Shadow                   CommitReport
      (moka+rayon)   (29 Tera)          Validate                   + RL Reward
```

Cada estado é um sealed struct com `PhantomData<S>`. Transições inválidas são erros de compilação.

### 1.4 Os 30 GeneratorKinds

**Rust Source (13)**:
RustModule, CliHandler, McpTool, HookHandler, Test, BenchmarkSuite, FuzzTarget,
DeriveMacro, AttributeMacro, FunctionMacro, ErrorCatalog, IncrementalPatch, FfiBinding

**Data/Schema (5)**:
Schema, MigrationScript, ProtoBufSchema, OpenApiSpec, AsyncApiSpec

**Documentation (7)**:
PlanMarkdown, SkillDocument, DiaryEntry, ChangelogEntry, Adr, ShellCompletion, ManPage

**Infrastructure (5)**:
PythonScript, DockerImage, KubernetesManifest, TerraformModule, CiWorkflow, ConsumerGenerator

### 1.5 Os 10 Adapters Feature-Gated

| Adapter | Feature | Função | Closure Target |
|---------|---------|--------|----------------|
| BkTreeFuzzyAdapter | simd-fuzzy | Levenshtein fuzzy match via symbol pool | `fuzzy_index` |
| SynWiringGateAdapter | syn-quote | Rust AST validation + rejeita allow(dead_code) | `wiring_gate_fn` |
| AnalysisGateAdapter | analysis-gate | DB wiring analysis, orphan delta ≤5 | `wiring_gate_fn` |
| RkyvFileSnapshotAdapter | zero-copy | Rollback <100µs via rkyv serialization | snapshot/restore |
| WasmSandboxAdapter | wasm-sandbox | WASM defense-in-depth validator | `wasm_sandbox_fn` |
| McctsEvalAdapter | mcts-synthesis | MCTS planning via SemanticGraph | `mcts_eval_fn` |
| TracingTelemetrySink | observability | AtomicU64 counters + tracing events | `metrics` |
| NlpPlanRankerAdapter | nlp-reranking | Aho-Corasick keyword ranking | `cognitive_nexus_fn` |
| SemanticGraphAdapter | cognitive-nexus | Concept nodes + neighbor scoring | `semantic_graph_fn` |
| LinUCBRewardSink | rl-integration | RL reward injection via OnlineRLEngine | `rl_sink` |

---

## 2. Python Scripts Predecessores

### 2.1 VGP (`~/.claude/scripts/vgp/`)

| Módulo | Propósito | Rust Equivalente |
|--------|-----------|------------------|
| `models.py` | SymbolInfo, VerificationResult, CacheStats | VgpReport + NormalizedScore |
| `patterns.py` | 9 regex patterns para Python | N/A (Rust usa index direto) |
| `cache.py` | TTLCache 300s, 1000 entries | moka cache TTI 5min/TTL 1h, 10k entries |
| `verifier.py` | verify_symbol com retry 3x (1s/2s/4s) | VgpEngine com IncrementalIndex fast-path |
| `parallel.py` | ThreadPoolExecutor 4 workers | rayon pool (CPU-count - 2) |
| `cli.py` | CLI: `vgp extract <file>` | `touring generate verify --file` |

### 2.2 ACO Generators (`~/.claude/scripts/aco/generators/`)

| Módulo | Propósito | Rust Equivalente |
|--------|-----------|------------------|
| `base_generator.py` | TouringGeneratorBase (pre/post hooks) | PlanExecutor typestate pipeline |
| `validate_generator.py` | 6 structural checks | SynWiringGateAdapter + cross-audit tests |
| `rollback_generator.py` | Rollback via touring memory | rollback_plan() + rkyv snapshot |
| `gen_generator.py` | Meta-generator (gera generators) | ConsumerGenerator kind (parcial) |

### 2.3 Plan Generator (`~/.claude/lib/plan_generator/`)

| Módulo | Propósito | Rust Equivalente |
|--------|-----------|------------------|
| `models.py` | Plan, Phase, Task, SubTask | GeneratorPlan JSON Schema |
| `generators.py` | 13 generators (session/memory/decompose) | 24 CLI + 20 MCP tools |
| `cli.py` | --dry-run, --validate, --audit, RL reward | CLI subcommands |
| `audit.py` | check_json, check_doctor, touring dispatch | touring e2e --depth deep |

---

## 3. Mapa Comparativo — Python vs Rust

| Capacidade | Python | Rust | Veredito |
|-----------|--------|------|----------|
| Symbol verification | TTLCache + subprocess | moka + IncrementalIndex fast-path | **Rust >>>** |
| Parallel verification | ThreadPoolExecutor 4w | rayon pool (CPU-count) | **Rust >>>** |
| AST validation | 6 string checks | syn parse + orphan analysis | **Rust >>>** |
| Rollback | touring memory + file rm | rkyv zero-copy (<100µs) | **Rust >>>** |
| Plan modeling | frozen dataclasses | JSON Schema (schemars) | **Rust >>>** |
| Template rendering | Python f-strings | 29 pre-compiled Tera templates | **Rust >>>** |
| CLI interface | Manual scripts | 24 subcommands + 20 MCP tools | **Rust >>>** |
| RL feedback | subprocess `touring learning reward` | LinUCBRewardSink direto | **Rust >>>** |
| **Session lifecycle** | auto session start/checkpoint/assess | **AUSENTE** | **GAP** |
| **Cache stats API** | CacheStats struct | AtomicU32 internos | **GAP** |
| **Meta-generator** | gen_generator.py | ConsumerGenerator (limitado) | **GAP** |
| **Task decompose integration** | tpc.decompose_create/add/validate | **ZERO referências** | **GAP CRÍTICO** |

---

## 4. GAP CRÍTICO: Isolamento Generator ↔ Decompose

### 4.1 Estado Atual

O touring-generator e o touring decompose (sistema de tarefas) são **completamente isolados**:

- `touring-generator`: Pipeline typestate para gerar artefatos de código
- `touring-server/reasoning/decomposer.rs`: DAG de tarefas com subtasks, dependências, status

**Zero referências** entre os dois sistemas. O GeneratorPlan não conhece SubTask.
O TaskDecomposer não conhece GeneratorKind.

### 4.2 O Sistema de Tarefas Touring (decomposer.rs)

```rust
// crates/touring-server/src/reasoning/decomposer.rs

pub struct Task {
    pub id: String,              // "task_<N>"
    pub task_type: String,       // "refactor", "debug", "feature", etc.
    pub description: String,
    pub subtasks: Vec<SubTask>,  // DAG de subtasks
}

pub struct SubTask {
    pub id: String,
    pub description: String,
    pub status: SubTaskStatus,   // Pending → InProgress → Completed/Failed
    pub depends_on: Vec<String>, // DAG edges
    pub priority: u8,
    pub complexity: Option<ComplexityHint>,
    pub retry_policy: Option<RetryPolicy>,
}

pub enum SubTaskStatus {
    Pending, InProgress, Completed, Failed, Skipped,
}
```

**Capacidades**:
- Kahn's algorithm para topological sort
- `parallel_groups()` identifica camadas de execução concorrente
- CILA routing (L0-L4) para nível de decomposição
- ACO pheromone tracking via ComplexityHint.tags
- Retry/timeout policies per subtask
- 22 tasks, 168 subtasks ativos atualmente

### 4.3 Oportunidade de Integração

O que DEVERIA acontecer:

```
GeneratorPlan JSON → touring_decompose (create task)
  ├─ SubTask S-1: VGP verify (auto)
  ├─ SubTask S-2: Template render (auto)
  ├─ SubTask S-3: Shadow validate (auto)
  └─ SubTask S-4: Commit (auto)
```

Cada estágio do typestate pipeline deveria criar/atualizar uma subtask no decompose.
Isso daria:
- **Observabilidade**: cada plan execution visível no `touring decompose status`
- **Retry**: subtask com RetryPolicy automática
- **Parallel groups**: múltiplos plans paralelos identificados por camada
- **Histórico**: touring memory armazena plan → task mapping

---

## 5. Estratégias de Integração

### Estratégia 1 — Skill Claude Code (P1 — Impacto Imediato, Zero Code)

**O que**: Criar `~/.claude/skills/touring-generator/SKILL.md` que ensina o Claude Code a usar
touring-generator automaticamente quando detecta necessidade de code generation.

**Triggers**: "gerar módulo", "criar hook handler", "adicionar MCP tool", "criar template"

**Fluxo**:
1. `touring generate schema-dump` → obter schema do GeneratorPlan
2. Construir GeneratorPlan JSON conforme schema
3. `touring_generator_submit_plan` → pipeline completo
4. `touring generate plan-status` → verificar output

**ROI**: Altíssimo. Zero mudança de código. O Claude Code já tem acesso aos 20 MCP tools.

### Estratégia 2 — Session Lifecycle Closures (P2 — Fecha Gap Principal)

**O que**: Adicionar 3 closures opcionais ao `GeneratorContext`:

```rust
pub session_start_fn: Option<Box<dyn Fn(&str, &str) -> Result<String> + Send + Sync>>,
pub session_checkpoint_fn: Option<Box<dyn Fn(&str, &str) -> Result<()> + Send + Sync>>,
pub session_assess_fn: Option<Box<dyn Fn(&str) -> Result<f64> + Send + Sync>>,
```

**Wiring**: touring-server/tools/generator_tools.rs::make_context() injeta closures que
chamam `touring session start/checkpoint/assess` via daemon socket.

**Impacto**: Cada plan execution cria sessão touring, checkpoints, e assessment automaticamente.

### Estratégia 3 — Generator ↔ Decompose Bridge (P3 — Integração Crítica)

**O que**: Criar bridge bidirecional entre GeneratorPlan e TaskDecomposer.

**Direção 1: Plan → Task**

Quando um `GeneratorPlan` é submetido via `submit_plan`:
1. Auto-criar `Task` no decompose com `task_type = "generator"`
2. Cada estágio typestate → `SubTask`:
   - S-1: "VGP Verify" (depends_on: [])
   - S-2: "Template Render" (depends_on: [S-1])
   - S-3: "Shadow Validate" (depends_on: [S-2])
   - S-4: "Commit" (depends_on: [S-3])
3. Status transitions do typestate atualizam SubTaskStatus

**Direção 2: Task → Plan**

Quando uma `SubTask` do decompose tem `tags: ["generate"]`:
1. Auto-criar GeneratorPlan a partir da description
2. Invocar pipeline typestate
3. Resultado da geração atualiza SubTaskStatus

**Implementação proposta**:

```rust
// Em touring-generator/src/executor/typestate.rs
pub struct DecomposeBridge {
    task_id: String,
    subtask_mapping: HashMap<PlanStage, String>, // stage → subtask_id
}

impl<S: PlanStage> PlanExecutor<S> {
    fn update_decompose(&self, stage: &str, status: SubTaskStatus) {
        if let Some(bridge) = &self.decompose_bridge {
            // touring decompose update <task_id> <subtask_id> <status>
        }
    }
}
```

### Estratégia 4 — Deprecação Gradual dos Python Scripts (P4)

**Timeline de migração**:

| Fase | Ação | Python Script | Rust Redirect |
|------|------|--------------|---------------|
| A (imediata) | Wrapper | `vgp/cli.py extract` | `touring generate verify` |
| A (imediata) | Wrapper | `aco/validate_generator.py` | `touring generate plan-validate` |
| A (imediata) | Wrapper | `aco/rollback_generator.py` | `touring generate plan-rollback` |
| B (1 mês) | Monitor | Tracking usage via touring learning | RL observa chamadas Python |
| C (2 meses) | Archive | Python scripts movidos para `archive/` | Touring-only |

### Estratégia 5 — RL Feedback Loop Completo (P5)

**O que**: Cada transição typestate injeta reward automaticamente:

| Transição | Reward | Context |
|-----------|--------|---------|
| Draft → Verified | `reward("verify", vgp_score)` | Score do VGP report |
| Verified → Rendered | `reward("render", 1.0)` | Template renderizado com sucesso |
| Rendered → Speculated | `reward("speculate", shadow_score)` | Score da shadow validation |
| Speculated → Committed | `reward("commit", 1.0)` | Commit realizado |
| Falha em qualquer estágio | `reward(stage, -0.5)` | Diagnóstico do erro |

### Estratégia 6 — Hook Runtime Auto-Suggest (P6)

**O que**: Handler em hook_runtime.rs que detecta:

| Evento | Ação |
|--------|------|
| PreWrite em `.rs` | Oferece `touring generate verify` automaticamente |
| PostEdit em `templates/*.tera` | Auto-reload template engine |
| SessionStart | `touring generate plan-recall` para planos anteriores |
| TaskCreated com tag "generate" | Auto-submete GeneratorPlan |

---

## 6. Integração com Estruturas de Tarefas do Touring

### 6.1 Arquitetura Proposta: Generator-Decompose Unified Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│ Claude Code (User Request: "criar módulo X")                    │
│   ↓ SKILL touring-generator detecta intent                      │
├─────────────────────────────────────────────────────────────────┤
│ touring_decompose (Task Layer — N2)                              │
│   create_task("generator:rust_module:X")                         │
│   ├── S-1: vgp_verify [Pending]                                  │
│   ├── S-2: template_render [Pending] depends_on:[S-1]            │
│   ├── S-3: shadow_validate [Pending] depends_on:[S-2]            │
│   └── S-4: commit_artifacts [Pending] depends_on:[S-3]           │
├─────────────────────────────────────────────────────────────────┤
│ touring-generator (Typestate Pipeline — N1)                      │
│   Draft ──► Verified ──► Rendered ──► Speculated ──► Committed   │
│     │          │            │            │              │         │
│   S-1:IP    S-1:OK       S-2:IP       S-3:IP        S-4:OK      │
│             S-2:IP       S-2:OK       S-3:OK                     │
│                                                                   │
│   [Cada transição atualiza SubTaskStatus no decompose]            │
├─────────────────────────────────────────────────────────────────┤
│ touring-daemon (Persistence Layer)                                │
│   Memory: plan manifest + created files                           │
│   Session: lifecycle tracking (start/checkpoint/assess)           │
│   Learning: RL reward per stage transition                        │
│   Wiring: orphan tracking + integration score                     │
│   Decompose: task/subtask status + parallel_groups                │
└─────────────────────────────────────────────────────────────────┘
```

### 6.2 Pontos de Integração Task ↔ Generator

| Ponto | Touring Task API | Generator Pipeline |
|-------|-----------------|-------------------|
| **Criação** | `touring_decompose(create, "generator:kind:name")` | `PlanExecutor::new(plan)` |
| **VGP** | `update_status(S-1, InProgress)` → `update_status(S-1, Completed)` | `Draft.verify()` → `Verified` |
| **Render** | `update_status(S-2, InProgress/Completed)` | `Verified.render()` → `Rendered` |
| **Speculate** | `update_status(S-3, InProgress/Completed)` | `Rendered.speculate()` → `Speculated` |
| **Commit** | `update_status(S-4, Completed)` | `Speculated.commit()` → `Committed` |
| **Falha** | `update_status(S-N, Failed)` + retry_policy | Pipeline error → replan |
| **Rollback** | `update_status(S-*, Skipped)` | `rollback_plan()` |
| **Parallel** | `parallel_groups()` → camadas concorrentes | Múltiplos plans simultâneos |
| **Prioridade** | `SubTask.priority` | `GeneratorPlan.priority` |
| **Complexidade** | `ComplexityHint { estimated_minutes, tags }` | `CapacityLimits` |

### 6.3 Fluxo Completo: Da Tarefa ao Artefato

```
1. TACO Orchestrator cria Task via touring_decompose
   → task_type: "generator", description: "Criar HookHandler para pre-edit enrichment"

2. Decompose auto-decompõe em subtasks (via CILA routing):
   → S-1: "Escolher GeneratorKind" (L0, auto: HookHandler)
   → S-2: "VGP Verify symbols referenciados" (L1)
   → S-3: "Render template hook_handler.tera" (L1)
   → S-4: "Shadow validate resultado" (L2)
   → S-5: "Commit e registrar no wiring" (L2)

3. Generator Pipeline executa typestate:
   Draft(plan) → verify(VGP) → render(Tera) → speculate(shadow) → commit(write)
   
   Em cada transição:
   - SubTask status atualizado via touring_decompose(update_status)
   - Session checkpoint via touring session checkpoint
   - RL reward via touring learning reward
   - Memory store via touring memory store

4. Resultado:
   - Arquivo .rs criado no filesystem
   - Wiring registrado (pub symbols tracked)
   - Task marked Completed no decompose
   - Session assessed com quality score
   - RL engine atualizado com reward
```

### 6.4 Validação de Tarefas via Generator

O touring-generator pode ser usado para VALIDAR tarefas do decompose:

```
touring_decompose(get_plan, task_id)
  → Para cada subtask com tag "validate":
    → touring generate plan-validate
    → Se falha: update_status(Failed) + retry

touring_decompose(validate_order, task_id)
  → Kahn's algorithm verifica DAG
  → Generator verifica que cada kind tem template
  → Wiring gate verifica orphan delta
```

---

## 7. Prioridades de Implementação

| # | Ação | Esforço | ROI | Dependências | Status |
|---|------|---------|-----|-------------|--------|
| P1 | Criar SKILL.md touring-generator | 30min | Altíssimo | Nenhuma | DONE (2026-04-12) |
| P2 | Session lifecycle closures | 2h | Alto | P1 | DONE (2026-04-12) |
| P3 | Generator ↔ Decompose bridge | 4h | Altíssimo | P2 | DONE (2026-04-12) |
| P4 | Pheromone + DSPy closures (S-1/S-4) | 2h | Alto | P2 | DONE (2026-04-12) — S-1: build_pheromone_fn() wires UnifiedPheromoBus(0.05); S-4: build_dspy_closure() wires DspyCortexAdapter under cognitive-nexus |
| P5 | RL reward propagation to daemon (S-2) | 1h | Alto | P2 | DONE (2026-04-12) — inject_daemon_rl_reward() fire-and-forget tokio::spawn |
| P6 | Post-commit reindex (S-3) | 1h | Alto | P2 | DONE (2026-04-12) — touring index rebuild per unique parent dir of committed files |
| P7 | Hook runtime auto-suggest | 2h | Alto | P3 | PENDING |
| P8 | Python deprecation wrappers | 1h | Médio | P1 | PENDING |
| P9 | ConsumerGenerator expansion | 3h | Médio | P3 | PENDING |

### Critical Path: P1 → P2 → P3 → P4/P5/P6 (DONE) → P7

### Implementation Summary (2026-04-12 Synergy Session)

All generator-hooks synergy tasks S-1 through S-4 completed in `generator_tools.rs`:

**S-1 — Pheromone wiring**: `build_pheromone_fn()` creates `Arc<Mutex<UnifiedPheromoBus::new(0.05)>>`, deposits to `PheroKey::TemplateId(tool)` with `score.value()`. Activates 4 dormant `pheromone_update()` sites in typestate.rs (lines 133, 245, 332, 426).

**S-2 — RL reward propagation**: `inject_daemon_rl_reward(tool, reward, context)` spawns fire-and-forget `tokio::spawn` calling `touring learning reward` subprocess on commit success.

**S-3 — Post-commit reindex**: `speculate_and_commit()` Ok arm deduplicates parent dirs from `files_written` and spawns `touring index rebuild --dir` per unique dir.

**S-4 — DSPy closure**: `build_dspy_closure()` under `#[cfg(feature = "cognitive-nexus")]` wraps `DspyModule::new(code_generation_sig())` with `HashMap<String,Value>` ↔ `HashMap<String,String>` adapter. Injected as `inner.dspy_sig_fn`.

**Feature gate cleanup**: Removed `rl-feedback` from touring-offensive (cyclic dep) and `burn-transformer` from touring-learning (dep:burn unavailable). `cargo check --all-features` now passes with 0 errors.

---

## 8. Métricas de Sucesso

| Métrica | Baseline | Target |
|---------|----------|--------|
| Python script invocations / sessão | ~5 | 0 (100% via touring-generator) |
| Generator plans tracked no decompose | 0 | 100% (bridge ativo) |
| Session lifecycle auto-calls | 0 | 3 por plan (start/checkpoint/assess) |
| RL rewards por pipeline execution | 0 | 4-5 (por transição) |
| Wiring orphan delta per generation | Untracked | ≤5 (analysis-gate enforced) |
| Plan execution latency P99 | Untracked | <500ms (VGP fast-path) |

---

*Documento gerado pelo TACO Orchestrator v6.2 — Sessão a5002c4a — 12/04/2026*
