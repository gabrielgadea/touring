//! Cross-encoder reranker for hybrid search results.
//!
//! Reranking applies a lightweight cross-encoder model to re-score and reorder
//! the fused candidates for improved relevance.

use serde::{Deserialize, Serialize};

/// Configuration for the reranker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerConfig {
    /// Model identifier (e.g., "cross-encoder/ms-marco-MiniLM-L-6-v2").
    pub model_id: String,
    /// Maximum batch size for reranking.
    pub batch_size: usize,
    /// Score threshold — results below this are dropped.
    pub score_threshold: Option<f32>,
}

impl RerankerConfig {
    /// Creates a default reranker config with the given model ID.
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            batch_size: 32,
            score_threshold: None,
        }
    }

    /// Sets the batch size.
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Sets the score threshold.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.score_threshold = Some(threshold);
        self
    }
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self::new("cross-encoder/ms-marco-MiniLM-L-6-v2")
    }
}

/// A candidate document awaiting reranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankCandidate {
    /// Document unique identifier.
    pub doc_id: String,
    /// Original fusion score (RRF-weighted).
    pub fusion_score: f32,
    /// Keyword BM25 score (if available).
    pub keyword_score: Option<f32>,
    /// Semantic embedding score (if available).
    pub semantic_score: Option<f32>,
    /// Document text content for cross-encoder scoring.
    pub content: String,
    /// Query text for cross-encoder scoring.
    pub query: String,
}

/// A reranked result with new score and original metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankResult {
    /// Document identifier.
    pub doc_id: String,
    /// New cross-encoder score.
    pub rerank_score: f32,
    /// Original fusion score (preserved for comparison).
    pub fusion_score: f32,
    /// Score improvement (rerank - fusion).
    pub score_delta: f32,
}

impl RerankResult {
    /// Creates a new rerank result.
    pub fn new(doc_id: String, rerank_score: f32, fusion_score: f32) -> Self {
        let score_delta = rerank_score - fusion_score;
        Self {
            doc_id,
            rerank_score,
            fusion_score,
            score_delta,
        }
    }
}

/// Cross-encoder reranker — re-scores fused candidates using a cross-encoder model.
#[derive(Clone, Debug)]
pub struct Reranker {
    config: RerankerConfig,
}

impl Reranker {
    /// Creates a new reranker with the given config.
    pub fn new(config: RerankerConfig) -> Self {
        Self { config }
    }

    /// Default reranker with standard model.
    pub fn default_reranker() -> Self {
        Self::new(RerankerConfig::default())
    }

    /// Reranks a list of candidates, returning results sorted by rerank score.
    ///
    /// In this placeholder implementation, we use a simple heuristic:
    /// `fusion_score * (1.0 + intent_boost)` where intent_boost favors
    /// semantic_score over keyword_score based on query intent classification.
    ///
    /// A real implementation would load and run a cross-encoder model here.
    pub async fn rerank(&self, candidates: Vec<RerankCandidate>) -> Vec<RerankResult> {
        let mut results: Vec<RerankResult> = Vec::with_capacity(candidates.len());

        for candidate in candidates {
            // Placeholder cross-encoder: weighted combination of available signals
            // Real implementation would call: cross_encoder.score([query, content])
            let base_score = candidate.fusion_score;

            // Boost semantic over keyword when both are available
            let semantic_boost = candidate.semantic_score.unwrap_or(0.0) * 0.3;
            let keyword_boost = candidate.keyword_score.unwrap_or(0.0) * 0.1;

            let rerank_score = base_score + semantic_boost + keyword_boost;

            // Apply threshold filter
            if let Some(threshold) = self.config.score_threshold
                && rerank_score < threshold
            {
                continue;
            }

            results.push(RerankResult::new(
                candidate.doc_id,
                rerank_score,
                candidate.fusion_score,
            ));
        }

        // Sort by rerank score descending
        results.sort_by(|a, b| {
            b.rerank_score
                .partial_cmp(&a.rerank_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }

    /// Returns the reranker configuration.
    pub fn config(&self) -> &RerankerConfig {
        &self.config
    }
}

impl Default for Reranker {
    fn default() -> Self {
        Self::default_reranker()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reranker_config_new() {
        let config = RerankerConfig::new("my-model");
        assert_eq!(config.model_id, "my-model");
        assert_eq!(config.batch_size, 32);
        assert!(config.score_threshold.is_none());
    }

    #[test]
    fn test_reranker_config_with_batch_size() {
        let config = RerankerConfig::new("test").with_batch_size(64);
        assert_eq!(config.batch_size, 64);
    }

    #[test]
    fn test_reranker_config_with_threshold() {
        let config = RerankerConfig::new("test").with_threshold(0.5);
        assert_eq!(config.score_threshold, Some(0.5));
    }

    #[test]
    fn test_rerank_result_new() {
        let result = RerankResult::new("doc1".to_string(), 0.9, 0.7);
        assert_eq!(result.doc_id, "doc1");
        assert!((result.rerank_score - 0.9).abs() < 1e-6);
        assert!((result.fusion_score - 0.7).abs() < 1e-6);
        assert!((result.score_delta - 0.2).abs() < 1e-6);
    }

    #[test]
    fn test_reranker_default() {
        let reranker = Reranker::default_reranker();
        assert_eq!(
            reranker.config().model_id,
            "cross-encoder/ms-marco-MiniLM-L-6-v2"
        );
    }

    #[tokio::test]
    async fn test_reranker_rerank_no_candidates() {
        let reranker = Reranker::default_reranker();
        let results = reranker.rerank(vec![]).await;
        assert!(results.is_empty());
    }

    #[test]
    fn test_reranker_rerank_with_threshold_filters() {
        let config = RerankerConfig::new("test").with_threshold(0.8);
        let reranker = Reranker::new(config);
        let candidates = vec![
            RerankCandidate {
                doc_id: "low".to_string(),
                fusion_score: 0.5,
                keyword_score: Some(0.4),
                semantic_score: Some(0.3),
                content: "content".to_string(),
                query: "query".to_string(),
            },
            RerankCandidate {
                doc_id: "high".to_string(),
                fusion_score: 0.9,
                keyword_score: Some(0.8),
                semantic_score: Some(0.9),
                content: "content".to_string(),
                query: "query".to_string(),
            },
        ];
        let rt = tokio::runtime::Runtime::new().unwrap();
        let results = rt.block_on(reranker.rerank(candidates));
        // "low" should be filtered out by threshold 0.8
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "high");
    }

    #[test]
    fn test_reranker_rerank_sorts_by_rerank_score() {
        let reranker = Reranker::default_reranker();
        let candidates = vec![
            RerankCandidate {
                doc_id: "doc_a".to_string(),
                fusion_score: 0.3,
                keyword_score: Some(0.2),
                semantic_score: Some(0.1),
                content: "content a".to_string(),
                query: "query".to_string(),
            },
            RerankCandidate {
                doc_id: "doc_b".to_string(),
                fusion_score: 0.8,
                keyword_score: Some(0.9),
                semantic_score: Some(0.7),
                content: "content b".to_string(),
                query: "query".to_string(),
            },
        ];
        let rt = tokio::runtime::Runtime::new().unwrap();
        let results = rt.block_on(reranker.rerank(candidates));
        // Higher fusion_score + boosts = higher rerank_score → doc_b first
        assert_eq!(results[0].doc_id, "doc_b");
        assert_eq!(results[1].doc_id, "doc_a");
    }
}
