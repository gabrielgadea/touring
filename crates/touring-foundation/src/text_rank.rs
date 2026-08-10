//! Shared BM25 ranking math — the single scoring implementation in the crate.
//!
//! Extracted from [`crate::tool_catalog`] (2026-08-08) so that intent-ranked
//! discovery over *any* corpus reuses one scorer instead of growing a second
//! copy. The tokenizer is deliberately **not** shared: `tool_catalog` ranks a
//! curated English catalog, while [`crate::portfolio`] ranks mined prose that
//! must also answer Portuguese intents (see [`crate::portfolio::lexicon`]).
//! Shared math, distinct lexicons.
//!
//! # Example
//!
//! ```
//! use touring_foundation::text_rank::{Bm25Corpus, bm25_score_doc};
//!
//! let docs = vec![
//!     vec!["generate".to_string(), "map".to_string()],
//!     vec!["parse".to_string(), "config".to_string()],
//! ];
//! let corpus = Bm25Corpus::new(docs);
//! let terms = vec!["map".to_string()];
//! let df = corpus.doc_freq(&terms);
//! let hit = bm25_score_doc(corpus.doc(0), &terms, &df, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
//! let miss = bm25_score_doc(corpus.doc(1), &terms, &df, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
//! assert!(hit > 0.0 && miss == 0.0);
//! ```

/// BM25 term-frequency saturation parameter.
pub const BM25_K1: f64 = 1.2;

/// BM25 document-length normalization parameter.
pub const BM25_B: f64 = 0.75;

/// A tokenized corpus with the aggregate statistics BM25 needs.
///
/// Holds the per-document token vectors plus `n` (corpus size) and `avgdl`
/// (mean document length), so callers compute them once per query instead of
/// once per document.
#[derive(Debug, Clone)]
pub struct Bm25Corpus {
    docs: Vec<Vec<String>>,
    avgdl: f64,
}

impl Bm25Corpus {
    /// Build a corpus from pre-tokenized documents.
    ///
    /// An empty corpus yields `avgdl == 1.0` so scoring never divides by zero.
    #[must_use]
    pub fn new(docs: Vec<Vec<String>>) -> Self {
        let total_len: usize = docs.iter().map(Vec::len).sum();
        let avgdl = if docs.is_empty() || total_len == 0 {
            1.0
        } else {
            total_len as f64 / docs.len() as f64
        };
        Self { docs, avgdl }
    }

    /// Number of documents in the corpus, as the `f64` BM25 wants.
    #[must_use]
    pub fn n(&self) -> f64 {
        self.docs.len() as f64
    }

    /// Mean document length.
    #[must_use]
    pub fn avgdl(&self) -> f64 {
        self.avgdl
    }

    /// The token vector of document `i`, or an empty slice when out of range.
    #[must_use]
    pub fn doc(&self, i: usize) -> &[String] {
        self.docs.get(i).map_or(&[], Vec::as_slice)
    }

    /// Total document count as `usize`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// True when the corpus holds no documents.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Document frequency of each query term over the whole corpus.
    ///
    /// Returns a vector parallel to `terms`; `df[t] == 0.0` means the term
    /// appears in no document and contributes nothing to any score.
    #[must_use]
    pub fn doc_freq(&self, terms: &[String]) -> Vec<f64> {
        terms
            .iter()
            .map(|term| {
                self.docs
                    .iter()
                    .filter(|d| d.iter().any(|w| w == term))
                    .count() as f64
            })
            .collect()
    }
}

/// BM25 score of one document against `terms`, with an optional field boost.
///
/// `df[t]` is the document frequency of `terms[t]`, `n` the corpus size and
/// `avgdl` the mean document length. `boost_term` decides, per term, whether
/// that term matched a high-value field (a name, a keyword) and therefore earns
/// the `field_boost` multiplier — this is the hand-rolled analog of tantivy's
/// `QueryParser::set_field_boost`.
///
/// Pure and total: unknown terms and empty documents score `0.0`.
#[must_use]
pub fn bm25_score_doc(
    doc: &[String],
    terms: &[String],
    df: &[f64],
    n: f64,
    avgdl: f64,
    boost_term: &dyn Fn(&str) -> bool,
    field_boost: f64,
) -> f64 {
    let dl = doc.len() as f64;
    let mut score = 0.0_f64;
    for (t, term) in terms.iter().enumerate() {
        let tf = doc.iter().filter(|w| *w == term).count() as f64;
        let df_t = df.get(t).copied().unwrap_or(0.0);
        if df_t == 0.0 || tf == 0.0 {
            continue;
        }
        let idf = (1.0 + (n - df_t + 0.5) / (df_t + 0.5)).ln();
        let norm = tf * (BM25_K1 + 1.0) / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avgdl));
        let mut term_score = idf * norm;
        if boost_term(term) {
            term_score *= field_boost;
        }
        score += term_score;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn empty_corpus_has_safe_avgdl_and_scores_zero() {
        let corpus = Bm25Corpus::new(vec![]);
        assert!(corpus.is_empty());
        assert_eq!(corpus.avgdl(), 1.0, "avgdl must never be 0 (division guard)");
        let terms = toks("map");
        let df = corpus.doc_freq(&terms);
        assert!(df.iter().all(|d| *d == 0.0));
        let s = bm25_score_doc(corpus.doc(0), &terms, &df, corpus.n(), corpus.avgdl(), &|_| false, 2.0);
        assert_eq!(s, 0.0);
    }

    #[test]
    fn matching_document_outranks_non_matching() {
        let corpus = Bm25Corpus::new(vec![toks("generate a map of the module graph"), toks("parse a toml config file")]);
        let terms = toks("map graph");
        let df = corpus.doc_freq(&terms);
        let a = bm25_score_doc(corpus.doc(0), &terms, &df, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
        let b = bm25_score_doc(corpus.doc(1), &terms, &df, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
        assert!(a > b, "a={a} b={b}");
        assert_eq!(b, 0.0);
    }

    #[test]
    fn field_boost_multiplies_only_boosted_terms() {
        let corpus = Bm25Corpus::new(vec![toks("generate map"), toks("generate chart")]);
        let terms = toks("map");
        let df = corpus.doc_freq(&terms);
        let plain = bm25_score_doc(corpus.doc(0), &terms, &df, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
        let boosted = bm25_score_doc(corpus.doc(0), &terms, &df, corpus.n(), corpus.avgdl(), &|t| t == "map", 3.0);
        assert!((boosted - plain * 3.0).abs() < 1e-9, "plain={plain} boosted={boosted}");
    }

    #[test]
    fn df_shorter_than_terms_does_not_panic() {
        // Defensive: a caller that mismatches df/terms lengths gets 0 for the
        // unknown tail rather than an index panic.
        let corpus = Bm25Corpus::new(vec![toks("generate map")]);
        let terms = toks("generate map extra");
        let df = vec![1.0];
        let s = bm25_score_doc(corpus.doc(0), &terms, &df, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
        assert!(s > 0.0);
    }

    #[test]
    fn rarer_term_earns_higher_idf() {
        // "map" appears in 1 of 3 docs, "generate" in all 3 → map must score higher.
        let corpus = Bm25Corpus::new(vec![
            toks("generate map"),
            toks("generate chart"),
            toks("generate report"),
        ]);
        let rare = toks("map");
        let common = toks("generate");
        let df_rare = corpus.doc_freq(&rare);
        let df_common = corpus.doc_freq(&common);
        let s_rare = bm25_score_doc(corpus.doc(0), &rare, &df_rare, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
        let s_common = bm25_score_doc(corpus.doc(0), &common, &df_common, corpus.n(), corpus.avgdl(), &|_| false, 1.0);
        assert!(s_rare > s_common, "rare={s_rare} common={s_common}");
    }
}
