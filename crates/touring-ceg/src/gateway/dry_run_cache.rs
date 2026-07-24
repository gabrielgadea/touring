//! Content-hash dry-run cache for the Code Execution Gateway — phase **P4.5**
//! of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X5 SANDBOX runs the code body once in a real subprocess. That run is
//! dominated by interpreter cold-start — tens to hundreds of milliseconds for
//! `python3` / `node` — which is pure overhead when the *same code body* is
//! gated again (a retried tool call, a re-run of an identical snippet).
//!
//! [`DryRunCache`] removes that repeat cost: it is a `moka` cache keyed by a
//! **BLAKE3 digest** of the `(tool, payload, capability profile)` triple. The
//! first dry-run of a body runs the sandbox and stores its [`SandboxOutcome`];
//! every later dry-run of a byte-identical body returns the stored outcome and
//! **skips the subprocess entirely**.
//!
//! The profile is part of the key because it sets the sandbox timeout — the
//! same code can time out under a `Deny`-profile's short leash yet finish
//! under an `Allow`-profile's, so their outcomes must not share a slot.
//!
//! # Integration
//!
//! [`cached_dry_run_in_sandbox`] is a drop-in for
//! [`dry_run_in_sandbox`] — same signature — so the X5 wiring (`guarded_dry_run`)
//! swaps one for the other. The daemon shares one process-global cache,
//! [`DryRunCache::global`].
//!
//! Best-practice basis: `docs/2026-05-17-ceg-best-practices.md` and the `moka`
//! bounded-cache pattern already used across the workspace.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::{env, fmt};

use moka::sync::Cache;

use super::sandbox_stage::{SandboxCapabilities, SandboxOutcome, dry_run_in_sandbox};
use super::typestate::RawInvocation;

/// Default number of dry-run outcomes the cache retains.
const DEFAULT_MAX_CAPACITY: u64 = 512;
/// Upper bound on the configured capacity — a guard against a mistyped env var.
const MAX_CAPACITY_CEILING: u64 = 65_536;
/// Default time a cached outcome lives before it must be re-observed.
const DEFAULT_TTL: Duration = Duration::from_secs(3600);
/// Environment variable overriding the cache capacity:
/// `TOURING_CEG_DRY_RUN_CACHE_CAPACITY`.
const ENV_MAX_CAPACITY: &str = "TOURING_CEG_DRY_RUN_CACHE_CAPACITY";
/// Environment variable overriding the cache TTL (whole seconds):
/// `TOURING_CEG_DRY_RUN_CACHE_TTL_SECS`.
const ENV_TTL_SECS: &str = "TOURING_CEG_DRY_RUN_CACHE_TTL_SECS";

/// How a [`DryRunCache`] is sized and how long an entry lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheConfig {
    /// The maximum number of cached dry-run outcomes.
    pub max_capacity: u64,
    /// How long a cached outcome stays valid before it is re-observed.
    pub ttl: Duration,
}

impl CacheConfig {
    /// Build a config from explicit values, clamping `max_capacity` into the
    /// safe `[1, 65536]` range so the cache can never be built unusable.
    #[must_use]
    pub fn new(max_capacity: u64, ttl: Duration) -> Self {
        Self {
            max_capacity: max_capacity.clamp(1, MAX_CAPACITY_CEILING),
            ttl,
        }
    }

    /// Resolve a config from the process environment
    /// (`TOURING_CEG_DRY_RUN_CACHE_CAPACITY` / `TOURING_CEG_DRY_RUN_CACHE_TTL_SECS`),
    /// falling back to the default for any field unset or unparseable.
    #[must_use]
    pub fn from_env() -> Self {
        Self::resolve(env::var(ENV_MAX_CAPACITY).ok(), env::var(ENV_TTL_SECS).ok())
    }

    /// The pure core of [`from_env`](Self::from_env): resolve a config from the
    /// *raw* env strings. Separated from the env read so the resolution logic
    /// is testable without mutating process-global state. An absent,
    /// non-numeric, or zero value falls back to the default for that field.
    #[must_use]
    pub fn resolve(max_capacity: Option<String>, ttl_secs: Option<String>) -> Self {
        let cap = max_capacity
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_CAPACITY);
        let ttl = ttl_secs
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
            .map_or(DEFAULT_TTL, Duration::from_secs);
        Self::new(cap, ttl)
    }
}

impl Default for CacheConfig {
    /// The default 512-entry capacity and 1-hour TTL.
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CAPACITY, DEFAULT_TTL)
    }
}

/// A snapshot of a [`DryRunCache`]'s live state and lifetime counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    /// Outcomes currently held.
    pub entries: u64,
    /// Lifetime count of lookups that found a cached outcome.
    pub hits: u64,
    /// Lifetime count of lookups that missed and had to run the sandbox.
    pub misses: u64,
    /// Lifetime count of outcomes evicted by the underlying `moka` cache —
    /// capacity bound, TTL expiry, or explicit invalidation. Incremented by
    /// the cache's `eviction_listener`. **Wave 2026-05-22 (A6)** —
    /// Context7-driven observability addition.
    pub evictions: u64,
}

impl CacheStats {
    /// The hit rate in `[0.0, 1.0]` — `0.0` when nothing has been looked up.
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// The content-addressed cache key for a dry-run.
///
/// A BLAKE3 digest over the tool name, the code body, and the capability
/// profile name — `\0`-separated so distinct triples cannot collide by
/// concatenation (e.g. `("ab", "c")` vs `("a", "bc")`). The profile is keyed
/// because it sets the sandbox timeout, so the same body can yield a different
/// outcome under a different profile.
#[must_use]
pub fn dry_run_cache_key(raw: &RawInvocation, caps: &SandboxCapabilities) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(raw.tool.as_bytes());
    hasher.update(&[0]);
    hasher.update(raw.payload.as_bytes());
    hasher.update(&[0]);
    hasher.update(caps.profile_name.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// A bounded, content-addressed cache of X5 dry-run outcomes.
///
/// See the [module documentation](self) for the rationale. Construct one with
/// [`new`](Self::new), or share the daemon-wide [`global`](Self::global)
/// instance. The cache is `Send + Sync`.
#[derive(Debug)]
pub struct DryRunCache {
    /// The bounded store — keyed by [`dry_run_cache_key`], TTL- and
    /// capacity-evicted.
    cache: Cache<String, SandboxOutcome>,
    /// Lifetime count of lookups that hit.
    hits: AtomicU64,
    /// Lifetime count of lookups that missed.
    misses: AtomicU64,
    /// Lifetime count of evictions reported by the underlying `moka` cache.
    /// Shared with the eviction listener closure so the counter is updated
    /// out-of-band whenever moka removes an entry (capacity overflow, TTL
    /// expiry, or explicit invalidation). **Wave 2026-05-22 (A6)**.
    evictions: Arc<AtomicU64>,
}

impl DryRunCache {
    /// Build a cache sized by `config`.
    ///
    /// The underlying `moka` cache is named `"ceg-dry-run-cache"` so it
    /// surfaces distinctly in debugging tooling (`tokio-console`, log
    /// traces, profilers) — a Context7 best-practice 2026-05-22 carry-over
    /// from `moka::sync::CacheBuilder::name`.
    ///
    /// An `eviction_listener` increments the shared `evictions` counter on
    /// every removal (capacity overflow / TTL expiry / explicit invalidation)
    /// — the snapshot is then exposed via [`CacheStats::evictions`] for
    /// operational observability without inspecting the moka internals.
    /// **Wave 2026-05-22 (A6)**.
    #[must_use]
    pub fn new(config: CacheConfig) -> Self {
        let evictions = Arc::new(AtomicU64::new(0));
        let evictions_for_listener = Arc::clone(&evictions);
        let listener = move |_key: Arc<String>,
                             _value: SandboxOutcome,
                             _cause: moka::notification::RemovalCause| {
            evictions_for_listener.fetch_add(1, Ordering::Relaxed);
        };
        Self {
            cache: Cache::builder()
                .name("ceg-dry-run-cache")
                .max_capacity(config.max_capacity)
                .time_to_live(config.ttl)
                .eviction_listener(listener)
                .build(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions,
        }
    }

    /// The process-global cache, configured from the environment on first use.
    ///
    /// Every gateway entry point shares this one instance, so a code body
    /// dry-run anywhere in the daemon is cached for every later gate.
    #[must_use]
    pub fn global() -> &'static DryRunCache {
        static GLOBAL: OnceLock<DryRunCache> = OnceLock::new();
        GLOBAL.get_or_init(|| DryRunCache::new(CacheConfig::from_env()))
    }

    /// Look up a cached outcome by content key, recording the hit or miss.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<SandboxOutcome> {
        match self.cache.get(key) {
            Some(outcome) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(outcome)
            }
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Store a dry-run outcome under its content key.
    pub fn insert(&self, key: String, outcome: SandboxOutcome) {
        self.cache.insert(key, outcome);
    }

    /// A snapshot of the cache's live state and lifetime counters.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        // Drain moka's pending insert/evict tasks so `entry_count` and the
        // eviction-listener counter are both exact at the snapshot instant.
        self.cache.run_pending_tasks();
        CacheStats {
            entries: self.cache.entry_count(),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
        }
    }
}

impl fmt::Display for CacheStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "dry-run cache: {} entries, {} hits, {} misses, {} evictions ({:.1}% hit rate)",
            self.entries,
            self.hits,
            self.misses,
            self.evictions,
            self.hit_rate() * 100.0
        )
    }
}

/// The X5 production runner **with content-hash caching**.
///
/// A drop-in replacement for [`dry_run_in_sandbox`] — same signature — so the
/// X5 wiring (`guarded_dry_run`) swaps one for the other. On a **miss** it runs
/// the real sandbox dry-run and stores the outcome in [`DryRunCache::global`];
/// on a **hit** it returns the stored [`SandboxOutcome`] and skips the
/// subprocess entirely. Identical code bodies pay the sandbox cost once.
#[must_use]
pub fn cached_dry_run_in_sandbox(
    raw: &RawInvocation,
    caps: &SandboxCapabilities,
) -> SandboxOutcome {
    let key = dry_run_cache_key(raw, caps);
    let cache = DryRunCache::global();
    if let Some(hit) = cache.get(&key) {
        return hit;
    }
    let outcome = dry_run_in_sandbox(raw, caps);
    cache.insert(key, outcome.clone());
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityProfile, Decision};
    use crate::gateway::summarize::OutputSummary;

    /// A representative sandbox outcome for cache tests.
    fn sample_outcome() -> SandboxOutcome {
        SandboxOutcome {
            exit_code: 0,
            output_bytes: 7,
            was_truncated: false,
            timed_out: false,
            content_hash: "cafebabe".to_owned(),
            capability_profile: "p45".to_owned(),
            summary: OutputSummary::empty(0),
        }
    }

    /// A `(raw, caps)` pair for keying tests.
    fn probe(tool: &str, payload: &str, profile: &str) -> (RawInvocation, SandboxCapabilities) {
        let raw = RawInvocation::new(tool, payload);
        let caps =
            SandboxCapabilities::from_profile(&CapabilityProfile::new(profile, Decision::Allow));
        (raw, caps)
    }

    #[test]
    fn cache_key_is_deterministic_and_64_hex_chars() {
        let (raw, caps) = probe("Bash", "echo hi", "p");
        let k1 = dry_run_cache_key(&raw, &caps);
        let k2 = dry_run_cache_key(&raw, &caps);
        assert_eq!(k1, k2, "identical input must yield an identical key");
        assert_eq!(k1.len(), 64, "BLAKE3 hex digest is 64 chars");
        assert!(k1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn cache_key_distinguishes_tool_payload_and_profile() {
        let base = {
            let (r, c) = probe("Bash", "echo hi", "p");
            dry_run_cache_key(&r, &c)
        };
        let other_tool = {
            let (r, c) = probe("SandboxPython", "echo hi", "p");
            dry_run_cache_key(&r, &c)
        };
        let other_payload = {
            let (r, c) = probe("Bash", "echo bye", "p");
            dry_run_cache_key(&r, &c)
        };
        let other_profile = {
            let (r, c) = probe("Bash", "echo hi", "q");
            dry_run_cache_key(&r, &c)
        };
        assert_ne!(base, other_tool, "a different tool must change the key");
        assert_ne!(base, other_payload, "a different body must change the key");
        assert_ne!(
            base, other_profile,
            "a different profile must change the key"
        );
    }

    #[test]
    fn cache_config_resolve_defaults_and_env() {
        let defaults = CacheConfig::resolve(None, None);
        assert_eq!(defaults, CacheConfig::default());
        let from_env = CacheConfig::resolve(Some("100".to_owned()), Some("60".to_owned()));
        assert_eq!(from_env.max_capacity, 100);
        assert_eq!(from_env.ttl, Duration::from_secs(60));
        // Garbage and zero fall back to the field default.
        let garbage = CacheConfig::resolve(Some("xyz".to_owned()), Some("0".to_owned()));
        assert_eq!(garbage, CacheConfig::default());
    }

    #[test]
    fn cache_config_new_clamps_capacity() {
        assert_eq!(
            CacheConfig::new(0, DEFAULT_TTL).max_capacity,
            1,
            "a zero capacity is clamped up to 1"
        );
        assert_eq!(
            CacheConfig::new(u64::MAX, DEFAULT_TTL).max_capacity,
            MAX_CAPACITY_CEILING
        );
    }

    #[test]
    fn get_miss_then_insert_then_hit() {
        let cache = DryRunCache::new(CacheConfig::default());
        let key = "deadbeef".to_owned();
        assert!(cache.get(&key).is_none(), "an empty cache misses");
        cache.insert(key.clone(), sample_outcome());
        let hit = cache.get(&key).expect("the inserted outcome is found");
        assert_eq!(hit, sample_outcome());
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn cache_stats_hit_rate_is_correct() {
        let empty = CacheStats {
            entries: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        };
        assert!((empty.hit_rate() - 0.0).abs() < f64::EPSILON);
        let mixed = CacheStats {
            entries: 3,
            hits: 3,
            misses: 1,
            evictions: 0,
        };
        assert!((mixed.hit_rate() - 0.75).abs() < 1e-9);
        assert!(mixed.to_string().contains("75.0%"));
    }

    #[test]
    fn cache_stats_display_includes_eviction_count() {
        let stats = CacheStats {
            entries: 2,
            hits: 5,
            misses: 1,
            evictions: 7,
        };
        let rendered = stats.to_string();
        assert!(rendered.contains("7 evictions"), "got: {rendered:?}");
    }

    #[test]
    fn dry_run_cache_evictions_counter_starts_at_zero() {
        let cache = DryRunCache::new(CacheConfig::default());
        let stats = cache.stats();
        assert_eq!(stats.evictions, 0, "fresh cache must have 0 evictions");
    }

    #[test]
    fn global_cache_is_a_stable_singleton() {
        let a = DryRunCache::global();
        let b = DryRunCache::global();
        assert!(std::ptr::eq(a, b), "global() must return one shared cache");
    }

    #[test]
    fn cached_dry_run_skips_the_sandbox_on_a_repeat_body() {
        // First call: a miss — runs the real bash sandbox. Second call: a hit
        // on the byte-identical body — skips the subprocess.
        let (raw, caps) = probe("Bash", "echo p45-skip-probe-marker", "p45-skip");
        let before = DryRunCache::global().stats();
        let first = cached_dry_run_in_sandbox(&raw, &caps);
        let second = cached_dry_run_in_sandbox(&raw, &caps);
        let after = DryRunCache::global().stats();

        assert!(
            first.succeeded(),
            "the echo dry-run should succeed: {first:?}"
        );
        assert_eq!(first, second, "the repeat must return the cached outcome");
        assert!(
            after.hits > before.hits,
            "the repeat call must register a cache hit"
        );
        assert!(
            after.misses > before.misses,
            "the first call must register a cache miss"
        );
    }

    #[test]
    fn p45_cache_hit_latency_p99_under_5ms() {
        // Acceptance: "P99 of a cache hit < 5ms". The hit path is
        // `dry_run_cache_key` (BLAKE3) + a moka lookup — measured here over a
        // pre-warmed cache, with no subprocess involved.
        let cache = DryRunCache::new(CacheConfig::default());
        let (raw, caps) = probe("Bash", "echo p45-latency-probe", "p45-lat");
        let warm_key = dry_run_cache_key(&raw, &caps);
        cache.insert(warm_key.clone(), sample_outcome());
        assert!(cache.get(&warm_key).is_some(), "the entry must be warm");

        const N: usize = 2_000;
        let mut samples = Vec::with_capacity(N);
        for _ in 0..N {
            let start = std::time::Instant::now();
            let key = dry_run_cache_key(&raw, &caps);
            let hit = cache.get(&key);
            samples.push(start.elapsed());
            assert!(hit.is_some(), "every probe is a warm hit");
        }
        samples.sort_unstable();
        let p99 = samples[(N as f64 * 0.99) as usize];
        assert!(
            p99 < Duration::from_millis(5),
            "P99 cache-hit latency {p99:?} must be < 5ms"
        );
    }
}
