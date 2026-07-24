//! Similarity trait definitions.
//!
//! Pln3 Phase 1B - L1: Interface Contracts

/// Trait for Jaccard similarity computation on token sets.
pub trait JaccardSimilarity {
    /// Compute Jaccard similarity between two sorted sets.
    ///
    /// Returns value in [0, 1] where 1 = identical sets.
    fn jaccard(&self, a: &[u32], b: &[u32]) -> f64;

    /// Batch computation of Jaccard similarities.
    ///
    /// Returns similarity for each (query, candidate) pair.
    fn jaccard_batch(&self, queries: &[Vec<u32>], candidates: &[Vec<u32>]) -> Vec<f64>;
}

/// Trait for Cosine similarity computation on vectors.
pub trait CosineSimilarity {
    /// Compute cosine similarity between two vectors.
    ///
    /// Returns value in [-1, 1] where 1 = same direction.
    fn cosine(&self, a: &[f32], b: &[f32]) -> f64;

    /// Batch computation of cosine similarities.
    fn cosine_batch(&self, queries: &[Vec<f32>], candidates: &[Vec<f32>]) -> Vec<f64>;
}

/// Generic similarity trait for any item type.
///
/// Prefer using this with slice types (`[f32]`, `[u32]`) rather than `Vec`.
pub trait Similarity<T: ?Sized> {
    /// Compute similarity between two items.
    fn similarity(&self, a: &T, b: &T) -> f64;
}
