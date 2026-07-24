# Touring Quality Harness — MASTER PLAN: Premium-Elite Multi-Scope Infrastructure (50 dims × todos os escopos)

> **Data**: 2026-06-21 (rev. 2 — expandida & aperfeiçoada) | **Autor**: TACO (Gabriel Gadea, comandante) | **Nível**: L4
> **Crate alvo**: `crates/touring-quality` (Rust) + `docs/*.py` (elite gates) + skill `comprehensive-review:full-review`
> **Precede**: `2026-06-21-touring-quality-multiscope-harness-diagnosis.md` (diagnóstico + Thrust B scaffold ✅)
> **Grounding**: context7 `/websites/sonarsource_sonarqube-server` (quality gates, conditions, ratings A-E, Clean as You Code) `[FACT 1.0]`
> **Invariante constitucional (Gabriel)**: as 50 dims aparecem em TODO relatório, em TODO escopo, com **critérios objetivos, claros e mensuráveis**; nenhuma dim jamais descartada.

---

## §0 — O que esta revisão acrescenta (sobre a rev.1)

| Adição | Por quê |
|---|---|
| **Hierarquia de planejamento completa** (Master Plan → Roadmaps → Plan → Waves → Phases → Tasks → Subtasks → QuickAction) | requisito de Gabriel: estrutura de elite navegável |
| **Modelo de critério objetivo** (`condition = metric ⊕ operator ⊕ threshold`, SonarQube) + **tabela canônica das 50 dims** | requisito: "critérios objetivos, claros e mensuráveis em todos os escopos" |
| **Scope Ladder explícito** arquivo → feature → **workflow** → repositório → workspace (+ os 3 in-between) | requisito: a escada de escopos nomeada por Gabriel |
| **Unificação dos 2 harnesses** (`touring-quality` 50-dim ⟷ `elite_aggregate.py` 13-gate ⟷ skill `full-review`) | descoberta da exploração: hoje rodam em paralelo; o report 50-dim workspace deve ser a fonte da verdade |
| **Gate-verdict machine-readable** (JSON análogo ao webhook SonarQube) por escopo | objetividade auditável; consumível por CI/MCP |

---

## §1 — Exploração profunda: o terreno (ground truth `[FACT 1.0]`)

### 1.1 Dois harnesses paralelos (a serem unificados)

| Harness | Forma | Escopo | Saída |
|---|---|---|---|
| **`touring-quality`** | Rust, **51 verifier files** (f1_1..f4_12), `lib.rs` `DimId` (50) + `phase()`/`enforcement()`/`agg_kind()`, `DimScore`, `composite.rs` (Block 2.0/Warn 1.5/Advisory 1.0), `tier.rs` (Diamond .95/Platinum .90/Gold .80/Silver .70/Bronze .60) | per-file + (Thrust B) per-scope scaffold | `QualityReport`/`ScopeReport` |
| **`elite_aggregate.py`** | Python, **13 gates** (02_architecture=wiring_integrity, 05/09=file_size, 06=gen_reference, 08=root_hygiene, 10=scalability, 11=extensibility, 14=craftsmanship, 16=ux, 17=sync_metrics, 04=perf_p99, 03/15=cargo-deny) | **workspace release composite** (Diamond 0.9703) | composite + tier; é o gate de CI |

**Problema**: o `elite_aggregate` é um composite de **13 proxies workspace-level**; o `touring-quality` tem as **50 dims reais** mas só amadureceu o file-scope. Eles medem coisas sobrepostas com motores diferentes → **fonte-da-verdade dupla**. A infraestrutura Premium-Elite os **unifica** (§5).

### 1.2 A skill `comprehensive-review:full-review` — a versão agent-orchestrada do harness

`.full-review/` materializa exatamente a taxonomia das 50 dims:

| Fase da skill | Arquivo | Dims cobertas |
|---|---|---|
| Phase 1 Code Quality & Architecture | `01-quality-architecture.md` (01a code + 01b arch) | **F1.1–F1.12** |
| Phase 2 Security & Performance | `02-security-performance.md` (02a sec + 02b perf) | **F2.1–F2.13** |
| Phase 3 Testing & Documentation | `03-testing-documentation.md` (03a test + 03b doc) | **F3.1–F3.13** |
| Phase 4 Best Practices & CI/CD | `04-best-practices.md` (04a rust + 04b cicd) | **F4.1–F4.12** |
| Phase 5 Consolidated | `05-final-report.md` + `state.json` (phase verdicts, running_totals, remediations_applied) | composite + tier |

→ **O harness `touring-quality` é o motor determinístico; a skill `full-review` é a narrativa agent-orchestrada por cima.** A infraestrutura de elite faz o harness emitir o relatório **estruturado idêntico ao `.full-review/`** por escopo, e a skill consome os scores objetivos em vez de re-derivá-los.

### 1.3 Os 7 gaps de fidelidade (recap do diagnóstico, ainda válidos)

G1 17 ScopeNative concatenam+grep · G2 CoverageRatio falso (DimScore só `value:f32`) · G3 herança não sobe ao artefato · G4 trait cega ao file-set (keystone) · G5 sem dual-measurement · G6 CLI/integração incompleta · G7 grafo interno vs daemon. **+ G8 (novo): harness duplo não-unificado · G9 (novo): sem condições objetivas explícitas por dim/escopo.**

---

## §2 — Modelo de critério objetivo (SonarQube-grounded) — o coração da revisão

### 2.1 Forma canônica de uma condição (idêntica para as 50 dims)

Do SonarQube `[FACT 1.0]`: `condition = (metric, operator, errorThreshold, onLeakPeriod)` → `status ∈ {OK, FAIL, NO_VALUE}`. A **"Sonar way for AI code"** (gate recomendado p/ código gerado por IA) tem 7 condições objetivas: **0 new issues · hotspots revisados · new coverage ≥ 80% · new duplication ≤ 3% · Security A · Reliability ≥ C**.

Generalizo para um **contrato por-dim**, instanciado por escopo:

```
DimContract {
  dim,                       // F1.1..F4.12
  measure,                   // o que é contado (objetivo)
  unit,                      // %, count, ratio, score[0..1]
  agg_kind,                  // WorstOf | WeightedLoc | CoverageRatio | ScopeNative
  operator,                  // >= | <= | ==
  threshold_gold,            // piso de entrega (Gold 0.80 / regra-específica)
  threshold_diamond,         // alvo elite (0.95 / regra-específica)
  threshold_newcode,         // Clean-as-You-Code (mais estrito; onLeakPeriod)
  enforcement,               // BLOCK | WARN | ADVISORY
  scope_native,              // bool — onde é medido nativamente
}
```

O **veredito de escopo** = conjunto de condições avaliadas → JSON machine-readable (§6), análogo ao webhook SonarQube.

### 2.2 Tabela canônica — as 50 dims com critério objetivo (Fase 1: Code Quality & Architecture)

| Dim | Measure objetivo | AggKind | Gold (≥/≤) | Diamond | New-code (CaYC) | Enf |
|---|---|---|---|---|---|---|
| F1.1 complexity | LOC-wt mean de (CC≤10 ? 1 : decay) | WeightedLoc | ≥0.80 | ≥0.95 | nova fn CC≤10 | WARN |
| F1.2 maintainability | LOC-wt (fn-len≤50, nomes≥3ch, 0 magic#) | WeightedLoc | ≥0.80 | ≥0.95 | — | WARN |
| F1.3 duplication | dup_lines / total_lines (densidade) | CoverageRatio | ≤8% | ≤3% | ≤3% | WARN |
| F1.4 solid | LOC-wt (god-struct/ISP/DIP) | WeightedLoc | ≥0.80 | ≥0.95 | — | WARN |
| F1.5 tech-debt | densidade TODO/FIXME/allow / KLOC | WeightedLoc | ≥0.80 | ≥0.95 | 0 novo `allow(dead_code)` | WARN |
| F1.6 error-handling | LOC-wt (0 `unwrap` prod) | WeightedLoc | ≥0.90 | ≥0.95 | 0 novo unwrap prod | WARN |
| F1.7 boundaries | LOC-wt pub-surface ratio | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| **F1.8 dep-cycles** | **#SCC(size≥2)** (Tarjan) | **ScopeNative** | **==0** | ==0 | 0 ciclo novo | ADV→BLOCK |
| F1.9 api-design | LOC-wt (naming/Result-typed pub) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| F1.10 data-model | LOC-wt (newtype/enum/illegal-states) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| F1.11 patterns | LOC-wt (idioms Rust) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| **F1.12 arch-consistency** | **#layer-violations** (grafo) | **ScopeNative** | **==0** | ==0 | 0 violação nova | ADV |

### 2.3 Tabela canônica (Fase 2: Security & Performance)

| Dim | Measure objetivo | AggKind | Gold | Diamond | New-code | Enf |
|---|---|---|---|---|---|---|
| **F2.1 owasp** | #injection-sinks (worst-of) | WorstOf | ==0 | ==0 | **0 (BLOCK)** | ⛔BLOCK |
| F2.2 input-validation | #sinks sem allowlist | WorstOf | ==0 | ==0 | 0 | WARN |
| F2.3 authz | #endpoints sem checagem | WorstOf | ==0 | ==0 | 0 | WARN |
| **F2.4 secrets** | #high-entropy/token hits | WorstOf | ==0 | ==0 | **0 (BLOCK)** | ⛔BLOCK |
| **F2.5 dep-cves** | #CVE/yanked no manifest+lock | ScopeNative | ==0 | ==0 | **0 (BLOCK)** | ⛔BLOCK |
| **F2.6 config** | #misconfig (debug/CORS*) worst-of | WorstOf | ==0 | ==0 | **0 (BLOCK)** | ⛔BLOCK |
| F2.7 db-perf | LOC-wt (N+1) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| F2.8 memory | LOC-wt (unbounded/clone) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| F2.9 caching | LOC-wt (invalidação) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| F2.10 io | LOC-wt (blocking-in-async) | WeightedLoc | ≥0.80 | ≥0.95 | 0 novo block-in-async | ADV |
| F2.11 concurrency | LOC-wt (lock-across-await) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| F2.12 frontend | LOC-wt web/wasm (NA→1.0) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| **F2.13 scalability** | #SPOF/global-state (grafo) | ScopeNative | ==0 | ==0 | — | ADV |

### 2.4 Tabela canônica (Fase 3: Testing & Documentation)

| Dim | Measure objetivo | AggKind | Gold | Diamond | New-code | Enf |
|---|---|---|---|---|---|---|
| F3.1 coverage | covered_lines / total | CoverageRatio | ≥80% | ≥90% | **≥80%** | WARN |
| F3.2 test-quality | LOC-wt (assert/mutation) | WeightedLoc | ≥0.80 | ≥0.95 | — | WARN |
| **F3.3 test-pyramid** | unit:integration:e2e ratio | ScopeNative | base unit larga | — | — | WARN |
| F3.4 edge-cases | LOC-wt (boundary/property) | WeightedLoc | ≥0.80 | ≥0.95 | — | WARN |
| F3.5 test-maint | LOC-wt (isolamento, 0 `#[ignore]`) | WeightedLoc | ≥0.80 | ≥0.95 | 0 novo `#[ignore]` | WARN |
| **F3.6 sec-tests** | #controles-seg com teste-negativo | ScopeNative | ≥0.80 | ≥0.95 | — | WARN |
| **F3.7 perf-tests** | #benches presentes (criterion) | ScopeNative | ≥1 | regression-guard | — | WARN |
| F3.8 inline-doc | pub-items documentados / total | CoverageRatio | ≥80% | ≥95% | novo pub documentado | ADV |
| F3.9 api-doc | pub-API documentada / total | CoverageRatio | ≥80% | ≥95% | — | ADV |
| **F3.10 arch-doc** | #ADRs + diagramas presentes | ScopeNative | ≥1 ADR | C4 completo | — | ADV |
| **F3.11 readme** | README seções essenciais / 6 | ScopeNative | ≥0.80 | ≥0.95 | — | ADV |
| F3.12 doc-accuracy | LOC-wt (doctest compila, 0 drift) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| **F3.13 changelog** | CHANGELOG presente + [Unreleased] | ScopeNative | presente | Keep-a-Changelog | entrada p/ breaking | ADV |

### 2.5 Tabela canônica (Fase 4: Best Practices & CI/CD)

| Dim | Measure objetivo | AggKind | Gold | Diamond | New-code | Enf |
|---|---|---|---|---|---|---|
| F4.1 idioms | LOC-wt (clippy-clean) | WeightedLoc | ≥0.90 | ≥0.95 | 0 clippy warn novo | WARN |
| F4.2 frameworks | LOC-wt (framework patterns) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| **F4.3 deprecated** | #usos de API deprecada worst-of | WorstOf | ==0 | ==0 | **0 (BLOCK)** | ⛔BLOCK |
| F4.4 modernization | LOC-wt (edition/features) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| **F4.5 pkg-mgmt** | #dep > cap OR EOL no manifest | ScopeNative | ≤cap | enxuto | **0 EOL novo (BLOCK)** | ⛔BLOCK |
| F4.6 build-config | [profile.*] otimizado (Cargo.toml) | ScopeNative | ≥0.80 | ≥0.95 | — | ADV |
| **F4.7 cicd** | #gates obrigatórios no `.github/` | ScopeNative | ≥0.80 | gates completos | — | ADV |
| **F4.8 deploy** | estratégia+rollback nos manifests | ScopeNative | ≥0.80 | progressive | — | ADV |
| **F4.9 iac** | IaC presente + scaneada | ScopeNative | ≥0.80 | checkov-clean | — | ADV |
| F4.10 monitoring | LOC-wt (tracing vs println) | WeightedLoc | ≥0.80 | ≥0.95 | — | ADV |
| **F4.11 incident** | runbooks presentes | ScopeNative | ≥1 | SEV+escalation | — | ADV |
| **F4.12 env** | 0 secret em `.env` + vault | ScopeNative | ==0 | SOPS/Vault | 0 secret novo | ADV |

> **Distribuição (verificada)**: WorstOf 6 · CoverageRatio 4 · WeightedLoc 23 · ScopeNative 17 = 50. As 6 BLOCK (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5) são `==0` fail-closed em todo escopo. Toda dim tem critério **objetivo** (count/ratio/score com operador+threshold), **claro** (uma linha) e **mensurável** (o verifier o computa).

---

## §3 — Scope Ladder (a escada de escopos de Gabriel, objetiva)

| Rung (Gabriel) | ScopeKind | Resolução objetiva | Artefatos ScopeNative ativos | Gate típico |
|---|---|---|---|---|
| **arquivo individual** | `File` | o arquivo | herdados do crate envolvente (rotulados) | pre-edit (6 BLOCK per-file) |
| (subtree) | `Path` | dir subtree | herdados | revisão dirigida |
| **nova feature** | `Feature` | root + `--include`/`--new-code` globs (changeset, git-free) | herdados do repo | **Clean-as-You-Code** (new ≥0.95) |
| **workflow** | `Workflow`* | conjunto nomeado de arquivos de 1 fluxo ponta-a-ponta (`--include` multi-dir OR manifest de fluxo) | grafo do fluxo + artefatos do repo | gate de fluxo (cycles==0, SPOF==0) |
| (crate) | `Crate` | dir com `[package]` (+ seu `Cargo.toml`) | manifest do crate + grafo intra-crate | gate de crate (Gold 0.80) |
| **repositório** | `Repo` | dir com `.git`/README (artefatos ativos) | todos os 17 ScopeNative nativos | gate de repo |
| (projeto) | `Project` | ≈ Repo OU conjunto nomeado de membros | artefatos do projeto | gate de projeto |
| (sistema) | `System` | conjunto coerente de crates (`--include` multi-crate) | grafo cross-crate + artefatos | gate de sistema |
| **workspace completo** | `Workspace` | raiz `[workspace]`; todos os membros | ciclos/boundaries workspace-wide; manifest-tree | **release gate** (= unifica elite 13-gate) |

*`Workflow` é introduzido nesta revisão = um **fluxo de processo** (ex.: "o pipeline de indexação" = `crates/touring-*/src/**/index*.rs` + consumidores) resolvido por glob-set nomeado, pontuado como unidade com seu próprio veredito. Reusa a maquinaria `System`/`Feature` (glob-bounded), sem variante nova de código (honest alias) — mas com **rótulo e gate próprios**.

**Dual-measurement (SonarQube)**: todo escopo ≥ `Feature` reporta **dois** valores por dim — *overall* (todo o file-set) e *new-code* (`--new-code` glob/changeset). O gate de PR reprova no **new-code** (Clean as You Code), nunca penalizando legado.

---

## §4 — A hierarquia de planejamento

### ▛ MASTER PLAN — `MP-TQH` "Touring Quality Harness: Premium-Elite Multi-Scope"

**North Star**: UM motor determinístico que pontua as **50 dimensões** em **qualquer escopo** (arquivo→workspace) com **condições objetivas mensuráveis**, **unifica** os dois harnesses, e alimenta a skill `full-review` + o CI + o MCP. **Definition of Done (mensurável)**:
1. `touring-quality score <t> --scope <k>` emite as 50 dims fiéis em qualquer `k`, 0 placeholders. 
2. Cada dim tem condição objetiva avaliada (§2) e veredito JSON (§6). 
3. `--scope workspace` reproduz/supera o Diamond 0.9703 e o `elite_aggregate` passa a lê-lo. 
4. Gate Clean-as-You-Code (`--scope feature --new-code`) bloqueia regressão em código novo. 
5. 0 regressão (check/clippy/test/fmt/elite todos verdes a cada passo).

### ▛ ROADMAPS (4 lanes de resultado)

| Roadmap | Resultado | Phases |
|---|---|---|
| **R1 — Faithfulness Engine** | as 50 dims genuinamente fiéis por escopo | P0,P1,P2,P3,P4 |
| **R2 — Objective Criteria & Gates** | contrato `DimContract` + veredito JSON + 6-BLOCK por escopo | P5,P6 |
| **R3 — Scope Ladder & Dual-Measurement** | os 9 rungs + overall/new-code (CaYC) | P7,P8 |
| **R4 — Integration & Unification** | unificar 50-dim ⟷ 4 seams (CEG X7 · meta-loop · CI · full-review); CI/subcommand/MCP/obs | P9,P10,P11,P12 |

### ▛ WAVES (4 fatias temporais, cada uma cruza roadmaps e é shippable)

| Wave | Tema | Phases | Entrega shippable |
|---|---|---|---|
| **W1 — Foundation** | costura + modelo de critério | P0, P5 (contrato) | seam + DimContract + JSON verdict; 33 dims já fiéis |
| **W2 — Faithful Verifiers** | os 17+4 fiéis | P1, P2, P3, P4 | todas as 50 dims fiéis em todo escopo |
| **W3 — Scope & Gates** | ladder + dual-measure + gates | P6, P7, P8 | Clean-as-You-Code + 6-BLOCK por escopo + os 9 rungs |
| **W4 — Unification & Elite** | unificar os 4 seams + surface | P9, P10, P11, P12 | `touring quality` subcmd + CI + MCP + full-review + **CEG X7 runtime gate** consomem o 50-dim |

### ▛ PHASES → TASKS → SUBTASKS

#### **P0 — Seam & ScopeContext** (W1 · R1) · **M** · LOW
- **T0.1** criar `src/scope_context.rs`. Subtasks: (a) struct `ScopeContext{kind,root,repo_root,manifest,files,file_loc,inherited}`; (b) `resolve_repo_root` (walk-up .git/[workspace]/README); (c) `resolve_manifest` (walk-up Cargo.toml/package.json/pyproject); (d) `default_rollup(verifier,cx)` (per-file + aggregate atual).
- **T0.2** `Verification::check_scope(&ScopeContext)` default-method = `default_rollup`. Subtask: zero churn nos 33 verifiers.
- **T0.3** `DimScore.ratio: Option<(u64,u64)>` + `#[serde(skip_serializing_if=…)]` + `schema_version++`.
- **T0.4** `score_scope` roteia via `check_scope(cx)`.
- **Gate**: golden snapshot dos 33 roll-up dims idêntico; Diamond intacto.

#### **P1 — Manifest + artefato-arquivo (10 dims)** (W2 · R1) · **L** · LOW · *pipeline 1-verifier-cada*
- **T1.1 manifest** (F4.5 pkg-mgmt, F4.6 build-config): `check_scope` parseia `cx.manifest` (dep-count/EOL/`[profile.*]`). F2.5 já-fiel → só auditar.
- **T1.2 artefato** (F3.10,F3.11,F3.13,F4.7,F4.8,F4.9,F4.11,F4.12): `check_scope` localiza o artefato em `cx.repo_root` (README.md, CHANGELOG.md, docs/adr/, .github/workflows/, deploy/, *.tf, RUNBOOKS, .env). Subtask comum: herança `inherited=true` + rótulo no sub-escopo.
- **Dogfood**: `score crates/touring-server --scope crate --dims F3.11` pontua o README real.

#### **P2 — Classificação de file-set (3 dims)** (W2 · R1) · **M** · LOW
- **T2.1** F3.3 test-pyramid: classificar `cx.files` (unit `#[cfg(test)]` vs `tests/` vs e2e dir) → ratio.
- **T2.2** F3.6 sec-tests / **T2.3** F3.7 perf-tests (`benches/`, criterion).

#### **P3 — Grafo ScopeNative (3 dims, long-pole/USP)** (W2 · R1) · **L** · **MEDIUM**
- **T3.1** extrator de use-graph interno de `cx.files` (nó=módulo, aresta=`use crate::`/`mod`). Subtasks: parse imports (regex conservador OR tree-sitter), build adjacency.
- **T3.2** F1.8 dep-cycles = **Tarjan-SCC real** (#SCC size≥2). **T3.3** F1.12 arch-consistency (layer-order). **T3.4** F2.13 scalability (fan-in/global-state).
- **T3.5** (G7) enriquecimento opcional: se socket vivo, `touring wiring cycles -j` (Tarjan do daemon, USP); fallback graceful ao grafo interno.
- **Gate**: invariante acíclico do workspace ⇒ F1.8 workspace == 0.

#### **P4 — CoverageRatio real (4 dims)** (W2 · R1) · **M** · LOW
- **T4.1** F1.3 duplication, **T4.2** F3.1 coverage, **T4.3** F3.8 inline-doc, **T4.4** F3.9 api-doc: `check_scope` devolve `DimScore.ratio=(num,den)`; `aggregate(CoverageRatio)` soma → Σnum/Σden real (substitui o `agg_weighted` placeholder em `aggregate.rs:126`).

#### **P5 — DimContract + veredito JSON** (W1 · R2) · **M** · LOW
- **T5.1** `src/criteria.rs`: `const CRITERIA: [DimContract; 50]` (a §2 codificada). **T5.2** `evaluate_condition(dim, value, ratio) -> Condition{status}`. **T5.3** `ScopeReport::gate_verdict() -> GateVerdict` (JSON §6).

#### **P6 — 6-BLOCK por escopo + fail-closed** (W3 · R2) · **M** · LOW
- **T6.1** `touring-quality check --gate <dim> --target <t> --scope <k>` retorna exit≠0 se BLOCK-dim < threshold. **T6.2** `--fail-below`/`--fail-gate` por escopo. **T6.3** integração com os 6-BLOCK PreToolUse hooks (já per-file; adicionar pre-merge de escopo).

#### **P7 — Scope Ladder completo (9 rungs)** (W3 · R3) · **M** · LOW
- **T7.1** `ScopeKind::Workflow` (alias glob-set nomeado + rótulo/gate). **T7.2** resolução Project/System por conjunto-de-membros nomeado. **T7.3** precedência de auto-detect documentada (workspace>package>repo>path).

#### **P8 — Dual-measurement (overall + new-code)** (W3 · R3) · **M** · LOW
- **T8.1** `score_scope(scope, dims, new_code: Option<&[glob]>)`; `ScopeReport` carrega 2 `DimScore`/dim. **T8.2** CLI `--new-code <glob>`. **T8.3** gate CaYC (new ≥0.95 BLOCK; overall ≥0.80 advisory).

#### **P9 — Unificação dos harnesses** (W4 · R4) · **L** · MEDIUM
- **T9.1** `touring-quality score . --scope workspace --format gate-json` emite o condition-set. **T9.2** `elite_aggregate.py` passa a **consumir** esse JSON (os 13 gates viram um **adapter** que mapeia 50→13 p/ back-compat de CI). **T9.3** matriz de mapeamento 50-dim → 13-gate documentada.

#### **P10 — Surface: subcommand + CI + MCP** (W4 · R4) · **M** · LOW-MED
- **T10.1** `touring quality …` subcommand (shim em touring-server; process-spawn OR dep feature-gated — manter standalone). **T10.2** `ci.yml`: passo `--scope feature --new-code <diff> --fail-below 0.95` (BLOCK novo) + `--scope workspace --fail-below 0.80` (advisory). **T10.3** MCP `quality_score_scope`.

#### **P11 — full-review integration + observabilidade + docs** (W4 · R4) · **M** · LOW
- **T11.1** harness emite o layout `.full-review/` (00-scope..05-final) por escopo; a skill consome scores objetivos. **T11.2** `gate-metrics` counters (`scope_score_count`, `blockers_by_scope`, `cayc_block_count`). **T11.3** co-evolução: D-rules cross-ref de escopo, ARCHITECTURE/modules regen.

### ▛ QUICKACTIONS (atômicas, S, começáveis JÁ — sem dependência cruzada)

| QA | Ação (1 comando/edit) | Fase | Dep |
|---|---|---|---|
| **QA1** | `perfect-create src/scope_context.rs` (struct vazia + walk-up resolvers) | P0 | — |
| **QA2** | add `ratio: Option<(u64,u64)>` a `DimScore` (serde-skip) | P0 | — |
| **QA3** | add `check_scope` default-method ao trait `Verification` | P0 | QA1 |
| **QA4** | `perfect-create src/criteria.rs` com `DimContract` + 6 BLOCK rows | P5 | — |
| **QA5** | substituir `aggregate.rs:126` placeholder por `agg_coverage_ratio` (usa `.ratio`) | P4 | QA2 |
| **QA6** | golden-snapshot test dos 33 roll-up dims (fixture fixa) | P0 | — |
| **QA7** | `ci.yml`: add passo advisory `touring-quality score . --scope workspace --fail-below 0.80` | P10 | — |
| **QA8** | F3.11 readme `check_scope` → localizar README.md em `cx.repo_root` (1º artefato fiel, prova o padrão) | P1 | QA3 |

QA1–QA3+QA6 desbloqueiam tudo (são P0). QA8 é o "primeiro verifier fiel" — prova o padrão de P1 end-to-end.

---

## §5 — Unificação: 50-dim como fonte-da-verdade dos **QUATRO** harnesses (rev. 2 — achado de arquitetura)

> **Correção de escopo (exploração 2026-06-21)**: não são 3 mas **quatro** costuras, e a **mais importante NÃO é o CI** — é o **CEG X7** (runtime/L3). O harness não é só um relatório; ele já é parte do **meta-loop de execução** que gateia auto-mutação.

### 5.1 As quatro costuras (a topologia real, lida do código `[FACT 1.0]`)

| # | Seam | Onde (código) | O que faz hoje | Pós-unificação |
|---|---|---|---|---|
| **S1 — Runtime gate (L3, o mais importante)** | `ceg::gateway::harness_extension` (X7) | consome `touring_harness` (17-dim); **BLOQUEIA Edit/Write/MultiEdit se `tier < Gold`** via `harness_block_for_tool()→HarnessVerdict` | X7 consome o **50-dim por escopo** (`File` no arquivo tocado); BLOCK fail-closed nas 6 P0 + tier<Gold |
| **S2 — Meta-loop self-mutation** | `ceg::gateway::{harness_metric::HarnessQuality, change_contract::ChangeContract}` (R5/R8) | `HarnessQuality` = **6 dims de saúde do SISTEMA** (executable·inspectable·stateful·governed·performant·evolving), NÃO qualidade de código; `ChangeContract` só commita se não regredir essas 6 | o composite 50-dim **alimenta** `HarnessQuality.governed` (input), **não o substitui** — ver **§11.4** (correção factual desta rev.) |
| **S3 — CI release gate** | `docs/elite_aggregate.py` (13 gates Python) | re-mede com proxies workspace-level | vira **adapter 50→13**: lê o `gate-json` (`--scope workspace`) |
| **S4 — Narrativa agent-orchestrated** | skill `comprehensive-review:full-review` | re-deriva F1.x–F4.x do zero | **consome** o `gate-json` por escopo → foca em remediação |

### 5.2 Topologia (uma fonte, quatro consumidores)

```
                 touring-quality (50 dims, Rust)  ←★ FONTE DA VERDADE ★
                   score <t> --scope <k> [--new-code <g>] --format gate-json
                                   │
        ┌──────────────┬───────────┴───────────┬──────────────────────┐
        ▼              ▼                         ▼                      ▼
   S1 CEG X7      S2 meta-loop            S3 elite_aggregate      S4 full-review
   harness_       HarnessQuality +        (adapter 50→13)         (narrativa)
   extension      ChangeContract           02_arch←F1.7/8/12        Phase1←F1.x
   BLOCK Edit/    (commit só se            05_test←F3.1..7          Phase2←F2.x
   Write se       não-regride o            14_craft←F1.1/2/5        Phase3←F3.x
   tier<Gold      composite 50-dim)        17_docs←F3.8..13         Phase4←F4.x
        ▼              ▼                         ▼                      ▼
   RUNTIME (L3)   META-LOOP (L4)          CI release (Diamond)    remediação humana
```

### 5.3 `touring-harness` (17-dim) — projeção curada das 50, não harness rival

Os 17 gates (architecture · security · performance · testing · documentation · best-practices · ci/cd · modularization · scalability · extensibility · **naming · navigability** · craftsmanship · dependencies · UX · product-docs) são **agrupamentos grossos das 50 dims**. Unificação: `touring-harness::EliteScore` passa a ser computado como **projeção determinística** do `ScopeReport` 50-dim (mapa 50→17), preservando a API que o CEG X7 já consome (zero churn no gateway). Os 2 gates "extra" do 17-dim sem dim 1:1 (naming, navigability) viram dims derivadas (naming ⊂ F1.2 maintainability; navigability ⊂ F1.7 boundaries + F3.8 inline-doc) ou novas micro-dims documentadas.

### 5.4 Implicação no plano: **nova Phase P12 (CEG integration)** na Wave W4 / Roadmap R4

- **P12 — Harness seam unification** · **L** · **MEDIUM**. **T12.1** `ScopeReport → EliteScore` projeção 50→17 (back-compat da API `touring-harness`). **T12.2** `HarnessQuality` deriva do composite 50-dim por escopo. **T12.3** X7 `harness_extension` chama `touring-quality check --scope file` no arquivo tocado (BLOCK 6 P0 + tier<Gold) — fail-open se o motor faltar (CEG é fail-open por design). **T12.4** `ChangeContract` usa o delta 50-dim (commit só se não-regride). **Risco**: latência X7 no hot-path de Edit/Write → mitigar com cache content-keyed (igual ao F-4 wiring memoization) + medir P99.
- DAG: P12 ← P9 (precisa do `gate-json`). Wave W4 ganha P12. **DoD-7 (novo)**: o CEG X7 gateia Edit/Write no score 50-dim por escopo, com P99 ≤ baseline.

> **Por que isso importa**: hoje o runtime gateia com o 17-dim (`touring-harness`) e o CI com 13 proxies Python — três medidas divergentes da mesma coisa. Pós-unificação há **uma** medida (50-dim, por escopo), consumida em **runtime (S1)**, **meta-loop (S2)**, **CI (S3)** e **narrativa (S4)**. O harness deixa de ser "um relatório" e vira o **substrato de governança L3+L4** que o `architecture.md` já descreve.

---

## §6 — Gate-verdict machine-readable (análogo ao webhook SonarQube)

`touring-quality score <t> --scope <k> [--new-code <g>] --format gate-json`:

```json
{
  "scope": {"kind": "workspace", "root": ".", "file_count": 1477, "total_loc": 538000},
  "tier": "Diamond", "composite": 0.9703,
  "conditions": [
    {"dim":"F2.4","metric":"secret_hits","operator":"==","threshold":0,"value":0,"on_new_code":true,"status":"OK","enforcement":"BLOCK"},
    {"dim":"F3.1","metric":"coverage","operator":">=","threshold":0.80,"value":0.84,"on_new_code":true,"status":"OK","enforcement":"WARN"},
    {"dim":"F1.3","metric":"dup_density","operator":"<=","threshold":0.03,"value":0.012,"on_new_code":true,"status":"OK","enforcement":"WARN"},
    {"dim":"F1.8","metric":"scc_count","operator":"==","threshold":0,"value":0,"status":"OK","enforcement":"ADVISORY"}
  ],
  "gate_status": "OK",
  "blockers": [], "warnings": [], "schema_version": 2
}
```

Consumível por CI (exit code), MCP (`quality_score_scope`), e a skill `full-review` (verdict → narrativa). **Cada condição é objetiva, clara, mensurável e auditável.**

---

## §7 — Gate stack de validação (real-exit) + dogfood por wave

```bash
cargo fmt -p touring-quality --check
cargo check --workspace --message-format=short            # exit 0
cargo clippy --workspace --all-targets -- -D warnings     # exit 0
cargo test -p touring-quality                             # verdes + golden snapshots + 50-dim invariant test
for d in F2.1 F2.4 F2.5 F2.6 F4.3 F4.5; do touring-quality check --gate $d --target . --scope workspace; done
python3 docs/elite_aggregate.py --check                   # Diamond 0.9703 NÃO regride
python3 docs/file_size_gate.py --check
# DOGFOOD (prova de fidelidade da wave):
touring-quality score crates/touring-server --scope crate --format gate-json   # condições objetivas, 0 placeholder
touring-quality score . --scope feature --include 'crates/touring-quality/**' --new-code 'crates/touring-quality/src/criteria.rs'
```

Mecânica REGRA #14: `perfect-create` (módulos novos) → conteúdo via temp `.txt` (heredoc + `sed -n` verbatim) → `cp`; `perfect-edit` (edits); `Edit` p/ inserts de 1 linha.

---

## §8 — Riscos & mitigações

| # | Risco | Prob/Imp | Mitigação |
|---|---|---|---|
| R1 | grafo interno (P3) erra arestas → ciclo falso/perdido | MED/HIGH | piso conservador + daemon `wiring cycles -j` quando vivo + invariante acíclico (espera 0 em workspace) |
| R2 | seam (P0) regride os 33 / composite | LOW/HIGH | `default check_scope` = comportamento exato; golden snapshot; Diamond gate por wave |
| R3 | `DimScore.ratio`/`gate-json` quebram consumidores | LOW/HIGH | serde-skip + `schema_version++`; aditivo |
| R4 | `touring quality` (P10) infla build/acopla | MED/MED | standalone preservado; subcommand = spawn OR feature-gated; decidir no P10 |
| R5 | herança ScopeNative mislabel | LOW/MED | walk-up ao artefato mais próximo; rótulo com path; teste de cadeia |
| R6 | unificação (P9) altera o Diamond | LOW/HIGH | adapter 50→13 calibrado p/ reproduzir 0.9703 antes do cutover; rollback = manter os 13 proxies |
| R7 | thresholds (§2) arbitrários/calibração | MED/MED | ancorados nas D-rules + Sonar way; `--weights`/threshold-file configurável; calibrar contra o próprio workspace (já Diamond) |

---

## §9 — Auto-validação do plano

1. **Atômico & shippable?** ✅ cada Wave entrega uma fatia funcionando (W1: 33 dims+contrato; W2: 50 fiéis; W3: gates; W4: unificação). Cada QuickAction é S e independente. Nenhuma dim dropada em fase incompleta.
2. **Dependências explícitas & acíclicas?** ✅ DAG: P0→{P1,P2,P3,P4}; P5⫫P0; P6←P5; P7,P8←P0..P4; P9←P8; P10,P11,P12←P9 (P12 = CEG X7 seam, consome o `gate-json`). Roadmaps (vertical) × Waves (horizontal) sem ciclo.
3. **Estimativas realistas?** ✅ esforço real concentrado em P3 (grafo) e P9 (unificação); P1/P2/P4 são preenchimentos uniformes guiados pelas D-rules + a tabela §2.
4. **Riscos mitigados?** ✅ 7 riscos, cada um com mitigação concreta (R1/R6 são os críticos, cobertos).

---

## §10 — Critérios de sucesso (mensuráveis — Definition of Done do Master Plan)

| # | Critério objetivo | Como medir |
|---|---|---|
| DoD-1 | 50 dims fiéis em todo escopo, 0 placeholder | grep por `read_target_source` nos 17 ScopeNative == 0; cada `check_scope` testado |
| DoD-2 | condição objetiva por dim avaliada | `--format gate-json` emite 50 conditions com status |
| DoD-3 | Clean-as-You-Code ativo | `--scope feature --new-code` reprova só no novo; teste com fixture regressiva |
| DoD-4 | unificação | `elite_aggregate` lê o gate-json; Diamond 0.9703 reproduzido |
| DoD-5 | 0 regressão | check/clippy/test/fmt/elite verdes a cada wave |
| DoD-6 | full-review consome scores | `.full-review/` gerado a partir do harness, não re-derivado |

---

_Master Plan exaustivo — o scaffold (Thrust B) é o esqueleto; este plano é o corpo + o sistema nervoso: uma costura (P0) + contrato objetivo (P5) + 21 verifiers fiéis (P1-P4) + ladder/dual-measure/gates (P6-P8) + unificação dos três harnesses (P9-P11), cada passo aditivo, validado por exit-code real, dogfooded, com critério objetivo/claro/mensurável por dimensão e por escopo, e sem jamais dropar uma dimensão. Premium de Elite de Mercado._

---

## §11 — Rev.3: rodada de exploração profunda (verifier-correctness G10 + correções) `[FACT 1.0]`

> Leitura direta dos verifiers + `touring-harness/gate.rs` + CEG `harness_extension`/`change_contract`/`harness_metric` + `touring-analysis`. O workflow de 8 agents spawned bateu no **rate-limit de servidor** (padrão documentado) → exploração self-served por tool-calls diretos do main loop.

### 11.1 — GAP NOVO **G10: correção da medida-core de ~8 verifiers**

A §2 assumiu que cada verifier computa seu critério objetivo. O código revela ~8 com proxy/placeholder/**bug semântico**:

| Verifier | O que mede HOJE (código real) | Deveria (D-rule/§2) | Sev | Ação |
|---|---|---|---|---|
| **f1_8 dep-cycles** | `1 - count("mod "/"use crate::")*0.1` — **penaliza ter imports** | #SCC≥2 Tarjan `==0` | 🔴 | P3 — e é **ativo-errado**: pune código bem-conectado |
| **f3_1 coverage** | `tests/(loc/50)` — **test-density**, sem line-coverage | covered/total ≥80% | 🔴 | nova task: cargo-llvm-cov; até lá §2 honesto = "test-density" |
| **f4_3 deprecated** | conta `#[deprecated` **definições** (pune deprecar bem!) | **#USOS** de API deprecada `==0` | 🔴 | **bug semântico** — inverter |
| f3_8 inline-doc | `doc_lines/fn_count` | pub-documentado/total | 🟡 | P4 (CoverageRatio) + escopo a pub |
| f1_6 error-handling | pune `.expect(` = unwrap + conta unwraps de teste | 0 unwrap **PROD** | 🟡 | excluir `#[cfg(test)]` + não punir expect justificado |
| f1_1 complexity | keyword-count por-arquivo | per-fn LOC-weighted | 🟡 | per-fn na faithful pass |
| f1_3 duplication | só intra-arquivo | cross-file density | 🟡 | cross-scope dup (mais fundo que §2) |
| f4_5 pkg-mgmt | `= "` super-conta linhas não-dep do Cargo.toml | só `[dependencies]` | 🟡 | parsear a seção |
| f2_1/f2_4 | `hits==0?1:0` worst-of + self-FP allowlist | ✓ batem | ✅ | — |

→ **Nova Wave W0 — Verifier Correctness** (precede W2; paraleliza W1). **Phase P-1** (consertar a medida-core ANTES de torná-los scope-native — não adianta agregar fielmente uma medida errada). S por verifier, pipeline 1-cada; os 🔴 (f1_8/f3_1/f4_3) primeiro. **DoD-8**: cada verifier mede o que sua D-rule define (teste com fixture conhecida: f4_3 num arquivo que USA API deprecada → <0.5; num que DEFINE `#[deprecated]` → 1.0).

### 11.2 — Correções da §2 (thresholds que divergem do código real)

- **F3.1 coverage**: o motor **não** mede line-coverage (sem llvm-cov); §2 "≥80%" só é real após a task de integração. Até lá a medida honesta é **test-density**.
- **F4.3 deprecated**: critério correto = **"#usos de API deprecada == 0"** (o código atual conta definições — invertido).
- **F1.8 dep-cycles**: "==0 SCC" só vale **pós-P3**; hoje é proxy import-count.
- **F3.8 inline-doc**: **pub-documentado/total** (não doc-lines/fn).

### 11.3 — Mapa preciso **16-gate (touring-harness) → 50-dim** (seam S1)

`GateId` real (`gate.rs`) = **16 gates** (lib.rs diz "17" — discrepância a auditar em P12):

| Gate (17-dim) | Projeta de (50-dim) |
|---|---|
| Architecture | F1.7 F1.8 F1.9 F1.10 F1.11 F1.12 |
| Security | F2.1 F2.2 F2.3 F2.4 F2.5 F2.6 |
| Performance | F2.7 F2.8 F2.9 F2.10 F2.11 F2.12 F2.13 |
| Testing | F3.1 F3.2 F3.3 F3.4 F3.5 F3.6 F3.7 |
| Documentation | F3.8 F3.9 F3.12 |
| BestPractices | F4.1 F4.2 F4.4 |
| CiCdDevops | F4.7 F4.8 F4.9 |
| Modularization | F1.7 F1.8 |
| Scalability | F2.13 |
| Extensibility | F1.11 F4.4 |
| **Naming** | F1.2 (sem dim 1:1 — deriva de maintainability) |
| **Navigability** | F1.7 + F3.8 (sem dim 1:1 — boundaries + inline-doc) |
| Craftsmanship | F1.1 F1.2 F1.5 |
| Dependencies | F1.8 F2.5 F4.5 |
| **Ux** | meta-CLI (sem dim 1:1 — `ux_audit.py`: completions/help) |
| ProductDocs | F3.10 F3.11 F3.13 |

→ T12.1 (`ScopeReport → EliteScore` projeção): para cada gate, `min`/`weighted-mean` das dims-fonte conforme o AggKind; naming/navigability/ux viram dims derivadas documentadas.

### 11.4 — Correção da **§5.1 / S2** (erro factual desta rev.2)

`HarnessQuality` (consumido por `ChangeContract`) **NÃO são as 50 dims de qualidade de código** — são **6 dims de saúde do META-LOOP**: `executable · inspectable · stateful · governed · performant · evolving` (composite = média aritmética; `harness_metric.rs`). Eu os conflei. Correção: o composite 50-dim **alimenta** `HarnessQuality.governed` (o input "toda execução passa por um policy gate"), **não o substitui**. O eixo de qualidade-de-código é o **S1** (17-dim, `harness_block_for_tool`), onde o 50-dim de fato entra.

### 11.5 — P12 API precisa (specs T12.x)

```rust
// ceg::gateway::harness_extension — o que P12 deve alimentar com o 50-dim
pub fn harness_block_for_tool(tool: &str, payload: &str,
    agent_id: Option<&str>, model: Option<&str>) -> HarnessVerdict;
pub enum HarnessVerdict {
    Allow { composite: f32, tier: String, badge: String },        // tier ≥ Gold
    Deny  { composite: f32, tier: String, canonical_fix: String },// tier < Gold → BLOCK
    PassThrough,                                                   // tool ∉ FILE_MUTATION_TOOLS
}
```
**T12.1**: substituir o cômputo interno do composite 17-dim por `projeção(touring-quality score <arquivo-tocado> --scope file)` → mesma `HarnessVerdict`, zero churn nos callers do X7. **T12.3**: fail-open se o motor 50-dim faltar (CEG é fail-open). **Risco**: latência no hot-path Edit/Write → cache content-keyed (igual F-4) + P99 guard.

### 11.6 — P3/G7 RESOLVIDO: reuso de `touring-analysis`

`touring-quality` **já declara** `touring-analysis` (optional, feature `workspace-integration`). Mas a API é **DB-backed** (`wiring::analyze_chains(conn: &rusqlite::Connection) -> ChainResult`, `blast_radius` BFS/HNSW sobre o grafo no SQLite). Logo o tiered de P3 fica: **(floor)** grafo file-set interno (sempre disponível, standalone) → **(enrich)** `touring-analysis::wiring`/`blast_radius` quando a feature `workspace-integration` + DB estão presentes → **(USP)** `touring wiring cycles -j` (Tarjan real do daemon) quando o socket vive. **Sem dep nova** (D44). G7 fechado.

### 11.7 — Subsistemas restantes (A2/A6/A7, cobertura leve)

- **generator/VGP** (L2): 36 kinds, typestate Draft→Verified→Rendered→Speculated→Committed; VGP verifica símbolos vs índice. **Oportunidade**: o harness pode **acionar generators de auto-remediação** por dim < threshold (o `taco-forge perfect-quality-*` PLANNED W7 vira real aqui) — anotar como Roadmap futuro R5 (auto-remediation).
- **offensive** (Cap II: erickson/solver/vuln): concolic + SMT + bug-bounty → poderia **feedar F2.1/F2.2** (injection real via solver, não string-match) — R5/elite.
- **full-review format** (P11): `.full-review/{00-scope,01-quality-architecture,…,05-final-report}.md` + `state.json` (phase verdicts + running_totals + remediations_applied) — o `ScopeReport → .full-review/` layout (T11.1) mapeia phase N ← Fase N das 50 dims.

### 11.8 — Atualização do DAG/Waves

Nova **W0 (Verifier Correctness, P-1)** precede W2; **Roadmap R0 — Verifier Fidelity** (consertar medida-core) acima de R1 (que torna scope-native). DoD-8 adicionado. Os 🔴 (f1_8/f3_1/f4_3) são o caminho crítico de credibilidade — sem eles, as condições objetivas da §2 são medidas erradas com precisão.

---

## §12 — Rev.4: auditoria de efetividade → GAP NOVO G11 (badge inflation / teatro) `[FACT 1.0]`

Auditoria empírica dos 6 harnesses (`docs/2026-06-21-harness-effectiveness-audit.md`): rodei cada um. **Veredito: NÃO convergem** — 3 composites dão Bronze(0.66)/Silver(0.76)/Diamond(0.97) no MESMO código.

**G11 — Badge inflation (teatro)**: os badges Diamond são parcialmente ilusórios.
- `touring-elite` (17-gate): **9 de 15 gates = "external CI step (assumed PASS)" = 1.00 sem medição**; Diamond 0.9774 numa mudança VAZIA.
- `elite_aggregate` (13-gate): **testing+modularization = file-size proxy** (`# proxy: file size = testability`); **security+deps = N/A constante 1.0** (peso 1.5 cada).
- CEG X7 17-dim gate: **não-wired** (`ceg_blocked_count=0`); o runtime BLOCK real é `touring-quality-block-all.sh` (50-dim 6 P0), mas inclui o f4_3 invertido (G10).

→ **Nova Roadmap R0.5 — De-theatralization** (paralela a R0 verifier-correctness, antes de declarar qualquer Diamond crível):
- **P-2a**: 13-gate `05_testing`/`09_modularization` deixam de usar file-size → ligam ao 50-dim (F3.1-7 / F1.7-8).
- **P-2b**: 13-gate `03_security`/`15_dependencies` deixam de ser N/A-1.0 → ligam ao 50-dim F2.5/F4.5 (que já leem o manifest).
- **P-2c**: `touring-elite` 17-gate deixa de assume-PASS → projeção do 50-dim (§11.3 mapa 16→50).
- **P-2d**: CEG X7 — wirar `harness_block_for_tool` (alimentado pelo 50-dim) OU remover (REGRA #0 dead code).
- **DoD-9**: rodar os 3 composites no mesmo alvo → divergência ≤ 0.05 (convergência por construção via fonte única).

**Implicação estratégica**: a unificação (P9-P12) não é só elegância — é o que torna os badges CRÍVEIS. Enquanto testing=file-size e security=N/A, "Diamond 0.9703" não é uma alegação Premium-Elite defensável perante um auditor de mercado (SonarQube exige medição real de coverage/testing).
