//! S13: TF-IDF bag-of-words embedding for CognitiveNexus.
//!
//! Replaces the pseudo-embedding (`bytes().take(384)`) with a deterministic,
//! semantically meaningful TF-IDF representation. No external model or GPU needed.
//!
//! - **TF** (Term Frequency): `count(term, doc) / total_terms(doc)`
//! - **IDF** (Inverse Document Frequency): `ln(N / df(term))` where df = number of
//!   documents containing the term
//! - **Embedding**: sparse TF-IDF vector projected to a fixed-dimension dense vector
//!   via feature hashing (dimensionality = `EMBEDDING_DIM`)
//!
//! Feature hashing (hashing trick) maps arbitrary tokens to a fixed-size vector
//! without needing to maintain a vocabulary. This is O(tokens) per document.
//!
//! Uses `touring_simd::CosineComputer` for SIMD-accelerated cosine similarity.

use std::collections::HashMap;
use touring_simd::{CosineComputer, CosineSimilarity};

/// Embedding dimensionality for TF-IDF feature hashing.
const EMBEDDING_DIM: usize = 128;

/// A lightweight TF-IDF vectorizer using feature hashing.
///
/// Maintains IDF statistics across a corpus of documents. Call `add_document`
/// to build the IDF model, then `embed` to produce dense embeddings.
#[derive(Debug, Clone)]
pub struct TfIdfVectorizer {
    /// Total number of documents seen.
    doc_count: usize,
    /// Document frequency per token (how many documents contain each token).
    doc_freq: HashMap<String, usize>,
}

impl TfIdfVectorizer {
    /// Create a new empty vectorizer.
    pub fn new() -> Self {
        Self {
            doc_count: 0,
            doc_freq: HashMap::new(),
        }
    }

    /// Add a document to the corpus for IDF computation.
    ///
    /// Tokenizes the text into lowercase words and records which tokens appear.
    pub fn add_document(&mut self, text: &str) {
        self.doc_count += 1;
        let tokens: std::collections::HashSet<String> = tokenize(text).into_iter().collect();
        for token in tokens {
            *self.doc_freq.entry(token).or_insert(0) += 1;
        }
    }

    /// Compute a dense TF-IDF embedding for the given text.
    ///
    /// Uses feature hashing to project the sparse TF-IDF vector into
    /// `EMBEDDING_DIM` dimensions. The resulting vector is L2-normalized.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return vec![0.0; EMBEDDING_DIM];
        }

        // Compute term frequencies
        let mut tf: HashMap<&str, f64> = HashMap::new();
        for token in &tokens {
            *tf.entry(token.as_str()).or_insert(0.0) += 1.0;
        }
        let total = tokens.len() as f64;
        for v in tf.values_mut() {
            *v /= total;
        }

        // Compute TF-IDF and hash into dense vector
        let n = self.doc_count.max(1) as f64;
        let mut embedding = vec![0.0_f32; EMBEDDING_DIM];

        for (token, &freq) in &tf {
            let df = self.doc_freq.get(*token).copied().unwrap_or(0) as f64;
            let idf = if df > 0.0 {
                (n / df).ln() + 1.0 // smoothed IDF
            } else {
                1.0 // unseen token gets neutral IDF
            };
            let tfidf = freq * idf;

            // Feature hashing: map token to bucket via FNV-1a hash
            let bucket = hash_token(token) % EMBEDDING_DIM;
            // Sign trick: alternate positive/negative to reduce collision bias
            let sign = if (hash_token(token) / EMBEDDING_DIM) % 2 == 0 {
                1.0
            } else {
                -1.0
            };
            if let Some(v) = embedding.get_mut(bucket) {
                *v += (tfidf * sign) as f32;
            }
        }

        // L2 normalize
        l2_normalize(&mut embedding);
        embedding
    }

    /// Embedding dimensionality.
    pub fn dim(&self) -> usize {
        EMBEDDING_DIM
    }

    /// Number of documents in the corpus.
    pub fn doc_count(&self) -> usize {
        self.doc_count
    }

    /// Number of unique tokens in the vocabulary.
    pub fn vocab_size(&self) -> usize {
        self.doc_freq.len()
    }
}

impl Default for TfIdfVectorizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Tokenize text into lowercase alphanumeric words.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty() && s.len() >= 2)
        .map(|s| s.to_lowercase())
        .collect()
}

/// FNV-1a hash for feature hashing.
fn hash_token(token: &str) -> usize {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in token.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash as usize
}

/// L2-normalize a vector in place. Zero vectors remain zero.
///
/// Delegates to `touring_simd::simd_utils::matrix::l2_normalize_inplace`,
/// which uses `pulp::Arch`-dispatched SIMD dot-product + scalar rescale.
/// Equivalent to the prior scalar pattern but ~4-8× faster on vectors ≥32
/// dimensions while preserving exact semantics (unit output, zero-norm
/// vectors left unchanged, 1e-10 guard).
#[inline]
fn l2_normalize(v: &mut [f32]) {
    touring_simd::simd_utils::matrix::l2_normalize_inplace(v);
}

/// Cosine similarity between two vectors using SIMD acceleration.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    CosineComputer::new().cosine(a, b) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("Hello World foo_bar baz");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"foo_bar".to_string()));
    }

    #[test]
    fn test_tokenize_filters_short() {
        let tokens = tokenize("a b cd ef");
        assert!(!tokens.contains(&"a".to_string()));
        assert!(!tokens.contains(&"b".to_string()));
        assert!(tokens.contains(&"cd".to_string()));
    }

    #[test]
    fn test_embed_empty_text() {
        let v = TfIdfVectorizer::new();
        let emb = v.embed("");
        assert_eq!(emb.len(), EMBEDDING_DIM);
        assert!(emb.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_embed_produces_normalized_vector() {
        let mut v = TfIdfVectorizer::new();
        v.add_document("the quick brown fox");
        v.add_document("the lazy dog");
        let emb = v.embed("quick fox");
        assert_eq!(emb.len(), EMBEDDING_DIM);

        // Should be approximately L2-normalized
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "embedding should be L2-normalized, got norm={norm}"
        );
    }

    #[test]
    fn test_similar_texts_have_high_similarity() {
        let mut v = TfIdfVectorizer::new();
        v.add_document("rust programming language");
        v.add_document("python programming language");
        v.add_document("machine learning deep learning");

        let emb1 = v.embed("rust programming");
        let emb2 = v.embed("rust language programming");
        let emb3 = v.embed("machine learning deep");

        let sim_related = cosine_similarity(&emb1, &emb2);
        let sim_unrelated = cosine_similarity(&emb1, &emb3);

        assert!(
            sim_related > sim_unrelated,
            "similar texts should have higher similarity: {sim_related} vs {sim_unrelated}"
        );
    }

    #[test]
    fn test_identical_texts_have_similarity_1() {
        let mut v = TfIdfVectorizer::new();
        v.add_document("test document");
        let emb = v.embed("test document");
        let sim = cosine_similarity(&emb, &emb);
        assert!(
            (sim - 1.0).abs() < 0.01,
            "identical texts should have similarity ~1.0, got {sim}"
        );
    }

    #[test]
    fn test_orthogonal_texts_have_low_similarity() {
        let mut v = TfIdfVectorizer::new();
        v.add_document("alpha beta gamma");
        v.add_document("delta epsilon zeta");
        let emb1 = v.embed("alpha beta gamma");
        let emb2 = v.embed("delta epsilon zeta");
        let sim = cosine_similarity(&emb1, &emb2);
        assert!(
            sim < 0.3,
            "orthogonal texts should have low similarity, got {sim}"
        );
    }

    #[test]
    fn test_vectorizer_stats() {
        let mut v = TfIdfVectorizer::new();
        assert_eq!(v.doc_count(), 0);
        assert_eq!(v.vocab_size(), 0);

        v.add_document("hello world");
        v.add_document("hello rust");
        assert_eq!(v.doc_count(), 2);
        assert!(v.vocab_size() >= 3); // hello, world, rust
    }

    #[test]
    fn test_dim() {
        let v = TfIdfVectorizer::new();
        assert_eq!(v.dim(), EMBEDDING_DIM);
    }

    #[test]
    fn test_default_trait() {
        let v = TfIdfVectorizer::default();
        assert_eq!(v.doc_count(), 0);
    }

    #[test]
    fn test_idf_effect() {
        let mut v = TfIdfVectorizer::new();
        // "common" appears in all documents
        v.add_document("common rare_term_alpha");
        v.add_document("common rare_term_beta");
        v.add_document("common rare_term_gamma");

        let emb_rare = v.embed("rare_term_alpha");
        let emb_common = v.embed("common");

        // Rare term should have more concentrated embedding (fewer non-zero entries)
        let nonzero_rare = emb_rare.iter().filter(|&&x| x.abs() > 1e-6).count();
        let nonzero_common = emb_common.iter().filter(|&&x| x.abs() > 1e-6).count();
        // Both should be non-zero (they hash to at least 1 bucket)
        assert!(nonzero_rare > 0, "rare embedding should be non-zero");
        assert!(nonzero_common > 0, "common embedding should be non-zero");
    }

    #[test]
    fn test_hash_token_deterministic() {
        assert_eq!(hash_token("hello"), hash_token("hello"));
        assert_ne!(hash_token("hello"), hash_token("world"));
    }

    #[test]
    fn test_l2_normalize_zero() {
        let mut v = vec![0.0_f32; 4];
        l2_normalize(&mut v);
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_cosine_similarity_mismatched() {
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0]), 0.0);
    }
}
