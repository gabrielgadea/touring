---
name: graph-viz-wave-5
description: Wave 5 (Hybrid Semantic Search) — Deliverables D22, D23, D24, D25
type: project
related_files:
  - graph-viz-master-plan_OVERVIEW.md
  - graph-viz-master-plan_STATUS.md
  - graph-viz-master-plan_WAVES_1_2.md
  - graph-viz-master-plan_WAVE_3.md
  - graph-viz-master-plan_WAVE_4.md
  - graph-viz-master-plan_WAVE_5.md
  - graph-viz-master-plan_WAVE_6.md
  - graph-viz-master-plan_WAVE_7.md
  - graph-viz-master-plan_WAVE_8.md
  - graph-viz-master-plan_DEPENDENCIES.md
---

# Wave 5 — Hybrid Semantic Search (STRATEGIC INVESTMENT)

**Target**: v31.0.0 | **Data**: 2026-05-02

> **CRITICAL**: Maior gap competitivo. Touring NÃO tem dense embeddings. Sem isto, queries naturais como "where do we handle retries?" não funcionam adequadamente. Voyage Code-3 paper: +14.52% precision com hybrid vs dense-only.

---

## WAVE 5.1 — D22 Embedding Provider Abstraction 🟡 PARCIAL (40%)

**Implementado**:
- `touring-embeddings/src/` crate existe
- `providers/candle_bge.rs`, `providers/fastembed.rs`, `providers/voyage.rs`

**Falta**:
- [ ] `EmbeddingProvider` trait completo:
  ```rust
  #[async_trait]
  pub trait EmbeddingProvider: Send + Sync {
      fn id(&self) -> &str;
      fn family(&self) -> ModelFamily;
      fn dimensions(&self) -> usize;
      async fn embed(&self, texts: &[String]) -> Result<Vec<DenseVector>>;
      async fn embed_query(&self, query: &str) -> Result<DenseVector>;
  }
  ```
- [ ] `ModelFamily` struct com name + generation
- [ ] Sparse provider trait (BM25 wrap)
- [ ] 3 backends funcionais:
  - Candle BGE-small (puro Rust, ~130MB, <50ms CPU)
  - FastEmbed (tokio-process Python)
  - Voyage AI (HTTP client, API key)

**Testes**: 27 (9 per provider)

---

## WAVE 5.2 — D23 Vector Store Abstraction 🟡 PARCIAL (10%)

**Falta**:
- [ ] `VectorStore` trait:
  ```rust
  #[async_trait]
  pub trait VectorStore: Send + Sync {
      async fn upsert(&self, points: Vec<Point>) -> Result<UpsertResult>;
      async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
      async fn delete(&self, ids: &[PointId]) -> Result<()>;
      async fn collection_info(&self, name: &str) -> Result<CollectionInfo>;
      async fn create_collection(&self, name: &str, schema: &CollectionSchema) -> Result<()>;
  }
  ```
- [ ] Backend 1: **sqlite-vec** (puro local, zero deps externas)
- [ ] Backend 2: **Qdrant** (HTTP client, feature flag)
- [ ] Backend 3: **InMemory** (tests + airgapped fallback)
- [ ] Hybrid query: `SearchQuery { dense: Option<Vec<f32>>, sparse: Option<SparseVec>, weights: HybridWeights }`

**Testes**: 30 (24 unit + 6 integration)

---

## WAVE 5.3 — D24 Hybrid Scoring + RRF + Reranking 🟡 PARCIAL (70%)

**Implementado**:
- `touring-search-fusion/src/hybrid/pipeline.rs` (13.420 LOC)
- `touring-search-fusion/src/hybrid/fusion.rs` (5.717 LOC)
- `touring-search-fusion/src/hybrid/reranker.rs` (8.566 LOC)

**Falta**:
- [ ] `HybridWeights` struct: `{dense: 0.65, sparse: 0.35}`
- [ ] RRF fusion (k=60) combinando 3 backends
- [ ] Reranker cascade (primary fail → fallback → no-op)
- [ ] Reranker MVP: trivial sort by score
- [ ] Reranker advanced (gated): cross-encoder via Candle, Cohere, Voyage
- [ ] Integration com D13 (intent boost applied AFTER hybrid+RRF+rerank)

**Testes**: 16 unit

---

## WAVE 5.4 — D25 Asymmetric Embeddings + Manifest 🔴 PENDENTE (0%)

**Dependencies**: D22, D23, D18 (checkpoint fingerprint)

**Falta**:
- [ ] `FileManifestEntry` extension:
  ```rust
  pub dense_embedding_provider: Option<String>,
  pub dense_embedding_model: Option<String>,
  pub sparse_embedding_provider: Option<String>,
  pub sparse_embedding_model: Option<String>,
  ```
- [ ] `get_files_needing_embeddings()`:
  - Dense missing → embed dense
  - Sparse missing → embed sparse
  - Family changed → reembed both
  - Family same + model different → keep doc embeddings (asymmetric ok)
- [ ] CLI `touring session start` mostra count `dense_only=X, sparse_only=Y`

**Testes**: 14 unit

---

## VALIDAÇÃO GATE WAVE 5

```bash
# embedding provider
touring embeddings test-provider candle bge-small "fn foo() {}" -j | jq '.dimensions'  # → 384

# vector store
touring vector-store status -j | jq '.backend'  # → "sqlite-vec"
touring index --rebuild-with-embeddings
touring search semantic "authentication flow" -j | jq '.results | length'  # ≥ 1

# hybrid scoring
touring search unified "where do we validate JWT" --hybrid -j | jq '.strategy_used'  # → "hybrid"
touring search unified "where do we validate JWT" --hybrid -j | jq '.results[0].score_breakdown'

# asymmetric embeddings
touring config set embeddings.dense.model bge-base-v1.5
touring session start asym-test type "test"  # should NOT reembed if family matches
```

---

## CRITICAL PATH (8 hops)

```
D22 (Embedding) ──► D23 (Vector Store) ──► D24 (Hybrid Scoring) ──► D25 (Manifest) ──► D26 (find_code)
                    ↑
                    D18 (fingerprint) ──► D25
```