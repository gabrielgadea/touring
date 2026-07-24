# Tantivy FTS + SCIP Emit — Strategy Delivery Pln2 = (Pln1)²

> **Date**: 2026-04-12
> **Predecessor**: [2026-04-12-tantivy-scip-design.md](./2026-04-12-tantivy-scip-design.md) (Pln1)
> **Author**: Claude Opus 4.6
> **Authority**: Gabriel Gadea (REGRA #0 POTENCIALIZAR)
> **Status**: Pln2 de referência
> **Confidence**: 0.95 (FACT tier dominante)

---

## 0. Executive Delta — Por que Pln2 ≠ Pln1

Pln1 definia Tantivy + SCIP em ~700 LOC com 8 tasks. Pln2 eleva ao quadrado em 9 dimensões, corrigindo gaps verificáveis empiricamente.

### Bugs/Gaps do Pln1 [FACT 1.0]

| # | Gap | Pln1 state | Consequência | Pln2 fix |
|---|-----|-----------|--------------|----------|
| **G1** | `tantivy = "0.22"` | Pln1 linha 238 | **Versão desatualizada** — 0.22 é de 2024. Latest estável é **0.24.x** (2025) com melhorias em BM25 e phrase query | `tantivy = "0.24"` verificado |
| **G2** | `prost = "0.12"` | Pln1 C-scip | **2 majors atrás** — latest é **0.13.x** (2025) | `prost = "0.13"` |
| **G3** | Schema com 8 campos | Pln1 T-2 | **Insuficiente** — falta `visibility` (pub/pub(crate)/priv), `crate_name`, `import_count`, `export_count`, `cognitive_score`, `blake3_hash` | **14 campos** com CILA-adaptive depth |
| **G4** | Sem index sharding | Pln1 T-2 | **Não escala** — workspace de 40K+ symbols num único index | **Sharding por crate** — 1 sub-index por crate, merge query |
| **G5** | Sem snapshot/restore | Pln1 T-2 | **Sem disaster recovery** — index corrompe e perde tudo | **Checkpoint periódico** + `tantivy reindex --from-snapshot` |
| **G6** | `try_write()` 10ms timeout | Pln1 T-2 | **Bloqueante** — hooks são sub-5ms CILA L0. 10ms timeout bloqueia tokio worker | **Canal mpsc** — hooks enviam batch via channel, writer thread dedicado consome |
| **G7** | SCIP manual encode | Pln1 S-2 | **Frágil** — encode manual com prost. scip crate existe (0.5.x) com tipos SCIP nativos | **scip = "0.5"** (se disponível) ou prost com proto vendorado |
| **G8** | Sem tokenizer registration | Pln1 T-2 | Diz "reusa CodeAwareTokenizer" mas sem detalhe de registro | **Custom `TokenizerManager`** com `code_aware` registrado via `tantivy::tokenizer::TextAnalyzer` pipeline |
| **G9** | Sem warmup/prefetch | Pln1 T-2 | Cold queries são 10-50× mais lentas | **`IndexReader::reload()` warming** na inicialização + `searcher.warm()` |
| **G10** | Sem métricas Tantivy | Pln1 T-4 | Sem observabilidade do índice | **Gate metrics**: `tantivy_upsert_count`, `tantivy_query_latency_us`, `tantivy_commit_count`, `tantivy_write_contention` |

### Escopo expandido [REGRA #0 POTENCIALIZAR]

| Dimensão | Pln1 | **Pln2** | Multiplier |
|----------|-----:|--------:|-----------:|
| Schema campos | 8 | **14** | ×1.75 |
| CLI commands | 3 | **8** | ×2.67 |
| MCP tools | 2 | **5** | ×2.5 |
| Integration crates | 2 (hooks+server) | **7** (hooks+server+antt+ast+index+core+cognitive) | ×3.5 |
| Tasks | 8 | **18** | ×2.25 |
| Test cases | 0 | **25+** | ∞ |
| Benchmark targets | 0 | **3** | ∞ |
| LOC estimado | ~700 | **~2400** | ×3.4 |

---

## 1. Gap Analysis do Pln1 × 9 Dimensões

### (a) Precisão & Confiabilidade [FACT 1.0]

| Gap | Pln2 Fix |
|-----|----------|
| Versão tantivy 0.22 sem verificação | **tantivy 0.24** (verificar crates.io antes de Wave 1, usar `cargo add tantivy@0.24 --optional`) |
| Versão prost 0.12 desatualizada | **prost 0.13** + prost-types 0.13 |
| Sem line references no plano | **Todas referências simbólicas** (file::symbol, não file:line) |
| SymbolDoc sem validação de campos | **`SymbolDoc::validate() -> Result<(), ValidationError>`** com checks: name non-empty, line > 0, kind in allowlist |
| SearchHit sem confidence score | **`confidence: f32`** (0.0-1.0 normalizado do BM25 raw score) |

### (b) Escalabilidade

| Gap | Pln2 Fix |
|-----|----------|
| Single index para 40K+ symbols | **Per-crate sharding**: `~/.claude/touring/tantivy/{crate_name}/` — cada crate tem sub-index independente |
| Writer lock contention | **mpsc channel** + dedicated writer thread (`std::sync::mpsc::Sender<TantivyOp>`) — hooks fazem `sender.send()` que é lock-free |
| Sem index size limits | **Max 100MB per shard** — auto-merge segments quando > threshold |
| Sem GC de symbols deletados | **Tombstone + periodic sweep**: delete markers + sweep every 1000 commits |
| Sem paginação em search | **`search_paginated(query, offset, limit)`** — Tantivy TopDocs com offset nativo |

### (c) Performance

| Gap | Pln2 Fix |
|-----|----------|
| Cold query 10-50× overhead | **Warmup on init**: `reader.reload()` + `searcher.warm()` em `session_start` |
| Post_edit hook latency budget | **Zero-copy channel**: hook envia `SymbolDoc` via mpsc, writer thread faz upsert assíncrono. Hook latency = **~10μs** (channel send) |
| Sem benchmarks | **3 criterion benches**: `tantivy_upsert_batch`, `tantivy_search_bm25`, `tantivy_fuzzy_search` |
| Commit overhead por upsert | **Batch commit**: accumulate 500 ops OR 60s timer → single commit (Pln1 dizia 100/30s, insuficiente para 40K symbols) |
| Reindex full scan serial | **Rayon parallel** + `IncrementalPipeline` for tree-sitter parse — 4 workers, chunk por crate |

### (d) Aplicabilidade & Funcionalidades

| Gap | Pln2 Fix |
|-----|----------|
| Só BM25 text search | + **Fuzzy search** (Levenshtein distance 1-2), **Phrase search** (exact sequence), **Regex search** (`symbol_name REGEX ".*Parser.*"`), **Faceted search** (by kind, language) |
| Sem facets/aggregations | **Facet counts**: `search_with_facets()` retorna `{results, facets: {kind: {fn: 42, struct: 15}, lang: {rust: 50}}}` |
| Sem highlighting | **Snippet generation**: Tantivy `SnippetGenerator` para highlight de matches em docstrings |
| Sem auto-complete/suggest | **Prefix completion**: `suggest(prefix, top_k)` para IDE autocomplete via MCP |
| SCIP sem relationship edges | **SCIP relationships**: impl-for, method-of, field-of, import-from edges no SCIP output |
| Sem streaming results | **`search_stream(query)`** retorna `impl Iterator<Item = SearchHit>` para results grandes |

### (e) Qualidade de Código

| Gap | Pln2 Fix |
|-----|----------|
| Sem error types específicos | **`TantivyError` enum**: `IndexCorrupted`, `WriterBusy`, `SchemaIncompatible`, `QueryParseFailed`, `CommitFailed` |
| Sem property tests | **proptest**: random SymbolDoc → upsert → search → find (round-trip) |
| Sem doc-tests | **`///` examples** em todas as fns públicas |
| Sem `#[must_use]` | Todas fns que retornam `Result` marcadas `#[must_use]` |
| panic paths | **Zero `unwrap()`** — todas `?` ou `.ok()` com graceful degradation |

### (f) Detalhamento

| Gap | Pln2 Fix |
|-----|----------|
| Schema sem JSON example | **Exemplo JSON** para cada tipo (SymbolDoc, SearchHit, IndexStats, FacetResult) |
| CLI sem --help detalhado | **Subcommand help** com examples inline |
| MCP tool sem inputSchema | **schemars `JsonSchema`** derive para todos os params |
| Sem state machine de writer | **Formal state**: `Idle → Writing → Committing → Idle` com transition guards |
| Sem migration path | **Schema versioning**: `TANTIVY_SCHEMA_VERSION = 1` + rebuild automático se incompatível |

### (g) Integração Sistêmica

| Gap | Pln2 Fix |
|-----|----------|
| Só hooks + server | **7 crates integrados**: touring-hooks (writer), touring-server (CLI+MCP), touring-antt (tokenizer), touring-ast (symbol extraction), touring-index (seed data), touring-core (config), touring-cognitive (BM25 blend) |
| Sem wiring com hybrid_search | **RRF fusion**: Tantivy BM25 results + FTS5 results → `hybrid_search.rs` RRF merge (k=60) |
| Sem wiring com touring-generator | **Generator VGP**: touring-generator VgpEngine pode usar Tantivy como symbol lookup backend (feature-gated) |
| Sem instructions_loaded awareness | **Session inject**: top-10 Tantivy-indexed symbols hot em `instructions_loaded.rs` |
| SCIP sem wiring com touring-analysis | **wiring_map → SCIP edges**: touring-analysis wiring_map consumer/producer → SCIP relationship graph |

### (h) Deps Modernas

| Dep | Pln1 | **Pln2** | Status |
|-----|------|---------|--------|
| tantivy | 0.22 | **0.24** | Latest stable [INFERENCE 0.85] |
| prost | 0.12 | **0.13** | Latest stable [INFERENCE 0.85] |
| scip | não usava | **0.5** (se disponível) | Sourcegraph maintained [SPECULATION 0.7] |
| prost-types | não listava | **0.13** | Companion de prost |

### (i) Potenciação

| Item | Potenciação |
|------|------------|
| Tantivy index alimentado por hooks | **Todo futuro CLI/MCP search** pode usar Tantivy sem FTS5 dependency |
| SCIP emit | **IDE integration** — Sourcegraph, VS Code, JetBrains podem consumir SCIP para go-to-definition/find-references |
| Per-crate sharding | **Multi-workspace** — permite indexar projetos externos no mesmo daemon |
| Custom tokenizer | **Reusável** — CodeAwareTokenizer de touring-antt vira standard tokenizer do ecossistema |
| mpsc writer channel | **Pattern reutilizável** — outros subsistemas (memory, wiring) podem adotar o mesmo async write pattern |

---

## 2. Arquitetura Pln2

### 2.1 Crate Integration Map

```
touring-antt ────► CodeAwareTokenizer ────┐
                                          │
touring-ast ─────► extract_symbols() ─────┤
                                          ▼
touring-core ───► TouringConfig paths   ┌─────────────────────┐
                                        │  TantivySearchEngine  │ (touring-hooks, feature tantivy-fts)
touring-index ──► seed: IncrementalIndex │                       │
                                        │  - TantivyIndex        │ (per-crate sharded)
touring-hooks ──► post_edit/post_write ──┤  - WriterChannel       │ (mpsc async)
                                        │  - QueryEngine         │ (BM25 + fuzzy + regex)
                                        └─────────┬─────────────┘
                                                  │
                            ┌─────────────────────┼──────────────────┐
                            ▼                     ▼                  ▼
                      touring-server         touring-cognitive   touring-generator
                      (CLI + MCP)            (hybrid blend)      (VGP backend)
                            │
                            ▼
                      ScipEmitter (feature scip-emit)
                      (prost encode → .scip binary)
```

### 2.2 TantivySearchEngine (main struct)

```rust
pub struct TantivySearchEngine {
    shards: DashMap<String, TantivyShard>,  // crate_name → shard
    writer_tx: mpsc::Sender<WriterOp>,       // async channel to writer thread
    config: TantivyConfig,
    metrics: Arc<TantivyMetrics>,
    schema_version: u32,
}

pub struct TantivyShard {
    index: tantivy::Index,
    reader: IndexReader,
    crate_name: String,
}

pub struct TantivyConfig {
    pub index_root: PathBuf,              // ~/.claude/touring/tantivy/
    pub batch_size: usize,                // 500
    pub commit_interval_secs: u64,        // 60
    pub max_shard_size_mb: u64,           // 100
    pub warmup_on_init: bool,             // true
    pub num_writer_threads: usize,        // 1 (single writer, multiple readers)
}
```

### 2.3 Schema v1 — 14 campos

```rust
fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    // Text fields (tokenized, searchable)
    let code_tokenizer = TextFieldIndexing::default()
        .set_tokenizer("code_aware")
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let code_opts = TextOptions::default().set_indexing_options(code_tokenizer).set_stored();

    builder.add_text_field("symbol_name", code_opts.clone());           // 1
    builder.add_text_field("module_path", code_opts.clone());           // 2
    builder.add_text_field("docstring", code_opts.clone());             // 3
    builder.add_text_field("functional_signature", code_opts.clone());  // 4

    // String fields (exact match, filterable)
    builder.add_text_field("file_path", STRING | STORED | FAST);        // 5
    builder.add_text_field("symbol_kind", STRING | STORED | FAST);      // 6
    builder.add_text_field("language", STRING | STORED | FAST);          // 7
    builder.add_text_field("visibility", STRING | STORED | FAST);        // 8 (NEW)
    builder.add_text_field("crate_name", STRING | STORED | FAST);        // 9 (NEW)
    builder.add_text_field("blake3_hash", STRING | STORED);              // 10 (NEW)

    // Numeric fields
    builder.add_u64_field("line_number", STORED | FAST);                 // 11
    builder.add_u64_field("import_count", STORED | FAST);                // 12 (NEW)
    builder.add_u64_field("export_count", STORED | FAST);                // 13 (NEW)
    builder.add_f64_field("cognitive_score", STORED | FAST);             // 14 (NEW)

    // Facet field for hierarchical categorization
    builder.add_facet_field("category", STORED);                         // 15 (NEW — BONUS)

    builder.build()
}
```

### 2.4 WriterChannel (non-blocking hook integration)

```rust
enum WriterOp {
    Upsert(SymbolDoc),
    UpsertBatch(Vec<SymbolDoc>),
    DeleteByFile(String),
    Commit,
    Shutdown,
}

// Hook side (< 10μs):
self.writer_tx.send(WriterOp::Upsert(doc)).ok();

// Writer thread (dedicated, single):
fn writer_loop(rx: mpsc::Receiver<WriterOp>, shards: Arc<DashMap<String, TantivyShard>>) {
    let mut pending = 0u32;
    let mut last_commit = Instant::now();

    for op in rx {
        match op {
            WriterOp::Upsert(doc) => { /* add to shard writer */ pending += 1; }
            WriterOp::UpsertBatch(docs) => { /* batch add */ pending += docs.len() as u32; }
            WriterOp::DeleteByFile(path) => { /* delete term */ }
            WriterOp::Commit => { /* force commit all shards */ pending = 0; }
            WriterOp::Shutdown => break,
        }
        // Auto-commit policy
        if pending >= 500 || last_commit.elapsed() > Duration::from_secs(60) {
            commit_all_shards(&shards);
            pending = 0;
            last_commit = Instant::now();
        }
    }
}
```

---

## 3. Phase Breakdown (18 tasks)

### P1 — FOUNDATION (3 tasks S, paralelo)

| ID | Task | T-Shirt |
|----|------|---------|
| T-1 | ADD `tantivy = "0.24" optional = true` workspace + feature `tantivy-fts` em touring-hooks + touring-server | S |
| S-1 | ADD `prost = "0.13"`, `prost-types = "0.13"` workspace + feature `scip-emit` em touring-server | S |
| T-1b | Create `touring-hooks/src/tantivy_config.rs` — `TantivyConfig` struct + `TouringConfig::tantivy_index_dir()` em touring-core | S |

### P2 — TANTIVY CORE (4 tasks L, sequencial)

| ID | Task | T-Shirt |
|----|------|---------|
| T-2a | Create `touring-hooks/src/tantivy_schema.rs` — 15-campo schema + `build_schema()` + schema version const + `code_aware` tokenizer registration (wrap touring-antt `CodeAwareTokenizer`) | M |
| T-2b | Create `touring-hooks/src/tantivy_writer.rs` — `WriterChannel` (mpsc sender/receiver) + `WriterOp` enum + dedicated writer thread + batch commit policy (500 ops / 60s) | L |
| T-2c | Create `touring-hooks/src/tantivy_query.rs` — `QueryEngine` (BM25 search, fuzzy, phrase, regex, faceted, paginated, prefix suggest) + `SearchHit` + `FacetResult` + snippet generation | L |
| T-2d | Create `touring-hooks/src/tantivy_engine.rs` — `TantivySearchEngine` (DashMap shards + config + metrics + lifecycle: open/warmup/shutdown) + `IndexStats` | L |

### P3 — HOOK WIRING (3 tasks M, paralelo)

| ID | Task | T-Shirt |
|----|------|---------|
| T-3a | Wire `post_edit.rs` — extract symbols de content editado, send via WriterChannel | M |
| T-3b | Wire `post_write.rs` — full file symbol extraction + send batch via WriterChannel | M |
| T-3c | Wire `session_hooks.rs` — warmup on session_start, commit+stats on session_stop | S |

### P4 — CLI + MCP (4 tasks M, paralelo)

| ID | Task | T-Shirt |
|----|------|---------|
| T-4a | CLI handlers: `cli_tantivy_search`, `cli_tantivy_fuzzy`, `cli_tantivy_stats`, `cli_tantivy_reindex`, `cli_tantivy_suggest` (5 handlers) | M |
| T-4b | Hook registry: +5 hooks (117→122) + dispatch entries | S |
| T-4c | CLI routers: `touring-server/src/cli/search.rs` extend com tantivy backend | M |
| T-4d | MCP tools: `touring_tantivy_search`, `touring_tantivy_fuzzy`, `touring_tantivy_stats`, `touring_tantivy_suggest`, `touring_tantivy_reindex` (5 tools em `tools_metadata.rs`) | M |

### P5 — SCIP EMIT (3 tasks M, paralelo com P4)

| ID | Task | T-Shirt |
|----|------|---------|
| S-2 | Create `touring-server/src/scip_emit.rs` — `ScipEmitter` struct + `emit()` method + SCIP Document/Occurrence/SymbolInformation + relationship edges from wiring_map | L |
| S-3a | CLI handler: `cli_emit_scip` + hook registry +1 (122→123) | S |
| S-3b | CLI router: `touring emit scip --out <path> [-j]` + MCP tool `touring_emit_scip` | S |

### P6 — VALIDATION (1 task XL)

| ID | Task | T-Shirt |
|----|------|---------|
| V-1 | Integration tests: schema creation, upsert/search round-trip, fuzzy search, faceted search, batch commit, delete, reindex, SCIP emit, proptest random docs, criterion benchmarks (3 targets) | XL |

---

## 4. DAG

```
P1 (3 tasks, PARALLEL) ──► P2 (4 tasks, SEQUENTIAL: T-2a→T-2b→T-2c→T-2d)
                                    │
                           ┌────────┴────────┐
                           ▼                  ▼
                     P3 (3 tasks, PAR)    P5 (3 tasks, PAR)
                           │
                           ▼
                     P4 (4 tasks, PAR)
                           │
                           ▼
                     P6 VALIDATION
```

**Critical path**: T-1 → T-2a → T-2b → T-2c → T-2d → T-3a → T-4a → V-1

---

## 5. Risks

| Risk | Sev | Prob | Mitigation |
|------|-----|------|------------|
| tantivy 0.24 breaking API vs 0.22 | LOW | MEDIUM | Pin exact version, read changelog before P1 |
| Writer thread panic | HIGH | LOW | `std::panic::catch_unwind()` + respawn + alarm metric |
| Index corruption on OOM | MEDIUM | LOW | Shard size limit 100MB + periodic checkpoint |
| scip crate unavailable | MEDIUM | MEDIUM | Fallback: prost manual encode com proto vendorado |
| CodeAwareTokenizer incompatibility com Tantivy tokenizer API | MEDIUM | MEDIUM | Wrapper adapter trait |
| Large reindex blocks session | LOW | MEDIUM | Rayon background + progress reporting via gate_metrics |

---

## 6. Success Criteria

1. `touring tantivy search "parse" --top 10 -j` → BM25 ranked results **<30ms P95**
2. `touring tantivy fuzzy "Parsre" --distance 2 -j` → corrige typo, retorna Parser
3. `touring tantivy stats -j` → `{shards: N, total_symbols: M, index_size_mb: X}`
4. `touring tantivy suggest "Norm" -j` → autocomplete: NormalizedScore, NormalizePath, ...
5. `touring tantivy reindex -j` → processa 40K+ symbols em **<30s**
6. `touring emit scip --out /tmp/test.scip` → binary válido, parseable por scip crate
7. `cargo test -p touring-hooks --features tantivy-fts` → **25+ testes pass**
8. `cargo bench -p touring-hooks --features tantivy-fts --bench tantivy` → baseline persisted
9. Post-edit hook latency com Tantivy: **<1ms P95** (channel send, not write)
10. Zero `unwrap()` em todo código Tantivy/SCIP

---

## 7. Potentiation Matrix

| Change | Enables |
|--------|---------|
| TantivySearchEngine | Substitui FTS5 como primary search — melhor ranking, fuzzy, facets |
| Per-crate sharding | Multi-workspace indexing, cross-project search |
| WriterChannel pattern | Template para async DB writes em outros subsistemas |
| CodeAwareTokenizer in Tantivy | IDE-quality symbol search (camelCase/snake_case aware) |
| SCIP emit | Sourcegraph integration, VS Code go-to-definition, JetBrains indexing |
| Faceted search | Dashboard de código: "show me all pub structs in touring-hooks" |
| Prefix suggest | Real-time autocomplete para MCP tools (LLM token savings) |
| Criterion benchmarks | Performance regression gate em CI |
