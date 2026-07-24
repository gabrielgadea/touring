//! Telemetry + NLP ranking adapters.
//!
//! Extracted from `core/context.rs` (F-9 modularization): `TracingTelemetrySink`
//! (production `TelemetrySink` emitting `tracing` events/metrics) and
//! `NlpPlanRankerAdapter` (Aho-Corasick keyword plan ranking). Re-exported from
//! `core::context` so the public API (`crate::TracingTelemetrySink`,
//! `crate::NlpPlanRankerAdapter`) is preserved verbatim. The `TelemetrySink`
//! trait, the `CognitiveNexusFn` type alias, and `PlanSimilarityScore` stay in
//! `context.rs` and are imported here (cfg-gated to keep this module clean under
//! every feature combination).

#[cfg(feature = "observability")]
use crate::core::context::TelemetrySink;
#[cfg(feature = "observability")]
use uuid::Uuid;

#[cfg(feature = "nlp-reranking")]
use crate::core::context::{CognitiveNexusFn, PlanSimilarityScore};
#[cfg(feature = "nlp-reranking")]
use std::sync::Arc;

// ── TracingTelemetrySink (PLN2 section 8.1 — feature `observability`) ───────

/// Production `TelemetrySink` emitting real `tracing` events and structured
/// histogram metrics via the global subscriber (W9 of PLN2).
///
/// Feature-gated on `observability`. Activates OpenTelemetry-compatible spans
/// on every lifecycle transition, counter, and histogram emitted by the
/// generator pipeline. Integrates transparently with any downstream exporter
/// wired into the global `tracing` subscriber (opentelemetry-jaeger,
/// tracing-opentelemetry, tokio-console, etc.).
///
/// # Fields
///
/// - `plan_events` — monotonic counter for lifecycle transitions (non-blocking)
/// - `counters_total` — aggregate counter increments (debug observability)
/// - `histogram_samples` — total histogram samples recorded
///
/// These atomics let the sink answer "how many events have I emitted?"
/// without holding a lock — useful for self-audit and test verification.
#[cfg(feature = "observability")]
#[derive(Debug, Default)]
pub struct TracingTelemetrySink {
    plan_events: std::sync::atomic::AtomicU64,
    counters_total: std::sync::atomic::AtomicU64,
    histogram_samples: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "observability")]
impl TracingTelemetrySink {
    /// Construct a new sink with all counters at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of lifecycle transitions recorded so far.
    #[must_use]
    pub fn plan_event_count(&self) -> u64 {
        self.plan_events.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the aggregate counter sum recorded so far.
    #[must_use]
    pub fn counter_total(&self) -> u64 {
        self.counters_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the number of histogram samples recorded so far.
    #[must_use]
    pub fn histogram_sample_count(&self) -> u64 {
        self.histogram_samples
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(feature = "observability")]
impl TelemetrySink for TracingTelemetrySink {
    fn record_lifecycle_transition(&self, from: &str, to: &str, plan_id: Uuid, elapsed_ns: u64) {
        self.plan_events
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::info!(
            target: "touring_generator::lifecycle",
            plan_id = %plan_id,
            from = %from,
            to = %to,
            elapsed_ns = elapsed_ns,
            "plan lifecycle transition"
        );
    }

    fn increment_counter(&self, name: &'static str, value: u64) {
        self.counters_total
            .fetch_add(value, std::sync::atomic::Ordering::Relaxed);
        tracing::debug!(
            target: "touring_generator::metrics",
            metric_name = name,
            metric_value = value,
            metric_type = "counter",
            "counter increment"
        );
    }

    fn record_histogram(&self, name: &'static str, value: f64) {
        self.histogram_samples
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        tracing::debug!(
            target: "touring_generator::metrics",
            metric_name = name,
            metric_value = value,
            metric_type = "histogram",
            "histogram sample"
        );
    }
}

// ── NlpPlanRankerAdapter (PLN2 section 8.1 — feature `nlp-reranking`) ────────

/// NLP-backed plan ranker wrapping `touring_intelligence::ann::KeywordMatcher`.
///
/// Uses Aho-Corasick keyword matching to rank candidate plans by intent
/// similarity to a query. Unlike the `SemanticGraphAdapter` (which scores
/// plans by graph connectivity), this adapter scores by token overlap —
/// complementary approaches that can be composed for higher-quality recall.
///
/// # Wiring
///
/// The adapter exposes:
/// - `rank_intents()` — pure utility: rank a list of `(plan_id, intent)` pairs
///   by keyword overlap with a query
/// - `into_cognitive_nexus_fn()` — builder returning a `CognitiveNexusFn`
///   that scores incoming keys using a fixed query captured at build time
///
/// # POTENCIALIZAR
///
/// Integrates `touring-antt`'s Aho-Corasick engine (previously unused by
/// touring-generator) into the plan recall pipeline. Provides keyword-level
/// similarity that the graph-based `SemanticGraphAdapter` cannot express.
#[cfg(feature = "nlp-reranking")]
pub struct NlpPlanRankerAdapter {
    matcher_config: touring_intelligence::ann::MatcherConfig,
}

#[cfg(feature = "nlp-reranking")]
impl std::fmt::Debug for NlpPlanRankerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NlpPlanRankerAdapter")
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "nlp-reranking")]
impl Default for NlpPlanRankerAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "nlp-reranking")]
impl NlpPlanRankerAdapter {
    /// Construct with `POTENCIALIZAR` defaults (case-insensitive, non-overlapping).
    #[must_use]
    pub fn new() -> Self {
        Self {
            matcher_config: touring_intelligence::ann::MatcherConfig::default(),
        }
    }

    /// Construct with a custom `MatcherConfig`.
    #[must_use]
    pub fn with_config(matcher_config: touring_intelligence::ann::MatcherConfig) -> Self {
        Self { matcher_config }
    }

    /// Rank candidate plans by keyword match count vs a query string.
    ///
    /// Extracts query tokens (alphanumeric words of length ≥ 3) and uses
    /// `KeywordMatcher::find_matches` against each candidate's intent.
    /// Returns `(plan_id, match_count)` pairs sorted descending by count.
    ///
    /// # Arguments
    ///
    /// - `query` — the reference string whose keywords drive the ranking
    /// - `candidates` — `(plan_id, intent)` pairs to rank against the query
    ///
    /// # Returns
    ///
    /// A sorted `Vec<(String, usize)>` where the first entry is the best
    /// match. Candidates with zero matches are still included but ordered last.
    #[must_use]
    pub fn rank_intents(
        &self,
        query: &str,
        candidates: &[(String, String)],
    ) -> Vec<(String, usize)> {
        let tokens: Vec<String> = Self::extract_tokens(query);
        if tokens.is_empty() {
            return candidates.iter().map(|(id, _)| (id.clone(), 0)).collect();
        }

        let matcher =
            touring_intelligence::ann::KeywordMatcher::new(tokens, self.matcher_config.clone());
        let mut ranked: Vec<(String, usize)> = candidates
            .iter()
            .map(|(id, intent)| {
                let hits = matcher.find_matches(intent);
                (id.clone(), hits.len())
            })
            .collect();
        ranked.sort_by_key(|&(_, hits)| std::cmp::Reverse(hits));
        ranked
    }

    /// Extract alphanumeric tokens of length ≥ 3 from a query string.
    ///
    /// Keeps the adapter independent from external tokenization logic and
    /// keeps the returned token list free of single-char noise that would
    /// produce spurious matches in Aho-Corasick.
    fn extract_tokens(query: &str) -> Vec<String> {
        query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 3)
            .map(str::to_lowercase)
            .collect()
    }

    /// Build a `CognitiveNexusFn` closure that scores a key against a
    /// snapshot of candidate plans captured at build time.
    ///
    /// The closure receives a query key and returns a `PlanSimilarityScore`
    /// based on how many query tokens match any candidate intent. Returns
    /// `None` when the candidate list is empty or the key has no tokens.
    #[must_use]
    pub fn into_cognitive_nexus_fn(self, candidates: Vec<(String, String)>) -> CognitiveNexusFn {
        let adapter = Arc::new(self);
        let candidates = Arc::new(candidates);
        Arc::new(move |key: &str| {
            if candidates.is_empty() {
                return None;
            }
            let ranked = adapter.rank_intents(key, candidates.as_ref());
            if ranked.is_empty() {
                return None;
            }
            let best_matches = ranked[0].1;
            if best_matches == 0 {
                return Some(PlanSimilarityScore::clamped(0.0));
            }
            // Normalize by candidate intent token count: more matches → closer to 1.
            // Saturates at 10 matches for a full 1.0 score.
            #[allow(clippy::cast_precision_loss)]
            let raw = (best_matches as f64) / 10.0;
            Some(PlanSimilarityScore::clamped(raw.min(1.0)))
        })
    }
}
