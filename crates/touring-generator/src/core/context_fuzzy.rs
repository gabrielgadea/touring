//! Fuzzy symbol-name matching — the [`FuzzyMatcher`] trait + BK-tree adapter.
//!
//! Extracted from `core/context.rs` (F-9 modularization): a self-contained
//! BK-tree / Levenshtein fuzzy-search unit. Re-exported from `core::context`
//! so the public API (`crate::FuzzyMatcher`, `crate::BkTreeFuzzyAdapter`, …)
//! is preserved verbatim.

use crate::core::score::NormalizedScore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Fuzzy symbol name matcher — abstracts BK-tree / trigram / SIMD implementations.
///
/// PLN2 intent: `touring_simd::BkTreeFuzzy::top_k` for O(log N) suggestions.
/// Current default: `NoopFuzzyMatcher` returning empty results.
/// Wire a real implementation via `GeneratorContext::builder()` or `for_testing_with_fuzzy()`.
pub trait FuzzyMatcher: Send + Sync {
    /// Return the `k` closest symbol names to `query`, ordered by edit distance.
    fn top_k(&self, query: &str, k: usize) -> Vec<FuzzySuggestion>;
}

/// No-op fuzzy matcher — returns empty suggestions. Used in tests and as default.
pub struct NoopFuzzyMatcher;

impl FuzzyMatcher for NoopFuzzyMatcher {
    fn top_k(&self, _query: &str, _k: usize) -> Vec<FuzzySuggestion> {
        Vec::new()
    }
}

// ── BK-tree internals (simd-fuzzy) ────────────────────────────────────────────

/// A single node in the BK-tree.
///
/// Each node stores a symbol and a map from edit-distance to child node.
/// `BTreeMap` gives deterministic iteration order and compact memory layout.
#[cfg(feature = "simd-fuzzy")]
struct BkNode {
    symbol: String,
    children: std::collections::BTreeMap<usize, BkNode>,
}

/// BK-tree for O(log N) fuzzy symbol search via Levenshtein distance pruning.
///
/// Insert inserts into the tree using edit distance as the branching key.
/// Query prunes branches whose distance-window `[d-max, d+max]` cannot contain
/// candidates within `max_dist` of the query.
#[cfg(feature = "simd-fuzzy")]
struct BkTree {
    root: Option<BkNode>,
}

#[cfg(feature = "simd-fuzzy")]
impl BkTree {
    /// Construct an empty BK-tree.
    fn new() -> Self {
        Self { root: None }
    }

    /// Insert a symbol into the tree.
    ///
    /// Uses `dist_fn` to compute edit distance at each node.
    fn insert(&mut self, symbol: String, dist_fn: impl Fn(&str, &str) -> usize) {
        match self.root {
            None => {
                self.root = Some(BkNode {
                    symbol,
                    children: std::collections::BTreeMap::new(),
                });
            }
            Some(ref mut root) => {
                BkTree::insert_into(root, symbol, &dist_fn);
            }
        }
    }

    /// Recursive helper: insert `symbol` into the subtree rooted at `node`.
    fn insert_into(node: &mut BkNode, symbol: String, dist_fn: &impl Fn(&str, &str) -> usize) {
        let d = dist_fn(&symbol, &node.symbol);
        match node.children.get_mut(&d) {
            Some(child) => BkTree::insert_into(child, symbol, dist_fn),
            None => {
                node.children.insert(
                    d,
                    BkNode {
                        symbol,
                        children: std::collections::BTreeMap::new(),
                    },
                );
            }
        }
    }

    /// Collect all symbols within `max_dist` of `query` into `results`.
    ///
    /// BK-tree pruning: at each node, compute `d = dist(query, node.symbol)`.
    /// Only recurse into children with key `k` in `[d - max_dist, d + max_dist]`.
    fn query<'a>(
        &'a self,
        query: &str,
        max_dist: usize,
        dist_fn: impl Fn(&str, &str) -> usize + Copy,
        results: &mut Vec<(usize, &'a str)>,
    ) {
        if let Some(ref root) = self.root {
            BkTree::query_node(root, query, max_dist, dist_fn, results);
        }
    }

    /// Recursive helper: visit `node` and prune children outside the window.
    fn query_node<'a>(
        node: &'a BkNode,
        query: &str,
        max_dist: usize,
        dist_fn: impl Fn(&str, &str) -> usize + Copy,
        results: &mut Vec<(usize, &'a str)>,
    ) {
        let d = dist_fn(query, &node.symbol);
        if d <= max_dist {
            results.push((d, &node.symbol));
        }
        // Only recurse into children in the window [d - max_dist, d + max_dist].
        let lo = d.saturating_sub(max_dist);
        let hi = d + max_dist;
        for (&k, child) in &node.children {
            if k >= lo && k <= hi {
                BkTree::query_node(child, query, max_dist, dist_fn, results);
            }
        }
    }
}

/// BK-tree fuzzy adapter with Levenshtein distance search.
///
/// Activated under the `simd-fuzzy` feature. Uses a real BK-tree for
/// O(log N) pruned fuzzy search instead of the previous O(N) linear scan.
///
/// # BK-tree algorithm
///
/// Insert: each symbol is placed by computing `edit_dist(new, node)` and
/// descending to the child keyed by that distance (creating it if absent).
///
/// `Query(q, max_dist)`: at each node compute `d = edit_dist(q, node)`.
/// Collect node if `d <= max_dist`. Only recurse into children with key in
/// `[d - max_dist, d + max_dist]` — all other branches are provably outside
/// the tolerance ball and are pruned.
///
/// # Lazy-seed behaviour
///
/// On the first call to `top_k`, if the tree is empty and a seed has not yet
/// been attempted, the adapter spawns a `touring index search` subprocess to
/// populate itself from the daemon's live index.  If the daemon is unavailable
/// (e.g. in CI), the subprocess fails gracefully and the tree remains empty —
/// `top_k` returns an empty `Vec` without panicking.
#[cfg(feature = "simd-fuzzy")]
pub struct BkTreeFuzzyAdapter {
    /// BK-tree for O(log N) fuzzy search. Populated lazily on first query.
    tree: std::sync::Mutex<BkTree>,
    /// Tracks how many symbols have been inserted (for `len()` and stats).
    symbol_count: std::sync::atomic::AtomicUsize,
    /// Ensures CLI seed is attempted at most once, even under concurrent access.
    seed_attempted: std::sync::atomic::AtomicBool,
}

#[cfg(feature = "simd-fuzzy")]
impl Default for BkTreeFuzzyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "simd-fuzzy")]
impl BkTreeFuzzyAdapter {
    /// Construct an empty adapter.
    ///
    /// On the first `top_k` call the adapter will automatically attempt to
    /// seed itself from `touring index search` (lazy-seed).  You may also call
    /// `seed()` explicitly before the first query to skip the CLI subprocess.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tree: std::sync::Mutex::new(BkTree::new()),
            symbol_count: std::sync::atomic::AtomicUsize::new(0),
            seed_attempted: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Number of symbols currently stored in the BK-tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbol_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns `true` if the BK-tree contains no symbols.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Seed the adapter with a symbol pool (e.g. from the project index).
    ///
    /// Replaces the current BK-tree with a freshly built one from `symbols`.
    /// Thread-safe: acquires the tree mutex for the duration of the rebuild.
    pub fn seed(&self, symbols: Vec<String>) {
        let count = symbols.len();
        let mut new_tree = BkTree::new();
        for s in symbols {
            new_tree.insert(s, levenshtein_dist);
        }
        if let Ok(mut guard) = self.tree.lock() {
            *guard = new_tree;
            self.symbol_count
                .store(count, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Attempt to populate the pool via `touring index search "" -j`.
    ///
    /// Returns an empty `Vec` on any failure (daemon degraded, parse error, etc.)
    /// so that callers can degrade gracefully without panicking.
    fn load_from_cli() -> Vec<String> {
        use std::process::Command;

        let output = match Command::new("touring")
            .args(["index", "search", "", "-j"])
            .output()
        {
            Ok(o) if o.status.success() => o.stdout,
            Ok(o) => {
                tracing::warn!("touring index search returned non-zero exit: {}", o.status);
                return Vec::new();
            }
            Err(e) => {
                tracing::warn!("failed to spawn touring CLI for fuzzy seed: {e}");
                return Vec::new();
            }
        };

        Self::parse_cli_json(&output)
    }

    /// Parse the JSON bytes produced by `touring index search -j` into symbol names.
    ///
    /// Handles two shapes:
    /// - Object with `"results"` array (standard CLI response)
    /// - Plain JSON array of objects or strings (fallback)
    fn parse_cli_json(bytes: &[u8]) -> Vec<String> {
        match serde_json::from_slice::<serde_json::Value>(bytes) {
            Ok(serde_json::Value::Object(map)) => {
                // Standard touring CLI response: {"results": [...]}
                map.get("results")
                    .and_then(|v| v.as_array())
                    .map(|arr| Self::extract_names_from_array(arr.iter()))
                    .unwrap_or_default()
            }
            Ok(serde_json::Value::Array(arr)) => Self::extract_names_from_array(arr.iter()),
            Ok(_) => {
                tracing::warn!(
                    "touring index search returned unexpected JSON shape for fuzzy seed"
                );
                Vec::new()
            }
            Err(e) => {
                tracing::warn!("failed to parse touring index search JSON for fuzzy seed: {e}");
                Vec::new()
            }
        }
    }

    /// Extract symbol name strings from an iterator of JSON values.
    ///
    /// Each value may be an object with `"symbol_name"` or `"name"` field,
    /// or a plain JSON string.
    fn extract_names_from_array<'a>(
        iter: impl Iterator<Item = &'a serde_json::Value>,
    ) -> Vec<String> {
        iter.filter_map(|v| {
            v.get("symbol_name")
                .or_else(|| v.get("name"))
                .and_then(|n| n.as_str())
                .map(std::borrow::ToOwned::to_owned)
                .or_else(|| v.as_str().map(std::borrow::ToOwned::to_owned))
        })
        .collect()
    }
}

#[cfg(feature = "simd-fuzzy")]
impl FuzzyMatcher for BkTreeFuzzyAdapter {
    fn top_k(&self, query: &str, k: usize) -> Vec<FuzzySuggestion> {
        use std::sync::atomic::Ordering;

        // Lazy-seed: on first call attempt to populate the tree from the CLI.
        // `Relaxed` ordering is intentional — worst case two threads both see
        // `false` and both attempt a seed; the second seed is harmless because
        // `seed()` holds the Mutex and simply overwrites with an equivalent tree.
        if !self.seed_attempted.load(Ordering::Relaxed) {
            let is_empty = self.symbol_count.load(Ordering::Relaxed) == 0;
            // Mark attempted BEFORE the subprocess to prevent redundant concurrent calls.
            self.seed_attempted.store(true, Ordering::Relaxed);
            if is_empty {
                let names = Self::load_from_cli();
                if names.is_empty() {
                    tracing::debug!(
                        "BkTreeFuzzyAdapter lazy-seed returned empty (daemon degraded?)"
                    );
                } else {
                    tracing::debug!(
                        count = names.len(),
                        "BkTreeFuzzyAdapter lazy-seeded from touring CLI"
                    );
                    self.seed(names);
                }
            }
            // If tree was already populated (e.g. via explicit seed()), nothing to do.
        }

        if self.symbol_count.load(Ordering::Relaxed) == 0 {
            return Vec::new();
        }

        // BK-tree query: use max_dist = k.min(32) as initial search radius.
        // If too few results, widen the radius up to a cap to avoid O(N) fallback.
        // In practice, top_k with small k should need only a small radius.
        let tree = self
            .tree
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Use a generous max_dist so BK-tree returns enough candidates for top_k.
        // The caller controls k (count) not max_dist; we default to 8 which covers
        // common typo scenarios while still pruning far branches effectively.
        let max_dist: usize = 8;
        let mut candidates: Vec<(usize, &str)> = Vec::new();
        tree.query(query, max_dist, levenshtein_dist, &mut candidates);

        // Sort by distance ascending, then take first k.
        candidates.sort_unstable_by_key(|(d, _)| *d);
        candidates
            .into_iter()
            .take(k)
            .map(|(dist, name)| FuzzySuggestion {
                name: name.to_owned(),
                distance: u8::try_from(dist.min(usize::from(u8::MAX))).unwrap_or(u8::MAX),
                confidence: NormalizedScore::clamped(
                    1.0 / (1.0 + f64::from(u32::try_from(dist).unwrap_or(u32::MAX))),
                ),
            })
            .collect()
    }
}

/// Levenshtein edit distance between two strings (space-optimised, O(min(m,n)) space).
#[cfg(feature = "simd-fuzzy")]
fn levenshtein_dist(a: &str, b: &str) -> usize {
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let (m, n) = (av.len(), bv.len());
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if av[i - 1] == bv[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j].min(curr[j - 1]).min(prev[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// A fuzzy symbol suggestion from the [`FuzzyMatcher`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FuzzySuggestion {
    /// Suggested symbol name.
    pub name: String,
    /// Edit distance from the query.
    pub distance: u8,
    /// Confidence score (inverse of distance, normalised).
    pub confidence: NormalizedScore,
}

#[cfg(all(test, feature = "simd-fuzzy"))]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod fuzzy_tests {
    use super::*;

    // ── levenshtein_dist edge cases ──────────────────────────────────────────

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein_dist("", ""), 0);
    }

    #[test]
    fn levenshtein_empty_vs_nonempty() {
        assert_eq!(levenshtein_dist("", "abc"), 3);
        assert_eq!(levenshtein_dist("abc", ""), 3);
    }

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein_dist("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_single_char() {
        assert_eq!(levenshtein_dist("a", "b"), 1);
        assert_eq!(levenshtein_dist("a", "a"), 0);
    }

    #[test]
    fn levenshtein_substitution_only() {
        // "kitten" → "sitten" (1 sub)
        assert_eq!(levenshtein_dist("kitten", "sitten"), 1);
    }

    #[test]
    fn levenshtein_classic_kitten_sitting() {
        // Standard reference: distance("kitten", "sitting") = 3
        assert_eq!(levenshtein_dist("kitten", "sitting"), 3);
    }

    #[test]
    fn levenshtein_insertion_deletion() {
        assert_eq!(levenshtein_dist("abc", "ac"), 1); // delete 'b'
        assert_eq!(levenshtein_dist("ac", "abc"), 1); // insert 'b'
    }

    // ── BkTreeFuzzyAdapter integration ──────────────────────────────────────

    #[test]
    fn fuzzy_adapter_empty_pool_returns_empty() {
        let adapter = BkTreeFuzzyAdapter::new();
        let results = adapter.top_k("anything", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn fuzzy_adapter_top_k_respects_limit() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec!["foo".into(), "bar".into(), "baz".into(), "qux".into()]);
        let results = adapter.top_k("foo", 2);
        assert!(results.len() <= 2);
    }

    #[test]
    fn fuzzy_adapter_exact_match_has_zero_distance() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec!["GeneratorContext".into()]);
        let results = adapter.top_k("GeneratorContext", 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].distance, 0);
        assert_eq!(results[0].name, "GeneratorContext");
    }

    #[test]
    fn fuzzy_adapter_confidence_inversely_proportional_to_distance() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec!["exact".into(), "exaxt".into()]);
        let results = adapter.top_k("exact", 2);
        // "exact" (distance 0) should have higher confidence than "exaxt" (distance 1)
        let exact = results
            .iter()
            .find(|r| r.name == "exact")
            .expect("'exact' should be in results");
        let close = results
            .iter()
            .find(|r| r.name == "exaxt")
            .expect("'exaxt' should be in results");
        assert!(exact.confidence.value() > close.confidence.value());
    }

    // ── lazy-seed behaviour ─────────────────────────────────────────────────

    #[test]
    fn fuzzy_adapter_top_k_without_explicit_seed_is_graceful() {
        // When daemon is not available (CI), load_from_cli returns empty.
        // top_k must return empty Vec — NOT panic.
        let adapter = BkTreeFuzzyAdapter::new();
        let results = adapter.top_k("any_query", 5);
        // Either empty (daemon degraded) or non-empty (daemon available) — no panic.
        assert!(results.len() <= 5);
    }

    #[test]
    fn fuzzy_adapter_explicit_seed_takes_priority_over_lazy_seed() {
        // If the pool is populated before the first top_k call, the lazy-seed
        // subprocess must NOT overwrite it (seed_attempted stays false until top_k).
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec!["symbol_a".into(), "symbol_b".into()]);
        let results = adapter.top_k("symbol_a", 3);
        // The explicit seed should be found — exact match has distance 0.
        assert!(!results.is_empty());
        assert_eq!(results[0].distance, 0);
    }

    #[test]
    fn seed_attempted_flag_is_set_after_first_top_k() {
        use std::sync::atomic::Ordering;
        let adapter = BkTreeFuzzyAdapter::new();
        assert!(!adapter.seed_attempted.load(Ordering::Relaxed));
        let _ = adapter.top_k("anything", 1); // triggers lazy-seed attempt
        assert!(adapter.seed_attempted.load(Ordering::Relaxed));
    }

    // ── parse_cli_json unit tests ────────────────────────────────────────────

    #[test]
    fn parse_cli_json_standard_results_shape() {
        // Standard CLI response: {"count":N,"results":[{"symbol_name":"..."},...]}
        let json = br#"{"count":2,"results":[{"symbol_name":"FooBar"},{"symbol_name":"BazQux"}]}"#;
        let names = BkTreeFuzzyAdapter::parse_cli_json(json);
        assert_eq!(names, vec!["FooBar", "BazQux"]);
    }

    #[test]
    fn parse_cli_json_plain_array_shape() {
        // Fallback: plain array of objects with "name" field
        let json = br#"[{"name":"Alpha"},{"name":"Beta"}]"#;
        let names = BkTreeFuzzyAdapter::parse_cli_json(json);
        assert_eq!(names, vec!["Alpha", "Beta"]);
    }

    #[test]
    fn parse_cli_json_empty_results() {
        let json = br#"{"count":0,"results":[]}"#;
        let names = BkTreeFuzzyAdapter::parse_cli_json(json);
        assert!(names.is_empty());
    }

    #[test]
    fn parse_cli_json_invalid_json_returns_empty() {
        let names = BkTreeFuzzyAdapter::parse_cli_json(b"not json at all");
        assert!(names.is_empty());
    }

    // ── BK-tree structure tests ──────────────────────────────────────────────

    /// Empty BK-tree query returns empty results.
    #[test]
    fn bktree_empty_returns_empty() {
        let mut tree = BkTree::new();
        {
            let mut results: Vec<(usize, &str)> = Vec::new();
            tree.query("hello", 2, levenshtein_dist, &mut results);
            assert!(results.is_empty());
        }
        // Also verify insert then query on a fresh tree after query on empty.
        tree.insert("hello".to_string(), levenshtein_dist);
        {
            let mut results: Vec<(usize, &str)> = Vec::new();
            tree.query("hello", 0, levenshtein_dist, &mut results);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].0, 0);
        }
    }

    /// Exact match found with distance 0.
    #[test]
    fn bktree_exact_match_distance_zero() {
        let mut tree = BkTree::new();
        for s in ["alpha", "beta", "gamma", "delta", "epsilon"] {
            tree.insert(s.to_string(), levenshtein_dist);
        }
        let mut results = Vec::new();
        tree.query("alpha", 0, levenshtein_dist, &mut results);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 0);
        assert_eq!(results[0].1, "alpha");
    }

    /// Fuzzy match: "helo" is within distance 1 of "hello".
    #[test]
    fn bktree_fuzzy_match_one_deletion() {
        let mut tree = BkTree::new();
        for s in ["hello", "world", "rust", "cargo", "clippy"] {
            tree.insert(s.to_string(), levenshtein_dist);
        }
        let mut results = Vec::new();
        tree.query("helo", 1, levenshtein_dist, &mut results);
        let names: Vec<&str> = results.iter().map(|(_, n)| *n).collect();
        assert!(
            names.contains(&"hello"),
            "expected 'hello' in results: {names:?}"
        );
    }

    /// No candidates within tight `max_dist` returns empty.
    #[test]
    fn bktree_no_match_within_max_dist() {
        let mut tree = BkTree::new();
        for s in ["abcdefgh", "ijklmnop", "qrstuvwx"] {
            tree.insert(s.to_string(), levenshtein_dist);
        }
        let mut results = Vec::new();
        // "xyz" has distance >= 6 from all symbols above; max_dist=1 → empty
        tree.query("xyz", 1, levenshtein_dist, &mut results);
        assert!(results.is_empty(), "expected no match, got: {results:?}");
    }

    /// `BkTreeFuzzyAdapter.len()` reflects `symbol_count` correctly.
    #[test]
    fn adapter_len_reflects_seed_count() {
        let adapter = BkTreeFuzzyAdapter::new();
        assert_eq!(adapter.len(), 0);
        assert!(adapter.is_empty());
        adapter.seed(vec!["foo".into(), "bar".into(), "baz".into()]);
        assert_eq!(adapter.len(), 3);
        assert!(!adapter.is_empty());
    }

    /// BK-tree `top_k` via adapter returns correct candidates sorted by distance.
    #[test]
    fn adapter_top_k_sorted_by_distance() {
        let adapter = BkTreeFuzzyAdapter::new();
        // Insert 5 symbols at varying distances from "rust"
        // "rust" d=0, "ruse" d=1, "gust" d=2, "must" d=1, "bust" d=1
        adapter.seed(vec![
            "rust".into(),
            "ruse".into(),
            "gust".into(),
            "must".into(),
            "bust".into(),
        ]);
        let results = adapter.top_k("rust", 3);
        assert!(!results.is_empty(), "expected some results");
        // First result must be exact match
        assert_eq!(results[0].name, "rust");
        assert_eq!(results[0].distance, 0);
        // Results are in non-decreasing distance order
        let distances: Vec<u8> = results.iter().map(|r| r.distance).collect();
        let mut sorted = distances.clone();
        sorted.sort_unstable();
        assert_eq!(
            distances, sorted,
            "results not sorted by distance: {distances:?}"
        );
    }

    /// BK-tree `top_k` returns at most k results even with many candidates.
    #[test]
    fn adapter_top_k_respects_k_limit_with_bktree() {
        let adapter = BkTreeFuzzyAdapter::new();
        // 10 symbols all close to "abc"
        let symbols: Vec<String> = vec![
            "abc", "abcd", "ab", "xbc", "axc", "abx", "aXc", "Xbc", "abcX", "XYZ",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        adapter.seed(symbols);
        let results = adapter.top_k("abc", 3);
        assert!(
            results.len() <= 3,
            "expected at most 3, got {}",
            results.len()
        );
    }

    /// Seed rebuild replaces prior tree contents entirely.
    #[test]
    fn adapter_seed_replaces_tree() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec!["alpha".into(), "beta".into()]);
        assert_eq!(adapter.len(), 2);
        // Re-seed with different symbols
        adapter.seed(vec!["gamma".into(), "delta".into(), "epsilon".into()]);
        assert_eq!(adapter.len(), 3);
        // Old symbols no longer present as exact matches
        let results = adapter.top_k("alpha", 5);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            !names.contains(&"alpha"),
            "stale 'alpha' found after re-seed: {names:?}"
        );
    }

    /// BK-tree with a single symbol returns it for any `max_dist` query.
    #[test]
    fn bktree_single_symbol_root_only() {
        let mut tree = BkTree::new();
        tree.insert("singleton".to_string(), levenshtein_dist);
        let mut results = Vec::new();
        tree.query("singleton", 0, levenshtein_dist, &mut results);
        assert_eq!(results.len(), 1);
        results.clear();
        tree.query("singelton", 2, levenshtein_dist, &mut results);
        assert!(
            !results.is_empty(),
            "expected 'singleton' within distance 2 of 'singelton'"
        );
    }

    /// Confidence decreases as distance increases.
    #[test]
    fn adapter_confidence_decreases_with_distance() {
        let adapter = BkTreeFuzzyAdapter::new();
        adapter.seed(vec!["hello".into(), "helo".into(), "hlo".into()]);
        let results = adapter.top_k("hello", 3);
        // Find each result by name
        let get = |name: &str| {
            results
                .iter()
                .find(|r| r.name == name)
                .map(|r| r.confidence.value())
        };
        let exact = get("hello").expect("'hello' must be present");
        let one_off = get("helo").expect("'helo' must be present");
        let two_off = get("hlo").expect("'hlo' must be present");
        assert!(
            exact > one_off,
            "exact ({exact}) should have higher confidence than one_off ({one_off})"
        );
        assert!(
            one_off > two_off,
            "one_off ({one_off}) should have higher confidence than two_off ({two_off})"
        );
    }
}
