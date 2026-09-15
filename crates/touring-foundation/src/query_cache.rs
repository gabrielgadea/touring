//! Wave 17 (2026-04-18) — TTL-based query result cache.
//!
//! # Purpose
//!
//! Memoizes the most common read-only query handlers in the daemon
//! (`cli_index_find`, `cli_tantivy_search`, `cli_ast_meta`, …) so
//! repeated lookups within a session — extremely common during code
//! generation (VGP verifies the same symbol multiple times) and
//! editing (pre_edit reads the same metadata multiple times) — return
//! in ~1 µs instead of doing a full DB / index round-trip.
//!
//! # Architecture
//!
//! Single process-wide [`moka::sync::Cache`] keyed by a stable string
//! `query_kind::scope::canonical_payload`. TinyLFU admission policy + 60s TTL
//! gives strong hit ratios for hot symbols without serving stale data
//! across long sessions.
//!
//! - **Capacity**: 4096 entries (≈ 4 KB key + ~2 KB value avg = ~24 MB max)
//! - **TTL**: 60 s (a code generation pass typically completes in <30 s;
//!   beyond that the underlying state may have moved)
//! - **Hits/Misses**: counted in `shared::gate_metrics` under
//!   `query_cache_hit_count` and `query_cache_miss_count`
//!
//! # Invariants
//!
//! 1. Cache values are always serialized JSON. Callers do **NOT** mutate
//!    the cached String — moka shares ownership via `Arc<str>` internally.
//! 2. Keys must be canonical: identical query semantics → identical key.
//!    Helpers ([`make_key`](crate::query_cache::make_key)) enforce this.
//! 3. Cache is opt-in per call site. Hot paths wrap their compute step
//!    in [`get_or_compute`](crate::query_cache::get_or_compute); cold/write paths bypass entirely.
//! 4. Cache MUST NOT be used for queries that depend on mutable runtime
//!    state (e.g. learning EMA, gotcha lists). Only safe for index/AST
//!    reads which are stable within a session.
//! 5. Every key names its SCOPE — the root of the store the query read. One
//!    daemon serves several projects from this one cache; until 13/09/2026 the
//!    keys carried no scope, and `index status`, `index find`, `tantivy search`
//!    and `ast meta` answered one project with another's cached result for up
//!    to the TTL. [`make_key`](crate::query_cache::make_key) takes the scope, so no call site can forget it.

use moka::sync::Cache;
use std::sync::OnceLock;
use std::time::Duration;

const QUERY_CACHE_CAPACITY: u64 = 4096;
const QUERY_CACHE_TTL_SECS: u64 = 60;

static QUERY_CACHE: OnceLock<Cache<String, String>> = OnceLock::new();

fn cache() -> &'static Cache<String, String> {
    QUERY_CACHE.get_or_init(|| {
        Cache::builder()
            .max_capacity(QUERY_CACHE_CAPACITY)
            .time_to_live(Duration::from_secs(QUERY_CACHE_TTL_SECS))
            .build()
    })
}

/// Construct a canonical cache key from the `scope` (the root of the store the
/// query reads — the project root, for a daemon handler), the query kind and a
/// payload fragment. Callers should pre-normalize the payload (lowercase /
/// trim) when query semantics ignore those dimensions.
#[must_use]
pub fn make_key(scope: &std::path::Path, query_kind: &str, payload: &str) -> String {
    format!("{query_kind}::{}::{payload}", scope.display())
}

/// Look up a cached query result by key. Returns `Some(json)` on hit
/// and bumps the `query_cache_hit` gate metric, or `None` on miss
/// and bumps `query_cache_miss`.
#[must_use]
pub fn get(key: &str) -> Option<String> {
    match cache().get(key) {
        Some(v) => {
            crate::gate_metrics::record_query_cache_hit();
            Some(v)
        }
        None => {
            crate::gate_metrics::record_query_cache_miss();
            None
        }
    }
}

/// Store a query result under `key`. Overwrites existing entries
/// (last-write wins). Use when the caller is confident the value is
/// fresh — typically right after an expensive compute step.
pub fn put(key: String, value: String) {
    cache().insert(key, value);
}

/// Memoize a query computation via moka's native `get_with` — **single-flight
/// coalescing** (Wave 21, 2026-04-18 — Context7 best-practice from moka-rs).
///
/// When N threads concurrently request a missing key, only ONE thread runs
/// `compute`; the others wait for the result and receive the cached value.
/// This eliminates **cache stampede** on hot paths (e.g. `cli_ast_meta` for
/// the same file across parallel workers).
///
/// Instrumentation note: we cannot use the native `get_with` signature
/// directly because we need to record hit/miss via our gate_metrics.
/// Strategy: probe first with `get()` (bumps hit/miss counter), then fall
/// back to `get_with()` which coalesces the compute.
#[must_use]
pub fn get_or_compute<F: FnOnce() -> String>(key: &str, compute: F) -> String {
    // Fast path: hit → bumps hit counter via `get()`.
    if let Some(v) = get(key) {
        return v;
    }
    // Miss path: `get()` already bumped miss counter. Now use moka's
    // native `get_with` which guarantees single-flight compute across
    // concurrent callers requesting the same key.
    //
    // NB: a benign race exists between the probe above and the
    // `get_with` call — if another thread inserts in the gap, our
    // `compute` is skipped (desired behavior). Monotonicity of
    // hit/miss counters is preserved because `get_with` does NOT
    // re-probe `get()`.
    cache().get_with(key.to_string(), compute)
}

/// Invalidate a single cache entry. Useful after a mutation that
/// would otherwise leave the cache pointing to stale data.
pub fn invalidate(key: &str) {
    cache().invalidate(key);
}

/// Invalidate every entry of `query_kind`, in every scope. For a mutation that
/// cannot name its scope (a storage layer that does not know its project root):
/// dropping the other projects' entries too costs misses, never wrong answers.
/// Returns the number of entries invalidated.
pub fn invalidate_kind(query_kind: &str) -> u64 {
    let c = cache();
    let prefix = format!("{query_kind}::");
    let to_remove: Vec<String> = c
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .map(|(k, _)| (*k).clone())
        .collect();
    for k in &to_remove {
        c.invalidate(k);
    }
    to_remove.len() as u64
}

/// Clear the entire query cache. Reserved for tests and explicit
/// reset operations (e.g. CLI `touring health-delta reset` may want
/// to flush related queries).
pub fn clear_all() {
    cache().invalidate_all();
}

/// Wave 18 — Invalidate every cache entry whose key contains
/// `file_path` as a substring. Called by `post_edit` / `post_write`
/// after a successful edit/write so subsequent queries return fresh
/// data instead of stale cache entries.
///
/// Implementation: moka's `invalidate_entries_if` is lazy, so we use
/// `iter() + invalidate()` which guarantees immediate removal. The
/// iterator is a snapshot — concurrent inserts after iteration starts
/// are NOT visited (acceptable: they observe the new state).
///
/// Returns the number of entries invalidated (useful for tests and
/// observability).
pub fn invalidate_by_path(file_path: &str) -> u64 {
    let c = cache();
    let mut count = 0_u64;
    let to_remove: Vec<String> = c
        .iter()
        .filter_map(|(k, _v)| {
            if k.contains(file_path) {
                Some((*k).clone())
            } else {
                None
            }
        })
        .collect();
    for k in to_remove {
        c.invalidate(&k);
        count += 1;
    }
    if count > 0 {
        crate::gate_metrics::record_query_cache_invalidate(count);
    }
    count
}

/// Current number of cached entries. Useful for diagnostics and tests.
#[must_use]
pub fn entry_count() -> u64 {
    cache().entry_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialises the tests that depend on GLOBAL cache state.
    ///
    /// `clear_all()` wipes the process-wide cache, so it can land between
    /// another test's `put` and its `get` — turning an expected hit into a miss
    /// and failing the metrics assertion. Observed 2026-08-07: the metrics test
    /// passed in isolation and failed under the full parallel run, which is the
    /// signature of shared mutable state, not of a broken cache. Distinct keys
    /// are not enough when one test's operation is "erase everything".
    static GLOBAL_CACHE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Take the lock, ignoring poisoning: a panic in one test must not cascade
    /// into spurious failures in the others.
    fn global_cache_guard() -> std::sync::MutexGuard<'static, ()> {
        GLOBAL_CACHE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn make_key_concatenates_kind_scope_and_payload() {
        let scope = std::path::Path::new("/proj/a");
        assert_eq!(
            make_key(scope, "index_find", "Foo"),
            "index_find::/proj/a::Foo"
        );
        assert_eq!(
            make_key(scope, "tantivy_search", ""),
            "tantivy_search::/proj/a::"
        );
        assert_ne!(
            make_key(scope, "index_find", "Foo"),
            make_key(std::path::Path::new("/proj/b"), "index_find", "Foo"),
            "the same query in two projects is two entries"
        );
    }

    #[test]
    fn invalidate_kind_drops_that_kind_in_every_scope_only() {
        let _guard = global_cache_guard();
        let a = make_key(std::path::Path::new("/kind/a"), "kind_probe_9d", "v1");
        let b = make_key(std::path::Path::new("/kind/b"), "kind_probe_9d", "v1");
        let other = make_key(std::path::Path::new("/kind/a"), "kind_probe_9e", "v1");
        for k in [&a, &b, &other] {
            put(k.clone(), "x".to_string());
        }
        assert!(invalidate_kind("kind_probe_9d") >= 2);
        assert!(get(&a).is_none() && get(&b).is_none());
        assert!(get(&other).is_some(), "another kind stays");
    }

    #[test]
    fn get_returns_none_for_missing_key() {
        let _guard = global_cache_guard();
        let key = "wave17::nonexistent_42";
        invalidate(key);
        assert_eq!(get(key), None);
    }

    #[test]
    fn put_then_get_returns_value() {
        let _guard = global_cache_guard();
        let key = "wave17::put_get_test";
        invalidate(key);
        put(key.to_string(), r#"{"hello":"world"}"#.to_string());
        assert_eq!(get(key), Some(r#"{"hello":"world"}"#.to_string()));
    }

    #[test]
    fn get_or_compute_hits_on_second_call() {
        let _guard = global_cache_guard();
        let key = "wave17::compute_hit";
        invalidate(key);
        let mut compute_calls = 0;
        let mut compute = || {
            compute_calls += 1;
            r#"{"computed":true}"#.to_string()
        };

        // First call: miss → compute runs → result stored.
        let v1 = get_or_compute(key, &mut compute);
        assert_eq!(v1, r#"{"computed":true}"#);
        // Second call: hit → compute does NOT run.
        let v2 = get_or_compute(key, &mut compute);
        assert_eq!(v2, r#"{"computed":true}"#);
        assert_eq!(compute_calls, 1, "compute must run only on miss");
    }

    #[test]
    fn get_or_compute_single_flight_under_concurrency() {
        let _guard = global_cache_guard();
        // Wave 21: Context7 moka best practice — `get_with` coalesces
        // concurrent compute calls on the same missing key.
        // Before Wave 21: 16 threads requesting same missing key ran
        // compute 16 times (cache stampede).
        // After Wave 21: compute runs EXACTLY ONCE.
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::thread;

        let key = "wave21::single_flight_test";
        invalidate(key);
        let counter = Arc::new(AtomicU32::new(0));

        let handles: Vec<_> = (0..16)
            .map(|_| {
                let counter = Arc::clone(&counter);
                let key = key.to_string();
                thread::spawn(move || {
                    get_or_compute(&key, || {
                        counter.fetch_add(1, Ordering::SeqCst);
                        // Simulate expensive compute so threads overlap.
                        thread::sleep(std::time::Duration::from_millis(50));
                        r#"{"coalesced":true}"#.to_string()
                    })
                })
            })
            .collect();

        for h in handles {
            let v = h.join().expect("thread panicked");
            assert_eq!(v, r#"{"coalesced":true}"#, "all threads see same value");
        }

        let calls = counter.load(Ordering::SeqCst);
        // moka's get_with guarantees ≤1 compute per missing key under
        // concurrent access. Allow 1 (strict single-flight).
        assert_eq!(
            calls, 1,
            "compute must run exactly once under concurrency, got {calls}"
        );
    }

    #[test]
    fn invalidate_drops_entry() {
        let _guard = global_cache_guard();
        let key = "wave17::invalidate_test";
        put(key.to_string(), "value".to_string());
        assert!(get(key).is_some());
        invalidate(key);
        assert_eq!(get(key), None);
    }

    #[test]
    fn clear_all_drops_everything() {
        let _guard = global_cache_guard();
        put("wave17::clear_a".to_string(), "a".to_string());
        put("wave17::clear_b".to_string(), "b".to_string());
        clear_all();
        assert_eq!(get("wave17::clear_a"), None);
        assert_eq!(get("wave17::clear_b"), None);
    }

    // ── Wave 18 — invalidate_by_path ────────────────────────────────────

    #[test]
    fn invalidate_by_path_removes_only_matching_keys() {
        let _guard = global_cache_guard();
        // Prime cache with 3 entries: 2 contain `/wave18/target.rs`, 1 doesn't.
        let target = "/wave18/target.rs";
        let other = "/wave18/other.rs";
        let scope = std::path::Path::new("/wave18");
        let key_a = make_key(scope, "cli_ast_meta", &format!("{target}|skeleton"));
        let key_b = make_key(scope, "cli_ast_blast", target);
        let key_c = make_key(scope, "cli_ast_meta", &format!("{other}|skeleton"));
        put(key_a.clone(), "a".to_string());
        put(key_b.clone(), "b".to_string());
        put(key_c.clone(), "c".to_string());
        assert!(get(&key_a).is_some());
        assert!(get(&key_b).is_some());
        assert!(get(&key_c).is_some());

        let removed = invalidate_by_path(target);
        assert!(
            removed >= 2,
            "must remove >= 2 entries containing target, got {removed}"
        );
        // Target keys gone; non-matching key still present.
        assert!(get(&key_a).is_none(), "key_a (target) must be invalidated");
        assert!(get(&key_b).is_none(), "key_b (target) must be invalidated");
        assert!(get(&key_c).is_some(), "key_c (other path) must remain");
    }

    /// `post_edit` invalidates by the RELATIVE path: it must reach a key built
    /// from the relative path (`ast meta`) and one built from the absolute path
    /// (`ast overview` keys whatever the caller sent).
    #[test]
    fn invalidate_by_relative_path_reaches_relative_and_absolute_keys() {
        let _guard = global_cache_guard();
        let scope = std::path::Path::new("/proj/rel9f");
        let relative = make_key(scope, "cli_ast_meta", "src/rel9f.rs|summary");
        let absolute = make_key(scope, "cli_ast_overview", "/proj/rel9f/src/rel9f.rs");
        put(relative.clone(), "r".to_string());
        put(absolute.clone(), "a".to_string());
        assert!(invalidate_by_path("src/rel9f.rs") >= 2);
        assert!(get(&relative).is_none(), "relative key must go");
        assert!(get(&absolute).is_none(), "absolute key must go");
    }

    #[test]
    fn invalidate_by_path_returns_zero_when_no_match() {
        let _guard = global_cache_guard();
        let removed = invalidate_by_path("/wave18/never_indexed.rs");
        assert_eq!(removed, 0, "no match must return 0");
    }

    #[test]
    fn invalidate_by_path_increments_counter() {
        let _guard = global_cache_guard();
        use std::sync::atomic::Ordering;
        let path = "/wave18/counter_test.rs";
        let key = make_key(
            std::path::Path::new("/wave18"),
            "cli_ast_meta",
            &format!("{path}|skeleton"),
        );

        let before = crate::gate_metrics::global()
            .query_cache_invalidate_count
            .load(Ordering::Relaxed);
        // The cache is process-global: a concurrent test can evict the entry
        // between the put and the invalidate (observed flaking 2026-08-23).
        // Bounded retries — the put→invalidate window is nanoseconds, and one
        // successful round is all the counter contract needs.
        let mut removed = 0u64;
        for _ in 0..10 {
            put(key.clone(), "v".to_string());
            removed = invalidate_by_path(path);
            if removed >= 1 {
                break;
            }
        }
        assert!(removed >= 1);
        let after = crate::gate_metrics::global()
            .query_cache_invalidate_count
            .load(Ordering::Relaxed);
        assert!(after >= before + 1, "invalidate counter must advance");
    }

    #[test]
    fn cache_metrics_advance_on_hit_and_miss() {
        use std::sync::atomic::Ordering;
        // Held across the whole hit/miss sequence: a concurrent `clear_all()`
        // between the `put` and the second `get` turns the expected hit into a
        // miss.
        let _guard = global_cache_guard();
        let key = "wave17::metrics_test";
        invalidate(key);

        let hits_before = crate::gate_metrics::global()
            .query_cache_hit_count
            .load(Ordering::Relaxed);
        let misses_before = crate::gate_metrics::global()
            .query_cache_miss_count
            .load(Ordering::Relaxed);

        // Miss → bumps misses.
        let _ = get(key);
        // Insert + Hit → bumps hits.
        put(key.to_string(), "v".to_string());
        let _ = get(key);

        let hits_after = crate::gate_metrics::global()
            .query_cache_hit_count
            .load(Ordering::Relaxed);
        let misses_after = crate::gate_metrics::global()
            .query_cache_miss_count
            .load(Ordering::Relaxed);
        assert!(hits_after > hits_before, "hit counter must advance");
        assert!(misses_after > misses_before, "miss counter must advance");
    }
}
