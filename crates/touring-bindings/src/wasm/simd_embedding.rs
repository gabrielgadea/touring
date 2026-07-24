#![allow(clippy::indexing_slicing)]

//! SIMD-accelerated embedding search for WASM plugins.
//!
//! Provides high-performance similarity search capabilities to WASM plugins
//! without requiring them to implement their own vector operations.
//!
//! # Integration with touring-simd
//!
//! This module bridges WASM plugin evaluation with SIMD-accelerated similarity
//! search from `touring_simd::TopKSearcher`. Plugins can perform embedding-based
//! retrieval without implementing vector math themselves.
//!
//! # Example
//!
//! ```
//! use touring_bindings::wasm::simd_embedding::{EmbeddingSearch, EmbeddingDoc};
//!
//! let mut search = EmbeddingSearch::new(3); // top-3 results
//! search.add("doc1", vec![1.0, 0.0, 0.0]);
//! search.add("doc2", vec![0.0, 1.0, 0.0]);
//! search.add("doc3", vec![0.9, 0.1, 0.0]);
//!
//! let results = search.search(&[1.0, 0.0, 0.0], 2);
//! assert_eq!(results[0].id, "doc1");
//! assert!((results[0].score - 1.0).abs() < 1e-6);
//! ```

use touring_simd::{TopKResult, TopKSearcher};

/// A document with its embedding vector.
#[derive(Debug, Clone)]
pub struct EmbeddingDoc {
    /// Unique document identifier.
    pub id: String,
    /// Embedding vector.
    pub embedding: Vec<f32>,
    /// Optional document text or metadata.
    pub text: Option<String>,
}

impl EmbeddingDoc {
    /// Create a new embedding document.
    pub fn new(id: impl Into<String>, embedding: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            embedding,
            text: None,
        }
    }

    /// Add optional text content.
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

/// Error returned by [`EmbeddingSearch::add`] / [`EmbeddingSearch::add_with_text`]
/// (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    /// The embedding's dimension does not match the index's existing dimension.
    #[error("embedding dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch {
        /// Dimension already established by previously-added documents.
        expected: usize,
        /// Dimension of the embedding being added.
        got: usize,
    },
}

/// SIMD-accelerated embedding search index for WASM plugins.
///
/// Wraps `touring_simd::TopKSearcher` to provide high-performance
/// nearest-neighbor search with configurable top-K retrieval.
#[derive(Debug)]
pub struct EmbeddingSearch {
    searcher: TopKSearcher,
    docs: Vec<EmbeddingDoc>,
    embedding_dim: Option<usize>,
    default_k: usize,
}

impl Default for EmbeddingSearch {
    fn default() -> Self {
        Self::new(64)
    }
}

impl EmbeddingSearch {
    /// Create a new embedding search index.
    ///
    /// The `default_k` parameter sets the default number of results to return
    /// when `search` is called without an explicit k value.
    pub fn new(default_k: usize) -> Self {
        Self {
            searcher: TopKSearcher::new(default_k),
            docs: Vec::new(),
            embedding_dim: None,
            default_k,
        }
    }

    /// Add a document to the search index.
    ///
    /// Returns an error if the embedding dimension doesn't match existing docs.
    pub fn add(
        &mut self,
        id: impl Into<String>,
        embedding: Vec<f32>,
    ) -> Result<(), EmbeddingError> {
        let dim = embedding.len();

        if let Some(existing) = self.embedding_dim {
            if existing != dim {
                return Err(EmbeddingError::DimensionMismatch {
                    expected: existing,
                    got: dim,
                });
            }
        } else {
            self.embedding_dim = Some(dim);
        }

        self.docs.push(EmbeddingDoc::new(id, embedding));
        Ok(())
    }

    /// Add a document with text content.
    pub fn add_with_text(
        &mut self,
        id: impl Into<String>,
        embedding: Vec<f32>,
        text: impl Into<String>,
    ) -> Result<(), EmbeddingError> {
        let dim = embedding.len();

        if let Some(existing) = self.embedding_dim {
            if existing != dim {
                return Err(EmbeddingError::DimensionMismatch {
                    expected: existing,
                    got: dim,
                });
            }
        } else {
            self.embedding_dim = Some(dim);
        }

        self.docs
            .push(EmbeddingDoc::new(id, embedding).with_text(text));
        Ok(())
    }

    /// Search for the top-K most similar embeddings.
    ///
    /// Returns documents sorted by cosine similarity (highest first).
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchHit> {
        if self.docs.is_empty() || query.is_empty() {
            return Vec::new();
        }

        let k = k.min(self.docs.len());

        let embeddings: Vec<Vec<f32>> = self.docs.iter().map(|d| d.embedding.clone()).collect();

        self.searcher
            .top_k(query, &embeddings, k)
            .into_iter()
            .filter_map(|TopKResult { index, score }| {
                if index < self.docs.len() {
                    let doc = &self.docs[index];
                    Some(SearchHit {
                        id: doc.id.clone(),
                        score,
                        text: doc.text.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Search with the default K value.
    pub fn search_default(&self, query: &[f32]) -> Vec<SearchHit> {
        self.search(query, self.default_k)
    }

    /// Number of documents in the index.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Get the embedding dimension.
    pub fn embedding_dim(&self) -> Option<usize> {
        self.embedding_dim
    }

    /// Clear all documents from the index.
    pub fn clear(&mut self) {
        self.docs.clear();
        self.embedding_dim = None;
    }
}

/// A search result hit.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Document identifier.
    pub id: String,
    /// Cosine similarity score.
    pub score: f64,
    /// Optional text content.
    pub text: Option<String>,
}

/// Embedding computation utilities for WASM plugins.
///
/// Provides SIMD-accelerated vector operations via `touring_simd::CosineComputer`
/// that can be used during plugin evaluation for scoring and ranking.
pub mod compute {
    use super::*;
    use touring_simd::{CosineComputer, CosineSimilarity};

    /// SIMD-accelerated cosine similarity via `touring_simd::CosineComputer`.
    ///
    /// Falls back to scalar computation if SIMD dispatch fails.
    #[inline]
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        CosineComputer::new().cosine(a, b)
    }

    /// Compute squared Euclidean distance between two vectors.
    #[inline]
    pub fn squared_distance(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return f32::INFINITY;
        }

        a.iter()
            .zip(b.iter())
            .map(|(x, y)| {
                let d = x - y;
                d * d
            })
            .sum()
    }

    /// Batch compute similarities between a query and multiple candidates.
    pub fn batch_similarity(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f64> {
        candidates
            .iter()
            .map(|c| cosine_similarity(query, c))
            .collect()
    }

    /// Find top-K indices by similarity score.
    pub fn top_k_indices(query: &[f32], candidates: &[Vec<f32>], k: usize) -> Vec<(usize, f64)> {
        let searcher = TopKSearcher::new(k);
        let results = searcher.top_k(query, candidates, k);
        results.into_iter().map(|r| (r.index, r.score)).collect()
    }
}

/// Plugin context extension for embedding-aware evaluation.
///
/// This trait extends `TypedPluginContext` with embedding search capabilities.
pub trait EmbeddingSearchExt {
    /// Get the query embedding if present.
    fn query_embedding(&self) -> Option<&[f32]>;

    /// Get the number of results to return.
    fn top_k(&self) -> Option<usize>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_search_add_and_search() {
        let mut search = EmbeddingSearch::new(2);
        search.add("a", vec![1.0, 0.0, 0.0]).unwrap();
        search.add("b", vec![0.0, 1.0, 0.0]).unwrap();
        search.add("c", vec![0.9, 0.1, 0.0]).unwrap();

        let results = search.search(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert!((results[0].score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_search_with_text() {
        let mut search = EmbeddingSearch::new(2);
        search
            .add_with_text("doc1", vec![1.0, 0.0], "Hello world")
            .unwrap();

        let results = search.search(&[1.0, 0.0], 1);
        assert_eq!(results[0].id, "doc1");
        assert_eq!(results[0].text.as_deref(), Some("Hello world"));
    }

    #[test]
    fn test_embedding_search_dimension_mismatch() {
        let mut search = EmbeddingSearch::new(2);
        search.add("a", vec![1.0, 0.0]).unwrap();
        let result = search.add("b", vec![1.0, 0.0, 0.0]);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("dimension mismatch")
        );
    }

    #[test]
    fn test_embedding_search_empty() {
        let search = EmbeddingSearch::new(2);
        let results = search.search(&[1.0, 0.0], 2);
        assert!(results.is_empty());
    }

    #[test]
    fn test_embedding_search_query_empty() {
        let mut search = EmbeddingSearch::new(2);
        search.add("a", vec![1.0, 0.0]).unwrap();
        let results = search.search(&[], 2);
        assert!(results.is_empty());
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((compute::cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
        assert!((compute::cosine_similarity(&a, &c) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_squared_distance() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0];
        assert!((compute::squared_distance(&a, &b) - 25.0).abs() < 1e-6);
    }

    #[test]
    fn test_batch_similarity() {
        let query = vec![1.0, 0.0];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![0.707, 0.707]];

        let scores = compute::batch_similarity(&query, &candidates);
        assert_eq!(scores.len(), 3);
        assert!((scores[0] - 1.0).abs() < 1e-6);
        assert!((scores[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_top_k_indices() {
        let query = vec![1.0, 0.0];
        let candidates = vec![vec![0.5, 0.5], vec![1.0, 0.0], vec![0.0, 0.1]];

        let results = compute::top_k_indices(&query, &candidates, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1); // [1.0, 0.0] is most similar
        assert!((results[0].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_clear() {
        let mut search = EmbeddingSearch::new(2);
        search.add("a", vec![1.0, 0.0]).unwrap();
        assert_eq!(search.len(), 1);

        search.clear();
        assert!(search.is_empty());
        assert!(search.embedding_dim().is_none());
    }
}
