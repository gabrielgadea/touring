//! Semantic symbol search with cosine similarity.
//!
//! Provides [`SemanticSymbolIndex`] for indexing symbols and finding
//! semantically similar symbols based on fixed-dimensional feature vectors.
//!
//! This module is self-contained and does not depend on `touring-learning` to
//! avoid a circular dependency (`touring-learning` optionally depends on
//! `touring-ast` via the `ast-features` feature flag).
//!
//! # LRU Cache
//!
//! Symbols are stored in an internal LRU cache. When capacity is exceeded, the
//! least-recently-inserted entry is evicted (insertion order, not access order,
//! to keep the implementation lightweight).
//!
//! # Feature Vector (16 dimensions)
//!
//! ```text
//! [0]   name_len:         normalized name length (0.0–1.0, capped at 64)
//! [1]   line_count:       normalized line span (0.0–1.0, capped at 200)
//! [2]   complexity:       normalized complexity (0.0–1.0, capped at 20)
//! [3]   is_async:         1.0 if async, 0.0 otherwise
//! [4]   is_public:        1.0 if public, 0.0 otherwise
//! [5]   has_docstring:    1.0 if docstring present, 0.0 otherwise
//! [6]   has_parent:       1.0 if method (has parent_name), 0.0 otherwise
//! [7]   decorator_count:  normalized decorator count (0.0–1.0, capped at 5)
//! [8..15] kind one-hot:   Function, AsyncFunction, Method, Class,
//!                          Struct, Enum, Trait, Other (8 buckets)
//! Total: 16
//! ```

use moka::sync::Cache;
#[cfg(feature = "simd-search")]
use touring_simd::CosineSimilarity;

use crate::ast::symbols::{Symbol, SymbolKind};

/// Dimensionality of the symbol embedding.
pub const EMBED_DIM: usize = 16;

/// Internal entry: embedding vector for a symbol.
#[derive(Debug, Clone)]
struct Entry {
    embedding: Vec<f32>,
}

/// Semantic symbol index with SIMD-friendly cosine similarity search.
///
/// Symbols are embedded into a 16-dimensional feature vector and stored
/// in an LRU cache (backed by `moka::sync::Cache`). When capacity is exceeded,
/// the least-recently-indexed entry is evicted. Re-indexing a symbol via
/// `index_symbol` refreshes its recency, preventing premature eviction.
///
/// This is deliberately self-contained — it does not use `touring-learning`
/// to avoid a circular crate dependency.
///
/// # Example
///
/// ```rust,no_run
/// use touring_code::ast::semantic_search::SemanticSymbolIndex;
/// use touring_code::ast::{Symbol, SymbolKind};
///
/// let mut index = SemanticSymbolIndex::new(256);
/// let sym = Symbol::new("compute_reward", SymbolKind::Function, 1, 10, 0, 0, 100, "fn compute_reward() -> f64", true);
/// index.index_symbol(&sym);
///
/// let query = Symbol::new("compute_score", SymbolKind::Function, 20, 30, 0, 100, 200, "fn compute_score() -> f64", true);
/// let similar = index.find_similar_symbols(&query, 0.5, 5);
/// println!("{:?}", similar);
/// ```
pub struct SemanticSymbolIndex {
    entries: Cache<String, Entry>,
}

impl SemanticSymbolIndex {
    /// Create a new index with the given LRU capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Cache::builder()
                .max_capacity(capacity.max(1) as u64)
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
        }
    }

    /// Index a symbol: embed it and store in the LRU cache.
    ///
    /// If the capacity is exceeded, the least-recently-indexed entry is evicted.
    /// Re-indexing an existing symbol refreshes its recency (true LRU, not FIFO).
    ///
    /// When `simd-search` is enabled, the embedding is pre-normalized to unit length
    /// so that dot product equals cosine similarity — enabling `find_similar_batch`
    /// to skip per-query normalization of candidates.
    pub fn index_symbol(&mut self, sym: &Symbol) {
        let embedding = Self::embed_symbol(sym);

        // Pre-normalize to unit vector so dot(query, embedding) == cosine(query, embedding).
        // This amortizes the normalization cost across all future queries.
        #[cfg(feature = "simd-search")]
        let embedding = touring_simd::similarity::normalized(&embedding);

        // Cache::insert handles insert and update; moves to MRU on every call.
        // Automatically evicts the LRU entry when at capacity.
        self.entries.insert(sym.name.clone(), Entry { embedding });
        // Flush pending writes so entry_count() and iter() are consistent.
        self.entries.run_pending_tasks();
    }

    /// Find symbols similar to the query, at or above `threshold` cosine similarity.
    ///
    /// Returns up to `limit` results as `(symbol_name, similarity)` pairs,
    /// sorted by descending similarity.
    ///
    /// When the `simd-search` feature is enabled, uses [`touring_simd::TopKSearcher`]
    /// for O(n log k) partial selection instead of a full O(n log n) sort.
    pub fn find_similar_symbols(
        &self,
        query: &Symbol,
        threshold: f64,
        limit: usize,
    ) -> Vec<(String, f64)> {
        self.entries.run_pending_tasks();
        if self.entries.entry_count() == 0 || limit == 0 {
            return vec![];
        }

        let query_emb = Self::embed_symbol(query);

        // Early exit for zero-vector query (no symbol can match).
        if !query_emb.iter().any(|&x| x.abs() > f32::EPSILON) {
            return vec![];
        }

        #[cfg(feature = "simd-search")]
        {
            self.find_similar_symbols_topk(&query_emb, threshold, limit)
        }

        #[cfg(not(feature = "simd-search"))]
        {
            self.find_similar_symbols_fallback(&query_emb, threshold, limit)
        }
    }

    /// SIMD-accelerated top-K search via `TopKSearcher`.
    ///
    /// Complexity: O(n log k) where k = limit, vs O(n log n) for full sort.
    #[cfg(feature = "simd-search")]
    fn find_similar_symbols_topk(
        &self,
        query_emb: &[f32],
        threshold: f64,
        limit: usize,
    ) -> Vec<(String, f64)> {
        // Collect keys and embeddings in parallel arrays.
        // Skip entries with empty embeddings.
        let mut keys: Vec<String> = Vec::with_capacity(self.entries.entry_count() as usize);
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(self.entries.entry_count() as usize);

        for (k, entry) in self.entries.iter() {
            if !entry.embedding.is_empty() {
                keys.push((*k).clone());
                embeddings.push(entry.embedding.clone());
            }
        }

        if embeddings.is_empty() {
            return Vec::new();
        }

        // Request more than `limit` from TopK to account for threshold filtering.
        // At most we need all candidates (if many are above threshold).
        let searcher = touring_simd::TopKSearcher::new(limit);
        let top_results = searcher.top_k(query_emb, &embeddings, limit);

        top_results
            .into_iter()
            .filter(|r| r.score >= threshold)
            .filter_map(|r| keys.get(r.index).map(|k| (k.clone(), r.score)))
            .collect()
    }

    /// Batch dot-product search over all indexed embeddings using rayon parallelism.
    ///
    /// Because `index_symbol` pre-normalizes embeddings to unit length, `dot(q, e)`
    /// is equivalent to `cosine(q, e)` — eliminating per-candidate normalization.
    ///
    /// Uses [`touring_simd::learning::batch_dot_products_par`] for SIMD-accelerated,
    /// rayon-parallel scoring of all candidates in a single pass.
    ///
    /// # Arguments
    /// * `query_emb` — Raw (not necessarily normalized) query embedding.
    /// * `threshold` — Minimum similarity score to include in results.
    /// * `limit` — Maximum number of results to return.
    ///
    /// # Returns
    /// Up to `limit` results as `(symbol_name, similarity)` pairs, sorted descending.
    /// SIMD-accelerated batch search via CosineComputer.
    ///
    /// Uses `CosineComputer.cosine_batch` which dispatches to GPU backend when available,
    /// falling back to CPU SIMD (pulp AVX-512/AVX2+FMA/NEON) when not.
    ///
    /// Because `index_symbol` pre-normalizes embeddings to unit length, `cosine(q, e)`
    /// equals `dot(q, e)` — eliminating per-candidate normalization.
    #[cfg(feature = "simd-search")]
    pub fn find_similar_batch(
        &self,
        query_emb: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Vec<(String, f32)> {
        self.entries.run_pending_tasks();
        if self.entries.entry_count() == 0 || limit == 0 {
            return Vec::new();
        }

        // Normalize query so dot(query_norm, unit_embedding) == cosine similarity.
        let query_norm = touring_simd::similarity::normalized(query_emb);

        // Collect keys and embeddings into parallel arrays.
        let mut keys: Vec<String> = Vec::with_capacity(self.entries.entry_count() as usize);
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(self.entries.entry_count() as usize);

        for (k, entry) in self.entries.iter() {
            if !entry.embedding.is_empty() {
                keys.push((*k).clone());
                embeddings.push(entry.embedding.clone());
            }
        }

        if embeddings.is_empty() {
            return Vec::new();
        }

        // Direct CosineComputer with SIMD dispatch (GPU when backend available,
        // CPU SIMD via pulp otherwise). cosine_batch uses CosineSinglePass
        // for 3x fewer cache misses vs separate dot + norm passes.
        let computer = touring_simd::CosineComputer::new();
        let scores_f64 = computer.cosine_batch(&[query_norm], &embeddings);

        // Filter by threshold, then partial-select top-K.
        let mut pairs: Vec<(usize, f32)> = scores_f64
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s >= threshold as f64)
            .map(|(i, &s)| (i, s as f32))
            .collect();

        if pairs.len() > limit {
            pairs.select_nth_unstable_by(limit, |a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            });
            pairs.truncate(limit);
        }
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        pairs
            .into_iter()
            .filter_map(|(i, s)| keys.get(i).map(|k| (k.clone(), s)))
            .collect()
    }

    /// Fallback search using manual cosine similarity + full sort.
    #[cfg(not(feature = "simd-search"))]
    fn find_similar_symbols_fallback(
        &self,
        query_emb: &[f32],
        threshold: f64,
        limit: usize,
    ) -> Vec<(String, f64)> {
        let query_norm = l2_norm(query_emb);

        let mut results: Vec<(String, f64)> = self
            .entries
            .iter()
            .filter_map(|(k, entry)| {
                if entry.embedding.is_empty() {
                    return None;
                }
                let sim = compute_similarity(query_emb, &entry.embedding, query_norm);
                if sim >= threshold {
                    Some(((*k).clone(), sim))
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    /// Embed a [`Symbol`] into a fixed 16-dimensional f32 feature vector.
    ///
    /// All dimensions are normalized to [0.0, 1.0] for stable cosine similarity.
    #[allow(clippy::indexing_slicing)] // Vec is created with EMBED_DIM=16; all indices 0..=15 are within bounds.
    pub fn embed_symbol(sym: &Symbol) -> Vec<f32> {
        let mut v = vec![0.0f32; EMBED_DIM];

        // [0] name length (0..64 → 0.0..1.0)
        v[0] = (sym.name.len() as f32 / 64.0).min(1.0);

        // [1] line span (0..200 → 0.0..1.0)
        let line_span = sym.end_line.saturating_sub(sym.line) as f32;
        v[1] = (line_span / 200.0).min(1.0);

        // [2] complexity (0..20 → 0.0..1.0)
        let complexity = sym.complexity.unwrap_or(1) as f32;
        v[2] = (complexity / 20.0).min(1.0);

        // [3] is_async
        v[3] = if sym.is_async { 1.0 } else { 0.0 };

        // [4] is_public
        v[4] = if sym.is_public { 1.0 } else { 0.0 };

        // [5] has docstring
        v[5] = if sym.docstring.is_some() { 1.0 } else { 0.0 };

        // [6] has parent (is a method)
        v[6] = if sym.parent_name.is_some() { 1.0 } else { 0.0 };

        // [7] decorator count (0..5 → 0.0..1.0)
        v[7] = (sym.decorators.len() as f32 / 5.0).min(1.0);

        // [8..15] kind one-hot (8 buckets)
        let kind_idx: usize = match &sym.kind {
            SymbolKind::Function => 8,
            SymbolKind::AsyncFunction => 9,
            SymbolKind::Method => 10,
            SymbolKind::Class => 11,
            SymbolKind::Struct => 12,
            SymbolKind::Enum => 13,
            SymbolKind::Trait => 14,
            _ => 15, // All other kinds
        };
        v[kind_idx] = 1.0;

        v
    }

    /// Number of indexed symbols.
    pub fn len(&self) -> usize {
        self.entries.run_pending_tasks();
        self.entries.entry_count() as usize
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.run_pending_tasks();
        self.entries.entry_count() == 0
    }

    /// Clear all indexed symbols.
    pub fn clear(&mut self) {
        self.entries.invalidate_all();
        self.entries.run_pending_tasks();
    }

    /// AST-2: Fused symbol search combining similarity, co-edit, and blast radius
    /// signals via Reciprocal Rank Fusion (RRF).
    ///
    /// RRF formula: `score(d) = Σ 1/(k + rank_i(d))` across all ranking functions.
    /// `k` is typically 60.0 (balances contribution of each signal).
    pub fn find_symbols_fused(
        &self,
        query: &Symbol,
        co_edit_scores: &std::collections::HashMap<String, f64>,
        blast_radius_scores: &std::collections::HashMap<String, f64>,
        k: f64,
        limit: usize,
    ) -> Vec<(String, f64)> {
        self.entries.run_pending_tasks();
        if self.entries.entry_count() == 0 {
            return Vec::new();
        }

        // Signal 1: similarity ranking (use threshold=0.0 to get all)
        let sim_ranked = self.find_similar_symbols(query, 0.0, self.entries.entry_count() as usize);

        // Signal 2: co-edit ranking (sort by co_edit_scores descending)
        let mut co_edit_ranked: Vec<String> = sim_ranked.iter().map(|(n, _)| n.clone()).collect();
        co_edit_ranked.sort_by(|a, b| {
            let sa = co_edit_scores.get(b).copied().unwrap_or(0.0);
            let sb = co_edit_scores.get(a).copied().unwrap_or(0.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Signal 3: blast radius ranking
        let mut br_ranked: Vec<String> = sim_ranked.iter().map(|(n, _)| n.clone()).collect();
        br_ranked.sort_by(|a, b| {
            let sa = blast_radius_scores.get(b).copied().unwrap_or(0.0);
            let sb = blast_radius_scores.get(a).copied().unwrap_or(0.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Compute RRF scores
        let mut rrf_scores: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();

        for (rank, (name, _)) in sim_ranked.iter().enumerate() {
            *rrf_scores.entry(name.clone()).or_insert(0.0) += 1.0 / (k + rank as f64 + 1.0);
        }
        for (rank, name) in co_edit_ranked.iter().enumerate() {
            *rrf_scores.entry(name.clone()).or_insert(0.0) += 1.0 / (k + rank as f64 + 1.0);
        }
        for (rank, name) in br_ranked.iter().enumerate() {
            *rrf_scores.entry(name.clone()).or_insert(0.0) += 1.0 / (k + rank as f64 + 1.0);
        }

        // Sort by RRF score descending, take limit
        let mut result: Vec<(String, f64)> = rrf_scores.into_iter().collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result.truncate(limit);
        result
    }
}

// ── Cosine similarity helpers ─────────────────────────────────────────────────

/// Compute cosine similarity between two embeddings using precomputed query norm.
///
/// Only used when `simd-search` is disabled; the SIMD path uses
/// [`touring_simd::TopKSearcher`] directly for O(n log k) top-K selection.
#[cfg(not(feature = "simd-search"))]
fn compute_similarity(a: &[f32], b: &[f32], a_norm: f32) -> f64 {
    cosine_similarity(a, b, a_norm)
}

/// Compute L2 norm of a vector.
#[cfg(not(feature = "simd-search"))]
fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Cosine similarity using a precomputed query norm.
#[cfg(not(feature = "simd-search"))]
fn cosine_similarity(a: &[f32], b: &[f32], a_norm: f32) -> f64 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let b_norm = l2_norm(b);
    if b_norm < f32::EPSILON || a_norm < f32::EPSILON {
        return 0.0;
    }
    (dot / (a_norm * b_norm)) as f64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::needless_range_loop)] // test vecs asserted non-empty; range used for index-based assertions
    use super::*;

    fn make_sym(name: &str, kind: SymbolKind, line: usize, end_line: usize) -> Symbol {
        Symbol::new(name, kind, line, end_line, 0, 0, 100, "fn test()", true)
    }

    #[test]
    fn test_index_and_len() {
        let mut idx = SemanticSymbolIndex::new(10);
        assert!(idx.is_empty());

        idx.index_symbol(&make_sym("foo", SymbolKind::Function, 1, 5));
        assert_eq!(idx.len(), 1);

        idx.index_symbol(&make_sym("bar", SymbolKind::Method, 10, 15));
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn test_find_similar_same_kind() {
        let mut idx = SemanticSymbolIndex::new(10);
        idx.index_symbol(&make_sym("compute_reward", SymbolKind::Function, 1, 10));

        // Same kind query should find the indexed symbol
        let query = make_sym("compute_score", SymbolKind::Function, 1, 10);
        let results = idx.find_similar_symbols(&query, 0.5, 5);
        assert!(
            !results.is_empty(),
            "Same-kind symbols should score above 0.5"
        );
        assert_eq!(results[0].0, "compute_reward");
    }

    #[test]
    fn test_find_similar_empty_index() {
        let idx = SemanticSymbolIndex::new(10);
        let sym = make_sym("foo", SymbolKind::Function, 1, 5);
        let results = idx.find_similar_symbols(&sym, 0.5, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_lru_eviction() {
        let mut idx = SemanticSymbolIndex::new(2);
        idx.index_symbol(&make_sym("alpha", SymbolKind::Function, 1, 5));
        idx.index_symbol(&make_sym("beta", SymbolKind::Function, 1, 5));
        idx.index_symbol(&make_sym("gamma", SymbolKind::Function, 1, 5)); // evicts "alpha"
        assert_eq!(idx.len(), 2);
        // alpha should be gone
        let query = make_sym("alpha", SymbolKind::Function, 1, 5);
        let res = idx.find_similar_symbols(&query, 0.99, 10);
        assert!(
            res.iter().all(|(k, _)| k != "alpha"),
            "alpha should have been evicted"
        );
    }

    #[test]
    fn test_clear() {
        let mut idx = SemanticSymbolIndex::new(10);
        idx.index_symbol(&make_sym("foo", SymbolKind::Function, 1, 5));
        idx.clear();
        assert!(idx.is_empty());
    }

    #[test]
    fn test_embed_dim() {
        let sym = make_sym("test_fn", SymbolKind::Function, 1, 20);
        let emb = SemanticSymbolIndex::embed_symbol(&sym);
        assert_eq!(emb.len(), EMBED_DIM);
        // All values in [0, 1]
        for val in &emb {
            assert!(
                *val >= 0.0 && *val <= 1.0,
                "Embedding value out of range: {}",
                val
            );
        }
    }

    #[test]
    fn test_kind_one_hot() {
        let kinds = [
            (SymbolKind::Function, 8usize),
            (SymbolKind::AsyncFunction, 9),
            (SymbolKind::Method, 10),
            (SymbolKind::Class, 11),
            (SymbolKind::Struct, 12),
            (SymbolKind::Enum, 13),
            (SymbolKind::Trait, 14),
        ];
        for (kind, expected_idx) in kinds {
            let sym = make_sym("x", kind, 1, 2);
            let emb = SemanticSymbolIndex::embed_symbol(&sym);
            assert_eq!(
                emb[expected_idx], 1.0,
                "kind one-hot at index {} should be 1.0",
                expected_idx
            );
            // All other kind buckets should be 0
            for idx in 8..16 {
                if idx != expected_idx {
                    assert_eq!(emb[idx], 0.0, "kind one-hot at {} should be 0.0", idx);
                }
            }
        }
    }

    #[test]
    fn test_dedup_insert() {
        let mut idx = SemanticSymbolIndex::new(10);
        idx.index_symbol(&make_sym("foo", SymbolKind::Function, 1, 5));
        idx.index_symbol(&make_sym("foo", SymbolKind::Method, 1, 5)); // same name, different kind
        // Should still have only 1 entry (dedup by name)
        assert_eq!(idx.len(), 1);
    }

    // ── AST-2: find_symbols_fused RRF tests ───────────────────

    #[test]
    fn test_fused_search_empty_index() {
        let idx = SemanticSymbolIndex::new(10);
        let query = make_sym("foo", SymbolKind::Function, 1, 5);
        let result = idx.find_symbols_fused(
            &query,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            60.0,
            10,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn test_fused_search_respects_limit() {
        let mut idx = SemanticSymbolIndex::new(100);
        for i in 0..10 {
            idx.index_symbol(&make_sym(
                &format!("sym_{i}"),
                SymbolKind::Function,
                i * 10,
                i * 10 + 5,
            ));
        }
        let query = make_sym("sym_0", SymbolKind::Function, 0, 5);
        let result = idx.find_symbols_fused(
            &query,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            60.0,
            3,
        );
        assert!(result.len() <= 3);
    }

    #[test]
    fn test_fused_search_combines_signals() {
        let mut idx = SemanticSymbolIndex::new(100);
        idx.index_symbol(&make_sym("alpha", SymbolKind::Function, 1, 10));
        idx.index_symbol(&make_sym("beta", SymbolKind::Function, 20, 30));

        let mut co_edits = std::collections::HashMap::new();
        co_edits.insert("beta".to_string(), 0.9);

        let query = make_sym("alpha", SymbolKind::Function, 1, 10);
        let result = idx.find_symbols_fused(
            &query,
            &co_edits,
            &std::collections::HashMap::new(),
            60.0,
            10,
        );
        // Both symbols should appear
        assert!(!result.is_empty());
        assert!(
            result.iter().any(|(n, _)| n.as_str() == "alpha"),
            "query 'alpha' should appear in fused results"
        );
    }

    #[test]
    fn test_fused_search_rrf_scores_positive() {
        let mut idx = SemanticSymbolIndex::new(100);
        idx.index_symbol(&make_sym("x", SymbolKind::Function, 1, 5));
        let query = make_sym("x", SymbolKind::Function, 1, 5);
        let result = idx.find_symbols_fused(
            &query,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            60.0,
            10,
        );
        for (_name, score) in &result {
            assert!(*score > 0.0, "RRF scores must be positive");
        }
    }

    #[test]
    fn test_fused_search_single_signal() {
        let mut idx = SemanticSymbolIndex::new(100);
        idx.index_symbol(&make_sym("only", SymbolKind::Method, 1, 5));
        let query = make_sym("only", SymbolKind::Method, 1, 5);
        let mut co_edits = std::collections::HashMap::new();
        co_edits.insert("only".to_string(), 1.0);
        let result = idx.find_symbols_fused(
            &query,
            &co_edits,
            &std::collections::HashMap::new(),
            60.0,
            10,
        );
        assert_eq!(result.len(), 1);
    }
}
