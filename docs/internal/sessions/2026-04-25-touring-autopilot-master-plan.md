# Touring-Autopilot — Master Plan

> **Status**: SPEC v1.0 (constitutional draft) | **Author**: TACO (Claude Opus 4.7) under Gabriel's authorization | **Date**: 2026-04-25 | **Risk class**: HIGH | **Authorization required**: explicit Gabriel opt-in per phase
>
> This document is the constitutional reference for the Touring-Autopilot subsystem. Implementation MUST quote this spec by section number when justifying decisions. Deviations require Gabriel's explicit approval and an addendum at the bottom of this file.

---

## 0. Sumário Executivo

**O que é**: um co-processador proativo que observa o estado do workspace touring continuamente, detecta oportunidades/problemas/riscos via os 22 crates touring existentes, **propõe** ações com evidência verificável, e aprende com a aceitação/rejeição do Gabriel — sem nunca aplicar mudanças sem autorização explícita por escopo.

**Linhagem (genealogia)**: touring-autopilot **subsume e estende A1** (Autonomous detect-propose loop) do plan original `2026-04-24-waves-Q-R-M-A-T-P-plan.md`. A1 era a semente conceitual ("autonomous detect-propose"); o autopilot é a forma adulta — mesmo princípio (detectar+propor sem aplicar), agora especificado de ponta-a-ponta com os 22 crates touring como substrato. **A task #64 (A1) deve ser fechada como "succeeded by autopilot"**; o trabalho continua sob este documento.

**Companion document — EXTERNAL EXPANSION**: este master plan especifica autopilot operando DENTRO de `~/.claude/rust/`. A operação em workspaces externos (analise/, kazuba-cargo/, templates/, projetos novos) é especificada em `~/.claude/rust/docs/2026-04-25-touring-autopilot-expansion-plan.md`.

**Pre-A AUTORIZADA por Gabriel em 2026-04-25** ✅ — as abstrações `WorkspaceProfile` + `WorkspaceCapability` + `WorkspaceRegistry` + `.autopilot/manifest.toml` parser (expansion §3) entram como **Phase Pre-A** (M size, 3-5 dias) ANTES de Phase A. Custo agora: 5 dias. Custo se pulado: 6 semanas de retrofit em Phase J + rewrite de todos os 16 detectors. Ver §12 Phase Pre-A para sub-tasks atómicos (Pre-A.1 → Pre-A.8) e acceptance criteria. Phase A passa a consumir essas abstrações desde o detector #1 — nenhum detector hard-codeia `&Path`.

**Por que agora**: o ecossistema touring atingiu massa crítica (45.962 símbolos indexados, 1.235 módulos, 5.777 pub symbols, 17 crates ativos, ~5.100 testes verdes, RL convergindo, gotcha DB povoada, mutation testing wired em T1+T2). A infra reativa (responde-quando-perguntada) está saturada — o próximo salto de produtividade vem de fechar o loop **percepção → diagnóstico → proposta → decisão → aprendizado** de forma autônoma na detecção e proposta, mantendo decisão sob controle humano.

**Princípio rector** (não-negociável): **Detect autonomously, propose with evidence, decide with the human.** Autopilot nunca executa mudança sem opt-in explícito por (categoria, escopo, autonomy level).

**Posição relativa ao TACO**: TACO é o orquestrador imperativo (Gabriel diz → TACO faz). Autopilot é o co-piloto declarativo (touring observa → autopilot propõe → Gabriel decide → TACO executa). Ortogonais, complementares.

**Tamanho estimado**: 4-6 semanas para Phase A-E (foundation + triage + propostas + surfacing + decisão+RL); +2-3 semanas para Phase F-H (mais detectors + autonomy controller + cross-validation harness). Total ~6-9 semanas calendário com pausas para validação humana.

---

## 1. Visão e Princípios

### 1.1 Princípios fundadores (ordenados por prioridade)

1. **HUMAN-IN-THE-LOOP por padrão** — autopilot detecta+propõe; humano decide. Nenhuma escrita autônoma sem opt-in declarativo prévio (`touring autopilot enable --category quality.regression --level L3`).
2. **EVIDÊNCIA antes de PROPOSTA** — toda proposta carrega: (a) finding com confidence ≥ 0.8, (b) speculative validation ≥ 0.8, (c) referências verificáveis (`touring index find` outputs, `touring ast blast` snapshots), (d) blast radius mensurado, (e) custo de reversão estimado.
3. **FALSO POSITIVO É BUG** — toda proposta rejeitada injeta reward negativo + atualiza heurísticas. Target: **false positive rate < 5%** medido em janela rolling de 30 dias.
4. **GRADIENTE DE AUTONOMIA, NÃO INTERRUPTOR** — 6 níveis (L0-L5) por categoria; default L0 (passive observe). Promoção entre níveis é decisão Gabriel-only.
5. **DEDUPLICATION é INVARIANTE** — mesmo finding suprimido por 24h após qualquer decisão; mesma categoria suprime por N dias se snoozed; rate-limit absoluto N propostas/sessão.
6. **REUTILIZA, NÃO REINVENTA** — autopilot é uma camada DELGADA sobre os 22 crates touring. Toda detecção delega a primitivas existentes (`touring-analysis`, `touring-ast`, `touring-cognitive`, etc.). Código novo: < 3.000 LOC esperado.
7. **OBSERVABILIDADE FIRST** — todo evento (detection, triage, proposal, decision, learning) emite contador em `gate_metrics` + entry em SQLite ledger. Sem observabilidade = sem deploy.
8. **HARD RULE #11 INVIOLÁVEL** — autopilot NUNCA invoca git. Histórico via `touring memory recall` + `touring evolution insights` (M1+M2 reescopados); diff via `touring ast blast` + `WiringFinding`.
9. **REVERSIBILIDADE TOTAL** — qualquer ação aplicada em L4+ deve ter caminho de reversão documentado e testado antes de ser oferecida.
10. **DECAY DE CONFIANÇA** — findings envelhecem. Após 7 dias sem ação, finding é re-validado ou descartado (estado pode ter mudado).

### 1.2 Não-objetivos explícitos

- **NÃO** é IDE-replacement. Autopilot complementa o ciclo Edit/Write do Claude Code.
- **NÃO** é CI replacement. CI roda no GitHub Actions; autopilot opera localmente como co-piloto.
- **NÃO** é monitoramento de produção. Foco é developer-loop, não observability runtime.
- **NÃO** é AI-pair-programmer (estilo Copilot). Autopilot opera em background; AI-pair é sincrono. São camadas diferentes.

> **Revisado 2026-04-25 (Gabriel directive)**: a posição original "NÃO é code-formatter automático" foi **revogada**. Formatting AGORA é uma categoria legítima do autopilot (D16, ver §5.1) **desde que** acompanhada de:
> 1. **Previsibilidade**: opt-in declarativo via `touring autopilot enable format.drift --level Lx` (default L0 — invisible).
> 2. **Diagnóstico de impacto**: novo `format_impact_score` segrega whitespace-only de structural diffs antes de propor; rejeita propostas com impact < 0.5 (single-line whitespace = noise).
> 3. **Dedup com refactor proposals**: se outro detector (D01, D12) já propõe action no mesmo file, D16 é absorvido como sub-step daquela proposta (evita duplicação).
>
> Justificativa: `touring ast format-rust` (Wave 4, prettyplease, deterministic) já existe e é seguro. Excluir formatting era preconceito; com diagnóstico de impacto, é a categoria mais auditável do autopilot. Spec completa em §5.1 D16.

### 1.3 Casos-de-uso canônicos (15 detection categories — §5)

Cobertura mental: o que o autopilot DEVE pegar que hoje passa despercebido:

| ID  | Pergunta operacional do desenvolvedor                                          |
|-----|---------------------------------------------------------------------------------|
| D01 | "Esse arquivo era A+ ontem; por que está C+ hoje?"                              |
| D02 | "Adicionei pub fn 3 dias atrás, ainda ninguém usa — esqueci de wirar?"          |
| D03 | "A wiring DB diz que isso é orphan, mas eu acabei de usar — cache stale?"       |
| D04 | "Esse handler tem 80 linhas novas e mutation_kill_rate caiu de 85 para 60%"     |
| D05 | "P99 de `touring ast blast` aumentou 3× em uma semana — que mudou?"             |
| D06 | "Editei 12 arquivos; 3 docstrings ficaram contraditórias — onde?"               |
| D07 | "Já corrigi o mesmo bug-pattern 4 vezes esse mês — vale criar gotcha rule?"     |
| D08 | "Esse arquivo regressed health 4 vezes seguidas — algo estrutural está errado"  |
| D09 | "O código que vou colar matches 3 known gotchas — ainda quero colar?"           |
| D10 | "rustsec advisory novo afeta 2 crates do workspace"                             |
| D11 | "Esse novo módulo tem blast radius 47 mas zero tests"                           |
| D12 | "Função que era CC=8 agora está CC=22 — refactor antes de continuar?"           |
| D13 | "Introduzi um cycle entre touring-ast e touring-analysis sem perceber"          |
| D14 | "API pública de `WiringFinding` mudou; 9 consumers vão quebrar"                 |
| D15 | "TODO em `mod.rs:42` tem 73 dias — vale fechar ou apagar?"                      |

Cada D01-D15 é detectável combinando primitivas touring que **já existem** (§3).

---

## 2. Posicionamento na arquitetura touring

```
┌────────────────────────────────────────────────────────────────────┐
│                          GABRIEL (decision)                        │
│                              ▲                                     │
│                              │ accept / reject / snooze            │
│                              │                                     │
│   ┌──────────────────────────┴──────────────────────────────────┐  │
│   │  TOURING-AUTOPILOT (proactive proposal layer — NEW)         │  │
│   │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │  │
│   │  │  Detectors   │→ │   Triage     │→ │  Proposer    │       │  │
│   │  │  (15 traits) │  │  (priority + │  │  (markdown + │       │  │
│   │  │              │  │   confidence)│  │   speculate) │       │  │
│   │  └──────────────┘  └──────────────┘  └──────────────┘       │  │
│   │       ↑                  ↑                  ↓               │  │
│   │       │                  │                  │               │  │
│   │       │  ┌───────────────┴──────────────┐   │               │  │
│   │       │  │  Decision Recorder + RL Loop │   │               │  │
│   │       │  │  (SQLite ledger, learning    │   │               │  │
│   │       │  │   reward injection)          │   │               │  │
│   │       │  └──────────────────────────────┘   │               │  │
│   └───────┼──────────────────────────────────────┼───────────────┘  │
│           │                                      │                  │
│   ┌───────┴──────────────────────────────────────┴──────────────┐   │
│   │       TOURING ECOSYSTEM (already operational)                │  │
│   │  ┌────────────┐  ┌────────────┐  ┌────────────┐             │  │
│   │  │touring-ast │  │  touring-  │  │touring-    │             │  │
│   │  │  (syn,     │  │  analysis  │  │ cognitive  │             │  │
│   │  │ tree-sitter│  │ (quality,  │  │ (MCTS,     │             │  │
│   │  │  TDG,scan) │  │  blast,    │  │  graph-    │             │  │
│   │  │            │  │  e2e,wirin │  │  informed) │             │  │
│   │  └────────────┘  └────────────┘  └────────────┘             │  │
│   │  ┌────────────┐  ┌────────────┐  ┌────────────┐             │  │
│   │  │  touring-  │  │  touring-  │  │  touring-  │             │  │
│   │  │   index    │  │  learning  │  │  generator │             │  │
│   │  │ (45k syms) │  │ (LinUCB,   │  │  (30 kinds,│             │  │
│   │  │  tantivy   │  │  evolution │  │   VGP)     │             │  │
│   │  │   BM25)    │  │  insights) │  │            │             │  │
│   │  └────────────┘  └────────────┘  └────────────┘             │  │
│   │  ┌────────────┐  ┌────────────┐  ┌────────────┐             │  │
│   │  │  touring-  │  │  touring-  │  │  touring-  │             │  │
│   │  │  hooks     │  │  rkyv      │  │  mutation- │             │  │
│   │  │ (153 hook  │  │  (zero-cpy │  │  test (T1) │             │  │
│   │  │  registry) │  │   IPC)     │  │            │             │  │
│   │  └────────────┘  └────────────┘  └────────────┘             │  │
│   └─────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────┘
```

**Interpretação**: autopilot é uma camada FINA acima do ecossistema. Ele **não duplica** detecção; **delega** a primitivas existentes e adiciona: (1) coordenação cross-source, (2) confidence scoring, (3) speculation pre-proposal, (4) surfacing canalizado, (5) decision tracking, (6) RL feedback.

---

## 3. Mapeamento de capabilities touring (catalog completo)

Esta seção inventaria QUE primitivas existem e como o autopilot consome cada uma. Confidence: 1.0 — todas verificadas em código (`/home/gabrielgadea/.claude/rust/crates/`).

### 3.1 touring-ast (modulo + grep + semantic)

| Capacidade                          | Função usada                                                | Output canônico                                              | Detector(s) consumidor(es) |
|-------------------------------------|-------------------------------------------------------------|--------------------------------------------------------------|----------------------------|
| AST extraction multi-lang           | `touring ast meta <file> --depth full -j`                    | symbols, blast, fan-in/out, cognitive_score, quality_score   | D01, D11, D12              |
| Blast radius                        | `touring ast blast <file>` + `cli_ast_blast`                 | árvore deps + impacted modules                               | D11, D14                   |
| Cross-feature blast                 | `touring ast blast-cross-feature`                            | gated symbols por feature                                    | D14                        |
| Call graph                          | `touring-ast::call_graph::build_call_graph`                  | edge list source→sink                                        | D11, D13                   |
| TDG grade letter (Q1)               | `touring ast tdg <file>`                                     | A+..F + 6 dimensions                                         | D01                        |
| ast-grep batch scan (Q2)            | `touring ast scan --rules <dir>`                             | structural matches per rule                                  | D09                        |
| API surface diff (Wave 5)           | `touring-ast::rust_semantic::diff_api_surfaces`              | ApiChange[] {Added, Removed, Modified}                       | D14                        |
| API cascade plan (C2)               | `touring-ast::api_cascade::plan_api_cascade`                 | CascadePlan<SubtaskProposal>                                 | D14                        |
| Workspace info (cargo metadata)     | `touring ast workspace-info`                                 | packages, features, dependents_of                            | D14, D10                   |
| Rust semantic (syn 2.0)             | `touring ast rust-semantic <file.rs>`                        | generics, trait bounds, semantic_complexity                  | D12                        |
| File heat (recency-weighted)        | `touring-ast::file_heat::FileHeat`                           | edit recency × access weight                                 | D08, D15                   |
| Code-gen workflow hint              | `touring-ast::code_gen_workflow`                             | RL-ready hint para generator                                 | D04                        |
| Learning loop signals               | `touring-ast::learning_loop`                                 | quality drift signals                                        | D01, D08                   |
| Format Rust (in-memory)             | `touring-ast::surgery::format_rust_code(&str)` (prettyplease) | Shadow-format without disk touch                            | **D16**                    |
| Format Rust best-effort             | `touring-ast::surgery::format_rust_code_best_effort`          | Tolerate partial-syntax files                               | **D16**                    |

**Cobertura**: 15 capabilities (13 originais + 2 format-rust); alimenta 9 dos 16 detectors.

### 3.2 touring-analysis (quality + wiring + e2e)

| Capacidade                          | Função usada                                                 | Output canônico                                              | Detector consumidor |
|-------------------------------------|--------------------------------------------------------------|--------------------------------------------------------------|---------------------|
| Quality complexity (Halstead+MI+CC) | `touring-analysis::quality::complexity`                      | n1,n2,N1,N2,V,D,E,B,T,MI,CC                                  | D12                 |
| Quality antipatterns                | `touring-analysis::quality::antipatterns`                    | unwrap/panic/.expect/clone-loop counts                       | D04, D09            |
| Quality RustQualitySignals (W7-8)   | `touring-analysis::quality::RustQualitySignals::health_score`| f32 ∈ [0,1.2]                                                | D01, D08            |
| Wiring findings (Q4)                | `touring-analysis::wiring::WiringFinding` (W-100..W-120)     | structured finding com diagnostic code                       | D02, D03, D13       |
| Blast warnings (Q4)                 | `touring-analysis::blast_radius::BlastWarning` (B-300..B-320)| structured warning                                           | D11                 |
| Health score (existing)             | `touring-analysis::health::*`                                | composite 0-1                                                | D01, D08            |
| Error-rate aggregator               | `touring-analysis::error_rate`                               | failure rate per tool/file                                   | D07                 |
| Knowledge stats                     | `touring-analysis::knowledge`                                | DB-derived stats                                             | (cross-cutting)     |
| Learning convergence                | `touring-analysis::learning`                                 | RL EMA + drift                                               | (gating)            |
| E2E health composite                | `touring e2e -j`                                             | composite 0-1 + per-phase scores                             | (gating)            |
| Pipeline orchestration              | `touring-analysis::pipeline`                                 | reusable composition                                         | (impl reuse)        |
| Security stub                       | `touring-analysis::security`                                 | unwrap_audit + antipattern scoring                           | D09                 |
| Temporal series                     | `touring-analysis::temporal`                                 | time-windowed deltas                                         | D01, D05            |

**Cobertura**: 13 capabilities; alimenta 9 dos 15 detectors.

### 3.3 touring-cognitive (MCTS + GoT + reasoning)

| Capacidade                       | Função usada                                | Output canônico                            | Uso no autopilot         |
|----------------------------------|---------------------------------------------|--------------------------------------------|--------------------------|
| MCTS shadow rollout              | `touring-cognitive::mcts::PheromoneMCTS`    | search tree + score                        | Speculator (Phase C)     |
| Graph-informed MCTS              | `CognitiveMCTS = GraphInformedMCTS`         | Tarjan SCC + UCT                           | Speculator + D13         |
| Adaptive engine                  | `adaptive_engine::AdaptiveEngine`           | depth/breadth bandit                       | Triage budget control    |
| Reasoning engine                 | `reasoning_engine`                          | structured reasoning trace                 | Proposal drafting        |
| Hybrid engine                    | `hybrid_engine`                             | symbolic + neural fusion                   | Confidence scoring       |
| Got (Graph of Thought)           | `got`                                       | DAG of thought nodes                       | Proposal alternatives    |

**Cobertura**: 6 capabilities; usadas em Speculator, Triage, Proposer.

### 3.4 touring-index + tantivy (47k symbols)

| Capacidade                  | Função                                  | Uso no autopilot           |
|-----------------------------|-----------------------------------------|----------------------------|
| Exact symbol lookup (VGP)   | `touring index find <symbol>`           | False-positive elimination |
| Prefix search               | `touring index search <prefix>`         | Discovery                  |
| Index status                | `touring index status`                  | Gating (estale → abort)    |
| BM25 ranked search          | `touring tantivy search`                | Cross-source corroboration |
| Fuzzy levenshtein           | `touring tantivy fuzzy`                 | Symbol misspelling robust  |
| Prefix autocomplete         | `touring tantivy suggest`               | Proposal helpers           |

**Cobertura**: 6 capabilities; usadas para verification + corroboration.

### 3.5 touring-learning (LinUCB + evolution)

| Capacidade                   | Função                                          | Uso no autopilot                              |
|------------------------------|-------------------------------------------------|-----------------------------------------------|
| LinUCB bandit (8 arms)       | `touring-learning::bandit::linucb`              | Detection arm selection (which detector run)  |
| Granularity bandit (C1)      | `GranularityBandit` (4 arms)                    | Triage budget allocation                      |
| Evolution drift detection    | `touring evolution drift -j`                    | Gating (degraded → escalate proposal urgency) |
| Evolution insights           | `touring evolution insights -j`                 | D07 (repeated patterns)                       |
| Evolution tools              | `touring evolution tools -j`                    | D07 (tool effectiveness)                      |
| Reward injection             | `touring learning reward <tool> <val> [ctx]`    | Decision feedback loop                        |

**Cobertura**: 6 capabilities; alimentam Detection arm selection + Decision RL.

### 3.6 touring-generator (30 kinds, VGP, typestate) — DETAILED

Esta é a integração mais profunda do autopilot. Mapeamento baseado em código verificado em `crates/touring-generator/src/`.

**Estrutura interna** (verificada em `executor/typestate.rs`, `vgp/`, `speculate/`, `generator/`):

```
touring-generator/
├── core/                       # shared types
├── plan/                       # plan contracts + schema + result + failure
├── generator/
│   ├── kinds.rs                # GeneratorKind enum (30 variants)
│   └── trait_def.rs            # Generator trait
├── executor/
│   ├── typestate.rs            # PlanStage trait + Draft/Verified/Rendered/Speculated/Committed
│   └── replan.rs               # ReplanContext (re-formulate after low spec score)
├── vgp/
│   ├── engine.rs               # VgpEngine + SymbolKey + VgpLookupResult + VgpCacheStats
│   └── fuzzy.rs                # FuzzySearcher trait + NoopFuzzySearcher
├── speculate/
│   └── bridge.rs               # SpeculateBridge (shadow validation entry point)
├── template/                   # Tera template loader
├── registry/                   # plan registry / persistence
├── validate/                   # schema validation
└── error.rs                    # GenerateError + Diagnostic codes G-400..G-420
```

**30 GeneratorKind variants** (CLI-verified `touring generate list-kinds`):

| Group              | Kinds                                                                                  |
|--------------------|----------------------------------------------------------------------------------------|
| Code (Rust)        | Rust Module, CLI Handler, MCP Tool, Hook Handler, Test, Benchmark Suite, Fuzz Target  |
| Macros             | Derive Macro, Attribute Macro, Function Macro                                          |
| Schemas/Specs      | Error Catalog, JSON Schema, ProtoBuf Schema, OpenAPI Spec, AsyncAPI Spec               |
| Patches            | Incremental Patch                                                                      |
| Interop            | FFI Binding                                                                            |
| Migrations         | Migration Script                                                                       |
| Docs               | Plan (Markdown), Skill Document, Diary Entry, Changelog Entry, ADR, Man Page           |
| DevOps             | Shell Completion, Dockerfile, Kubernetes Manifest, Terraform Module, CI Workflow       |
| Other languages    | Python Script                                                                          |

**Typestate pipeline** (compile-time enforcement of order):

```rust
PlanExecutor<Draft>      // user-supplied intent
    ↓ verify()
PlanExecutor<Verified>   // VgpEngine confirmed all symbols exist (no hallucinations)
    ↓ render()
PlanExecutor<Rendered>   // Tera template applied; artifacts produced as RenderedFile[]
    ↓ speculate()
PlanExecutor<Speculated> // SpeculateBridge ran shadow validation; score ∈ [0,1]
    ↓ commit()           // ONLY if score ≥ threshold AND user opts in
PlanExecutor<Committed>  // CommitReport with applied artifacts
```

**The autopilot uses stages 1-4 ONLY** for L0-L3 (no commits). For L4+ (gated, future), the commit step is invoked through TACO Phase 5.

| Generator capability                | Type / API surface                                          | Autopilot use case                                       |
|-------------------------------------|-------------------------------------------------------------|----------------------------------------------------------|
| `GeneratorKind` (30 variants)       | enum in `generator/kinds.rs`                                | Proposer selects kind based on detector category         |
| `VgpEngine` + `SymbolKey`           | `vgp/engine.rs`                                             | Verify all symbols in proposed plan exist (anti-halluc)  |
| `VgpLookupResult`                   | `{exists: bool, file_path?, line?, signature?}`             | Evidence in proposal markdown (real symbol coordinates)  |
| `VgpCacheStats`                     | hit/miss for VGP cache                                      | Telemetry → gate_metrics                                 |
| `FuzzySearcher` trait               | `vgp/fuzzy.rs`                                              | Plug touring-tantivy fuzzy as fallback when exact misses |
| `PlanExecutor<Draft>`               | `executor/typestate.rs`                                     | Speculator entry point                                   |
| `PlanExecutor<Verified>::render()`  | typestate transition                                        | Materialize artifacts in shadow workspace                |
| `PlanExecutor<Rendered>::speculate()`| typestate transition                                       | Score the proposed change without applying              |
| `SpeculateBridge`                   | `speculate/bridge.rs`                                       | Shadow validation: cargo check + tests in tmp dir        |
| `ReplanContext`                     | `executor/replan.rs`                                        | If spec score < 0.8 → replan with adjusted intent        |
| `RenderedFile` / `FileAction`       | `lib.rs` re-exports                                         | Diff preview in proposal markdown                        |
| `CommitReport`                      | `lib.rs` re-exports                                         | Reserved for L4+; autopilot doesn't invoke commit        |
| `GenerateError::to_diagnostic_opt()`| `error.rs` (RFC-100 G-400..G-420)                           | Structured errors → autopilot ledger evidence            |
| `diagnostic_speculate_passed()`     | `error.rs` (G-410 success path)                             | Positive signal in proposal "speculation passed"         |
| `HealthDeltaRecordFn / ComputeFn`   | closure types in `GeneratorContext`                         | Speculator records pre-state, computes post-state delta  |
| Plan markdown kind                  | `Plan (Markdown)` GeneratorKind via `plan.md.tera`          | Proposer reuses this kind to draft proposals (eat own dog food) |
| ADR kind                            | `Architecture Decision Record` via `adr.tera`               | Long-lived autopilot decisions emitted as ADRs in `docs/autopilot/decisions/` |
| `Incremental Patch` kind            | via `incremental_patch.tera`                                | Auto-fix candidates for D02, D11, D12, D14               |
| `Skill Document` kind               | via `skill_document.tera`                                   | D06 (docs.drift) auto-update suggestions                 |
| `Changelog Entry` kind              | via `changelog_entry.tera`                                  | Decision Recorder → CHANGELOG.md drafts                  |

**Cobertura ampliada**: 20 capabilities mapeadas (era 8 antes da exploração); Speculator + Proposer + ReplanContext usam heavily; FuzzySearcher trait permite plug touring-tantivy como fallback (anti-falso-negativo de VGP).

**Integration code shape (Phase C — Speculator)**:

```rust
// crates/touring-hooks/src/autopilot/speculator.rs (sketch — NOT YET IMPLEMENTED)
use touring_generator::{
    PlanExecutor, Draft, Verified, Rendered, Speculated,
    GeneratorKind, VgpEngine, SpeculateBridge,
};

pub struct Speculator<'a> {
    rt: &'a HookRuntime,
    vgp: &'a VgpEngine,
    bridge: &'a SpeculateBridge,
}

impl<'a> Speculator<'a> {
    pub fn try_speculate(&self, finding: &Finding) -> Result<SpeculatedFinding> {
        // 1. Synthesize intent from finding category + evidence
        let intent = self.intent_from_finding(finding);

        // 2. Pick the right GeneratorKind for this category
        let kind = match finding.category.as_str() {
            "wiring.orphan"        => GeneratorKind::IncrementalPatch,
            "quality.regression"   => GeneratorKind::IncrementalPatch,
            "complexity.spike"     => GeneratorKind::IncrementalPatch,
            "docs.drift"           => GeneratorKind::SkillDocument,
            "testing.coverage_gap" => GeneratorKind::Test,
            _ => return Ok(finding.clone().into_unspeculated()),  // not speculatable
        };

        // 3. Drive the typestate pipeline through Speculated (NO commit)
        let executor = PlanExecutor::<Draft>::new(intent, kind);
        let verified = executor.verify(self.vgp)?;       // VGP — no hallucinations
        let rendered = verified.render()?;                // Tera template
        let speculated = rendered.speculate(self.bridge)?;// shadow cargo check + tests

        let score = speculated.score();
        if score < 0.6 {
            return Ok(finding.clone().drop_with_reason("low_speculation"));
        }

        Ok(SpeculatedFinding {
            inner: finding.clone(),
            speculation_score: score,
            rendered_artifacts: speculated.artifacts().to_vec(),
            generator_kind: kind,
        })
    }
}
```

**Why this matters for precision** (the user's key requirement): the generator's typestate forces VGP **before** rendering, and shadow validation **before** anything is shown to Gabriel. A proposal that reaches `Speculated` state has:

1. ✅ Every referenced symbol verified to exist (`VgpEngine`)
2. ✅ Template rendered without errors (`render()` returned Ok)
3. ✅ Shadow `cargo check` passed in temp workspace (`SpeculateBridge`)
4. ✅ Score ≥ threshold (default 0.6 for surfacing, 0.8 for L3+ DRAFT)

This is **structurally stronger** than rust-analyzer assists (which don't run cargo check in shadow) and stronger than Copilot suggestions (which don't VGP-verify symbols).

### 3.7 touring-hooks (153 registry, suggesters, bidirectional)

| Capacidade                              | Função                                                      | Uso no autopilot                           |
|-----------------------------------------|-------------------------------------------------------------|--------------------------------------------|
| `bidirectional::suggester::Suggester`   | trait + run_suggester com dedup                             | **REUSE FRAMEWORK** (Phase A foundation)   |
| `cc_action_suggestions` SQLite table    | provenance + consumed flag + dedup                          | Decision Recorder schema reuse             |
| `task_digest::digest_pending_tasks`     | additionalContext injection                                 | Surfacing channel (Phase D)                |
| 3 existing suggesters (stuck/fail/plan) | reference impls                                             | Pattern blueprint                          |
| `health_delta` singleton                | per-path streak + alerts                                    | D08                                        |
| `gate_metrics` AtomicU64 counters       | observability primitive                                     | All telemetry                              |
| `gotcha_loader` (Q3)                    | YAML rule library                                           | D09                                        |
| `mutation_test` (T1)                    | cargo-mutants wrapper + cache                               | D04                                        |
| `tfidf_retriever` (M1)                  | corpus over memory + decompose                              | D07                                        |
| `memory_recall_rrf_merge_n` (M2)        | RRF over N sources                                          | Evidence corroboration                     |

**Cobertura**: 10 capabilities, INCLUINDO o `suggester` framework que **vamos estender** (não substituir).

### 3.8 Resumo agregado

- **22 crates** disponíveis
- **62 capabilities** mapeadas (primitivas reutilizáveis para autopilot)
- **3 suggesters existentes** (blueprint para os 12 novos detectors)
- **Zero crates novos necessários** — autopilot é um módulo dentro de `touring-hooks/src/autopilot/`

---

## 4. Arquitetura de referência (LSP-inspired)

### 4.1 Inspiração: LSP `publishDiagnostics` + `CodeAction` + `codeAction/resolve`

Confirmado via Context7 (`/microsoft/language-server-protocol`):

- **Server PUSHES diagnostics** ao client (`publishDiagnostics` notification). Não é polling. Touring autopilot adota: scanner emite findings → push via additionalContext + PushNotification.
- **CodeAction** separa metadata (title, kind, isPreferred, disabled+reason) de implementação (edit OR command). Lazy resolution via `codeAction/resolve` — detalhes computados só quando user vai aplicar. Touring autopilot adota: Finding → cheap; Proposal materialization → on-demand.
- **CodeActionKind taxonomy**: `quickfix`, `refactor.extract`, `refactor.inline`, `refactor.rewrite`, `source.organizeImports`, `source.fixAll`. Touring estende com: `wiring.connect`, `quality.refactor`, `test.add`, `gotcha.guard`, `cycle.break`, `api.migrate`.
- **Diagnostic.severity**: 1=Error, 2=Warning, 3=Information, 4=Hint. Mapeamos: Critical, High, Medium, Low.
- **Diagnostic.tags**: 1=Unnecessary, 2=Deprecated. Adotamos: Unused, Deprecated, Stale, FlakyTest.
- **rust-analyzer `assists_with_fixes`** (via `/rust-lang/rust-analyzer`): retorna `Vec<Assist>` com `label + id + kind` cheap; resolve completo via `AssistResolveStrategy`. Touring espelha: detector retorna `Vec<Finding>` cheap; speculation completo on-demand.

### 4.2 Componentes (10)

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                     │
│   ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐   │
│ 1 │  Detector  │  │  Detector  │  │  Detector  │  │  Detector  │ ..│
│   │   (D01)    │  │   (D02)    │  │   (D03)    │  │   (D…15)   │   │
│   └─────┬──────┘  └──────┬─────┘  └──────┬─────┘  └──────┬─────┘   │
│         │                │                │                │        │
│         └────────────────┴────────────────┴────────────────┘        │
│                              │                                      │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 2 │              Finding Aggregator (dedup, merge)             │    │
│   │   - hash(category, location, pattern) → suppress 24h       │    │
│   │   - merge findings on same symbol from different sources   │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │ Vec<Finding>                         │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 3 │      Confidence Engine (multi-source corroboration)        │    │
│   │   confidence = w1·source_count + w2·VP-Scout + w3·index    │    │
│   │      + w4·temporal_stability + w5·RL_prior                 │    │
│   │   discard if confidence < 0.5; backlog if 0.5..0.8         │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │ Vec<ScoredFinding>                   │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 4 │      Triage Engine (priority + budget allocation)          │    │
│   │   priority = severity × (1 - blast_normalized) × confidence│    │
│   │   - P0: act immediately (notify Gabriel even quiet hours)  │    │
│   │   - P1: queue for next session digest                      │    │
│   │   - P2/P3: accumulate weekly                               │    │
│   │   GranularityBandit selects budget per category            │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │ Vec<TriagedFinding>                  │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 5 │      Speculator (touring-generator pre-validation)         │    │
│   │   for each finding with auto-fix candidate:                │    │
│   │     plan = generate plan-suggest --intent <finding>        │    │
│   │     score = generate plan-speculate --file <plan>          │    │
│   │     if score < 0.8: drop OR demote to manual-review        │    │
│   │   MCTS shadow rollout for high-impact (>10 blast)          │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │ Vec<SpeculatedFinding>               │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 6 │      Proposer (markdown drafter + evidence bundler)        │    │
│   │   writes <ws>/.touring-cache/autopilot/proposals/<id>.md   │    │
│   │   includes: title, why, evidence[], reversibility,         │    │
│   │     suggested-action (CLI command OR generator plan ref),  │    │
│   │     blast preview, test coverage, related findings         │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │ Vec<Proposal>                        │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 7 │      Surface Adapter (channeled push)                      │    │
│   │   channel = autonomy_level + priority + quiet_hours        │    │
│   │   - additionalContext (default, low-friction)              │    │
│   │   - session digest entry (instructions-loaded hook)        │    │
│   │   - PushNotification (only if level ≥ L2 + P0/P1)          │    │
│   │   - CLI: touring autopilot list / show <id>                │    │
│   │   - MCP: touring_autopilot_list / show / decide            │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │ Surface event                        │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 8 │      Decision Recorder (SQLite ledger)                     │    │
│   │   table: autopilot_proposals                               │    │
│   │     columns: id, finding_hash, category, severity,         │    │
│   │       confidence, surfaced_at, decided_at, decision,       │    │
│   │       decision_actor, applied_at, applied_by, reverted_at  │    │
│   │   touring autopilot accept|reject|snooze|apply <id>        │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │ DecisionEvent                        │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│ 9 │      RL Feedback Loop                                      │    │
│   │   on accept: reward(+1.0, ctx="autopilot:<category>")      │    │
│   │   on reject: reward(-1.0) + suppress(24h, finding_hash)    │    │
│   │   on snooze: reward(-0.3) + suppress(N days, category)     │    │
│   │   updates LinUCB arm selection for future detection runs   │    │
│   │   updates GranularityBandit budget per category            │    │
│   └─────────────────────────┬──────────────────────────────────┘    │
│                              │                                      │
│                              ▼                                      │
│   ┌────────────────────────────────────────────────────────────┐    │
│10 │      Autonomy Controller                                   │    │
│   │   per-category state machine: L0 → L5                      │    │
│   │   promotes only on Gabriel's explicit opt-in               │    │
│   │   demotes automatically on FP rate spike                   │    │
│   │   kill switch: touring autopilot disable                   │    │
│   └────────────────────────────────────────────────────────────┘    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.3 Data flow timing

| Stage              | Latency budget | Frequency                                      |
|--------------------|----------------|------------------------------------------------|
| Detector scan      | 50-500ms each  | Triggered: post-edit, post-write, hourly cron  |
| Aggregator+dedup   | < 10ms         | Inline                                         |
| Confidence         | < 50ms         | Inline                                         |
| Triage             | < 50ms         | Inline                                         |
| Speculator         | 1-30s per item | Background (`touring jobs spawn`)              |
| Proposer           | < 100ms        | Inline post-speculation                        |
| Surface            | < 10ms         | Hook-driven (instructions-loaded, post-edit)   |
| Decision recorder  | < 10ms         | On user action                                 |
| RL feedback        | < 50ms         | Async after decision                           |

**Total cold path**: 60s budget (one full scan over a typical workspace).
**Hot path** (incremental, single-file post-edit): < 200ms enriched signal in additionalContext.

---

## 5. Catálogo de detectors (15 categorias)

Cada detector implementa o trait:

```rust
pub trait AutopilotDetector: Send + Sync {
    /// Stable identifier (e.g. "quality.regression" — used as RL context key).
    fn category(&self) -> &'static str;

    /// LSP-style severity. Override per-finding via finding.severity.
    fn default_severity(&self) -> Severity;

    /// Cheap scan — must NOT block more than self.budget_ms().
    fn scan(&self, rt: &HookRuntime, scope: ScanScope) -> Vec<Finding>;

    /// Per-detector budget cap. Triage may further restrict.
    fn budget_ms(&self) -> u32 { 500 }

    /// Whether speculator should attempt auto-fix candidate generation.
    fn speculatable(&self) -> bool { false }

    /// Required minimum touring infra for this detector to run.
    /// Returns None if requirements met; Some(reason) if it must be skipped.
    fn precheck(&self, rt: &HookRuntime) -> Option<&'static str>;
}
```

`ScanScope` covers: `Workspace` (full), `Crate(name)`, `File(path)`, `SymbolDelta(symbol_name)`. Hot-path detectors run with `File`/`SymbolDelta`; cron-triggered run with `Workspace`/`Crate`.

### 5.1 Lista completa

| ID  | Category                  | Severity | Speculatable | Touring deps                                      | Trigger                          |
|-----|---------------------------|----------|--------------|---------------------------------------------------|----------------------------------|
| D01 | quality.regression        | Warning  | Yes (refac)  | tdg, RustQualitySignals, temporal, health_delta   | post-edit, hourly                |
| D02 | wiring.orphan             | Hint     | Yes (suggest)| wiring orphans, index find, generate plan-suggest | post-write (>24h since insert)   |
| D03 | wiring.cache_staleness    | Info     | No           | wiring orphans + grep cross-check                 | wiring DB write                  |
| D04 | testing.mutation_gap      | Warning  | No           | mutation-test cache, blast radius                 | post-merge, mutation-baseline CI |
| D05 | performance.regression    | Warning  | No           | gate_metrics histograms (P99), temporal           | hourly                           |
| D06 | docs.drift                | Hint     | Yes (gen doc)| call_graph, file_heat, ast skeleton vs docstring  | post-edit                        |
| D07 | pattern.repeated_failure  | Info     | Yes (gotcha) | error_rate, evolution insights, tfidf retriever   | weekly                           |
| D08 | health.regression_streak  | Warning  | No           | health_delta streak counters                      | post-edit (streak≥3)             |
| D09 | gotcha.match              | Warning  | No           | gotcha_loader (Q3), ast scan rules                | pre-edit, pre-write              |
| D10 | supply_chain.advisory     | Critical | No           | cargo deny advisories (workspace-info)            | daily                            |
| D11 | testing.coverage_gap      | Warning  | Yes (test)   | blast radius, llvm-cov summary, mutation cache    | post-write                       |
| D12 | complexity.spike          | Warning  | Yes (refac)  | complexity (CC, Halstead, MI), rust-semantic      | post-edit                        |
| D13 | wiring.cycle_introduced   | Critical | No           | wiring cycles (Tarjan SCC F2), call_graph         | post-write                       |
| D14 | api.breaking_change       | Critical | Yes (cascade)| diff_api_surfaces, api_cascade, dependents_of     | pub symbol modification          |
| D15 | todo.stagnant             | Hint     | No           | ast todos, file_heat, temporal age                | weekly                           |
| D16 | format.drift              | Hint     | Yes (apply)  | format_rust_code (prettyplease), similar crate    | post-edit (debounced 5min)       |

### 5.2 Detector spec template (D01 como referência completa)

```
DETECTOR: quality.regression
PURPOSE: detect TDG/health drop on a file vs its 7d baseline.

ALGORITHM:
  1. for each file modified in last 24h (file_heat top-K):
  2.   curr_tdg = touring ast tdg <file>  (Q1)
  3.   curr_health = RustQualitySignals::health_score (Rust) OR multi-lang health
  4.   baseline = touring memory recall "tdg:<file>:baseline"
  5.   if baseline.exists and curr_tdg.composite < baseline.composite - 0.05:
  6.     finding = Finding {
            category: "quality.regression",
            severity: if curr_tdg.composite < 0.6 then Critical
                      else if drop > 0.15 then Warning
                      else Info,
            location: file,
            evidence: { baseline_tdg, curr_tdg, drop, health_score },
            confidence_hint: 0.9,
            speculatable: true,
            suggested_action: GenerateKind::Refactor,
          }
  7.   touring memory store "tdg:<file>:rolling" curr_tdg

CONFIDENCE_HINT: 0.9 (TDG is mathematical; little ambiguity)
FALSE_POSITIVE_GUARDS:
  - drop must persist across 2+ consecutive scans (transient noise filter)
  - file must have ≥ 50 LOC (stub files volatile)
  - if grade was already F → no proposal (already at floor)
ANTI-LOOP:
  - same file dedup 24h after any decision
  - if Gabriel rejected D01 for this file 2× in 30 days → suppress 60 days

BUDGET_MS: 200 per file (parallel via rayon)
SCAN_SCOPE: File (post-edit) OR Workspace (hourly cron, top-50 hot files)
```

(Specs análogas de D02-D15 ficam em `docs/autopilot/detectors/D{NN}-*.md` — geradas durante Phase F.)

### 5.2.1 Detector spec — D16 format.drift (full algorithm)

```
DETECTOR: format.drift
PURPOSE: detect formatting drift in modified Rust files; offer auto-format
  with FULL impact diagnostic so Gabriel knows what would change before
  approving.

SAFETY POSTURE:
  - Default autonomy: L0 (invisible). Promote ONLY via explicit
    `touring autopilot enable format.drift --level Lx`.
  - Severity capped at Hint (formatting is never Critical).
  - NEVER applies in L0/L1; surfaces only with full impact preview.

ALGORITHM:
  1. Trigger: post-edit hook on *.rs file, debounced 5 minutes per file
     (avoid firing during active editing burst).
  2. Skip pre-conditions:
     a. file LOC < 50  → skip (small files volatile, low value)
     b. last_format_age < 1h (memory key "format:<file>:applied") → skip
        (Gabriel may be experimenting with format)
     c. another active proposal exists for same file → skip (dedup)
     d. file contains > 30% functions marked `#[rustfmt::skip]` → skip
        (intentional opt-out signal)
     e. per-crate D16 count this session ≥ 1 → skip (rate limit)
  3. Read source: original = fs::read_to_string(file)
  4. Compute formatted in-memory:
       formatted = touring_ast::format_rust_code(&original)?
     If best_effort_unchanged: skip (formatter no-op)
  5. Compute diff (similar::TextDiff::from_lines(&original, &formatted)):
       hunks = group_changes_by_proximity(changes, gap=3)
  6. Bucket each changed line:
       whitespace_only: trim_eq(orig_line, fmt_line) AND lines differ only
                        in indent/trailing-space
       structural:      everything else (token reordering, line splits,
                        attribute rearrangement, etc.)
  7. format_impact_score:
       impact = 10.0 * log10(structural_lines + 1)
              +  1.0 * log10(whitespace_only_lines + 1)
              +  0.5 * log10(hunks_count + 1)
     Skip if impact < 0.5 (single-line whitespace = noise)
  8. Build Finding {
       category: "format.drift",
       severity: Hint,
       location: file,
       evidence: {
         original_loc: ...,
         formatted_loc: ...,
         hunks_count: N,
         whitespace_only_lines: W,
         structural_lines: S,
         impact_score: X,
         diff_preview: first 50 hunks abbreviated,
         prettyplease_version: pinned,
       },
       confidence_hint: 1.0,            # deterministic
       speculatable: true,
       suggested_action: ApplyFormatted,
     }
  9. Anti-loop: hash = blake3(file + impact_score_bucket); 24h dedup.

DIAGNOSTIC OUTPUT (always shown in proposal):
  - Lines breakdown (structural vs whitespace)
  - Top 3 hunks with before/after preview (truncated to 5 lines each)
  - Estimated review time = ceil(structural_lines / 50) minutes
  - Reversibility: instant — re-apply original via memory snapshot
                    of pre-format content
  - Format provenance: prettyplease version + invocation source

DEDUP WITH OTHER DETECTORS:
  - When D01/D12 (refactor) already proposes action on same file:
    D16 is NOT surfaced as separate proposal; instead absorbed as
    "format step" in the refactor proposal markdown.
  - When D14 (api.breaking_change) propose API migration on same file:
    D16 is held until D14 decision (apply formatting AFTER migration).

AUTONOMY LEVEL EFFECTS:
  L0: detect + ledger only; no surfacing
  L1: appears in `touring autopilot list --include-low`
  L2: NOT surfaced via additionalContext (would create noise)
  L3: full markdown DRAFT with diff preview
  L4: STAGED — pre-applied to <ws>/.touring-cache/autopilot/staged/<id>.rs
      ready for `touring autopilot apply <id>` (instant)
  L5: NEVER (violates "previsto" requirement — Gabriel directive)

OBSERVABILITY COUNTERS:
  - autopilot_format_drift_detected_count
  - autopilot_format_drift_skipped_{loc,recent,dedup,skip_attr,rate_limit}_count
  - autopilot_format_drift_impact_score_histogram (hdrhistogram)
  - autopilot_format_drift_accepted_count
  - autopilot_format_drift_rejected_count
  - autopilot_format_drift_avg_structural_lines
  - autopilot_format_drift_avg_whitespace_lines

BUDGET_MS: 100 per file (format_rust_code is fast; diff is O(LOC))
SCAN_SCOPE: File (post-edit debounced) ONLY — never Workspace
            (would generate dozens of proposals, even rate-limited)

GABRIEL'S CONDITION COMPLIANCE:
  ✓ "Previsto"      — opt-in default L0, explicit enable required
  ✓ "Diagnosticado" — impact_score + structural/whitespace bucketing
                      mandatory in every proposal markdown
  ✓ Reversibility   — pre-state snapshot in memory; revert instant
```

### 5.2.2 Multi-language formatting (Phase F.7+ extension)

D16 ships Rust-only (prettyplease) in MVP. Multi-lang formatters are deferred:

| Language     | Formatter      | Touring integration status                         |
|--------------|----------------|----------------------------------------------------|
| Rust         | prettyplease   | ✅ via `touring ast format-rust` (D16 MVP)         |
| Python       | black, ruff format | future: wrap as `touring ast format-python`   |
| TypeScript   | prettier, biome    | future: wrap as `touring ast format-ts`       |
| JavaScript   | prettier           | future: wrap as `touring ast format-js`       |
| Go           | gofmt              | future: wrap as `touring ast format-go`       |
| C/C++        | clang-format       | future: wrap as `touring ast format-c`        |

Each language extension reuses the D16 algorithm — only the formatter dispatch changes. Gating: ship Rust D16 first, measure FP rate over 30d, then promote to multi-lang.

### 5.3 Critérios de elegibilidade para virar detector

Para um pattern entrar no catálogo, deve satisfazer ALL:

1. **Detectável determinísticamente** com primitivas touring existentes (sem invocar LLM em hot path).
2. **Confidence ≥ 0.7** atingível em ≥ 80% dos casos reais (medido em validação Phase H).
3. **Não-trivial**: pattern já passou despercebido ≥ 3 vezes em sessões reais (extraído de touring memory recall).
4. **Reversível**: ação proposta tem caminho de revert testado.
5. **Falso-positivo controlável**: existe mecanismo de suppress/snooze documentado.

Padrões que NÃO entram (ainda): code smells subjetivos, style preferences, "could-be-more-idiomatic". Esses ficam para Phase H+ se RL converge a sinal estável.

---

## 6. Confidence + Triage Engine

### 6.1 Confidence formula

```
confidence = clamp(
    w1 · source_count_normalized      // multi-source agreement
  + w2 · vp_scout_chain_pass_ratio    // see VP-Scout v1.1 (7 chains)
  + w3 · index_corroboration          // touring index find confirms
  + w4 · temporal_stability           // signal repeated across N scans
  + w5 · rl_prior(category)           // historical accept rate
  - p1 · suppress_history_penalty     // recent rejections of same hash
  - p2 · session_age_penalty          // older findings decay
, 0.0, 1.0
)

defaults: w1=0.20, w2=0.25, w3=0.15, w4=0.20, w5=0.20
penalties: p1=0.30, p2=0.10
```

### 6.2 Confidence tiers + actions

| Tier      | Range    | Action                                                              |
|-----------|----------|---------------------------------------------------------------------|
| **High**  | ≥ 0.85   | Proceed to triage; eligible for L2+ surfacing                       |
| **Medium**| 0.65-0.85| Backlog; show in `touring autopilot list --include-medium`          |
| **Low**   | 0.50-0.65| Internal only; shown in `touring autopilot list --include-low`      |
| **Drop**  | < 0.50   | Discarded; logged as `gate_metrics::autopilot_finding_dropped_count` |

### 6.3 Triage priority

```
priority = severity_weight                       // Critical=1.0, Warning=0.7, Info=0.4, Hint=0.2
         × (1 - min(blast_radius / 50, 1.0))     // huge blast = lower auto-priority (needs human)
         × confidence                             // already ∈ [0,1]
         × age_decay(finding.age_seconds)        // exp(-age / 86400)
```

| Priority | Range     | Routing                                                  |
|----------|-----------|----------------------------------------------------------|
| **P0**   | ≥ 0.8     | PushNotification ANY hour (subject to L2+ enabled)       |
| **P1**   | 0.5-0.8   | Next session digest entry (via `instructions-loaded`)    |
| **P2**   | 0.2-0.5   | Weekly summary roll-up                                   |
| **P3**   | < 0.2     | Backlog only; visible via `touring autopilot list --all` |

### 6.4 Budget allocation (GranularityBandit reuse)

Reusa `touring-learning::GranularityBandit` (4 arms). Arms map a per-cycle scan budget allocation across categories:

| Arm       | Description                          | Use case                              |
|-----------|--------------------------------------|---------------------------------------|
| `breadth` | Many detectors, shallow each         | Daily cron (touch everything cheaply) |
| `depth`   | Few detectors, exhaustive            | After major refactor signals          |
| `focused` | Single category, full workspace      | Post-incident (e.g. cycle introduced) |
| `idle`    | Skip cycle (system busy)             | High RL-detected daemon load          |

Bandit learns from accept-rate per arm.

---

## 7. Proposal lifecycle (10 estados)

```
       ┌─────────┐
       │ DRAFT   │ ← Speculator pending; not yet written to disk
       └────┬────┘
            │ proposer.materialize()
            ▼
       ┌─────────┐
       │ READY   │ ← Markdown file written; awaits surfacing
       └────┬────┘
            │ surface.push()
            ▼
       ┌─────────┐
       │SURFACED │ ← Visible to Gabriel via additionalContext / digest
       └────┬────┘
            │
   ┌────────┼─────────┬────────────┬─────────────┐
   │        │         │            │             │
   ▼        ▼         ▼            ▼             ▼
┌──────┐ ┌──────┐ ┌────────┐ ┌──────────┐ ┌──────────┐
│SEEN  │ │SNOOZED│ │REJECTED│ │ ACCEPTED │ │  EXPIRED │
└──┬───┘ └───┬──┘ └───┬────┘ └─────┬────┘ └────┬─────┘
   │        │        │             │           │
   │        │        │ rl(-1.0)    │ rl(+1.0)  │ rl(-0.5)
   │        │        │ +24h dedup  │           │
   │        │        ▼             ▼           ▼
   │        │     ARCHIVE       APPLIED → REVERTED?
   │        │                      │           │
   │        │                      │           ▼
   │        │                      │       rl(-1.5)
   │        │                      ▼       (worse than reject)
   │        │                  COMPLETED
   │        │
   │        └─→ wakes up after N days → re-validates → either re-surfaces or expires
   │
   └─→ no action 7 days → auto-expire
```

**Estados terminais**: `ARCHIVE`, `COMPLETED`, `REVERTED`. Ledger preserva histórico.

### 7.1 Proposal markdown schema (D01 example)

```markdown
# [autopilot:D01:quality.regression] crates/touring-hooks/src/foo.rs — TDG drop A→C+

**Proposal ID**: ap-20260425-a3f9c2
**Detected at**: 2026-04-25T15:42:18Z
**Confidence**: 0.91
**Priority**: P1
**Severity**: Warning
**Autonomy level required**: L1+ (suggest only) — NOT auto-applicable
**Reversibility cost**: low (refactor; revert via git OR via touring memory store of pre-state — Hard Rule #11 forbids git, prefer memory snapshot)

## Why

`touring ast tdg crates/touring-hooks/src/foo.rs` returned grade `C+` (composite 0.78);
7-day baseline (stored in memory key `tdg:foo.rs:baseline`) was grade `A` (0.92).

Drop = 0.14 over 4 edits. Health-delta streak shows 3 consecutive declines.

## Evidence

1. Current TDG (verified 2026-04-25T15:42:18Z):
   ```
   $ touring ast tdg crates/touring-hooks/src/foo.rs -j
   {"composite": 0.78, "complexity": 0.71, "antipatterns": 0.65, ...}
   ```
2. 7d baseline (stored 2026-04-18):
   ```
   {"composite": 0.92, "complexity": 0.88, "antipatterns": 0.95, ...}
   ```
3. Health-delta state:
   ```
   $ touring health-delta status crates/touring-hooks/src/foo.rs
   {"regression_streak": 3, "warning_hint": "⚠ regression streak: 3 consecutive declines on foo.rs — review"}
   ```
4. Likely cause (heuristic — confidence 0.7): function `process_batch` grew CC 8→17.

## Suggested action

```bash
# Option 1 (preferred, speculatable): generator-driven refactor
touring generate plan-suggest --intent "extract helper from process_batch in foo.rs to reduce CC"
touring generate plan-speculate --plan-file <suggested-plan>
# review plan, then plan-commit if ≥ 0.8 score

# Option 2 (manual): refactor process_batch into 2 functions
```

## Blast preview

`process_batch` has 4 direct consumers (touring ast blast snapshot attached).
Refactor must preserve signature.

## Decision

```bash
touring autopilot accept ap-20260425-a3f9c2     # Gabriel approves
touring autopilot reject ap-20260425-a3f9c2     # Gabriel disagrees (RL learns)
touring autopilot snooze ap-20260425-a3f9c2 7d  # postpone
touring autopilot show   ap-20260425-a3f9c2     # show this file again
```

---
_Generated by touring-autopilot v1.0_
```

---

## 8. Autonomia graduada (L0-L5)

### 8.1 Definição dos níveis

| Level | Name           | Behavior                                                                                                  | Default? |
|-------|----------------|-----------------------------------------------------------------------------------------------------------|----------|
| **L0**| PASSIVE        | Detect only; persist findings to ledger; no surfacing                                                      | YES      |
| **L1**| SUGGEST        | Surface in `touring autopilot list` (poll-based, pull). No additionalContext injection.                    | YES (after Phase D) |
| **L2**| PROPOSE        | Inject in `additionalContext` via `instructions-loaded` (digest entry, ≤ 3 per session)                    | NO (opt-in) |
| **L3**| DRAFT          | Materializes markdown plan + emits PushNotification for P0/P1                                              | NO (opt-in) |
| **L4**| STAGED         | Generates `touring generate plan-submit` snapshot + speculation report + branch-style proposal             | NO (opt-in, RESERVED — needs git or alternative) |
| **L5**| AUTO-APPLY     | Applies proposal directly using TACO Phase 5 (engineer agent); FULL revert path mandatory                  | NO (NEVER without per-category Gabriel approval) |

### 8.2 Per-category autonomy

Each detection category has its own (level, max_per_session, snooze_days_default). Stored in `~/.claude/rust/.touring-cache/autopilot/policy.toml`:

```toml
[default]
autonomy = "L0"
max_per_session = 0
snooze_days = 7

[category."quality.regression"]
autonomy = "L1"        # Gabriel can pull via CLI
max_per_session = 5

[category."wiring.cycle_introduced"]
autonomy = "L2"        # Critical → digest visible
max_per_session = 1
priority_floor = "P0"  # always surface immediately

[category."docs.drift"]
autonomy = "L0"        # Gabriel hasn't asked for these yet
```

Promotions/demotions logged to `autopilot_policy_changes` table.

### 8.3 Auto-demotion rules

The Autonomy Controller demotes a category by 1 level if ANY:

- **FP rate spike**: rejected/total > 0.4 in last 14 days
- **Snooze spike**: snoozed/total > 0.6 in last 14 days
- **Revert spike**: reverted/applied > 0.1 in last 30 days (only relevant L4+)
- **Manual demotion**: Gabriel runs `touring autopilot demote <category>`

Demotions emit PushNotification (Gabriel must know autopilot is becoming quieter).

### 8.4 Kill switches

| Switch                              | Effect                                              |
|-------------------------------------|-----------------------------------------------------|
| `touring autopilot disable`         | All detectors silenced; existing proposals frozen   |
| `touring autopilot disable <cat>`   | Per-category silence                                |
| `touring autopilot quiet-hours <hh:mm-hh:mm>` | No PushNotification in window             |
| `TOURING_AUTOPILOT=0` env var       | Hard-disable for current process                    |
| `~/.touring-autopilot-killswitch` file present | Hard-disable workspace-wide                |

Kill switches are honored at every component boundary; design defensively.

---

## 9. Anti-loop + safety guarantees

### 9.1 Five layers of anti-loop

1. **Hash dedup** — `hash(category, location_canonical, pattern_signature)` → suppress 24h after any decision.
2. **Category snooze** — Gabriel's snooze applies to the CATEGORY (not just the finding) for N days.
3. **Rate limit absoluto** — máximo `N` propostas surfaced per session (default N=5 across all categories).
4. **Quiet hours** — no PushNotification in user-defined window.
5. **Decay** — findings sem decisão expiram em 7 dias; re-validação obrigatória se re-emitidos.

### 9.2 Idempotency invariants

- Same finding hash applied to ledger 2× → second insert is no-op (UPSERT with seen_count++).
- `touring autopilot scan` invoked twice in same minute → second is short-circuited from cache (TTL 60s).
- Speculator output for same plan-file → memoized 1h.

### 9.3 Privacy + safety

- Autopilot **never** exfiltra data: tudo local, SQLite + filesystem.
- Não invoca rede em hot path (cargo deny advisories check em background daily).
- Não modifica config files (`Cargo.toml`, `.cargo/config.toml`, `settings.json`) sem prompt explícito.
- Não modifica `CLAUDE.md`, `tasks/lessons.md`, ou outros docs cuja autoridade é Gabriel-only.

### 9.4 Failure-mode safeguards

| Failure                            | Response                                                                  |
|------------------------------------|---------------------------------------------------------------------------|
| Detector panics                    | catch_unwind in scan loop; record failure; demote category 1 level        |
| Speculator times out (> 30s)       | drop finding; record `autopilot_speculator_timeout_count`                 |
| Decision Recorder DB locked        | retry 3× with 100ms backoff; on persistent failure → suspend autopilot    |
| RL injection fails                 | log warn; continue (decision is recorded; RL is best-effort)              |
| Autonomy file corrupt              | fall back to all-L0 policy; emit P0 notification                          |

---

## 10. Schema SQLite (autopilot tables)

Co-locado com `knowledge_db` (existing). Lazy CREATE on first write — same pattern as `cc_action_suggestions`.

```sql
CREATE TABLE IF NOT EXISTS autopilot_findings (
    finding_id      TEXT    PRIMARY KEY,           -- ulid
    finding_hash    TEXT    NOT NULL,              -- dedup key
    category        TEXT    NOT NULL,              -- e.g. "quality.regression"
    severity        TEXT    NOT NULL,              -- Critical|Warning|Info|Hint
    location_path   TEXT,                          -- file or symbol
    location_range  TEXT,                          -- "L23-L45" or null
    confidence      REAL    NOT NULL,              -- [0,1]
    priority        REAL    NOT NULL,              -- [0,1]
    evidence_json   TEXT    NOT NULL,
    speculatable    INTEGER NOT NULL DEFAULT 0,
    detected_at     TEXT    NOT NULL,
    expires_at      TEXT    NOT NULL               -- detected_at + 7d default
);
CREATE INDEX IF NOT EXISTS idx_autopilot_findings_hash ON autopilot_findings(finding_hash);
CREATE INDEX IF NOT EXISTS idx_autopilot_findings_cat  ON autopilot_findings(category, detected_at);

CREATE TABLE IF NOT EXISTS autopilot_proposals (
    proposal_id     TEXT    PRIMARY KEY,
    finding_id      TEXT    NOT NULL REFERENCES autopilot_findings(finding_id),
    state           TEXT    NOT NULL,              -- DRAFT|READY|SURFACED|SEEN|SNOOZED|REJECTED|ACCEPTED|EXPIRED|APPLIED|REVERTED|COMPLETED
    markdown_path   TEXT,                          -- on-disk plan path
    speculation_score REAL,                        -- [0,1] from generator
    surfaced_via    TEXT,                          -- additionalContext|digest|push|cli
    surfaced_at     TEXT,
    decided_at      TEXT,
    decided_by      TEXT,                          -- gabriel|auto-expire|auto-demote
    applied_at      TEXT,
    reverted_at     TEXT,
    metadata_json   TEXT
);

CREATE TABLE IF NOT EXISTS autopilot_suppressions (
    suppression_id  TEXT    PRIMARY KEY,
    scope_type      TEXT    NOT NULL,              -- finding_hash|category
    scope_key       TEXT    NOT NULL,
    reason          TEXT    NOT NULL,              -- rejection|snooze|policy
    suppressed_at   TEXT    NOT NULL,
    expires_at      TEXT    NOT NULL,
    created_by      TEXT    NOT NULL               -- gabriel|auto
);
CREATE INDEX IF NOT EXISTS idx_autopilot_suppressions_scope ON autopilot_suppressions(scope_type, scope_key, expires_at);

CREATE TABLE IF NOT EXISTS autopilot_policy_changes (
    change_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    category        TEXT    NOT NULL,
    old_level       TEXT,
    new_level       TEXT    NOT NULL,
    reason          TEXT,                          -- promote|demote|kill|init
    actor           TEXT    NOT NULL,
    changed_at      TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS autopilot_metrics_snapshots (
    snapshot_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    captured_at     TEXT    NOT NULL,
    fp_rate         REAL,                          -- rejected / surfaced (rolling 30d)
    accept_rate     REAL,                          -- accepted / surfaced
    snooze_rate     REAL,
    apply_rate      REAL,
    revert_rate     REAL,
    findings_total  INTEGER,
    proposals_total INTEGER,
    metrics_json    TEXT
);
```

Migration is idempotent. Schema changes go through standard daemon migration path (cf. `cli_handlers::cli_memory_store` for the pattern).

---

## 11. CLI + MCP surface

### 11.1 CLI subcommands (new, under `touring autopilot`)

| Subcommand                                          | Purpose                                                       |
|-----------------------------------------------------|---------------------------------------------------------------|
| `touring autopilot status [-j]`                     | Health snapshot (level per category, FP rate, last scan)      |
| `touring autopilot scan [--scope <s>]`              | Run a manual scan; emits findings count                       |
| `touring autopilot list [--include-low\|all] [-j]`  | List active proposals (default: medium+ only)                 |
| `touring autopilot show <id>`                       | Display proposal markdown                                     |
| `touring autopilot accept <id>`                     | Mark accepted (RL +1, ledger updated, no auto-apply)          |
| `touring autopilot reject <id> [--reason <r>]`      | Mark rejected (RL -1, dedup 24h)                              |
| `touring autopilot snooze <id> <duration>`          | e.g. `7d`, `2w`, `1m`                                         |
| `touring autopilot apply <id>`                      | Materialize via generator (only if speculatable + level ≥ L3) |
| `touring autopilot revert <id>`                     | Best-effort revert (uses pre-state snapshot if recorded)      |
| `touring autopilot enable <category> [--level Lx]`  | Promote category autonomy                                     |
| `touring autopilot disable [<category>]`            | Per-category or global silence                                |
| `touring autopilot demote <category>`               | Lower autonomy by 1                                           |
| `touring autopilot quiet-hours <hh:mm-hh:mm>`       | Set notification quiet window                                 |
| `touring autopilot history [--last N]`              | Show decision history                                         |
| `touring autopilot metrics [-j]`                    | FP/accept/snooze/revert rates rolling 30d                     |
| `touring autopilot policy [-j]`                     | Show current policy.toml                                      |

### 11.2 MCP tools

Mirror CLI for programmatic access:

| Tool                              | Params                                                    |
|-----------------------------------|-----------------------------------------------------------|
| `touring_autopilot_status`        | `{}`                                                      |
| `touring_autopilot_list`          | `{include_low?: bool}`                                    |
| `touring_autopilot_show`          | `{id: string}`                                            |
| `touring_autopilot_decide`        | `{id, decision: accept\|reject\|snooze, duration?}`       |
| `touring_autopilot_apply`         | `{id}`                                                    |
| `touring_autopilot_metrics`       | `{}`                                                      |

### 11.3 Hooks integration

| Hook                        | Autopilot action                                                         |
|-----------------------------|--------------------------------------------------------------------------|
| `pre-edit`                  | Quick D09 (gotcha match) check on candidate edit; surface inline if hit  |
| `post-edit`                 | Trigger D01, D02, D11, D12, D08 with `ScanScope::File`                   |
| `post-write`                | Same as post-edit + D02 (orphan likely)                                   |
| `post-tool-failure`         | Increment D07 (pattern.repeated_failure) counter                          |
| `instructions-loaded`       | Inject digest entry of P1+ findings via `additionalContext`              |
| `session-start`             | Show summary of unread proposals from previous session                    |
| `session-stop`              | Persist autopilot state; trigger weekly cron (D07, D15) if due           |
| `post-compact`              | Reset session-scoped rate limit                                           |

### 11.4 Hook registry impact

- New handlers: `cli-autopilot-{scan,list,show,accept,reject,snooze,apply,revert,enable,disable,demote,history,metrics,policy,status}` — **15 entries**.
- ALL_DAEMON_HOOK_NAMES count: 171 → **186**.

---

## 12. Roadmap de implementação (Phase Pre-A → H)

Cada phase é um deliverable atômico, validado por Gabriel antes da próxima começar. Estimativas por T-shirt sizing.

### Phase Pre-A — Workspace abstractions (foundation-of-foundation) — **M size, 3-5 dias** ✅ AUTORIZADO 2026-04-25

**Goal**: introduzir as 4 abstrações fundamentais do expansion plan (§3) ANTES dos detectors. Sem código de detection ainda — apenas o substrato sobre o qual Phase A construirá os 3 detectors iniciais. Custo agora: 3-5 dias. Custo se pulado: 6 semanas de retrofit em Phase J + rewrite de todos os 16 detectors.

**Deliverables atômicos**:

| Sub-task | Arquivo | LOC | Tests | Output |
|---|---|---|---|---|
| Pre-A.1 — `WorkspaceProfile` trait + `WorkspaceUri` + `Language` enum | `crates/touring-hooks/src/autopilot/workspace.rs` | ~200 | 8 | trait compila + `Language::from_extension()` mapeia 12 extensões |
| Pre-A.2 — `WorkspaceCapability` enum + `WorkspaceCapabilities` set | `crates/touring-hooks/src/autopilot/capability.rs` | ~150 | 6 | 11 variantes + `.has()` + roundtrip serde |
| Pre-A.3 — `TouringWorkspaceProfile` impl (wraps `HookRuntime`) | `crates/touring-hooks/src/autopilot/profile_touring.rs` | ~180 | 5 | wrapper sobre `~/.claude/rust/`; declara 8 capabilities |
| Pre-A.4 — `GenericWorkspaceProfile` impl (filesystem fallback) | `crates/touring-hooks/src/autopilot/profile_generic.rs` | ~120 | 4 | fallback para qualquer dir; declara apenas `AstParsing(detected)` |
| Pre-A.5 — `WorkspaceRegistry` (DashMap singleton) | `crates/touring-hooks/src/autopilot/registry.rs` | ~140 | 6 | register/unregister/get/list + `handle_change()` LSP-equivalent |
| Pre-A.6 — `.autopilot/manifest.toml` schema + loader | `crates/touring-hooks/src/autopilot/manifest.rs` | ~180 | 7 | parse `[autopilot.capabilities]` + `[autopilot.policy]` + `[autopilot.privacy]` (preserva extensão `.holon/`) |
| Pre-A.7 — Auto-register `~/.claude/rust/` no daemon startup | edits em `crates/touring-hooks/src/daemon.rs` (~30 LOC) | — | 2 E2E | daemon boot → registry contém 1 profile (current workspace) |
| Pre-A.8 — Stub CLI: `touring autopilot workspace {register,list,info}` | `crates/touring-server/src/cli/autopilot_workspace.rs` | ~100 | 3 | 3 subcomandos retornam JSON; nenhum scan ainda |

**Total Pre-A**: ~1.100 LOC + 41 tests + 0 detectors (zero surfacing — só substrato).

**Why these 8 sub-tasks specifically**:
- Pre-A.1+Pre-A.2 são as duas estruturas de tipo que TODO detector vai consumir. Sem isso, cada detector hard-codes `&Path` e quebra cross-workspace.
- Pre-A.3 é o "happy path" actual: o autopilot deve funcionar para `~/.claude/rust/` no dia 1 sem regressão de UX.
- Pre-A.4 prova que a abstração não vaza touring assumptions — qualquer dir vira workspace mínimo.
- Pre-A.5 é o singleton onde Phase A vai pendurar `scan(profile)` calls.
- Pre-A.6 prepara o ground para Phase J (workspaces externos lêem `.autopilot/manifest.toml` para opt-in); em Phase Pre-A apenas o parser entra, sem fluxo end-to-end.
- Pre-A.7 garante que o registry NUNCA está vazio em runtime (default = current workspace).
- Pre-A.8 dá a Gabriel uma superficie inspectable já neste ponto (`touring autopilot workspace list`).

**Acceptance criteria (Pre-A)**:
- [ ] `cargo check --workspace` exit 0
- [ ] 41 unit tests PASS
- [ ] `touring autopilot workspace list -j` retorna `[{"uri":"file:///home/gabrielgadea/.claude/rust","name":"touring-workspace","capabilities":[8 items]}]`
- [ ] `touring autopilot workspace info <uri>` retorna policy completa + cache_root + autopilot_db_path
- [ ] `GenericWorkspaceProfile::from_path("/tmp/dummy")` constrói com `capabilities = []` (graceful)
- [ ] `WorkspaceRegistry::handle_change(added=[uri], removed=[])` é idempotente (re-add é no-op)
- [ ] `manifest.rs` parser rejeita TOML malformado com erro estruturado (não panic)
- [ ] Hard Rule #11 preserved: zero git invocation no path Pre-A
- [ ] **Zero surfacing** — `touring autopilot scan` ainda NÃO existe (chega em Phase A)

**Não incluído em Pre-A** (ficam para Phase A):
- Trait `AutopilotDetector` (precisa de `WorkspaceProfile` antes — ordem importa)
- SQLite `autopilot_findings` schema (precisa decidir se per-workspace ou global)
- Polyglot dispatch (`LanguageQualityProvider`) — Phase K do expansion plan
- Per-workspace RL isolation — Phase L do expansion plan
- THSF capability namespace — Phase M do expansion plan

**Rationale para AUTORIZAÇÃO Pre-A** (Gabriel directive 2026-04-25):
> "autorizo Pre-A"

ROI: 5 dias agora vs 6 semanas de retrofit + rewrite de 16 detectors quando expansion chegar. Confidence 0.95 (HIGH) que esta é a decisão correta — abstrações cedo são baratas; abstrações tarde são caras.

### Phase A — Foundation (no proposals, just detection) — **L size, 1 semana**

**Goal**: trait `AutopilotDetector` (consome `&dyn WorkspaceProfile` de Pre-A), ledger schema, 3 detectors initial (D02 wiring.orphan, D08 health.regression_streak, D15 todo.stagnant), `touring autopilot scan` CLI, `list` CLI, `status` CLI. Findings persist; nothing surfaces.

**Why these 3 detectors first**:
- D02 reuses existing wiring orphans (lowest cost). Requires `WorkspaceCapability::WiringGraph` — only Touring workspace satisfies em Phase A; outros silently skip.
- D08 reuses existing health_delta streak (W13). Requires `WorkspaceCapability::HealthDelta`.
- D15 reuses `touring ast todos` (already exists). Requires `WorkspaceCapability::AstParsing`.

**Mudança vs spec original**: cada detector recebe `profile: &dyn WorkspaceProfile` em vez de hard-coded `Path`. `scan_workspace()` chama `registry.list()` e itera. Single-workspace é caso degenerado de N=1 — nenhuma regressão de UX no current state.

**Acceptance**: 3 scans produce findings ≥ 5 each across the rust workspace; ledger queryable; zero surfacing.

### Phase B — Confidence + Triage — **M size, 3 dias**

**Goal**: confidence formula (§6.1), priority calc (§6.3), filter at threshold 0.5, store priority in ledger.

**Acceptance**: findings have confidence + priority columns; `touring autopilot list` ordered by priority.

### Phase C — Speculator integration — **L size, 1 semana**

**Goal**: for `speculatable: true` findings, invoke `touring generate plan-suggest --intent <synthesized>` + `plan-speculate`; record `speculation_score` in proposals table; drop if < 0.6.

**Acceptance**: D02 (wiring.orphan) findings get an auto-fix candidate plan (saved to `.touring-cache/autopilot/plans/<finding_id>.json`) with score ≥ 0.6.

### Phase D — Surfacing (L0 → L1 only) — **M size, 3 dias**

**Goal**: `touring autopilot list` is the only surface (pull-based). Proposal markdown drafted for `state=READY` findings. Gabriel reads via `show <id>`. Decisions via `accept/reject/snooze`.

**Acceptance**: end-to-end scan → confidence → triage → speculation → proposal markdown → Gabriel decides → ledger updated. **NO** PushNotification, **NO** additionalContext injection yet (avoid noise during dial-in).

### Phase E — Decision tracking + RL feedback — **M size, 3 dias**

**Goal**: wire `touring learning reward` calls into accept/reject/snooze; suppression records inserted; metrics snapshots captured daily via cron.

**Acceptance**: rejecting same finding twice in 30d auto-suppresses for 60d; LinUCB arm selection visibly biases toward high-acceptance categories after 50+ decisions.

### Phase F — Detector expansion (D01, D04-D07, D09-D14) — **XL size, 2 semanas**

**Goal**: implement remaining 12 detectors. Each is small (~150 LOC + 5 tests + spec doc), but 12 of them total.

Sub-deliverables (parallelizable, 2-3 per iteration):
- F1: D01, D12 (quality + complexity — share `RustQualitySignals`)
- F2: D04, D11 (testing — share mutation-test cache)
- F3: D03, D13 (wiring — share wiring DB)
- F4: D06, D14 (docs/api — share api_cascade)
- F5: D05, D07 (perf/pattern — share temporal + tfidf)
- F6: D09, D10 (gotcha + supply — share rule libraries)
- F7: **D16 format.drift** (Rust MVP) — `format_rust_code` + `similar` diff + impact bucketing + dedup-with-refactor logic. Adds `similar` workspace dep. Default L0; Gabriel must `enable format.drift` to make visible. ETA: 2-3 dias.

**Acceptance**: 16 detectors operational; full FP rate measurement enabled; D16 specifically validated against synthetic fixtures with known formatting drift (whitespace-only, structural, mixed) producing expected impact_score buckets.

**F7 acceptance criteria specific**:
- [ ] `touring autopilot scan --category format.drift` runs end-to-end on `crates/touring-hooks/`
- [ ] Synthetic test: file with 10 whitespace-only diffs → `impact_score < 0.5` (skipped)
- [ ] Synthetic test: file with 5 structural reorderings → `impact_score > 0.7` (proposed)
- [ ] Dedup test: D01 + D16 simultaneously emitting on same file → only D01 surfaced with format step absorbed
- [ ] `#[rustfmt::skip]` test: file with 50% functions skipped → D16 skipped per heuristic
- [ ] Multi-lang stub: `touring ast format-python` etc. NOT shipped (Phase F.7+ extension)

### Phase G — Autonomy controller + L2 surfacing — **M size, 4 dias**

**Goal**: policy.toml + auto-demotion rules + L2 surfacing (additionalContext digest entry; max 3/session). PushNotification still gated to Gabriel opt-in.

**Acceptance**: Gabriel runs `touring autopilot enable wiring.orphan --level L2`; subsequent session shows ≤ 3 wiring proposals in additionalContext; FP rate spike auto-demotes back to L1.

### Phase H — Cross-validation harness + observability — **L size, 1 semana**

**Goal**:
- `tests/autopilot_e2e.rs` — synthetic workspace fixtures with known pattern injections; assert detector recall + precision.
- `cargo bench --bench autopilot_throughput` — scan latency budget regression guard.
- gate-metrics counters: `autopilot_{scan,finding,proposal,decision,fp,accept,snooze,apply,revert}_count` + per-category breakdowns.
- `touring autopilot metrics -j` dashboard.
- Documentation: `docs/autopilot/playbook.md`, `docs/autopilot/detectors/D{NN}.md` x 15.

**Acceptance**: target metrics met:
- FP rate < 5% (rolling 30d)
- Accept rate > 60% (high-priority findings)
- P95 scan latency < 5s for `Workspace` scope
- Zero spurious PushNotifications in last 14d

### Phase I (FUTURE, gated) — L3-L5 + advanced detectors

Reserved. Discuss with Gabriel after H completes and metrics stabilize for 30+ days.

### Roadmap timeline summary

```
Week 0.5-1: Phase Pre-A (M, 3-5 dias) ✅ AUTORIZADA — workspace abstractions
Week 1-2: Phase A   (L)              — 3 detectors initial (consume WorkspaceProfile)
Week 2-3: Phase B+C (M+L)
Week 3-4: Phase D+E (M+M)
Week 4-5: Phase F.1, F.2 (XL split)
Week 5-6: Phase F.3, F.4, F.5 (XL continued)
Week 6-7: Phase F.6 + G (XL+M)
Week 7-8: Phase H   (L)
Week 8-9: Validation + Gabriel sign-off
        ↓
Week 9+: Phase I (gated, optional) | Phase J-M (expansion plan, gated)
```

**Total estimate**: 6.5-9.5 calendar weeks (Pre-A: +0.5-1 semana sobre o estimate original; ROI: 6 semanas de retrofit em Phase J evitadas). Gabriel decision pauses entre cada fase preserved.

---

## 13. Métricas de sucesso

### 13.1 Output metrics (rolling 30d windows)

| Metric                          | Target          | Failure mode → action                                        |
|---------------------------------|-----------------|--------------------------------------------------------------|
| FP rate (rejected / surfaced)   | < 5%            | Auto-demote category; alert Gabriel                          |
| Accept rate (P0+P1)             | > 60%           | Tune confidence weights; review detector implementations     |
| Mean time-to-decision           | < 24h           | Re-evaluate surfacing channels; maybe escalate priority      |
| Snooze rate                     | < 20%           | Findings too noisy; tighten triage thresholds                |
| Apply success rate (L4+)        | > 95%           | Improve speculator coverage                                  |
| Revert rate (of applied)        | < 1%            | Strong signal autopilot is causing harm; auto-disable L4+    |

### 13.2 Latency metrics

| Metric                              | Target  |
|-------------------------------------|---------|
| Hot-path detector scan (single file)| < 200ms |
| Workspace scan (cron)               | < 60s   |
| Speculator per-finding              | < 30s   |
| Decision recorder write             | < 50ms  |
| Surfacing latency (decision → ack)  | < 1s    |

### 13.3 Coverage metrics

| Metric                                   | Target |
|------------------------------------------|--------|
| Detector code coverage                   | > 85%  |
| Mutation kill rate (autopilot module)    | > 75%  |
| Detector spec docs (D{NN}.md present)    | 15/15  |
| Hook registry consistency tests          | 100%   |

### 13.4 Adoption metrics (Gabriel-facing)

- Categories with autonomy ≥ L1 → reflects Gabriel's trust
- Number of accepted proposals leading to merged code → demonstrates value
- Frequency of `touring autopilot disable` → inverse trust signal

---

## 14. Risks + mitigations

| Risk                                                                  | Prob   | Impact | Mitigation                                                                                                            |
|-----------------------------------------------------------------------|--------|--------|-----------------------------------------------------------------------------------------------------------------------|
| **R1**: Notification fatigue → Gabriel ignores autopilot              | HIGH   | HIGH   | L0 default, rate limit 5/session, quiet hours, weekly digest preferred over real-time push                             |
| **R2**: False positive rate > 10% destroys trust                      | MEDIUM | HIGH   | Confidence ≥ 0.85 for surfacing; auto-demote on FP spike; Phase H validation harness                                  |
| **R3**: Speculator latency makes hot path unusable                    | MEDIUM | MEDIUM | Speculator runs in `touring jobs spawn` background; hot path emits findings only, speculation deferred                |
| **R4**: SQLite ledger contention under heavy scan load                | LOW    | MEDIUM | Reuse actor pattern from daemon (per-project actor); batch writes                                                     |
| **R5**: Detector panics crash daemon                                  | LOW    | HIGH   | catch_unwind around each `scan()`; demote category on panic; daemon untouched                                          |
| **R6**: Confidence formula weights are arbitrary                      | HIGH   | MEDIUM | Phase H benchmark against synthetic fixtures; bandit-tune w1..w5 over time                                            |
| **R7**: Autopilot recommends git-violating actions                    | MEDIUM | HIGH   | Hard-coded prohibition: any proposal whose `suggested_action` includes "git" is auto-rejected at validation step      |
| **R8**: L4+ apply causes regression                                   | LOW    | CRITICAL | Mandatory revert path test before category eligible for L4; rollback within 60s of apply if regression detected      |
| **R9**: Autopilot competes with TACO orchestration (concurrent edits) | MEDIUM | MEDIUM | Autopilot L0-L3 NEVER edit; L4+ goes through TACO Phase 5 (engineer agent) using normal locks                          |
| **R10**: Cron scan blocks daemon hot requests                         | MEDIUM | LOW    | Cron runs in dedicated rayon pool; per-project actor handles rate limiting                                            |
| **R11**: 22-crate dependency surface introduces churn                 | HIGH   | LOW    | Detector trait isolates impl; one crate change = one detector update                                                   |
| **R12**: Storage growth (proposals + plans markdown)                  | LOW    | LOW    | Auto-purge `state=COMPLETED OR EXPIRED` older than 90d                                                                 |
| **R13**: User-perceived autonomy creep                                | MEDIUM | HIGH   | Promotions L1→L2 etc. require explicit `enable` CLI; weekly metrics sent to Gabriel via PushNotification              |
| **R14**: Detector logic encodes biases (e.g. "Rust-only")             | MEDIUM | LOW    | Phase H multi-lang fixtures (Rust + Python + TS + Go via touring-ast-polyglot)                                         |
| **R15**: Gabriel's intent shifts and policy.toml drifts               | LOW    | MEDIUM | `touring autopilot policy --suggest-tune` quarterly: surface categories where accept rate suggests promote/demote     |
| **R16**: prettyplease/rustfmt version drift creates spurious D16 noise | MEDIUM | LOW    | Pin formatter version in `<ws>/.touring-cache/autopilot/format-pinning.toml`; warn on version change + auto-snooze D16 24h to recompute baseline |
| **R17**: D16 reformats `#[rustfmt::skip]` blocks against author intent | LOW    | MEDIUM | Pre-check: skip file if > 30% functions tagged with `#[rustfmt::skip]`; per-line skip respected by upstream prettyplease |
| **R18**: D16 bulk-reformats legacy code → wave of low-value proposals  | MEDIUM | MEDIUM | Hard rate-limit: max 1 D16 proposal per crate per session; debounce 5min per file post-edit; default L0 (invisible until enable) |
| **R19**: D16 absorbed-as-substep gets lost in parent proposal review   | LOW    | LOW    | When absorbed in D01/D12 refactor proposal, format step is shown as collapsed section with explicit `[autopilot:D16 absorbed]` marker |

---

## 15. Open questions + research needed

1. **Cron scheduling**: where does the daily/weekly cron live? Options:
   - (a) systemd timer (requires root); (b) embedded tokio cron in daemon; (c) GitHub Actions scheduled workflow.
   - **Recommendation**: (b) — daemon already long-running, no extra infra.

2. **Multi-workspace**: autopilot scope is per-workspace (one ledger per `<workspace>/.touring-cache/autopilot.db`). What happens for sub-workspaces (analise/ packages)?
   - **Open**: defer to Phase H; observe behavior with monolith first.

3. **Cross-tool memory**: should autopilot consume `cc_action_suggestions` (Pln3) entries as additional finding source?
   - **Recommendation**: yes, in Phase F as detector D16 (cross_agent.suggestion). Outside MVP scope.

4. **Notification UX**: PushNotification format → markdown summary or single-line?
   - **Open**: validate with Gabriel during Phase G smoke test.

5. **Speculator vs MCTS budget**: when does autopilot use generator's `plan-speculate` vs cognitive's MCTS shadow rollout?
   - **Tentative**: generator for code-edit candidates, MCTS for "is this design choice good?" questions (Phase I).

6. **Confidence calibration**: do we calibrate via Platt scaling? Isotonic regression?
   - **Recommendation**: ship raw RL bandit weights in Phase E, add calibration only if Phase H reveals miscalibration.

7. **Internationalization**: messages in proposal markdown — Portuguese (Gabriel's language) or English (code/CLI convention)?
   - **Recommendation**: hybrid — proposal title/why in PT-BR; commands/code in English. Match CLAUDE.md convention.

8. **Hard Rule #11 path**: revert without git — is there an existing snapshot mechanism in touring?
   - **Investigation**: `touring memory store` could persist file content blob keyed by sha256+path. Not yet implemented; prerequisite for L4+ reversibility.

---

## 16. Anti-patterns to avoid

Drawn from session memory (`touring memory recall "anti-pattern autopilot"` after Phase A):

1. **DON'T** make autopilot generate text that looks like Gabriel's prompts (confusion of authority).
2. **DON'T** accumulate findings silently — every finding must either surface within 7 days or auto-expire.
3. **DON'T** let detector panics propagate to daemon; always `catch_unwind`.
4. **DON'T** invoke generator pipeline in hot path (post-edit < 200ms budget).
5. **DON'T** let autopilot "improve itself" (modify autopilot module via L4+ proposals — recursion bomb).
6. **DON'T** trust touring index for symbol existence without `touring index find` corroboration (cache staleness).
7. **DON'T** propose mutations during merge conflicts or known-broken state (gate via `touring doctor` health check).
8. **DON'T** silently change policy.toml; all promotions/demotions log to `autopilot_policy_changes`.
9. **DON'T** assume Gabriel reads digest entries; require explicit ack before promoting category autonomy.
10. **DON'T** mix detection logic with surfacing logic (separation of concerns: §4.2 components 1-6 vs 7-9).

---

## 17. Implementation file layout (suggested)

```
crates/touring-hooks/src/autopilot/
├── mod.rs                         # public API + re-exports
│
│  # ─── Phase Pre-A (workspace abstractions, ships first) ───
├── workspace.rs                   # trait WorkspaceProfile + WorkspaceUri + Language enum
├── capability.rs                  # WorkspaceCapability enum + WorkspaceCapabilities set
├── profile_touring.rs             # TouringWorkspaceProfile (wraps HookRuntime)
├── profile_generic.rs             # GenericWorkspaceProfile (filesystem fallback)
├── registry.rs                    # WorkspaceRegistry (DashMap singleton)
├── manifest.rs                    # .autopilot/manifest.toml schema + loader
│
│  # ─── Phase A onwards (detection layer) ───
├── detector.rs                    # trait AutopilotDetector + ScanScope + Severity (consumes &dyn WorkspaceProfile)
├── finding.rs                     # Finding struct + serde + hash
├── confidence.rs                  # confidence formula + tier classification
├── triage.rs                      # priority calc + budget allocation (uses GranularityBandit)
├── speculator.rs                  # generator integration (plan-suggest + plan-speculate)
├── proposer.rs                    # markdown drafter + evidence bundler
├── surface.rs                     # additionalContext + digest + push channels
├── ledger.rs                      # SQLite CRUD over autopilot_* tables (per-workspace via WorkspaceProfile::autopilot_db_path)
├── policy.rs                      # policy.toml load + autonomy controller
├── rl_bridge.rs                   # touring learning reward integration
├── metrics.rs                     # gate_metrics counters
├── detectors/
│   ├── mod.rs                     # registry
│   ├── d01_quality_regression.rs
│   ├── d02_wiring_orphan.rs
│   ├── d03_wiring_cache_staleness.rs
│   ├── d04_testing_mutation_gap.rs
│   ├── d05_performance_regression.rs
│   ├── d06_docs_drift.rs
│   ├── d07_pattern_repeated_failure.rs
│   ├── d08_health_regression_streak.rs
│   ├── d09_gotcha_match.rs
│   ├── d10_supply_chain_advisory.rs
│   ├── d11_testing_coverage_gap.rs
│   ├── d12_complexity_spike.rs
│   ├── d13_wiring_cycle_introduced.rs
│   ├── d14_api_breaking_change.rs
│   ├── d15_todo_stagnant.rs
│   └── d16_format_drift.rs        # Rust-only MVP; multi-lang via Phase F.7
└── tests/
    ├── e2e_synthetic_fixtures.rs  # Phase H validation harness
    └── per_detector/              # one fixture per detector

crates/touring-server/src/cli/autopilot.rs   # CLI subcommand
crates/touring-hooks/src/cli_handlers_autopilot.rs   # daemon handlers (15 cli-autopilot-* entries)

docs/autopilot/
├── playbook.md
├── policy-template.toml
├── detectors/
│   ├── D01-quality-regression.md
│   ├── D02-wiring-orphan.md
│   └── ...
└── decisions/                     # ADRs as decisions accumulate
```

**Estimated total LOC**:
- Pre-A workspace abstractions (workspace/capability/profile_touring/profile_generic/registry/manifest): ~970 LOC
- Pre-A daemon wiring + CLI stub: ~130 LOC
- Pre-A tests: ~600 LOC (41 tests)
- Core (autopilot/*.rs minus Pre-A): ~1.500 LOC
- Detectors (16 × ~150 LOC avg, includes D16): ~2.400 LOC
- CLI + handler: ~700 LOC (added `autopilot workspace {register,list,info,unregister,activate}`)
- Tests (Phase A-H): ~2.000 LOC
- Docs: ~3.000 lines markdown (master + expansion + 16 per-detector specs)
- **Grand total**: ~8.300 LOC + 3.000 lines docs (~+1.950 vs pre-Pre-A estimate, +6 weeks retrofit avoided)

---

## 18. Decision matrix: por que cada componente é necessário

Para evitar over-engineering, cada componente de §4.2 justifica sua existência:

| Component                  | Could we skip?                                                              |
|----------------------------|------------------------------------------------------------------------------|
| 1. Detector trait          | NO — abstraction allows adding new detectors without core changes            |
| 2. Aggregator dedup        | NO — same finding from D02+D08 must merge to avoid double-surfacing          |
| 3. Confidence engine       | NO — without filtering, FP rate kills trust (R2)                             |
| 4. Triage engine           | NO — priority gates surfacing channel selection                              |
| 5. Speculator              | YES initially (Phase A-D skip); REQUIRED for L3+ (R8)                        |
| 6. Proposer                | NO — markdown is the audit trail Gabriel reviews                             |
| 7. Surface adapter         | NO — channeled push is what makes autopilot proactive                        |
| 8. Decision recorder       | NO — without ledger, no learning, no audit                                   |
| 9. RL feedback             | YES initially (Phase A-D); REQUIRED to converge to Gabriel's preferences    |
| 10. Autonomy controller    | YES initially (all-L0); REQUIRED before any L1+ enablement                  |

**Phase A MVP** = components {1, 2, 6 (minimal), 8} — produces findings + ledger; no surfacing, no learning. Validates the fundamental "do detectors find real signal" question before investing in surfacing/learning machinery.

---

## 19. Comparison to existing tools (context)

| Tool                 | Approach                          | Touring autopilot differs by                              |
|----------------------|-----------------------------------|------------------------------------------------------------|
| GitHub Copilot       | Inline suggestions during editing | Background detection; not synchronous to typing            |
| rust-analyzer assists| User-invoked refactoring          | Proactive (pull → push); cross-file findings              |
| SonarLint / SonarQube| Linter + dashboard                | RL-tuned per Gabriel; touring-graph-aware (blast radius)  |
| Dependabot           | Dependency updates only           | 15 categories, only D10 overlaps                           |
| Aider, Continue.dev  | Synchronous AI pair-programmer    | Asynchronous co-processor; not an LLM in hot loop          |

The unique value: **touring graph + RL convergence + Hard Rule #11 compliance + per-Gabriel customization via policy.toml**.

---

## 20. Authorization matrix

This document is a SPEC, not authorization. Phase-by-phase explicit Gabriel approval required.

**Genealogy note**: this document supersedes the original A1 (Autonomous detect-propose loop) item from `2026-04-24-waves-Q-R-M-A-T-P-plan.md`. Task #64 (A1) is to be marked **succeeded by autopilot** — the work continues here, no scope is lost.

| Phase  | Authorization needed                                              | Granted? | Decided at |
|--------|-------------------------------------------------------------------|----------|------------|
| Spec   | "Aprovar este SPEC v1.0 como referência"                          | Pending  | —          |
| Pre-A  | "Iniciar Pre-A (workspace abstractions, no detection)"            | **YES**  | 2026-04-25 |
| A      | "Iniciar foundation (3 detectors + ledger, consume Pre-A traits)" | Pending Pre-A acceptance | — |
| B     | "Adicionar confidence + triage"              | Pending  | —          |
| C     | "Wire speculator to touring-generator"       | Pending  | —          |
| D     | "Permitir surfacing L1 (CLI list)"           | Pending  | —          |
| E     | "Wire RL feedback loop + suppressions"       | Pending  | —          |
| F.1   | "Detectors D01, D12 (quality + complexity)"  | Pending  | —          |
| F.2   | "Detectors D04, D11 (testing batch)"         | Pending  | —          |
| F.3   | "Detectors D03, D13 (wiring batch)"          | Pending  | —          |
| F.4   | "Detectors D06, D14 (docs/api batch)"        | Pending  | —          |
| F.5   | "Detectors D05, D07 (perf/pattern batch)"    | Pending  | —          |
| F.6   | "Detectors D09, D10 (gotcha+supply batch)"   | Pending  | —          |
| F.7   | "Detector D16 format.drift (Rust MVP)"       | Pending  | —          |
| G     | "Permitir L2 (additionalContext digest)"     | Pending  | —          |
| H     | "Promover para production + cross-validation"| Pending  | —          |
| I     | "Avaliar L3+ (DRAFT/STAGED/AUTO) — gated"    | Pending  | —          |

Each grant is logged in `autopilot_policy_changes` with timestamp + actor.

---

## 21. Glossary

| Term                | Definition                                                                                       |
|---------------------|--------------------------------------------------------------------------------------------------|
| Autonomy level      | L0-L5 enum controlling per-category surfacing/applying behavior                                  |
| Confidence          | f32 ∈ [0,1] derived from multi-source corroboration + RL prior                                   |
| Detector            | Implementation of `AutopilotDetector` trait scanning a category                                  |
| Finding             | Single observed signal from a detector (not yet a proposal)                                      |
| Proposal            | Materialized markdown plan with evidence, awaiting Gabriel's decision                            |
| Severity            | LSP-style {Critical, Warning, Info, Hint}                                                        |
| Speculator          | Pre-proposal validator using `touring generate plan-speculate`                                   |
| Triage priority     | f32 ∈ [0,1] = severity × (1 - blast/50) × confidence × age_decay                                |
| VP-Scout            | 7-chain false-positive verifier (see VP-Scout.md v1.1)                                           |
| TACO                | Touring Agentic Code Orchestrator — imperative orchestration layer (CLAUDE.md §TACO)             |
| Hard Rule #11       | Absolute prohibition on git commands (CLAUDE.md §11)                                             |

---

## 22. Addenda (post-spec changes)

### Addendum A1 — 2026-04-25 — Auto-formatting reinstated as D16

**Source**: Gabriel directive, session 2026-04-25.

**Original §1.2 position**: "NÃO é code-formatter automático. Formatting fica com `prettyplease`/`rustfmt` invocados por Gabriel."

**Revised position**: Formatting é uma categoria legítima do autopilot (D16 `format.drift`) DESDE QUE acompanhada de **previsibilidade** (opt-in declarativo) e **diagnóstico de impacto** (whitespace vs structural bucketing antes de propor).

**Rationale (Gabriel's quote, paraphrased)**: "Touring-autopilot pode sim considerar a formatação automática do código, desde que seja prevista e diagnosticado o impacto dessa formatação."

**Sections updated**:
- §1.2 — exclusion revoked + revision note
- §3.1 — `format_rust_code` + `format_rust_code_best_effort` added to touring-ast capabilities (15 total)
- §5.1 — D16 added to detectors table (Hint severity, speculatable)
- §5.2.1 — full algorithm spec for D16 (impact_score formula, dedup-with-refactor, autonomy effects, observability counters)
- §5.2.2 — multi-language formatting deferred to Phase F.7+ extension
- §12 — Phase F.7 added to roadmap with explicit acceptance criteria
- §14 — R16, R17, R18, R19 added (formatter version drift, skip-attr, bulk-format, absorbed-substep)
- §17 — `d16_format_drift.rs` added to file layout
- §20 — F.7 authorization slot added

**New workspace dep needed**: `similar` crate (line-diff for impact bucketing). Estimated touring-hooks/Cargo.toml addition.

**Compliance with Gabriel's condition**:
- ✅ Previsto: default L0 (invisible); explicit `touring autopilot enable format.drift --level L1+` required
- ✅ Diagnosticado: `format_impact_score` segregating whitespace_only_lines vs structural_lines mandatory in every proposal markdown; threshold 0.5 minimum
- ✅ Reversibility: pre-format snapshot stored in memory; revert is instant (re-write original)
- ✅ NEVER L5 (auto-apply silent) — violates "previsto" requirement

### Addendum A2 — 2026-04-25 — Pre-A AUTORIZADA (workspace abstractions inserted before Phase A)

**Source**: Gabriel directive, session 2026-04-25 — exact wording: "autorizo Pre-A".

**Decision context**: companion expansion plan (`2026-04-25-touring-autopilot-expansion-plan.md`) demonstrou que sem as 4 abstrações fundamentais (`WorkspaceProfile`, `WorkspaceCapability`, `WorkspaceRegistry`, `.autopilot/manifest.toml`), todos os 16 detectors do Phase A-F precisariam rewrite quando autopilot expandir externamente (estimate: 6 semanas de retrofit + risco de regressão em paths estáveis). A alternativa é antecipar as abstrações em Phase Pre-A (custo: 3-5 dias, 41 tests, ~1.100 LOC).

**Trade-off resolved**: 5 dias agora vs 6 semanas depois. ROI: ~12× em ciclos calendário, sem contar custo de rewrite + risco de regressão. Autorização explícita.

**Sections updated**:
- §0 — companion-document note revised: "Pre-A AUTORIZADA por Gabriel em 2026-04-25 ✅" (substitui "Recomendação crítica")
- §12 — new **Phase Pre-A** section inserted between §12 intro and Phase A. 8 sub-tasks (Pre-A.1 → Pre-A.8) with file paths, LOC estimates, test counts, acceptance criteria, and rationale per sub-task
- §12 Phase A — goal text updated: trait `AutopilotDetector` consumes `&dyn WorkspaceProfile` from Pre-A; each detector uses `WorkspaceCapability::*` to declare requirements; `scan_workspace()` iterates `registry.list()`
- §12 Roadmap timeline summary — Week 0.5-1 added for Pre-A; total estimate revised 6.5-9.5 weeks (was 6-9)
- §17 — file layout split into "Phase Pre-A (workspace abstractions)" block (6 new files) + "Phase A onwards (detection layer)" block; `ledger.rs` gains comment about per-workspace path via `WorkspaceProfile::autopilot_db_path`
- §17 LOC estimate — Pre-A breakout (970 + 130 + 600 = 1.700 LOC) added; grand total 6.350 → 8.300 LOC

**Compliance with master plan principles**:
- ✅ Princípio §1.1.6 (REUTILIZA, NÃO REINVENTA): Pre-A reusa `HookRuntime` em `TouringWorkspaceProfile` (zero rewrite); apenas adiciona trait wrapper
- ✅ Princípio §1.1.8 (Hard Rule #11 INVIOLÁVEL): zero git invocation no path Pre-A — explicitamente verificado em acceptance criteria
- ✅ Princípio §1.1.4 (GRADIENTE DE AUTONOMIA): Pre-A entrega ZERO surfacing; nenhum scan, nenhum proposal — apenas substrato. Phase A continua com L0 default
- ✅ Princípio §1.1.6 (delgada layer): Pre-A é DUAS impls (TouringWorkspaceProfile + GenericWorkspaceProfile); abstração que paga seu próprio custo via dois consumers, não um

**Acceptance gate Pre-A → Phase A**:
- 8/8 sub-tasks PASS (all unit + E2E tests)
- `cargo check --workspace` exit 0
- Daemon boot wireup verified (registry contains current workspace by default)
- `touring autopilot workspace list -j` returns valid JSON
- Hard Rule #11 audit clean (grep for `git ` in `crates/touring-hooks/src/autopilot/` returns 0 matches)

**Next concrete action when Pre-A starts**: TACO Phase 0 health gate (`cargo check --workspace` + `touring doctor -j`) → Phase 1 scout (VP-Scout chains for `WorkspaceProfile` symbol existence — should be NONE, expected) → Phase 4 decompose Pre-A.1-Pre-A.8 as DAG → Phase 5 engineer sequential (Pre-A.1 → Pre-A.2 → ... → Pre-A.8, no parallelism in foundation work).

---

**End of master plan v1.0 + Addendum A2**

_Next action_: Pre-A AUTORIZADA — Gabriel pode emitir "iniciar Pre-A.1" para começar Phase 0 da TACO. Phase A subsequent gating on Pre-A acceptance + Gabriel sign-off (§20 row "Phase Pre-A — Granted: YES — 2026-04-25"; "Phase A — Granted: pending Pre-A acceptance").
