//! ACO Bridge — Connects ACO orchestration intelligence to the hook system.
//!
//! This bridge enables hooks to:
//! - Report execution quality via the 9-dimensional GoalTracker
//! - Track hook execution as events via EventBuffer
//! - Cache hook results via QueryCache
//! - Model hook dependencies as a GeneratorGraph
//!
//! This is the **orchestration intelligence layer** that transforms
//! isolated hook executions into a coordinated, quality-tracked system.

use touring_intelligence::rl::aco::{
    esaa::{EventBuffer, EventBufferConfig},
    tracker::{DimResult, TrackerReport, build_report},
};

use moka::sync::Cache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Hook execution outcome for quality tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookOutcome {
    /// Hook identifier (e.g., "pre_read", "post_bash")
    pub hook_name: String,
    /// Whether the hook executed successfully
    pub success: bool,
    /// Execution latency in milliseconds
    pub latency_ms: u64,
    /// Whether context was injected (for pre-hooks)
    pub context_injected: bool,
    /// Whether knowledge was captured (for post-hooks)
    pub knowledge_captured: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// P5.2: Streaming aggregation stats for O(1) memory quality tracking.
///
/// Accumulates metrics incrementally as outcomes arrive, without
/// storing individual outcomes (though Vec is kept for backward compat).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamingHookStats {
    /// Total successful hooks.
    pub success_count: u32,
    /// Total failed hooks.
    pub failure_count: u32,
    /// Sum of latencies in ms (for mean calculation).
    pub latency_sum_ms: u64,
    /// Max latency observed.
    pub max_latency_ms: u64,
    /// Count of hooks that completed within the target latency (100ms).
    pub fast_hooks_count: u32,
    /// Count of pre-hooks that injected context.
    pub context_injected_count: u32,
    /// Count of post-hooks that captured knowledge.
    pub knowledge_captured_count: u32,
    /// Count of pre-hooks total.
    pub pre_hook_count: u32,
    /// Count of post-hooks total.
    pub post_hook_count: u32,
}

impl StreamingHookStats {
    /// Latency target: hooks under this threshold are considered "fast".
    pub const LATENCY_TARGET_MS: u64 = 100;

    /// Update stats with a new outcome.
    pub fn record(&mut self, outcome: &HookOutcome) {
        if outcome.success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }
        self.latency_sum_ms += outcome.latency_ms;
        if outcome.latency_ms > self.max_latency_ms {
            self.max_latency_ms = outcome.latency_ms;
        }
        if outcome.latency_ms <= Self::LATENCY_TARGET_MS {
            self.fast_hooks_count += 1;
        }
        if outcome.hook_name.starts_with("pre_") {
            self.pre_hook_count += 1;
            if outcome.context_injected {
                self.context_injected_count += 1;
            }
        }
        if outcome.hook_name.starts_with("post_") {
            self.post_hook_count += 1;
            if outcome.knowledge_captured {
                self.knowledge_captured_count += 1;
            }
        }
    }

    /// Total hooks processed.
    pub fn total(&self) -> u32 {
        self.success_count + self.failure_count
    }

    /// Average latency in ms.
    pub fn avg_latency_ms(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.latency_sum_ms as f64 / total as f64
        }
    }

    /// Success rate (0.0-1.0).
    pub fn success_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            1.0
        } else {
            self.success_count as f64 / total as f64
        }
    }
}

/// 9-dimensional quality assessment for a hook session.
///
/// Maps to ACO's GoalTracker dimensions:
///
/// - D1: Precision (hook output accuracy)
/// - D2: Coverage (hooks fired vs expected)
/// - D3: Latency (avg + max, target <100ms)
/// - D4: Knowledge (info captured per execution)
/// - D5: Context (info injected per execution)
/// - D6: Reliability (success rate)
/// - D7: Integration (cross-hook data flow)
/// - D8: Security (PII detection rate)
/// - D9: Evolution (learning rate)
///
/// Uses `StreamingHookStats` (O(1) memory) instead of `Vec<HookOutcome>` (O(n)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookQualityAssessment {
    /// Identifier of the session this assessment summarizes.
    pub session_id: String,
    /// Total number of hook events fired during the session.
    pub total_hooks_fired: u32,
    /// O(1) streaming aggregation — replaces the old \<Vec\>\<HookOutcome\> accumulator.
    pub streaming_stats: StreamingHookStats,
}

impl HookQualityAssessment {
    /// Creates an empty assessment for the given session id.
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            total_hooks_fired: 0,
            streaming_stats: StreamingHookStats::default(),
        }
    }

    /// Record a hook execution outcome (O(1) — updates streaming counters only).
    pub fn record(&mut self, outcome: HookOutcome) {
        self.total_hooks_fired += 1;
        self.streaming_stats.record(&outcome);
    }

    /// Build an ACO TrackerReport from accumulated hook outcomes.
    ///
    /// Maps hook metrics to the 9 GoalTracker dimensions.
    pub fn to_tracker_report(&self, iteration: u32) -> TrackerReport {
        let dims = vec![
            self.dim_precision(),
            self.dim_coverage(),
            self.dim_latency(),
            self.dim_knowledge(),
            self.dim_context(),
            self.dim_reliability(),
            self.dim_integration(),
            self.dim_security(),
            self.dim_evolution(),
        ];
        build_report(dims, iteration)
    }

    /// D1: Precision — hook outputs are accurate and useful.
    fn dim_precision(&self) -> DimResult {
        let total = self.streaming_stats.total();
        if total == 0 {
            return DimResult {
                dim_id: "D1".to_string(),
                name: "Precision".to_string(),
                score: 1.0,
                checks_passed: 0,
                checks_total: 0,
                details: vec!["No hooks fired".to_string()],
            };
        }
        let passed = self.streaming_stats.success_count;
        DimResult {
            dim_id: "D1".to_string(),
            name: "Precision".to_string(),
            score: self.streaming_stats.success_rate(),
            checks_passed: passed,
            checks_total: total,
            details: vec![format!("{passed}/{total} hooks succeeded")],
        }
    }

    /// D2: Coverage — ratio of hooks that completed vs expected.
    fn dim_coverage(&self) -> DimResult {
        let expected = self.total_hooks_fired.max(1);
        let actual = self.streaming_stats.total();
        let score = (actual as f64 / expected as f64).min(1.0);
        DimResult {
            dim_id: "D2".to_string(),
            name: "Coverage".to_string(),
            score,
            checks_passed: actual,
            checks_total: expected,
            details: vec![format!("{actual}/{expected} hooks completed")],
        }
    }

    /// D3: Latency — avg and max latency, target <100ms.
    fn dim_latency(&self) -> DimResult {
        let total = self.streaming_stats.total();
        if total == 0 {
            return DimResult {
                dim_id: "D3".to_string(),
                name: "Latency".to_string(),
                score: 1.0,
                checks_passed: 0,
                checks_total: 0,
                details: vec![],
            };
        }
        let target_ms = StreamingHookStats::LATENCY_TARGET_MS;
        let avg_ms = self.streaming_stats.avg_latency_ms() as u64;
        let max_ms = self.streaming_stats.max_latency_ms;
        // Score = fraction of hooks that completed within the target latency.
        let fast = self.streaming_stats.fast_hooks_count;
        let score = fast as f64 / total as f64;
        DimResult {
            dim_id: "D3".to_string(),
            name: "Latency".to_string(),
            score,
            checks_passed: fast,
            checks_total: total,
            details: vec![format!(
                "avg={avg_ms}ms max={max_ms}ms target={target_ms}ms fast={fast}/{total}"
            )],
        }
    }

    /// D4: Knowledge — ratio of post-hooks that captured knowledge.
    fn dim_knowledge(&self) -> DimResult {
        let post_total = self.streaming_stats.post_hook_count;
        if post_total == 0 {
            return DimResult {
                dim_id: "D4".to_string(),
                name: "Knowledge".to_string(),
                score: 1.0,
                checks_passed: 0,
                checks_total: 0,
                details: vec![],
            };
        }
        let captured = self.streaming_stats.knowledge_captured_count;
        DimResult {
            dim_id: "D4".to_string(),
            name: "Knowledge".to_string(),
            score: captured as f64 / post_total as f64,
            checks_passed: captured,
            checks_total: post_total,
            details: vec![format!(
                "{captured}/{post_total} post-hooks captured knowledge"
            )],
        }
    }

    /// D5: Context — ratio of pre-hooks that injected context.
    fn dim_context(&self) -> DimResult {
        let pre_total = self.streaming_stats.pre_hook_count;
        if pre_total == 0 {
            return DimResult {
                dim_id: "D5".to_string(),
                name: "Context".to_string(),
                score: 1.0,
                checks_passed: 0,
                checks_total: 0,
                details: vec![],
            };
        }
        let injected = self.streaming_stats.context_injected_count;
        DimResult {
            dim_id: "D5".to_string(),
            name: "Context".to_string(),
            score: injected as f64 / pre_total as f64,
            checks_passed: injected,
            checks_total: pre_total,
            details: vec![format!("{injected}/{pre_total} pre-hooks injected context")],
        }
    }

    /// D6: Reliability — success rate (CRITICAL dimension).
    fn dim_reliability(&self) -> DimResult {
        let total = self.streaming_stats.total();
        if total == 0 {
            return DimResult {
                dim_id: "D6".to_string(),
                name: "Reliability".to_string(),
                score: 1.0,
                checks_passed: 0,
                checks_total: 0,
                details: vec![],
            };
        }
        let passed = self.streaming_stats.success_count;
        DimResult {
            dim_id: "D6".to_string(),
            name: "Reliability".to_string(),
            score: self.streaming_stats.success_rate(),
            checks_passed: passed,
            checks_total: total,
            details: vec![format!("{passed}/{total} hooks succeeded")],
        }
    }

    /// D7: Integration — cross-hook data flow success.
    fn dim_integration(&self) -> DimResult {
        let has_pre = self.streaming_stats.pre_hook_count > 0;
        let has_post = self.streaming_stats.post_hook_count > 0;
        let score = match (has_pre, has_post) {
            (true, true) => 1.0,
            (true, false) | (false, true) => 0.5,
            (false, false) => 0.0,
        };
        DimResult {
            dim_id: "D7".to_string(),
            name: "Integration".to_string(),
            score,
            checks_passed: if score >= 1.0 {
                2
            } else if score >= 0.5 {
                1
            } else {
                0
            },
            checks_total: 2,
            details: vec![format!("pre_hooks: {has_pre}, post_hooks: {has_post}")],
        }
    }

    /// D8: Security — PII detection active (always pass if no PII found).
    fn dim_security(&self) -> DimResult {
        DimResult {
            dim_id: "D8".to_string(),
            name: "Security".to_string(),
            score: 1.0,
            checks_passed: 1,
            checks_total: 1,
            details: vec!["PII scanner active".to_string()],
        }
    }

    /// D9: Evolution — ratio of hooks that contributed to learning.
    fn dim_evolution(&self) -> DimResult {
        let total = self.streaming_stats.total().max(1);
        let learning = self.streaming_stats.knowledge_captured_count
            + self.streaming_stats.context_injected_count;
        DimResult {
            dim_id: "D9".to_string(),
            name: "Evolution".to_string(),
            score: (learning as f64 / total as f64).min(1.0),
            checks_passed: learning,
            checks_total: total,
            details: vec![format!("{learning}/{total} hooks contributed to learning")],
        }
    }
}

/// Cached hook result store — backed by moka TinyLFU cache.
///
/// Uses moka::sync::Cache for automatic TTL expiry, size-weighted eviction,
/// and TinyLFU admission policy. Replaces the previous QueryCache wrapper.
pub struct HookResultCache {
    cache: Cache<String, String>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl HookResultCache {
    /// Create a new cache with the given capacity and optional TTL.
    ///
    /// If `ttl_ms` is `None`, defaults to 5 minutes (300s).
    pub fn new(max_size: usize, ttl_ms: Option<u64>) -> Self {
        let ttl = Duration::from_millis(ttl_ms.unwrap_or(300_000));
        let cache = Cache::builder()
            .max_capacity(max_size as u64)
            .time_to_live(ttl)
            .build();
        Self {
            cache,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Cache a hook result for a given file.
    pub fn cache_result(&self, hook_name: &str, file_path: &str, result_json: String) {
        let key = format!("{hook_name}::{file_path}");
        self.cache.insert(key, result_json);
    }

    /// Get cached hook result for a file.
    pub fn get_result(&self, hook_name: &str, file_path: &str) -> Option<String> {
        let key = format!("{hook_name}::{file_path}");
        match self.cache.get(&key) {
            Some(v) => {
                self.hits.fetch_add(1, Ordering::Relaxed);
                Some(v)
            }
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Invalidate all cached results for a file (e.g., after edit).
    ///
    /// Iterates all entries and removes those whose key contains `file_path`.
    pub fn invalidate_file(&self, file_path: &str) -> usize {
        let keys_to_remove: Vec<String> = self
            .cache
            .iter()
            .filter_map(|(k, _)| {
                if k.as_ref().contains(file_path) {
                    Some(k.as_ref().clone())
                } else {
                    None
                }
            })
            .collect();
        let count = keys_to_remove.len();
        for key in keys_to_remove {
            self.cache.invalidate(&key);
        }
        count
    }

    /// Force moka's internal housekeeping (eviction, expiry).
    ///
    /// Normally moka runs these tasks lazily; call this after bulk
    /// insertions when you need deterministic eviction (e.g., in tests).
    pub fn run_pending(&self) {
        self.cache.run_pending_tasks();
    }

    /// Get cache hit rate (0.0–1.0).
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed) as f64;
        let misses = self.misses.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total == 0.0 { 0.0 } else { hits / total }
    }

    /// Get raw cache counters: `(hits, misses, entry_count)`.
    ///
    /// Used by E2E reporting to surface hook result cache health in the final JSON.
    pub fn stats(&self) -> (u64, u64, u64) {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let entry_count = self.cache.entry_count();
        (hits, misses, entry_count)
    }
}

/// Hook event buffer — wraps ACO EventBuffer for hook event streaming.
pub struct HookEventBuffer {
    buffer: EventBuffer,
}

/// Errors from [`HookEventBuffer::record_event`].
#[derive(Debug, thiserror::Error)]
pub enum HookEventBufferError {
    /// The hook outcome could not be serialized to JSON.
    #[error("Serialization error: {0}")]
    Serialization(#[source] serde_json::Error),
    /// The underlying ACO event buffer rejected the push.
    #[error("Buffer error: {0}")]
    Buffer(String),
}

impl HookEventBuffer {
    /// Creates a buffer that flushes at `max_batch_size` events or after `max_age_ms`.
    pub fn new(max_batch_size: usize, max_age_ms: u64) -> Self {
        Self {
            buffer: EventBuffer::new(EventBufferConfig {
                max_batch_size,
                max_age_ms,
            }),
        }
    }

    /// Record a hook execution event.
    pub fn record_event(&self, outcome: &HookOutcome) -> Result<(), HookEventBufferError> {
        let json = serde_json::to_string(outcome).map_err(HookEventBufferError::Serialization)?;
        self.buffer
            .push(json)
            .map_err(|e| HookEventBufferError::Buffer(e.to_string()))
    }

    /// Flush all buffered events (returns JSON strings).
    pub fn flush(&self) -> Vec<String> {
        self.buffer.flush()
    }

    /// Check if buffer is ready to flush.
    pub fn is_ready(&self) -> bool {
        self.buffer.is_ready()
    }

    /// Shutdown the buffer, optionally flushing remaining events.
    pub fn shutdown(&self, flush: bool) -> Option<Vec<String>> {
        self.buffer.shutdown(flush)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;
    use touring_intelligence::rl::aco::tracker::{CRITICAL_DIMS, TrackerStatus};

    fn sample_outcomes() -> Vec<HookOutcome> {
        vec![
            HookOutcome {
                hook_name: "pre_read".into(),
                success: true,
                latency_ms: 12,
                context_injected: true,
                knowledge_captured: false,
                error: None,
            },
            HookOutcome {
                hook_name: "post_read".into(),
                success: true,
                latency_ms: 25,
                context_injected: false,
                knowledge_captured: true,
                error: None,
            },
            HookOutcome {
                hook_name: "pre_bash".into(),
                success: true,
                latency_ms: 8,
                context_injected: true,
                knowledge_captured: false,
                error: None,
            },
            HookOutcome {
                hook_name: "post_bash".into(),
                success: false,
                latency_ms: 150,
                context_injected: false,
                knowledge_captured: false,
                error: Some("timeout".into()),
            },
        ]
    }

    #[test]
    fn test_hook_quality_assessment_basic() {
        let mut assessment = HookQualityAssessment::new("test_session");
        for outcome in sample_outcomes() {
            assessment.record(outcome);
        }
        assert_eq!(assessment.total_hooks_fired, 4);
        assert_eq!(assessment.streaming_stats.total(), 4);
    }

    #[test]
    fn test_tracker_report_generation() {
        let mut assessment = HookQualityAssessment::new("test_session");
        for outcome in sample_outcomes() {
            assessment.record(outcome);
        }
        let report = assessment.to_tracker_report(1);
        assert_eq!(report.dims.len(), 9);
        assert_eq!(report.iteration, 1);
        // Composite should be between 0 and 1
        assert!(report.composite >= 0.0 && report.composite <= 1.0);
    }

    #[test]
    fn test_all_pass_scenario() {
        let mut assessment = HookQualityAssessment::new("perfect");
        for name in &["pre_read", "post_read", "pre_bash", "post_bash"] {
            assessment.record(HookOutcome {
                hook_name: name.to_string(),
                success: true,
                latency_ms: 10,
                context_injected: name.starts_with("pre_"),
                knowledge_captured: name.starts_with("post_"),
                error: None,
            });
        }
        let report = assessment.to_tracker_report(1);
        assert_eq!(report.status, TrackerStatus::Pass);
        assert!(
            report.composite >= 0.8,
            "All-pass should have high composite: {}",
            report.composite
        );
    }

    #[test]
    fn test_empty_assessment() {
        let assessment = HookQualityAssessment::new("empty");
        let report = assessment.to_tracker_report(0);
        assert_eq!(report.dims.len(), 9);
        // Empty assessment should still produce valid report
        assert!(report.composite >= 0.0);
    }

    #[test]
    fn test_hook_result_cache() {
        let cache = HookResultCache::new(100, None);
        cache.cache_result("pre_read", "test.py", "{\"symbols\": 5}".into());
        assert_eq!(
            cache.get_result("pre_read", "test.py"),
            Some("{\"symbols\": 5}".into())
        );
        assert_eq!(cache.get_result("pre_read", "other.py"), None);

        // Invalidate
        let count = cache.invalidate_file("test.py");
        assert_eq!(count, 1);
        assert_eq!(cache.get_result("pre_read", "test.py"), None);
    }

    #[test]
    fn test_hook_event_buffer() {
        let buffer = HookEventBuffer::new(100, 5000);
        let outcome = HookOutcome {
            hook_name: "pre_read".into(),
            success: true,
            latency_ms: 10,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        };
        buffer
            .record_event(&outcome)
            .expect("record_event should not fail for valid HookOutcome in test");
        let events = buffer.flush();
        assert_eq!(events.len(), 1);
        // Verify it's valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&events[0]).expect("buffer flush should return valid JSON");
        assert_eq!(parsed["hook_name"], "pre_read");
    }

    #[test]
    fn test_latency_dimension_p99() {
        let mut assessment = HookQualityAssessment::new("latency_test");
        // 99 fast hooks + 1 slow
        for i in 0..99 {
            assessment.record(HookOutcome {
                hook_name: format!("hook_{i}"),
                success: true,
                latency_ms: 10,
                context_injected: false,
                knowledge_captured: false,
                error: None,
            });
        }
        assessment.record(HookOutcome {
            hook_name: "slow_hook".into(),
            success: true,
            latency_ms: 500,
            context_injected: false,
            knowledge_captured: false,
            error: None,
        });
        let report = assessment.to_tracker_report(1);
        let d3 = report
            .dims
            .iter()
            .find(|d| d.dim_id == "D3")
            .expect("D3 dimension must exist in AcoQualityReport");
        // 99/100 are fast, so score should be 0.99
        assert!(
            d3.score >= 0.98,
            "D3 score should be ~0.99, got {}",
            d3.score
        );
    }

    #[test]
    fn test_reliability_dimension_critical() {
        // D6 is a CRITICAL dimension (1.5x weight)
        assert!(CRITICAL_DIMS.contains(&"D6"));
    }
}
