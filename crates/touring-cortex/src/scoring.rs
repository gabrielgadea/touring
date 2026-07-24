//! E1-S1: BM25-lite relevance scoring for context ranking.
//!
//! Ranks context items by relevance to the current query (tool name, file path,
//! event type) before injection. Ensures the most relevant context gets priority
//! within the token budget.

use std::collections::HashMap;

/// BM25 parameters (standard IR defaults).
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// Score a single document against query terms using simplified BM25.
///
/// Returns a relevance score >= 0. Higher = more relevant.
pub fn bm25_score(query_terms: &[&str], doc: &str, avg_doc_len: f64) -> f64 {
    if doc.is_empty() || query_terms.is_empty() || avg_doc_len <= 0.0 {
        return 0.0;
    }

    let doc_lower = doc.to_lowercase();
    let doc_words: Vec<&str> = doc_lower.split_whitespace().collect();
    let doc_len = doc_words.len() as f64;

    // Build term frequency map
    let mut tf_map: HashMap<&str, usize> = HashMap::new();
    for word in &doc_words {
        *tf_map.entry(word).or_insert(0) += 1;
    }

    let mut score = 0.0;
    for &term in query_terms {
        let term_lower = term.to_lowercase();
        // Check both exact word match and substring containment
        let tf = tf_map.get(term_lower.as_str()).copied().unwrap_or(0) as f64;
        let substring_bonus = if tf == 0.0 && doc_lower.contains(&term_lower) {
            0.5 // partial match bonus
        } else {
            0.0
        };

        let effective_tf = tf + substring_bonus;
        if effective_tf > 0.0 {
            let numerator = effective_tf * (K1 + 1.0);
            let denominator = effective_tf + K1 * (1.0 - B + B * doc_len / avg_doc_len);
            score += numerator / denominator;
        }
    }
    score
}

/// Rank a list of context items by BM25 relevance to the query.
///
/// Returns items sorted by descending relevance score.
/// Items with score 0 are included at the end (preserving original order).
pub fn rank_by_relevance<'a>(query_terms: &[&str], items: &'a [String]) -> Vec<(&'a str, f64)> {
    if query_terms.is_empty() || items.is_empty() {
        return items.iter().map(|s| (s.as_str(), 0.0)).collect();
    }

    let total_words: usize = items.iter().map(|s| s.split_whitespace().count()).sum();
    let avg_len = total_words as f64 / items.len() as f64;
    let avg_len = if avg_len <= 0.0 { 1.0 } else { avg_len };

    let mut scored: Vec<(&str, f64)> = items
        .iter()
        .map(|item| {
            let score = bm25_score(query_terms, item, avg_len);
            (item.as_str(), score)
        })
        .collect();

    // Stable sort: items with equal scores maintain original order
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

/// Extract query terms from CortexContext-like inputs.
///
/// Combines tool name, file path components, and event type into query terms
/// for BM25 scoring.
pub fn extract_query_terms(
    tool_name: Option<&str>,
    file_path: Option<&str>,
    event_name: &str,
) -> Vec<String> {
    let mut terms = Vec::new();

    if let Some(tool) = tool_name {
        terms.push(tool.to_lowercase());
    }

    if let Some(path) = file_path {
        // Extract meaningful path components (filename, extension, parent dir)
        if let Some(filename) = path.rsplit('/').next() {
            terms.push(filename.to_lowercase());
            if let Some(stem) = filename.rsplit('.').nth(1) {
                terms.push(stem.to_lowercase());
            }
        }
    }

    terms.push(event_name.to_lowercase());
    terms
}

/// E5-S7: Document length bucket for adaptive BM25 scoring.
///
/// Buckets normalize scoring based on document length:
/// - SHORT (< 50 words): compact, focused context
/// - MEDIUM (50-200 words): standard documents
/// - LONG (> 200 words): large documents with potentially redundant content
///
/// Using bucket averages instead of per-document avg prevents over-normalization
/// of short documents and under-normalization of long ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocLenBucket {
    /// Short documents (fewer than 50 words).
    Short,
    /// Medium documents (50 to 200 words).
    Medium,
    /// Long documents (more than 200 words).
    Long,
}

impl DocLenBucket {
    /// Classify a document by word count.
    pub fn from_word_count(count: usize) -> Self {
        if count < 50 {
            DocLenBucket::Short
        } else if count < 200 {
            DocLenBucket::Medium
        } else {
            DocLenBucket::Long
        }
    }

    /// Average document length for this bucket (fallback when no bucket data).
    pub fn avg_len(&self) -> f64 {
        match self {
            DocLenBucket::Short => 25.0,
            DocLenBucket::Medium => 100.0,
            DocLenBucket::Long => 350.0,
        }
    }
}

/// E5-S7: BM25 score using bucket-based average document length.
///
/// Falls back to global avg if bucket avg is unavailable.
pub fn bm25_score_with_bucket(
    query_terms: &[&str],
    doc: &str,
    bucket_avg: Option<f64>,
    fallback_avg: f64,
) -> f64 {
    let avg = bucket_avg.unwrap_or(fallback_avg);
    bm25_score(query_terms, doc, avg)
}

#[cfg(test)]
mod tests {
    use super::{DocLenBucket, bm25_score_with_bucket};

    #[test]
    fn test_doc_len_bucket_classification() {
        assert_eq!(DocLenBucket::from_word_count(10), DocLenBucket::Short);
        assert_eq!(DocLenBucket::from_word_count(50), DocLenBucket::Medium);
        assert_eq!(DocLenBucket::from_word_count(200), DocLenBucket::Long);
        assert_eq!(DocLenBucket::from_word_count(49), DocLenBucket::Short);
    }

    #[test]
    fn test_doc_len_bucket_avg() {
        assert!((DocLenBucket::Short.avg_len() - 25.0).abs() < 0.001);
        assert!((DocLenBucket::Medium.avg_len() - 100.0).abs() < 0.001);
        assert!((DocLenBucket::Long.avg_len() - 350.0).abs() < 0.001);
    }

    #[test]
    fn test_bm25_score_with_bucket() {
        let query = &["fn", "main"];
        let doc = "fn main() { let x = 1; }";
        let score = bm25_score_with_bucket(query, doc, Some(25.0), 100.0);
        assert!(score > 0.0);
    }

    #[test]
    fn test_bm25_score_with_bucket_fallback() {
        let query = &["fn", "main"];
        let doc = "fn main() { let x = 1; }";
        // With bucket_avg=None, should use fallback_avg=100.0
        let score = bm25_score_with_bucket(query, doc, None, 100.0);
        assert!(score > 0.0);
    }
}
#[cfg(test)]
mod bm25_tests {
    use super::*;

    #[test]
    fn test_bm25_empty_inputs() {
        assert_eq!(bm25_score(&[], "some doc", 10.0), 0.0);
        assert_eq!(bm25_score(&["term"], "", 10.0), 0.0);
        assert_eq!(bm25_score(&["term"], "doc", 0.0), 0.0);
    }

    #[test]
    fn test_bm25_exact_match_scores_higher() {
        let query = &["error", "file"];
        let doc_match = "error in file parsing";
        let doc_no_match = "everything working fine";
        let avg = 4.0;
        assert!(bm25_score(query, doc_match, avg) > bm25_score(query, doc_no_match, avg));
    }

    #[test]
    fn test_bm25_more_matches_higher_score() {
        let query = &["error", "file", "parse"];
        let doc_2_match = "error in file";
        let doc_1_match = "error detected";
        let avg = 3.0;
        assert!(bm25_score(query, doc_2_match, avg) > bm25_score(query, doc_1_match, avg));
    }

    #[test]
    fn test_bm25_case_insensitive() {
        let query = &["Error"];
        let doc = "error handling code";
        assert!(bm25_score(query, doc, 3.0) > 0.0);
    }

    #[test]
    fn test_bm25_substring_bonus() {
        let query = &["main"];
        let doc = "src/main.rs imports";
        assert!(bm25_score(query, doc, 3.0) > 0.0);
    }

    #[test]
    fn test_rank_by_relevance_ordering() {
        let items = vec![
            "unrelated content here".to_string(),
            "error in file parsing".to_string(),
            "file error handling".to_string(),
        ];
        let query = &["error", "file"];
        let ranked = rank_by_relevance(query, &items);
        // Items mentioning error+file should rank above unrelated
        assert!(ranked[0].1 > ranked[2].1);
    }

    #[test]
    fn test_rank_by_relevance_empty() {
        let items: Vec<String> = vec![];
        let ranked = rank_by_relevance(&["test"], &items);
        assert!(ranked.is_empty());
    }

    #[test]
    fn test_rank_preserves_zero_score_items() {
        let items = vec!["no match at all".to_string()];
        let ranked = rank_by_relevance(&["xyz"], &items);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].1, 0.0);
    }

    #[test]
    fn test_extract_query_terms_full() {
        let terms = extract_query_terms(Some("Read"), Some("/src/main.rs"), "PreToolUse");
        assert!(terms.contains(&"read".to_string()));
        assert!(terms.contains(&"main.rs".to_string()));
        assert!(terms.contains(&"main".to_string()));
        assert!(terms.contains(&"pretooluse".to_string()));
    }

    #[test]
    fn test_extract_query_terms_minimal() {
        let terms = extract_query_terms(None, None, "SessionStart");
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0], "sessionstart");
    }

    #[test]
    fn test_extract_query_terms_with_path() {
        let terms = extract_query_terms(None, Some("/home/user/project/lib.py"), "PostToolUse");
        assert!(terms.contains(&"lib.py".to_string()));
        assert!(terms.contains(&"lib".to_string()));
    }

    #[test]
    fn test_rank_empty_query_returns_all_zero() {
        let items = vec!["some content".to_string()];
        let ranked = rank_by_relevance(&[], &items);
        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].1, 0.0);
    }
}
