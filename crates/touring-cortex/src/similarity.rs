//! Similarity search module for touring-cortex.
//!
//! Provides SIMD-accelerated embedding similarity for semantic search
//! and retrieval augmentation. Uses touring-simd for high-performance
//! cosine similarity and top-K search via [`TopKSearcher`].
//!
//! # Relationship with touring-simd traits
//!
//! touring-simd defines generic similarity traits in
//! `touring_simd::similarity::traits`:
//! - `Similarity<T>`: generic trait for any item type
//! - `JaccardSimilarity`: Jaccard set similarity
//! - `CosineSimilarity`: cosine similarity on vectors
//!
//! This module does NOT implement or re-export those traits because
//! [`EmbeddingIndex`] uses [`TopKSearcher`] internally,
//! which performs SIMD-accelerated top-K search directly on stored embeddings
//! without requiring a generic `Similarity<T>` impl. The two abstractions serve
//! different use cases:
//! - touring-simd traits: custom similarity logic (user-defined types)
//! - this module: fast ANN-style retrieval over pre-computed embedding vectors
//!
//! # Integration Points
//!
//! - Handlers use this for embedding-based retrieval
//! - Pipeline enrichment via similarity scores
//! - RRF fusion with similarity-based rankings

use rayon::prelude::*;
use touring_simd::{TopKResult, TopKSearcher};

/// An embedding vector for similarity computation.
pub type Embedding = Vec<f32>;

/// A search result with similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Unique identifier for the result.
    pub id: String,
    /// Similarity score (cosine similarity).
    pub score: f64,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

impl SearchResult {
    /// Creates a result with the given id and score and no metadata.
    pub fn new(id: String, score: f64) -> Self {
        Self {
            id,
            score,
            metadata: None,
        }
    }

    /// Returns the result with attached metadata, replacing any previous value.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Embedding index for fast similarity search.
#[derive(Debug)]
pub struct EmbeddingIndex {
    /// Stored embeddings.
    embeddings: Vec<Embedding>,
    /// IDs corresponding to embeddings.
    ids: Vec<String>,
    /// Top-K searcher for fast retrieval.
    searcher: TopKSearcher,
}

impl Default for EmbeddingIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingIndex {
    /// Create a new embedding index.
    pub fn new() -> Self {
        Self {
            embeddings: Vec::new(),
            ids: Vec::new(),
            searcher: TopKSearcher::new(64),
        }
    }

    /// Add an embedding to the index.
    pub fn add(&mut self, id: &str, embedding: Embedding) {
        self.ids.push(id.to_string());
        self.embeddings.push(embedding);
    }

    /// Add multiple embeddings in parallel.
    pub fn add_batch(&mut self, items: Vec<(String, Embedding)>) {
        for (id, embedding) in items {
            self.ids.push(id);
            self.embeddings.push(embedding);
        }
    }

    /// Search for top-K most similar embeddings.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        if self.embeddings.is_empty() || query.is_empty() {
            return vec![];
        }

        let k = k.min(self.embeddings.len());

        self.searcher
            .top_k(query, &self.embeddings, k)
            .into_iter()
            .filter_map(|TopKResult { index, score }| {
                self.ids
                    .get(index)
                    .map(|id| SearchResult::new(id.clone(), score))
            })
            .collect()
    }

    /// Search by Euclidean distance instead of cosine similarity.
    pub fn search_by_distance(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        if self.embeddings.is_empty() || query.is_empty() {
            return vec![];
        }

        let k = k.min(self.embeddings.len());

        let mut results: Vec<(usize, f32)> = self
            .embeddings
            .iter()
            .enumerate()
            .filter(|(_, e)| e.len() == query.len())
            .map(|(i, e)| {
                let dist = e.iter().zip(query.iter()).fold(0.0f32, |acc, (a, b)| {
                    let diff = a - b;
                    acc + diff * diff
                });
                (i, dist)
            })
            .collect();

        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        results
            .into_iter()
            .take(k)
            .filter_map(|(i, dist)| {
                self.ids
                    .get(i)
                    .map(|id| SearchResult::new(id.clone(), 1.0 / (1.0 + dist as f64)))
            })
            .collect()
    }

    /// Number of embeddings in the index.
    pub fn len(&self) -> usize {
        self.embeddings.len()
    }

    /// Check if index is empty.
    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }

    /// Clear all embeddings.
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.ids.clear();
    }
}

/// Batch search results.
#[derive(Debug)]
pub struct BatchSearchResult {
    /// Per-query ranked results, one inner vector per input query.
    pub results: Vec<Vec<SearchResult>>,
}

impl BatchSearchResult {
    /// Wraps per-query result vectors into a batch result.
    pub fn new(results: Vec<Vec<SearchResult>>) -> Self {
        Self { results }
    }
}

/// Parallel batch search for multiple queries.
pub fn batch_search(index: &EmbeddingIndex, queries: &[Embedding], k: usize) -> BatchSearchResult {
    let results = queries
        .par_iter()
        .map(|query| index.search(query, k))
        .collect();

    BatchSearchResult::new(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_index_search() {
        let mut index = EmbeddingIndex::new();

        index.add("a", vec![1.0, 0.0, 0.0]);
        index.add("b", vec![0.0, 1.0, 0.0]);
        index.add("c", vec![0.9, 0.1, 0.0]);

        let results = index.search(&[1.0, 0.0, 0.0], 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert!((results[0].score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_empty_index() {
        let index = EmbeddingIndex::new();
        let results = index.search(&[1.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_by_distance() {
        let mut index = EmbeddingIndex::new();
        index.add("origin", vec![0.0, 0.0]);
        index.add("close", vec![1.0, 1.0]);
        index.add("far", vec![10.0, 10.0]);

        let results = index.search_by_distance(&[0.0, 0.0], 3);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "origin");
        assert_eq!(results[1].id, "close");
        assert_eq!(results[2].id, "far");
    }
}
