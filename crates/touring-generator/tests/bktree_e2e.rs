//! E2E tests proving BK-tree fuzzy symbol matching in touring-generator (T2.1).
//!
//! Tests are gated on the `simd-fuzzy` feature, which is enabled by default via
//! the `full` feature composite.
//!
//! Verifies:
//! 1. `BkTreeFuzzyAdapter::seed()` + `top_k()` via the `FuzzyMatcher` trait.
//! 2. Insert 10+ symbols, query with various edit distances.
//! 3. `top_k` on an empty (unseeded, daemon-absent) tree returns [].
//! 4. Exact-distance-0 match is ranked first.
//! 5. Results are ordered by ascending edit distance.

#[cfg(feature = "simd-fuzzy")]
mod bktree_e2e {
    use touring_generator::{BkTreeFuzzyAdapter, FuzzyMatcher};

    /// Seed the adapter with a representative symbol pool.
    fn seeded_adapter() -> BkTreeFuzzyAdapter {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec![
            "format_code".to_string(),
            "Format".to_string(),
            "format_string".to_string(),
            "format_output".to_string(),
            "reformat".to_string(),
            "fomat".to_string(), // 1-edit typo of "format"
            "compute_hash".to_string(),
            "hash_content".to_string(),
            "HashSet".to_string(),
            "HashMap".to_string(),
            "VecDeque".to_string(),
            "unrelated_symbol".to_string(),
        ]);
        adapter
    }

    // ── T2.1-A: basic insert + query ────────────────────────────────────────

    /// After seeding with 12 symbols, `len()` must report the correct count.
    #[test]
    fn test_bktree_len_after_seed() {
        let adapter = seeded_adapter();
        assert_eq!(
            adapter.len(),
            12,
            "len() must equal number of seeded symbols"
        );
        assert!(!adapter.is_empty(), "is_empty() must be false after seed");
    }

    /// `top_k("format", 5, …)` must return results within edit distance 8
    /// (the adapter's fixed radius), all of which are close to "format".
    #[test]
    fn test_bktree_top_k_fuzzy_format() {
        let adapter = seeded_adapter();
        let results = adapter.top_k("format", 5);

        // Must return at least 1 result (the typo "fomat" has dist=1, "Format" dist=1).
        assert!(
            !results.is_empty(),
            "top_k('format', 5) must return at least one result"
        );

        // All returned names must exist in the seed pool.
        let pool = &[
            "format_code",
            "Format",
            "format_string",
            "format_output",
            "reformat",
            "fomat",
            "compute_hash",
            "hash_content",
            "HashSet",
            "HashMap",
            "VecDeque",
            "unrelated_symbol",
        ];
        for suggestion in &results {
            assert!(
                pool.contains(&suggestion.name.as_str()),
                "suggestion {:?} must come from the seed pool",
                suggestion.name
            );
        }
    }

    /// Results must be ordered by ascending edit distance (distance field is non-decreasing).
    #[test]
    fn test_bktree_top_k_results_ordered_by_distance() {
        let adapter = seeded_adapter();
        let results = adapter.top_k("format", 6);

        for window in results.windows(2) {
            assert!(
                window[0].distance <= window[1].distance,
                "results must be ordered by ascending edit distance: {} > {}",
                window[0].distance,
                window[1].distance
            );
        }
    }

    /// Exact-match query: seeding "`HashMap`" then querying "`HashMap`" with k=1
    /// must return "`HashMap`" as the first result with distance=0.
    #[test]
    fn test_bktree_exact_match_distance_zero() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec![
            "HashMap".to_string(),
            "HashSet".to_string(),
            "VecDeque".to_string(),
        ]);

        let results = adapter.top_k("HashMap", 1);
        assert_eq!(
            results.len(),
            1,
            "top_k('HashMap', 1) must return exactly 1 result"
        );
        assert_eq!(
            results[0].name, "HashMap",
            "exact match must be named 'HashMap'"
        );
        assert_eq!(results[0].distance, 0, "exact match must have distance=0");
    }

    // ── T2.1-B: empty tree behaviour ────────────────────────────────────────

    /// An un-seeded adapter (tree is empty, CLI daemon absent in tests)
    /// must return an empty Vec without panicking.
    #[test]
    fn test_bktree_empty_query_returns_empty() {
        let adapter = BkTreeFuzzyAdapter::new();
        // Mark seed_attempted so the lazy-seed CLI subprocess is skipped.
        // We explicitly call seed() with an empty slice to keep tree empty
        // and mark the seed as done.
        adapter.seed(vec![]);

        let results = adapter.top_k("format", 3);
        assert!(
            results.is_empty(),
            "empty tree must return [] without panicking"
        );
    }

    /// `top_k` with k=0 must return an empty Vec (not a panic).
    #[test]
    fn test_bktree_k_zero_returns_empty() {
        let adapter = seeded_adapter();
        let results = adapter.top_k("format", 0);
        assert!(results.is_empty(), "k=0 must return empty results");
    }

    // ── T2.1-C: confidence field ─────────────────────────────────────────────

    /// Exact match (dist=0) must have confidence close to 1.0.
    /// `confidence = 1 / (1 + dist)` → dist=0 → 1.0.
    #[test]
    fn test_bktree_exact_match_confidence_is_one() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec!["foo".to_string(), "bar".to_string()]);

        let results = adapter.top_k("foo", 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].distance, 0);
        let conf: f64 = results[0].confidence.into();
        assert!(
            (conf - 1.0).abs() < 1e-9,
            "distance=0 → confidence must be 1.0, got {conf}"
        );
    }

    /// Higher edit distance → lower confidence (confidence is monotonically decreasing in dist).
    #[test]
    fn test_bktree_confidence_decreases_with_distance() {
        let adapter = BkTreeFuzzyAdapter::new();
        // "fo" is dist=1 from "foo"; "f" is dist=2 from "foo".
        adapter.seed(vec!["foo".to_string(), "fo".to_string(), "f".to_string()]);

        let results = adapter.top_k("foo", 3);
        // At least the exact match and one near match should appear.
        assert!(!results.is_empty(), "must have at least one result");

        // Verify monotonically non-increasing confidence as distance grows.
        for window in results.windows(2) {
            let c0: f64 = window[0].confidence.into();
            let c1: f64 = window[1].confidence.into();
            assert!(
                c0 >= c1,
                "confidence must decrease with distance: {c0} < {c1} (dist {} vs {})",
                window[0].distance,
                window[1].distance
            );
        }
    }

    // ── T2.1-D: reseed replaces pool ─────────────────────────────────────────

    /// Re-seeding replaces the entire pool — old symbols must not appear.
    #[test]
    fn test_bktree_reseed_replaces_old_pool() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec![
            "old_symbol_alpha".to_string(),
            "old_symbol_beta".to_string(),
        ]);

        // Reseed with an entirely different pool.
        adapter.seed(vec![
            "new_symbol_gamma".to_string(),
            "new_symbol_delta".to_string(),
        ]);
        assert_eq!(adapter.len(), 2, "len() must reflect the new pool only");

        // Query for "old_symbol_alpha" — should not be in results after reseed.
        let results = adapter.top_k("old_symbol_alpha", 5);
        for r in &results {
            assert_ne!(
                r.name, "old_symbol_alpha",
                "old symbol must not appear after reseed"
            );
        }
    }

    // ── T2.1-E: top_k respects k limit ───────────────────────────────────────

    /// `top_k` must never return more than k results.
    #[test]
    fn test_bktree_top_k_respects_k_limit() {
        let adapter = seeded_adapter();

        for k in [1, 2, 3, 5] {
            let results = adapter.top_k("format", k);
            assert!(
                results.len() <= k,
                "top_k with k={k} must return <= {k} results, got {}",
                results.len()
            );
        }
    }
}
