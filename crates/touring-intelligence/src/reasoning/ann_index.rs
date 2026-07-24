//! S8: Approximate Nearest Neighbor (ANN) index for SemanticGraph.
//!
//! Implements IVF-flat (Inverted File with flat scan) partitioning:
//! - K centroids are computed via simplified k-means
//! - Each embedding is assigned to its nearest centroid
//! - At query time, only the top-P closest partitions are scanned
//!
//! This reduces retrieval from O(N) full scan to O(N/K * P) where
//! K = number of partitions and P = number of probes.
//!
//! Uses `touring_simd::TopKSearcher` for SIMD-accelerated centroid finding
//! and cosine similarity computation.

use std::collections::HashMap;
use touring_simd::{CosineComputer, CosineSimilarity, TopKResult, TopKSearcher};

/// Default number of partitions (centroids).
const DEFAULT_NUM_PARTITIONS: usize = 8;

/// Default number of partitions to probe at query time.
const DEFAULT_NUM_PROBES: usize = 2;

/// Maximum k-means iterations.
const MAX_KMEANS_ITERATIONS: usize = 10;

/// An entry in the ANN index: (id, embedding).
#[derive(Debug, Clone)]
struct IndexEntry {
    id: String,
    embedding: Vec<f32>,
}

/// IVF-flat ANN index for fast approximate nearest neighbor search.
///
/// Partitions embeddings into K clusters via simplified k-means,
/// then at query time only scans the P closest partitions.
#[derive(Debug)]
pub struct AnnIndex {
    /// Centroid embeddings (one per partition).
    centroids: Vec<Vec<f32>>,
    /// Partition assignments: centroid_idx -> entries.
    partitions: HashMap<usize, Vec<IndexEntry>>,
    /// Number of partitions to probe during search.
    num_probes: usize,
    /// Embedding dimensionality.
    dim: usize,
    /// Total number of indexed entries.
    total_entries: usize,
}

impl AnnIndex {
    /// Create an empty index with default parameters.
    pub fn new(dim: usize) -> Self {
        Self {
            centroids: Vec::new(),
            partitions: HashMap::new(),
            num_probes: DEFAULT_NUM_PROBES,
            dim,
            total_entries: 0,
        }
    }

    /// Create with custom number of partitions and probes.
    pub fn with_params(dim: usize, num_probes: usize) -> Self {
        Self {
            centroids: Vec::new(),
            partitions: HashMap::new(),
            num_probes: num_probes.max(1),
            dim,
            total_entries: 0,
        }
    }

    /// Build the index from a set of (id, embedding) pairs.
    ///
    /// Runs simplified k-means to compute centroids, then assigns each
    /// embedding to its nearest centroid.
    pub fn build(&mut self, entries: &[(String, Vec<f32>)]) {
        self.partitions.clear();
        self.centroids.clear();
        self.total_entries = 0;

        if entries.is_empty() || self.dim == 0 {
            return;
        }

        // Filter entries with matching dimensionality
        let valid: Vec<&(String, Vec<f32>)> = entries
            .iter()
            .filter(|(_, e)| e.len() == self.dim)
            .collect();

        if valid.is_empty() {
            return;
        }

        let num_partitions = DEFAULT_NUM_PARTITIONS.min(valid.len());
        if num_partitions == 0 {
            return;
        }

        // Initialize centroids: take first K entries (deterministic)
        self.centroids = valid
            .iter()
            .take(num_partitions)
            .map(|(_, e)| e.clone())
            .collect();

        // K-means iterations
        for _ in 0..MAX_KMEANS_ITERATIONS {
            // Assign each entry to nearest centroid
            let mut assignments: HashMap<usize, Vec<usize>> = HashMap::new();
            for (i, (_, emb)) in valid.iter().enumerate() {
                let nearest = self.nearest_centroid(emb);
                assignments.entry(nearest).or_default().push(i);
            }

            // Update centroids as mean of assigned entries
            let mut changed = false;
            for (ci, indices) in &assignments {
                if indices.is_empty() {
                    continue;
                }
                let mut new_centroid = vec![0.0_f32; self.dim];
                for &idx in indices {
                    if let Some((_, emb)) = valid.get(idx) {
                        for (j, &v) in emb.iter().enumerate() {
                            if let Some(c) = new_centroid.get_mut(j) {
                                *c += v;
                            }
                        }
                    }
                }
                let count = indices.len() as f32;
                for v in &mut new_centroid {
                    *v /= count;
                }
                if let Some(centroid) = self.centroids.get_mut(*ci) {
                    if *centroid != new_centroid {
                        *centroid = new_centroid;
                        changed = true;
                    }
                }
            }

            if !changed {
                break;
            }
        }

        // Final assignment: populate partitions
        for (id, emb) in valid {
            let nearest = self.nearest_centroid(emb);
            self.partitions
                .entry(nearest)
                .or_default()
                .push(IndexEntry {
                    id: id.clone(),
                    embedding: emb.clone(),
                });
            self.total_entries += 1;
        }
    }

    /// Query the index for top-k nearest neighbors.
    ///
    /// Probes the `num_probes` closest partitions and returns entries
    /// sorted by cosine similarity descending.
    ///
    /// Uses `TopKSearcher` for SIMD-accelerated centroid finding.
    pub fn query(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if query.len() != self.dim || self.centroids.is_empty() {
            return vec![];
        }

        // Find closest partitions using TopKSearcher (SIMD-accelerated)
        let searcher = TopKSearcher::new(self.num_probes);
        let centroid_results = searcher.top_k(query, &self.centroids, self.num_probes);

        // Scan top-P partitions
        let mut results: Vec<(String, f32)> = Vec::new();
        for TopKResult {
            index: ci,
            score: _,
        } in centroid_results
        {
            if let Some(entries) = self.partitions.get(&ci) {
                for entry in entries {
                    let sim = cosine_sim(query, &entry.embedding);
                    results.push((entry.id.clone(), sim));
                }
            }
        }

        // Sort by similarity descending and take top-k
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        results
    }

    /// Number of indexed entries.
    pub fn len(&self) -> usize {
        self.total_entries
    }

    /// True if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.total_entries == 0
    }

    /// Number of partitions (centroids).
    pub fn num_partitions(&self) -> usize {
        self.centroids.len()
    }

    /// Find the nearest centroid index for an embedding.
    fn nearest_centroid(&self, embedding: &[f32]) -> usize {
        let mut best_idx = 0;
        let mut best_sim = f32::NEG_INFINITY;
        for (i, c) in self.centroids.iter().enumerate() {
            let sim = cosine_sim(embedding, c);
            if sim > best_sim {
                best_sim = sim;
                best_idx = i;
            }
        }
        best_idx
    }
}

/// Cosine similarity between two vectors. Returns 0.0 for zero/mismatched vectors.
fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    CosineComputer::new().cosine(a, b) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entries(n: usize, dim: usize) -> Vec<(String, Vec<f32>)> {
        (0..n)
            .map(|i| {
                let mut emb = vec![0.0_f32; dim];
                // Create distinct embeddings: unit vector in different directions
                emb[i % dim] = 1.0;
                // Add small noise for variety
                for (j, v) in emb.iter_mut().enumerate() {
                    *v += (i * 7 + j * 3) as f32 * 0.01;
                }
                (format!("node_{i}"), emb)
            })
            .collect()
    }

    #[test]
    fn test_empty_index() {
        let idx = AnnIndex::new(3);
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
        assert!(idx.query(&[1.0, 0.0, 0.0], 5).is_empty());
    }

    #[test]
    fn test_build_and_query() {
        let mut idx = AnnIndex::new(3);
        let entries = vec![
            ("a".to_string(), vec![1.0, 0.0, 0.0]),
            ("b".to_string(), vec![0.0, 1.0, 0.0]),
            ("c".to_string(), vec![0.9, 0.1, 0.0]),
        ];
        idx.build(&entries);

        assert_eq!(idx.len(), 3);
        assert!(!idx.is_empty());

        let results = idx.query(&[1.0, 0.0, 0.0], 2);
        assert!(!results.is_empty());
        // "a" should be closest to [1,0,0]
        assert_eq!(results[0].0, "a");
    }

    #[test]
    fn test_query_returns_top_k() {
        let mut idx = AnnIndex::new(4);
        let entries = make_entries(20, 4);
        idx.build(&entries);

        let results = idx.query(&[1.0, 0.0, 0.0, 0.0], 5);
        assert!(results.len() <= 5);
    }

    #[test]
    fn test_dimension_mismatch_filtered() {
        let mut idx = AnnIndex::new(3);
        let entries = vec![
            ("a".to_string(), vec![1.0, 0.0, 0.0]),
            ("bad".to_string(), vec![1.0, 0.0]), // wrong dim
        ];
        idx.build(&entries);
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn test_query_wrong_dim() {
        let mut idx = AnnIndex::new(3);
        let entries = vec![("a".to_string(), vec![1.0, 0.0, 0.0])];
        idx.build(&entries);

        // Query with wrong dimension returns empty
        assert!(idx.query(&[1.0, 0.0], 5).is_empty());
    }

    #[test]
    fn test_large_index() {
        let mut idx = AnnIndex::with_params(8, 3);
        let entries = make_entries(100, 8);
        idx.build(&entries);

        assert_eq!(idx.len(), 100);
        assert!(idx.num_partitions() <= DEFAULT_NUM_PARTITIONS);

        let results = idx.query(&[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 10);
        assert!(!results.is_empty());
        assert!(results.len() <= 10);
    }

    #[test]
    fn test_cosine_sim_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_sim(&a, &a);
        assert!((sim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_sim_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_sim(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn test_cosine_sim_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_sim(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_rebuild_index() {
        let mut idx = AnnIndex::new(3);
        let entries1 = vec![("a".to_string(), vec![1.0, 0.0, 0.0])];
        idx.build(&entries1);
        assert_eq!(idx.len(), 1);

        // Rebuild with different data
        let entries2 = vec![
            ("x".to_string(), vec![0.0, 1.0, 0.0]),
            ("y".to_string(), vec![0.0, 0.0, 1.0]),
        ];
        idx.build(&entries2);
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn test_single_entry_index() {
        let mut idx = AnnIndex::new(3);
        let entries = vec![("only".to_string(), vec![1.0, 0.0, 0.0])];
        idx.build(&entries);
        assert_eq!(idx.len(), 1);
        assert_eq!(idx.num_partitions(), 1);

        let results = idx.query(&[1.0, 0.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "only");
    }
}
