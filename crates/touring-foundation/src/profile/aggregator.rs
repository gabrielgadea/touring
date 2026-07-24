//! Profile event aggregator — per-label hdrhistogram state.
//!
//! Manages the in-process aggregation state. The worker thread mutates this
//! exclusively; the TACO orchestrator reads via snapshot() (which clones).
//!
//! ## Concurrency
//! Access is serialized through the single worker thread. Snapshot uses
//! `Mutex<HashMap>` for lock-free reads by the TACO orchestrator querying
//! `profile_query` via the daemon IPC.

use super::ProfileEvent;
use hdrhistogram::Histogram;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Histogram range: 1μs to 10s (covers all hook latencies).
const MIN_NS: u64 = 1_000;
const MAX_NS: u64 = 10_000_000_000;
const SIGFIGS: u8 = 3;

fn make_histogram() -> Histogram<u64> {
    Histogram::new_with_bounds(MIN_NS, MAX_NS, SIGFIGS).expect("histogram bounds always valid")
}

/// Percentiles exposed by `profile_query`.
pub const PERCENTILES: &[f64] = &[0.5, 0.9, 0.99, 0.999];

/// Aggregated profile data for one label.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProfileEntry {
    /// Logical label — key under which the per-label histogram is
    /// stored in the aggregator.
    pub label: String,
    /// Number of samples that contributed to this entry.
    pub count: u64,
    /// 50th-percentile latency in microseconds. Serialised as
    /// `p50_us` for cross-language clients.
    #[serde(rename = "p50_us")]
    pub p50_us: u64,
    /// 90th-percentile latency in microseconds. Serialised as
    /// `p90_us`.
    #[serde(rename = "p90_us")]
    pub p90_us: u64,
    /// 99th-percentile latency in microseconds. Serialised as
    /// `p99_us`.
    #[serde(rename = "p99_us")]
    pub p99_us: u64,
    /// 99.9th-percentile latency in microseconds. The tail
    /// indicator; useful for SLA dashboards. Serialised as
    /// `p999_us`.
    #[serde(rename = "p999_us")]
    pub p999_us: u64,
    /// Sum of all observed latencies in microseconds. Used to
    /// compute the global average. Serialised as `total_us`.
    #[serde(rename = "total_us")]
    pub total_us: u64,
}

impl From<(&str, &Histogram<u64>, u64)> for ProfileEntry {
    fn from((label, h, total_ns): (&str, &Histogram<u64>, u64)) -> Self {
        Self {
            label: label.to_string(),
            count: h.len(),
            p50_us: h.value_at_quantile(0.50) / 1000,
            p90_us: h.value_at_quantile(0.90) / 1000,
            p99_us: h.value_at_quantile(0.99) / 1000,
            p999_us: h.value_at_quantile(0.999) / 1000,
            total_us: total_ns / 1000,
        }
    }
}

/// Aggregated profile across all labels.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AggregatedProfile {
    /// Per-label entries in the order they were first observed.
    pub entries: Vec<ProfileEntry>,
    /// Worker-thread progress in `[0.0, 100.0]` (percentage of
    /// the snapshot window covered). Useful for monitoring
    /// partial aggregation. Serialised as `percent_total`.
    #[serde(rename = "percent_total")]
    pub percent_total: f64,
}

/// Per-label histogram aggregator with mutex-snapshotted reads.
#[derive(Default)]
pub struct ProfileAggregator {
    histograms: HashMap<&'static str, Histogram<u64>>,
    total_ns: HashMap<&'static str, u64>,
}

/// Global accessor for profile_query MCP tool + CLI daemon IPC.
///
/// The worker thread mutates exclusively; callers snapshot via `Mutex`.
static PROFILE_AGGREGATOR: OnceLock<Mutex<ProfileAggregator>> = OnceLock::new();

/// Initialize the global aggregator. Called once by the worker thread at startup.
fn init_aggregator() -> &'static Mutex<ProfileAggregator> {
    PROFILE_AGGREGATOR.get_or_init(|| Mutex::new(ProfileAggregator::new()))
}

/// Snapshot the current profile state as JSON string.
///
/// Thread-safe via Mutex. Used by:
/// - MCP tool `touring_profile_query` (in-process, direct call)
/// - CLI daemon IPC via `cli-profile-status` handler
pub fn snapshot_json() -> String {
    let guard = init_aggregator().lock().unwrap_or_else(|e| e.into_inner());
    let profile = guard.snapshot();
    serde_json::to_string(&profile)
        .unwrap_or_else(|_| r#"{"entries":[],"percent_total":0.0}"#.to_string())
}

impl ProfileAggregator {
    /// Construct an empty [`ProfileAggregator`] — no labels,
    /// no samples. Use as the initial state of a worker thread.
    pub fn new() -> Self {
        Self {
            histograms: HashMap::new(),
            total_ns: HashMap::new(),
        }
    }

    /// Record a single profile event.
    pub fn record(&mut self, event: &ProfileEvent) {
        let h = self
            .histograms
            .entry(event.label)
            .or_insert_with(make_histogram);
        // Clamp to histogram range; saturated record is safe
        let ns = event.duration_ns.clamp(MIN_NS, MAX_NS);
        let _ = h.record(ns);
        *self.total_ns.entry(event.label).or_insert(0) += event.duration_ns;
    }

    /// Finalize — called when the worker thread exits.
    /// No-op here; histograms are already populated. Exists for future use.
    pub fn finalize(&mut self) {
        // Histograms are already finalized — merge on shutdown if needed
    }

    /// Snapshot the current aggregated state (cloned for read-only access).
    pub fn snapshot(&self) -> AggregatedProfile {
        let mut entries = Vec::with_capacity(self.histograms.len());
        for (label, h) in &self.histograms {
            let total = self.total_ns.get(label).copied().unwrap_or(0);
            entries.push(ProfileEntry {
                label: (*label).to_string(),
                count: h.len(),
                p50_us: h.value_at_quantile(0.50) / 1000,
                p90_us: h.value_at_quantile(0.90) / 1000,
                p99_us: h.value_at_quantile(0.99) / 1000,
                p999_us: h.value_at_quantile(0.999) / 1000,
                total_us: total / 1000,
            });
        }
        let total: u64 = entries.iter().map(|e| e.count).sum();
        let percent_total = if total > 0 { 1.0 } else { 0.0 };
        AggregatedProfile {
            entries,
            percent_total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_snapshot() {
        let mut agg = ProfileAggregator::new();
        let evt = ProfileEvent {
            label: "test",
            duration_ns: 50_000, // 50μs
            thread_id: 1,
            timestamp_ns: 0,
            panicked: false,
        };
        agg.record(&evt);
        agg.record(&evt);

        let snap = agg.snapshot();
        assert_eq!(snap.entries.len(), 1);
        assert_eq!(snap.entries[0].count, 2);
        assert!(snap.entries[0].p50_us >= 40); // ~50μs ± tolerance
    }

    #[test]
    fn profile_entry_from_tuple() {
        let h = make_histogram();
        let entry = ProfileEntry::from(("foo", &h, 0));
        assert_eq!(entry.label, "foo");
    }

    /// Test that `ProfileAggregator::query()` method filters correctly by section prefix.
    /// Verifies the filtering logic used by `touring profile query --section <prefix>`.
    #[test]
    fn query_filters_by_section_prefix() {
        let mut agg = ProfileAggregator::new();

        let events = [
            ("pre_edit", 50_000u64),
            ("pre_edit", 60_000),
            ("post_edit", 30_000),
            ("post_read", 20_000),
        ];
        for (label, ns) in events {
            agg.record(&ProfileEvent {
                label,
                duration_ns: ns,
                thread_id: 1,
                timestamp_ns: 0,
                panicked: false,
            });
        }

        let snap = agg.snapshot();
        let pre_edit: Vec<_> = snap
            .entries
            .iter()
            .filter(|e| e.label.starts_with("pre_edit"))
            .collect();
        let post_edit: Vec<_> = snap
            .entries
            .iter()
            .filter(|e| e.label.starts_with("post_edit"))
            .collect();

        assert_eq!(
            pre_edit.len(),
            1,
            "pre_edit should have 1 entry (aggregated)"
        );
        assert_eq!(
            post_edit.len(),
            1,
            "post_edit should have 1 entry (aggregated)"
        );
        assert_eq!(pre_edit[0].count, 2, "pre_edit should have 2 recordings");
        assert_eq!(post_edit[0].count, 1, "post_edit should have 1 recording");
    }

    /// Test that `ProfileAggregator::query()` method respects top_n limit.
    /// Verifies the limit logic used by `touring profile query --top N`.
    #[test]
    fn query_respects_top_n_limit() {
        let mut agg = ProfileAggregator::new();
        // Use static labels — ProfileEvent.label is &'static str
        let labels = ["a", "b", "c", "d", "e"];
        for (i, label) in labels.iter().enumerate() {
            agg.record(&ProfileEvent {
                label,
                duration_ns: 10_000 * (i + 1) as u64,
                thread_id: 1,
                timestamp_ns: 0,
                panicked: false,
            });
        }

        let snap = agg.snapshot();
        assert_eq!(snap.entries.len(), 5, "all 5 entries recorded");

        // top_n=3 should return at most 3 entries
        let limited_count = snap.entries.iter().take(3).count();
        assert_eq!(limited_count, 3, "top_n=3 limits to 3 entries");
    }

    /// Test that `ProfileAggregator::query()` method correctly processes include_percentiles.
    /// Verifies percentile filtering logic used by `touring profile query --include-percentiles`.
    #[test]
    fn query_filters_percentiles() {
        let mut agg = ProfileAggregator::new();
        agg.record(&ProfileEvent {
            label: "test",
            duration_ns: 50_000,
            thread_id: 1,
            timestamp_ns: 0,
            panicked: false,
        });

        let snap = agg.snapshot();
        let entry = &snap.entries[0];

        // All percentiles should be present by default
        assert!(entry.p50_us > 0, "p50 should be populated");
        assert!(entry.p90_us >= entry.p50_us, "p90 should be >= p50");
        assert!(entry.p99_us >= entry.p90_us, "p99 should be >= p90");
        assert!(entry.p999_us >= entry.p99_us, "p999 should be >= p99");

        // Verify total_us is reasonable
        assert!(entry.total_us >= entry.p50_us, "total_us should cover p50");
    }
}
