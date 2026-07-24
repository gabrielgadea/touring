# Moka Expansion Wave — 2026-04-16

> **Ranking**: P1 #6 (score 8.4 pós-Context7) de `docs/analyses/2026-04-14-crates-inventory-ranking.md`
> **Status**: ✅ IMPLEMENTADO
> **Testes**: 9 E2E + 16 unit = 25 verdes | **Clippy**: limpo em touring-hooks + touring-learning
> **CRC**: L3 Refactoring — structural change com comportamento preservado + gates 6/6 PASS

---

## Problema

Três hot-paths do daemon recomputavam trabalho caro a cada invocação:

| Hot-path | Custo anterior | Chamadas típicas por sessão |
|---|---|---|
| `FileKnowledgeDB::query_extended` | 6-way LEFT JOIN SQLite | centenas (pre_read + pre_edit + pre_write + cli_handlers) |
| `TantivyIndex::{search,fuzzy_search,suggest}` | BM25 scoring sobre 1.1M docs | dezenas por hook |
| `job_registry` jobs terminais | `DashMap` retido indefinidamente mesmo após `drop_job` | acumula em daemons long-running |

O inventory-ranking recomendou moka W-TinyLFU (Context7 bench 92.1) mas apontava
que o wiring em JobRegistry ainda dependia de `DashMap`. Outros dois sites
(query_extended + Tantivy query cache) não tinham absorção alguma.

---

## Solução implementada

### 1. Módulo compartilhado `shared::moka_policies` (NOVO)

`crates/touring-hooks/src/shared/moka_policies.rs` (~170 LOC + 4 unit tests)

- Builders padronizados:
  - `build_knowledge_extended_cache::<T>()` — capacity 4096, TTL 120s, TTI 60s
  - `build_tantivy_query_cache::<T>()` — capacity 1024, TTL 30s, TTI 15s, weigher por hit-count
  - `build_terminal_job_cache::<T, F>(weigher)` — 32 MiB cap, TTL 30m, TTI 15m
- `MokaCacheStats` — snapshot serializable (entry_count, weighted_size)
- `sample_stats` — flush pending + colher métricas deterministicamente

**Por que centralizar?** Tuning de capacity/TTL/weigher agora muda num único arquivo; consumidores não precisam saber sobre política de eviction.

### 2. `FileKnowledgeDB.query_extended` — cache LEFT JOIN

`crates/touring-hooks/src/knowledge.rs`

```rust
pub struct FileKnowledgeDB {
    conn: Connection,
    extended_cache: Cache<String, Arc<FileKnowledgeEnriched>>,
}
```

- `new()` e `from_conn()` inicializam o cache via `build_knowledge_extended_cache()`
- `query_extended` verifica cache → miss executa LEFT JOIN → resultado é `Arc::new` e inserido
- Invalidação wired em 5 writers para prevenir stale reads:
  - `upsert` (base row)
  - `upsert_cognitive_enrichment`
  - `upsert_blake3_registry`
  - `upsert_file_community`
  - `upsert_test_coverage`
- API pública nova: `invalidate_extended_cache(path)`, `invalidate_extended_cache_all()`, `extended_cache_stats()`

### 3. `TantivyIndex` — BM25 query cache

`crates/touring-hooks/src/tantivy_index.rs`

```rust
pub struct TantivyIndex {
    // ...
    query_cache: moka::sync::Cache<String, Arc<Vec<SearchHit>>>,
    query_cache_hits: AtomicU64,
    query_cache_misses: AtomicU64,
}
```

- Chave estável: `"{op}\x1f{query}\x1f{top_k}"` (unit-separator evita colisões)
- Helper `cached_query(op, query, top_k, run)` funnels `search`/`fuzzy_search`/`suggest` através de um único cache cycle
- `search_uncached`/`fuzzy_search_uncached`/`suggest_uncached` encapsulam o BM25 real
- Invalidação automática em `commit()` e `delete_by_file()` — evita resultados servindo documentos já removidos
- API pública nova: `query_cache_counters()` (hits/misses), `query_cache_stats()`, `invalidate_query_cache()`

### 4. `shared::terminal_job_cache` (NOVO)

`crates/touring-hooks/src/shared/terminal_job_cache.rs` (~170 LOC + 5 unit tests)

- Substitui retenção indefinida no `DashMap` para jobs que já concluíram
- `TerminalJobRecord` carrega payload como `Arc<str>` — clones O(1)
- `store_terminal(id, json)` aceita apenas status `"completed"|"failed"`
- `lookup_terminal(id)` reconstitui `serde_json::Value` do payload
- `forget(id)` / `stats()` / `clear_for_tests()` completam a API
- Integração em `job_registry`:
  - `poll_worker` detecta terminal → `store_terminal` com snapshot
  - `drop_job` purga DashMap E cache
  - Fallback no `poll_worker`: client polls após drop_job → lookup_terminal serve o resultado até eviction

### 5. Fixes pre-existentes (REGRA #0 POTENCIALIZAR)

Resolvidos como parte da wave:

| Erro | Arquivo | Fix |
|---|---|---|
| 14 compile errors em `touring-python` (8 métodos faltantes em `MutableGeneratorGraph`) | `touring-learning/src/aco/graph.rs` | Adicionei `len`, `topological_sort`, `validate_acyclic`, `compute_levels`, `get_all_levels`, `detect_parallelizable`, `validate_contracts`, `validate_dependencies`, `freeze` |
| Clippy `redundant field names` | `touring-learning/src/aco/evolution.rs:104` | Shorthand |
| Clippy `iterate on map's keys` | `touring-learning/src/aco/graph.rs:223` | `.keys()` |
| Clippy `unnecessary closure` | `touring-learning/src/aco/graph.rs:664` | `.ok_or(...)` |
| Clippy `impl can be derived` (2×) | `touring-learning/src/aco/models.rs:178, 204` | `#[derive(Default)]` |
| Unused import `petgraph::Directed` | `touring-learning/src/aco/graph.rs:8` | Removido |

---

## Prova funcional

### Unit tests

```
shared::moka_policies       4/4 PASS (insert/read, hit-count weigher, byte weigher, serde)
shared::terminal_job_cache  5/5 PASS (from_json, round-trip, forget idempotent, stats, corrupted fallback)
shared::job_registry        7/7 PASS (existentes — nenhum regrediu)
```

### Integration tests (`crates/touring-hooks/tests/moka_caches_e2e.rs`)

9 testes cobrindo:

1. `query_extended_cache_hits_on_repeat_lookup` — hit no segundo call
2. `upsert_invalidates_extended_cache` — write invalidation
3. `cognitive_enrichment_write_invalidates_cache` — write propagation
4. `blake3_registry_write_invalidates_cache` — write propagation
5. `community_and_coverage_writes_invalidate_cache` — write propagation
6. `bulk_invalidation_clears_every_entry` — invalidate_all
7. `terminal_job_cache_round_trip_preserves_payload` — JSON round-trip
8. `terminal_job_cache_rejects_non_terminal_status` — guarda Running
9. `terminal_job_cache_stats_reflect_byte_sized_weight` — weigher

Todos passam em 0.05s.

### Workspace health

- `cargo check --workspace` → limpo (só warnings pre-existentes em touring-server)
- `cargo clippy -p touring-learning --lib` → zero errors (antes: 5)
- `cargo clippy -p touring-hooks --lib --tests` → zero errors no código novo
- `cargo test -p touring-hooks` → 242 unit + 9 moka E2E + 185 integration = 436 verdes

---

## Impacto esperado

| Métrica | Antes | Depois (estimado) |
|---|---|---|
| `query_extended` p50 latência (cache hit) | ~1.5ms (6 LEFT JOIN) | <10µs (Arc clone) |
| `query_extended` p50 latência (cache miss) | ~1.5ms | ~1.5ms + 1 cache insert |
| TantivyIndex busca repetida | O(log N) BM25 | O(1) moka get |
| Memória de job terminal após `drop_job` | Retida | 0 (purgado de ambos DashMap + cache) |
| Memória pico JobRegistry | Linear em total de jobs | 32 MiB cap termodinâmico |

Latências finais dependem de workload e serão medidas via `touring gate-metrics -j`
assim que houver tráfego real pós-deploy.

---

## API surface (delta)

Públicas novas:

```rust
// shared/moka_policies
pub fn build_knowledge_extended_cache<T>() -> Cache<String, Arc<T>>;
pub fn build_tantivy_query_cache<T>() -> Cache<String, Arc<Vec<T>>>;
pub fn build_terminal_job_cache<T, F>(weigher: F) -> Cache<String, Arc<T>>;
pub fn sample_stats<K, V>(cache: &Cache<K, V>) -> MokaCacheStats;
pub struct MokaCacheStats { entry_count, weighted_size }

// shared/terminal_job_cache
pub struct TerminalJobRecord { job_id, status, payload, bytes }
pub fn store_terminal(job_id: &str, value: &Value) -> bool;
pub fn lookup_terminal(job_id: &str) -> Option<Value>;
pub fn forget(job_id: &str);
pub fn stats() -> MokaCacheStats;

// FileKnowledgeDB
pub fn invalidate_extended_cache(&self, file_path: &str);
pub fn invalidate_extended_cache_all(&self);
pub fn extended_cache_stats(&self) -> MokaCacheStats;

// TantivyIndex
pub fn query_cache_counters(&self) -> (u64, u64);
pub fn query_cache_stats(&self) -> MokaCacheStats;
pub fn invalidate_query_cache(&self);

// MutableGeneratorGraph (aliases para touring-python)
pub fn len(&self) -> usize;
pub fn topological_sort(&self) -> Result<Vec<String>, GraphError>;
pub fn validate_acyclic(&self) -> Result<(), GraphError>;
pub fn get_all_levels(&self) -> Result<Vec<Vec<String>>, GraphError>;
pub fn detect_parallelizable(&self) -> Result<Vec<Vec<String>>, GraphError>;
pub fn validate_contracts(&self) -> Vec<String>;
pub fn validate_dependencies(&self) -> Vec<String>;
pub fn freeze(&self, objective_hash: Option<&str>) -> Result<GeneratorGraphModel, GraphError>;
```

---

## Próximas waves (do ranking original)

Wave 1 — "Inteligência Cognitiva":
- P0 #2 `candle-core + candle-nn` (score 15.5) — feature `semantic-embeddings` em touring-learning já pull candle; falta carregar modelo GGUF Q4_K_M
- P0 #1 `mentedb-cognitive` (score 15.4) — integrar Palace Hierarchy do Pln2

Wave 2 — "Auditoria Autônoma":
- P0 #3 `cargo-mutants` (score 10.1) — CI shard + hook post_edit
- P0 #4 `insta` (score 9.4) — snapshots AST/CallGraph/wiring

Wave 3 — parcialmente executada (rkyv IPC ativo desde 2026-04-14).

---

## Observações

- `dashmap` continua imprescindível para `JobState::Running { handle }` por causa do `JoinHandle` não-Clone. A hipótese "substituir DashMap por moka inteiramente" do doc original ignorava esse vínculo semântico — o novo desenho mantém ambos e roteia o que é cacheável.
- O hook de pre-edit reporta `complexity CC=28` em `query_extended` — a função era CC=25 antes. Acréscimo de 3 se deve às 2 branches (cache hit/miss) + 1 warming path. Aceito dentro do budget do módulo. Se gargalo, extrair `fn query_extended_fresh(&self, path)` e deixar `query_extended` como thin orchestrator.
- Post-edit hook reclama de muitos orphans (`clear_for_tests`, `stats`, `forget`): são API de observabilidade/test-harness e estão documentados no corpo do módulo — não são dead code.
