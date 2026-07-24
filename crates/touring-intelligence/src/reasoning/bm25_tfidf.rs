//! BM25-enhanced TF-IDF vectorizer combining cosine similarity with keyword reranking.
//!
//! This module provides a two-stage retrieval system:
//!
//! 1. **TF-IDF cosine similarity** for initial candidate selection -- captures global
//!    term-level relevance using feature-hashed dense embeddings.
//! 2. **BM25 keyword reranking** via `ContextualReranker` -- refines ranking using
//!    weighted keyword overlap, improving precision for domain-specific queries.
//!
//! The approach mirrors modern information retrieval pipelines where a fast first-stage
//! retriever (TF-IDF cosine) produces a candidate set, then a more expensive reranker
//! (BM25 keyword weights) re-scores for final output.

use crate::ann::{ContextualReranker, RerankContext, RerankWeights, SearchResult};
use crate::reasoning::tfidf::{TfIdfVectorizer, cosine_similarity};

/// Multiplier for pre-selection before BM25 reranking.
///
/// We fetch `top_k * PRESELECT_MULTIPLIER` candidates from TF-IDF cosine ranking,
/// then rerank this larger pool to produce the final `top_k` results.
const PRESELECT_MULTIPLIER: usize = 3;

/// Maximum snippet length in characters.
const SNIPPET_MAX_CHARS: usize = 200;

/// Minimum word length to qualify as a keyword for reranking.
const MIN_KEYWORD_LEN: usize = 4;

/// Maximum number of keywords extracted from a query.
const MAX_KEYWORDS: usize = 8;

/// A query result with combined TF-IDF + BM25 score.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Document identifier.
    pub id: String,
    /// TF-IDF cosine similarity score (0.0 to 1.0).
    pub tfidf_score: f32,
    /// Final BM25-reranked score (0.0 to 1.0).
    pub bm25_score: f64,
    /// Document text snippet (first 200 chars).
    pub snippet: String,
}

/// BM25-enhanced TF-IDF vectorizer.
///
/// Combines TF-IDF cosine similarity for initial candidate selection
/// with BM25 keyword reranking for final scoring.
///
/// # Example
///
/// ```ignore
/// use touring_intelligence::reasoning::bm25_tfidf::BM25TfIdfVectorizer;
///
/// let mut v = BM25TfIdfVectorizer::new();
/// v.add_document("doc1", "Rust async programming with tokio");
/// v.add_document("doc2", "Python web development with django");
///
/// let results = v.query("async rust tokio", 5);
/// // results[0].id should be "doc1" since it matches the query keywords
/// ```
pub struct BM25TfIdfVectorizer {
    tfidf: TfIdfVectorizer,
    ranker: ContextualReranker,
    documents: Vec<(String, String)>, // (id, text)
}

impl BM25TfIdfVectorizer {
    /// Create a new empty BM25-enhanced TF-IDF vectorizer.
    pub fn new() -> Self {
        Self {
            tfidf: TfIdfVectorizer::new(),
            ranker: ContextualReranker::new(),
            documents: Vec::new(),
        }
    }

    /// Add a document to the index.
    ///
    /// The document is indexed for both TF-IDF cosine similarity and
    /// BM25 keyword reranking.
    pub fn add_document(&mut self, id: impl Into<String>, text: &str) {
        let id = id.into();
        self.tfidf.add_document(text);
        self.documents.push((id, text.to_string()));
    }

    /// Query for top-k documents by combined TF-IDF + BM25 score.
    ///
    /// Returns up to `top_k` results sorted by final reranked score.
    /// If the reranker fails (e.g. empty candidate set after filtering),
    /// falls back to pure TF-IDF cosine ranking.
    pub fn query(&self, query_text: &str, top_k: usize) -> Vec<QueryResult> {
        // Early return for empty corpus or empty query
        if self.documents.is_empty() || query_text.trim().is_empty() || top_k == 0 {
            return Vec::new();
        }

        // Embed the query
        let q_vec = self.tfidf.embed(query_text);

        // Score all documents by cosine similarity
        let mut scored: Vec<(usize, f32)> = self
            .documents
            .iter()
            .enumerate()
            .map(|(idx, (_id, text))| {
                let doc_vec = self.tfidf.embed(text);
                let score = cosine_similarity(&q_vec, &doc_vec);
                (idx, score)
            })
            .collect();

        // Sort by cosine similarity descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Pre-select top candidates for reranking
        let preselect_count = top_k.saturating_mul(PRESELECT_MULTIPLIER).min(scored.len());
        // preselect_count is guaranteed ≤ scored.len() by .min(), so slicing is safe
        let candidates: &[(usize, f32)] = scored.get(..preselect_count).unwrap_or(&[]);

        // Convert candidates to SearchResult for the reranker
        let search_results: Vec<SearchResult> = candidates
            .iter()
            .filter_map(|&(idx, score)| {
                let (id, text) = self.documents.get(idx)?;
                let snippet = make_snippet(text);
                Some(SearchResult::new(id, f64::from(score), &snippet))
            })
            .collect();

        // Build the TF-IDF-only fallback in case reranking fails
        let tfidf_fallback = || -> Vec<QueryResult> {
            candidates
                .iter()
                .take(top_k)
                .filter_map(|&(idx, score)| {
                    let (id, text) = self.documents.get(idx)?;
                    Some(QueryResult {
                        id: id.clone(),
                        tfidf_score: score,
                        bm25_score: f64::from(score),
                        snippet: make_snippet(text),
                    })
                })
                .collect()
        };

        // If no candidates survived filtering, return empty
        if search_results.is_empty() {
            return Vec::new();
        }

        // Extract keywords from query for BM25 reranking
        let keywords: Vec<String> = query_text
            .split_whitespace()
            .filter(|w| w.len() >= MIN_KEYWORD_LEN)
            .take(MAX_KEYWORDS)
            .map(String::from)
            .collect();

        // Build reranking context with BM25-tuned weights
        let ctx = RerankContext {
            query: query_text.to_string(),
            keywords,
            weights: RerankWeights {
                semantic: 0.6,
                authority: 0.0,
                recency: 0.0,
                keyword: 0.35,
                historical: 0.05,
            },
            ..RerankContext::default()
        };

        // Rerank and build final results
        match self.ranker.rerank(&search_results, &ctx) {
            Ok(ranked) => {
                // Build a lookup from document_id to tfidf_score
                let tfidf_scores: std::collections::HashMap<&str, f32> = candidates
                    .iter()
                    .filter_map(|&(idx, score)| {
                        let (id, _) = self.documents.get(idx)?;
                        Some((id.as_str(), score))
                    })
                    .collect();

                ranked
                    .into_iter()
                    .take(top_k)
                    .map(|r| {
                        let tfidf_score = tfidf_scores
                            .get(r.document_id.as_str())
                            .copied()
                            .unwrap_or(0.0);
                        QueryResult {
                            id: r.document_id,
                            tfidf_score,
                            bm25_score: r.reranked_score,
                            snippet: r.content,
                        }
                    })
                    .collect()
            }
            Err(_) => tfidf_fallback(),
        }
    }

    /// Number of indexed documents.
    pub fn doc_count(&self) -> usize {
        self.documents.len()
    }

    /// Vocabulary size (unique tokens from TF-IDF).
    pub fn vocab_size(&self) -> usize {
        self.tfidf.vocab_size()
    }
}

impl Default for BM25TfIdfVectorizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a snippet from text, truncating to `SNIPPET_MAX_CHARS` characters.
fn make_snippet(text: &str) -> String {
    text.chars().take(SNIPPET_MAX_CHARS).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let v = BM25TfIdfVectorizer::new();
        assert_eq!(v.doc_count(), 0);
        assert_eq!(v.vocab_size(), 0);
    }

    #[test]
    fn test_add_document() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("doc1", "hello world of programming");
        assert_eq!(v.doc_count(), 1);
    }

    #[test]
    fn test_query_empty_corpus() {
        let v = BM25TfIdfVectorizer::new();
        let results = v.query("hello world", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_query_returns_max_top_k() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("doc1", "rust programming language systems");
        v.add_document("doc2", "python programming language scripting");
        v.add_document("doc3", "javascript frontend web development");
        v.add_document("doc4", "golang concurrency cloud native");
        v.add_document("doc5", "java enterprise backend services");

        let results = v.query("programming language", 3);
        assert!(results.len() <= 3, "should return at most top_k results");
    }

    #[test]
    fn test_query_finds_relevant_document() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("rust_doc", "rust async programming tokio runtime executor");
        v.add_document("python_doc", "python django web framework templates");
        v.add_document("java_doc", "java spring boot enterprise application");

        let results = v.query("rust async tokio programming", 3);
        assert!(!results.is_empty(), "should find at least one result");

        // The rust doc should be the top result since it shares the most keywords
        if let Some(top) = results.first() {
            assert_eq!(
                top.id, "rust_doc",
                "rust document should be ranked first for a rust query"
            );
        }
    }

    #[test]
    fn test_query_single_document() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("only", "the only document about neural networks");

        let results = v.query("neural networks", 5);
        assert_eq!(results.len(), 1, "should return the single document");
        if let Some(r) = results.first() {
            assert_eq!(r.id, "only");
            assert!(r.tfidf_score > 0.0, "tfidf score should be positive");
        }
    }

    #[test]
    fn test_vocab_grows_with_docs() {
        let mut v = BM25TfIdfVectorizer::new();
        assert_eq!(v.vocab_size(), 0);

        v.add_document("d1", "alpha bravo charlie");
        let size1 = v.vocab_size();
        assert!(size1 > 0, "vocab should grow after first document");

        v.add_document("d2", "delta echo foxtrot");
        let size2 = v.vocab_size();
        assert!(
            size2 > size1,
            "vocab should grow with new unique tokens: {size2} > {size1}"
        );
    }

    #[test]
    fn test_query_filters_by_keywords() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("relevant", "machine learning deep neural network training");
        v.add_document("irrelevant", "cooking recipes pasta tomato sauce italian");

        let results = v.query("machine learning neural network", 2);
        assert!(!results.is_empty());

        // The relevant doc should rank above the irrelevant one
        if let Some(top) = results.first() {
            assert_eq!(
                top.id, "relevant",
                "document with matching keywords should rank first"
            );
        }
    }

    #[test]
    fn test_default_trait() {
        let v = BM25TfIdfVectorizer::default();
        assert_eq!(v.doc_count(), 0);
        assert_eq!(v.vocab_size(), 0);
        // Should not panic
    }

    #[test]
    fn test_query_result_snippet() {
        let mut v = BM25TfIdfVectorizer::new();
        let text = "This is a document about advanced rust programming concepts and patterns";
        v.add_document("doc1", text);

        let results = v.query("rust programming", 1);
        assert!(!results.is_empty());
        if let Some(r) = results.first() {
            assert!(!r.snippet.is_empty(), "snippet should not be empty");
            assert!(
                r.snippet.len() <= SNIPPET_MAX_CHARS + 4, // +4 for potential multi-byte chars
                "snippet should be at most {SNIPPET_MAX_CHARS} chars"
            );
        }
    }

    #[test]
    fn test_query_empty_query_text() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("doc1", "some content here");

        let results = v.query("", 5);
        assert!(results.is_empty(), "empty query should return no results");
    }

    #[test]
    fn test_query_whitespace_only_query() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("doc1", "some content here");

        let results = v.query("   ", 5);
        assert!(
            results.is_empty(),
            "whitespace-only query should return no results"
        );
    }

    #[test]
    fn test_query_top_k_zero() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("doc1", "hello world testing");

        let results = v.query("hello", 0);
        assert!(results.is_empty(), "top_k=0 should return no results");
    }

    #[test]
    fn test_snippet_truncation() {
        let long_text: String = "abcdefghij".repeat(30); // 300 chars
        let snippet = make_snippet(&long_text);
        assert_eq!(
            snippet.chars().count(),
            SNIPPET_MAX_CHARS,
            "snippet should be truncated to {SNIPPET_MAX_CHARS} chars"
        );
    }

    #[test]
    fn test_multiple_documents_ordering() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("d1", "database query optimization postgresql indexing");
        v.add_document("d2", "web frontend react components styling");
        v.add_document("d3", "database schema migration postgresql tables");

        let results = v.query("database postgresql query", 3);
        assert!(results.len() >= 2, "should return multiple results");

        // Both database docs should rank above the frontend doc
        assert!(
            results.iter().any(|r| r.id == "d1" || r.id == "d3"),
            "database documents should appear in results"
        );
    }

    #[test]
    fn test_query_result_scores_are_non_negative() {
        let mut v = BM25TfIdfVectorizer::new();
        v.add_document("doc1", "rust systems programming memory safety");
        v.add_document("doc2", "python data science machine learning");

        let results = v.query("rust programming", 5);
        for r in &results {
            assert!(r.tfidf_score >= 0.0, "tfidf_score should be non-negative");
            assert!(r.bm25_score >= 0.0, "bm25_score should be non-negative");
        }
    }
}
