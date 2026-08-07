//! Prometheus-compatible metrics counters for the analysis server.
//!
//! Uses `std::sync::atomic::AtomicU64` for lock-free counter operations.
//! Designed for use with the Prometheus text format exposition (manual impl).
//! No external prometheus-client dependency — counters exported via `touring_metrics`
//! MCP tool as plain text in Prometheus format.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global metrics registry.
static METRICS: OnceLock<AnalysisServerMetrics> = OnceLock::new();

/// Server-level metrics counters and histograms.
///
/// All counters are monotonically increasing (never reset in process lifetime).
/// The `wiring_orphan_count` field is a gauge — overwritten on each update.
pub struct AnalysisServerMetrics {
    /// Total number of analysis runs.
    pub analysis_run_total: AtomicU64,
    /// Total number of symbol lookups.
    pub symbol_lookup_total: AtomicU64,
    /// Total cache hits in the analysis pipeline.
    pub cache_hit_total: AtomicU64,
    /// Total cache misses in the analysis pipeline.
    pub cache_miss_total: AtomicU64,
    /// Current count of wiring orphans (gauge, overwritten on each update).
    pub wiring_orphan_count: AtomicU64,
    /// Total analysis duration in milliseconds (sum for histogram approximation).
    pub analysis_duration_ms_total: AtomicU64,
}

impl Default for AnalysisServerMetrics {
    fn default() -> Self {
        Self {
            analysis_run_total: AtomicU64::new(0),
            symbol_lookup_total: AtomicU64::new(0),
            cache_hit_total: AtomicU64::new(0),
            cache_miss_total: AtomicU64::new(0),
            wiring_orphan_count: AtomicU64::new(0),
            analysis_duration_ms_total: AtomicU64::new(0),
        }
    }
}

impl AnalysisServerMetrics {
    /// Get or initialize the global metrics instance.
    pub fn global() -> &'static Self {
        METRICS.get_or_init(Default::default)
    }

    /// Increment `analysis_run_total`.
    pub fn inc_analysis_run(&self) {
        self.analysis_run_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment `symbol_lookup_total`.
    pub fn inc_symbol_lookup(&self) {
        self.symbol_lookup_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment `cache_hit_total`.
    pub fn inc_cache_hit(&self) {
        self.cache_hit_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment `cache_miss_total`.
    pub fn inc_cache_miss(&self) {
        self.cache_miss_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Set the current wiring orphan count (gauge — overwrites previous value).
    pub fn set_wiring_orphans(&self, count: u64) {
        self.wiring_orphan_count.store(count, Ordering::Relaxed);
    }

    /// Add `ms` milliseconds to the running duration total.
    pub fn add_duration_ms(&self, ms: u64) {
        self.analysis_duration_ms_total
            .fetch_add(ms, Ordering::Relaxed);
    }

    /// Render all counters in Prometheus text exposition format.
    ///
    /// Returns a string suitable for the `touring_metrics` MCP tool response.
    /// Export all metrics in Prometheus exposition format.
    pub fn render_prometheus(&self) -> String {
        self.render_filtered(None)
    }

    /// Export metrics in Prometheus exposition format, optionally filtered by name prefix.
    ///
    /// When `filter` is `Some(prefix)`, only metric blocks whose name starts with
    /// `prefix` are included. This allows callers to request e.g. `"analysis"` to
    /// receive only `analysis_*` counters.
    pub fn render_filtered(&self, filter: Option<&str>) -> String {
        let metrics: &[(&str, &str, u64)] = &[
            (
                "analysis_run_total",
                "counter",
                self.analysis_run_total.load(Ordering::Relaxed),
            ),
            (
                "symbol_lookup_total",
                "counter",
                self.symbol_lookup_total.load(Ordering::Relaxed),
            ),
            (
                "cache_hit_total",
                "counter",
                self.cache_hit_total.load(Ordering::Relaxed),
            ),
            (
                "cache_miss_total",
                "counter",
                self.cache_miss_total.load(Ordering::Relaxed),
            ),
            (
                "wiring_orphan_count",
                "gauge",
                self.wiring_orphan_count.load(Ordering::Relaxed),
            ),
            (
                "analysis_duration_ms_total",
                "counter",
                self.analysis_duration_ms_total.load(Ordering::Relaxed),
            ),
        ];

        let mut out = String::with_capacity(512);
        for &(name, kind, value) in metrics {
            if let Some(prefix) = filter
                && !name.starts_with(prefix)
            {
                continue;
            }
            out.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}\n",
                help = name.replace('_', " "),
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inc_analysis_run_increments() {
        let m = AnalysisServerMetrics::default();
        assert_eq!(m.analysis_run_total.load(Ordering::Relaxed), 0);
        m.inc_analysis_run();
        assert_eq!(m.analysis_run_total.load(Ordering::Relaxed), 1);
        m.inc_analysis_run();
        assert_eq!(m.analysis_run_total.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_inc_symbol_lookup_increments() {
        let m = AnalysisServerMetrics::default();
        m.inc_symbol_lookup();
        m.inc_symbol_lookup();
        m.inc_symbol_lookup();
        assert_eq!(m.symbol_lookup_total.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_cache_hit_miss_increments() {
        let m = AnalysisServerMetrics::default();
        m.inc_cache_hit();
        m.inc_cache_hit();
        m.inc_cache_miss();
        assert_eq!(m.cache_hit_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.cache_miss_total.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_set_wiring_orphans_overwrites() {
        let m = AnalysisServerMetrics::default();
        m.set_wiring_orphans(42);
        assert_eq!(m.wiring_orphan_count.load(Ordering::Relaxed), 42);
        m.set_wiring_orphans(7);
        assert_eq!(m.wiring_orphan_count.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn test_add_duration_ms_accumulates() {
        let m = AnalysisServerMetrics::default();
        m.add_duration_ms(100);
        m.add_duration_ms(250);
        assert_eq!(m.analysis_duration_ms_total.load(Ordering::Relaxed), 350);
    }

    #[test]
    fn test_render_prometheus_includes_all_metric_names() {
        let m = AnalysisServerMetrics::default();
        m.inc_analysis_run();
        m.inc_symbol_lookup();
        m.inc_cache_hit();
        m.inc_cache_miss();
        m.set_wiring_orphans(5);
        m.add_duration_ms(120);
        let output = m.render_prometheus();
        assert!(
            output.contains("analysis_run_total"),
            "missing analysis_run_total"
        );
        assert!(
            output.contains("symbol_lookup_total"),
            "missing symbol_lookup_total"
        );
        assert!(
            output.contains("cache_hit_total"),
            "missing cache_hit_total"
        );
        assert!(
            output.contains("cache_miss_total"),
            "missing cache_miss_total"
        );
        assert!(
            output.contains("wiring_orphan_count"),
            "missing wiring_orphan_count"
        );
        assert!(
            output.contains("analysis_duration_ms_total"),
            "missing analysis_duration_ms_total"
        );
        assert!(
            output.contains("# TYPE analysis_run_total counter"),
            "missing counter type"
        );
        assert!(
            output.contains("# TYPE wiring_orphan_count gauge"),
            "missing gauge type"
        );
        assert!(
            output.contains("analysis_run_total 1\n"),
            "wrong counter value"
        );
        assert!(
            output.contains("wiring_orphan_count 5\n"),
            "wrong gauge value"
        );
        assert!(
            output.contains("analysis_duration_ms_total 120\n"),
            "wrong duration value"
        );
    }

    #[test]
    fn test_global_returns_same_instance() {
        let a = AnalysisServerMetrics::global();
        let b = AnalysisServerMetrics::global();
        // Both pointers point to the same address
        assert!(std::ptr::eq(a, b));
    }
}
