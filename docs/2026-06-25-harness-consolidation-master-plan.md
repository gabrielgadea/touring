# Master Plan — Consolidação do Harness do Touring (Diamond 50-dim)

**Data**: 2026-06-25
**Autor**: TACO (Touring Agentic Code Orchestrator)
**Status**: ⏸️ PLANEJAMENTO (nenhuma modificação executada; aguardando aprovação de Gabriel)
**Pré-requisito**: `2026-06-25-harness-consolidation-diagnostic.md` (lido e aprovado)
**Decisões consolidadas**: Q1=B · Q2=A · Q3=A · Q4=B · Q5=A · Q6=A · Q7=C
**Padrão de qualidade**: 💎 Diamond em todas as 50 dims do harness (REGRA #0: potencializar, nunca reduzir)

---

## Sumário

| Seção | Conteúdo |
|-------|----------|
| 1. **Master Plan** | A visão em 1 parágrafo + 10 princípios invioláveis |
| 2. **Topologia Pós-Consolidação** | Diagrama da nova arquitetura (5 → 3 subsistemas) |
| 3. **Roadmaps** | 3 eixos paralelos (Engine, CLI, Integration) |
| 4. **Waves** | 7 waves sequenciais com checkpoints |
| 5. **Phases** | Aplicação TACO FASE 0..7 em cada wave |
| 6. **Tasks** | 32 tasks atômicas por wave |
| 7. **Subtasks** | Granularidade executável |
| 8. **QuickAction** | Primeiras 5 ações (≤ 30 min de execução) |
| 9. **CLI Specification** | `touring quality` completa com 15 subcomandos |
| 10. **Integration Map** | AST, Graph, LSP, Hooks (pre/post tool use) |
| 11. **50-dim Diamond Standard** | Acceptance criteria para Diamond tier |
| 12. **Riscos & Mitigações** | Matriz completa |
| 13. **Context7 References** | Bibliotecas consultadas |

---

## 1. Master Plan

> **Touring terá UM harness unificado**, organizado em **3 subsistemas com responsabilidade clara**:
>
> 1. **`touring-analysis`** (engine layer, 50 dim detectors reais) — fonte da verdade para detecção.
> 2. **`touring-quality`** (verifier + composite + gates + change + history + report) — **a casa unificada** do harness, com novo módulo `gates.rs` que faz o rollup 50→17, exposto via CLI `touring quality` (15 subcomandos) e via MCP (`touring-server`).
> 3. **`touring-ceg`** (sandbox, concern separado por design) — X7 DECISION passa a incluir o 6º sinal **W_QUALITY=0.20** do composite de 50 dim, integrado via `touring-quality::score_target`.
>
> **Os 14 gates stub são deletados** (Q5). O composite scoring fica **único** baseado em 50 dim (Q4). As 5 tools `touring_elite_*` migram para `touring-server` (Q2). O `touring-harness` e `touring-harness-mcp` desaparecem como crates (Q1, Q2). Score history unificado em um único JSONL (Q7). Tudo atinge **Diamond tier** em todas as 50 dims.

### 10 Princípios Invioláveis (REGRA #0 cristalizada)

| # | Princípio | Justificativa |
|---|-----------|---------------|
| 1 | **Single source of truth**: 50 dims são definidas uma vez em `touring-analysis/src/quality/` | Engines tp/fp-validados em arquivos reais não podem ser duplicados |
| 2 | **Composite único**: SEM dois composite scorers paralelos (Q4) | Bug do dia: 17-gate stub-dominante mascarou F4.x regressão |
| 3 | **No stubs**: 14 gates stub deletados, substituídos por rollup real (Q5) | Stubs retornam score=1.0 com `External` — bug existencial |
| 4 | **Polyglot first**: engines Rust+Python+JS/TS+Go+YAML+TF+MD | F4.6 build_config já é polyglot; o resto deve seguir |
| 5 | **Self-match prevention**: cada engine tem `is_detector_own_source()` | F2.4 fix de hoje + padrão consistente |
| 6 | **P0 BLOCK fail-closed**: 6 BLOCK dims (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5) — pre-write hook bloqueia | Nunca relaxar BLOCK (REGRA #21) |
| 7 | **History unificado**: single JSONL em `~/.claude/touring/quality-history.jsonl` (Q7) | Audit trail único para compliance |
| 8 | **CLI canonical**: `touring quality <sub>` é a única entry point (Q2) | Deletar `touring-elite` e `touring-harness-mcp` |
| 9 | **CEG X7 sempre consulta 50-dim**: W_QUALITY=0.20 no composite_score (Q3) | Sandbox precisa de quality signal, não só capability |
| 10 | **Diamond tier em todas as 50 dims**: 0 BLOCK fail, 0 score < 0.95 | Acceptance criterion inegociável |

---

## 2. Topologia Pós-Consolidação

```
                         ┌─────────────────────────────────────────────┐
                         │       touring-cli  (shell entry point)      │
                         │   $ touring quality <sub> ...               │
                         └─────────────────┬───────────────────────────┘
                                           │ dispatch
                ┌──────────────────────────┼────────────────────────────────┐
                │                          │                                │
        ┌───────▼────────┐         ┌────────▼─────────┐            ┌─────────▼─────────┐
        │ touring-       │         │ touring-server   │            │ touring-quality    │
        │ analysis       │         │ (MCP server)     │            │ (bin: standalone  │
        │ (engine layer) │         │ 90+ tools        │            │  CLI too)          │
        │                │         │ +5 elite tools   │            │                    │
        │ 50 dim engines │         │ migrated from    │            │ quality <sub>...   │
        │ polyglot       │         │ harness-mcp      │            │                    │
        └───────┬────────┘         └────────┬─────────┘            └─────────┬─────────┘
                │                          │                                │
                │ uses analyze_X           │ uses score_target               │ exposes:
                │ uses score_X             │ uses run_quality_pipeline       │ - score_target()
                │                          │                                 │ - run_quality_pipeline()
                │                          │                                 │ - aggregate_to_gates()
                │                          │                                 │ - emit_report()
                │                          │                                 │ - quality_history.jsonl
                │                          │                                 │
                └──────────────────────────┼─────────────────────────────────┘
                                           │
                                  ┌────────▼─────────┐
                                  │ touring-quality  │
                                  │ (UNIFIED HOME)   │
                                  │                  │
                                  │ verifications/   │ ← 50 wrappers
                                  │ gates.rs    [NEW]│ ← GateId + rollup 50→17
                                  │ composite.rs     │
                                  │ aggregate.rs     │
                                  │ change.rs    [MOVED]
                                  │ history.rs   [MOVED]
                                  │ report.rs    [MOVED]
                                  │ runner.rs    [MOVED]
                                  │ tier.rs          │
                                  └────────┬─────────┘
                                           │ uses score_target
                                           │ in X7 DECISION
                                  ┌────────▼─────────┐
                                  │ touring-ceg      │
                                  │ (SANDBOX)        │
                                  │                  │
                                  │ X0..X9 pipeline  │
                                  │ X7 composite:    │
                                  │   W_QUALITY=0.20 │ ← NEW
                                  │   W_STATIC=0.20  │
                                  │   W_VGP=0.15     │
                                  │   W_PREDICT=0.10 │
                                  │   W_SANDBOX=0.15 │
                                  │   W_GATE=0.20    │
                                  └──────────────────┘

DELETED (after W5):
  ✗ crates/touring-harness/         (Q1: dissolve into touring-quality)
  ✗ crates/touring-harness-mcp/     (Q2: tools migrated to touring-server)
  ✗ target/release/touring-elite    (Q1: standalone binário)
  ✗ target/release/touring-harness-mcp (Q2: daemon MCP)
```

**Mudanças concretas no `Cargo.toml` workspace**:

```diff
  [workspace]
  members = [
-     "crates/touring-harness",
-     "crates/touring-harness-mcp",
      "crates/touring-simd",
      "crates/touring-hooks",
      ...
  ]
```

---

## 3. Roadmaps (3 Eixos Paralelos)

### 3.1 Roadmap Eixo A — **Engine Layer** (touring-analysis)

```
A1 [P0] Mover quality/50 dim → mantém
A2 [P0] Adicionar is_detector_own_source() em engines que faltam (audit)
A3 [P1] Adicionar analyze_X_with_ast() variant que aceita AST context
A4 [P1] Surface graph insights (god-nodes, cycles, fan-in/out) em cada engine
A5 [P2] LSP diagnostic adapter (engine.violations_to_lsp_diagnostics())
A6 [P0] Test: cada engine roda em arquivo real (tp/fp-validado)
```

### 3.2 Roadmap Eixo B — **Verifier + Composite + Gates** (touring-quality)

```
B1 [P0] gates.rs: GateId enum (17) + GateMapping + aggregate_to_gates()
B2 [P0] Mover change/history/report/runner de touring-harness
B3 [P0] Deletar 14 stubs de touring-harness/src/builtins/*.rs
B4 [P0] Refactor composite.rs: composite único baseado em 50 dim
B5 [P0] Unificar score history em ~/.claude/touring/quality-history.jsonl
B6 [P0] Re-export GateId para compat com consumidores
B7 [P1] CLI `touring quality` (15 subcomandos)
B8 [P0] Test: 50→17 mapping correctness (cada dim aparece em pelo menos 1 gate)
```

### 3.3 Roadmap Eixo C — **Integration Layer**

```
C1 [P0] CEG X7: adicionar W_QUALITY=0.20 ao composite_score
C2 [P0] touring-server: migrar 5 tools `touring_elite_*` (de harness-mcp)
C3 [P1] LSP: tour­ing-lsp publica diagnostics por dim
C4 [P0] Hooks: pre-write BLOCK se composite < 0.80 (Gold)
C5 [P0] Hooks: post-write append ao history
C6 [P1] Hooks: post-tool-rl reward baseado em dim improvement
C7 [P1] AST: engines aceitam context (call sites, type info)
C8 [P2] Graph: dim F1.8 e F1.12 ganham cross-file aggregation
```

---

## 4. Waves (Sequenciais com Checkpoints)

### W0 — Preparation (QuickAction, ≤ 30 min)

| Task | Description | Acceptance |
|------|-------------|------------|
| W0.T1 | `touring doctor -j` baseline | 6/6 components ok |
| W0.T2 | `cargo test --workspace` baseline | 0 fail, 0 ignored (except known) |
| W0.T3 | `cargo clippy --workspace -- -D warnings` baseline | 0 warnings |
| W0.T4 | `touring-quality score` baseline em `touring-analysis/src/quality/` | 50 dims, composite ≥ 0.80 |
| W0.T5 | `touring memory store "harness-consolidation-w0-baseline"` | persisted |

### W1 — Foundation Migration (Mover Change/History/Report/Runner para touring-quality)

**Goal**: Mover 5 módulos de `touring-harness` para `touring-quality/src/` sem quebrar nada. Compat shim de 1 wave.

| Task | Subtask | File | Acceptance |
|------|---------|------|------------|
| W1.T1 | W1.T1.1 | `crates/touring-quality/src/change.rs` (novo) | `Change`, `ProposedFile`, `FileKind` definitions moved |
| W1.T1 | W1.T1.2 | `crates/touring-quality/src/history.rs` (novo) | `ScoreHistory`, `DriftReport`, `HistoryEntry` moved |
| W1.T1 | W1.T1.3 | `crates/touring-quality/src/report.rs` (novo) | `emit_report`, `ReportFormat` moved |
| W1.T1 | W1.T1.4 | `crates/touring-quality/src/runner.rs` (novo) | `run_quality_pipeline`, `HarnessConfig` moved |
| W1.T1 | W1.T1.5 | `crates/touring-quality/src/lib.rs` | re-export the 5 modules |
| W1.T2 | W1.T2.1 | `crates/touring-harness/src/lib.rs` | DELETED all internal modules |
| W1.T2 | W1.T2.2 | `crates/touring-harness/src/lib.rs` | replaced by 1-line re-exports from `touring_quality` |
| W1.T3 | W1.T3.1 | `crates/touring-harness/Cargo.toml` | `touring-quality = { path = "../touring-quality" }` |
| W1.T3 | W1.T3.2 | `crates/touring-harness/src/builtins/` | convert to 17 re-exports from `touring_quality::gates::aggregate_to_gates` |
| W1.T4 | — | `cargo test --workspace` | 0 fail |
| W1.T5 | — | `cargo clippy --workspace -- -D warnings` | 0 warnings |

**Checkpoint**: `touring-harness` agora é um thin compatibility shim. Todos os consumidores existentes (CEG, harness-mcp, touring-elite binário) continuam funcionando.

### W2 — gates.rs Module + Composite Único (Q5, Q4, Q6)

**Goal**: Adicionar `gates.rs` com 17 gates + rollup 50→17. Composite único baseado em 50 dim. Deletar 14 stubs.

| Task | Subtask | File | Acceptance |
|------|---------|------|------------|
| W2.T1 | W2.T1.1 | `crates/touring-quality/src/gates.rs` (novo) | `GateId` enum (17 variants) + `GateMapping` (table) + `aggregate_to_gates()` function |
| W2.T1 | W2.T1.2 | `crates/touring-quality/src/gates.rs` | `GateScore { gate_id, score, contributing_dims }` struct |
| W2.T1 | W2.T1.3 | `crates/touring-quality/src/gates.rs` | doc comments explicando o mapping 50→17 |
| W2.T2 | W2.T2.1 | `crates/touring-quality/src/gates.rs` (test mod) | test: empty input → empty map |
| W2.T2 | W2.T2.2 | `crates/touring-quality/src/gates.rs` (test mod) | test: single dim → contributes to right gate(s) |
| W2.T2 | W2.T2.3 | `crates/touring-quality/src/gates.rs` (test mod) | test: 50-dim input → 17 gate scores, all ≥ 0 |
| W2.T2 | W2.T2.4 | `crates/touring-quality/src/gates.rs` (test mod) | test: each dim appears in ≥ 1 gate (completeness) |
| W2.T2 | W2.T2.5 | `crates/touring-quality/src/gates.rs` (test mod) | test: WorstOf aggregation (any zero → zero) |
| W2.T2 | W2.T2.6 | `crates/touring-quality/src/gates.rs` (test mod) | test: WeightedLoc aggregation (god-files weigh more) |
| W2.T3 | W2.T3.1 | `crates/touring-quality/src/composite.rs` | NEW `compute_composite_v2()` uses `aggregate_to_gates` then weighted-avg of gates |
| W2.T3 | W2.T3.2 | `crates/touring-quality/src/composite.rs` | keep `compute_composite()` for back-compat, but mark `#[deprecated]` |
| W2.T4 | W2.T4.1-14 | delete 14 files in `touring-harness/src/builtins/` | stub.rs, code_quality.rs, performance.rs, testing.rs, documentation.rs, best_practices.rs, ci_cd_devops.rs, scalability.rs, extensibility.rs, naming.rs, navigability.rs, craftsmanship.rs, dependencies.rs, ux.rs, product_docs.rs |
| W2.T4 | W2.T4.15 | `crates/touring-harness/src/builtins/architecture.rs` | refactored: uses `touring_quality::gates::aggregate_to_gates` for Architecture |
| W2.T4 | W2.T4.16 | `crates/touring-harness/src/builtins/security.rs` | refactored: uses F2.1+F2.4+F2.5+F2.6 (BLOCK dims) |
| W2.T4 | W2.T4.17 | `crates/touring-harness/src/builtins/modularization.rs` | refactored: uses F1.7+F1.11 |
| W2.T5 | — | `cargo test --workspace` | 0 fail, NEW tests pass (≥ 6 new) |
| W2.T6 | — | `cargo clippy --workspace -- -D warnings` | 0 warnings |

**Checkpoint**: 14 stubs deletados. Composite scoring é único (50 dim weighted-avg via gates). `touring-quality::gates::GateId` é o canônico.

### W3 — CLI Unification: `touring quality` + 5 tools migration (Q2)

**Goal**: `touring quality` é a única entry point CLI. As 5 `touring_elite_*` migram para `touring-server`.

| Task | Subtask | File | Acceptance |
|------|---------|------|------------|
| W3.T1 | W3.T1.1 | `crates/touring-cli/src/cli/quality.rs` (novo) | `Quality` enum com subcommands: `Check`, `Score`, `Gate`, `Gates`, `List`, `Explain`, `History`, `Report`, `Compare`, `Baseline`, `Drift`, `Benchmark`, `Fix`, `Register`, `Status` |
| W3.T1 | W3.T1.2 | `crates/touring-cli/src/cli/quality.rs` | handler implementations delegam para `touring_quality::*` |
| W3.T1 | W3.T1.3 | `crates/touring-cli/src/cli/quality.rs` | unified output formats: `--format human|json|toon|badge` |
| W3.T1 | W3.T1.4 | `crates/touring-cli/src/cli/mod.rs` | wire `Quality` variant into `Commands` enum |
| W3.T2 | W3.T2.1 | `crates/touring-server/src/cli/harness_metric.rs` (renomeado) | `crates/touring-server/src/cli/elite_tools.rs` |
| W3.T2 | W3.T2.2 | `crates/touring-server/src/cli/elite_tools.rs` | 5 tool structs: `TouringEliteCheck`, `TouringEliteGate`, `TouringEliteBadge`, `TouringEliteRegister`, `TouringEliteHistory` |
| W3.T2 | W3.T2.3 | `crates/touring-server/src/cli/elite_tools.rs` | each tool calls `touring_quality::*` (NOT `touring_harness::*`) |
| W3.T2 | W3.T2.4 | `crates/touring-server/src/lib.rs` | register the 5 tools in MCP catalog |
| W3.T3 | W3.T3.1 | `crates/touring-harness-mcp/src/main.rs` | DELETED all content |
| W3.T3 | W3.T3.2 | `crates/touring-harness-mcp/src/main.rs` | replaced by 1-line: prints "DEPRECATED: use `touring-server` which now includes elite tools" |
| W3.T3 | W3.T3.3 | `crates/touring-harness-mcp/Cargo.toml` | add `publish = false` + deprecation notice |
| W3.T4 | — | `touring --help` | shows `quality` subcommand with 15 sub-subcommands |
| W3.T5 | — | `touring quality check crates/touring-analysis/src/quality/build_config.rs --format json` | returns 50-dim composite + tier |
| W3.T6 | — | `touring quality gate crates/touring-analysis/src/quality/build_config.rs` | returns 17 gates rolled up from 50 dims |
| W3.T7 | — | `cargo test --workspace` | 0 fail |

**Checkpoint**: `touring quality` é a única CLI. `touring-server` expõe as 5 elite tools. `touring-harness-mcp` é deprecation shim.

### W4 — CEG X7 Integration (Q3)

**Goal**: X7 composite inclui 6º sinal `W_QUALITY=0.20` do `touring-quality::score_target`.

| Task | Subtask | File | Acceptance |
|------|---------|------|------------|
| W4.T1 | W4.T1.1 | `crates/touring-ceg/Cargo.toml` | add `touring-quality = { path = "../touring-quality", default-features = false, optional = true }` |
| W4.T1 | W4.T1.2 | `crates/touring-ceg/Cargo.toml` | feature `quality-signal = ["dep:touring-quality"]` |
| W4.T2 | W4.T2.1 | `crates/touring-ceg/src/gateway/quality_extension.rs` (novo) | `quality_signal_score(target_path: &Path) -> f64` function |
| W4.T2 | W4.T2.2 | `crates/touring-ceg/src/gateway/quality_extension.rs` | calls `touring_quality::score_target(path)` and extracts composite |
| W4.T3 | W4.T3.1 | `crates/touring-ceg/src/gateway/decision.rs` | add `W_QUALITY = 0.20` constant |
| W4.T3 | W4.T3.2 | `crates/touring-ceg/src/gateway/decision.rs` | redistribute weights: W_STATIC=0.20, W_QUALITY=0.20, W_VGP=0.15, W_PREDICT=0.10, W_SANDBOX=0.15, W_GATE=0.20 (total=1.0) |
| W4.T3 | W4.T3.3 | `crates/touring-ceg/src/gateway/decision.rs` | add `quality_score: f64` to `EvidenceBundle` |
| W4.T3 | W4.T3.4 | `crates/touring-ceg/src/gateway/decision.rs` | `composite_score(evidence)` now uses 6 signals |
| W4.T4 | W4.T4.1 | `crates/touring-ceg/src/gateway/harness_extension.rs` | document why X7 now uses 50-dim + 17-gate (best of both) |
| W4.T5 | — | `cargo test -p touring-ceg` | 0 fail, new tests pass |
| W4.T6 | — | `cargo clippy -p touring-ceg --features quality-signal -- -D warnings` | 0 warnings |

**Checkpoint**: CEG X7 bloqueia Edit/Write se `composite_score < 0.5` (Deny) ou se algum P0 dim é Fail. W_QUALITY=0.20 dá peso significativo ao sinal de qualidade real.

### W5 — Delete Deprecated Structures

**Goal**: `touring-harness` e `touring-harness-mcp` deixam de existir como crates.

| Task | Subtask | File | Acceptance |
|------|---------|------|------------|
| W5.T1 | W5.T1.1 | `Cargo.toml` (workspace) | remove `crates/touring-harness` and `crates/touring-harness-mcp` from `members` |
| W5.T2 | W5.T2.1 | `crates/touring-harness/` | DELETE entire directory |
| W5.T2 | W5.T2.2 | `crates/touring-harness-mcp/` | DELETE entire directory |
| W5.T3 | W5.T3.1 | `~/.claude/rust/target/release/touring-elite` | DELETE binary |
| W5.T3 | W5.T3.2 | `~/.claude/rust/target/release/touring-harness-mcp` | DELETE binary |
| W5.T3 | W5.T3.3 | `~/.local/bin/touring-elite` | DELETE symlink |
| W5.T3 | W5.T3.4 | `~/.local/bin/touring-harness-mcp` | DELETE symlink |
| W5.T4 | W5.T4.1 | `Cargo.lock` | regenerate via `cargo check --workspace` |
| W5.T5 | — | `cargo test --workspace` | 0 fail (all consumers migrated) |
| W5.T6 | — | `cargo clippy --workspace -- -D warnings` | 0 warnings |
| W5.T7 | — | `touring --help` | shows `quality` (not `elite`) as subcommand |

**Checkpoint**: workspace tem 1 crate a menos (de 25 para 23). Apenas 1 entry point CLI.

### W6 — Hooks + LSP Integration (C3, C4, C5, C6)

**Goal**: Hooks pré-escrevem qualidade, LSP publica diagnostics, RL reward conectado.

| Task | Subtask | File | Acceptance |
|------|---------|------|------------|
| W6.T1 | W6.T1.1 | `crates/touring-hooks/src/quality_pre_write.rs` (novo) | `PreWriteHook` que chama `touring_quality::score_target`; BLOCK se < 0.80 |
| W6.T1 | W6.T1.2 | `crates/touring-hooks/src/quality_pre_write.rs` | fail-open if touring-quality indisponível (graceful degradation) |
| W6.T1 | W6.T1.3 | `crates/touring-hooks/src/cli/post_write.rs` | re-score após write; emit `touring learning reward` se dim melhorou |
| W6.T1 | W6.T1.4 | `crates/touring-hooks/src/quality_history.rs` | append events ao `~/.claude/touring/quality-history.jsonl` |
| W6.T2 | W6.T2.1 | `crates/touring-lsp/src/quality_diagnostics.rs` (novo) | `QualityDiagnostics` adapter que converte `DimScore` → LSP `Diagnostic` |
| W6.T2 | W6.T2.2 | `crates/touring-lsp/src/server.rs` | publish diagnostics on save/change via `textDocument/publishDiagnostics` |
| W6.T3 | W6.T3.1 | `crates/touring-lsp/src/severity.rs` | map DimStatus to LSP Severity: Block→Error, Warn→Warning, Advisory→Hint |
| W6.T4 | W6.T4.1 | `~/.claude/settings.json` | register `touring quality` subcommand as shell alias for pre-write hook |
| W6.T4 | W6.T4.2 | `~/.claude/settings.json` | add `quality-history.jsonl` watcher |
| W6.T5 | — | end-to-end: write a file via Claude Code, verify pre-write hook BLOCKS at composite < 0.80 | demo: PASS |
| W6.T6 | — | end-to-end: LSP diagnostic appears in editor on save | demo: PASS |

**Checkpoint**: Hooks e LSP publicam/atendem qualidade em tempo real.

### W7 — Validation + Diamond Standard (Acceptance)

**Goal**: Todas as 50 dims atingem Diamond tier no workspace. Composite ≥ 0.95.

| Task | Subtask | Acceptance |
|------|---------|------------|
| W7.T1 | `cargo test --workspace` | 0 fail (NEW tests for gates/CLI/CEG-quality pass) |
| W7.T2 | `cargo clippy --workspace --all-targets -- -D warnings` | 0 warnings |
| W7.T3 | `touring-quality score crates/touring-analysis/src/quality/ --dims ALL --format json` | composite ≥ 0.95, tier=Diamond |
| W7.T4 | `touring-quality score crates/touring-quality/src/ --dims ALL --format json` | composite ≥ 0.95, tier=Diamond |
| W7.T5 | `touring-quality score crates/touring-ceg/src/ --dims ALL --format json` | composite ≥ 0.95, tier=Diamond |
| W7.T6 | `touring-quality check crates/touring-server/src/ --dims ALL --format json` | composite ≥ 0.95, tier=Diamond |
| W7.T7 | `touring quality history --last 20` | JSONL valid, drift detection works |
| W7.T8 | `touring-elite-aggregate.py --check` (se existir) ou equivalente | composite ≥ 0.95 |
| W7.T9 | `touring memory store "harness-consolidation-w7-diamond"` | lesson persisted |
| W7.T10 | `crates/touring-harness/.archive.md` (se necessário) | CHANGELOG entry explaining migration |

**Final Checkpoint**: 💎 Diamond em todas as 50 dims. Arquitetura consolidada. CLI unificado. Sandbox integrado.

---

## 5. Phases (TACO FASE 0..7 aplicada por wave)

Cada wave aplica o protocolo TACO:

| Fase | Descrição | Application |
|------|-----------|-------------|
| **FASE 0** | Health Gate (cargo check + touring doctor) | obrigatório no INÍCIO de cada wave |
| **FASE 1** | Scout (busca símbolos antes de criar) | obrigatório para `W1.T1.*` (mover arquivos), `W2.T1.*` (GateId enum), `W3.T1.*` (CLI commands) |
| **FASE 2** | Architect (design integration) | obrigatório para `W2.T1.*` (mapping 50→17), `W4.T3.*` (weights), `W6.T1.*` (hook design) |
| **FASE 3** | Context7 (best practices) | obrigatório para cada novo módulo (gates.rs, quality_extension.rs) |
| **FASE 4** | Decompose (DAG de subtasks) | obrigatório para waves W2, W4, W6 (que têm > 5 tasks) |
| **FASE 4.5** | Pre-impl Audit (anti-FP gate) | obrigatório para `W2.T1.*` (mapping completeness), `W4.T3.*` (weight sum = 1.0) |
| **FASE 5** | Engineers (parallel/sequential) | execução das tasks (sequencial nesta consolidação por causa de dependências) |
| **FASE 6** | Post-impl Audit | obrigatório no FIM de cada wave (cargo test, clippy, diamond check) |
| **FASE 7** | Documentation | CHANGELOG.md + CLAUDE.md update (se necessário) + lesson em `touring memory store` |

---

## 6. Tasks (Inventário Completo)

**Total: 32 tasks atômicas distribuídas em 7 waves** (média 4-5 tasks por wave).

| Wave | Tasks | Subtasks | Tipo |
|------|-------|----------|------|
| W0 | 5 | 0 | Preparation baseline |
| W1 | 5 | 8 | Foundation migration (Change/History/Report/Runner) |
| W2 | 6 | 22 | gates.rs + composite refactor + 14 stubs deleted |
| W3 | 7 | 12 | CLI unification + 5 tools migration |
| W4 | 6 | 8 | CEG X7 6th signal |
| W5 | 7 | 5 | Delete deprecated crates |
| W6 | 6 | 9 | Hooks + LSP integration |
| W7 | 10 | 0 | Validation + Diamond acceptance |
| **Total** | **52** | **64** | |

**Detalhamento de cada task**: ver Seção 4 (Waves) com sub-tasks granulares.

---

## 7. Subtasks (Detalhamento)

Para节省 espaço, os subtasks já estão listados inline na Seção 4 (tabela por wave).

**Princípio de granularidade**: cada subtask é uma **única Edit/Write/Command** executável em ≤ 15 minutos. Tasks são unidades de acceptance; subtasks são unidades de execução.

---

## 8. QuickAction (Primeiras 5 Ações — ≤ 30 min)

```
┌──────────────────────────────────────────────────────────────────────────┐
│  QUICKACTION: Start W0 NOW                                              │
│                                                                          │
│  Q1. touring doctor -j  → confirmar 6/6 ok                              │
│  Q2. cargo test --workspace  → 0 fail (baseline)                       │
│  Q3. cargo clippy --workspace -- -D warnings  → 0 warnings              │
│  Q4. touring-quality score crates/touring-analysis/src/quality/ \         │
│        --dims ALL --format json  → composite ≥ 0.80 (baseline)         │
│  Q5. touring memory store "harness-consolidation-w0-baseline" \          │
│        "<composite scores snapshot>" --tier semantic                    │
│                                                                          │
│  ✅ Acceptance: 5/5 success → W1 may begin                              │
└──────────────────────────────────────────────────────────────────────────┘
```

**Tempo estimado**: 15-25 min (depende de cache sccache).

---

## 9. CLI Specification — `touring quality` (15 subcomandos)

```bash
touring quality
├── check <PATH>           # Run all 50 dims on a target (or workspace if --workspace)
│   ├── --format human|json|toon|badge  # Output format
│   ├── --dims F1.1,F2.4,...  # Subset of dims (default: all 50)
│   ├── --workspace           # Score the whole workspace
│   ├── --fail-below 0.80     # Exit 1 if composite < threshold
│   └── --parallel N          # Concurrency (default: num_cpus)
│
├── score <PATH>           # Alias for `check` with --format json (machine-readable)
│   └── (same flags as check)
│
├── gate <PATH>            # 17-gate rollup (uses gates::aggregate_to_gates)
│   ├── --gate ARCHITECTURE  # Single gate filter
│   ├── --format human|json
│   └── --explain            # Show which 50 dims contribute to each gate
│
├── gates <PATH>           # List 17 gates with their contributing dims
│   ├── --format human|json
│   └── --explain            # Same as gate --explain (alias for compatibility)
│
├── list                   # List all 50 dims with descriptions
│   ├── --phase 1|2|3|4      # Filter by phase
│   ├── --block              # Only BLOCK dims (P0)
│   └── --format human|json
│
├── explain <F2.4>         # Explain a dim or gate (what it detects, why)
│   ├── --format human|json
│   └── --example            # Include code example
│
├── history [--last N] [--drift]   # Unified score history (Q7)
│   ├── --drift              # Show drift detection
│   ├── --since TIMESTAMP    # Filter by date
│   └── --format human|json
│
├── report <PATH>          # Emit full report (Human/Json/Toon/Badge)
│   ├── --format human|json|toon|badge
│   ├── --output FILE        # Write to file (default: stdout)
│   └── --include-evidence   # Include evidence (slower, larger)
│
├── compare <CHANGE_A> <CHANGE_B>   # Compare two changes
│   ├── --format human|json
│   └── --include-evidence
│
├── baseline <NAME>        # Save current composite as baseline
│   ├── --note "..."        # Note for the baseline
│   └── --format human|json
│
├── drift [--from BASELINE] # Show evolution drift vs baseline
│   ├── --alert none|degraded|structural
│   └── --format human|json
│
├── benchmark               # Run micro-benchmarks on the 50-dim engines
│   ├── --target PATH       # File to benchmark
│   ├── --dims F1.1,F2.4
│   └── --iterations N
│
├── fix <F2.4> <PATH>       # Auto-remediate a dim (run generator)
│   ├── --dry-run
│   └── --format human|json
│
├── register --agent-id X [--model Y]  # Register an LLM agent in history
│
└── status                 # Diagnostics: composite, last run, history size
    └── --format human|json
```

**Exemplos de uso**:

```bash
# Check a single file
$ touring quality check src/lib.rs --format badge
💎 Diamond 0.984

# Check whole workspace
$ touring quality check . --workspace --fail-below 0.80
...

# Single dim check
$ touring quality check src/lib.rs --dims F2.4 --format human
✗ F2.4: 0.500 Cryptographic Issues: secret-related keyword present (no assigned value)
  → suggests comment-line filter improvement (already implemented 2026-06-24)

# 17-gate rollup
$ touring quality gate crates/touring-analysis/src/quality/
✓ Security 0.962 (F2.1=0.98, F2.2=1.0, F2.3=0.85, F2.4=1.0, F2.5=1.0, F2.6=0.94)
✓ Architecture 0.974 (F1.7=0.96, F1.8=0.97, F1.9=0.99, ...)
...

# History with drift
$ touring quality history --last 5 --drift
2026-06-25 10:30 composite=0.984 Diamond
2026-06-25 09:00 composite=0.972 Diamond
2026-06-24 18:44 composite=0.957 Diamond (drift: +0.025)
...

# Explain a dim
$ touring quality explain F2.4
F2.4 — Cryptographic Issues / Hardcoded Secrets
...
```

---

## 10. Integration Map (AST + Graph + LSP + Hooks)

### 10.1 AST Integration (touring-ast-polyglot)

| Engine layer | AST augmentation |
|--------------|------------------|
| F1.1 complexity | syn-based AST; counts match arms, generics |
| F2.1 OWASP | AST pattern matching for `format!()`, `std::fs`, `Command::new` |
| F2.4 secrets | regex + AST tokenization (no string-in-string) |
| F2.11 concurrency | AST: `async fn` body, lock scope detection |
| F4.1 idioms | AST: `let-else`, `if let chains`, `format!` inline args |

**Plan**:
- Each `analyze_X` accepts optional `ast_context: Option<&AstContext>`
- If `ast_context` is Some, engine uses AST-precise detection; else falls back to text
- `touring-ast-polyglot` exposes `AstContext::from_source(source, lang)`

### 10.2 Graph Integration (touring-analysis/wiring)

| Engine layer | Graph signal |
|--------------|--------------|
| F1.7 boundaries | `pub_surface` count from graph |
| F1.8 dep_cycles | Tarjan SCC from graph (already wired) |
| F1.12 arch_consistency | `layer_violation_count` from graph |
| F2.5 dep_cves | cargo-deny + graph (resolve path) |

**Plan**: `touring-quality::gates::aggregate_to_gates` aceita `graph: Option<&Graph>` para graph-augmented aggregation.

### 10.3 LSP Integration (touring-lsp)

```
┌─────────────────────────────────────────────────────────────┐
│ Editor (VSCode/Neovim)                                      │
│   │                                                         │
│   ├─ textDocument/didOpen → touring-lsp server               │
│   ├─ textDocument/didChange → re-score, publish diagnostics │
│   └─ workspace/diagnostic → show in Problems panel          │
│                                                             │
│ touring-lsp/src/quality_diagnostics.rs                       │
│   ├─ QualityDiagnostics::from_dim_score(score) → LSP[Diag]  │
│   └─ severity_map: Block→Error, Warn→Warning, Adv→Hint       │
│                                                             │
│ LSP methods:                                                 │
│   - textDocument/publishDiagnostics  (notification)         │
│   - workspace/executeCommand: "touring.quality.check"        │
└─────────────────────────────────────────────────────────────┘
```

**Plan**:
- `crates/touring-lsp/src/quality_diagnostics.rs` adapter
- Maps 50 dims → LSP diagnostics
- Severity mapping in `severity.rs`

### 10.4 Hooks Integration (touring-hooks)

| Hook | Stage | Action |
|------|-------|--------|
| **pre-write** | Before write | `touring-quality score <path>`; BLOCK if composite < 0.80 |
| **post-write** | After write | re-score; if improved, `touring learning reward` |
| **pre-edit** | Before edit | pre-flight score (informational, not blocking) |
| **post-edit** | After edit | re-score; append to history |
| **post-tool-rl** | After any tool | if dim improved, emit reward |
| **session-start** | On start | load last composite from history |
| **session-stop** | On stop | flush history to JSONL |

**Pre-write hook (BLOCK-by-default)**:

```rust
fn pre_write_hook(target_path: &Path) -> HookDecision {
    let score = touring_quality::score_target(target_path)?;
    if score.composite < 0.80 {
        HookDecision::Block {
            canonical_fix: format!(
                "Composite {:.3} < Gold (0.80). Dims failing: {:?}",
                score.composite, score.failing_dims()
            ),
        }
    } else {
        HookDecision::Allow
    }
}
```

---

## 11. 50-dim Diamond Standard (Acceptance Criteria)

Para o acceptance W7, todas as 50 dims devem atingir **Diamond tier** (≥ 0.95) no workspace consolidado. Critério explícito por tier:

| Tier | Range | Mandatory for |
|------|-------|---------------|
| 💎 **Diamond** | ≥ 0.95 | engines in `touring-analysis/src/quality/` (the canonical detectors) |
| 💎 **Diamond** | ≥ 0.95 | wrappers in `touring-quality/src/verifications/` (all 50) |
| 💎 **Diamond** | ≥ 0.95 | `gates.rs` (the 50→17 rollup) |
| 💎 **Diamond** | ≥ 0.95 | `touring-quality/src/lib.rs` (the public API) |
| 💎 **Diamond** | ≥ 0.95 | `touring-quality/src/bin/touring-quality.rs` (the CLI binary) |
| 💎 **Diamond** | ≥ 0.95 | `touring-ceg/src/gateway/decision.rs` (the X7 composite) |

**Per-dim diamond checklist** (each of 50 dims must show):
- ✓ `is_detector_own_source()` allowlist
- ✓ Real engine wired (not stub)
- ✓ Tests passing (≥ 2 per dim: positive + negative case)
- ✓ Composite score ≥ 0.95 in workspace
- ✓ Evidence string mentions `touring-analysis` (real engine, not fallback)

**P0 BLOCK fail-closed** (6 dims: F2.1, F2.4, F2.5, F2.6, F4.3, F4.5):
- ✓ PreToolUse hook BLOCKS writes that fail this dim
- ✓ 0 secret/CVE in workspace
- ✓ cargo-deny / cargo-audit pass

---

## 12. Riscos & Mitigações

| # | Risco | Severidade | Mitigação |
|---|-------|------------|-----------|
| R1 | **Mudança de score durante transição**: Composite muda de 17-gate (com stubs=1.0) para 50-dim (real). Releases Gold hoje podem virar Silver | 🔴 HIGH | Flag `--harness-backend=50dim|17gate` opt-in por 1 wave. Default = 17-gate (compatibility). Rollout gradual. |
| R2 | **Regressão em test coverage**: testes do harness usam stubs. Migrar para 50-dim quebra | 🟡 MED | Rodar `cargo test --workspace` antes de cada mudança. Mapear 1-a-1 testes existentes. |
| R3 | **Dependência circular**: `touring-quality` precisa de `GateId` (de harness) e harness precisa consumir quality | 🟡 MED | **Mover `GateId` para `touring-quality/src/gates.rs`**. harness re-exporta. |
| R4 | **Performance regression**: composite de 50 dim pode ser lento (50 verifiers × N files) | 🟢 LOW | Aggregate é O(files). Composite é O(50). Nada caro. |
| R5 | **`touring-harness-mcp` quebrar clientes**: clientes MCP perdem tools se movermos | 🟡 MED | Manter 1 wave como deprecation shim que delega a `touring-server` |
| R6 | **CEG X7 weight change**: novos pesos W_QUALITY=0.20 mudam comportamento de decisão | 🟡 MED | Documentar em changelog. Rodar E2E suite para validar BLOCK-by-default. |
| R7 | **Rollback problemático**: depois de W5 (delete crates), rollback requer git revert | 🟢 LOW | Wave 5 é ÚLTIMO passo. Até então, 100% reversível. |
| R8 | **Composite scoring change pode revelar bugs latentes**: novos signals podem flagar issues antes invisíveis | 🟡 MED | Tratar como achados (REGRA #21 — zero tolerância a falhas). Corrigir antes de declarar wave done. |
| R9 | **`touring quality` subcommand conflita com naming**: pode haver subcommand `quality` já | 🟢 LOW | Verificar com `touring --help` baseline (W0). Se houver, renomear (ex: `touring quality-check`). |

---

## 13. Context7 References

Bibliotecas consultadas (best practices):

| Library | Path | Usado em |
|---------|------|----------|
| **SonarQube** (consolidated scanner) | `/websites/sonarsource_sonarqube-community-build` | Padrão de single CLI com `--token` + `--projectKey`. Sub-comandos quality-gate. |
| **CodeQL** (data flow + taint) | `/github/codeql` | Padrão de composable modules (DataFlow::ConfigSig, TaintTracking::Global). Aplicado a `gates::aggregate_to_gates` config signature. |
| **CodeQL standard libs** | `/github/codeql` | Padrão de source/sink modeling. Aplicado a `quality_extension::quality_signal_score` (X7 6th signal). |
| **OWASP CheatSheets** | `/owasp/cheatsheetseries` | F2.1 OWASP + F2.4 secrets + F2.2 input validation best practices |
| **gitleaks** | `/gitleaks/gitleaks` | F2.4 secrets detection patterns (keyword prefilter + entropy) |
| **clippy + rust-api-guidelines** | `/rust-lang/rust-clippy`, `/rust-lang/api-guidelines` | F4.1 idioms, F4.3 deprecated, F4.4 modernization |
| **proptest + cargo-fuzz** | `/proptest-rs/proptest`, `/rust-fuzz/cargo-fuzz` | F3.4 edge cases + property-based testing |
| **tracing + OpenTelemetry** | `/open-telemetry/opentelemetry-rust`, tracing | F4.10 monitoring + observability |

---

## 14. Acceptance Final do Plano

| Item | Acceptance |
|------|------------|
| **1 unified harness** | `touring-quality` é a casa. `touring-harness` e `touring-harness-mcp` deletados. |
| **50 dim engines** | 50 engines reais em `touring-analysis/src/quality/`. tp/fp-validado. |
| **17 gates via rollup** | `touring-quality/src/gates.rs::aggregate_to_gates` mapeia 50→17. |
| **Single composite** | 50-dim weighted avg (Q4). Sem paralelo. |
| **Unified CLI** | `touring quality <sub>` com 15 subcomandos. |
| **MCP surface** | 5 tools em `touring-server`. `touring-harness-mcp` deletado. |
| **CEG X7 integration** | W_QUALITY=0.20. 6 sinais. Fail-closed. |
| **Hooks** | pre-write BLOCK < 0.80. post-write history append. |
| **LSP** | diagnostics por dim. Severity mapping. |
| **Score history** | `~/.claude/touring/quality-history.jsonl` (Q7). |
| **Diamond tier** | 50/50 dims em Diamond no workspace. |
| **Tests** | 0 fail. 0 warnings. 0 BLOCK violations. |

---

## 15. QuickAction Card (imprimível)

```
╔══════════════════════════════════════════════════════════════════╗
║  HARNESS CONSOLIDATION — MASTER PLAN                            ║
║  Goal: 1 unified harness. 50 dim. 17 gates. Diamond tier.      ║
║  Status: ⏸️ PLANEJAMENTO (nenhuma modificação ainda)            ║
╠══════════════════════════════════════════════════════════════════╣
║  Decisions locked: Q1=B · Q2=A · Q3=A · Q4=B · Q5=A · Q6=A · Q7=C ║
║  7 waves. 52 tasks. 64 subtasks. ~7 dias.                        ║
╠══════════════════════════════════════════════════════════════════╣
║  W0 (NOW): baseline (5 tasks, 15-25 min)                        ║
║  W1: foundation migration (Change/History/Report → quality)    ║
║  W2: gates.rs + 14 stubs deleted + single composite             ║
║  W3: `touring quality` CLI + 5 tools → touring-server            ║
║  W4: CEG X7 W_QUALITY=0.20                                     ║
║  W5: delete touring-harness + touring-harness-mcp              ║
║  W6: hooks + LSP integration                                    ║
║  W7: 50/50 Diamond acceptance                                   ║
╠══════════════════════════════════════════════════════════════════╣
║  Pre-flight:  cargo check --workspace && touring doctor -j      ║
║  Acceptance: cargo test --workspace + clippy + diamond score   ║
║  Quality:    50 dims Diamond. P0 BLOCK fail-closed.             ║
╚══════════════════════════════════════════════════════════════════╝
```

---

**Aguardando aprovação de Gabriel** para iniciar W0 (QuickAction).
