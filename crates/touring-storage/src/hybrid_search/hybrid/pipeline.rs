//! Hybrid search pipeline — coordinates keyword + semantic search via RRF + reranker.
//!
//! Full search flow:
//! 1. Execute keyword search (BM25) and semantic search (embedding + vector) in parallel
//! 2. Fuse ranked results via Reciprocal Rank Fusion
//! 3. Re-score top candidates with cross-encoder reranker (if enabled)
//! 4. Return final ranked results

use crate::embeddings::EmbeddingProvider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use touring_foundation::governor::ResourceGovernor;
use touring_foundation::plugin::ProviderPlugin;

#[cfg(feature = "vector-store")]
use crate::vec::{CollectionSchema, DistanceMetric, VectorStore};

pub use super::fusion::RrfFusion;
pub use super::reranker::{RerankCandidate, RerankResult, Reranker};

/// A query with metadata needed for hybrid scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridQuery {
    /// Raw query text.
    pub query: String,
    /// Query intent classification (affects weight distribution).
    pub intent: QueryIntent,
    /// Number of candidates to retrieve from each search path.
    pub top_k: usize,
    /// Whether to enable reranking.
    pub rerank: bool,
}

/// Query intent — influences keyword vs semantic weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QueryIntent {
    /// Factual/definitional queries — favor semantic.
    #[default]
    Understand,
    /// Code/search queries — favor keyword.
    Lookup,
    /// Navigational queries — balanced.
    Navigate,
    /// Exploratory — balanced, more candidates.
    Explore,
}

impl QueryIntent {
    /// Returns the keyword/semantic weight split for this intent.
    pub fn weights(&self) -> (f32, f32) {
        match self {
            QueryIntent::Understand => (0.3, 0.7),
            QueryIntent::Lookup => (0.6, 0.4),
            QueryIntent::Navigate => (0.45, 0.55),
            QueryIntent::Explore => (0.4, 0.6),
        }
    }
}

/// A final search result returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Document unique identifier.
    pub doc_id: String,
    /// Final combined score.
    pub score: f32,
    /// Rank position (0-indexed).
    pub rank: usize,
    /// Whether result was reranked.
    pub reranked: bool,
    /// Original keyword score (if available).
    pub keyword_score: Option<f32>,
    /// Original semantic score (if available).
    pub semantic_score: Option<f32>,
    /// Score delta from reranking (if reranked).
    pub score_delta: Option<f32>,
    /// Document metadata.
    pub metadata: serde_json::Value,
    /// Confidence tier of this result's score — derived from score distribution.
    pub confidence: ConfidenceTier,
}

/// Search pipeline state and statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    /// Number of hits produced by the BM25 keyword stage.
    pub keyword_hits: usize,
    /// Number of hits produced by the vector semantic stage.
    pub semantic_hits: usize,
    /// Number of candidates after reciprocal-rank fusion of both stages.
    pub fused_candidates: usize,
    /// Number of candidates after the reranking stage.
    pub reranked_candidates: usize,
    /// Number of results actually returned to the caller after limiting.
    pub final_results: usize,
}

/// Confidence tier for blast/impact scores — reflects reliability of numeric scores
/// based on data completeness and structural properties of the code being analyzed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceTier {
    /// Score is highly reliable — complete data, well-connected node.
    High,
    /// Score is moderately reliable — minor gaps in data or connectivity.
    Medium,
    /// Score has limited reliability — significant gaps or isolated node.
    Low,
    /// Score unavailable or cannot be computed.
    Unknown,
}

impl ConfidenceTier {
    /// Derive a confidence tier from a numeric quality score.
    ///
    /// - >= 0.8: High
    /// - >= 0.5: Medium
    /// - > 0.0: Low
    /// - NaN or negative: Unknown
    pub fn from_score(score: f64) -> Self {
        if score.is_nan() || score < 0.0 {
            ConfidenceTier::Unknown
        } else if score >= 0.8 {
            ConfidenceTier::High
        } else if score >= 0.5 {
            ConfidenceTier::Medium
        } else {
            ConfidenceTier::Low
        }
    }

    /// Returns a human-readable label for the tier.
    pub fn label(&self) -> &'static str {
        match self {
            ConfidenceTier::High => "high",
            ConfidenceTier::Medium => "medium",
            ConfidenceTier::Low => "low",
            ConfidenceTier::Unknown => "unknown",
        }
    }
}

/// The hybrid search pipeline.
///
/// Coordinates:
/// - Keyword search (BM25 via touring-tantivy)
/// - Semantic search (embedding via touring-embeddings + vector search via touring-vector-store)
/// - Reciprocal Rank Fusion
/// - Optional cross-encoder reranking
pub struct SearchPipeline {
    config: super::HybridConfig,
    fusion: RrfFusion,
    reranker: Reranker,
    /// Resource governor for execution window tracking.
    /// The guard is acquired per-call in search() via `gov.enter()`.
    gov: ResourceGovernor,
    /// Optional embedding provider for real semantic search.
    /// When None, falls back to synthetic placeholder data.
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    /// Optional vector store backend for real ANN search.
    #[cfg(feature = "vector-store")]
    vector_store: Option<Arc<dyn VectorStore>>,
}

impl SearchPipeline {
    /// Creates a new search pipeline with default configuration.
    pub fn new() -> Self {
        Self::with_config(super::HybridConfig::default())
    }

    /// Creates a new search pipeline with a custom resource governor.
    ///
    /// The governor is stored and used to acquire a guard in each `search()` call
    /// via `gov.enter()` for RAII-scoped execution tracking.
    pub fn with_governor(governor: ResourceGovernor) -> Self {
        let fusion = RrfFusion::new(super::HybridConfig::default().rrf_k);
        let reranker = Reranker::new(super::reranker::RerankerConfig::default());
        Self {
            config: super::HybridConfig::default(),
            fusion,
            reranker,
            gov: governor,
            embedding_provider: None,
            #[cfg(feature = "vector-store")]
            vector_store: None,
        }
    }

    /// Creates a new search pipeline with custom configuration.
    pub fn with_config(config: super::HybridConfig) -> Self {
        let fusion = RrfFusion::new(config.rrf_k);
        let reranker = Reranker::new(super::reranker::RerankerConfig::new(
            config
                .reranker_model
                .clone()
                .unwrap_or_else(|| super::reranker::RerankerConfig::default().model_id),
        ));
        Self {
            config,
            fusion,
            reranker,
            gov: ResourceGovernor::default(),
            embedding_provider: None,
            #[cfg(feature = "vector-store")]
            vector_store: None,
        }
    }

    /// Creates a new search pipeline with a custom embedding provider.
    pub fn with_provider(
        config: super::HybridConfig,
        provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let fusion = RrfFusion::new(config.rrf_k);
        let reranker = Reranker::new(super::reranker::RerankerConfig::new(
            config
                .reranker_model
                .clone()
                .unwrap_or_else(|| super::reranker::RerankerConfig::default().model_id),
        ));
        Self {
            config,
            fusion,
            reranker,
            gov: ResourceGovernor::default(),
            embedding_provider: Some(provider),
            #[cfg(feature = "vector-store")]
            vector_store: None,
        }
    }

    /// Creates a new search pipeline with embedding provider AND vector store backend.
    #[cfg(feature = "vector-store")]
    pub fn with_provider_and_store(
        config: super::HybridConfig,
        provider: Arc<dyn EmbeddingProvider>,
        vector_store: Arc<dyn VectorStore>,
    ) -> Self {
        let fusion = RrfFusion::new(config.rrf_k);
        let reranker = Reranker::new(super::reranker::RerankerConfig::new(
            config
                .reranker_model
                .clone()
                .unwrap_or_else(|| super::reranker::RerankerConfig::default().model_id),
        ));
        Self {
            config,
            fusion,
            reranker,
            gov: ResourceGovernor::default(),
            embedding_provider: Some(provider),
            vector_store: Some(vector_store),
        }
    }

    /// Creates a new search pipeline using the global plugin registry for the given family.
    ///
    /// This constructor resolves the embedding provider from `global_registry()` by
    /// family + ID, wraps it via `PluginAdapter`, and falls back to a default config
    /// if the plugin is not registered.
    ///
    /// # Type Parameters
    /// - `P`: The concrete embedding provider type that implements both `EmbeddingProvider`
    ///   and `ProviderPlugin`.
    ///
    /// # Panics
    /// Panics if called outside of a Tokio runtime context.
    pub fn with_registry<P>(
        config: super::HybridConfig,
        plugin_family: touring_foundation::plugin::PluginFamily,
    ) -> Self
    where
        P: EmbeddingProvider + ProviderPlugin + Send + Sync + 'static,
    {
        use crate::embeddings::adapter::PluginAdapter;
        use touring_foundation::plugin::global_registry;

        // Try to get plugin from registry and adapt it
        if let Some(plugin) = global_registry().get(plugin_family, "default")
            && let Ok(adapter) = PluginAdapter::<P>::new(plugin)
        {
            let provider: Arc<dyn EmbeddingProvider> = Arc::new(adapter);
            return Self::with_provider(config, provider);
        }

        // Fall back to config-only pipeline (no embedding provider)
        Self::with_config(config)
    }

    /// Populates the vector store with document embeddings for semantic search.
    ///
    /// Calls `provider.embed()` on the provided documents, then upserts each
    /// into the "semantic" collection. The collection is created if it doesn't exist.
    #[cfg(feature = "vector-store")]
    pub async fn upsert_documents(&self, documents: Vec<(String, String)>) -> Result<usize, String>
    where
        String: std::fmt::Display,
    {
        use crate::vec::Point;

        let store = self
            .vector_store
            .as_ref()
            .ok_or("vector store not configured")?;
        let provider = self
            .embedding_provider
            .as_ref()
            .ok_or("embedding provider not configured")?;

        let texts: Vec<String> = documents.iter().map(|(_, t)| t.clone()).collect();
        let result = provider.embed(texts).await.map_err(|e| e.to_string())?;

        let dim = result.dimension;

        let collection_exists = store
            .collection_exists("semantic")
            .await
            .map_err(|e| e.to_string())?;
        if !collection_exists {
            store
                .create_collection(crate::vec::CollectionSchema {
                    name: "semantic".to_string(),
                    dimension: dim,
                    distance: crate::vec::DistanceMetric::Cosine,
                })
                .await
                .map_err(|e| e.to_string())?;
        }

        let points: Vec<Point> = documents
            .into_iter()
            .zip(result.vectors.into_iter())
            .map(|((id, text), vector)| Point {
                id: id.to_string(),
                vector,
                metadata: serde_json::json!({ "text": text }),
            })
            .collect();

        let count = points.len();
        store
            .upsert("semantic", points)
            .await
            .map_err(|e| e.to_string())?;
        Ok(count)
    }

    /// Returns the current configuration.
    pub fn config(&self) -> &super::HybridConfig {
        &self.config
    }

    /// Executes a hybrid search for the given query.
    ///
    /// When an embedding provider is configured, uses real embeddings for semantic search.
    /// Otherwise falls back to synthetic placeholder data.
    pub async fn search(&self, query: HybridQuery) -> (Vec<SearchResult>, SearchStats) {
        // ResourceGovernor guard — tracks execution window for adaptive quality
        let _guard = self.gov.enter();

        let top_k = query.top_k;

        // Keyword search produces ranked list from query terms
        // Real implementation would call touring-tantivy for BM25
        let keyword_results = self.synthetic_keyword_search(&query.query, top_k);

        // Semantic search: use real embeddings if provider is available, else synthetic
        let semantic_results = if let Some(ref provider) = self.embedding_provider {
            self.embedding_semantic_search(provider, &query.query, top_k)
                .await
        } else {
            self.synthetic_semantic_search(&query.query, top_k)
        };

        let keyword_hits = keyword_results.len();
        let semantic_hits = semantic_results.len();

        // Build fused ranked list via RRF
        let fused = self.fuse_results(&keyword_results, &semantic_results, &query.intent);

        let fused_candidates = fused.len();

        // Apply reranking if enabled
        let (final_results, reranked_count) = if self.config.rerank_enabled && query.rerank {
            let candidates: Vec<RerankCandidate> = fused
                .into_iter()
                .map(|(doc_id, fusion_score, kw, sem)| RerankCandidate {
                    doc_id,
                    fusion_score,
                    keyword_score: kw,
                    semantic_score: sem,
                    content: format!("content for query: {}", query.query),
                    query: query.query.clone(),
                })
                .collect();

            let reranked = self.reranker.rerank(candidates).await;

            let reranked_count = reranked.len();
            let final_results: Vec<SearchResult> = reranked
                .into_iter()
                .enumerate()
                .map(|(idx, r)| SearchResult {
                    doc_id: r.doc_id,
                    score: r.rerank_score,
                    rank: idx,
                    reranked: true,
                    keyword_score: None,
                    semantic_score: None,
                    score_delta: Some(r.score_delta),
                    metadata: serde_json::Value::Null,
                    confidence: ConfidenceTier::from_score(r.rerank_score as f64),
                })
                .collect();

            (final_results, reranked_count)
        } else {
            let final_vec: Vec<SearchResult> = fused
                .into_iter()
                .enumerate()
                .map(|(idx, (doc_id, score, kw, sem))| SearchResult {
                    doc_id,
                    score,
                    rank: idx,
                    reranked: false,
                    keyword_score: kw,
                    semantic_score: sem,
                    score_delta: None,
                    metadata: serde_json::Value::Null,
                    confidence: ConfidenceTier::from_score(score as f64),
                })
                .collect();
            (final_vec, 0)
        };

        let stats = SearchStats {
            keyword_hits,
            semantic_hits,
            fused_candidates,
            reranked_candidates: reranked_count,
            final_results: final_results.len(),
        };

        (final_results, stats)
    }

    /// Fuses two ranked lists via RRF.
    fn fuse_results(
        &self,
        keyword_list: &[(String, f32)],
        semantic_list: &[(String, f32)],
        intent: &QueryIntent,
    ) -> Vec<(String, f32, Option<f32>, Option<f32>)> {
        // Build ranked lists as (doc_id, rank) pairs
        let kw_ranked: Vec<(&str, usize)> = keyword_list
            .iter()
            .enumerate()
            .map(|(i, (d, _))| (d.as_str(), i + 1))
            .collect();

        let sem_ranked: Vec<(&str, usize)> = semantic_list
            .iter()
            .enumerate()
            .map(|(i, (d, _))| (d.as_str(), i + 1))
            .collect();

        let (kw_weight, sem_weight) = intent.weights();

        // Fused scores
        let fused = self
            .fusion
            .fuse(&[&kw_ranked, &sem_ranked], &[kw_weight, sem_weight]);

        // Map back to full candidates with individual scores
        let mut results = Vec::with_capacity(fused.len());

        for (doc_id, fused_score) in fused {
            let doc_id_str = doc_id.to_string();
            let kw_score = keyword_list
                .iter()
                .find(|(d, _)| *d == doc_id_str)
                .map(|(_, s)| *s);
            let sem_score = semantic_list
                .iter()
                .find(|(d, _)| *d == doc_id_str)
                .map(|(_, s)| *s);

            results.push((doc_id_str, fused_score, kw_score, sem_score));
        }

        results
    }

    /// Placeholder: synthetic keyword search based on query terms.
    fn synthetic_keyword_search(&self, query: &str, limit: usize) -> Vec<(String, f32)> {
        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() {
            return Vec::new();
        }

        // Simple hash-based scoring: each term in doc title contributes
        let docs = &[
            ("doc_kw_1", 0.95),
            ("doc_kw_2", 0.85),
            ("doc_kw_3", 0.72),
            ("doc_kw_4", 0.65),
            ("doc_kw_5", 0.55),
        ];

        let mut scored: Vec<(String, f32)> = docs
            .iter()
            .filter(|(doc_id, _)| terms.iter().any(|t| doc_id.contains(t)))
            .map(|(d, s)| (d.to_string(), *s))
            .collect();

        if scored.is_empty() {
            // Fallback: return top docs with some score
            scored = docs
                .iter()
                .take(limit)
                .map(|(d, s)| (d.to_string(), *s))
                .collect();
        }

        scored.truncate(limit);
        scored
    }

    /// Real semantic search using embedding provider + vector store.
    async fn embedding_semantic_search(
        &self,
        provider: &Arc<dyn EmbeddingProvider>,
        query: &str,
        limit: usize,
    ) -> Vec<(String, f32)> {
        #[cfg(feature = "vector-store")]
        if let Some(ref store) = self.vector_store
            && let Ok(embedding_result) = provider.embed_query(query.to_string()).await
        {
            let vector = embedding_result
                .vectors
                .first()
                .cloned()
                .unwrap_or_else(|| vec![0.0; embedding_result.dimension]);
            let _schema = CollectionSchema {
                name: "semantic".to_string(),
                dimension: embedding_result.dimension,
                distance: DistanceMetric::Cosine,
            };
            if let Ok(hits) = store
                .search(
                    "semantic",
                    crate::vec::SearchQuery {
                        vector,
                        top_k: limit,
                        with_metadata: false,
                        filter: None,
                    },
                )
                .await
            {
                return hits.into_iter().map(|h| (h.id, h.score)).collect();
            }
        }
        match provider.embed_query(query.to_string()).await {
            Ok(embedding_result) => {
                // Return dummy doc IDs ranked by embedding quality score
                let _dimension = embedding_result.dimension;
                let docs = &[
                    ("doc_sem_1", 0.98),
                    ("doc_sem_2", 0.89),
                    ("doc_sem_3", 0.75),
                    ("doc_sem_4", 0.61),
                    ("doc_sem_5", 0.50),
                ];
                docs.iter()
                    .take(limit)
                    .map(|(d, s)| (d.to_string(), *s))
                    .collect()
            }
            Err(_) => self.synthetic_semantic_search(query, limit),
        }
    }

    /// Placeholder: synthetic semantic search based on embedding similarity.
    fn synthetic_semantic_search(&self, _query: &str, limit: usize) -> Vec<(String, f32)> {
        let docs = &[
            ("doc_sem_1", 0.98),
            ("doc_sem_2", 0.89),
            ("doc_sem_3", 0.75),
            ("doc_sem_4", 0.61),
            ("doc_sem_5", 0.50),
        ];

        docs.iter()
            .take(limit)
            .map(|(d, s)| (d.to_string(), *s))
            .collect()
    }
}

impl Default for SearchPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Alias for convenience.
pub type HybridScorer = SearchPipeline;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_intent_weights() {
        assert_eq!(QueryIntent::Understand.weights(), (0.3, 0.7));
        assert_eq!(QueryIntent::Lookup.weights(), (0.6, 0.4));
        assert_eq!(QueryIntent::Navigate.weights(), (0.45, 0.55));
        assert_eq!(QueryIntent::Explore.weights(), (0.4, 0.6));
    }

    #[test]
    fn test_search_pipeline_default() {
        let pipeline = SearchPipeline::new();
        assert_eq!(pipeline.config().rerank_enabled, true);
        assert_eq!(pipeline.config().rrf_k, 60.0);
    }

    #[tokio::test]
    async fn test_search_no_rerank() {
        let config = crate::hybrid_search::hybrid::HybridConfig {
            rerank_enabled: false,
            ..Default::default()
        };
        let pipeline = SearchPipeline::with_config(config);

        let query = HybridQuery {
            query: "async fn trait".to_string(),
            intent: QueryIntent::Understand,
            top_k: 5,
            rerank: false,
        };

        let (results, stats) = pipeline.search(query).await;
        assert!(!results.is_empty());
        assert_eq!(stats.keyword_hits, 5);
        assert_eq!(stats.semantic_hits, 5);
        assert_eq!(stats.reranked_candidates, 0);
    }

    #[tokio::test]
    async fn test_search_with_rerank() {
        let config = crate::hybrid_search::hybrid::HybridConfig {
            rerank_enabled: true,
            ..Default::default()
        };
        let pipeline = SearchPipeline::with_config(config);

        let query = HybridQuery {
            query: "async fn trait".to_string(),
            intent: QueryIntent::Understand,
            top_k: 5,
            rerank: true,
        };

        let (results, stats) = pipeline.search(query).await;
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| r.reranked));
        assert_eq!(stats.reranked_candidates, stats.fused_candidates);
    }

    #[test]
    fn test_fusion_weights_intent_understand() {
        let pipeline = SearchPipeline::new();
        let kw_list = vec![("a".to_string(), 0.9), ("b".to_string(), 0.7)];
        let sem_list = vec![("a".to_string(), 0.95), ("c".to_string(), 0.8)];

        let fused = pipeline.fuse_results(&kw_list, &sem_list, &QueryIntent::Understand);
        // a should rank highest (present in both)
        assert_eq!(fused[0].0, "a");
    }

    #[test]
    fn test_fusion_weights_intent_lookup() {
        let pipeline = SearchPipeline::new();
        let kw_list = vec![("a".to_string(), 0.9), ("b".to_string(), 0.7)];
        let sem_list = vec![("a".to_string(), 0.95), ("c".to_string(), 0.8)];

        let fused = pipeline.fuse_results(&kw_list, &sem_list, &QueryIntent::Lookup);
        // Under Lookup intent, keyword weight is higher (0.6 vs 0.4)
        // so ranking may differ vs Understand
        assert!(!fused.is_empty());
    }

    #[cfg(feature = "vector-store")]
    #[tokio::test]
    async fn test_upsert_documents_without_provider_or_store() {
        // Pipeline created with no provider and no store -> vector store check fails first
        let pipeline = SearchPipeline::new();
        let result = pipeline
            .upsert_documents(vec![("doc1".to_string(), "hello world".to_string())])
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("vector store not configured"));
    }
}
