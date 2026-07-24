//! Approximate Nearest Neighbor (ANN) search.
//!
//! Provides HNSW (Hierarchical Navigable Small World) index for
//! sub-linear similarity search on large vector collections.
//!
//! # Feature Gate
//!
//! Requires the `ann` feature flag.

#[cfg(feature = "ann")]
pub mod hnsw;

use crate::similarity::TopKResult;

/// Trait for ANN index implementations.
pub trait AnnIndex {
    /// Search for the k nearest neighbors of `query`.
    fn search(&self, query: &[f32], k: usize) -> Vec<TopKResult>;

    /// Insert a vector with the given ID.
    fn insert(&mut self, id: usize, vector: Vec<f32>);

    /// Number of vectors in the index.
    fn len(&self) -> usize;

    /// Whether the index is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
