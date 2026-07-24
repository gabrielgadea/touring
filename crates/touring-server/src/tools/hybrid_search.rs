//! Hybrid Search — Reciprocal Rank Fusion (RRF) for FTS5 + embeddings.
//!
//! Combines keyword search (BM25 via FTS5) with vector similarity
//! using RRF fusion (k=60, from the original paper). Adds query-aware
//! kind boosting: PascalCase→struct, snake_case→function, ::→qualified.
//! Inspired by code-review-graph's `search.py` hybrid search engine.

use serde::Serialize;
use std::collections::HashMap;

/// Default RRF fusion constant (from Cormack et al., 2009).
const DEFAULT_K: f64 = 60.0;

/// A search result with fused score.
#[derive(Debug, Clone, Serialize)]
pub struct HybridResult {
    /// Identifier of the matched result.
    pub key: String,
    /// Reciprocal-rank-fused score across the two rankings.
    pub fused_score: f64,
    /// Rank in the FTS5 keyword list, if present.
    pub fts_rank: Option<usize>,
    /// Rank in the vector-similarity list, if present.
    pub vec_rank: Option<usize>,
}

/// Reciprocal Rank Fusion combining two ranked result lists.
///
/// For each result, RRF score = sum of 1/(k + rank) across all lists
/// where the result appears. Higher k produces more uniform fusion.
pub fn rrf_fuse(
    fts_results: &[(String, f64)],
    vec_results: &[(String, f64)],
    k: Option<f64>,
) -> Vec<HybridResult> {
    let k = k.unwrap_or(DEFAULT_K);
    let mut scores: HashMap<String, (f64, Option<usize>, Option<usize>)> = HashMap::new();

    for (rank, (key, _bm25)) in fts_results.iter().enumerate() {
        let entry = scores.entry(key.clone()).or_insert((0.0, None, None));
        entry.0 += 1.0 / (k + rank as f64 + 1.0);
        entry.1 = Some(rank);
    }

    for (rank, (key, _cosine)) in vec_results.iter().enumerate() {
        let entry = scores.entry(key.clone()).or_insert((0.0, None, None));
        entry.0 += 1.0 / (k + rank as f64 + 1.0);
        entry.2 = Some(rank);
    }

    let mut results: Vec<HybridResult> = scores
        .into_iter()
        .map(|(key, (score, fts_rank, vec_rank))| HybridResult {
            key,
            fused_score: score,
            fts_rank,
            vec_rank,
        })
        .collect();

    results.sort_by(|a, b| {
        b.fused_score
            .partial_cmp(&a.fused_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

/// Apply kind-specific boost multipliers to fused results.
///
/// Modifies scores in-place based on query pattern heuristics.
pub fn apply_kind_boosts(results: &mut [HybridResult], boosts: &HashMap<String, f64>) {
    if boosts.is_empty() {
        return;
    }
    for result in results.iter_mut() {
        for (pattern, multiplier) in boosts {
            if result.key.contains(pattern.as_str()) {
                result.fused_score *= multiplier;
            }
        }
    }
    results.sort_by(|a, b| {
        b.fused_score
            .partial_cmp(&a.fused_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Detect query-aware kind boost multipliers from query text.
///
/// - PascalCase (e.g. `MyClass`) → boost keys containing uppercase (structs)
/// - snake_case (e.g. `get_users`) → boost keys containing underscores (functions)
/// - Contains `::` → boost qualified names
pub fn detect_kind_boosts(query: &str) -> HashMap<String, f64> {
    let mut boosts = HashMap::new();
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return boosts;
    }

    // PascalCase: starts with uppercase, contains lowercase
    let first_upper = trimmed
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);
    let has_lower = trimmed.chars().any(|c| c.is_lowercase());
    if first_upper && has_lower && !trimmed.contains('_') {
        boosts.insert("::".to_string(), 1.5); // Boost qualified (struct-like) names
    }

    // snake_case: all lowercase with underscores
    let all_lower_underscore = trimmed
        .chars()
        .all(|c| c.is_lowercase() || c == '_' || c.is_numeric());
    if trimmed.contains('_') && all_lower_underscore {
        boosts.insert("_".to_string(), 1.3); // Boost function-like names
    }

    // Qualified names: contains ::
    if trimmed.contains("::") {
        boosts.insert("::".to_string(), 2.0);
    }

    boosts
}

/// Full hybrid search pipeline: FTS5 + vector → RRF → kind boost → top-K.
pub fn hybrid_search(
    fts_results: &[(String, f64)],
    vec_results: &[(String, f64)],
    query: &str,
    top_k: usize,
) -> Vec<HybridResult> {
    let mut results = rrf_fuse(fts_results, vec_results, None);
    let boosts = detect_kind_boosts(query);
    apply_kind_boosts(&mut results, &boosts);
    results.truncate(top_k);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fuse_basic() {
        let fts = vec![("a".into(), 0.9), ("b".into(), 0.7), ("c".into(), 0.5)];
        let vec = vec![("b".into(), 0.8), ("d".into(), 0.6), ("a".into(), 0.4)];
        let results = rrf_fuse(&fts, &vec, None);
        // "a" and "b" appear in both lists → higher fused score
        assert!(results[0].key == "a" || results[0].key == "b");
        assert!(results[0].fused_score > results[2].fused_score);
    }

    #[test]
    fn test_rrf_fuse_overlap_higher() {
        let fts = vec![("shared".into(), 1.0)];
        let vec = vec![("shared".into(), 1.0)];
        let fts_only = vec![("solo".into(), 1.0)];
        let empty: Vec<(String, f64)> = vec![];

        let both = rrf_fuse(&fts, &vec, None);
        let one = rrf_fuse(&fts_only, &empty, None);

        assert!(
            both[0].fused_score > one[0].fused_score,
            "Appearing in both lists should score higher"
        );
    }

    #[test]
    fn test_rrf_fuse_disjoint() {
        let fts = vec![("a".into(), 1.0), ("b".into(), 0.5)];
        let vec = vec![("c".into(), 1.0), ("d".into(), 0.5)];
        let results = rrf_fuse(&fts, &vec, None);
        assert_eq!(results.len(), 4, "All items should appear");
    }

    #[test]
    fn test_detect_kind_boosts_pascal() {
        let boosts = detect_kind_boosts("MyClassName");
        assert!(!boosts.is_empty(), "PascalCase should produce boosts");
    }

    #[test]
    fn test_detect_kind_boosts_snake() {
        let boosts = detect_kind_boosts("get_user_by_id");
        assert!(
            boosts.contains_key("_"),
            "snake_case should boost underscore keys"
        );
    }

    #[test]
    fn test_detect_kind_boosts_qualified() {
        let boosts = detect_kind_boosts("module::function");
        assert!(boosts.contains_key("::"), ":: should boost qualified names");
        assert!(*boosts.get("::").unwrap() >= 2.0);
    }

    #[test]
    fn test_detect_kind_boosts_empty() {
        let boosts = detect_kind_boosts("");
        assert!(boosts.is_empty());
    }

    #[test]
    fn test_hybrid_search_pipeline() {
        let fts = vec![("fn_a".into(), 0.9), ("ClassB".into(), 0.5)];
        let vec = vec![("ClassB".into(), 0.9), ("fn_c".into(), 0.3)];
        let results = hybrid_search(&fts, &vec, "ClassB", 10);
        // ClassB should rank high (in both lists + PascalCase boost)
        assert_eq!(results[0].key, "ClassB");
    }

    #[test]
    fn test_hybrid_search_top_k() {
        let fts: Vec<(String, f64)> = (0..20)
            .map(|i| (format!("r{}", i), 1.0 - i as f64 * 0.05))
            .collect();
        let vec: Vec<(String, f64)> = vec![];
        let results = hybrid_search(&fts, &vec, "query", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_apply_kind_boosts_reorders() {
        let mut results = vec![
            HybridResult {
                key: "plain".into(),
                fused_score: 0.5,
                fts_rank: Some(0),
                vec_rank: None,
            },
            HybridResult {
                key: "module::func".into(),
                fused_score: 0.4,
                fts_rank: Some(1),
                vec_rank: None,
            },
        ];
        let mut boosts = HashMap::new();
        boosts.insert("::".to_string(), 2.0);
        apply_kind_boosts(&mut results, &boosts);
        assert_eq!(
            results[0].key, "module::func",
            "Boosted result should be first"
        );
    }
}
