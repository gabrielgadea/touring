# Touring Quality Harness — Multi-Scope Architecture: Diagnóstico Exaustivo

> **Data**: 2026-06-21 | **Autor**: TACO (Gabriel Gadea, comandante) | **Nível**: L4 (arquitetura)
> **Crate alvo**: `crates/touring-quality` (50-dim elite scoring engine, v0.1.0)
> **Baseline**: `cargo check --workspace` = **exit 0** `[FACT 1.0]` (5m10s, 2026-06-21)
> **Objetivo**: tornar as **50 dimensões** aplicáveis e fiéis em **toda** granularidade —
> arquivo · path · feature · crate · repositório · projeto · sistema · workspace —
> sem dropar nenhuma dimensão nem nenhum atributo (da arquitetura à documentação).

---

## 0. Sumário executivo + veredito

O harness `touring-quality` é **estruturalmente file-scoped**. Existe um modo "workspace"
(`--workspace`), mas ele é **infiel**: em vez de pontuar cada arquivo e agregar, ele
**concatena todo o source num blob (ordenado, com teto de 2 MiB) e roda cada detector-de-arquivo
uma única vez sobre o blob**. Isso produz quatro modos de falha mensuráveis (§3) — incluindo um
**buraco de segurança** (segredo após 2 MiB passa numa dimensão BLOCK).

**Veredito** `[FACT 1.0]`: o motor de *score por dimensão* e o *composite + tier* estão corretos e
são reutilizáveis. O que falta é uma **camada de escopo + agregação**: enumerar o file-set do
escopo → pontuar cada arquivo → **agregar por `AggregationKind` (uma por dimensão)** → computar
dimensões `ScopeNative` uma vez sobre o grafo/artefato do escopo. Isso é **aditivo** (não quebra
`score_target` de arquivo único), fiel ao modelo de mercado (SonarQube *measures* + *Clean as You
Code*), e **preserva as 50 dimensões em todo escopo** — nenhuma é descartada; as de granularidade
de repositório são *herdadas* na granularidade fina, não ignoradas.

---

## 1. Baseline (FASE 0 — gate)

| Gate | Resultado | Evidência |
|---|---|---|
| `cargo check --workspace` | **exit 0** | `Finished dev profile in 5m10s; CARGO_CHECK_EXIT=0` |
| Crate `touring-quality` | compila; 30+ testes lib | `lib.rs` 839 LOC + 50 verifiers + `composite.rs` + `tier.rs` |
| `#![cfg_attr(not(test), deny(clippy::unwrap_used))]` | ativo | invariante elite do próprio harness (lib.rs:34) |

> Daemon `touring` em estado `degraded` (race espúria de SessionStart, 6/6 componentes no banner —
> REGRA #19: **não** `pkill`; recupera sozinho). Discovery foi self-served por tool-calls diretos
> (sub-agents sofreram *rate-limit de servidor*, padrão documentado).

---

## 2. Arquitetura atual (ground truth — `[FACT 1.0]`, arquivos lidos)

### 2.1 Fluxo de score (single-target)

```
score_target(&Path, dims, format)            # lib.rs:570
  └─ score_target_impl                        # rayon par_iter sobre as dims pedidas
       └─ verifications::run_verification(dim, target)   # mod.rs:333 — dispatch table OnceLock<HashMap<DimId,Box<dyn Verification>>>
            └─ <FX_Y>::check(&self, target: &Path) -> Result<DimScore>   # trait Verification (mod.rs:62)
  └─ QualityReport::build(target, dims)        # lib.rs:495
       └─ compute_composite(dims, default_weights())     # composite.rs:32 — média ponderada
       └─ tier_from_composite(composite)       # tier.rs — 6-tier (Diamond..Unranked)
```

- `compute_composite` = **média ponderada**: `Σ(valor·peso)/Σpeso`, clamp [0,1]. Pesos por
  enforcement: **BLOCK 2.0 · WARN 1.5 · ADVISORY 1.0** (`composite.rs:13`). **Correto e reutilizável.**
- `DimStatus::from_score`: ≥0.8 Pass · ≥0.5 Warn · <0.5 Fail.
- 6 BLOCK dims (`lib.rs:290`): **F2.1 F2.4 F2.5 F2.6 F4.3 F4.5**; 13 WARN; 31 ADVISORY.

### 2.2 O "modo workspace" atual — concatenação

`bin/touring-quality.rs:122` →
```rust
let target_path = if workspace { std::env::current_dir()? } else { target };
let report = score_target(&target_path, &dim_filter, fmt)?;   // passa o DIR como target único
```

`verifications::read_target_source` (`mod.rs:110`) trata um diretório assim:
- caminhada recursiva determinística (entries `sort()`), pulando `SKIP_DIRS` + dot-dirs;
- concatena todo arquivo com extensão em `SOURCE_EXTS` (23 linguagens) numa **única String**;
- **teto `DIR_SCAN_BYTE_CAP = 2 MiB`** → retorna cedo ao atingir.

Ou seja: **o "score de workspace" é o score de um detector-de-arquivo rodando sobre um blob
concatenado e truncado.** Não há (a) enumeração do file-set como conjunto, (b) score por arquivo,
(c) roll-up, nem (d) passo nativo de escopo.

---

## 3. Diagnóstico — os 4 modos de falha da concatenação `[FACT 1.0]`

| # | Modo de falha | Consequência | Dimensões afetadas |
|---|---|---|---|
| **A** | **Soma de complexidade** | CC/cognitive de um blob de N arquivos é a soma — não a distribuição. Um workspace "saudável" parece catastrófico. | F1.1, F1.2, F1.4, distribuição em geral |
| **B** | **Teto de 2 MiB (truncamento silencioso)** | Workspaces grandes (o `touring` tem ~538k LOC) são truncados na ordem `sort()`. **Um segredo/CVE/injeção após o byte 2 Mi passa.** Numa dimensão **BLOCK** isso é um **buraco de segurança fail-open**. | F2.1, F2.4, F2.5, F2.6, F4.3 (BLOCK) |
| **C** | **Ratios destruídos** | Cobertura = coberto/total; concatenar perde os denominadores por arquivo → o ratio vira ruído. | F3.1, F3.8, F3.9, F1.3 (densidade) |
| **D** | **Dimensões scope-blind + self-FP** | Ciclos de dependência são *entre* arquivos (perdidos no blob). Artefatos de repo (README, CI, CHANGELOG, Cargo.toml) não são arquivos-fonte. E ao pontuar o próprio crate, os detectores casam suas **próprias pattern-strings** (self-FP). | F1.8, F1.12, F2.13, F3.3/6/7/10/11/13, F4.5-12 |

**Causa-raiz única**: *concatenar-depois-pontuar* trata um escopo como um arquivo gigante. O correto
é *pontuar-cada-arquivo-depois-agregar* + *computar o nativo-de-escopo uma vez*.

---

## 4. Grounding de mercado (context7 — `[FACT 1.0]`, `/websites/sonarsource_sonarqube-server`)

O modelo de agregação multi-escopo de elite (SonarQube) confirma a taxonomia abaixo:

1. **Overall code vs New code** — *"For most metrics, SonarQube calculates two values: one for the
   overall code and one for new code"* (metrics-definition). **New code** = *leak period* /
   changeset (**Clean as You Code**). → mapeia direto para nosso **escopo `Feature`** (git-free =
   path-glob/changeset). Todo relatório tem dois modos: escopo-cheio + escopo-feature.
2. **Ratings worst-of** — `new_security_rating`/`new_reliability_rating` agregam pelo **pior
   componente** (o rating do projeto é o do pior arquivo). → kind **WorstOf** para segurança.
3. **Coverage % + duplicated-lines-density** — quality gate falha em *"Coverage < X%"* e *"Duplicated
   lines density > X%"*: métricas de **numerador/denominador somados** por nível, depois ratio. →
   kind **CoverageRatio**.
4. **Quality gate = condição project-level-only** (sobre *new code*). → análogo a **ScopeNative**.

Corroboração `[training-data 0.9]` (não-context7, citado como reforço):
- **CodeScene** (Adam Tornhill) — *hotspots* = complexidade × frequência-de-mudança, roll-up
  **ponderado por tamanho/churn**, não média plana → justifica **WeightedLoc** (LOC-weighted) sobre `Mean`.
- **OpenSSF Scorecard** — sinais que só existem em **escopo de repo** (CI, branch-protection, SBOM,
  pinned-deps) → exatamente nosso **ScopeNative**.

---

## 5. Design — harness multi-escopo (todas as 50 dims, toda granularidade)

### 5.1 Invariante constitucional (requisito de Gabriel, 2026-06-21)

> **Nenhuma dimensão é jamais descartada em nenhum escopo.** As 50 dimensões — arquitetura,
> qualidade, segurança, performance, testes, **documentação**, CI/CD — aparecem em **todo**
> relatório, em **toda** granularidade. Dimensões `ScopeNative` numa granularidade abaixo do
> repositório são **herdadas** do artefato de repo mais próximo (rotuladas *"inherited from <scope>"*),
> nunca omitidas. Todo parâmetro/atributo de cada verifier é considerado na granularidade onde é
> mensurável e propagado para as demais.

### 5.2 `ScopeKind` (8 variantes) → cobre os 7 pedidos

| ScopeKind | Resolução | Pedido de Gabriel |
|---|---|---|
| `File` | o arquivo | **arquivo individual** (= `score_target` atual, intacto) |
| `Path` | subtree de um dir | **um path** |
| `Feature` | root + `--include`/`--exclude` globs (changeset git-free; análogo *Clean as You Code*) | **uma nova feature** |
| `Crate` | dir com `[package]` (file-set + `Cargo.toml` daquele crate) | (sistema unitário) |
| `Repo` | dir com `.git`/`README` na raiz (artefatos de repo ativos) | **um repositório** |
| `Project` | ≈ `Repo` ou conjunto nomeado de membros | **um projeto** |
| `System` | conjunto coerente de crates (`--include` multi-crate) | **um sistema** |
| `Workspace` | raiz Cargo `[workspace]`; todos os membros; ciclos/boundaries workspace-wide | **workspace inteiro** |

4 estratégias de resolução por baixo (arquivo / subtree / manifest-bounded / glob-set); `Project` e
`System` reusam a mesma maquinaria (aliases honestos), então as 8 variantes cobrem as 7 palavras
sem buraco.

### 5.3 `AggregationKind` (como N scores-de-arquivo viram 1 score-de-escopo, por dimensão)

| Kind | Fórmula | Quando | Evidência reportada |
|---|---|---|---|
| **WorstOf** | `min(valorᵢ)` | "qualquer violação reprova" — segurança worst-of (SonarQube ratings) | pior arquivo + valor |
| **WeightedLoc** | `Σ(valorᵢ·locᵢ)/Σlocᵢ` | distribuição (god-files pesam mais; não são lavados) | pior arquivo + p10 (pior decil) |
| **CoverageRatio** | `Σcobertoᵢ/Σtotalᵢ` (v1: ≈ LOC-weighted) | numerador/denominador (coverage, doc-coverage, densidade de dup) | ratio + maiores lacunas |
| **ScopeNative** | computado **1×** sobre o grafo/artefato do escopo | ciclos, boundaries, README/CI/CHANGELOG/Cargo.toml | o artefato inspecionado |
| **Mean** | média simples | fallback p/ dim não-classificada (durabilidade F5+) | — |

> **WorstOf fecha o buraco do truncamento (modo B)**: cada arquivo é pontuado, nenhum é pulado por
> teto de bytes → um segredo em *qualquer* arquivo zera aquele arquivo → `min` zera o escopo.

### 5.4 Pipeline de score de escopo

```
score_scope(&Scope, dims) -> ScopeReport
  ├─ files = Scope.resolve(target, kind, include, exclude)   # enumera o file-set (SEM teto de bytes)
  ├─ per_file = rayon( files × dims_non_scope_native )       # score por arquivo (flat parallel)
  ├─ for dim in dims:
  │     match dim.agg_kind():
  │        ScopeNative => run_verification(dim, scope.root) 1× (ou herda no sub-escopo)
  │        _           => aggregate(kind, per_file[dim], loc)
  ├─ compute_composite(dims_agg, default_weights())          # REUSA o composite existente
  └─ tier_from_composite(...)                                # REUSA o tier existente
```

`ScopeReport { scope_kind, root, file_count, total_loc, dimensions: BTreeMap<DimId,DimScore>,
per_dim_worst_files, composite, tier, blockers, warnings, suggestions }` — **sempre 50 dims**.

### 5.5 CLI

```
touring-quality score <target> --scope file|path|feature|crate|repo|project|system|workspace \
                                 [--include <glob>] [--exclude <glob>] [--format json|html|badge|compact] \
                                 [--fail-below 0.80]
touring-quality score --workspace       # alias de --scope workspace (back-compat)
touring-quality check  --gate <Fx.y> --target <t> --scope <kind>   # 1 dim, qualquer escopo
```

Auto-detect quando `--scope` ausente: file→`File`; dir `[workspace]`→`Workspace`; `[package]`→`Crate`;
`.git`/README→`Repo`; senão `Path`.

---

## 6. Mapeamento COMPLETO das 50 dimensões → AggregationKind (`[FACT 1.0]` — todas presentes)

> Verificado: 50/50 atribuídas, 0 ausentes. Distribuição: **WorstOf 6 · CoverageRatio 4 ·
> WeightedLoc 23 · ScopeNative 17**. As 6 BLOCK preservadas fail-closed em todo escopo.

### Fase 1 — Code Quality & Architecture (12)

| Dim | Nome | Kind | Tratamento por escopo |
|---|---|---|---|
| F1.1 | complexity | WeightedLoc | distribuição CC/cognitive ponderada por LOC; pior-fn surfaced |
| F1.2 | maintainability | WeightedLoc | fn-length/naming/magic-numbers ponderado |
| F1.3 | duplication | CoverageRatio | densidade dup = dup-lines/total (SonarQube) |
| F1.4 | solid | WeightedLoc | violações SOLID por arquivo, ponderado |
| F1.5 | tech-debt | WeightedLoc | densidade de marcadores TODO/FIXME/allow |
| F1.6 | error-handling | WeightedLoc | unwrap/swallowed prod ponderado; pior arquivo surfaced |
| F1.7 | boundaries | WeightedLoc | pub-surface ratio por arquivo (+nota ScopeNative cross-module) |
| **F1.8** | **dep-cycles** | **ScopeNative** | **ciclo é ENTRE arquivos — Tarjan SCC 1× no grafo do escopo** |
| F1.9 | api-design | WeightedLoc | naming/Result-typing de pub APIs |
| F1.10 | data-model | WeightedLoc | newtype/enum/illegal-states |
| F1.11 | patterns | WeightedLoc | idiomatic patterns |
| **F1.12** | **arch-consistency** | **ScopeNative** | **layering/cross-cutting — `wiring audit` 1× no escopo (USP)** |

### Fase 2 — Security & Performance (13)

| Dim | Nome | Kind | Tratamento por escopo |
|---|---|---|---|
| **F2.1** | **owasp** ⛔ | **WorstOf** | injeção em qualquer arquivo → escopo Fail |
| F2.2 | input-validation | WorstOf | sink sem allowlist = defeito worst-of |
| F2.3 | authz | WorstOf | checagem ausente = breach worst-of |
| **F2.4** | **secrets** ⛔ | **WorstOf** | **segredo em qualquer arquivo → escopo 0.0 (fecha buraco 2 MiB)** |
| **F2.5** | **dep-cves** ⛔ | **ScopeNative** | **CVE no `Cargo.toml`/lock do escopo (artefato manifest)** |
| **F2.6** | **config** ⛔ | **WorstOf** | debug=true/CORS-* em qualquer arquivo = pwn |
| F2.7 | db-perf | WeightedLoc | N+1 patterns ponderado |
| F2.8 | memory | WeightedLoc | unbounded/clone ponderado |
| F2.9 | caching | WeightedLoc | invalidação ponderada |
| F2.10 | io | WeightedLoc | blocking-in-async ponderado |
| F2.11 | concurrency | WeightedLoc | lock-across-await ponderado |
| F2.12 | frontend | WeightedLoc | bundle/wasm por arquivo web (NA→perfeito em backend) |
| **F2.13** | **scalability** | **ScopeNative** | **stateless/SPOF — arquitetural, 1× no escopo** |

### Fase 3 — Testing & Documentation (13)

| Dim | Nome | Kind | Tratamento por escopo |
|---|---|---|---|
| F3.1 | coverage | CoverageRatio | Σcoberto/Σtotal (SonarQube coverage%) |
| F3.2 | test-quality | WeightedLoc | assert/mutation por test-file |
| **F3.3** | **test-pyramid** | **ScopeNative** | **ratio unit:integration:e2e do escopo, 1×** |
| F3.4 | edge-cases | WeightedLoc | boundary/property tests por arquivo |
| F3.5 | test-maint | WeightedLoc | isolamento/flakiness por arquivo |
| **F3.6** | **sec-tests** | **ScopeNative** | **cobertura de testes de segurança do escopo, 1×** |
| **F3.7** | **perf-tests** | **ScopeNative** | **presença de benchmarks no escopo, 1×** |
| F3.8 | inline-doc | CoverageRatio | pub-items documentados/total |
| F3.9 | api-doc | CoverageRatio | pub-API documentada/total |
| **F3.10** | **arch-doc** | **ScopeNative** | **ADRs/diagramas do repo (artefato)** |
| **F3.11** | **readme** | **ScopeNative** | **README do repo (artefato); herdado em sub-escopo** |
| F3.12 | doc-accuracy | WeightedLoc | doc-drift/doctests por arquivo (+nota drift ScopeNative) |
| **F3.13** | **changelog** | **ScopeNative** | **CHANGELOG do repo (artefato)** |

### Fase 4 — Best Practices & CI/CD (12)

| Dim | Nome | Kind | Tratamento por escopo |
|---|---|---|---|
| F4.1 | idioms | WeightedLoc | clippy-idioms por arquivo |
| F4.2 | frameworks | WeightedLoc | framework patterns por arquivo |
| **F4.3** | **deprecated** ⛔ | **WorstOf** | API deprecada em qualquer arquivo = break futuro |
| F4.4 | modernization | WeightedLoc | edition/features modernas por arquivo |
| **F4.5** | **pkg-mgmt** ⛔ | **ScopeNative** | **dep count/EOL no `Cargo.toml` (artefato manifest)** |
| F4.6 | build-config | ScopeNative | profiles do `Cargo.toml` (artefato) |
| F4.7 | cicd | ScopeNative | workflow `.github/` (artefato repo) |
| F4.8 | deploy | ScopeNative | manifests de deploy (artefato repo) |
| F4.9 | iac | ScopeNative | terraform/k8s (artefato repo) |
| F4.10 | monitoring | WeightedLoc | tracing-vs-println por arquivo (+nota repo) |
| F4.11 | incident | ScopeNative | runbooks (artefato repo) |
| F4.12 | env | ScopeNative | env-files/vault (artefato repo + worst-of) |

> Em escopo `File`/`Path` sub-repo, as 17 `ScopeNative` aparecem com valor **herdado** do repo
> envolvente (rotulado), garantindo que **as 50 dimensões estão sempre no relatório** — exatamente o
> requisito de Gabriel.

---

## 7. Thrust A — backlog de complexidade (refatoração L5 cirúrgica)

`docs/craftsmanship_tdg_gate.py --json` `[FACT 1.0]`: **305 auditados / 166 failures**. Os top
offenders saturam em `cognitive_score = 1.0` — **a métrica satura e não discrimina** entre os piores,
e a maioria são **algoritmos coesos intrínsecos** (não tocar):

- **Deixar intactos (coesos)**: `rl/clustering/leiden.rs` + `hooks-shared/leiden.rs` (clustering),
  `rl/aco/tracker.rs`, `rl/metacognitive_pipeline.rs`, `simd/matrix.rs` + `simd/ops.rs`,
  `reasoning/focus_cache.rs`, `cortex/signal_fusion.rs`, `cortex/cache_strategy.rs`,
  `ceg/gateway/exec_pool.rs`, `hooks-shared/forbidden_patterns.rs` (tabela de padrões).
- **Candidatos a split genuíno (junk-drawer — validar por estrutura, não pelo número)**:
  `touring-server/src/cli/common.rs` (1214 LOC, nome clássico de junk-drawer),
  `server/tools_context_router.rs` (841), `hooks-core/generator_hints.rs` (1006),
  `hooks-core/aco_wiring.rs` (674) + `aco_bridge.rs` (704), `cli/repo_score.rs` (605),
  `lifecycle/file_changed/hints.rs` (931).

**Princípio (não-negociável)**: split **só** quando há múltiplas responsabilidades NÃO-relacionadas
co-localizadas (coesão melhora de verdade); algoritmos coesos ficam intactos. Cada split é validado
por `cargo check`/`clippy`/`test` **e** pelo próprio `score_scope` (dogfooding: pontuar o path antes
e depois, provar que a coesão de escopo subiu). O `.full-review` lista craftsmanship como **status
advisory**, não como finding — portanto Thrust A é melhoria opcional, executada cirurgicamente, nunca
regredindo o que já é elite (Diamond 0.9703).

---

## 8. Plano de implementação (aditivo, validado a cada passo)

| Arquivo | Ação | Conteúdo |
|---|---|---|
| `src/scope.rs` (novo, ~280 LOC) | criar | `ScopeKind` (8) + `Scope` + `resolve()` + auto-detect + glob matcher inline (`**`/`*`, sem dep nova — D44) + LOC count; reusa o walk de `read_target_source` refatorado em `enumerate_source_files(root)->Vec<PathBuf>` |
| `src/aggregate.rs` (novo, ~220 LOC) | criar | `AggKind` (5) + `aggregate(kind, &[(PathBuf,DimScore,loc)])->DimScore` com evidência rica (n_files, pior-arquivo, p10) |
| `src/scope_report.rs` (novo, ~200 LOC) | criar | `ScopeReport` + `score_scope(&Scope, dims)` (rayon flat + ScopeNative 1× + herança sub-repo) + render compact/json |
| `src/lib.rs` | editar | `DimId::agg_kind()` (junto a `enforcement()`); `pub mod scope/aggregate/scope_report`; reexport `score_scope`; `score_target` intacto |
| `src/bin/touring-quality.rs` | editar | `--scope`/`--include`/`--exclude`; auto-detect; rotear dir→`score_scope`, file→`score_target`; `--workspace` alias |
| `verifications/mod.rs` | editar | extrair `enumerate_source_files`; `read_target_source` passa a chamá-lo (DRY) |
| testes | criar | unit por módulo + integração com fixture-tree (segredo em 1 de 3 → escopo F2.4=0.0; arquivo complexo domina por LOC; ScopeNative roda 1×) via `tempfile` |

**Gates de validação (real exit codes — [[real-exit-codes]])** a cada passo:
`cargo fmt --check` · `cargo check --workspace` · `clippy --workspace --all-targets -D warnings` ·
`cargo test -p touring-quality` · 6 BLOCK dims · `elite_aggregate.py --check` (Diamond 0.9703 não
regride) · `file_size_gate` (cada módulo novo <2000) · `craftsmanship_tdg_gate` (cada módulo
cognitive-friendly).

**Bônus Premium-Elite (CI)**: gate workspace-scope advisory —
`touring-quality score <root> --scope workspace --fail-below 0.80` — Gold floor no workspace inteiro,
sem poder regredir o Diamond release-gate.

---

## 9. Calibração de confiança

| Afirmação | Nível |
|---|---|
| Arquitetura atual (concatenação, 2 MiB cap, composite/dispatch) | **FACT [1.0]** (arquivos lidos) |
| 4 modos de falha | **FACT [1.0]** (derivados do código) |
| SonarQube overall/new-code, ratings worst-of, coverage ratio, quality-gate project-only | **FACT [1.0]** (context7) |
| CodeScene/OpenSSF corroboração | INFERENCE [0.9] (training-data, citado como reforço) |
| Mapeamento 50-dim → AggKind | INFERENCE [0.9] (grounded em D-rules + SonarQube; ratificável por leitura dos verifiers) |
| Triage Thrust A (coeso vs junk-drawer) | INFERENCE [0.85] (por nome/LOC; cada split validado por estrutura antes de tocar) |

---

_Diagnóstico exaustivo — base para a implementação multi-escopo. As 50 dimensões permanecem todas,
consideradas em toda granularidade, da arquitetura à documentação._

---

## 10. Status de implementação (2026-06-21) — Thrust B ✅ COMPLETO

O design das §§5-6 foi **implementado, validado e dogfooded** no mesmo dia.

### Arquivos entregues (todos via `taco-forge perfect-create` + validação real)

| Arquivo | LOC | Conteúdo |
|---|---|---|
| `src/verifications/mod.rs` (edit) | +50 | `enumerate_source_files()` — file-set primitive **sem teto de bytes** (fecha o buraco do modo B); `read_target_source` delega a ele (DRY) |
| `src/scope.rs` (novo) | 323 | `ScopeKind` (8) + `Scope::resolve` + auto-detect + glob matcher `**`/`*` dependency-free (D44) + `file_loc`/`total_loc` |
| `src/aggregate.rs` (novo) | 275 | `AggKind` (5) + `AGG_TABLE[50]` (mapa completo verificado 6/4/17/23) + `aggregate()` com worst-file & p10 |
| `src/scope_report.rs` (novo) | 276 | `ScopeReport` + `score_scope()` (rayon per-file → roll-up; ScopeNative 1× na raiz; herança sub-repo rotulada) + render compact/json |
| `src/lib.rs` (edit) | +12 | `DimId::agg_kind()` + `pub mod`/`pub use` (Scope, ScopeKind, AggKind, ScopeReport, score_scope) |
| `src/bin/touring-quality.rs` (edit) | refactor | `score --scope <kind> --include/--exclude <glob>` + `check --scope`; auto-detect; `--workspace`=alias; back-compat file→`score_target`. Removido `#[allow(dead_code)]` (REGRA #0); CC 18→OK |

### Invariante de Gabriel — provada em teste

O teste `scope_report::tests::all_50_dims_present_at_path_scope` **prova** que todo `ScopeReport`
carrega as **50 dimensões** em qualquer granularidade; `ScopeNative` em sub-repo é avaliada na raiz
e rotulada (`[scope-native @ <scope> root]`), nunca descartada.

### Validação (real exit codes — `[FACT 1.0]`)

| Gate | Resultado |
|---|---|
| `cargo fmt -p touring-quality --check` | 0 |
| `cargo check --workspace` | 0 |
| `cargo clippy --workspace --all-targets -D warnings` | 0 |
| `cargo test -p touring-quality --lib` | **170 passed / 0 failed** (+16 novos: scope 7, aggregate 6, scope_report 3) |
| `elite_aggregate.py --check` | **Diamond 0.9703** (06_documentation re-sincronizado via gen_reference + sync_metrics) |
| `file_size_gate.py` | PASS (módulos novos 275-323 LOC) |

### Dogfood — prova end-to-end em código real

```
$ touring-quality score crates/touring-quality/src --scope crate --dims F1.1,F2.4,F1.8
crates/touring-quality/src [crate] — 0.602 (Bronze) — 59 files, 7385 LOC
  ✗ F1.1 [weighted-loc] 0.473 — LOC-weighted over 59 files (worst touring-quality.rs=0.000, p10=0.000)
  ✗ F1.8 [scope-native] 0.000 — computed once on the root
  ✓ F2.4 [worst-of] 1.000 — worst-of 59 files
```

As três `AggregationKind` (WeightedLoc com worst-file+p10, ScopeNative 1×, WorstOf) produzem
resultados fiéis e atribuídos em código real, no escopo `crate`. Os 7 granularidades de Gabriel são
endereçáveis: file (back-compat) · path · feature (globs) · crate · repo · project · system ·
workspace — todas pela mesma maquinaria, com as 50 dimensões sempre presentes.

**Próximo**: Thrust A (refatoração L5 cirúrgica) — só nos junk-drawers da §7, validado arquivo a
arquivo, usando o próprio `score_scope` para provar a melhoria de coesão (dogfooding).

---

## §11 — Thrust A COMPLETE: Surgical L5 Cohesion Refactor (2026-06-21)

Gabriel `/goal`: attack the complexity backlog **"só onde melhora a coesão de verdade, deixando os algoritmos coesos intactos"**, dogfooding `score_scope` to prove the cohesion gain.

### Verdict: 1 genuine junk-drawer split, 4 cohesive-breadth files preserved
Triage validated by **structure**, NOT by `cognitive_score` (saturates 1.0 on all 5 — would have falsely demanded 5 splits):

| File | LOC | Structural verdict | Action |
|---|---|---|---|
| `touring-server/src/cli/common.rs` | 1214 | **junk-drawer** — module-doc names 4 unrelated concerns (arg-parse + output + daemon-comm + 849-LOC `command_table()` registry) | **SPLIT** → 364 + `command_table.rs` |
| `hooks-core/generator_hints.rs` | 1006 | 31 homogeneous pure matchers `maybe_X_hint_on_task_create(&str)->Option<String>` | leave intact |
| `dispatch/lifecycle/file_changed/hints.rs` | 931 | 38 pure path→hint matchers (same family) | leave intact |
| `hooks-core/bridges/aco_bridge.rs` | 704 | facets of one ACO↔hook bridge unit | leave intact |
| `hooks-core/aco_wiring.rs` | 674 | one `AcoWiringState` (1 impl + KS-stat algo) — cohesive state-machine | leave intact |

### The split (identity-preserving, F-9 pattern)
`command_table()` → new sibling `cli/command_table.rs`, partitioned into **4 builders by the section markers already present in the source** (`hook_commands`/`standalone_commands`/`config_commands`/`daemon_commands` = 14/2/8/90 entries) + a `command_table()` concatenator preserving display order. Types `CommandDescriptor`/`ErrorPolicy` + `print_help` stay in `common.rs`; a `pub use super::command_table::command_table;` re-export keeps `main.rs:238/246` + 3 internal tests **untouched**. Safe because both modules are children of `cli` → the `super::pii::run()` handler-closure paths are **invariant** under the move; the 3 helpers the closures call (`json_to_stdout`/`arg_or`/`flag_value`) are `pub` and imported via `use super::common::{...}`.

### Dogfood — `score_scope` cohesion dims (F1.1/F1.2 are length-saturated, validate by structure)
| Dim | BEFORE common.rs (1214) | AFTER common.rs (364) | command_table.rs (883) |
|---|---|---|---|
| F1.4 SOLID | 0.792 ⚠ | **0.809 ✓** | **0.889 ✓** |
| F1.7 Boundaries | 0.740 ⚠ | 0.760 ⚠ | **0.980 ✓** |

### Gates (real `$PIPESTATUS`) — ZERO regression
`check`/`clippy --workspace --all-targets -D`=0, `fmt`=0, command_table no-regression tests **3/3** (`has_entries`/`has_hooks_and_tools`/`names_are_unique`), `file_size_gate`=0, **`elite_aggregate` Diamond 0.9703 exit 0** (06_documentation re-synced via `gen_reference.py`+`sync_metrics.py --sync`).

`craftsmanship_tdg_gate` is **advisory by design in CI** (`ci.yml:131` `… --check || echo "advisory"`); its 166 `cognitive_score>0.7` flags are the saturation false-positive on cohesive algorithm files. command_table.rs (branchless `vec!` registry, real complexity 0.8) **relocates** common.rs's flag onto a non-block advisory (net 0); the daemon group alone is 681 LOC so the registry cannot go <500 without fragmenting a cohesive list → **circuit-breaker STOP** (don't fragment cohesive data for an advisory).

**Outcome**: the complexity backlog is overwhelmingly cohesive breadth, not tangled complexity. One genuine junk-drawer fixed; four cohesive files correctly preserved per Gabriel's "deixando os algoritmos coesos intactos".
