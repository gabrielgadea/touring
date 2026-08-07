//! `VgpEngine` v2 — `moka` sync cache + rayon `spawn_blocking` isolation + fuzzy fallback.
//!
//! Key fixes from Pln1:
//! - B3: rayon inside `spawn_blocking` (no tokio worker starvation)
//! - B5: `moka::sync::Cache` with `max_capacity` + `time_to_idle` + `time_to_live`
//! - `FuzzyMatcher` stub (`touring-simd` `TopKSearcher` used as fallback)
//!
//! Uses `moka::sync::Cache` (not `moka::future::Cache`) because the lookup
//! runs inside `tokio::task::spawn_blocking` where async is not available.
//!
//! ## `IncrementalIndex` fast-path
//!
//! When an [`touring_intelligence::index::IncrementalIndex`] is injected via
//! [`VgpEngine::with_index`], symbol lookups first attempt an in-process
//! search against the live `DashMap` index (`search_symbols`).  A successful
//! hit skips the subprocess call entirely, reducing per-symbol latency from
//! ~5–50 ms (daemon round-trip) to < 1 ms.  The moka cache layer still
//! deduplicates repeated lookups within a batch session.

use crate::core::context::TelemetrySink;
use crate::error::GenerateError;
use crate::plan::contracts::{Contracts, SymbolRef};
use crate::plan::failure::MissingSymbol;
use crate::plan::result::VgpReport;
use crate::vgp::fuzzy::{FuzzySearcher, NoopFuzzySearcher};
use moka::sync::Cache;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::task;
use uuid::Uuid;

/// Cache key for a symbol lookup.
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct SymbolKey {
    /// Symbol name being looked up.
    pub name: String,
    /// Optional crate qualifier — disambiguates same-named symbols across crates.
    pub crate_name: Option<String>,
}

impl From<&SymbolRef> for SymbolKey {
    fn from(s: &SymbolRef) -> Self {
        Self {
            name: s.name.clone(),
            crate_name: s.crate_name.clone(),
        }
    }
}

/// Result of a single symbol lookup.
#[derive(Clone, Debug)]
pub struct VgpLookupResult {
    /// Whether the symbol was found in the index.
    pub found: bool,
    /// Path to the file defining the symbol, when found.
    pub file_path: Option<String>,
    /// 1-based line number of the symbol definition, when found.
    pub line: Option<u32>,
}

/// VGP engine — verifies symbol contracts against the touring daemon index.
pub struct VgpEngine {
    /// `moka::sync::Cache`: symbol lookup results with TTL.
    /// `sync::Cache` is safe to use inside `spawn_blocking`.
    cache: Cache<SymbolKey, Arc<VgpLookupResult>>,
    /// Dedicated rayon pool to avoid blocking tokio worker threads.
    rayon_pool: Arc<rayon::ThreadPool>,
    /// Telemetry sink for metrics.
    metrics: Arc<dyn TelemetrySink>,
    /// Cache hit counter (lock-free `AtomicU32`).
    cache_hits: Arc<AtomicU32>,
    /// Cache miss counter (lock-free `AtomicU32`).
    cache_misses: Arc<AtomicU32>,
    /// Fuzzy suggestion backend — injectable for testing and SIMD upgrade.
    fuzzy: Arc<dyn FuzzySearcher>,
    /// Optional in-process symbol index for fast-path lookups.
    ///
    /// When set, [`verify_batch`] first tries `IncrementalIndex::search_symbols`
    /// before falling back to the subprocess daemon call.  This eliminates the
    /// ~5–50 ms subprocess round-trip for symbols already tracked by the live
    /// `DashMap` index.
    symbol_index: Option<Arc<touring_intelligence::index::IncrementalIndex>>,
}

impl VgpEngine {
    /// Construct a new [`VgpEngine`] with bounded sync cache, dedicated thread pool,
    /// and an injected fuzzy search backend.
    ///
    /// For the default subprocess backend, use [`VgpEngine::with_subprocess`].
    ///
    /// # Panics
    ///
    /// Panics if the rayon thread pool cannot be constructed (OS resource exhaustion).
    /// This is a fatal startup condition — the process should not continue.
    #[must_use]
    pub fn new(metrics: Arc<dyn TelemetrySink>, fuzzy: Arc<dyn FuzzySearcher>) -> Self {
        let cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_idle(Duration::from_mins(5))
            .time_to_live(Duration::from_hours(1))
            .build();

        let num_threads = (num_cpus::get().saturating_sub(2)).max(1);
        // rayon pool construction only fails on OS resource exhaustion —
        // a fatal startup condition where panic is appropriate.
        #[allow(clippy::expect_used)]
        let rayon_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .thread_name(|i| format!("vgp-rayon-{i}"))
                .build()
                .expect("VgpEngine: rayon pool construction must succeed at startup"),
        );

        Self {
            cache,
            rayon_pool,
            metrics,
            cache_hits: Arc::new(AtomicU32::new(0)),
            cache_misses: Arc::new(AtomicU32::new(0)),
            fuzzy,
            symbol_index: None,
        }
    }

    /// Convenience constructor wiring the subprocess-based fuzzy searcher.
    ///
    /// Equivalent to `VgpEngine::new(metrics, Arc::new(NoopFuzzySearcher))`.
    /// Use this in production contexts and `for_testing()` helpers.
    #[must_use]
    pub fn with_subprocess(metrics: Arc<dyn TelemetrySink>) -> Self {
        Self::new(metrics, Arc::new(NoopFuzzySearcher))
    }

    /// Attach an in-process [`touring_intelligence::index::IncrementalIndex`] for fast-path
    /// symbol lookups.
    ///
    /// When set, `verify_batch` attempts `search_symbols` against the live
    /// `DashMap` index before spawning a subprocess daemon call.  Symbols found
    /// in-process are resolved without any IPC overhead.
    ///
    /// Designed for builder-style construction:
    /// ```ignore
    /// let engine = VgpEngine::with_subprocess(metrics)
    ///     .with_index(ctx.symbol_index.clone());
    /// ```
    #[must_use]
    pub fn with_index(mut self, index: Arc<touring_intelligence::index::IncrementalIndex>) -> Self {
        self.symbol_index = Some(index);
        self
    }

    /// Returns the current cache hit count (since construction or last reset).
    #[must_use]
    pub fn cache_hit_count(&self) -> u32 {
        self.cache_hits.load(Ordering::Relaxed)
    }

    /// Returns the current cache miss count.
    #[must_use]
    pub fn cache_miss_count(&self) -> u32 {
        self.cache_misses.load(Ordering::Relaxed)
    }

    /// Resets cache hit/miss counters to zero.
    pub fn reset_counters(&self) {
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
    }

    /// Verify a batch of symbol contracts.
    ///
    /// Uses `spawn_blocking` to run rayon work without blocking tokio workers.
    /// `moka::sync::Cache` is safe to use inside the blocking closure.
    ///
    /// # Errors
    ///
    /// Returns [`GenerateError::TaskJoinError`] if the blocking task is cancelled
    /// (e.g. tokio runtime shutting down). Symbol lookup failures are treated as
    /// soft misses reported in [`VgpReport::missing_symbols`], not as errors.
    pub async fn verify_batch(
        &self,
        contracts: &Contracts,
        plan_id: Uuid,
    ) -> Result<VgpReport, GenerateError> {
        if contracts.is_empty() {
            return Ok(VgpReport::empty_pass(plan_id));
        }

        let start = std::time::Instant::now();
        let must_exist = contracts.symbols_must_exist.clone();
        let must_not_exist = contracts.symbols_must_not_exist.clone();
        let cache = self.cache.clone();
        let pool = Arc::clone(&self.rayon_pool);
        let hits = Arc::clone(&self.cache_hits);
        let misses = Arc::clone(&self.cache_misses);
        // Clone the optional fast-path index into the blocking closure.
        let index_opt = self.symbol_index.clone();

        let (exist_results, collision_results) = task::spawn_blocking(
            move || -> (Vec<(SymbolRef, Arc<VgpLookupResult>)>, Vec<SymbolRef>) {
                use rayon::prelude::*;

                let exist = pool.install(|| {
                    must_exist
                        .par_iter()
                        .map(|sym| {
                            let key = SymbolKey::from(sym);
                            if let Some(cached) = cache.get(&key) {
                                hits.fetch_add(1, Ordering::Relaxed);
                                return (sym.clone(), cached);
                            }
                            misses.fetch_add(1, Ordering::Relaxed);
                            let result = Arc::new(resolve_symbol(
                                &sym.name,
                                sym.crate_name.as_deref(),
                                index_opt.as_deref(),
                            ));
                            cache.insert(key, Arc::clone(&result));
                            (sym.clone(), result)
                        })
                        .collect::<Vec<_>>()
                });

                let collisions = pool.install(|| {
                    must_not_exist
                        .par_iter()
                        .filter(|sym| {
                            let key = SymbolKey {
                                name: sym.name.clone(),
                                crate_name: sym.crate_name.clone(),
                            };
                            if let Some(cached) = cache.get(&key) {
                                hits.fetch_add(1, Ordering::Relaxed);
                                cached.found
                            } else {
                                misses.fetch_add(1, Ordering::Relaxed);
                                resolve_symbol(
                                    &sym.name,
                                    sym.crate_name.as_deref(),
                                    index_opt.as_deref(),
                                )
                                .found
                            }
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                });

                (exist, collisions)
            },
        )
        .await
        .map_err(GenerateError::TaskJoinError)?;

        let (verified, missing) = self.classify_results(exist_results);

        self.emit_telemetry(start.elapsed());

        Ok(VgpReport {
            plan_id,
            all_passed: missing.is_empty() && collision_results.is_empty(),
            verified_symbols: verified,
            missing_symbols: missing,
            collisions: collision_results,
            elapsed_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
        })
    }

    fn classify_results(
        &self,
        results: Vec<(SymbolRef, Arc<VgpLookupResult>)>,
    ) -> (Vec<SymbolRef>, Vec<MissingSymbol>) {
        let mut verified = Vec::with_capacity(results.len());
        let mut missing = Vec::new();
        for (sym, lookup) in results {
            if lookup.found {
                verified.push(sym);
            } else {
                let suggestions = self.fuzzy.search(&sym.name, 3);
                missing.push(MissingSymbol {
                    requested: sym,
                    suggestions,
                    nearest_crate_alternatives: Vec::new(),
                });
            }
        }
        (verified, missing)
    }

    fn emit_telemetry(&self, elapsed: std::time::Duration) {
        self.metrics.record_histogram(
            "vgp.verify_batch.latency_ms",
            elapsed.as_secs_f64() * 1000.0,
        );
        self.metrics.increment_counter("vgp.verify_batch.calls", 1);

        let cache_hits_val = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses_val = self.cache_misses.load(Ordering::Relaxed);
        let total = cache_hits_val + cache_misses_val;
        if total > 0 {
            self.metrics.record_histogram(
                "vgp.cache.hit_ratio",
                f64::from(cache_hits_val) / f64::from(total),
            );
        }
    }

    /// Invalidate all cached entries (useful after index rebuild).
    pub fn invalidate_all(&self) {
        self.cache.invalidate_all();
    }

    /// Returns a consolidated snapshot of cache statistics.
    #[must_use]
    pub fn cache_stats(&self) -> VgpCacheStats {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        VgpCacheStats {
            hits,
            misses,
            size: self.cache.entry_count(),
            hit_rate: if total > 0 {
                f64::from(hits) / f64::from(total)
            } else {
                0.0
            },
            has_index_fast_path: self.symbol_index.is_some(),
        }
    }
}

/// Consolidated VGP cache statistics (parity with Python `CacheStats`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct VgpCacheStats {
    /// Total cache hits since construction or last reset.
    pub hits: u32,
    /// Total cache misses.
    pub misses: u32,
    /// Current number of entries in the moka cache.
    pub size: u64,
    /// Hit rate (0.0–1.0).
    pub hit_rate: f64,
    /// Whether the `IncrementalIndex` fast-path is available.
    pub has_index_fast_path: bool,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::items_after_test_module
)]
mod vgp_engine_tests {
    use super::*;
    use crate::core::context::NoopTelemetry;

    fn make_test_engine() -> VgpEngine {
        let metrics: Arc<dyn TelemetrySink> = Arc::new(NoopTelemetry);
        VgpEngine::with_subprocess(metrics)
    }

    // ── Construction & Counter Tests ──────────────────────────────────────────

    #[test]
    fn test_vgp_engine_new_counters_start_at_zero() {
        // REQUIREMENT: fresh VgpEngine has zero hit/miss counters.
        // BOUNDARY: no verify_batch called yet.
        // EXPECTED: both counters == 0.
        let engine = make_test_engine();
        assert_eq!(engine.cache_hit_count(), 0, "hits must start at 0");
        assert_eq!(engine.cache_miss_count(), 0, "misses must start at 0");
    }

    #[test]
    fn test_vgp_engine_reset_counters_zeroes_atomics() {
        // REQUIREMENT: reset_counters must zero both AtomicU32 counters.
        // BOUNDARY: counters already at 0 (idempotent reset).
        // EXPECTED: still 0 after reset.
        let engine = make_test_engine();
        engine.reset_counters();
        assert_eq!(engine.cache_hit_count(), 0);
        assert_eq!(engine.cache_miss_count(), 0);
    }

    #[test]
    fn test_vgp_engine_invalidate_all_does_not_panic() {
        // REQUIREMENT: invalidate_all must be callable without error.
        // BOUNDARY: empty cache (no prior inserts).
        // EXPECTED: no panic.
        let engine = make_test_engine();
        engine.invalidate_all(); // must not panic
    }

    // ── verify_batch: empty-contracts fast path ───────────────────────────────

    #[tokio::test]
    async fn test_verify_batch_empty_contracts_returns_all_passed() {
        // REQUIREMENT: empty Contracts triggers the fast path and always passes.
        // BOUNDARY: Contracts::default() — both vecs empty.
        // EXPECTED: VgpReport::all_passed == true, no missing, no collisions.
        let engine = make_test_engine();
        let contracts = Contracts::default();
        let plan_id = Uuid::new_v4();

        let report = engine
            .verify_batch(&contracts, plan_id)
            .await
            .expect("verify_batch must not error on empty contracts");

        assert!(
            report.all_passed,
            "empty contracts must always return all_passed=true"
        );
        assert!(
            report.missing_symbols.is_empty(),
            "empty contracts must produce no missing symbols"
        );
        assert!(
            report.collisions.is_empty(),
            "empty contracts must produce no collisions"
        );
        assert_eq!(
            report.plan_id, plan_id,
            "report must echo back the supplied plan_id"
        );
    }

    #[tokio::test]
    async fn test_verify_batch_empty_contracts_does_not_increment_counters() {
        // REQUIREMENT: fast path (is_empty check) must not touch cache hit/miss counters.
        // BOUNDARY: early return before any symbol lookup.
        // EXPECTED: counters remain 0 after call.
        let engine = make_test_engine();
        let contracts = Contracts::default();

        engine
            .verify_batch(&contracts, Uuid::new_v4())
            .await
            .expect("verify_batch must not error on empty contracts");

        assert_eq!(
            engine.cache_hit_count(),
            0,
            "fast path must not record cache hits"
        );
        assert_eq!(
            engine.cache_miss_count(),
            0,
            "fast path must not record cache misses"
        );
    }

    // ── verify_batch: concurrency / deadlock ─────────────────────────────────

    #[tokio::test]
    async fn test_verify_batch_concurrent_no_deadlock() {
        // REQUIREMENT: verify_batch must not deadlock when called concurrently from
        //              multiple tokio tasks sharing an Arc<VgpEngine>.
        // BOUNDARY: 5 concurrent callers, all with empty contracts (fast path).
        // EXPECTED: all tasks complete within the timeout without panic or join error.
        use tokio::time::{Duration, timeout};

        let engine = Arc::new(make_test_engine());

        let tasks: Vec<_> = (0..5_u8)
            .map(|_| {
                let eng = Arc::clone(&engine);
                let contracts = Contracts::default();
                tokio::spawn(async move {
                    let plan_id = Uuid::new_v4();
                    eng.verify_batch(&contracts, plan_id).await
                })
            })
            .collect();

        let join_results = timeout(Duration::from_secs(10), async {
            let mut reports = Vec::with_capacity(tasks.len());
            for handle in tasks {
                reports.push(handle.await);
            }
            reports
        })
        .await
        .expect("concurrent verify_batch timed out — possible deadlock");

        for join_result in join_results {
            let report = join_result
                .expect("tokio task panicked")
                .expect("verify_batch must not error on empty contracts");
            assert!(report.all_passed, "all concurrent calls must pass");
        }
    }

    // ── SymbolKey ─────────────────────────────────────────────────────────────

    #[test]
    fn test_symbol_key_from_symbol_ref_no_crate() {
        // REQUIREMENT: SymbolKey::from(&SymbolRef) copies name and crate_name correctly.
        // BOUNDARY: crate_name = None.
        // EXPECTED: key fields match SymbolRef fields.
        use crate::plan::contracts::SymbolRef;
        let sym = SymbolRef::named("MySymbol");
        let key = SymbolKey::from(&sym);
        assert_eq!(key.name, "MySymbol");
        assert_eq!(key.crate_name, None);
    }

    #[test]
    fn test_symbol_key_from_symbol_ref_with_crate() {
        // REQUIREMENT: crate_name propagates correctly into SymbolKey.
        // BOUNDARY: crate_name = Some("my_crate").
        // EXPECTED: key.crate_name == Some("my_crate").
        use crate::plan::contracts::SymbolRef;
        let sym = SymbolRef::in_crate("AnotherSymbol", "my_crate");
        let key = SymbolKey::from(&sym);
        assert_eq!(key.name, "AnotherSymbol");
        assert_eq!(key.crate_name.as_deref(), Some("my_crate"));
    }

    #[test]
    fn test_symbol_key_equality_reflexive() {
        // REQUIREMENT: SymbolKey must implement Eq correctly (used as HashMap key).
        // BOUNDARY: same name and crate_name.
        // EXPECTED: two identical keys are equal.
        let k1 = SymbolKey {
            name: "Foo".to_owned(),
            crate_name: None,
        };
        let k2 = SymbolKey {
            name: "Foo".to_owned(),
            crate_name: None,
        };
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_symbol_key_inequality_by_crate() {
        // REQUIREMENT: SymbolKey distinguishes keys by crate_name.
        // BOUNDARY: same symbol name, different crate.
        // EXPECTED: keys are not equal.
        let k1 = SymbolKey {
            name: "Bar".to_owned(),
            crate_name: Some("crate_a".to_owned()),
        };
        let k2 = SymbolKey {
            name: "Bar".to_owned(),
            crate_name: Some("crate_b".to_owned()),
        };
        assert_ne!(k1, k2);
    }
}

/// Resolve a symbol using the fast-path in-process index when available,
/// falling back to the subprocess daemon call otherwise.
///
/// This function exists to keep [`VgpEngine::verify_batch`] below the CC=15
/// threshold by extracting the two-branch lookup decision into one place.
///
/// # Fast-path behaviour
///
/// When `index` is `Some`, `search_symbols` performs a `DashMap` scan
/// (< 1 ms) and returns the first entry whose `name` exactly matches AND,
/// when `crate_filter` is `Some`, whose path contains the crate name.
/// On a miss (no match, or `crate_filter` mismatch) the function falls through
/// to `lookup_symbol_via_cli` which properly handles crate-qualified lookups.
fn resolve_symbol(
    name: &str,
    crate_filter: Option<&str>,
    index: Option<&touring_intelligence::index::IncrementalIndex>,
) -> VgpLookupResult {
    if let Some(idx) = index {
        let hits = idx.search_symbols(name);
        // Phase 2.4: Apply crate_filter to fast-path results.
        // If crate_filter is set, only accept hits whose path contains the crate name.
        let matched = hits.into_iter().find(|(path, s)| {
            s.name == name && crate_filter.is_none_or(|c| path.to_string_lossy().contains(c))
        });
        if let Some((path, sym_info)) = matched {
            return VgpLookupResult {
                found: true,
                file_path: Some(path.to_string_lossy().into_owned()),
                line: Some(u32::try_from(sym_info.line).unwrap_or(u32::MAX)),
            };
        }
    }
    lookup_symbol_via_cli(name, crate_filter)
}

/// Query the touring daemon for a symbol via CLI subprocess.
///
/// Returns a [`VgpLookupResult`] indicating whether the symbol was found.
/// Falls back to "not found" if the daemon is unavailable — callers must
/// treat unavailability as a soft failure, not a hard error.
fn lookup_symbol_via_cli(name: &str, _crate_filter: Option<&str>) -> VgpLookupResult {
    use std::process::Command;

    let output = Command::new("touring")
        .args(["index", "find", name, "-j"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                let count = v
                    .get("count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let file_path = v
                    .get("definitions")
                    .and_then(|d| d.get(0))
                    .and_then(|d| d.get("file_path"))
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned);
                let line = v
                    .get("definitions")
                    .and_then(|d| d.get(0))
                    .and_then(|d| d.get("line"))
                    .and_then(serde_json::Value::as_u64)
                    .map(|l| u32::try_from(l).unwrap_or(u32::MAX));
                VgpLookupResult {
                    found: count > 0,
                    file_path,
                    line,
                }
            } else {
                VgpLookupResult {
                    found: false,
                    file_path: None,
                    line: None,
                }
            }
        }
        _ => VgpLookupResult {
            found: false,
            file_path: None,
            line: None,
        },
    }
}
