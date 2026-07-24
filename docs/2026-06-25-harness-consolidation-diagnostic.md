# Diagnóstico Exaustivo — Consolidação das 5 Estruturas de "Harness" no Touring

**Data**: 2026-06-25
**Autor**: TACO (Touring Agentic Code Orchestrator)
**Escopo**: Análise read-only das 5 estruturas que disputam o papel de "code quality harness" no workspace `~/.claude/rust/`. Nenhum código foi modificado; nenhuma estrutura foi renomeada, deletada ou mesclada. **Nenhuma ação será tomada sem aprovação explícita de Gabriel.**
**Trigger**: O gap do stub-vs-real F4.x (engines criados mas wrappers ainda como W3/W4 heuristic) passou despercebido pelo "harness" — sinal de que **a integração entre as estruturas não está acontecendo na prática**.

---

## TL;DR (Sumário Executivo)

| # | Estrutura | LOC | Papel real | Integra com quem? | Status |
|---|-----------|-----|-----------|-------------------|--------|
| 1 | `crates/touring-analysis/src/quality/` | 55 .rs (~5.500 LOC engine + tests) | **Engine layer** — 50 dim detectores reais (memchr / non_executable_regions / polyglot) | Consumido APENAS por `touring-quality` | ✅ Real e testado (950+ tests passing) |
| 2 | `crates/touring-quality/` | 52 .rs (~11.000 LOC) | **Verifier layer** — 50 wrappers + composite + tier + scope aggregation | Importa `touring-analysis` via feature `workspace-integration` (default-on). **NÃO** consulta `touring-harness` nem `touring-ceg`. | ✅ Real e testado (300+ tests passing após fix F2.4 FP de hoje) |
| 3 | `crates/touring-harness/` | 25 .rs (~2.700 LOC) | **Change aggregator** — 17 gates + Change tracking + Score history + Report | CEG importa (via `touring-harness::run_harness` em `harness_extension.rs`). **NÃO** consulta `touring-quality`. | ⚠️ Parcial — apenas 3/17 gates têm implementação real; 14/17 são **stubs `External`** |
| 4 | `crates/touring-harness-mcp/` | 1 .rs (434 LOC) | **MCP server wrapper** — 5 tools JSON-RPC stdio sobre `touring-harness` | Depende apenas de `touring-harness`. **NÃO** está no `touring-server` (que NÃO tem `touring_elite_*` tools). | 🟡 Funciona, mas é **crate separado** — duplicação parcial com `touring-server` |
| 5 | `crates/touring-ceg/` | 36 .rs (~13.000 LOC) | **Sandbox/pre-exec** — X0..X9 typestate pipeline + capability model + landlock | `touring-harness::run_harness` chamado por `harness_extension.rs::harness_block_for_tool`. **NÃO** consulta `touring-quality` (50-dim composite). | ✅ Real; isolado por design (sandbox ≠ análise) |

**Diagnóstico em uma frase**: existem **dois composite scorers paralelos** (17-gate em `touring-harness` vs 50-dim em `touring-quality`) que **não se conversam**, e o **sandbox CEG** consulta o de 17-gates — não o de 50-dim, que tem detecção real e tp/fp-validada.

**Achado crítico #0**: `touring-harness/src/lib.rs` documenta que `touring elite {check, gate, badge, ...}` é o entry point CLI. **Esse subcommand não existe.** O binário correto é `touring-elite` (separado, no workspace) e a CLI principal (`touring`) retorna `Unknown subcommand: elite`.

---

## 1. Inventário Detalhado

### 1.1 `crates/touring-analysis/src/quality/` — Engine Layer (50 dim)

**Cargo.toml** (`crates/touring-analysis/Cargo.toml`):
- `petgraph`, `rusqlite`, `rayon`, `serde`, `tracing`, `memchr`, `aho-corasick`, `stringzilla`, `moka`, `rustsec`, `toml`, `glob`
- Features: `blast-radius`, `quality`, `wiring`, `temporal`, `simd-temporal`, `ann-blast`, `deep`, `simd-wiring`, `simd-temporal-ac`, `erickson-bridge`
- Default = todas

**Conteúdo** (55 .rs files, ~5.500 LOC de engines + tests):
- **F1.1–F1.12** (Code Quality & Architecture): complexity, maintainability, duplication, solid, tech_debt, error_handling, boundaries, dep_cycles, api_design, data_model, patterns, arch_consistency
- **F2.1–F2.13** (Security & Performance): owasp, input_validation, authz, **secrets** (827 LOC — com detector de entropy, gitleaks-style keywords, comment-line filter recém-adicionado), dep_cves, config, db_perf, memory, caching, io, concurrency, frontend, scalability
- **F3.1–F3.13** (Testing & Documentation): coverage, test_quality, test_pyramid, edge_cases, test_maint, sec_tests, perf_tests, inline_doc, api_doc, arch_doc, readme, doc_accuracy, changelog
- **F4.1–F4.12** (Best Practices & CI/CD): idioms, **frameworks** (D41), deprecated, modernization, pkg_mgmt, **build_config** (D46, polyglot Rust+Python+JS/TS+Go), **cicd** (D47), **deploy** (D48), **iac** (D49, Docker/Terraform), **monitoring** (D50), **incident** (D51), **env** (D52)

**Pontos fortes**:
- **Real implementation** — cada engine tem seu próprio conjunto de testes internos (`#[test]` dentro de cada arquivo) com **950+ tests passing** no `touring-analysis`.
- **Polyglot** — engines aceitam `source: &str, lang: &str` e tratam Rust/Python/JS/TS/Go/YAML/TOML/JSON/MD.
- **Self-match prevention** — cada engine tem `is_detector_own_source()` (allowlist) para evitar FP ao escanear a si mesmo.
- **Performance** — usa `memchr` + `non_executable_regions` (suprime comentários e test bodies) para varredura linear.
- **`density_score` compartilhado** — `super::score_utils::density_score(weighted_total, total_lines, SCALE)` substitui a fórmula `1 - density*SCALE` ad-hoc em 13+ engines (DRY real).

**Pontos fracos**:
- **Não tem comando CLI próprio** — só é consumível como crate library.
- **API ainda em transição** — os módulos `mod.rs` adicionaram `pub use` para os F4.x recentemente (gap que pegamos hoje: 8 engines sem `pub use` impediam o `pub use ... as score_X` dos wrappers).

### 1.2 `crates/touring-quality/` — Verifier Layer (50 dim → score)

**Cargo.toml** (`crates/touring-quality/Cargo.toml`):
- `serde`, `serde_json`, `anyhow`, `clap`, `tera`, `tokio`, `rayon`, `tracing`, `tracing-subscriber`
- `touring-analysis = { path = "../touring-analysis", optional = true }`
- **Feature flag** `workspace-integration = ["dep:touring-analysis"]` — **default-on**.
- Standalone fallback: `--no-default-features` → cada wrapper vira heurística substring-density.

**Conteúdo** (52 .rs, ~11.000 LOC):
- `lib.rs` (851 LOC) — define `DimId` enum com 50 variantes (F1_1..F4_12), `Enforcement` (Block/Warn/Advisory), `DimStatus`, `DimScore`, `QualityReport`
- `composite.rs` (146 LOC) — `compute_composite(scores, weights)` (média ponderada), `default_weights()` (P0=2.0, P1=1.5, P2=1.0)
- `tier.rs` (123 LOC) — `Tier::from_composite` (Diamond ≥0.95, Platinum ≥0.90, Gold ≥0.80, Silver ≥0.70, Bronze ≥0.60, Unranked <0.60)
- `aggregate.rs` (275 LOC) — `AggKind` enum (WorstOf / WeightedLoc / CoverageRatio / ScopeNative / Mean) — base da agregação multi-scope que **fechou o gap do `--workspace unfaithful`** (multiscope Wave 2026-06-21).
- `scope.rs` (323 LOC) + `scope_report.rs` (276 LOC) — orquestração de score por escopo (arquivo / crate / workspace)
- `verifications/mod.rs` (552 LOC) — `Verification` trait + 50 wrappers (1 por dim)
- `verifications/f*_*.rs` (50 files) — cada wrapper tem 2 modos:
  - `#[cfg(feature = "workspace-integration")]` → chama `touring_analysis::quality::{analyze_X, score_X}` (engine real)
  - `#[cfg(not(feature = "workspace-integration"))]` → heurística substring
- `bin/touring-quality.rs` (321 LOC) — CLI binário: `touring-quality score <TARGET>`, `check --gate <F2.1>`, `list`

**Pontos fortes**:
- **300+ tests passing** (verificado hoje após fix F2.4 FP).
- **Composite scoring é o algoritmo certo**: Block=2.0, Warn=1.5, Advisory=1.0 → P0 dims puxam a média para baixo quando falham.
- **Multi-scope aggregation** (Wave 2026-06-21) — fecha o gap histórico do `--workspace` que somava CC e deixava arquivos >2MiB passarem de BLOCK.
- **6 BLOCK dims** (P0) — fail-closed pré-Write: F2.1 OWASP, F2.4 Secrets, F2.5 CVEs, F2.6 Config, F4.3 Deprecated, F4.5 Pkg-mgmt.
- **Self-match prevention** — cada wrapper tem `is_detector_own_source(target)` (mesmo padrão da layer 1).

**Pontos fracos**:
- **NÃO conhece o conceito de "gate"** — só fala em "dim" (50) e "tier" (6). Se alguém quer agregar 50 dims em 17 gates hierárquicos, **precisa escrever a função do zero**.
- **NÃO tem Change concept** — scoring é per-file/per-scope; não tem "antes vs depois" como o harness.
- **NÃO tem Score history** — score é stateless (cada run é fresh).

### 1.3 `crates/touring-harness/` — Change Aggregator (17 gates)

**Cargo.toml** (`crates/touring-harness/Cargo.toml`):
- `serde`, `serde_json`, `rayon`, `chrono`, `thiserror`, `anyhow`, `clap`
- **Sem dependência de `touring-quality` ou `touring-analysis`** (verificado via grep `use touring_quality` / `use touring_analysis` → ambos retornam 0 matches).

**Conteúdo** (25 .rs, ~2.700 LOC):
- `lib.rs` (96 LOC) — define pub API: `Change`, `ProposedFile`, `FileKind`, `Gate`, `GateId`, `GateOutcome`, `GateSeverity`, `GateStatus`, `HarnessConfig`, `run_harness`, `EliteScore`, `EliteTier`, `tier_for`, `emit_report`, `ReportFormat`, `ScoreHistory`, `DriftReport`
- `gate.rs` (336 LOC) — `GateId` enum com 17 variantes (CodeQuality, Architecture, Security, Performance, Testing, Documentation, BestPractices, CiCdDevops, Modularization, Scalability, Extensibility, Naming, Navigability, Craftsmanship, Dependencies, Ux, ProductDocs). Cada gate tem `default_weight()` (Security/Dependencies = 1.5, resto = 1.0 ou default).
- `score.rs` (296 LOC) — `EliteScore { composite: f32, tier: EliteTier }`, `EliteTier` enum (Diamond/Platinum/Gold/Silver/Bronze/Unranked). Composite = Σ(weight × score) / Σ(weight).
- `runner.rs` (224 LOC) — `run_harness(change, gates, cfg)` executa as gates em paralelo via rayon, captura pânico como `Advisory` (fail-open default).
- `change.rs` (225 LOC) — `Change` (agent, model, files), `ProposedFile { path, before, after, kind }`, `FileKind` (Create/Modify/Delete).
- `history.rs` (307 LOC) — `ScoreHistory` (JSONL append-only em `~/.claude/touring/elite-history.jsonl`), drift detection.
- `report.rs` (216 LOC) — `emit_report` em 4 formatos: Human, Json, Toon, Badge.
- `bin/touring-elite.rs` (306 LOC) — binário CLI separado (`touring-elite`):
  - `touring-elite check` — roda 17 gates
  - `touring-elite gate <slug>` — uma gate só
  - `touring-elite badge` — só o badge tier
  - `touring-elite report` — relatório detalhado
  - `touring-elite history [--last-n N]` — audit trail
  - `touring-elite register --agent-id X` — registra agente (no-op hoje)
- `builtins/` (17 .rs):
  - **3 REAL**: `architecture.rs` (cycles + blast radius heurístico), `security.rs` (deny.toml advisory count), `modularization.rs` (file sizes)
  - **14 STUB**: `code_quality.rs`, `performance.rs`, `testing.rs`, `documentation.rs`, `best_practices.rs`, `ci_cd_devops.rs`, `scalability.rs`, `extensibility.rs`, `naming.rs`, `navigability.rs`, `craftsmanship.rs`, `dependencies.rs`, `ux.rs`, `product_docs.rs` — todos retornam `External` com mensagem "external CI step (assumed PASS)" e score 1.0.
  - `stub.rs` (42 LOC) — helper `external_advisory(id, severity, msg)` para os 14 stubs.

**Pontos fortes**:
- **Change tracking** — `Change` representa um diff (before/after) que 50-dim per-file **não captura**.
- **History** — JSONL append-only permite trend analysis + drift detection ao longo do tempo.
- **17 gates cobrem categorias mais amplas** que 50 dims (ex: "Scalability" agrega F2.13 + F2.11 + F2.8 + F2.9, que é uma visão de "horizontal scalability" — não é uma dim só).
- **Integração CEG X7 já existe** — `touring-ceg/src/gateway/harness_extension.rs::harness_block_for_tool` chama `touring_harness::run_harness` para Edit/Write/MultiEdit/NotebookEdit e BLOCK if `tier < Gold`.

**Pontos fracos**:
- **14/17 gates são stubs** — esses 14 retornam `GateStatus::External` com score 1.0 (assumed PASS). Na prática, o `composite_score` está sendo puxado por apenas **3 gates reais** (Architecture + Security + Modularization). **Esse é o motivo central pelo qual o gap do F4.x stub passou despercebido** — o harness está tão "vazio" que composite_score não tem como flagar qualidade do código real.
- **NÃO consulta `touring-quality`** — duas implementações paralelas de composite scoring existem no mesmo workspace, sem se comunicarem.
- **NÃO tem detecção por linguagem** — gates "Testing" / "Documentation" / "BestPractices" não rodam análise real; assumem que CI externa faz isso.
- **`run_harness` roda Change vazia como sintético** (default) — em vez de detectar os arquivos modificados pelo tool call real.

### 1.4 `crates/touring-harness-mcp/` — MCP Server Wrapper (5 tools)

**Cargo.toml** (`crates/touring-harness-mcp/Cargo.toml`):
- `touring-harness = { path = "../touring-harness" }`
- `serde`, `serde_json`, `anyhow`, `tokio`, `rmcp` (com features `["server", "transport-io", "macros", "schemars"]`)
- `schemars`, `chrono`

**Conteúdo** (1 .rs, 434 LOC):
- `main.rs`:
  - **JSON-RPC 2.0 manual** (não usa `#[tool]` macros) — stdin/stdout line-delimited
  - 5 tools expostos via `tools/list`:
    | Tool name | Description |
    |---|---|
    | `touring_elite_check` | Run all 17 gates on a proposed code change. Use before any Edit/Write to BLOCK changes below Gold tier (composite < 0.80). |
    | `touring_elite_gate` | Run a single gate by its ID slug (e.g. 'architecture', 'security') and return its outcome. |
    | `touring_elite_badge` | Compute and return the Diamond/Platinum/Gold/Silver/Bronze/Unranked badge for a synthetic change. |
    | `touring_elite_register` | Register an LLM/agent identifier in the score-history audit trail. |
    | `touring_elite_history` | Return the last N entries from the score-history JSONL (most recent first) and a drift-detection report. |
  - `tool_check`: lê arquivo, monta `Change::Modify`, chama `run_harness` com `default_gates()`.
  - `tool_gate`: roteia `gate_id` slug → `GateId` enum, executa gate específica, retorna outcome.

**Pontos fortes**:
- **Funciona** — testado via stdin (`echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | touring-harness-mcp` retorna os 5 tools corretamente).
- **Falha clean** — comentários indicam: "manual JSON-RPC, no macro dependency on `rmcp::serve` which has signature instability" → escolha defensiva para evitar drift de macro.
- **Fail-loud** — `RBP-01 elite-lint ratchet` (`#![cfg_attr(not(test), deny(clippy::unwrap_used))]`) aplicado.

**Pontos fracos**:
- **NÃO está integrado a `touring-server`** — verificado via grep: `crates/touring-server/src/` **não tem** nenhuma referência a `touring_elite_*` tools nem a `touring_harness`. Isso significa que **qualquer cliente MCP que fala com `touring-server` NÃO tem acesso ao harness**.
- **Duplica infra** — `touring-server` tem seu próprio catálogo de tools (90+ tools); o `touring-harness-mcp` é um daemon MCP separado que precisa ser registrado separadamente no `settings.json` do Claude Code.
- **Não usa `score_target` do `touring-quality`** — quando chama `run_harness`, vai direto ao `touring-harness` (17 gates stub-dominant). Perde a chance de usar o engine real de 50 dim.
- **Mantido como crate separado** quando poderia ser um módulo de `touring-server` — `touring-server/src/cli/harness_metric.rs` JÁ existe e tem o conceito, mas só consome `touring-harness-metric` (outro conceito, derivado de `touring-quality`).

### 1.5 `crates/touring-ceg/` — Sandbox/Pre-Exec Gateway (X0..X9)

**Cargo.toml** (`crates/touring-ceg/Cargo.toml`):
- `touring-contracts`, `touring-foundation`, `touring-hooks-shared`, `touring-offensive`, `touring-ast-polyglot`
- **`touring-harness = { path = "../touring-harness" }`** — feature `H1-B (2026-06-13): CEG X7 DECISION extension — BLOCK-by-default em Edit/Write se EliteScore.tier < Gold. Adds `touring_harness::run_harness` to the X7 DECISION pipeline.`
- `serde`, `serde_json`, `thiserror`, `tracing`, `regex`, `tokio`, `moka`, `blake3`

**Conteúdo** (36 .rs, ~13.000 LOC):
- `gateway/`:
  - `pre_exec.rs` (985 LOC) — `run_gateway(deps) -> GatewayOutcome` driver
  - `typestate.rs` (894 LOC) — `Execution<S>` typestate pipeline (X0..X7 enforced at compile time)
  - `decision.rs` (1004 LOC) — **X7 DECISION**: `composite_score(evidence) -> f64` pondera 5 sinais:
    - `W_STATIC = 0.25` (X2 STATIC — análise estática de forbidden patterns)
    - `W_VGP = 0.15` (X3 VGP — verifica símbolos via `touring index find`)
    - `W_PREDICT = 0.15` (X4 PREDICT — speculative prediction de outcome)
    - `W_SANDBOX = 0.20` (X5 SANDBOX — dry-run em CapabilityProfile)
    - `W_GATE = 0.25` (X6 CAPABILITY-GATE — Deno-style deny-by-default)
    - `EvidenceBundle` (non-terminal) — decompõe os 5 scores individuais (§5.2.2 de "Code as Agent Harness" arXiv 2605.18747)
    - `Verdict` = Allow / Warn / Deny + `GateDecision` + `canonical_fix`
  - `harness_extension.rs` (178 LOC) — **integração com touring-harness**:
    ```rust
    pub fn harness_block_for_tool(tool, payload, agent_id, model) -> HarnessVerdict {
        // ... parse Change, run_harness(&change, &default_gates(), &cfg) ...
        if score.is_release_ready() { Allow { composite, tier, badge } }
        else { Deny { composite, tier, canonical_fix } }
    }
    ```
    - BLOCK-by-default para Edit / Write / MultiEdit / NotebookEdit
    - Fail-open: payload unparseable → PassThrough (não bloqueia)
  - `learn.rs` (774 LOC), `outcome_learner.rs` (894 LOC), `drift_corrector.rs` (310 LOC) — X9 LEARN pipeline
  - `sandbox_executor.rs` (1429 LOC) — kernel-enforced sandbox (landlock V1-V6)
  - `capability_class.rs` (685 LOC), `predict.rs` (487 LOC), `vgp_stage.rs` (253 LOC), `static_stage.rs` (270 LOC), `classify.rs` (265 LOC), `capture.rs` (175 LOC) — X0..X6 stages
  - `metrics.rs` (391 LOC) — counter observability (`record_ceg_captured_count`, `record_verdict_counters`)
- `capability/` (134-619 LOC) — Deno-style capability model, 4 built-in profiles (ReadOnly / StagedWrite / Trusted / Sandboxed), landlock + rlimit enforcement

**Pontos fortes**:
- **Isolamento por design** — sandbox é um concern separado (não é "qualidade de código" mas "segurança de execução"). Justifica manter como crate separado.
- **X0..X9 typestate enforced em compile time** — X3 VGP e X5 SANDBOX são estruturalmente unskippable.
- **Tem integration com harness (parcial)** — `harness_extension.rs` chama `touring_harness::run_harness` para Edit/Write. É o único ponto onde CEG consome um "harness".

**Pontos fracos**:
- **NÃO consulta `touring-quality`** — o `composite_score` em X7 usa 5 sinais internos (static, vgp, predict, sandbox, gate). **Falta o sinal "qualidade real do código"** (F1-F4). Para o exemplo concreto do gap de hoje (F4.x wrappers eram stubs), o CEG X7 **não detectou** que Edit/Write para `crates/touring-quality/src/verifications/f4_*.rs` era "downgrade de real para stub" porque:
  1. `run_harness` (chamado pelo CEG) retorna `tier = Gold` ou superior baseado nos 3 gates reais (Architecture/Security/Modularization) + 14 stubs retornando `External / score=1.0`.
  2. **Nenhum dos 3 gates reais detecta "wrapper foi rebaixado de real para stub"**.
  3. `touring-quality::score_target` (que detectaria esse rebaixamento via 50-dim composite) **não foi consultado**.
- **A integração com harness existe mas é subdimensionada** — chama o composite de 17 gates stub-dominante quando o composite de 50 dim real está disponível a 1 import de distância.

---

## 2. Mapa de Overlap e Integração Cruzada

### 2.1 Grafo de dependências (Cargo.toml level)

```
                          ┌──────────────────────────────────┐
                          │   touring-quality (verifier)    │
                          │   ~11.000 LOC, 300+ tests        │
                          │   depends: touring-analysis     │
                          └──────────────┬───────────────────┘
                                         │ uses analyze_X / score_X
                                         ▼
┌──────────────────┐         ┌────────────────────────────┐
│ touring-analysis │         │  touring-harness (gate aggr)│
│ (engine layer)   │◄────────│  ~2.700 LOC, 14/17 STUB     │
│ ~5.500 LOC       │  NOT    │  NO deps on quality/analysis │
│ 950+ tests       │ used    └──────────────┬─────────────┘
└──────────────────┘ by                    │
                              ┌────────────┼────────────┐
                              │ uses       │ dep        │ uses
                              ▼            ▼            ▼
              ┌───────────────────┐  ┌─────────────────────┐  ┌────────────────────┐
              │ touring-harness-  │  │  touring-ceg (sand) │  │ touring-server      │
              │ mcp (MCP wrapper) │  │  ~13.000 LOC        │  │ (general MCP srv)   │
              │ 434 LOC, 5 tools  │  │  deps: harness      │  │ does NOT have       │
              │ NO overlap with   │  │  X7 BLOCK harness  │  │ touring_elite_*    │
              │ touring-server    │  └─────────────────────┘  └────────────────────┘
              └───────────────────┘
                              ▲
                              │ standalone binary (workspace member)
                              │
              ┌─────────────────────────────┐
              │  touring-elite (binary)      │
              │  306 LOC, 7 subcommands      │
              │  check / gate / badge / etc  │
              │  ALSO unreachable via        │
              │  `touring elite` (no such    │
              │  subcommand in touring-cli)  │
              └─────────────────────────────┘
```

### 2.2 Matriz de overlap de conceito

| Conceito | `touring-analysis` | `touring-quality` | `touring-harness` | `touring-harness-mcp` | `touring-ceg` |
|----------|--------------------|--------------------|--------------------|----------------------|--------------|
| Composite scoring (0.0-1.0) | — | ✅ (50-dim weighted avg) | ✅ (17-gate weighted avg) | — | ✅ (5-stage X0..X9 weighted avg) |
| Tier mapping (Diamond..Unranked) | — | ✅ (6 tiers) | ✅ (6 tiers, mesmos nomes) | — | — |
| Per-file detection | ✅ (engines) | ✅ (verifiers) | — (changes only) | — | — |
| Change tracking (before/after) | — | — | ✅ (Change struct) | — | — |
| Score history (JSONL) | — | — | ✅ (`~/.claude/touring/elite-history.jsonl`) | — | — |
| MCP exposure | — | — | — | ✅ (5 tools JSON-RPC manual) | — (CEG expõe via `touring-server`) |
| Sandbox/capability | — | — | — | — | ✅ (X0..X9 + landlock) |
| BLOCK-by-default | — | — (apenas Fail em test) | ✅ (Gold tier) | — | ✅ (X7 Deny) |
| `is_detector_own_source` pattern | ✅ (engines) | ✅ (wrappers) | — | — | — |
| `density_score` helper | ✅ (shared utility) | — | — | — | — |
| `score_history` JSONL | — | — | ✅ | — | — |

### 2.3 Quem fala com quem (real, não documentado)

| From | To | How | Status |
|------|----|----|--------|
| `touring-quality` | `touring-analysis` | `use touring_analysis::quality::{analyze_X, score_X}` via feature flag `workspace-integration` (default-on) | ✅ Real (50 verifiers) |
| `touring-ceg` (X7) | `touring-harness` | `use touring_harness::run_harness` em `harness_extension.rs` para Edit/Write | ✅ Real (parcial — só chama 17 gates stub-dominante) |
| `touring-harness-mcp` | `touring-harness` | `use touring_harness::{Change, run_harness, ...}` | ✅ Real |
| `touring-harness` | (nenhum) | — | ❌ Não consulta `touring-quality` nem `touring-analysis` |
| `touring-ceg` (X7) | `touring-quality` | — | ❌ **NÃO consulta** o composite de 50-dim |
| `touring-cli` | `touring-harness` | — | ❌ Subcommand `elite` documentado em `touring-harness/src/lib.rs` mas **NÃO EXISTE** — `touring --help` retorna `Unknown subcommand: elite` |
| `touring-server` | qualquer dos 3 | — | ❌ Nenhuma das 5 tools `touring_elite_*` está no catálogo MCP de `touring-server` |
| `touring-elite` (binário) | `touring-harness` | `use touring_harness::run_harness` via API direta | ✅ Real (standalone) |

---

## 3. Critérios "Best of" — quem tem o quê (com evidência)

| Critério / Capability | Melhor implementação | Onde | Por quê |
|----------------------|---------------------|------|---------|
| **Detector de secrets** (F2.4) | `touring-quality/src/verifications/f2_4_secrets.rs` (827 LOC) | `touring-quality` | 5 sinais ordenados por confiança (provider markers, structural tokens, conn-string creds, secret-named assignments, generic entropy), com gitleaks-style keyword pre-filter, comment-line filter (FP fix hoje), 42 testes passando |
| **Detector de complexidade** (F1.1) | `touring-analysis/src/quality/f1_1_complexity.rs` | `touring-analysis` (consumido por `touring-quality`) | Cyclomatic + cognitive + TDG grade + hotspot mapping; AST semantic via syn |
| **Detector de OWASP** (F2.1) | `touring-quality/src/verifications/f2_1_owasp.rs` | `touring-quality` | 42 testes, integração com `touring_analysis::SecurityAnalyzer` (10 vuln detectors tp/fp-validado) |
| **Detector de CVEs** (F2.5) | `touring-quality/src/verifications/f2_5_dep_cves.rs` | `touring-quality` | Integração com RustSec advisory DB; 11 deps com aviso informational |
| **Detector de CI/CD** (F4.7) | `touring-quality/src/verifications/f4_7_cicd.rs` | `touring-quality` | Detectou **22 CI smells reais** (20 actions não-pinadas por SHA) em `.github/workflows/ci.yml` durante a auditoria de hoje |
| **Detector de IaC** (F4.9) | `touring-quality/src/verifications/f4_9_iac.rs` | `touring-quality` | Detectores para Terraform (S3/SG/RDS) + Dockerfile (no-user, latest, EXPOSE 22) |
| **Detector de build_config polyglot** (F4.6) | `touring-analysis/src/quality/build_config.rs` (971 LOC, 29 tests) | `touring-analysis` (consumido por `touring-quality`) | Rust + Python + JS/TS + Go com `canonical_lang` enum |
| **Composite scoring algorithm** | `touring-quality/src/composite.rs::compute_composite` (146 LOC) | `touring-quality` | Weighted avg com pesos P0=2.0, P1=1.5, P2=1.0 (50 dim) |
| **Tier mapping** | `touring-quality/src/tier.rs::tier_from_composite` | `touring-quality` | Diamond ≥0.95, Platinum ≥0.90, Gold ≥0.80, Silver ≥0.70, Bronze ≥0.60, Unranked <0.60 |
| **Multi-scope aggregation** | `touring-quality/src/aggregate.rs::AggKind` (WorstOf / WeightedLoc / CoverageRatio / ScopeNative / Mean) | `touring-quality` | Fecha o gap do `--workspace unfaithful` (Wave 2026-06-21) |
| **Change tracking (before/after)** | `touring-harness/src/change.rs::Change` | `touring-harness` | Único conceito que 50-dim não tem — Change representa diff proposto |
| **Score history JSONL** | `touring-harness/src/history.rs::ScoreHistory` | `touring-harness` | `~/.claude/touring/elite-history.jsonl` append-only com drift detection |
| **Sandbox/landlock** | `touring-ceg/src/gateway/sandbox_executor.rs` + `capability/enforce_linux.rs` | `touring-ceg` | Único com kernel enforcement (landlock V1-V6, rlimit, cgroup v2) |
| **Capability model (Deno-style)** | `touring-ceg/src/capability/profile.rs::CapabilityProfile` (228 LOC) | `touring-ceg` | 4 built-in profiles, deny-by-default |
| **X0..X9 typestate** | `touring-ceg/src/gateway/typestate.rs::Execution<S>` | `touring-ceg` | Compile-time enforcement de ordem X3 VGP + X5 SANDBOX são unskippable |
| **MCP server (JSON-RPC manual)** | `touring-harness-mcp/src/main.rs` (434 LOC) | `touring-harness-mcp` | 5 tools `touring_elite_*` |
| **MCP server (rmcp framework)** | `touring-server/src/` | `touring-server` | 90+ tools; **NÃO tem** `touring_elite_*` (gap a fechar) |
| **Drift detection** | `touring-harness/src/history.rs::DriftReport` | `touring-harness` | Histórico + drift entre runs |
| **Evolution drift (tool/code drift)** | `touring` CLI (`touring evolution drift`) | `touring` CLI | Distinto: detecta drift de tool effectiveness via RL |

---

## 4. Gaps Estruturais Identificados (auditoria honesta)

### GAP #1 — `touring-harness` **NÃO consulta** o motor de 50 dim
- **Sintoma**: o gap dos wrappers F4.x stub-dominante passou despercebido pelo composite do harness, porque os 14 gates stub retornam `External / score=1.0` sem detectar a regressão.
- **Causa raiz**: `touring-harness` é uma árvore isolada — não importa `touring-quality` nem `touring-analysis`. Não tem como saber que existe um composite de 50 dim rodando em paralelo.
- **Severidade**: 🔴 BLOCK — o composite de "release readiness" hoje é majoritariamente composto de stubs assumindo PASS. É um problema existencial.

### GAP #2 — CEG X7 composite **não inclui** sinal de qualidade de código
- **Sintoma**: CEG X7 pondera 5 sinais internos (static/vgp/predict/sandbox/gate). Falta o 6º: composite de 50-dim do `touring-quality` aplicado ao `Change` proposto.
- **Causa raiz**: integração parcial. CEG importa `touring-harness` (que tem 14 stubs) mas não `touring-quality` (que tem 50 dim reais).
- **Severidade**: 🟡 HIGH — sandbox está protegido contra injection/exec, mas não está usando a melhor qualidade de detecção de código disponível.

### GAP #3 — `touring-harness-mcp` é um crate separado que duplica infra
- **Sintoma**: `touring-harness-mcp` é um daemon MCP separado que expõe 5 tools (`touring_elite_*`). `touring-server` (o daemon MCP principal do Touring) **NÃO tem** essas tools. Clientes MCP precisam escolher entre 2 daemons.
- **Causa raiz**: decisão histórica (C7 — "JSON-RPC manual" para evitar instabilidade de macro do `rmcp::serve`) levou a um crate separado em vez de um módulo em `touring-server`.
- **Severidade**: 🟡 HIGH — fragmenta o catálogo MCP do Touring e adiciona manutenção dupla (rebuild de binário, registro em `settings.json`, versionamento).

### GAP #4 — `touring elite` (CLI subcommand) **NÃO EXISTE**
- **Sintoma**: `touring-harness/src/lib.rs:53` documenta `touring elite {check, gate, badge, report, history, register}`. Mas `touring --help` retorna `Unknown subcommand: elite`. O binário correto é `touring-elite` (separado, com hyphen).
- **Causa raiz**: docs em `touring-harness/src/lib.rs` foram escritas assumindo integração com `touring-cli` que nunca foi feita.
- **Severidade**: 🟡 HIGH — documentação engana novos usuários e LLMs sobre onde o harness está exposto.

### GAP #5 — 14/17 gates do harness são stubs
- **Sintoma**: `builtins/code_quality.rs`, `performance.rs`, `testing.rs`, `documentation.rs`, `best_practices.rs`, `ci_cd_devops.rs`, `scalability.rs`, `extensibility.rs`, `naming.rs`, `navigability.rs`, `craftsmanship.rs`, `dependencies.rs`, `ux.rs`, `product_docs.rs` — todos têm 22-24 LOC, retornam `External / score=1.0` com mensagem "external CI step (assumed PASS)".
- **Causa raiz**: gates foram criados como "placeholders para integração futura" (comentário no `mod.rs`) — nunca substituídos por implementações reais.
- **Severidade**: 🔴 BLOCK — sem implementação real, o harness está efetivamente desligado para essas dimensões. **A melhor implementação de cada uma JÁ EXISTE** em `touring-quality` (F1.1, F1.3, F2.11, F3.1, F4.1, F4.7, etc.) e não está sendo usada.

### GAP #6 — Composite de 50-dim não é consultado pelo CEG X7
- **Sintoma**: a melhor detecção disponível (50-dim com 950+ tests passing e tp/fp-validado em arquivos reais) **não é consultada** pelo gateway X0..X9. Apenas o composite de 17-gate stub-dominante é.
- **Causa raiz**: arquitetura original do CEG (Pln2 2026-05-17) não previa integração com `touring-quality` (que nasceu depois, na Wave F4.x de 2026-06-21).
- **Severidade**: 🟡 HIGH — sandbox X7 está abaixo do que poderia estar.

### GAP #7 — `touring-quality`'s `composite_score` e `touring-harness`'s `composite_score` usam o **mesmo nome, fórmula diferente**
- **Sintoma**: ambos têm `composite_score()` mas com semânticas distintas — 50-dim weighted avg (touring-quality) vs 17-gate weighted avg (touring-harness). Composite do harness é majoritariamente baseado em stubs.
- **Causa raiz**: dois composite scorers paralelos, sem camada de rollup que mapeie dims → gates.
- **Severidade**: 🟡 HIGH — confusão semântica, dificuldade de raciocínio sobre "qual composite vale mais".

---

## 5. Análise de Criticidade (Matriz)

| Gap | Quem detecta hoje? | Quem DEVERIA detectar? | Criticidade |
|-----|--------------------|----------------------|-------------|
| #1 F4.x stubs | Ninguém (CEG X7 passa, harness passa) | `touring-quality`'s 50-dim check (que detecta regressão) | 🔴 BLOCK |
| #2 CEG sem qualidade | CEG X7 (não detecta) | CEG X7 + integração `touring-quality` | 🟡 HIGH |
| #3 MCP fragmentado | Ninguém | `touring-server` deveria incluir `touring_elite_*` tools | 🟡 HIGH |
| #4 CLI `elite` inexistente | Usuário descobre por tentativa | Doc-comment em `touring-harness/lib.rs` está errado | 🟡 HIGH |
| #5 14/17 gates stub | Ninguém (stubs retornam PASS) | `touring-harness` deveria chamar `touring-quality` | 🔴 BLOCK |
| #6 Composite 50-dim subutilizado | `touring-quality` score | CEG X7 + harness deveriam consumir | 🟡 HIGH |
| #7 Composite naming confuso | Nenhum | Refactor de naming | 🟢 LOW |

**Resumo executivo**: 2 BLOCKs (#1, #5) e 4 HIGHs (#2, #3, #4, #6) — a estrutura atual está com **dois composite scorers desconectados**, e o melhor deles (50-dim, real, testado) **não é usado pelo caminho crítico de decisão** (CEG X7 → harness BLOCK).

---

## 6. Mapeamento Proposto: 17 gates ↔ 50 dims (se a consolidação for feita)

Cada gate do harness agrega um conjunto de dims (ponderado por prioridade). Esta é a tabela de mapping canônica que deveria existir:

| GateId (17) | default_weight | Mapeia para DimId (50) | Agregação sugerida |
|--------------|---------------|------------------------|--------------------|
| CodeQuality | 1.0 | F1.1 + F1.2 + F1.3 + F1.4 + F1.5 + F1.6 + F4.1 | LOC-weighted mean |
| Architecture | 1.0 | F1.7 + F1.8 + F1.9 + F1.10 + F1.11 + F1.12 | WorstOf (qualquer cycle/boundary violation quebra a gate) |
| Security | 1.5 | F2.1 + F2.2 + F2.3 + F2.4 + F2.5 + F2.6 | WorstOf (P0 BLOCK dims — qualquer um é fail-closed) |
| Performance | 1.0 | F2.7 + F2.8 + F2.9 + F2.10 + F2.11 + F2.12 + F2.13 | Mean |
| Testing | 1.0 | F3.1 + F3.2 + F3.3 + F3.4 + F3.5 + F3.6 + F3.7 | LOC-weighted (god-files pesam mais) |
| Documentation | 1.0 | F3.8 + F3.9 + F3.10 + F3.11 + F3.12 + F3.13 | CoverageRatio (proporção de arquivos documentados) |
| BestPractices | 1.0 | F4.1 + F4.2 + F4.3 + F4.4 + F4.5 + F4.6 | Mean (com P0 Pkg-mgmt puxando para baixo) |
| CiCdDevops | 1.0 | F4.7 | ScopeNative (CI config) |
| Modularization | 1.0 | F1.7 + F1.11 (file size, boundaries) | WorstOf |
| Scalability | 0.7 | F2.13 + F2.11 | Mean |
| Extensibility | 0.7 | F1.9 + F1.10 + F1.11 | Mean |
| Naming | 0.7 | F1.2 + F4.1 | LOC-weighted |
| Navigability | 0.7 | F3.10 + F3.11 | Mean |
| Craftsmanship | 0.7 | F1.1 + F1.4 + F4.4 + F1.2 | Mean |
| Dependencies | 1.5 | F4.5 + F2.5 | WorstOf (CVE + EOL = fail) |
| Ux | 0.6 | F2.12 + F1.2 | Mean |
| ProductDocs | 0.7 | F3.10 + F3.11 + F3.13 | Mean |

A função de agregação (que poderia viver em `touring-quality/src/gates.rs`, novo módulo) seria:

```rust
pub fn aggregate_to_gates(
    dim_scores: &BTreeMap<DimId, DimScore>,
    mapping: GateMapping, // tabela acima
) -> HashMap<GateId, f32>
```

E o composite do harness seria:

```rust
composite = Σ (gate_weight × gate_score) / Σ gate_weight
```

Onde `gate_score` vem da agregação de dims, eliminando o problema dos 14 stubs (cada gate agora é a agregação real das dims que têm implementação real em `touring-quality`).

---

## 7. Riscos da Consolidação

| Risco | Severidade | Mitigação |
|-------|------------|-----------|
| **Mudança de score**: o composite de 17 gates mudará quando passarmos a usar 50-dim. Releases Gold hoje podem virar Silver amanhã. | 🟡 HIGH | Versionar a transição (manter 17-gate stub por 1 wave + warning). Adicionar flag `use_50dim_backend` opt-in. |
| **Regressão em test coverage**: testes existentes do harness usam stubs para validar o composite. Mudar para dims reais quebrará esses testes. | 🟡 HIGH | Rodar `touring test --workspace` antes de cada mudança; mapear 1-a-1 os testes existentes para o novo backend. |
| **Dependência circular**: `touring-quality` precisa conhecer `GateId` (do harness) e `touring-harness` precisa consumir `touring-quality`. | 🟡 HIGH | **Mover `GateId` enum para `touring-quality`** (mais natural, é onde vive o conceito de "dim"). `touring-harness` re-exporta de `touring-quality`. |
| **Performance regression**: o composite de 50-dim por change pode ser lento (50 verifiers × N files). | 🟢 LOW | Aggregate é LOC-weighted (custa O(files)). Composite é O(50 dims). Nada caro. |
| **`touring-harness-mcp` quebrar**: clientes MCP que dependem do daemon separado ficarão sem tools se movermos para `touring-server`. | 🟡 HIGH | Manter `touring-harness-mcp` como shim que delega a `touring-server` (binary re-export). Ou documentar migração. |
| **touring-elite binário**: é standalone e funciona. Manter ou deletar? | 🟢 LOW | Manter como CLI direto (não interfere com consolidação). Apenas consertar a doc-comment em `lib.rs`. |

---

## 8. Perguntas em Aberto para Decisão

Antes de QUALQUER consolidação, decisões que dependem de preferência humana:

### Q1: Sobrevivência do `touring-harness` como crate separado
- **Opção A**: Manter como crate, mas refatorar para ser **thin wrapper** que orquestra `touring-quality`. Toda análise real sai de touring-quality.
- **Opção B**: Dissolver `touring-harness` e mover Change/History/Report para dentro de `touring-quality`. `touring-quality` ganha um novo módulo `gates.rs` com o rollup 50→17.
- **Opção C**: Manter ambos como estão, apenas adicionar a ponte (consultar `touring-quality` dentro de `touring-harness::run_harness`).

### Q2: Sobrevivência do `touring-harness-mcp`
- **Opção A**: Deletar. Migrar as 5 tools para `touring-server/src/cli/harness_metric.rs` (que já existe e tem o conceito).
- **Opção B**: Manter como shim que re-exporta via `touring-server`. Update `settings.json` para apontar para `touring-server`.
- **Opção C**: Manter como está (independente, paralelo).

### Q3: Posição do `touring-ceg` na consolidação
- **Opção A**: Manter como crate separado (sandbox é concern diferente). Apenas **adicionar** um sinal "50-dim composite do touring-quality" no X7 `composite_score`. Novos pesos: redistribuir (W_QUALITY=0.20, W_GATE=0.20, etc.).
- **Opção B**: Dissolver `touring-ceg` em `touring-hooks` (era lá antes de Pln2 2026-05-17). Refazer.

### Q4: Composite scoring
- **Opção A**: Manter dois composite scorers paralelos (50-dim para qualidade, 17-gate para release). Documentar quando usar qual.
- **Opção B**: Composite único 50-dim, e os 17 gates viram "labels" para alertas humanos (não usados para composite).
- **Opção C**: Composite único 17-gate, onde cada gate é a agregação das dims relevantes (tabela do item 6).

### Q5: Stubs do harness
- **Opção A**: Deletar os 14 stubs e substituí-los por rollup real de dims.
- **Opção B**: Manter como "fast path" (substring) para fallback offline; preferir rollup real quando disponível.

### Q6: Local canônico para `GateId` enum
- **Opção A**: `touring-quality/src/gates.rs` (junto com `DimId`, `Enforcement`, `Tier`).
- **Opção B**: `touring-harness/src/gate.rs` (manter onde está hoje).
- **Opção C**: Novo crate `touring-elite-core` para evitar ciclo.

### Q7: Score history
- **Opção A**: Manter JSONL do `touring-harness` (`~/.claude/touring/elite-history.jsonl`).
- **Opção B**: Adicionar score history também em `touring-quality` (similar JSONL com timestamp+composite+tier).
- **Opção C**: Unificar em uma única estrutura de histórico.

---

## 9. Recomendação (apenas texto — sem ação)

A **consolidação recomendada** segue a **REGRA #0** (potencializar, nunca reduzir) e absorve **o melhor de cada parte**:

1. **`touring-analysis` permanece como engine layer** (mantém 50 engines reais, é a fonte da verdade para detecção).
2. **`touring-quality` permanece como verifier/composite layer**, e **ganha um novo módulo `gates.rs`** com:
   - `GateId` enum (movido de `touring-harness`)
   - Tabela de mapping 50 dim → 17 gate (item 6 acima)
   - Função `aggregate_to_gates(dim_scores) -> HashMap<GateId, f32>`
   - Composite de 17 gates derivado da agregação de dims (sem stubs)
3. **`touring-harness` torna-se thin orchestrator**:
   - Mantém `Change`, `ProposedFile`, `FileKind`, `ScoreHistory`, `DriftReport` (conceitos únicos)
   - `run_harness(change)` agora chama `touring-quality::score_target(path)` em cada file da change, agrega via `aggregate_to_gates`, computa composite final via pesos dos gates
   - Os 17 `GateId`s continuam existindo (re-export de `touring-quality::gates`)
   - O score_history JSONL recebe **adicionalmente** o composite de 50-dim (campo novo) sem perder o de 17-gate
4. **`touring-harness-mcp` é absorvido por `touring-server`**:
   - As 5 tools viram módulo `touring-server/src/cli/elite_tools.rs` (ou similar)
   - O binário `touring-harness-mcp` é mantido como shim que delega a `touring-server` (zero-friction para clientes existentes)
   - O `Cargo.toml` workspace mantém `touring-harness-mcp` apenas durante a transição (1 wave); remoção após validar migração
5. **`touring-ceg` integra o composite de 50-dim**:
   - CEG X7 `composite_score` ganha um sexto sinal: `touring-quality::score_target` (0.0-1.0)
   - Pesos redistribuídos: W_STATIC=0.20, W_QUALITY=0.20, W_VGP=0.15, W_PREDICT=0.10, W_SANDBOX=0.15, W_GATE=0.20 (total = 1.0)
   - Critério do usuário **"se o sandbox tiver mais ou melhores critérios, o harness deve absorve-lo"** → reverse também: se o harness tiver melhor critério (50-dim real), o CEG **deve absorve-lo**
6. **Doc-comment fix**: `touring-harness/src/lib.rs:53` mencionar `touring-elite` (binário standalone), não `touring elite` (subcommand inexistente).

**Resultado**: 1 engine, 1 verifier+composite, 1 aggregator (thin), 1 MCP surface, 1 sandbox com integração. As 5 estruturas viram **uma estrutura consolidada** com CEG como único subsistema separado (por design — sandbox é concern distinto).

---

## 10. Apêndice — Comandos Úteis para Verificação

```bash
# Mapear os 50 engines reais vs stubs
ls /home/gabrielgadea/.claude/rust/crates/touring-analysis/src/quality/*.rs | wc -l
ls /home/gabrielgadea/.claude/rust/crates/touring-quality/src/verifications/*.rs | wc -l

# Confirmar gap #1 (harness não consulta quality)
grep -rE 'use touring_quality|touring_quality::' /home/gabrielgadea/.claude/rust/crates/touring-harness/src/
# (esperado: 0 matches)

# Confirmar gap #2 (CEG X7 sem sinal de qualidade)
grep -rE 'touring_quality::score_target|score_target' /home/gabrielgadea/.claude/rust/crates/touring-ceg/src/
# (esperado: 0 matches)

# Confirmar gap #4 (CLI `touring elite` não existe)
/home/gabrielgadea/.claude/rust/target/release/touring --help 2>&1 | grep -i 'elite'
# (esperado: 0 matches)

# Confirmar gap #5 (14/17 gates stub)
for f in /home/gabrielgadea/.claude/rust/crates/touring-harness/src/builtins/*.rs; do
  loc=$(wc -l < "$f")
  [ "$loc" -lt "30" ] && echo "STUB: $f ($loc LOC)"
done
# (esperado: 14 arquivos < 30 LOC)

# Confirmar que touring-elite binário existe e funciona
/home/gabrielgadea/.claude/rust/target/release/touring-elite --help

# Confirmar que touring-harness-mcp responde JSON-RPC
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | /home/gabrielgadea/.claude/rust/target/release/touring-harness-mcp

# Confirmar que touring-quality score roda em arquivo real
/home/gabrielgadea/.claude/rust/target/release/touring-quality score /home/gabrielgadea/.claude/rust/crates/touring-analysis/src/quality/mod.rs --dims F4.6,F4.7,F4.9,F4.10

# Verificar CEG já importa touring-harness
grep -E 'touring_harness::' /home/gabrielgadea/.claude/rust/crates/touring-ceg/src/gateway/harness_extension.rs
```

---

**FIM DO DIAGNÓSTICO.** Nenhuma estrutura foi renomeada, deletada ou mesclada. Aguardando direção de Gabriel para qualquer ação subsequente.
