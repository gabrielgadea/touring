//! IC-3: PredictiveFocusCache — ACO pheromone-guided prefetch for FocusCache.
//!
//! Records file co-access events as pheromone trails, then ranks likely
//! next-access files so callers can warm the LRU cache before they are
//! actually requested.
//!
//! # How it works
//!
//! 1. Each time two files are accessed together, call `observe_co_access(a, b)`.
//!    This deposits pheromone on both `(a→b)` and `(b→a)` edges inside an
//!    internal `SymbolPheromoneMap`, capped at the map's maximum (10.0).
//! 2. When switching to file X, call `prefetch_likely(X, candidates, k)`.
//!    It ranks `candidates` by pheromone strength relative to X and returns
//!    the top-k paths (excluding X itself).
//! 3. Callers may then warm the inner `FocusCache` via `prefetch_context(path, ctx)`
//!    to avoid cold misses when those files are actually opened.
//! 4. Periodic `evaporate()` calls fade stale trails so recent co-access patterns
//!    dominate over historical ones.
//!
//! # Thread safety
//!
//! `PredictiveFocusCache` is `Send + Sync`.  The inner `FocusCache` uses
//! `RwLock`; the pheromone map uses `Arc<RwLock<…>>` so it can be shared
//! with other components (e.g. a global session-level map).

use std::sync::{Arc, RwLock};

use touring_code::ast::graph::SymbolPheromoneMap;

use crate::reasoning::focus_cache::FocusCache;

/// IC-3: Pheromone-guided LRU cache that ranks likely co-accessed files.
///
/// Wraps [`FocusCache`] with a [`SymbolPheromoneMap`] tracking which files
/// are frequently co-accessed.  The pheromone map can be shared with other
/// components via [`pheromone_map()`](Self::pheromone_map).
pub struct PredictiveFocusCache {
    /// Inner LRU cache for graph-context strings (capacity 16).
    cache: FocusCache,
    /// Co-access pheromone map (file paths used as "symbol" identifiers).
    pheromone: Arc<RwLock<SymbolPheromoneMap>>,
}

impl PredictiveFocusCache {
    /// Create a new `PredictiveFocusCache` with a fresh pheromone map.
    ///
    /// `evaporation_rate` controls trail decay per [`evaporate()`](Self::evaporate):
    /// - `0.0` — no decay (trails accumulate indefinitely)
    /// - `0.1` — 10 % decay per tick (typical session setting)
    /// - `1.0` — full reset per tick
    pub fn new(evaporation_rate: f64) -> Self {
        Self {
            cache: FocusCache::new(),
            pheromone: Arc::new(RwLock::new(SymbolPheromoneMap::new(evaporation_rate))),
        }
    }

    /// Create from an externally-managed pheromone map (shared with other components).
    pub fn with_pheromone(pheromone: Arc<RwLock<SymbolPheromoneMap>>) -> Self {
        Self {
            cache: FocusCache::new(),
            pheromone,
        }
    }

    /// Record a co-access event between two file paths.
    ///
    /// Reinforces pheromone on both `(from → to)` and `(to → from)` edges,
    /// capped at the map's maximum (10.0).  Self-access (`from == to`) is silently
    /// ignored by the underlying map.
    pub fn observe_co_access(&self, from: &str, to: &str) {
        let mut ph = self.pheromone.write().unwrap_or_else(|e| e.into_inner());
        ph.record_co_access(from, to);
    }

    /// Return the top-`top_k` files from `known_files` most likely to be accessed
    /// next, given `current_file` as context.
    ///
    /// Results are sorted descending by pheromone strength.  `current_file` itself
    /// is excluded.  Returns an empty `Vec` when `known_files` is empty or
    /// `top_k` is zero.
    pub fn prefetch_likely(
        &self,
        current_file: &str,
        known_files: &[String],
        top_k: usize,
    ) -> Vec<String> {
        if known_files.is_empty() || top_k == 0 {
            return Vec::new();
        }
        let ph = self.pheromone.read().unwrap_or_else(|e| e.into_inner());
        let mut ranked: Vec<(f64, &String)> = known_files
            .iter()
            .filter(|f| f.as_str() != current_file)
            .map(|f| (ph.pheromone_strength(current_file, f.as_str()), f))
            .collect();
        // Descending by pheromone strength; equal strengths keep original order.
        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        ranked
            .into_iter()
            .take(top_k)
            .map(|(_, f)| f.clone())
            .collect()
    }

    /// Get or compute context for `key`, delegating to the inner [`FocusCache`].
    pub fn get_or_compute<F>(&self, key: &str, compute_fn: F) -> String
    where
        F: FnOnce() -> String,
    {
        self.cache.get_or_compute(key, compute_fn)
    }

    /// Warm the inner cache for `key` without computing.
    ///
    /// Only inserts if the key is not already present (does not overwrite
    /// fresher data).  Does not affect hit/miss counters.
    pub fn prefetch_context(&self, key: &str, context: String) {
        self.cache.prefetch(key, context);
    }

    /// Apply pheromone evaporation — fades stale co-access trails.
    pub fn evaporate(&self) {
        let mut ph = self.pheromone.write().unwrap_or_else(|e| e.into_inner());
        ph.evaporate();
    }

    /// Access the inner [`FocusCache`] for metrics or direct cache operations.
    pub fn cache(&self) -> &FocusCache {
        &self.cache
    }

    /// Return a clone of the shared pheromone map for external coordination.
    pub fn pheromone_map(&self) -> Arc<RwLock<SymbolPheromoneMap>> {
        Arc::clone(&self.pheromone)
    }
}

impl Default for PredictiveFocusCache {
    /// Default evaporation rate: 5 % per tick — suitable for session-length use.
    fn default() -> Self {
        Self::new(0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // ── prefetch_likely ────────────────────────────────────────────────────

    #[test]
    fn test_prefetch_ranks_by_co_access_frequency() {
        let pfc = PredictiveFocusCache::new(0.0); // no evaporation
        let known = files(&["a.rs", "b.rs", "c.rs", "d.rs"]);

        // Record: a+b frequently, a+c sometimes, a+d once
        for _ in 0..5 {
            pfc.observe_co_access("a.rs", "b.rs");
        }
        for _ in 0..3 {
            pfc.observe_co_access("a.rs", "c.rs");
        }
        pfc.observe_co_access("a.rs", "d.rs");

        let likely = pfc.prefetch_likely("a.rs", &known, 2);
        assert_eq!(likely.len(), 2);
        assert_eq!(likely[0], "b.rs", "b.rs should be top (most co-accesses)");
        assert_eq!(likely[1], "c.rs", "c.rs should be second");
    }

    #[test]
    fn test_prefetch_excludes_current_file() {
        let pfc = PredictiveFocusCache::new(0.0);
        let known = files(&["x.rs", "y.rs"]);
        pfc.observe_co_access("x.rs", "y.rs");

        let result = pfc.prefetch_likely("x.rs", &known, 5);
        assert!(
            !result.contains(&"x.rs".to_string()),
            "current file must be excluded"
        );
        assert!(result.contains(&"y.rs".to_string()));
    }

    #[test]
    fn test_prefetch_empty_known_files_returns_empty() {
        let pfc = PredictiveFocusCache::new(0.0);
        assert!(pfc.prefetch_likely("any.rs", &[], 3).is_empty());
    }

    #[test]
    fn test_prefetch_top_k_zero_returns_empty() {
        let pfc = PredictiveFocusCache::new(0.0);
        let known = files(&["a.rs"]);
        assert!(pfc.prefetch_likely("a.rs", &known, 0).is_empty());
    }

    #[test]
    fn test_prefetch_top_k_larger_than_candidates() {
        let pfc = PredictiveFocusCache::new(0.0);
        let known = files(&["a.rs", "b.rs"]);
        pfc.observe_co_access("a.rs", "b.rs");
        // top_k=10 but only 1 candidate other than current
        let result = pfc.prefetch_likely("a.rs", &known, 10);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_prefetch_unknown_current_file_returns_zero_strength_order() {
        // No co-access recorded — all strengths are 0.0, order is arbitrary
        let pfc = PredictiveFocusCache::new(0.0);
        let known = files(&["a.rs", "b.rs", "c.rs"]);
        let result = pfc.prefetch_likely("x.rs", &known, 3);
        // Should return all 3 candidates (all with 0.0 strength), no panic
        assert_eq!(result.len(), 3);
    }

    // ── cache delegation ───────────────────────────────────────────────────

    #[test]
    fn test_get_or_compute_delegates_to_inner() {
        let pfc = PredictiveFocusCache::new(0.0);
        let r1 = pfc.get_or_compute("f.rs", || "ctx_f".to_string());
        let r2 = pfc.get_or_compute("f.rs", || "should_not_be_called".to_string());
        assert_eq!(r1, "ctx_f");
        assert_eq!(r2, "ctx_f"); // cache hit
        assert_eq!(pfc.cache().hit_count(), 1);
        assert_eq!(pfc.cache().miss_count(), 1);
    }

    #[test]
    fn test_prefetch_context_warms_cache() {
        let pfc = PredictiveFocusCache::new(0.0);
        pfc.prefetch_context("warm.rs", "warmed context".to_string());

        let mut called = false;
        let result = pfc.get_or_compute("warm.rs", || {
            called = true;
            "cold".to_string()
        });
        assert!(
            !called,
            "compute_fn should not be called for prefetched key"
        );
        assert_eq!(result, "warmed context");
    }

    // ── evaporation ────────────────────────────────────────────────────────

    #[test]
    fn test_evaporate_reduces_pheromone() {
        let pfc = PredictiveFocusCache::new(0.5); // 50% decay per tick
        let known = files(&["a.rs", "b.rs"]);
        // Record 10 co-accesses → strength = 10.0 (capped at cap=10)
        for _ in 0..10 {
            pfc.observe_co_access("a.rs", "b.rs");
        }

        // Strength before evaporation: 10.0 → b.rs ranked first
        let before = pfc.prefetch_likely("a.rs", &known, 1);
        assert_eq!(before.first().map(String::as_str), Some("b.rs"));

        pfc.evaporate(); // 50% decay: 10.0 * 0.5 = 5.0 (still > prune_threshold)

        // Still ranked (strength decayed but not pruned)
        let after = pfc.prefetch_likely("a.rs", &known, 1);
        assert_eq!(after.first().map(String::as_str), Some("b.rs"));
    }

    #[test]
    fn test_evaporate_full_rate_prunes_trails() {
        let pfc = PredictiveFocusCache::new(1.0); // full reset per tick
        let known = files(&["a.rs", "b.rs"]);
        pfc.observe_co_access("a.rs", "b.rs");

        pfc.evaporate(); // strength → 0.0 → pruned (below threshold 0.001)

        // pheromone_strength should be 0.0 now — but prefetch_likely still returns
        // b.rs (just with zero strength, meaning arbitrary order)
        let _ = pfc.prefetch_likely("a.rs", &known, 1); // no panic
    }

    // ── shared pheromone map ───────────────────────────────────────────────

    #[test]
    fn test_shared_pheromone_map_propagates() {
        let pfc1 = PredictiveFocusCache::new(0.0);
        let shared = pfc1.pheromone_map();

        // Observe via pfc1
        pfc1.observe_co_access("a.rs", "b.rs");

        // Create pfc2 sharing the same map
        let pfc2 = PredictiveFocusCache::with_pheromone(Arc::clone(&shared));
        let known = files(&["a.rs", "b.rs"]);
        let result = pfc2.prefetch_likely("a.rs", &known, 1);

        assert_eq!(
            result.first().map(String::as_str),
            Some("b.rs"),
            "pfc2 should see pheromone deposited by pfc1 via shared map"
        );
    }

    // ── default ────────────────────────────────────────────────────────────

    #[test]
    fn test_default_is_empty() {
        let pfc = PredictiveFocusCache::default();
        assert!(pfc.cache().is_empty());
        let result = pfc.prefetch_likely("a.rs", &files(&["b.rs"]), 1);
        // No co-access recorded — returns b.rs with 0.0 strength
        assert_eq!(result.len(), 1);
    }
}
