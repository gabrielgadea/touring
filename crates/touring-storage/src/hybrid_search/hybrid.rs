//! Hybrid search scoring — combines keyword + semantic rankings via RRF.
//!
//! D24 delivers the hybrid scoring layer that extends touring-search-fusion.
//! Architecture:
//! - `HybridScorer` — coordinates keyword (BM25) + semantic (embedding) search
//! - `RrfFusion` — Reciprocal Rank Fusion for combining ranked results
//! - `Reranker` — cross-encoder style reranking of fused candidates
//!
//! # Example
//!
//! ```
//! use touring_storage::hybrid_search::hybrid::{HybridScorer, HybridQuery, pipeline::QueryIntent as HybridIntent};
//!
//! let pipeline = HybridScorer::new();
//! let query = HybridQuery {
//!     query: "async fn trait".to_string(),
//!     intent: HybridIntent::Understand,
//!     top_k: 10,
//!     rerank: false,
//! };
//! // NOTE: actual search requires runtime — use pipeline.search(query).await in async context
//! ```

use serde::{Deserialize, Serialize};

pub mod fusion;
pub mod pipeline;
pub mod reranker;

pub use fusion::RrfFusion;
pub use pipeline::{HybridQuery, HybridScorer, SearchPipeline, SearchResult};
pub use reranker::Reranker;

/// Configuration for hybrid search fusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridConfig {
    /// Weight for keyword (BM25) scores in fusion. Range [0.0, 1.0].
    pub keyword_weight: f32,
    /// Weight for semantic (embedding) scores in fusion. Range [0.0, 1.0].
    pub semantic_weight: f32,
    /// RRF constant — higher values give more weight to rank differences.
    pub rrf_k: f32,
    /// Number of candidates to retrieve from each search path.
    pub candidates_per_path: usize,
    /// Number of final results to return after reranking.
    pub final_results: usize,
    /// Whether to enable reranking step.
    pub rerank_enabled: bool,
    /// Reranker model identifier (for cross-encoder).
    pub reranker_model: Option<String>,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            keyword_weight: 0.4,
            semantic_weight: 0.6,
            rrf_k: 60.0,
            candidates_per_path: 100,
            final_results: 10,
            rerank_enabled: true,
            reranker_model: None,
        }
    }
}

/// A ranked search result with combined score and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedResult {
    /// Unique document identifier.
    pub doc_id: String,
    /// Combined fusion score (RRF-weighted).
    pub score: f32,
    /// Rank position after fusion.
    pub rank: usize,
    /// Keyword match score (BM25).
    pub keyword_score: Option<f32>,
    /// Semantic similarity score.
    pub semantic_score: Option<f32>,
    /// Query intent used for scoring.
    pub intent: String,
    /// Whether result passed reranker boosting.
    pub reranked: bool,
    /// Document metadata (title, snippet, URL, etc.).
    pub metadata: serde_json::Value,
}

impl RankedResult {
    /// Creates a new ranked result.
    pub fn new(doc_id: String, score: f32, rank: usize, metadata: serde_json::Value) -> Self {
        Self {
            doc_id,
            score,
            rank,
            keyword_score: None,
            semantic_score: None,
            intent: String::new(),
            reranked: false,
            metadata,
        }
    }

    /// Returns the document identifier.
    pub fn id(&self) -> &str {
        &self.doc_id
    }

    /// Returns true if this result has keyword score.
    pub fn has_keyword_score(&self) -> bool {
        self.keyword_score.is_some()
    }

    /// Returns true if this result has semantic score.
    pub fn has_semantic_score(&self) -> bool {
        self.semantic_score.is_some()
    }
}

/// Debug/info representation of fusion state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionDebug {
    /// Number of candidates returned by the BM25 keyword stage.
    pub keyword_results: usize,
    /// Number of candidates returned by the vector semantic stage.
    pub semantic_results: usize,
    /// Number of candidates remaining after reciprocal-rank fusion.
    pub fused_results: usize,
    /// Number of candidates remaining after the reranking stage.
    pub reranked_results: usize,
    /// Minimum and maximum fused scores, as a `(min, max)` pair.
    pub fusion_score_range: (f32, f32),
    /// Display name of the query intent applied to weight the fusion.
    pub intent_applied: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_config_defaults() {
        let cfg = HybridConfig::default();
        assert!((cfg.keyword_weight - 0.4).abs() < 1e-6);
        assert!((cfg.semantic_weight - 0.6).abs() < 1e-6);
        assert!((cfg.rrf_k - 60.0).abs() < 1e-6);
        assert_eq!(cfg.candidates_per_path, 100);
        assert_eq!(cfg.final_results, 10);
        assert!(cfg.rerank_enabled);
    }

    #[test]
    fn test_ranked_result_new() {
        let meta = serde_json::json!({"title": "Test", "url": "https://example.com"});
        let result = RankedResult::new("doc1".to_string(), 0.85, 0, meta.clone());
        assert_eq!(result.doc_id, "doc1");
        assert!((result.score - 0.85).abs() < 1e-6);
        assert_eq!(result.rank, 0);
        assert!(!result.has_keyword_score());
        assert!(!result.has_semantic_score());
        assert!(!result.reranked);
    }

    #[test]
    fn test_ranked_result_scores() {
        let mut result = RankedResult::new("doc1".to_string(), 0.9, 1, serde_json::Value::Null);
        result.keyword_score = Some(0.7);
        result.semantic_score = Some(0.95);
        assert!(result.has_keyword_score());
        assert!(result.has_semantic_score());
    }

    #[test]
    fn test_rrf_fusion_ordering() {
        // Test that higher rank positions (worse) produce lower scores
        let fusion = RrfFusion::new(60.0);
        let doc_a = fusion.rrf_score(1); // rank 1
        let doc_b = fusion.rrf_score(2); // rank 2
        let doc_c = fusion.rrf_score(5); // rank 5
        // Lower rank number = higher score
        assert!(doc_a > doc_b);
        assert!(doc_b > doc_c);
    }

    #[test]
    fn test_ranked_result_id() {
        let result = RankedResult::new("test-doc-123".to_string(), 0.5, 0, serde_json::Value::Null);
        assert_eq!(result.id(), "test-doc-123");
    }
}
