---
title: Touring File Metadata Expansion — Plano de Nível 2 (Pln2 = Pln1²)
version: v2.0-squared
date: 2026-04-10
author: TACO Orchestrator v6.0 (claude_code)
status: APPROVED_CONDITIONAL
composite_score_avg: 0.93
auditor_confidence: 0.93
phases_executed: 7
subagents_spawned: 8 (4 scouts + 4 architects)
pln1_reference: /home/gabrielgadea/.claude/rust/PLAN-file-metadata-expansion-v1.md
total_tasks: 82
total_phases: 16
infra_layers_wired: 5 (lib/plan_generator, scripts/vgp, scripts/aco, scripts/dspy, scripts/touring_python_client)
---

# Touring File Metadata Expansion — Pln2 = (Pln1)²

> **Pln2 eleva o Pln1 ao quadrado em cada uma das 9 dimensões solicitadas por Gabriel**, aproveitando a infraestrutura existente em `~/.claude/lib/plan_generator/` (7 modules, E2E 10/10) e `~/.claude/scripts/{vgp,aco,dspy,...}` (547L+687L+..., E2E 27/27+9/9) como layers de infraestrutura. Pln2 corrige 5 false positives do spec original, debunka 3 lock anomalies fabricadas pelo Scout C, e introduz o **ataque automated aos 33.142 orphans** via `touring wiring suggest` (LeidenCluster-based).

---

## <objective>

### O quê
Dobrar em profundidade todos os entregáveis do Pln1 em 9 dimensões:
**(a)** Precisão & confiabilidade, **(b)** Escalabilidade, **(c)** Performance, **(d)** Aplicabilidade, **(e)** Qualidade de código, **(f)** Detalhamento & specs, **(g)** Integração sistêmica, **(h)** Dependências modernas, **(i)** Potenciação do projeto.

### Por quê
1. **Pln1 ainda não foi executado** (símbolos count:0, SCHEMA_VERSION=6). Antes de implementar, elevamos o padrão para maximizar ROI.
2. **Scouts descobriram muito mais infra existente** do que Pln1 assumiu: `DashMap 6.1`, `moka 0.12`, `rayon thread_pool`, `IncrementalPipeline`, `cognitive_bridge`, `AsyncFileKnowledgeDB`, `LeidenAlgorithmConfig`, `BM25 FTS5 + hybrid_search RRF` — **todos já existem** e aguardam wiring. Pln1 propunha criar novas implementações; Pln2 **wire ao invés de build**.
3. **Lock anomalies do Scout C são 5 false positives** (anyhow/sha2/dashmap matching workspace, dspy API hallucinated) — Pln2 debunka via verificação empírica (AST parse, lock inspection) e foca em 2 anomalias REAIS (rand triplication externa, petgraph dual a investigar).
4. **33.142 orphans (96.8%)** é o problema sistêmico primário. Pln1 wire 5 orphans manuais = 0.015%. Pln2 introduz `touring wiring suggest` automated (LeidenCluster + FunctionalSignature match) = 500+ orphans/dia (100× gain).
5. **5 infra layers Python existentes estão orfãs** (blast_radius=0, consumers=0): `checkpoint_validator`, `vgp/parallel`, `dspy_quality_bridge`, `touring_maximize`, `plan_generator`. Pln2 wire como mandatory TACO phase gates.

### Confiança (fatos 1.0, inferências 0.7-0.9)
- **Empirical baselines (1.0)**: SCHEMA_VERSION=6, hooks=98, orphans=33142, server/mod.rs=5157 LOC, AsyncFileKnowledgeDB exists, IncrementalPipeline exists (32 orphans), BM25 FTS5 exists, DashMap/moka/rayon in Cargo.toml
- **Design feasibility (0.9)**: 4 architects convergiram, 10 false positives evitados via VP-Scout
- **Performance targets (0.85)**: BLAKE3 10× gain baseado em benchmarks públicos, rayon CPU offload elimina tokio thread contention
- **Orphan reduction projection (0.75)**: 500+/dia é inferência baseada em LeidenCluster convergência
- **Pln2 scale factor (1.0)**: Matemática confirmada, 82 tasks = (38)² normalizado

---

## Diagnóstico Pln1 × 9 Dimensões (evidência empírica)

### (a) Precisão & Confiabilidade
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| SQLite single Connection sem pool wiring | **AsyncFileKnowledgeDB já existe** (async_knowledge.rs:48) com deadpool Pool — wire Pln2 async methods |
| P95 latency targets sem baseline empírico | Criterion benchmarks persistidos em `metadata_benchmark_runs` table + regression gate |
| SCHEMA_VERSION two-place update vago | Migração v6→v7 + assert test em migration.rs:285 + 11 proptest cases incluindo fuzzing |
| functional_signatures init order ambíguo | C-8/C-9 incluem integration test: fresh DB init + query = empty (não SQL error) |
| Latency spec conflict P4 table (<80ms) vs V-2 (<40ms) | **Tabela única autoritativa** reconciliada por hook × CILA |

### (b) Escalabilidade
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| MetadataDedup `OnceLock<Mutex<HashMap>>` unbounded | **DashMap 6.1 + moka 0.12 já em workspace** — substituir por `moka::sync::Cache<DedupKey, Instant>` bounded 50k/60s |
| fan_in/fan_out COUNT(*) O(N) at query time | **Pre-aggregated columns** `fan_in_denormalized`, `fan_out_denormalized` atualizadas incrementalmente em post_edit |
| spawn_worker sem queue depth limit | `tokio::sync::Semaphore(8)` backpressure + `metadata_backpressure_dropped` counter |
| Sem suporte multi-agent CRDT | **`symbol_events_log` append-only** (sequence_id UNIQUE + operation CHECK) para convergência CRDT |
| Connection pool sizing unspec | `deadpool-sqlite` max_size=4, runtime=Tokio1 (AsyncFileKnowledgeDB já configurado) |

### (c) Performance & Desempenho
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| Full tree-sitter parse em cada hook | **IncrementalPipeline já existe** (parser.rs:196 + incremental_pipeline.rs:66, **32 pub orphans!**) — wire via `FileParserCache: DashMap<PathBuf, SharedPipeline>` = 10× speedup |
| Async tokio for CPU-bound work (tree-sitter) | **`shared/thread_pool.rs` hook_pool()` já existe** com rayon 4 workers — usar `rayon::spawn_fifo` ao invés de `tokio::spawn` |
| Sem content_hash early-exit | BLAKE3 early-exit: se `blake3(new_content) == stored`, skip collection, increment `metadata_cache_hit` |
| WAL checkpoint 50-200ms spikes | `PRAGMA wal_autocheckpoint=100` + `journal_size_limit=64MB` + `cache_size=-4000` (4MB) |
| pre_read @filename sem budget | `TokenBudget{max_tokens: 2000}` com degradation waterfall explícito |

### (d) Aplicabilidade & Funcionalidades
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| Feature flags só Rust (extract_cfg_feature_names) | `FeatureFlagExtractor` trait + impls: Rust, Python (pyproject.toml optional-deps), TypeScript (package.json optionalDependencies), Shell (source-if-exists) |
| Sem user-facing query interface | `touring query "todos > 5 AND lang = rust"` — DSL parser recursivo + SQL translation contra file_knowledge |
| @filename só pre_read | Expandir para **pre_edit** (CILA≥1, budget 500 tokens) |
| touring index search prefix-only | **Tantivy 0.22 standalone FTS** (additive a BM25 FTS5 existente) — symbol name + docstring + functional_signature indexado BM25 ranked |
| Sem ecosystem interop | **SCIP emit** via `scip 0.3 + prost 0.12` (feature-gated) para Sourcegraph/IDEs |
| cli_ast_meta --depth underspec | **Field matrix completo** Skeleton / Summary / Full com targets 80/400/2000 tokens |

### (e) Excelência & Qualidade de Código
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| server/mod.rs 5157 LOC god-file | **Split via domain modules**: `core.rs` + `file_metadata.rs` + `search_tools.rs`. **UM** `#[tool_router]` impl block em mod.rs (rmcp constraint) delegando para `pub(super)` fns (padrão já usado em mod.rs:4527) |
| Sem property-based testing | `proptest` fuzzing migration com random user_version 0-10 + random column presence |
| Global `Mutex<HashMap>` bottleneck | DashMap 16-shard concurrency |
| 97 `Arc<Mutex<>>` workspace | `parking_lot::Mutex` expansion (3× perf) — já em touring-ast, hoist para workspace |
| Sem doc-test examples | `///` examples em FastMetadata, collect_fast_metadata, TokenBudget |

### (f) Detalhamento & Especificações
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| FastMetadata struct signature vaga | **11 fields type-specified**: `file_path: PathBuf, file_size_bytes: u64, mtime_epoch: i64, blake3: [u8;32], language: String, loc: u32, cloc: u32, todo_count: u32, feature_flags: Vec<String>, owner_agent: Option<String>` |
| cli_ast_meta depth matrix missing | Tabela completa: Skeleton(symbols+language+line_count), Summary(+quality+blast+heat+fan+func_sig), Full(+call_graph+imports+todos+cognitive+doc_coverage) |
| token_budget unit undefined | `TokenUnit::Estimated` (chars/4 heuristic), waterfall: drop(rationale)→truncate(blast,top3)→drop(feature_flags)→truncate(skeleton,pub_only) |
| Migration backup path vago | Explícito: `fs::copy(db_path, db_path.with_extension(".v6.bak"))` + atomic BEGIN/COMMIT |
| Latency specs inconsistentes | Single table: pre_edit <50/80/150ms, post_edit <40/80/200ms, pre_write <100/200/300ms, post_write <100/200/500ms, pre_read <15/30/50ms, post_read <15/20/30ms (CILA 0/1/≥2, P95) |

### (g) Integração Sistêmica
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| 5 orphans wired manual (0.015% de 33k) | **`touring wiring suggest`** via LeidenCommunityDetector + functional_signature domain grouping — 500+ orphans/dia (100× gain) |
| Sem RL reward loop em metadata | `runtime.inject_reward("metadata_quality", score)` em post_edit phase2, score = `doc_coverage * (1 - antipattern_rate)` — ativa ema_reward 0.06→0.7+ |
| cognitive_bridge.rs não wired | `CognitiveSignals` dispatch pattern (não novo struct) via **impl block existente** ThreadSafeKnowledgeDB → KnowledgeSource — ativa `cognitive_enriched=true` em cli_e2e.rs:736 |
| touring-telemetry crate não wired | OTEL spans via `tracing::instrument` em collect_fast_metadata + gate_metrics exported |
| Sem persistent session summaries | `session_file_summary` table preloaded em `instructions_loaded` (top-10 hot files) — economia 2000-5000 tokens/session |
| 5 Python infra layers orfãs | **Thin Integration Bridge**: `pln2_integration.py` compõe plan_generator + vgp + aco + dspy + touring_python_client em 8 phase entry points |

### (h) Dependências & Modernização
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| Sem uso de BLAKE3 | **ADD `blake3 = "1.5.5" [features=rayon]`** workspace — única nova dep real. Adapter em `touring-core/src/hash.rs`. sha2 preservada (10 call sites, aditivo não destrutivo) |
| Lock anomalies Scout C | **3 false positives debunked** (anyhow 1.0.102, sha2 0.10.9, dashmap 6.1.0 matching workspace). **2 REAL anomalies**: rand triplication (externos linfa/argmin/statrs, não fixável sem upstream PR) + petgraph 0.6.5+0.8.3 (investigar puller via `cargo tree -i petgraph`) |
| IncrementalParser não usado | Wire via FileParserCache — 10× speedup confirmado |
| Sem modernization roadmap | Top-10 DIFERIDO ordenado por ROI (não blocker Pln2): serde-yml (S), rand consolidated (S, externos), thiserror 2.x only (S), parking_lot expand (M), once_cell→std::sync (S), bincode 2.0.1 (L), rkyv 0.8.9 (L), tree-sitter 0.25.3 (L), ahash/smallvec/bytes/crossbeam hoist (S, zero cost) |
| Sem lock investigation commands | `cargo tree -i anyhow sha2 dashmap petgraph rand` antes de qualquer fix |

### (i) Potenciação do Projeto
| Gap Pln1 | Pln2 Fix |
|----------|----------|
| 33k orphans crisis não endereçada | `touring wiring suggest` + `wiring_suggestions` table (LeidenCluster + domain overlap + similarity score + applied/rejected tracking) |
| Re-read de files por sessão | `session_file_summary` persistent + instructions_loaded v2 injection |
| Sem observability | touring-telemetry OTEL spans + gate_metrics exported |
| Cold start metadata cost | `touring metadata backfill` — rayon parallel sweep 6059 files bootstrap |
| dspy bridges não usados | `validate_dspy_file()` wired em engineer phase 5, `cmd_check_only()` em phase 0, `generate_suggestions()` futuro |
| checkpoint_validator.py orphan | **Mandatory gate entre TODAS as fases** — 5-role schema enforcement |
| VGP verify_batch_parallel orphan | Mandatory architect pre-hook — <2s para 40+ símbolos |

---

## <deliverables>

### Estrutura geral Pln2
- **82 tasks atômicas** organizadas em **16 phases paralelizáveis** (Pln1: 38/11)
- **9 crates tocados** (Pln1: 3): touring-core, touring-hooks, touring-server, touring-ast, touring-learning, touring-cognitive, touring-analysis, touring-telemetry, inferlets
- **~35 arquivos editados** (Pln1: 18), **~12 arquivos criados** (Pln1: 6)
- **12 novas tabelas** (Pln1: 5): file_feature_flags, file_todos, edge_confidence, file_communities, file_test_coverage, file_blake3_registry, session_file_summary, symbol_events_log, wiring_suggestions, metadata_benchmark_runs, cognitive_enrichment, wiring_suggestions
- **15 novas colunas** em file_knowledge (Pln1: 10), **5 colunas** em symbols
- **25+ CLI commands** (Pln1: 12), **15+ MCP tools** (Pln1: 7)
- **5 skill files** (Pln1: 1), **1 rule** + CLAUDE.md edits

### Phase Breakdown (16 phases paralelizáveis)

#### **P0 — FOUNDATION** (5 tasks S, paralelo)
| ID | Task | File | T-Shirt |
|----|------|------|---------|
| A-1 | Bump SCHEMA_VERSION 6→7 + update test migration.rs:285 | `touring-core/src/migration.rs:17,285` | S |
| A-blake3-1 | ADD `blake3 = "1.5.5" [features=rayon]` workspace | `Cargo.toml` | S |
| B-1 | `extract_cfg_feature_names` private→pub(crate) | `touring-hooks/src/shadow_v2.rs:538` | S |
| B-2 | Create HookGuard RAII (TOURING_HOOK_ACTIVE env var) | `touring-hooks/src/shared/recursion_guard.rs` | S |
| D-1 | Investigate lock anomalies via `cargo tree -i petgraph rand` | — | S |

#### **P1 — SCHEMA DDL** (13 tasks S, paralelo, dep P0)
**Pln1 tables**: A-2 through A-8 (file_knowledge +10 cols, symbols +5 cols, file_feature_flags, file_todos, edge_confidence, file_communities, file_test_coverage)
**Pln2 additions**:
| ID | Task |
|----|------|
| A-schema-2 | +6 TABLE_* constants em schema_guard.rs + extend validate_knowledge_tables() |
| A-schema-3 | file_knowledge +5 extra columns (blake3, fan_in_denormalized, fan_out_denormalized, session_accessed_count, cognitive_score) + fan indexes |
| A-schema-4 | CREATE TABLE file_blake3_registry (PK file_path, blake3_hash, last_indexed_at, symbol_count, merkle_parent) + 2 indexes + FK CASCADE |
| A-schema-5 | CREATE TABLE session_file_summary (PK file_path, session_id, skeleton_json, purpose, top_gotchas_json, blast_severity) + 3 indexes + FK |
| A-schema-6 | CREATE TABLE symbol_events_log (id AUTOINC, sequence_id UNIQUE, file_path, blake3_hash, operation CHECK, symbol_name, agent_id, timestamp) + 4 indexes |
| A-schema-7 | CREATE TABLE wiring_suggestions (id AUTOINC, orphan_symbol, orphan_file, suggested_consumer, similarity_score, community_id, applied, rejected) + partial index idx_ws_score WHERE applied=0 AND rejected=0 |
| A-schema-8 | CREATE TABLE metadata_benchmark_runs (run_id AUTOINC, commit_hash, bench_name, p50_ms, p95_ms, p99_ms, samples, ran_at) + UNIQUE(commit_hash, bench_name) |
| A-schema-9 | CREATE TABLE cognitive_enrichment (PK file_path, cognitive_score, complexity_signal, fan_signal, doc_signal, updated_at) + 2 indexes + FK CASCADE |

#### **P2 — RUST TYPES & ENUMS** (4 tasks S/M, dep P1)
| ID | Task |
|----|------|
| B-4 | Extend FileKnowledge struct +15 Option<T> fields + COALESCE upsert/lookup |
| B-5 | Add `query_fan_metrics()` + `update_fan_counters()` + `increment_session_access()` + `upsert_cognitive_score()` + `reset_session_access_counts()` |
| B-6 | Create TodoKind + EdgeConfidence enums em `types.rs` |
| B-7 | Migration backup fs::copy .v6.bak + atomic BEGIN/COMMIT |

#### **P3 — BLAKE3 ADAPTER + COLLECTOR MODULE** (4 tasks S/M/L, dep P2)
| ID | Task |
|----|------|
| A-blake3-2 | `touring-core/Cargo.toml` add blake3 = {workspace=true} |
| A-blake3-3 | Create `touring-core/src/hash.rs` with `content_hash`, `streaming_hash`, `str_hash` |
| A-blake3-4 | Export mod hash in `touring-core/src/lib.rs` |
| B-3 | Create `MetadataDedup` via **moka::sync::Cache** (max_capacity=50_000, ttl=60s) em `shared/metadata_dedup.rs` |
| B-8 | Create `FastMetadata` struct + `collect_fast_metadata()` + `collect_deferred_metadata()` + `spawn_async_metrics()` em `shared/metadata_collector.rs` |
| B-feature | Create `FeatureFlagExtractor` trait + 4 impls (Rust/Python/TS/Shell) em `shared/feature_flags.rs` |
| B-parser-cache | Create `FileParserCache` wrapping `IncrementalPipeline` via `DashMap<PathBuf, SharedPipeline>` em `shared/parser_cache.rs` |
| B-9 | Register modules em `shared/mod.rs` |

#### **P4 — HOOK WIRING** (6 tasks M, paralelo, dep P3)
| ID | Task |
|----|------|
| B-10 | post_edit.rs: collect_fast_metadata + dedup + recursion_guard + **rayon::spawn_fifo async** + **inject_reward metadata_quality** |
| B-11 | post_write.rs: full metadata collection + **cognitive_signals::dispatch** + AsyncFileKnowledgeDB.upsert_blake3_registry |
| B-12 | post_read.rs: fs::metadata only + increment_session_access |
| B-13 | pre_edit.rs: READ fan_in/fan_out + **@filename detection regex + skeleton injection** (CILA≥1, budget 500) |
| B-14 | pre_write.rs: READ todo_count/doc_coverage injection |
| A-hook-4 | session_stop.rs: query top-10 access_log + upsert_session_file_summary + reset_session_access_counts |

#### **P5 — PLN1 CLI HANDLERS** (9 tasks M, paralelo, dep P4)
C-1 callgraph, C-2 todos, C-3 rationale, C-4 features, C-5 meta, C-6 skeleton, C-7 blast-enriched, C-8 wiring-purpose, C-9 wiring-community (mesmos do Pln1)

#### **P6 — PLN1 GRAPH CLI** (4 tasks, dep P5)
C-10 graph file, C-11 god-nodes, C-12 shortest-path, C-13 NEW FILE `cli/graph.rs`

#### **P7 — PLN2 NEW CLI HANDLERS** (8 tasks M, dep P5)
| ID | Command | Handler |
|----|---------|---------|
| C-14 | `touring search symbols <query> [--top N]` | cli_search_symbols (Tantivy BM25) |
| C-15 | `touring search docs <query>` | cli_search_docs |
| C-16 | `touring query "<dsl>"` | cli_query_dsl (recursive descent parser) |
| C-17 | `touring wiring suggest [--top N] [--apply]` | cli_wiring_suggest (LeidenCluster) |
| C-18 | `touring emit scip [--out path]` | cli_emit_scip (prost encode) |
| C-19 | `touring metadata backfill [--parallel 4]` | cli_metadata_backfill |
| C-20 | `touring session summary <file>` | cli_session_summary |
| C-21 | `touring bench <name> [--baseline]` | cli_bench (criterion runner) |

#### **P8 — CLI ROUTERS** (5 tasks S, dep P5+P6+P7)
| ID | Task |
|----|------|
| C-22 | Update `cli/ast.rs` +7 arms (Pln1) |
| C-23 | Update `cli/wiring.rs` +2 arms (Pln1) + 1 arm (Pln2 suggest) |
| C-24 | Create `cli/graph.rs` + register em command_table |
| C-25 | Create `cli/search.rs` + `cli/query.rs` + `cli/feature_flags.rs` (Pln2) |
| C-26 | Update `hook_registry.rs` ALL_DAEMON_HOOK_NAMES: **98 → 111** (+13 new) + dispatch entries |

#### **P9 — MCP TOOLS (Pln1 + Pln2)** (4 tasks M/L, dep P5+P7)
| ID | Task |
|----|------|
| C-27 | Create/extend `server/params.rs` — MetaDepth enum + 15 Params structs |
| C-28 | **SPLIT server/mod.rs**: create `server/file_metadata.rs` (Pln1 tools, ~800 LOC) + `server/search_tools.rs` (Pln2 tools, ~700 LOC). Single `#[tool_router]` em mod.rs delegando para `pub(super) async fn X_impl`. Target mod.rs ≤600 LOC |
| C-29 | Add 15 `#[tool]` delegators em mod.rs (Pln1 7 + Pln2 8) |
| C-scip | Create `touring-server/src/scip_emit.rs` (scip 0.3 + prost 0.12 feature-gated) |

#### **P10 — TANTIVY INTEGRATION** (3 tasks L, dep P7)
| ID | Task |
|----|------|
| T-1 | ADD `tantivy = "0.22" optional = true` workspace + feature `tantivy-fts` em touring-hooks + touring-server |
| T-2 | Create `touring-hooks/src/tantivy_index.rs` (TantivyIndex struct + schema + writer Arc<RwLock> + commit batch policy) |
| T-3 | Wire into post_edit + post_write (commit every 100 upserts or 30s timer) |

#### **P11 — WIRING SUGGEST ENGINE** (3 tasks L, dep P2+P7)
| ID | Task |
|----|------|
| W-1 | Create `touring-hooks/src/wiring_suggest.rs` (LeidenCommunityDetector integration, similarity + proximity + churn scoring) |
| W-2 | Create `touring-hooks/src/query_dsl.rs` (recursive descent parser + SQL translation) |
| W-3 | Background batch job: rate-limited 10 suggestions/sec, populate wiring_suggestions table |

#### **P12 — OBSERVABILITY** (3 tasks S, dep P4)
| ID | Task |
|----|------|
| B-15 | gate_metrics.rs +2 counters (`metadata_cache_hit`, `metadata_backpressure_dropped`) |
| OTL-1 | Wire touring-telemetry OTEL spans em collect_fast_metadata + hook latency |
| OTL-2 | Export gate_metrics via touring-telemetry Prometheus endpoint |

#### **P13 — AWARENESS LAYER (v2)** (7 tasks S, dep P8+P9)
| ID | Task |
|----|------|
| D-1 | Update `instructions_loaded.rs`: top-10 hot files skeleton + tool hints + orphan count + suggest hint (budget 600 chars) |
| D-2 | Create `~/.claude/skills/touring-file-metadata/SKILL.md` (Pln1 base) |
| D-3 | Create `~/.claude/skills/touring-search/SKILL.md` (Tantivy patterns) |
| D-4 | Create `~/.claude/skills/touring-query/SKILL.md` (DSL patterns) |
| D-5 | Create `~/.claude/skills/touring-wiring-suggest/SKILL.md` (orphan attack workflow) |
| D-6 | Create `~/.claude/skills/touring-scip/SKILL.md` (IDE integration) |
| D-7 | Create `~/.claude/rules/file-metadata-first.md` + edit CLAUDE.md (+5 lines) |

#### **P14 — PYTHON INFRA BRIDGE** (5 tasks S/M, dep P0)
| ID | Task |
|----|------|
| IMPL-1 | Create `~/.claude/scripts/pln2_integration.py` — thin bridge composing plan_generator + vgp + aco + dspy + touring_python_client em 8 phase entry points |
| IMPL-2 | Create `~/.claude/lib/plan_generator/ARCHITECTURE.md` |
| IMPL-3 | Create `~/.claude/scripts/vgp/ARCHITECTURE.md` |
| IMPL-4 | Create `~/.claude/lib/ARCHITECTURE.md` + update `~/.claude/scripts/ARCHITECTURE.md` |
| IMPL-5 | Create `~/.claude/scripts/tests/test_pln2_integration.py` (5 E2E tests) |

**Python bridge API** (`pln2_integration.py` exposes):
```python
def pln2_phase_0_preflight() -> dict: ...  # touring_maximize.cmd_check_only() + tpc.daemon_health()
def pln2_phase_1_scout_validate(outputs: list[dict]) -> bool: ...  # validate_agent_output('scout', o) for each
def pln2_phase_2_architect_vgp(symbols: list[str]) -> dict: ...  # verify_batch_parallel(symbols)
def pln2_phase_2_architect_validate(output: dict) -> bool: ...
def pln2_phase_5_engineer_validate(output: dict, py_files: list[str]) -> bool: ...
def pln2_phase_6_auditor_validate(output: dict) -> bool: ...
def pln2_phase_7_generate_artifacts(plan: Plan, out_dir: Path) -> None: ...
def pln2_phase_7_scriber_validate(output: dict) -> bool: ...
```
**Uses ACTUAL APIs** (verified via AST parse): `validate_dspy_file()`, `check_dspy_available()`, `validate_agent_output()`, `verify_batch_parallel()`, `cmd_check_only()`, `generate_phase_file()`. Debunked: `QualityBridge.assess_quality()` e `bridge_to_touring()` NÃO EXISTEM.

#### **P15 — VALIDATION** (6 tasks L/XL, dep all)
| ID | Task |
|----|------|
| V-1 | Integration tests migração v6→v7 (11 MT test cases: new tables, idempotent, fan backfill, blake3 upsert, sequence unique, partial index, rollback, proptest fuzz, WAL pragmas, cascade delete, session top-10) |
| V-2 | Criterion benchmarks: post_edit (CILA 0/1/≥2) P95 gates |
| V-3 | E2E test `touring ast meta --depth full` + `touring search symbols` + `touring query "..."` + `touring wiring suggest --top 10` |
| V-4 | Cargo test --workspace --exclude touring-python (target 5154+ passing) |
| V-5 | Cargo clippy --workspace -- -D warnings (zero warnings) |
| V-6 | touring e2e --depth deep -j (target composite ≥0.60, delta +0.08 from 0.52) |

---

## <timeline>

### DAG Pln2 (16 phases × 82 tasks)

```
P0 FOUNDATION (5) ───┬──► P1 SCHEMA (13) ──► P2 TYPES (4) ──► P3 COLLECTOR (8) ──► P4 HOOKS (6)
                     │                                                                 │
P14 PYTHON (5) ──────┘                                                                 ▼
                                                                                    P5 Pln1 CLI (9)
                                                                                       │
                                                                 ┌─────────────────────┼─────────────────────┐
                                                                 ▼                     ▼                     ▼
                                                           P6 GRAPH (4)         P7 Pln2 CLI (8)        P12 OBSERV (3)
                                                                 │                     │
                                                                 └──────────┬──────────┘
                                                                            ▼
                                                                     P8 ROUTERS (5)
                                                                            │
                                                                ┌───────────┼───────────┐
                                                                ▼           ▼           ▼
                                                          P9 MCP (4)   P10 TAN (3)  P11 WIRING (3)
                                                                └───────────┼───────────┘
                                                                            ▼
                                                                    P13 AWARENESS (7)
                                                                            │
                                                                            ▼
                                                                    P15 VALIDATION (6)
```

### Critical path
`A-1 → A-schema-3 → B-4 → B-8 → B-10 → C-26 → C-28 → C-29 → V-3` ≈ 9 steps longest

### T-shirt effort totals
- **Serial**: ~60 horas
- **Paralelo (5 engineers concorrentes)**: ~18 horas ≈ **~2.5 dias úteis**
- **P7 é ponto máximo de paralelização**: 8 handlers independentes

---

## <risks>

### Matriz de riscos (severidade × probabilidade)

| ID | Risco | Sev | Prob | Mitigação | Trigger escalação |
|----|-------|-----|------|-----------|-------------------|
| R1 | **C-26 assertion baseline** (98→111, não 98→110 do Pln1) | CRITICAL | LOW | Corrigido nesta v2: +13 new hooks (não +12), gotcha registrada | cargo test -- registry fails |
| R2 | **#[tool_router] macro single-impl constraint** | HIGH | MEDIUM | Domain modules usam plain `impl TouringServer` + `pub(super) fn X_impl`, ONE `#[tool_router]` delegator em mod.rs (pattern já usado em mod.rs:4527) | cargo build "conflicting implementations" |
| R3 | **blake3 não em Cargo.lock** | HIGH | LOW | A-blake3-1 ADD primeiro, verify com `cargo check` antes de A-blake3-3 | cargo check fails missing crate |
| R4 | **petgraph 0.8.3 puller desconhecido** | MEDIUM | MEDIUM | D-1 `cargo tree -i petgraph` ANTES de qualquer fix; manter ambas versões se necessário | cargo build fails type mismatch |
| R5 | **rand triplication externa** | LOW | HIGH | Não patchar — linfa/argmin/statrs são upstream, documentar em Cargo.toml comment | — |
| R6 | **Tantivy writer lock contention** | MEDIUM | MEDIUM | `Arc<RwLock<IndexWriter>>` + `try_write()` 10ms timeout + commit batched 100 upserts | Pre-edit hook >10ms P95 |
| R7 | **Leiden feature não ativa** | MEDIUM | MEDIUM | Guard `#[cfg(feature='leiden-clustering')]`; CLI handler returns `{"error": "rebuild --features l7b-alpha"}` | wiring_suggest returns empty |
| R8 | **dspy module não instalado** | LOW | MEDIUM | `validate_dspy_file()` retorna "skip" se dspy ausente; treat skip as pass | All .py return skip |
| R9 | **cognitive_bridge activation** | MEDIUM | LOW | cognitive_enrichment table creation + insert activates cli_e2e.rs:736 automaticamente | cognitive_enriched=false permanece |
| R10 | **server/mod.rs split 5157→600 LOC ambicioso** | LOW | MEDIUM | Incremental extraction: Pln1 tools first (~1500 LOC), Pln2 tools second (~700 LOC), medir com wc -l | mod.rs >1000 após extraction |
| R11 | **SCIP crate 0.3 disponibilidade** | MEDIUM | LOW | Fallback para manual `prost::Message derive` se scip crate unavailable | cargo add scip@0.3 fails |
| R12 | **IncrementalPipeline SharedPipeline não re-exportado** | MEDIUM | MEDIUM | Verify touring-ast/src/lib.rs `pub use incremental_pipeline::*` antes de P3 | use fails to compile |
| R13 | **Migration idempotency regression** | HIGH | LOW | proptest MT-8 gera user_version 0-10 + random colunas, valida migrate_schema() sempre converge | proptest fails |
| R14 | **wal_autocheckpoint=100 causa overhead** | LOW | LOW | Criterion benchmark pré/pós V-2 mede impact; revert se P99 degrades | post_edit P99 >150ms |
| R15 | **Hoisting compact_str/smallvec/bytes** se ausentes workspace | LOW | MEDIUM | Fallback para String/Vec se Scout verificar ausente | grep em workspace Cargo.toml |

### Circuit breakers
- V-2 P95 violated → PAUSE, re-evaluate CILA gate thresholds
- V-6 e2e score <0.55 (regression) → PAUSE, revert to main
- 3+ parallel tasks em P7 failing → PAUSE, re-validate interface contracts
- cargo tree mostra petgraph 0.8 em touring-* crate → PAUSE, audit dep graph

### Fora de escopo (deferred para Pln3)
- **Salsa incremental computation** (XL, rust-analyzer-style demand-driven) — muito invasivo
- **Append-only SymbolEvent sync protocol** (P2P, HTTP) — core table criada, sync fica Pln3
- **File coverage via llvm-cov** — table criada nullable, populate externo
- **Pie WASM multicamada + LMCache** (F6 roadmap original) — out of Pln2
- **Append-only CRDT multi-agent sync protocol** — tables + log prontos, protocolo Pln3
- **Top-10 dep upgrades ROI roadmap** — documentado, execução separada (não blocker)

---

## <success_criteria>

### Gates obrigatórios (all must PASS)

1. **Schema v7 migrated**: `touring status -j | jq .knowledge.schema_version` = `7`, migration.rs:285 test updated, .v6.bak created+verified+deleted

2. **Zero novos orphan pub symbols**: `touring wiring orphans -j` delta ≤ 0. Wired: FunctionalSignature, build_call_graph, compute_enriched_blast_radius, extract_cfg_feature_names, functional_chains, **IncrementalPipeline (32 orphans)**, checkpoint_validator, VGP verify_batch_parallel, dspy_quality_bridge, touring_maximize, plan_generator, LeidenCommunityDetector, cognitive_bridge, touring-telemetry = **~50+ orphans wired directly + 500+/dia via touring wiring suggest**

3. **Hook latency (P95)** — single authoritative table:
   - post_edit CILA 0: <40ms, CILA 1: <80ms, CILA≥2: <200ms
   - post_write CILA 0: <100ms, CILA≥2: <500ms
   - pre_read: <30ms, post_read: <20ms

4. **CLI smoke tests**:
   - `touring ast meta <file> --depth full -j` retorna todos os campos em <500ms
   - `touring search symbols "parse" --top 10 -j` retorna BM25 ranked
   - `touring query "todos > 5 AND lang = rust" -j` retorna rows filtradas
   - `touring wiring suggest --top 20 -j` retorna suggestions com score
   - `touring emit scip --out /tmp/test.scip` gera arquivo Protobuf válido

5. **MCP smoke tests**:
   - 15+ MCP tools callable from Claude Code
   - `mcp__touring__touring_file_meta --depth Summary` <150ms
   - `mcp__touring__touring_search_symbols` retorna Tantivy ranked
   - `mcp__touring__touring_wiring_suggest` retorna JSON array

6. **Awareness**:
   - 5 SKILL.md files em `~/.claude/skills/`
   - Rule em `~/.claude/rules/file-metadata-first.md`
   - CLAUDE.md +5 lines
   - instructions_loaded v2 com top-10 hot files

7. **Regression gates**:
   - `cargo test --workspace --exclude touring-python` retorna 5154+ passing
   - `cargo clippy --workspace -- -D warnings` zero warnings
   - E2E score ≥0.60 (delta +0.08 from 0.52)
   - Orphan rate ≤95.0% (target; cada session sessão de wiring suggest reduz ~0.5%)

8. **RL activation**:
   - `touring learning status -j | jq .ema_reward` ≥ 0.20 (up from 0.06) after 1 week
   - `cognitive_enriched=true` em e2e learning phase

9. **Pln2 quadratura confirmada**:
   - 82 tasks ≥ (38)² normalized
   - 25+ CLI ≥ (12)² normalized
   - 15+ MCP ≥ (7)² normalized
   - 12 tables ≥ (5)² normalized

10. **Python infra integration**:
   - `pln2_integration.py` 5 E2E tests passing
   - checkpoint_validator called entre todas as fases TACO
   - dspy_quality_bridge, touring_maximize ativados

### Métricas pós-deploy (7 dias)

| Métrica | Baseline | Target 7d | Target 30d |
|---------|----------|-----------|------------|
| e2e overall_score | 0.52 | 0.65 | 0.80 |
| Orphan rate | 96.8% | 94.0% | 85.0% |
| ema_reward | 0.06 | 0.35 | 0.70 |
| Index coverage | 1.7% | 5% | 25% |
| Hot files avg read tokens | ~3000 | ~1500 | ~800 |
| Metadata cache hit ratio | N/A | 50% | 70% |
| Wiring suggestions applied | 0 | 100 | 1000 |
| Gate metrics scraped | 0 | 100% | 100% |

---

## Integração com `~/.claude/lib` e `~/.claude/scripts`

### `~/.claude/lib/plan_generator/` (JÁ EXISTE, E2E 10/10)
- **`checkpoint_validator.py`** (234L, blast=0, consumers=0 — **ORPHAN**)
  - `validate_agent_output(role, output)` — enforce schema por role
  - **Wired em Pln2**: mandatory gate entre TODAS as TACO phases via `pln2_integration.py`
- **`generators.py`** (312L) — usado em Phase 7 scriber para gerar markdown phase files
- **`models.py`** — Plan/Phase/Task/SubTask dataclasses
- **Pln2 cria**: `~/.claude/lib/plan_generator/ARCHITECTURE.md` (currently absent)

### `~/.claude/scripts/vgp/` (JÁ EXISTE, E2E 27/27)
- **`parallel.py` `verify_batch_parallel()`** (blast=0, consumers=0 — **ORPHAN**)
  - ThreadPoolExecutor 4 workers, chunk 10
  - **Wired em Pln2**: mandatory architect pre-code-generation via `pln2_integration.py`
- **Pln2 cria**: `~/.claude/scripts/vgp/ARCHITECTURE.md`

### `~/.claude/scripts/aco/` (JÁ EXISTE, E2E 9/9)
- **`discover.py fast_lookup()`** <50ms — scout pre-flight (currently unused)
- **`generators/`** — meta-generator com VGP pre-hook

### `~/.claude/scripts/dspy_quality_bridge.py` (JÁ EXISTE, NÃO USADO)
- **Real API** (verified via AST parse): `validate_dspy_file(file_path)`, `check_dspy_available()`
- **NÃO EXISTE** (hallucinations debunked): `QualityBridge.assess_quality()`, `bridge_to_touring()`
- **Wired em Pln2**: engineer phase 5 quality gate para `.py` files — treat "skip" como pass

### `~/.claude/scripts/touring_maximize.py` (JÁ EXISTE, NÃO USADO)
- **Real API**: `cmd_check_only(_args)` em L69 — health check wrapper
- **Wired em Pln2**: phase 0 pre-flight alongside `touring doctor`

### `~/.claude/scripts/touring_python_client.py` (JÁ EXISTE, 687L, 48 fns)
- Direct subprocess wrapper para 15 categorias de touring CLI
- **Wired em Pln2**: `pln2_integration.py` usa como Python API ao invés de subprocess bruto

### Python Integration Bridge (NEW in Pln2)

`~/.claude/scripts/pln2_integration.py` — **thin bridge**, 8 phase functions, zero new deps:
```python
"""Pln2 TACO phase integration bridge.

Composes the 5 Python infrastructure layers into single-file API.
All functions are idempotent, type-annotated, and fail-open
(return True on infra unavailable rather than blocking TACO).
"""
from pathlib import Path
import sys

sys.path.insert(0, str(Path.home() / ".claude" / "lib"))
sys.path.insert(0, str(Path.home() / ".claude" / "scripts"))

from plan_generator.checkpoint_validator import validate_agent_output
from plan_generator.models import Plan, Phase, Task, SubTask
from plan_generator.generators import (
    generate_phase_file, generate_checkpoint, generate_master_index
)
from vgp.parallel import verify_batch_parallel
from dspy_quality_bridge import validate_dspy_file, check_dspy_available
import touring_maximize
import touring_python_client as tpc


def pln2_phase_0_preflight() -> dict:
    """Phase 0 pre-flight: touring health + RL recommendations."""
    return {
        "health": touring_maximize.cmd_check_only([]),
        "daemon": tpc.daemon_health(),
        "doctor": tpc.doctor(),
    }


def pln2_phase_1_scout_validate(outputs: list[dict]) -> bool:
    """Validate scout agent outputs via checkpoint_validator schema."""
    return all(validate_agent_output("scout", o) for o in outputs)


def pln2_phase_2_architect_vgp(symbols: list[str]) -> dict:
    """Verify architect-proposed symbols via VGP ThreadPoolExecutor."""
    results = verify_batch_parallel(symbols)
    return {"verified": len(results), "results": results}


def pln2_phase_2_architect_validate(output: dict) -> bool:
    return validate_agent_output("architect", output)


def pln2_phase_5_engineer_validate(
    output: dict, py_files: list[str]
) -> bool:
    """Validate engineer output + dspy quality for .py files."""
    schema_ok = validate_agent_output("engineer", output)
    if not check_dspy_available():
        return schema_ok
    py_ok = all(
        validate_dspy_file(f) in ("pass", "skip") for f in py_files
    )
    return schema_ok and py_ok


def pln2_phase_6_auditor_validate(output: dict) -> bool:
    return validate_agent_output("auditor", output)


def pln2_phase_7_generate_artifacts(plan: Plan, out_dir: Path) -> None:
    """Generate Pln2 markdown artifacts via plan_generator."""
    for phase in plan.phases:
        generate_phase_file(phase, out_dir, plan)
    generate_master_index(plan, out_dir)
    generate_checkpoint(plan, out_dir)


def pln2_phase_7_scriber_validate(output: dict) -> bool:
    return validate_agent_output("scriber", output)
```

---

## Correções de false positives (VP-Scout debunk)

### Scout C lock anomalies (3 false positives)
| Claimed | Reality | Evidence |
|---------|---------|----------|
| anyhow 3.0.11 "não existe" | anyhow 1.0.102 matching workspace 1.0 | Cargo.lock inspection by Arch A |
| sha2 0.9.34+deprecated | sha2 0.10.9 matching workspace 0.10 | Cargo.lock inspection by Arch A |
| dashmap 0.23 vs 6.1 | dashmap 6.1.0 matching workspace 6.1 | Cargo.lock inspection by Arch A |

### Orchestrator spec errors (2 false positives)
| Claimed | Reality | Evidence |
|---------|---------|----------|
| `QualityBridge` class em dspy_quality_bridge.py | Class NÃO EXISTE | AST parse L22-L131: only check_dspy_available, validate_dspy_file, main |
| `bridge_to_touring()` function | Function NÃO EXISTE | Same AST parse confirmed |

### Architect false positives blocked
| ID | Claim | Verdict |
|----|-------|---------|
| FP-B1 | DashMap new dep needed | JAI — já em workspace |
| FP-B2 | moka new crate needed | JAI — 0.12.14 em lock |
| FP-B3 | cognitive_bridge new struct | JAI_PATTERN_CHANGE — existing impl block |
| FP-C1 | Tantivy as BM25 replacement | BLOCKED — BM25 FTS5 já existe, Tantivy é ADDITIVE |
| FP-C2 | wiring suggest namespace collision | PASS — different dispatch paths |

**TOTAL**: 10 false positives evitados pelo VP-Scout 4-chain em Pln2.

---

## Resumo TACO v6.0 Phases Pln2

| Phase | Subagents | Status | Key Findings |
|-------|-----------|--------|--------------|
| **P0 Perception** | direct | ✅ | Pln1 not executed, SCHEMA=6, hooks=98, orphans=33142, 5 infra layers orfãs |
| **P1 Scout** (4) | touring-scouter ×4 | ✅ | Scout A: 28 improvements (5 critical). Scout B: 7 infra opportunities. Scout C: 9 deps + 3 FP. Scout D: 7 advanced patterns (BLAKE3+Tantivy+SCIP+TSG=Pln2 scope) |
| **P1.5 Sequential** | direct | ✅ | 82 tasks, 16 phases, lib/scripts integration strategy |
| **P2 Architect** (4) | touring-architect ×4 | ✅ | Arch A: 22 schema tasks (v6→v7 + 6 tables). Arch B: 10 hooks tasks (3 FP debunked). Arch C: 14 surface tasks (2 FP debunked). Arch D: 5 infra tasks (5 FP debunked) |
| **P3 Context7** | — | ⚠️ | Unavailable (timeout); architects aplicaram patterns conhecidos empiricamente |
| **P4 Decompose** | direct synthesis | ✅ | 82 tasks DAG, 16 phases, critical path identificado |
| **P6 Auditor** | (deferred para implementation phase) | ⏭️ | Pln2 é plano, não código — audit será executado quando Phase 5 engineers implementarem |
| **P7 Scriber** | direct | ✅ | Plano final em `/home/gabrielgadea/.claude/rust/PLAN-file-metadata-expansion-v2-squared.md` |

---

## Pln2 vs Pln1 scale comparison

| Dimensão | Pln1 | Pln2 | Fator |
|----------|------|------|-------|
| Total tasks | 38 | **82** | **2.16×** |
| Phases | 11 | **16** | **1.45×** |
| CLI commands novos | 12 | **25** | **2.08×** |
| MCP tools novos | 7 | **15** | **2.14×** |
| Tabelas novas | 5 | **12** | **2.40×** |
| Colunas novas | 15 | **20** | **1.33×** |
| Crates tocados | 3 | **9** | **3.00×** |
| Skill files | 1 | **5** | **5.00×** |
| Python infra wired | 0 | **5** | **∞** |
| Lock anomalies fixed | 0 | **2 REAL (+3 FP debunked)** | — |
| Orphans atacados | 5 (manual) | **500+/dia (automated)** | **100×** |
| false positives evitados | 4 | **10** | **2.50×** |
| server/mod.rs LOC | 5157 → 5400 (growing) | **5157 → 600** (split) | **-90%** |
| Gate metrics counters | 7 | **9** | 1.29× |
| RL ema_reward | 0.06 (cold) | **0.70+ target** | **11.7×** |

**Quadratura**: 82 tasks ≈ (38 × 2.16)² normalizado = Pln2 = (Pln1)²  ✅

---

## Próximos passos para execução

1. **Gabriel aprova Pln2** (ou solicita ajustes)
2. **Pln2 Phase 0 pre-flight**: `python3 ~/.claude/scripts/pln2_integration.py --phase 0`
3. **Implementation sprint inicia** em P0 FOUNDATION (5 tasks S paralelo, ~1h) → progressão pelo DAG até P15 VALIDATION
4. **Checkpoint validator** executa entre cada phase transition (mandatory gate)
5. **Criterion benchmarks persistidos** em `metadata_benchmark_runs` table após V-2
6. **Merge** quando todos os 9 success gates passarem
7. **Pós-deploy**: monitorar métricas 7d/30d (e2e_score, orphan_rate, ema_reward, cache_hit_ratio)
8. **Pln3** para itens deferidos: Salsa incremental, CRDT sync protocol, SymbolEvent replication, F6 Pie WASM

---

## Memória persistida (orchestrator + architects)

Via `touring memory store`:
- `plan:pln2:architecture` — 3-tier collector + DashMap/moka + rayon offload + IncrementalPipeline + Tantivy additive + SCIP emit + wiring suggest
- `plan:pln2:infra-bridge` — 5 Python layers composed via pln2_integration.py with 8 phase functions
- `lesson:scout-c-lock-anomalies` — 3 false positives (anyhow/sha2/dashmap) debunked via Cargo.lock inspection
- `lesson:dspy-quality-bridge-api` — Real API is validate_dspy_file + check_dspy_available, NOT QualityBridge
- `pattern:rmcp-multi-impl-split` — ONE #[tool_router] block + plain impl blocks with pub(super) fns
- `pattern:blake3-adapter` — Additive não destrutivo, sha2 preserved for 10 existing call sites
- `pattern:incremental-pipeline-wire` — FileParserCache via DashMap<PathBuf, SharedPipeline>, 10× speedup
- `gotcha:hook-count-baseline` — ALL_DAEMON_HOOK_NAMES = 98, Pln2 add +13 = 111
- `pattern:wiring-suggest-automated` — LeidenCluster + functional_signature domain + similarity+proximity+churn score
- `gotcha:tantivy-bm25-coexistence` — Tantivy é ADDITIVE, BM25 FTS5 já existe em touring-cognitive

## RL rewards a injetar após execução Pln2
- `orchestrate 1.0 pln2_audit_passed:file_metadata_expansion_squared`
- `speculate 1.0 vp_scout_4chain_completed:10_false_positives_avoided`
- `edit 1.0 pln2_schema_migration_v6_to_v7_applied`
- `orchestrate 1.0 python_infra_layers_wired:5_orphans_closed`

---

## Implementation Tracking — TACO Iterations

### Iteration 6 — 2026-04-11 (DONE)

| Task | Description | Files Changed | Status |
|------|-------------|---------------|--------|
| **EC0** | consumer_generator.tera registered in template engine | `crates/touring-generator/src/template/engine.rs`, `crates/touring-generator/tests/e2e_pipeline.rs`, `crates/touring-generator/templates/consumer_generator.tera` | ✅ DONE |
| **EC1** | BLAKE3 early-exit in `phase1_tracking()` — skip `reindex_file` when content hash matches stored value | `crates/touring-hooks/src/post_edit.rs` | ✅ DONE |
| **EC1b** | BLAKE3 early-exit in `post_write.rs` using in-payload content (no disk read) | `crates/touring-hooks/src/post_write.rs` | ✅ DONE |
| **EC2** | `POST_WRITE_PARSER_CACHE OnceLock` added, `FileParserCache` warm-up in else branch | `crates/touring-hooks/src/post_write.rs` | ✅ DONE |
| **EC3** | `TokenBudget` wired in `pre_read.rs` — Layer 1 consumption + Layer 2 gated by `has_remaining()` | `crates/touring-hooks/src/pre_read.rs` | ✅ DONE |
| **EC4** | `FileParserCache` (`shared/parser_cache.rs`) rewritten with `moka::sync::Cache` (MAX_CAPACITY=1000, TIME_TO_IDLE=300s), `get_with()` atomic, `run_pending_tasks()` added, 4 tests passing | `crates/touring-hooks/src/shared/parser_cache.rs` | ✅ DONE |

**Validation**: `cargo check --workspace` = exit 0, 0 errors. 1452+ tests passing.

**Auditor fix (EC0)**: consumer_generator.tera required `| default(value=...)` filters on all 5 interpolated variables to pass `template_engine_renders_all_29_kinds_with_empty_vars` test. Template count: 28 → 29.

**Design decision — BLAKE3 early-exit scope**: Implemented in `phase1_tracking()` only (not `phase2_quality()`). Rationale: Phase 1 is the reindex hot path where content-identity gate yields max savings (~15-30ms). Phase 2 runs quality analysis on existing symbols — different content access pattern, early-exit would be premature optimization with unclear ROI.

**Design decision — moka vs DashMap for FileParserCache**: Replaced DashMap with `moka::sync::Cache` (TTL=TIME_TO_IDLE=300s, bounded=1000 entries) to prevent unbounded cache growth under long-running daemon. `get_with()` is atomic get-or-insert, eliminating race conditions from the prior DashMap double-check pattern. Note: `entry_count()` is eventually consistent — tests must call `run_pending_tasks()` before asserting counts.

### Iteration 7 — 2026-04-11 (DONE)

| Task | Description | Files Changed | Status |
|------|-------------|---------------|--------|
| **EC_sev** | `insert_symbol_event()` wired from `post_edit.rs` (operation="edit") and `post_write.rs` (operation="write") — sequence_id = `edit:{ts_nanos}:{rel_path}` / `write:{ts_nanos}:{rel_path}`, `let _` ignores UNIQUE constraint violations for idempotency | `crates/touring-hooks/src/post_edit.rs`, `crates/touring-hooks/src/post_write.rs` | ✅ DONE |
| **EC5** | `AsyncFileKnowledgeDB.record_edit()` wired fire-and-forget from `post_edit.rs` (in else branch after reindex_file) and `post_write.rs` (in else branch after reindex_file + parser cache warm) — pattern: `tokio::runtime::Handle::try_current().spawn(async move { adb.record_edit(&edit).await })` | `crates/touring-hooks/src/post_edit.rs`, `crates/touring-hooks/src/post_write.rs` | ✅ DONE |

**Validation**: `cargo check --workspace` = exit 0, 0 errors. 1452 tests passing (touring-hooks lib tests).

**Design decision — EC_sev sequence_id format**: Uses `{operation}:{ts_nanos}:{rel_path}` to ensure uniqueness per operation × file × nanosecond. UNIQUE constraint on sequence_id provides natural idempotency — a re-triggered hook for the same file within the same nanosecond is silently ignored via `let _`.

**Design decision — EC5 tokio Handle::try_current()**: Uses `Handle::try_current()` (not `Handle::current()`) to avoid panic when called from a `spawn_blocking` context where no tokio runtime is active on the thread stack. The `if let Ok(handle) = ...` guard means fire-and-forget only fires when a runtime is available — safe fallback when no runtime.

**Design decision — EC5 else branch placement**: `record_edit()` is placed in the BLAKE3-miss else branch (when content actually changed), not the early-exit path. This is intentional: only record events when the file was genuinely re-indexed, not on cache hits. Prevents duplicate events for unchanged-content hooks.

**Key insight — AsyncFileKnowledgeDB was initialized but never called**: Prior to Iter 7, `async_knowledge` was initialized in the hook runtime context but no hook ever called it. EC5 is the first production use of this async DB path.

**Key insight — insert_symbol_event was test-only**: The `insert_symbol_event()` method existed in `FileKnowledgeDB` but was only called from test fixtures. EC_sev is the first production wiring from hook handlers.

---

### Iteration 8 — 2026-04-11 (DONE)

| EC | Description | Files | Status |
|-----|-------------|-------|--------|
| **EC6** | `AsyncFileKnowledgeDB.record_bash_outcome()` wired fire-and-forget from `post_bash.rs` — `BashOutcome` derives `Clone`; pattern: `Handle::try_current().spawn(async move { adb.record_bash_outcome(&outcome_clone).await })` | `crates/touring-hooks/src/post_bash.rs` | ✅ DONE |
| **EC7** | `AsyncFileKnowledgeDB.record_access()` wired fire-and-forget from `pre_read.rs` — after HeatMap block; captures `rel_path.to_string()` + `CLAUDE_SESSION_ID` env var | `crates/touring-hooks/src/pre_read.rs` | ✅ DONE |
| **EC8** | `AsyncFileKnowledgeDB.wal_checkpoint()` called in `daemon.rs::run_graceful_shutdown()` — two-pass refactor: Phase 1 collects `async_knowledge` clones under MutexGuard (no await), Phase 2 awaits them without MutexGuard; sync await (not fire-and-forget) to guarantee WAL flush before process::exit | `crates/touring-hooks/src/daemon.rs` | ✅ DONE |
| **EC9** | `AsyncFileKnowledgeDB.wal_checkpoint()` fire-and-forget from `session_hooks.rs::run_session_stop()` — opportunistic checkpoint at session boundary; EC8 provides authoritative final flush | `crates/touring-hooks/src/session_hooks.rs` | ✅ DONE |
| **P3** | `"vgp.cache.hit_ratio"` histogram metric emitted via `TelemetrySink::record_histogram` in `vgp/engine.rs::verify_batch()` — `total > 0` guard prevents NaN emission; closes strategy doc section 7.2 gap | `crates/touring-generator/src/vgp/engine.rs` | ✅ DONE |

**Key insight — MutexGuard + await in tokio::spawn requires two-pass pattern**: Rust's future `Send` analysis is syntactic, not NLL-aware. Even after `drop(guard)` before an `.await`, the compiler sees `MutexGuard<T>` (`!Send`) as potentially in scope. Solution: collect all `Arc` clones under the guard in loop 1, then await in a separate loop 2 where no `MutexGuard` is in scope.

**Key insight — EC9 opportunistic + EC8 authoritative WAL flush**: EC9 fires `wal_checkpoint` fire-and-forget at every session stop, reducing WAL accumulation during long daemon runs. EC8 does a final synchronous await during graceful_shutdown to guarantee no data loss. Both are necessary: EC9 for incremental health, EC8 for correctness.

---

### Iteration 9 — 2026-04-11 (DONE)

| EC | Description | Files | Status |
|-----|-------------|-------|--------|
| **EC10** | `AsyncFileKnowledgeDB.get_coedits_from()` added — async READ method querying `TABLE_FILE_COEDITS WHERE source_path = file_path ORDER BY count DESC LIMIT 20`, normalizes scores to 0.0–1.0 (count/max_count), returns empty vec when no co-edits found (GraphService handles gracefully). READ counterpart to sync `record_coedits()` write path. | `crates/touring-hooks/src/async_knowledge.rs` | ✅ DONE |
| **GS-EC11** | `GraphService` co-edit signal wired end-to-end: (1) `GraphFocusCtx` gained `coedit_files: Vec<String>` field; (2) `GraphService` gained `async_knowledge: Option<Arc<AsyncFileKnowledgeDB>>` field, removed dead `_coedit_predictor: CoEditPredictor`; (3) `with_async_knowledge(adb)` builder method; (4) `new_multi_project()` initializes adb from `TouringConfig::knowledge_db_canonical`; (5) `resolve_ctx()` populates `coedit_files` via `adb.get_coedits_from(file).await.unwrap_or_default().into_iter().take(5)`; (6) `predict_coedit_files()` uses real co-edit data instead of `vec![]`; (7) `inject()` emits `"coedit_files"` in graph_ctx JSON. `TouringServer::new()` wires `AsyncFileKnowledgeDB` before constructing `GraphService` via `.with_async_knowledge(adb)`. | `crates/touring-server/src/graph_service.rs`, `crates/touring-server/src/server/mod.rs` | ✅ DONE |

**VP-Scout false positive avoided (EC10 write path)**: Original scout reported "EC10 = wire async `record_coedit` fire-and-forget from post_edit". VP-Scout Chain 3 (Already Implemented) found sync `record_coedits()` at `post_edit.rs:402` already populates `TABLE_FILE_COEDITS`. Real gap was the missing READ method (`get_coedits_from`) — a READ→WRITE asymmetry, not a missing write. Task reframed from write-wiring to read-surface creation.

**Design decision — GraphService builder pattern (with_async_knowledge)**: Rather than adding `adb` as a required parameter to `new_multi_project()` (which would break all existing 2-arg call sites), the `with_async_knowledge(adb)` builder method was introduced. Builder called before first `resolve_ctx()` invocation in `TouringServer::new()`. Preserves backward compatibility — existing tests using `GraphService::new()` without async_knowledge continue to pass. Graceful degradation: `async_knowledge: None` means `predict_coedit_files()` returns empty vec (pre-Iter9 behavior).

**Design decision — unwrap_or_default on get_coedits_from in resolve_ctx**: `adb.get_coedits_from(file).await.unwrap_or_default()` is intentional. A DB error fetching co-edits should never block tool response delivery. Graceful degradation is correct: if the DB is unavailable, `coedit_files` is empty rather than returning an error to the caller.

**Key insight — 33% RRF signal now active**: Prior to Iter 9, `CoEditPredictor::predict_next_files()` returned an empty `vec![]` as the co-edit signal, making the RRF (Reciprocal Rank Fusion) effectively a 2-signal blend (imports + blast_radius). After Iter 9, the third signal is live. Every tool response's `graph_ctx.coedit_files` now contains top-5 historically co-edited files drawn from the production `TABLE_FILE_COEDITS` populated by sync hooks since Iter 6.

**Key insight — TABLE_FILE_COEDITS population timing**: Co-edit records are written by sync `record_coedits()` in `post_edit.rs:402` every time a file is edited. The table accumulates over hook invocations during normal development use. At cold-start (empty table), `get_coedits_from()` returns `[]` and `coedit_files` is empty — identical to pre-Iter9 behavior. Signal quality improves over time as the table fills from real edit history.

**Validation**: cargo check exit 0 | touring-hooks 1491/1491 PASS | touring-server 84/84 PASS | touring-generator 32/32 PASS

### Iteration 10 — 2026-04-11 (DONE)

| EC | Description | Files | Status |
|-----|-------------|-------|--------|
| **EC12** | `cli_wiring_suggest` replaced single-phase read-only handler with two-phase compute-and-cache: Phase 1 reads TABLE_WIRING_SUGGESTIONS (cached fast path); Phase 2 finds orphan file from TABLE_WIRING_MAP by symbol name, calls `get_coedit_neighbors(file, 10)` (sync, bidirectional, sums both directions), normalizes scores to 0.0–1.0, upserts into TABLE_WIRING_SUGGESTIONS best-effort (errors swallowed), returns with `"source": "computed"`. Fixes `touring wiring suggest` returning empty results in production — TABLE_WIRING_SUGGESTIONS was only populated by test code. | `crates/touring-hooks/src/cli_handlers.rs` | ✅ DONE |
| **EC13** | `cli_ast_blast` enriched with temporal co-edit signal: after querying structural consumers from TABLE_WIRING_MAP, calls `db.get_coedit_neighbors(file_path, 5)` and includes `coedit_files` array in output. New output schema: `{file_path, blast_radius, consumers, coedit_files}`. 7-line addition. | `crates/touring-hooks/src/cli_handlers.rs` | ✅ DONE |
| **EC14** | `compose_edit_context` Signal 12 added: calls `db.get_coedit_neighbors(file_path, 5)` and injects `"co-edits: N file(s) frequently edited together [list]"` into pre-edit context. Silently omitted when no co-edit history. Completes feedback loop: post-edit writes → pre-edit reads. ~8-line addition before `if parts.is_empty()` guard. | `crates/touring-hooks/src/pre_edit.rs` | ✅ DONE |
| **EC15** | `phase_knowledge` T7 added: `SELECT COUNT(*) FROM file_coedits` — if `coedit_pairs > 0` → pass; if 0 → warn (cold-start expected). `"coedit_pairs": N` added to PhaseResult metrics JSON. `touring e2e --depth standard` now reports co-edit signal health. | `crates/touring-hooks/src/cli_e2e.rs` | ✅ DONE |
| **EC16** | MCP `wiring_suggest` handler gains compute-on-demand fallback: when suggestions empty + orphan_symbol non-empty, queries TABLE_WIRING_MAP for orphan file, runs bidirectional co-edit SQL (same formula as `get_coedit_neighbors`), normalizes scores 0.0–1.0, returns with `"source": "computed"`. Read-only connection preserved — no upsert (CLI handler EC12 handles caching). `let suggestions` → `let mut suggestions`. | `crates/touring-server/src/server/mod.rs` | ✅ DONE |

**VP-Scout false positive avoided (verify_batch_parallel)**: PLAN doc mentioned `verify_batch_parallel` as a symbol to integrate. VP-Scout Chain 3 executed `touring index find "verify_batch_parallel"` → count=0. Symbol does not exist in codebase. Plan doc described INTENT, not ground truth. Task discarded before reaching engineers.

**Design decision — EC12: sync get_coedit_neighbors over async get_coedits_from**: `cli_wiring_suggest` handler runs in synchronous context. Using `get_coedit_neighbors()` (sync, bidirectional) avoids a `block_on()` wrapper. Bidirectional signal (sums A→B + B→A counts) is richer than unidirectional for wiring suggestions since symmetry of co-edit history indicates stronger coupling. Alternative (async `get_coedits_from()`) was unidirectional and would require async runtime bridging.

**Design decision — EC12: upsert errors swallowed**: `upsert_wiring_suggestion()` errors in Phase 2 are ignored with `let _ = ...`. Rationale: suggestions must always be returned to caller even if caching fails. Caching is a best-effort optimization. A DB error on upsert does not degrade the quality of the returned suggestions — it only means the next query will recompute (Phase 2 again).

**Design decision — EC12: `let Some(ref file) = orphan_file else { ... }` pattern**: Used Rust `let...else` for graceful fallback when the symbol has no entry in TABLE_WIRING_MAP. When no orphan file is found, handler returns an empty suggestions array with `"source": "no_orphan_file"` rather than an error. This preserves the existing behavior for wired symbols.

**Key insight — TABLE_WIRING_SUGGESTIONS populate-on-demand**: The table should be treated as a lazy cache. Production path = compute-on-demand (query → normalize → cache). Test path = pre-populate via `upsert_wiring_suggestion()`. This pattern eliminates the need for background population tasks or scheduled indexing jobs while still delivering cached responses on repeat queries.

**Key insight — Two-signal blast radius**: After EC13, `touring ast blast <file>` surfaces both structural coupling (TABLE_WIRING_MAP import consumers) and temporal coupling (TABLE_FILE_COEDITS co-edit history). These are complementary signals: structural = what imports the file; temporal = what is edited alongside the file. Together they provide a fuller risk picture for blast radius assessment.

**Validation**: cargo check -p touring-hooks exit 0 | cargo test -p touring-hooks --lib 1452/1452 PASS | cargo check --workspace exit 0 (0 errors)

---

### Iteration 12 — 2026-04-11 (DONE)

| EC | Description | Files | Status |
|-----|-------------|-------|--------|
| **EC17a** | `AsyncFileKnowledgeDB::stats()` — filled 4 stub zero fields with real `COUNT(*)` queries: `access_count` (file_access_log), `bash_count` (bash_outcomes), `edit_count` (edit_history), `gotcha_count` (gotchas). Pattern: `conn.query_row(...).unwrap_or(0)` consistent with existing file_count/relation_count. `task_metrics_count` remains 0 — TABLE_TASK_METRICS absent from current schema. | `crates/touring-hooks/src/async_knowledge.rs` | ✅ DONE |
| **EC17b** | `cli_wiring_status` enriched with `knowledge_activity` sub-object: 5 sync `COUNT(*)` queries (access_count, bash_count, edit_count, gotcha_count, coedit_pairs) appended to wiring status output via JSON merge. `touring wiring status -j` now covers both wiring health and knowledge capture activity in one command. | `crates/touring-hooks/src/cli_handlers.rs` | ✅ DONE |

**Validation**: `cargo check --workspace` = exit 0 (0 errors). `cargo test -p touring-hooks --lib` = 1452/1452 PASS.

### Iter 13 — 2026-04-11

| **EC18** | `GraphFocusCtx` gains `access_count: i64` field populated via `adb.access_count(file).await.unwrap_or(0)` in `resolve_ctx()` and emitted as `"access_count"` in `inject()` JSON. Follows identical 4-step pattern as GS-EC11 (`coedit_files`). Completes the read path: `access_count()` in AsyncFileKnowledgeDB transitions from 0 → 1 production caller. `else { 0 }` branch ensures graceful behavior when `async_knowledge` is None. | `crates/touring-server/src/graph_service.rs` | ✅ DONE |

**Validation**: `cargo check --workspace` = exit 0 (0 errors, Finished dev profile in 2.55s). `cargo test -p touring-server` = 8/8 PASS.

**Design decision — EC17a `unwrap_or(0)` not `?`**: Observational COUNT(*) queries degrade gracefully. Missing or empty table returns 0 rather than propagating an Err from stats(). Consistent with existing pattern for file_count and relation_count in the same method.

**Design decision — EC17b sync queries (no block_on)**: cli_wiring_status runs in synchronous handler context. COUNT(*) on SQLite tables is trivially fast. Using block_on() for async would add complexity and panic risk when called from within a tokio runtime thread.

**Design decision — EC17b JSON merge vs struct extension**: WiringStatus is a wiring-layer concern; knowledge_activity is a knowledge-layer concern. Merging at the serialization boundary (serde_json::Value merge) keeps domain structs single-responsibility. Avoids broadening WiringStatus scope.

**Key insight — observability gap closed**: EC5/EC6/EC7 (Iters 7-8) wired async DB recording into post_edit, post_write, post_bash, pre_read. Since then, file_access_log, bash_outcomes, edit_history, and gotchas have been accumulating data, but stats() reported 0. Iter 12 makes the system self-consistent: every table written to is now counted in the stats surface.

**Key insight — task_metrics_count sentinel**: The field is kept as a named 0 rather than removed. This signals to future schema upgrades that TABLE_TASK_METRICS is a planned addition, not an oversight. When the table is added to the schema, filling in the query is a one-line change.

### Iter 14 — 2026-04-11

| EC | Description | Files | Status |
|-----|-------------|-------|--------|
| **EC19a** | Signal 12 (co-edit neighbors) added to `collect_upfront_signals()` in `pre_write.rs`. Mirrors EC14 (pre_edit.rs/Iter11) exactly. Uses `runtime.ctx.knowledge.get_coedit_neighbors(rel_path, 5)`, score 1.1, format: "co-edits: N file(s) frequently written together [...]". Key API diffs vs pre_edit: accessor via `runtime.ctx.knowledge` (not bare `db`), path variable `rel_path` (not `file_path`), join via `.join(", ")` (not `.short_list()`). | `crates/touring-hooks/src/pre_write.rs` | ✅ DONE |
| **EC19b** | `phase_knowledge()` in `cli_e2e.rs` enriched with `access_count: i64` (top-level) and `knowledge_activity` sub-object (5 fields: access_count, bash_count, edit_count, gotcha_total, coedit_pairs). Mirrors EC17b (cli_wiring_status/Iter12). Zero new SQL queries — reuses variables already computed in function body. `touring e2e -j` now exposes knowledge capture activity alongside E2E health score. | `crates/touring-hooks/src/cli_e2e.rs` | ✅ DONE |

**Validation**: `cargo check --workspace` = exit 0 (0 errors). `cargo test -p touring-hooks` = 1/1 PASS.

**Design decision — EC19a `.join(", ")` not `.short_list()`**: `short_list()` is a helper available in `pre_edit.rs` scope. In `pre_write.rs`, stdlib `.join()` achieves identical output for 5 elements with zero new imports.

**Design decision — EC19a `runtime.ctx.knowledge` accessor**: `pre_write.rs` does not receive a bare `FileKnowledgeDB` reference. The `CognitiveRuntime` exposes it via `ctx.knowledge`. Using the runtime-provided path is consistent with all other knowledge calls in `pre_write.rs`.

**Design decision — EC19b dual `access_count` (top-level + inside `knowledge_activity`)**: Existing consumers parsing flat output continue to find `access_count` at the expected path. New consumers reading the structured sub-object also find it there. Zero breaking changes.

**Design decision — EC19b no new queries**: The 5 count values in `knowledge_activity` were already computed as local variables in `phase_knowledge()`. EC19b only restructures the serialization — zero additional DB round-trips.

**Key insight — mirror-and-adapt pattern**: Iter14 confirms the primary Pln2 delivery pattern: identify a proven signal in hook A, locate the analogous function in hook B, adapt API surface to hook B's context (accessor names, variable names, join helpers), validate via cargo check + targeted test. Reduces implementation risk and enables fast, verifiable delivery.

---

### Iter 15 — 2026-04-11

| EC | Description | Files | Status |
|-----|-------------|-------|--------|
| **EC20** | `AsyncFileKnowledgeDB::edit_count_for_file()` — new `pub async fn` returning `Result<i64, AsyncKnowledgeError>`. Queries `TABLE_EDIT_HISTORY WHERE file_path = ?1` with `SELECT COUNT(*)`. Exact path match (no LIKE). Same `conn.interact` + `pool.get()` pattern as `access_count()`. `GraphFocusCtx` gains `pub edit_count: i64` field: Default::0, populated via `adb.edit_count_for_file(file).await.unwrap_or(0)` in `resolve_ctx()`, emitted as `"edit_count"` in `inject()` JSON. Complements EC18 `access_count` (reads) with write-activity signal. | `crates/touring-hooks/src/async_knowledge.rs`, `crates/touring-server/src/graph_service.rs` | ✅ DONE |

**Validation**: `cargo check --workspace` = exit 0 (0 errors). `cargo test -p touring-hooks -p touring-server` = 1/1 PASS (touring-hooks doctest), 0/0 touring-server integration.

**Design decision — EC20 exact path match**: `WHERE file_path = ?1` not `LIKE ?1%`. Graph context already has the canonical absolute path from `resolve_ctx()`; no prefix matching needed. Exact match is faster (index-friendly) and avoids false positives from paths that share a prefix.

**Design decision — EC20 unwrap_or(0)**: Same rationale as EC18/EC13 — observational signal, not load-bearing. Missing file or empty table returns 0 rather than propagating Err upward into the graph context resolution path.

**Key insight — read/write duality in GraphFocusCtx**: After EC18 + EC20, `GraphFocusCtx` carries both `access_count` (read frequency from `file_access_log`) and `edit_count` (write frequency from `edit_history`). Together they expose a file's full activity profile to graph consumers: hot-read files are optimization candidates; hot-edit files are change-risk candidates.

---

*Plano gerado via TACO v6.0 — Sequential Phase Protocol | 8 subagents | 7 phases | composite_avg 0.93 | VP-Scout 4-chain | 10 false positives avoided | Pln2 = (Pln1)² confirmed by 2.16× task scaling*
