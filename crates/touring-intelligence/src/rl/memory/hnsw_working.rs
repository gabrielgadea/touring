//! HNSW-backed working memory -- feature-gated alternative to LRU.
//!
//! Uses `instant-distance` for approximate nearest neighbor search.
//! `bumpalo` is available for arena allocation in future extensions.
//!
//! Trade-off: rebuild cost on every insert (suitable for small working sets,
//! typically < 1000 entries), but O(log n) search vs O(n) for LRU.
//!
//! For async compatibility, use `tokio::task::spawn_blocking` to wrap
//! HNSW queries that may take >1ms on large indices.
//!
//! # SIMD Acceleration
//!
//! Distance computation is accelerated via `touring_simd::similarity::cosine_distance`
//! which uses hardware SIMD (AVX2/NEON) when available, falling back to portable scalar
//! code transparently.

use std::hash::Hash;

use instant_distance::{Builder, HnswMap, Search};

use super::working::WorkingMemory;
use touring_simd::similarity::cosine_distance;

/// A point wrapper for instant-distance compatibility.
///
/// Implements cosine distance (1 - cosine_similarity) for the HNSW index.
/// Uses `touring_simd::similarity::cosine_distance` for SIMD-accelerated computation.
#[derive(Clone)]
struct EmbeddingPoint(Vec<f32>);

impl instant_distance::Point for EmbeddingPoint {
    fn distance(&self, other: &Self) -> f32 {
        // SIMD-accelerated cosine distance via touring-simd.
        // Falls back to scalar implementation when SIMD is not available.
        cosine_distance(&self.0, &other.0) as f32
    }
}

/// HNSW-backed working memory with approximate nearest neighbor search.
///
/// Rebuilds the HNSW index on every insert (suitable for small working sets).
/// For large datasets, consider incremental indexing strategies.
pub struct WorkingMemoryHnsw<K>
where
    K: Eq + Hash + Clone,
{
    capacity: usize,
    keys: Vec<K>,
    values: Vec<String>,
    embeddings: Vec<Vec<f32>>,
    index: Option<HnswMap<EmbeddingPoint, usize>>,
}

impl<K> WorkingMemoryHnsw<K>
where
    K: Eq + Hash + Clone,
{
    /// Create a new HNSW working memory with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            keys: Vec::with_capacity(capacity),
            values: Vec::with_capacity(capacity),
            embeddings: Vec::with_capacity(capacity),
            index: None,
        }
    }

    /// Insert with embedding and rebuild the HNSW index.
    #[allow(dead_code, clippy::indexing_slicing)] // pos from position() is always valid within the same vec
    pub(crate) fn insert_with_embedding(&mut self, key: K, value: String, embedding: Vec<f32>) {
        // Replace if key exists
        if let Some(pos) = self.keys.iter().position(|k| k == &key) {
            self.values[pos] = value;
            self.embeddings[pos] = embedding;
        } else {
            // Evict oldest if at capacity
            if self.keys.len() >= self.capacity {
                self.keys.remove(0);
                self.values.remove(0);
                self.embeddings.remove(0);
            }
            self.keys.push(key);
            self.values.push(value);
            self.embeddings.push(embedding);
        }
        self.rebuild_index();
    }

    /// Query nearest neighbors by embedding vector.
    ///
    /// Returns up to `k` entries as `(&key, cosine_similarity)` pairs,
    /// sorted by descending similarity.
    ///
    /// For async contexts, wrap in `spawn_blocking`:
    /// ```rust,ignore
    /// // Note: query_nearest returns references tied to self, so spawn_blocking
    /// // requires an owned wrapper (see WorkingMemoryHnsw::find_similar for the
    /// // owned-result variant suitable for async contexts).
    /// let results = mem.query_nearest(&[0.1_f32], 5);
    /// ```
    #[allow(clippy::indexing_slicing, clippy::needless_collect)] // idx bounds-checked; collect releases borrow on search
    pub(crate) fn query_nearest(&self, query: &[f32], k: usize) -> Vec<(&K, f64)> {
        let Some(ref index) = self.index else {
            return Vec::new();
        };

        let query_point = EmbeddingPoint(query.to_vec());
        let mut search = Search::default();
        let results: Vec<_> = index.search(&query_point, &mut search).take(k).collect();

        results
            .into_iter()
            .filter_map(|item| {
                let idx = *item.value;
                if idx < self.keys.len() {
                    let similarity = 1.0 - item.distance as f64;
                    Some((&self.keys[idx], similarity))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Rebuild the HNSW index from current embeddings.
    ///
    /// **Complexity**: O(n log n) where n = number of entries with embeddings.
    /// Suitable for small working sets (< 1000 entries). For larger datasets,
    /// consider incremental index updates.
    ///
    /// Only builds when there are entries with non-empty embeddings.
    #[allow(dead_code, clippy::indexing_slicing)] // indices from has_embeddings filter — always valid
    fn rebuild_index(&mut self) {
        // Filter to entries that actually have embeddings
        let has_embeddings: Vec<usize> = self
            .embeddings
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_empty())
            .map(|(i, _)| i)
            .collect();

        if has_embeddings.is_empty() {
            self.index = None;
            return;
        }

        let points: Vec<EmbeddingPoint> = has_embeddings
            .iter()
            .map(|&i| EmbeddingPoint(self.embeddings[i].clone()))
            .collect();
        let values: Vec<usize> = has_embeddings;

        self.index = Some(Builder::default().build(points, values));
    }
}

impl<K> WorkingMemory<K, String> for WorkingMemoryHnsw<K>
where
    K: Eq + Hash + Clone,
{
    #[allow(clippy::indexing_slicing)] // pos from position() is valid within the same vec
    fn insert(&mut self, key: K, value: String) {
        // Insert without embedding -- no HNSW benefit, but trait compliance
        if let Some(pos) = self.keys.iter().position(|k| k == &key) {
            self.values[pos] = value;
        } else {
            if self.keys.len() >= self.capacity {
                self.keys.remove(0);
                self.values.remove(0);
                self.embeddings.remove(0);
            }
            self.keys.push(key);
            self.values.push(value);
            self.embeddings.push(Vec::new());
        }
        // No rebuild: empty embeddings won't participate in HNSW search
    }

    #[allow(clippy::indexing_slicing)] // i from position() is valid within the same vec
    fn get(&mut self, key: &K) -> Option<&String> {
        self.keys
            .iter()
            .position(|k| k == key)
            .map(|i| &self.values[i])
    }

    fn find_similar(&self, query: &[f32], threshold: f64, limit: usize) -> Vec<(K, f64)> {
        self.query_nearest(query, limit)
            .into_iter()
            .filter(|(_, sim)| *sim >= threshold)
            .map(|(k, sim)| (k.clone(), sim))
            .collect()
    }

    fn find_similar_topk(&self, query: &[f32], k: usize) -> Vec<(K, f64)> {
        self.query_nearest(query, k)
            .into_iter()
            .map(|(key, sim)| (key.clone(), sim))
            .collect()
    }

    fn len(&self) -> usize {
        self.keys.len()
    }

    fn clear(&mut self) {
        self.keys.clear();
        self.values.clear();
        self.embeddings.clear();
        self.index = None;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;

    #[test]
    fn test_hnsw_insert_and_get() {
        let mut mem = WorkingMemoryHnsw::<String>::new(10);
        mem.insert("key1".to_string(), "value1".to_string());
        assert_eq!(mem.get(&"key1".to_string()), Some(&"value1".to_string()));
        assert_eq!(mem.len(), 1);
    }

    #[test]
    fn test_hnsw_update_existing_key() {
        let mut mem = WorkingMemoryHnsw::<String>::new(10);
        mem.insert("key1".to_string(), "value1".to_string());
        mem.insert("key1".to_string(), "value2".to_string());
        assert_eq!(mem.get(&"key1".to_string()), Some(&"value2".to_string()));
        assert_eq!(mem.len(), 1);
    }

    #[test]
    fn test_hnsw_capacity_eviction() {
        let mut mem = WorkingMemoryHnsw::<String>::new(2);
        mem.insert("k1".to_string(), "v1".to_string());
        mem.insert("k2".to_string(), "v2".to_string());
        mem.insert("k3".to_string(), "v3".to_string());
        assert_eq!(mem.len(), 2);
        assert!(mem.get(&"k1".to_string()).is_none()); // evicted
        assert_eq!(mem.get(&"k3".to_string()), Some(&"v3".to_string()));
    }

    #[test]
    fn test_hnsw_similarity_search() {
        let mut mem = WorkingMemoryHnsw::<String>::new(10);
        mem.insert_with_embedding("a".to_string(), "alpha".to_string(), vec![1.0, 0.0, 0.0]);
        mem.insert_with_embedding("b".to_string(), "beta".to_string(), vec![0.0, 1.0, 0.0]);
        mem.insert_with_embedding("c".to_string(), "gamma".to_string(), vec![0.9, 0.1, 0.0]);

        // Query similar to "a"
        let results = mem.query_nearest(&[1.0, 0.0, 0.0], 3);
        assert!(!results.is_empty());
        // First result should be "a" (exact match, similarity ~1.0)
        assert_eq!(*results[0].0, "a");
        assert!((results[0].1 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hnsw_find_similar_with_threshold() {
        let mut mem = WorkingMemoryHnsw::<String>::new(10);
        mem.insert_with_embedding("close".to_string(), "near".to_string(), vec![1.0, 0.0, 0.0]);
        mem.insert_with_embedding(
            "far".to_string(),
            "distant".to_string(),
            vec![0.0, 1.0, 0.0],
        );

        // High threshold should only return the close match
        let results = mem.find_similar(&[0.95, 0.05, 0.0], 0.9, 10);
        assert!(results.iter().all(|(_, sim)| *sim >= 0.9));
    }

    #[test]
    fn test_hnsw_empty_query() {
        let mem = WorkingMemoryHnsw::<String>::new(10);
        let results = mem.query_nearest(&[1.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_hnsw_clear() {
        let mut mem = WorkingMemoryHnsw::<String>::new(10);
        mem.insert("x".to_string(), "y".to_string());
        assert!(!mem.is_empty());
        mem.clear();
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
    }

    #[test]
    fn test_hnsw_mixed_embedded_and_plain() {
        let mut mem = WorkingMemoryHnsw::<String>::new(10);
        // Insert some with embeddings, some without
        mem.insert("plain".to_string(), "no_embedding".to_string());
        mem.insert_with_embedding(
            "vec".to_string(),
            "has_embedding".to_string(),
            vec![1.0, 0.0],
        );

        assert_eq!(mem.len(), 2);
        // Plain entries still accessible by key
        assert_eq!(
            mem.get(&"plain".to_string()),
            Some(&"no_embedding".to_string())
        );
        // Similarity search only returns embedded entries
        let results = mem.query_nearest(&[1.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(*results[0].0, "vec");
    }

    #[test]
    fn test_hnsw_parity_with_lru() {
        use super::super::working::LruWorkingMemory;

        let mut lru = LruWorkingMemory::<String, String>::new(5);
        let mut hnsw = WorkingMemoryHnsw::<String>::new(5);

        // Same insert operations
        for i in 0..5 {
            let key = format!("key_{i}");
            let val = format!("val_{i}");
            lru.insert(key.clone(), val.clone());
            hnsw.insert(key, val);
        }

        assert_eq!(lru.len(), hnsw.len());
        assert_eq!(
            lru.get(&"key_3".to_string()).cloned(),
            hnsw.get(&"key_3".to_string()).cloned()
        );
    }

    #[test]
    fn test_hnsw_eviction_with_embeddings() {
        let mut mem = WorkingMemoryHnsw::<String>::new(2);
        mem.insert_with_embedding("a".to_string(), "va".to_string(), vec![1.0, 0.0]);
        mem.insert_with_embedding("b".to_string(), "vb".to_string(), vec![0.0, 1.0]);
        mem.insert_with_embedding("c".to_string(), "vc".to_string(), vec![0.5, 0.5]);

        assert_eq!(mem.len(), 2);
        assert!(mem.get(&"a".to_string()).is_none());
        // Index should still work after eviction
        let results = mem.query_nearest(&[0.5, 0.5], 2);
        assert_eq!(results.len(), 2);
    }
}
