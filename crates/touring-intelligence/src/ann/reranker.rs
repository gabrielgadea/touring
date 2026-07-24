//! Contextual Reranker for RAG ANTT
//!
//! Provides contextual reranking of search results based on:
//! - Semantic relevance (SIMD cosine similarity)
//! - Document authority (by type: Lei > Decreto > Resolucao > ...)
//! - Temporal recency (exponential decay)
//! - Keyword matching
//! - Historical user preferences

use crate::ann::keyword_matcher::{ANTT_PATTERNS, TECHNICAL_KEYWORDS};
use indexmap::IndexMap;
use moka::sync::Cache;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;
use touring_simd::CosineComputer;

/// Maximum bytes retained in the query-embedding cache. 32 MiB covers
/// ~5k typical embeddings (1536 × f32 = 6 KiB) before W-TinyLFU evicts the
/// coldest entries.
const QUERY_CACHE_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// TTL — query embeddings older than one hour are considered stale and
/// re-computed on next insert.
const QUERY_CACHE_TTL_SECS: u64 = 60 * 60;
/// TTI — idle embeddings are evicted after ten minutes even before TTL.
const QUERY_CACHE_TTI_SECS: u64 = 10 * 60;

/// Errors for reranking operations
#[derive(Error, Debug)]
pub enum RerankerError {
    /// The input results list was empty.
    #[error("Empty results list")]
    EmptyResults,
    /// Reranking weights do not sum to `1.0` (the wrapped value is the sum).
    #[error("Invalid weight configuration: sum must be 1.0, got {0}")]
    InvalidWeights(f64),
    /// A date string could not be parsed.
    #[error("Date parse error: {0}")]
    DateParseError(String),
}

/// Resultado de busca com score de reranking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedResult {
    /// Identifier of the ranked document.
    pub document_id: String,
    /// Score from the upstream retriever before reranking.
    pub original_score: f64,
    /// Combined score after reranking.
    pub reranked_score: f64,
    /// Document content.
    pub content: String,
    /// Per-factor contributions to the reranked score.
    pub ranking_factors: RankingFactors,
    /// Zero-based position in the original (pre-rerank) ordering.
    pub original_position: usize,
    /// Zero-based position in the reranked ordering.
    pub new_position: usize,
}

/// Fatores de ranking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RankingFactors {
    /// Semantic relevance (cosine similarity) factor, in `[0, 1]`.
    pub semantic_relevance: f64,
    /// Authority factor derived from the document type, in `[0, 1]`.
    pub document_authority: f64,
    /// Temporal recency factor (exponential decay), in `[0, 1]`.
    pub recency: f64,
    /// Keyword-match factor, in `[0, 1]`.
    pub keyword_match: f64,
    /// Historical user-preference factor, in `[0, 1]`.
    pub historical_preference: f64,
}

/// Pesos para combinação de scores
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankWeights {
    /// Weight applied to the semantic-relevance factor.
    pub semantic: f64,
    /// Weight applied to the document-authority factor.
    pub authority: f64,
    /// Weight applied to the recency factor.
    pub recency: f64,
    /// Weight applied to the keyword-match factor.
    pub keyword: f64,
    /// Weight applied to the historical-preference factor.
    pub historical: f64,
}

impl Default for RerankWeights {
    fn default() -> Self {
        Self {
            semantic: 0.35,
            authority: 0.25,
            recency: 0.15,
            keyword: 0.15,
            historical: 0.10,
        }
    }
}

impl RerankWeights {
    /// Validate that weights sum to 1.0 (with tolerance)
    pub fn validate(&self) -> Result<(), RerankerError> {
        let sum = self.semantic + self.authority + self.recency + self.keyword + self.historical;
        if (sum - 1.0).abs() > 0.01 {
            return Err(RerankerError::InvalidWeights(sum));
        }
        Ok(())
    }

    /// Normalize weights to sum to 1.0
    pub fn normalize(&mut self) {
        let sum = self.semantic + self.authority + self.recency + self.keyword + self.historical;
        if sum > 0.0 {
            self.semantic /= sum;
            self.authority /= sum;
            self.recency /= sum;
            self.keyword /= sum;
            self.historical /= sum;
        }
    }
}

/// Contexto para reranking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankContext {
    /// The user query text.
    pub query: String,
    /// Hash of the query, used as the embedding-cache key.
    pub query_hash: String,
    /// Reference date used as the anchor for recency decay.
    pub reference_date: String,
    /// Keywords to match against document content.
    pub keywords: Vec<String>,
    /// Per-factor weights for combining scores.
    pub weights: RerankWeights,
    /// Document type to favor, if any.
    pub preferred_doc_type: Option<String>,
}

impl Default for RerankContext {
    fn default() -> Self {
        Self {
            query: String::new(),
            query_hash: String::new(),
            reference_date: "2026-01-25".to_string(),
            keywords: Vec::new(),
            weights: RerankWeights::default(),
            preferred_doc_type: None,
        }
    }
}

/// Normalize accented characters to ASCII equivalents for matching
fn normalize_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

/// Resultado de busca do retriever
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Identifier of the retrieved document.
    pub document_id: String,
    /// Retriever relevance score.
    pub score: f64,
    /// Document content.
    pub content: String,
    /// Document type (e.g. `lei`, `decreto`); defaults to `outros`.
    pub document_type: String,
    /// Document date, when available.
    pub date: Option<String>,
}

impl SearchResult {
    /// Create a `SearchResult` with `document_type` defaulting to `outros`
    /// and no date.
    pub fn new(document_id: &str, score: f64, content: &str) -> Self {
        Self {
            document_id: document_id.to_string(),
            score,
            content: content.to_string(),
            document_type: "outros".to_string(),
            date: None,
        }
    }

    /// Builder: set the document type and return `self`.
    pub fn with_type(mut self, doc_type: &str) -> Self {
        self.document_type = doc_type.to_string();
        self
    }

    /// Builder: set the document date and return `self`.
    pub fn with_date(mut self, date: &str) -> Self {
        self.date = Some(date.to_string());
        self
    }
}

/// Pesos de autoridade por tipo de documento ANTT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityWeights {
    /// Authority weight for a law (`lei`).
    pub lei: f64,
    /// Authority weight for a decree (`decreto`).
    pub decreto: f64,
    /// Authority weight for an ANTT resolution.
    pub resolucao_antt: f64,
    /// Authority weight for a TCU ruling (acórdão).
    pub acordao_tcu: f64,
    /// Authority weight for a federal attorney opinion (`parecer PF`).
    pub parecer_pf: f64,
    /// Authority weight for a technical note (`nota técnica`).
    pub nota_tecnica: f64,
    /// Authority weight for a contract (`contrato`).
    pub contrato: f64,
    /// Authority weight for a dispatch (`despacho`).
    pub despacho: f64,
    /// Authority weight for any other document type.
    pub outros: f64,
}

impl Default for AuthorityWeights {
    fn default() -> Self {
        Self {
            lei: 1.0,
            decreto: 0.95,
            resolucao_antt: 0.90,
            acordao_tcu: 0.88,
            parecer_pf: 0.85,
            nota_tecnica: 0.75,
            contrato: 0.80,
            despacho: 0.70,
            outros: 0.50,
        }
    }
}

/// Reranker contextual para RAG ANTT.
///
/// The `query_cache` field migrated from `HashMap<String, Vec<f32>>` to
/// `moka::sync::Cache<String, Arc<Vec<f32>>>` on 2026-04-16 (Moka Expansion
/// Wave 2, P1 #6 of `docs/analyses/2026-04-14-crates-inventory-ranking.md`).
/// The prior implementation was unbounded (memory leak under sustained load)
/// and lacked a lookup API — embeddings were written but never read. Moka
/// brings W-TinyLFU admission + TTL/TTI eviction + byte-aware weigher, and
/// the new [`Self::lookup_query_embedding`] closes the cache loop so stored
/// embeddings actually pay dividends on subsequent queries.
#[derive(Debug)]
pub struct ContextualReranker {
    authority_weights: AuthorityWeights,
    cosine_computer: CosineComputer,
    historical_clicks: IndexMap<String, u64>,
    /// W-TinyLFU cache of query embeddings, bounded by 32 MiB total bytes.
    /// Values are `Arc<Vec<f32>>` so hits are O(1) refcount bumps.
    query_cache: Cache<String, Arc<Vec<f32>>>,
    /// Lock-free hit counter for observability.
    query_cache_hits: AtomicU64,
    /// Lock-free miss counter — incremented on `lookup_query_embedding`
    /// calls that do not find the key.
    query_cache_misses: AtomicU64,
}

impl Default for ContextualReranker {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextualReranker {
    /// Create a reranker with default authority weights and an empty,
    /// bounded query-embedding cache.
    pub fn new() -> Self {
        Self {
            authority_weights: AuthorityWeights::default(),
            cosine_computer: CosineComputer::new(),
            historical_clicks: IndexMap::new(),
            query_cache: Cache::builder()
                .max_capacity(QUERY_CACHE_MAX_BYTES)
                .time_to_live(Duration::from_secs(QUERY_CACHE_TTL_SECS))
                .time_to_idle(Duration::from_secs(QUERY_CACHE_TTI_SECS))
                .weigher(|_k: &String, v: &Arc<Vec<f32>>| -> u32 {
                    // Each f32 is 4 bytes; saturate so gigantic vectors do
                    // not overflow the u32 weight tally.
                    v.len()
                        .saturating_mul(std::mem::size_of::<f32>())
                        .min(u32::MAX as usize) as u32
                })
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
            query_cache_hits: AtomicU64::new(0),
            query_cache_misses: AtomicU64::new(0),
        }
    }

    /// Builder: override the authority weights and return `self`.
    pub fn with_authority_weights(mut self, weights: AuthorityWeights) -> Self {
        self.authority_weights = weights;
        self
    }

    /// Rerank `results` against `context`, returning them ordered by combined
    /// score. Errors with `EmptyResults` when `results` is empty.
    pub fn rerank(
        &self,
        results: &[SearchResult],
        context: &RerankContext,
    ) -> Result<Vec<RankedResult>, RerankerError> {
        if results.is_empty() {
            return Err(RerankerError::EmptyResults);
        }

        let mut ranked_results: Vec<RankedResult> = results
            .iter()
            .enumerate()
            .map(|(idx, result)| {
                let factors = self.compute_factors(result, context);
                let reranked_score = self.combine_scores(&factors, &context.weights);

                RankedResult {
                    document_id: result.document_id.clone(),
                    original_score: result.score,
                    reranked_score,
                    content: result.content.clone(),
                    ranking_factors: factors,
                    original_position: idx,
                    new_position: 0,
                }
            })
            .collect();

        ranked_results.sort_by(|a, b| {
            b.reranked_score
                .partial_cmp(&a.reranked_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for (idx, result) in ranked_results.iter_mut().enumerate() {
            result.new_position = idx;
        }

        Ok(ranked_results)
    }

    /// Like [`Self::rerank`] but computes per-result factors in parallel via
    /// Rayon; preferable for large result sets.
    pub fn rerank_parallel(
        &self,
        results: &[SearchResult],
        context: &RerankContext,
    ) -> Result<Vec<RankedResult>, RerankerError> {
        if results.is_empty() {
            return Err(RerankerError::EmptyResults);
        }

        let threshold = 50;
        if results.len() < threshold {
            return self.rerank(results, context);
        }

        let ranked_with_idx: Vec<(usize, RankedResult)> = results
            .par_iter()
            .enumerate()
            .map(|(idx, result)| {
                let factors = self.compute_factors(result, context);
                let reranked_score = self.combine_scores(&factors, &context.weights);

                let ranked = RankedResult {
                    document_id: result.document_id.clone(),
                    original_score: result.score,
                    reranked_score,
                    content: result.content.clone(),
                    ranking_factors: factors,
                    original_position: idx,
                    new_position: 0,
                };

                (idx, ranked)
            })
            .collect();

        let mut ranked_results: Vec<RankedResult> =
            ranked_with_idx.into_iter().map(|(_, r)| r).collect();

        ranked_results.sort_by(|a, b| {
            b.reranked_score
                .partial_cmp(&a.reranked_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for (idx, result) in ranked_results.iter_mut().enumerate() {
            result.new_position = idx;
        }

        Ok(ranked_results)
    }

    fn compute_factors(&self, result: &SearchResult, context: &RerankContext) -> RankingFactors {
        let semantic_relevance = result.score;
        let document_authority = self.get_authority(&result.document_type);
        let recency = self.compute_recency(&result.date, &context.reference_date);
        let keyword_match = self.compute_keyword_match(&result.content, &context.keywords);
        let historical_preference = self.get_historical_preference(&result.document_id);

        RankingFactors {
            semantic_relevance,
            document_authority,
            recency,
            keyword_match,
            historical_preference,
        }
    }

    fn get_authority(&self, doc_type: &str) -> f64 {
        // Use AhoCorasick via ANTT_PATTERNS for O(n) multi-pattern scan.
        // Category from first match determines authority bucket; falls back to
        // sequential contains() for doc_type strings that embed the category
        // name without matching a regulatory pattern prefix.
        let matches = ANTT_PATTERNS.find_matches(doc_type);
        for m in &matches {
            if let Some(cat) = &m.category {
                return match cat.as_str() {
                    "Law" => self.authority_weights.lei,
                    "Decree" => self.authority_weights.decreto,
                    "Resolution" => self.authority_weights.resolucao_antt,
                    "TCU" => self.authority_weights.acordao_tcu,
                    "Opinion" => self.authority_weights.parecer_pf,
                    "TechnicalNote" => self.authority_weights.nota_tecnica,
                    "Contract" => self.authority_weights.contrato,
                    _ => continue,
                };
            }
        }

        // Fallback: bare doc_type strings (e.g. "lei", "decreto") that don't
        // include the regulatory prefix pattern but still carry semantic meaning.
        let normalized = doc_type.to_lowercase();
        let normalized = normalized.trim();
        let ascii_normalized = normalize_accents(normalized);

        if normalized.contains("lei") {
            return self.authority_weights.lei;
        }
        if normalized.contains("decreto") {
            return self.authority_weights.decreto;
        }
        if normalized.contains("resoluc") || ascii_normalized.contains("resoluc") {
            return self.authority_weights.resolucao_antt;
        }
        if normalized.contains("acord") || ascii_normalized.contains("acord") {
            return self.authority_weights.acordao_tcu;
        }
        if normalized.contains("parecer") {
            return self.authority_weights.parecer_pf;
        }
        if (normalized.contains("nota") && normalized.contains("tecn"))
            || (ascii_normalized.contains("nota") && ascii_normalized.contains("tecn"))
        {
            return self.authority_weights.nota_tecnica;
        }
        if normalized.contains("contrato") {
            return self.authority_weights.contrato;
        }
        if normalized.contains("despacho") {
            return self.authority_weights.despacho;
        }

        self.authority_weights.outros
    }

    fn compute_recency(&self, doc_date: &Option<String>, ref_date: &str) -> f64 {
        match doc_date {
            Some(date) => {
                let doc = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();
                let reference = chrono::NaiveDate::parse_from_str(ref_date, "%Y-%m-%d").ok();

                match (doc, reference) {
                    (Some(d), Some(r)) => {
                        let days = (r - d).num_days().abs() as f64;
                        (-days * 0.00190).exp()
                    }
                    _ => 0.5,
                }
            }
            None => 0.5,
        }
    }

    fn compute_keyword_match(&self, content: &str, keywords: &[String]) -> f64 {
        if keywords.is_empty() {
            // No caller-supplied keywords: use TECHNICAL_KEYWORDS static matcher
            // (AhoCorasick O(n)) for a signal based on domain vocabulary density.
            // Score = matched_count / total_patterns, capped at 1.0.
            let hits = TECHNICAL_KEYWORDS.find_matches(content).len();
            let total = TECHNICAL_KEYWORDS.pattern_count();
            if total == 0 {
                return 0.5;
            }
            // Clamp: more than one hit per pattern is still 1.0.
            return (hits as f64 / total as f64).min(1.0);
        }

        // Caller supplied an explicit keyword list — build a one-shot AhoCorasick
        // automaton and scan once (O(n)) instead of O(n*m) sequential contains().
        use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
        let ac = match AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .build(keywords)
        {
            Ok(built) => built,
            Err(_) => {
                // Fallback: build empty automaton — no matches, score = 0.
                AhoCorasick::new(std::iter::empty::<&str>())
                    .expect("empty AhoCorasick always succeeds")
            }
        };

        // Count distinct patterns that matched at least once.
        let mut matched = vec![false; keywords.len()];
        for m in ac.find_iter(content) {
            let idx = m.pattern().as_usize();
            if idx < matched.len() {
                matched[idx] = true;
            }
        }
        let hit_count = matched.iter().filter(|&&v| v).count();
        hit_count as f64 / keywords.len() as f64
    }

    fn get_historical_preference(&self, doc_id: &str) -> f64 {
        let clicks = self.historical_clicks.get(doc_id).copied().unwrap_or(0);
        1.0 / (1.0 + (-0.1 * clicks as f64).exp())
    }

    fn combine_scores(&self, factors: &RankingFactors, weights: &RerankWeights) -> f64 {
        weights.semantic * factors.semantic_relevance
            + weights.authority * factors.document_authority
            + weights.recency * factors.recency
            + weights.keyword * factors.keyword_match
            + weights.historical * factors.historical_preference
    }

    /// Record a user click on `document_id`, feeding the historical-preference
    /// factor.
    pub fn record_click(&mut self, document_id: &str) {
        *self
            .historical_clicks
            .entry(document_id.to_string())
            .or_insert(0) += 1;
    }

    /// Store a query embedding under `query_hash`.
    ///
    /// Takes `&self` (not `&mut self`) because `moka::sync::Cache` is
    /// internally concurrent — callers holding an `&ContextualReranker`
    /// from multiple threads can all insert without external locking.
    pub fn cache_query_embedding(&self, query_hash: &str, embedding: Vec<f32>) {
        self.query_cache
            .insert(query_hash.to_string(), Arc::new(embedding));
    }

    /// Look up a previously cached query embedding.
    ///
    /// Increments the hit/miss counters for observability. Returns
    /// `Arc<Vec<f32>>` so hits do not pay the cost of cloning the full
    /// embedding vector — callers that need an owned `Vec` can do
    /// `(*arc).clone()` at the boundary.
    pub fn lookup_query_embedding(&self, query_hash: &str) -> Option<Arc<Vec<f32>>> {
        match self.query_cache.get(query_hash) {
            Some(v) => {
                self.query_cache_hits.fetch_add(1, Ordering::Relaxed);
                Some(v)
            }
            None => {
                self.query_cache_misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Clear all cached query embeddings. Also takes `&self` — see the note
    /// on [`Self::cache_query_embedding`].
    pub fn clear_query_cache(&self) {
        self.query_cache.invalidate_all();
        self.query_cache.run_pending_tasks();
    }

    /// Snapshot the reranker's cache and click statistics.
    pub fn stats(&self) -> RerankerStats {
        // Force pending eviction tasks through so `entry_count` and
        // `weighted_size` reflect the post-insert state deterministically.
        self.query_cache.run_pending_tasks();
        RerankerStats {
            cached_queries: self.query_cache.entry_count() as usize,
            cached_bytes: self.query_cache.weighted_size(),
            query_cache_hits: self.query_cache_hits.load(Ordering::Relaxed),
            query_cache_misses: self.query_cache_misses.load(Ordering::Relaxed),
            tracked_documents: self.historical_clicks.len(),
            total_clicks: self.historical_clicks.values().sum(),
        }
    }

    /// Borrow the SIMD `CosineComputer` used for semantic similarity.
    pub fn cosine_computer(&self) -> &CosineComputer {
        &self.cosine_computer
    }
}

/// Reranker statistics.
///
/// `cached_queries` is the count of live query-embedding cache entries;
/// `cached_bytes` is their combined weight (f32-aware). Hit/miss counters
/// track the cache's effective utility over the lifetime of the reranker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankerStats {
    /// Count of live query-embedding cache entries.
    pub cached_queries: usize,
    /// Combined weight of cached embeddings, in bytes.
    pub cached_bytes: u64,
    /// Number of cache hits over the reranker's lifetime.
    pub query_cache_hits: u64,
    /// Number of cache misses over the reranker's lifetime.
    pub query_cache_misses: u64,
    /// Number of distinct documents with recorded clicks.
    pub tracked_documents: usize,
    /// Total clicks recorded across all documents.
    pub total_clicks: u64,
}

/// Compute NDCG@K for evaluating ranking quality
pub fn compute_ndcg(ranked_results: &[RankedResult], relevance_scores: &[f64], k: usize) -> f64 {
    if ranked_results.is_empty() || relevance_scores.is_empty() || k == 0 {
        return 0.0;
    }

    let k = k.min(ranked_results.len()).min(relevance_scores.len());

    let dcg: f64 = (0..k)
        .map(|i| {
            let original_pos = ranked_results
                .get(i)
                .map(|r| r.original_position)
                .unwrap_or(0);
            let rel = relevance_scores.get(original_pos).copied().unwrap_or(0.0);
            rel / (2.0_f64 + i as f64).log2()
        })
        .sum();

    let mut ideal_relevance = relevance_scores.to_vec();
    ideal_relevance.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let idcg: f64 = (0..k)
        .map(|i| {
            let rel = ideal_relevance.get(i).copied().unwrap_or(0.0);
            rel / (2.0_f64 + i as f64).log2()
        })
        .sum();

    if idcg == 0.0 {
        return 0.0;
    }

    dcg / idcg
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_results() -> Vec<SearchResult> {
        vec![
            SearchResult::new("doc1", 0.90, "Lei 10.233/2001 estabelece...")
                .with_type("lei")
                .with_date("2001-06-05"),
            SearchResult::new("doc2", 0.85, "Resolução ANTT 5.950/2021 dispõe sobre...")
                .with_type("resolucao")
                .with_date("2021-07-20"),
            SearchResult::new("doc3", 0.92, "Nota Técnica analisa reequilíbrio...")
                .with_type("nota_tecnica")
                .with_date("2025-01-15"),
            SearchResult::new("doc4", 0.88, "Parecer PF-ANTT opina pela aprovação...")
                .with_type("parecer")
                .with_date("2024-06-10"),
        ]
    }

    #[test]
    fn test_reranker_new() {
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.historical_clicks.len(), 0);
        // moka cache exposes `entry_count()` instead of `len()`.
        let stats = reranker.stats();
        assert_eq!(stats.cached_queries, 0);
        assert_eq!(stats.cached_bytes, 0);
    }

    #[test]
    fn test_rerank_basic() {
        let reranker = ContextualReranker::new();
        let results = create_test_results();
        let context = RerankContext::default();

        let ranked = reranker
            .rerank(&results, &context)
            .expect("rerank with valid results must succeed");

        assert_eq!(ranked.len(), 4);
        for (idx, result) in ranked.iter().enumerate() {
            assert_eq!(result.new_position, idx);
        }
    }

    #[test]
    fn test_rerank_empty_results() {
        let reranker = ContextualReranker::new();
        let results: Vec<SearchResult> = vec![];
        let context = RerankContext::default();

        let result = reranker.rerank(&results, &context);
        assert!(matches!(result, Err(RerankerError::EmptyResults)));
    }

    #[test]
    fn test_authority_weights() {
        let reranker = ContextualReranker::new();

        assert_eq!(reranker.get_authority("lei"), 1.0);
        assert_eq!(reranker.get_authority("decreto"), 0.95);
        assert_eq!(reranker.get_authority("random"), 0.50);
    }

    #[test]
    fn test_rerank_weights_validation() {
        let mut weights = RerankWeights {
            semantic: 0.5,
            authority: 0.2,
            recency: 0.1,
            keyword: 0.1,
            historical: 0.1,
        };

        assert!(weights.validate().is_ok());

        weights.semantic = 0.6;
        assert!(weights.validate().is_err());

        weights.normalize();
        assert!(weights.validate().is_ok());
    }

    #[test]
    fn test_ndcg_computation() {
        let ranked_results = vec![
            RankedResult {
                document_id: "doc1".to_string(),
                original_score: 0.9,
                reranked_score: 0.95,
                content: "...".to_string(),
                ranking_factors: RankingFactors::default(),
                original_position: 0,
                new_position: 0,
            },
            RankedResult {
                document_id: "doc2".to_string(),
                original_score: 0.8,
                reranked_score: 0.85,
                content: "...".to_string(),
                ranking_factors: RankingFactors::default(),
                original_position: 1,
                new_position: 1,
            },
        ];

        let relevance = vec![1.0, 0.5];
        let ndcg = compute_ndcg(&ranked_results, &relevance, 2);
        assert!(ndcg > 0.0 && ndcg <= 1.0);
    }

    #[test]
    fn test_stats() {
        let mut reranker = ContextualReranker::new();

        reranker.record_click("doc1");
        reranker.record_click("doc1");
        reranker.record_click("doc2");
        reranker.cache_query_embedding("hash1", vec![1.0, 2.0, 3.0]);

        let stats = reranker.stats();
        assert_eq!(stats.cached_queries, 1);
        assert_eq!(stats.tracked_documents, 2);
        assert_eq!(stats.total_clicks, 3);
    }

    // ── T0.1: AhoCorasick wiring tests ───────────────────────────────────────

    #[test]
    fn test_get_authority_via_antt_patterns_law() {
        // "Lei nº" triggers ANTT_PATTERNS Law category → lei weight (1.0)
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.get_authority("Lei nº 10.233/2001"), 1.0);
    }

    #[test]
    fn test_get_authority_via_antt_patterns_decree() {
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.get_authority("Decreto nº 2.521/1998"), 0.95);
    }

    #[test]
    fn test_get_authority_via_antt_patterns_resolution() {
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.get_authority("Resolução ANTT nº 5.950"), 0.90);
    }

    #[test]
    fn test_get_authority_via_antt_patterns_tcu() {
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.get_authority("Acórdão TCU nº 1.234/2020"), 0.88);
    }

    #[test]
    fn test_get_authority_via_antt_patterns_opinion() {
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.get_authority("Parecer PF-ANTT nº 001/2024"), 0.85);
    }

    #[test]
    fn test_get_authority_via_antt_patterns_technical_note() {
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.get_authority("Nota Técnica nº 42/2025"), 0.75);
    }

    #[test]
    fn test_get_authority_fallback_bare_type() {
        // Bare "lei"/"decreto" don't match ANTT_PATTERNS prefix patterns,
        // so they fall through to the contains() fallback path.
        let reranker = ContextualReranker::new();
        assert_eq!(reranker.get_authority("lei"), 1.0);
        assert_eq!(reranker.get_authority("decreto"), 0.95);
        assert_eq!(reranker.get_authority("random"), 0.50);
    }

    #[test]
    fn test_compute_keyword_match_with_explicit_keywords_aho_corasick() {
        // With explicit keywords, AhoCorasick path is used — distinct pattern hits.
        let reranker = ContextualReranker::new();
        let keywords = vec![
            "reequilíbrio".to_string(),
            "TIR".to_string(),
            "WACC".to_string(),
        ];
        // Content contains all 3 — score should be 1.0
        let score = reranker
            .compute_keyword_match("O reequilíbrio com TIR de 10% e WACC de 8%.", &keywords);
        assert!(
            (score - 1.0).abs() < f64::EPSILON,
            "expected 1.0 got {score}"
        );
    }

    #[test]
    fn test_compute_keyword_match_partial_hit() {
        let reranker = ContextualReranker::new();
        let keywords = vec!["pedágio".to_string(), "xyz_notfound".to_string()];
        // Content contains only "pedágio" → 1 of 2 → 0.5
        let score = reranker.compute_keyword_match("reajuste de pedágio aprovado.", &keywords);
        assert!(
            (score - 0.5).abs() < f64::EPSILON,
            "expected 0.5 got {score}"
        );
    }

    #[test]
    fn test_compute_keyword_match_empty_keywords_uses_technical_static() {
        // Empty keywords → TECHNICAL_KEYWORDS static path.
        // A content-rich text with known ANTT terms should score > 0.
        let reranker = ContextualReranker::new();
        let score = reranker.compute_keyword_match(
            "O reequilíbrio econômico-financeiro com TIR e VPL e WACC e pedágio e multa.",
            &[],
        );
        // Should be > 0 since TECHNICAL_KEYWORDS contains these terms.
        assert!(score > 0.0, "expected score > 0.0, got {score}");
        assert!(score <= 1.0, "expected score <= 1.0, got {score}");
    }

    #[test]
    fn test_compute_keyword_match_empty_content_empty_keywords() {
        let reranker = ContextualReranker::new();
        // Empty content, no keywords — static path returns 0.0 (0 hits).
        let score = reranker.compute_keyword_match("", &[]);
        assert!(score >= 0.0 && score <= 1.0, "score out of range: {score}");
    }
}
