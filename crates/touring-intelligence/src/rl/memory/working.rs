//! Working memory implementation with LRU cache.
//!
//! Unified from rust-core/src/learning/memory/working.rs (254 LOC)
//! Uses touring-simd CosineComputer and TopKSearcher for SIMD-accelerated similarity search.

use indexmap::IndexMap;
use std::hash::Hash;
use touring_simd::{CosineComputer, CosineSimilarity, TopKResult, TopKSearcher};

/// Working memory with LRU eviction and similarity search.
pub trait WorkingMemory<K, V>
where
    K: Clone,
{
    /// Inserts a key/value pair, evicting the least-recently-used entry if full.
    fn insert(&mut self, key: K, value: V);
    /// Returns the value for `key`, marking it as recently used.
    fn get(&mut self, key: &K) -> Option<&V>;
    /// Returns entries whose similarity to `query` exceeds `threshold`, up to `limit`.
    fn find_similar(&self, query: &[f32], threshold: f64, limit: usize) -> Vec<(K, f64)>;
    /// Find top-K most similar entries using SIMD-accelerated search.
    fn find_similar_topk(&self, query: &[f32], k: usize) -> Vec<(K, f64)>;
    /// Returns the number of entries currently held.
    fn len(&self) -> usize;
    /// Returns `true` if the memory holds no entries.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Removes all entries.
    fn clear(&mut self);
}

/// Entry in working memory with embedding.
#[derive(Debug, Clone)]
pub struct MemoryEntry<V> {
    /// Stored value of the entry.
    pub value: V,
    /// Embedding vector used for similarity search.
    pub embedding: Vec<f32>,
    /// Number of times the entry has been accessed.
    pub access_count: u64,
}

/// LRU working memory with SIMD-accelerated similarity search.
#[derive(Debug)]
pub struct LruWorkingMemory<K, V>
where
    K: Eq + Hash + Clone,
{
    capacity: usize,
    entries: IndexMap<K, MemoryEntry<V>>,
    cosine: CosineComputer,
    topk_searcher: TopKSearcher,
}

impl<K, V> LruWorkingMemory<K, V>
where
    K: Eq + Hash + Clone,
{
    /// Creates an empty working memory with the given LRU capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: IndexMap::with_capacity(capacity),
            cosine: CosineComputer::new(),
            topk_searcher: TopKSearcher::new(64),
        }
    }

    /// Returns the configured LRU capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn insert_with_embedding(&mut self, key: K, value: V, embedding: Vec<f32>) {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            self.entries.shift_remove_index(0);
        }
        self.entries.shift_remove(&key);
        self.entries.insert(
            key,
            MemoryEntry {
                value,
                embedding,
                access_count: 1,
            },
        );
    }

    pub(crate) fn get_entry(&mut self, key: &K) -> Option<&MemoryEntry<V>> {
        match self.entries.shift_remove(key) {
            Some(entry) => {
                let key_clone = key.clone();
                let mut updated_entry = entry;
                updated_entry.access_count += 1;
                self.entries.insert(key_clone, updated_entry);
                self.entries.get(key)
            }
            _ => None,
        }
    }

    /// Returns the value for `key` without updating its recency.
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|e| &e.value)
    }

    /// Removes the entry for `key`, returning whether it was present.
    pub fn remove(&mut self, key: &K) -> bool {
        self.entries.shift_remove(key).is_some()
    }

    /// Returns each key paired with its embedding slice.
    pub fn get_all_embeddings(&self) -> Vec<(&K, &[f32])> {
        self.entries
            .iter()
            .map(|(k, e)| (k, e.embedding.as_slice()))
            .collect()
    }

    /// Find top-K most similar entries using SIMD-accelerated search.
    ///
    /// More efficient than `find_similar` for large memories when you only
    /// need the top-K results without a threshold.
    pub fn find_topk(&self, query: &[f32], k: usize) -> Vec<(K, f64)> {
        if query.is_empty() || self.entries.is_empty() {
            return vec![];
        }

        // Build parallel vectors for TopK search
        let entries: Vec<(K, Vec<f32>)> = self
            .entries
            .iter()
            .filter(|(_, e)| !e.embedding.is_empty())
            .map(|(k, e)| (k.clone(), e.embedding.clone()))
            .collect();

        if entries.is_empty() {
            return vec![];
        }

        let candidates: Vec<Vec<f32>> = entries.iter().map(|(_, e)| e.clone()).collect();

        let results = self.topk_searcher.top_k(query, &candidates, k);

        results
            .into_iter()
            .filter_map(|TopKResult { index, score }| {
                entries.get(index).map(|(k, _)| (k.clone(), score))
            })
            .collect()
    }

    /// Find by Euclidean distance instead of cosine similarity.
    pub fn find_by_distance(&self, query: &[f32], k: usize) -> Vec<(K, f64)> {
        use touring_simd::squared_euclidean;

        if query.is_empty() || self.entries.is_empty() {
            return vec![];
        }

        let mut entries: Vec<(K, f32)> = self
            .entries
            .iter()
            .filter(|(_, e)| !e.embedding.is_empty() && e.embedding.len() == query.len())
            .map(|(k, e)| (k.clone(), squared_euclidean(&e.embedding, query)))
            .collect();

        entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        entries.truncate(k);
        entries.into_iter().map(|(k, d)| (k, d as f64)).collect()
    }
}

impl<K, V> WorkingMemory<K, V> for LruWorkingMemory<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    fn insert(&mut self, key: K, value: V) {
        self.insert_with_embedding(key, value, vec![]);
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        self.get_entry(key).map(|e| &e.value)
    }

    fn find_similar(&self, query: &[f32], threshold: f64, limit: usize) -> Vec<(K, f64)> {
        if query.is_empty() {
            return vec![];
        }

        let mut results: Vec<(K, f64)> = self
            .entries
            .iter()
            .filter_map(|(k, entry)| {
                if entry.embedding.is_empty() {
                    return None;
                }
                let similarity = self.cosine.cosine(&entry.embedding, query);
                if similarity >= threshold {
                    Some((k.clone(), similarity))
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    fn find_similar_topk(&self, query: &[f32], k: usize) -> Vec<(K, f64)> {
        self.find_topk(query, k)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_insert_and_get() {
        let mut memory: LruWorkingMemory<String, i32> = LruWorkingMemory::new(10);
        memory.insert("key1".to_string(), 42);
        assert_eq!(memory.get(&"key1".to_string()), Some(&42));
        assert_eq!(memory.get(&"key2".to_string()), None);
    }

    #[test]
    fn test_lru_eviction() {
        let mut memory: LruWorkingMemory<i32, i32> = LruWorkingMemory::new(3);
        memory.insert(1, 100);
        memory.insert(2, 200);
        memory.insert(3, 300);
        assert_eq!(memory.len(), 3);
        memory.insert(4, 400);
        assert_eq!(memory.len(), 3);
        assert_eq!(memory.get(&1), None);
        assert_eq!(memory.get(&4), Some(&400));
    }

    #[test]
    fn test_clear() {
        let mut memory: LruWorkingMemory<i32, i32> = LruWorkingMemory::new(10);
        memory.insert(1, 100);
        memory.insert(2, 200);
        memory.clear();
        assert!(memory.is_empty());
    }

    #[test]
    fn test_find_topk() {
        let mut memory: LruWorkingMemory<String, i32> = LruWorkingMemory::new(10);
        memory.insert_with_embedding("a".to_string(), 1, vec![1.0, 0.0, 0.0]);
        memory.insert_with_embedding("b".to_string(), 2, vec![0.0, 1.0, 0.0]);
        memory.insert_with_embedding("c".to_string(), 3, vec![0.9, 0.1, 0.0]);

        let results = memory.find_topk(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "a");
        assert!((results[0].1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_find_by_distance() {
        let mut memory: LruWorkingMemory<String, i32> = LruWorkingMemory::new(10);
        memory.insert_with_embedding("a".to_string(), 1, vec![0.0, 0.0]);
        memory.insert_with_embedding("b".to_string(), 2, vec![3.0, 4.0]);

        let results = memory.find_by_distance(&[0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "a");
    }
}
