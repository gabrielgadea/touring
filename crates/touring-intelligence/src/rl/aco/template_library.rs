//! TemplateLibrary — persistent store for LearnedPatterns extracted by evolution.
//!
//! Stores patterns with usage tracking, similarity search, and LRU-style eviction.
//! INS-1 (ROI=2.00): cross-session pattern reuse reduces repeated extraction.
//! INS-L2: Optional EmbeddingStore trait object for real semantic similarity search.

use std::collections::HashMap;
use std::sync::Arc;

use super::models::LearnedPattern;

// ---------------------------------------------------------------------------
// INS-L2: EmbeddingStore trait
// ---------------------------------------------------------------------------

/// INS-L2: Backend for computing semantic similarity between two text strings.
///
/// Implementations can use any embedding strategy (dense vectors, TF-IDF, BM25).
/// Return value must be in [0.0, 1.0].
pub trait EmbeddingStore: Send + Sync {
    /// Compute similarity between two strings (0.0 = unrelated, 1.0 = identical).
    fn similarity(&self, a: &str, b: &str) -> f64;
}

/// A stored template entry with usage metadata.
#[derive(Debug, Clone)]
pub struct TemplateEntry {
    /// The stored learned pattern.
    pub pattern: LearnedPattern,
    /// Number of times this template has been retrieved/applied.
    pub usage_count: u32,
    /// Timestamp of the most recent use, for LRU-style eviction.
    pub last_used: u64,
}

/// Library of reusable LearnedPatterns with similarity search and usage tracking.
/// INS-L2: Optional `embedding_store` enables real semantic similarity search.
#[derive(Default)]
pub struct TemplateLibrary {
    entries: HashMap<String, TemplateEntry>,
    version: u32,
    /// INS-L2: Optional embedding backend for semantic similarity.
    embedding_store: Option<Arc<dyn EmbeddingStore>>,
}

impl std::fmt::Debug for TemplateLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateLibrary")
            .field("entries_count", &self.entries.len())
            .field("version", &self.version)
            .field("has_embedding_store", &self.embedding_store.is_some())
            .finish()
    }
}

impl TemplateLibrary {
    /// Create a new empty TemplateLibrary.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            version: 0,
            embedding_store: None,
        }
    }

    /// INS-L2: Attach a semantic embedding backend (builder-style).
    pub fn with_embedding_store(mut self, store: Arc<dyn EmbeddingStore>) -> Self {
        self.embedding_store = Some(store);
        self
    }

    /// INS-L2: Find patterns using embedding similarity when available, falling
    /// back to Jaccard tag overlap when no embedding store is configured.
    ///
    /// Returns patterns whose similarity to `query` exceeds `threshold`.
    pub fn find_similar_by_embedding(&self, query: &str, threshold: f64) -> Vec<LearnedPattern> {
        if let Some(ref store) = self.embedding_store {
            self.entries
                .values()
                .filter(|e| {
                    let sim = store.similarity(query, &e.pattern.description);
                    sim >= threshold
                })
                .map(|e| e.pattern.clone())
                .collect()
        } else {
            // Fallback: Jaccard on tags vs query tokens (split by whitespace).
            let query_tags: Vec<String> =
                query.split_whitespace().map(|s| s.to_lowercase()).collect();
            let query_set: std::collections::HashSet<&str> =
                query_tags.iter().map(String::as_str).collect();
            self.entries
                .values()
                .filter(|e| {
                    let sim = jaccard_similarity(&query_set, &e.pattern.tags);
                    sim >= threshold
                })
                .map(|e| e.pattern.clone())
                .collect()
        }
    }

    /// Record a pattern. Deduplicates by pattern_id; increments usage_count if exists.
    pub fn record_template(&mut self, pattern: LearnedPattern) {
        let id = pattern.pattern_id.clone();
        match self.entries.get_mut(&id) {
            Some(entry) => {
                entry.usage_count += 1;
            }
            None => {
                self.entries.insert(
                    id,
                    TemplateEntry {
                        pattern,
                        usage_count: 1,
                        last_used: 0,
                    },
                );
                self.version += 1;
            }
        }
    }

    /// Find patterns by domain and/or tag intersection.
    /// Returns patterns where domain matches AND tag overlap score >= 0.5.
    pub fn find_similar(&self, domain: &str, tags: &[String]) -> Vec<&LearnedPattern> {
        self.entries
            .values()
            .filter(|e| {
                e.pattern.domain == domain && {
                    if tags.is_empty() {
                        true
                    } else {
                        let matches = tags.iter().filter(|t| e.pattern.tags.contains(t)).count();
                        matches as f64 / tags.len() as f64 >= 0.5
                    }
                }
            })
            .map(|e| &e.pattern)
            .collect()
    }

    /// Return the top-k patterns sorted by usage_count descending.
    pub fn top_k(&self, k: usize) -> Vec<&LearnedPattern> {
        let mut sorted: Vec<&TemplateEntry> = self.entries.values().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.usage_count));
        sorted.into_iter().take(k).map(|e| &e.pattern).collect()
    }

    /// Number of stored templates.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no templates stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Current library version (bumped on every new insert).
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Evict patterns with usage_count <= threshold. Returns count evicted.
    pub fn evict_stale(&mut self, min_usage: u32) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, e| e.usage_count > min_usage);
        let evicted = before - self.entries.len();
        if evicted > 0 {
            self.version += 1;
        }
        evicted
    }

    /// Get a pattern by exact pattern_id.
    pub fn get(&self, pattern_id: &str) -> Option<&LearnedPattern> {
        self.entries.get(pattern_id).map(|e| &e.pattern)
    }

    /// Get usage count for a pattern_id.
    pub fn usage_count(&self, pattern_id: &str) -> u32 {
        self.entries
            .get(pattern_id)
            .map(|e| e.usage_count)
            .unwrap_or(0)
    }

    /// IL-3: Find semantically similar patterns using Jaccard similarity on tags.
    ///
    /// Computes the Jaccard similarity coefficient between `query_tags` and
    /// each pattern's tags, then returns the top-`top_k` matches sorted by
    /// similarity descending. Patterns with zero tag overlap are excluded.
    ///
    /// When `query_tags` is empty, falls back to `top_k` by usage count.
    ///
    /// # Arguments
    /// * `query_domain` — if `Some(d)`, only patterns in domain `d` are considered.
    /// * `query_tags`   — tags to compare against (Jaccard similarity).
    /// * `top_k`        — maximum number of results to return.
    pub fn find_similar_semantic(
        &self,
        query_domain: Option<&str>,
        query_tags: &[String],
        top_k: usize,
    ) -> Vec<TemplateMatch> {
        if top_k == 0 {
            return Vec::new();
        }

        // Precompute query tag set for O(1) lookups.
        let query_set: std::collections::HashSet<&str> =
            query_tags.iter().map(String::as_str).collect();

        let mut scored: Vec<TemplateMatch> = self
            .entries
            .values()
            .filter(|e| query_domain.map(|d| e.pattern.domain == d).unwrap_or(true))
            .filter_map(|e| {
                let similarity = if query_set.is_empty() {
                    // No query tags: use normalised usage count as proxy.
                    e.usage_count as f64 / (e.usage_count as f64 + 1.0)
                } else {
                    jaccard_similarity(&query_set, &e.pattern.tags)
                };
                if similarity > 0.0 {
                    Some(TemplateMatch {
                        pattern: e.pattern.clone(),
                        similarity,
                    })
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);
        scored
    }
}

/// IL-3: Result of a semantic similarity search.
#[derive(Debug, Clone)]
pub struct TemplateMatch {
    /// The matched pattern.
    pub pattern: LearnedPattern,
    /// Jaccard similarity score ∈ (0.0, 1.0].
    pub similarity: f64,
}

/// Compute Jaccard similarity between a query tag set and a pattern's tag list.
fn jaccard_similarity(query: &std::collections::HashSet<&str>, pattern_tags: &[String]) -> f64 {
    if query.is_empty() && pattern_tags.is_empty() {
        return 1.0;
    }
    if query.is_empty() || pattern_tags.is_empty() {
        return 0.0;
    }
    let pat_set: std::collections::HashSet<&str> =
        pattern_tags.iter().map(String::as_str).collect();
    let intersection = query.iter().filter(|t| pat_set.contains(*t)).count();
    let union = query.len() + pat_set.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pattern(id: &str, domain: &str, tags: &[&str]) -> LearnedPattern {
        LearnedPattern {
            pattern_id: id.to_string(),
            description: format!("desc-{id}"),
            generator_template: format!("tpl-{id}"),
            domain: domain.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_record_template_new() {
        let mut lib = TemplateLibrary::new();
        lib.record_template(make_pattern("p1", "rust", &["refactor"]));
        assert_eq!(lib.len(), 1);
        assert_eq!(lib.usage_count("p1"), 1);
    }

    #[test]
    fn test_record_template_dedup_increments_usage() {
        let mut lib = TemplateLibrary::new();
        lib.record_template(make_pattern("p1", "rust", &[]));
        lib.record_template(make_pattern("p1", "rust", &[]));
        lib.record_template(make_pattern("p1", "rust", &[]));
        assert_eq!(lib.len(), 1);
        assert_eq!(lib.usage_count("p1"), 3);
    }

    #[test]
    fn test_find_similar_by_domain() {
        let mut lib = TemplateLibrary::new();
        lib.record_template(make_pattern("p1", "rust", &[]));
        lib.record_template(make_pattern("p2", "python", &[]));
        let found = lib.find_similar("rust", &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].pattern_id, "p1");
    }

    #[test]
    fn test_find_similar_by_tags() {
        let mut lib = TemplateLibrary::new();
        lib.record_template(make_pattern("p1", "rust", &["a", "b", "c"]));
        lib.record_template(make_pattern("p2", "rust", &["x", "y"]));
        // query: domain=rust, tags=[a, b] -> p1 has 2/2 = 1.0 >= 0.5 ✓, p2 has 0/2 = 0.0 < 0.5 ✗
        let found = lib.find_similar("rust", &["a".to_string(), "b".to_string()]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].pattern_id, "p1");
    }

    #[test]
    fn test_top_k_returns_most_used() {
        let mut lib = TemplateLibrary::new();
        lib.record_template(make_pattern("p1", "rust", &[]));
        lib.record_template(make_pattern("p2", "rust", &[]));
        lib.record_template(make_pattern("p2", "rust", &[])); // p2 usage=2
        lib.record_template(make_pattern("p3", "rust", &[]));
        lib.record_template(make_pattern("p3", "rust", &[]));
        lib.record_template(make_pattern("p3", "rust", &[])); // p3 usage=3
        let top = lib.top_k(2);
        assert_eq!(top.len(), 2);
        // top[0] must be p3 (usage=3), top[1] must be p2 (usage=2)
        assert_eq!(top[0].pattern_id, "p3");
        assert_eq!(top[1].pattern_id, "p2");
    }

    #[test]
    fn test_top_k_empty_library() {
        let lib = TemplateLibrary::new();
        assert_eq!(lib.top_k(5).len(), 0);
    }

    #[test]
    fn test_version_bump_on_new_insert() {
        let mut lib = TemplateLibrary::new();
        assert_eq!(lib.version(), 0);
        lib.record_template(make_pattern("p1", "rust", &[]));
        assert_eq!(lib.version(), 1);
        // dedup does NOT bump version
        lib.record_template(make_pattern("p1", "rust", &[]));
        assert_eq!(lib.version(), 1);
        lib.record_template(make_pattern("p2", "rust", &[]));
        assert_eq!(lib.version(), 2);
    }

    #[test]
    fn test_evict_stale_removes_low_usage() {
        let mut lib = TemplateLibrary::new();
        lib.record_template(make_pattern("p1", "rust", &[])); // usage=1
        lib.record_template(make_pattern("p2", "rust", &[])); // usage=1
        lib.record_template(make_pattern("p2", "rust", &[])); // usage=2
        lib.record_template(make_pattern("p3", "rust", &[])); // usage=1
        // evict usage <= 1
        let evicted = lib.evict_stale(1);
        assert_eq!(evicted, 2); // p1 and p3 evicted
        assert_eq!(lib.len(), 1);
        assert!(lib.get("p2").is_some());
    }
}
