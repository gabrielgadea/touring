//! NEW-3 — `touring tantivy gain [-j]` analytics CLI.
//!
//! Turns the raw `GateMetrics` counters into a human-readable token-savings
//! dashboard (or JSON via `-j` for CI consumption). Composes with the
//! Wave 2026-05-08 counters (phrase_query_match_count, tantivy_trigram_query_count,
//! tool_outputs_ttl_skip_count, tool_outputs_cleanup_deleted_count) plus the
//! NEW-1 `compression_profile_applied_count`.
//!
//! Ratio heuristic: tokens ≈ bytes / 4 (claude tokenizer ratio for English).
//! Documented in CLI help; future flag `--tokenizer claude` for tiktoken-rs
//! precision is deferred (NEW-Sprint-6).

use super::daemon_query;
use serde_json::Value;

/// Compact, structured report consumed by both human formatter and `-j`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GainReport {
    /// Total raw bytes of tool output routed through the compression layer
    /// (taken from `tool_output_raw_bytes`, falling back to `routed * 30_000`).
    pub bytes_routed_total: u64,
    /// Estimated tokens saved by compression, computed as `saved_bytes / 4`
    /// (the claude tokenizer bytes-per-token heuristic).
    pub tokens_saved_estimated: u64,
    /// Average compression ratio (`stored_bytes / raw_bytes`); `0.0` when no
    /// bytes have been routed yet.
    pub compression_ratio_avg: f64,
    /// Top 5 tools by routed-output count, as `(tool_name, count)` pairs
    /// sourced from the tantivy `tool_name` term aggregation.
    pub top_5_tools: Vec<(String, u64)>,
    /// Number of times a compression profile was applied
    /// (`compression_profile_applied_count`).
    pub profile_applications: u64,
    /// Number of sandbox tee outputs persisted to disk
    /// (`sandbox_tee_persisted_count`).
    pub tee_persisted_count: u64,
    /// Count of trigram-based tantivy queries (`tantivy_trigram_query_count`).
    pub trigram_queries: u64,
    /// Count of phrase-query matches (`phrase_query_match_count`).
    pub phrase_matches: u64,
    /// Count of tool-output cache writes skipped due to TTL
    /// (`tool_outputs_ttl_skip_count`).
    pub ttl_skips: u64,
    /// Count of tool-output cache entries removed by cleanup
    /// (`tool_outputs_cleanup_deleted_count`).
    pub cleanup_deleted: u64,
}

impl GainReport {
    /// Approximate token estimate. Documented heuristic (bytes/4).
    pub fn from_metrics(m: &Value, top_terms: &Value) -> Self {
        let routed = m
            .get("tool_output_routed_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let stored = m
            .get("tool_output_stored_bytes")
            .and_then(Value::as_u64)
            .or_else(|| m.get("tool_output_summary_bytes").and_then(Value::as_u64))
            .unwrap_or(0);
        let raw_total = m
            .get("tool_output_raw_bytes")
            .and_then(Value::as_u64)
            .unwrap_or(routed.saturating_mul(30_000));
        let saved_bytes = raw_total.saturating_sub(stored);
        let tokens_saved = saved_bytes / 4;
        let ratio = if raw_total > 0 {
            (stored as f64) / (raw_total as f64)
        } else {
            0.0
        };
        let top_5_tools = top_terms
            .get("buckets")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .take(5)
                    .filter_map(|b| {
                        let name = b.get("term").and_then(Value::as_str)?.to_string();
                        let count = b.get("count").and_then(Value::as_u64)?;
                        Some((name, count))
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self {
            bytes_routed_total: raw_total,
            tokens_saved_estimated: tokens_saved,
            compression_ratio_avg: ratio,
            top_5_tools,
            profile_applications: m
                .get("compression_profile_applied_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            tee_persisted_count: m
                .get("sandbox_tee_persisted_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            trigram_queries: m
                .get("tantivy_trigram_query_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            phrase_matches: m
                .get("phrase_query_match_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            ttl_skips: m
                .get("tool_outputs_ttl_skip_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cleanup_deleted: m
                .get("tool_outputs_cleanup_deleted_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        }
    }

    /// Render the report as a boxed ASCII dashboard for terminal display,
    /// listing every counter plus the top-5 tools table and the bytes/4
    /// token-estimate footnote.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str("┌─── touring tantivy gain ──────────────────────────────────┐\n");
        out.push_str(&format!(
            "│ bytes_routed_total       : {:>22}             │\n",
            self.bytes_routed_total
        ));
        out.push_str(&format!(
            "│ tokens_saved_estimated   : {:>22}  (bytes/4)   │\n",
            self.tokens_saved_estimated
        ));
        out.push_str(&format!(
            "│ compression_ratio_avg    : {:>22.4}             │\n",
            self.compression_ratio_avg
        ));
        out.push_str(&format!(
            "│ profile_applications     : {:>22}             │\n",
            self.profile_applications
        ));
        out.push_str(&format!(
            "│ tee_persisted_count      : {:>22}             │\n",
            self.tee_persisted_count
        ));
        out.push_str(&format!(
            "│ trigram_queries          : {:>22}             │\n",
            self.trigram_queries
        ));
        out.push_str(&format!(
            "│ phrase_matches           : {:>22}             │\n",
            self.phrase_matches
        ));
        out.push_str(&format!(
            "│ ttl_skips                : {:>22}             │\n",
            self.ttl_skips
        ));
        out.push_str(&format!(
            "│ cleanup_deleted          : {:>22}             │\n",
            self.cleanup_deleted
        ));
        out.push_str("├─── top 5 tools (by routed-output count) ─────────────────┤\n");
        if self.top_5_tools.is_empty() {
            out.push_str("│   (no tools routed yet)                                  │\n");
        } else {
            for (i, (tool, count)) in self.top_5_tools.iter().enumerate() {
                out.push_str(&format!(
                    "│ {:>2}. {:<35}  {:>10}      │\n",
                    i + 1,
                    truncate(tool, 35),
                    count
                ));
            }
        }
        out.push_str("└──────────────────────────────────────────────────────────┘\n");
        out.push_str(
            "Note: token estimate is bytes/4 (claude tokenizer heuristic). \
             Use `--tokenizer claude` (deferred) for precise tiktoken-rs counts.\n",
        );
        out
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

/// Entry point — `touring tantivy gain [-j]`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let json_mode = args.iter().any(|a| a == "-j" || a == "--json");
    let metrics_raw = daemon_query("cli-gate-metrics", serde_json::json!({}))?;
    let metrics: Value = serde_json::from_str(&metrics_raw).unwrap_or(Value::Null);
    // Top-5 tools via tantivy aggregate_terms; fall back to empty buckets.
    let top_terms_raw = daemon_query(
        "cli-tantivy-aggregate",
        serde_json::json!({"field": "tool_name", "top_k": 5}),
    )
    .unwrap_or_else(|_| serde_json::json!({"buckets": []}).to_string());
    let top_terms: Value = serde_json::from_str(&top_terms_raw).unwrap_or(Value::Null);
    let report = GainReport::from_metrics(&metrics, &top_terms);
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.render_human());
    }
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_gain_report_envelope() {
        let m = json!({
            "tool_output_routed_count": 10,
            "tool_output_raw_bytes": 100_000,
            "tool_output_stored_bytes": 10_000,
            "compression_profile_applied_count": 5,
        });
        let buckets = json!({"buckets": []});
        let r = GainReport::from_metrics(&m, &buckets);
        assert_eq!(r.bytes_routed_total, 100_000);
        assert_eq!(r.tokens_saved_estimated, (100_000 - 10_000) / 4);
        assert_eq!(r.profile_applications, 5);
    }

    #[test]
    fn test_gain_compression_ratio_calculation() {
        let m = json!({
            "tool_output_raw_bytes": 10_000_000u64,
            "tool_output_stored_bytes": 1_000_000u64,
        });
        let r = GainReport::from_metrics(&m, &json!({"buckets": []}));
        assert!((r.compression_ratio_avg - 0.10).abs() < 1e-6);
    }

    #[test]
    fn test_gain_top_5_tools_extracted() {
        let buckets = json!({
            "buckets": [
                {"term": "Bash", "count": 100},
                {"term": "Read", "count": 50},
                {"term": "Edit", "count": 25},
            ]
        });
        let r = GainReport::from_metrics(&json!({}), &buckets);
        assert_eq!(r.top_5_tools.len(), 3);
        assert_eq!(r.top_5_tools[0], ("Bash".to_string(), 100));
    }

    #[test]
    fn test_gain_json_flag_outputs_machine_readable() {
        let m = json!({
            "tool_output_routed_count": 1,
            "tool_output_raw_bytes": 4_000,
            "tool_output_stored_bytes": 400,
        });
        let r = GainReport::from_metrics(&m, &json!({"buckets": []}));
        let serialized = serde_json::to_string(&r).expect("serializable");
        let roundtrip: GainReport = serde_json::from_str(&serialized).expect("roundtrip");
        assert_eq!(roundtrip.bytes_routed_total, 4_000);
    }

    #[test]
    fn test_gain_human_output_contains_dashboard() {
        let r = GainReport {
            bytes_routed_total: 100,
            tokens_saved_estimated: 25,
            compression_ratio_avg: 0.5,
            top_5_tools: vec![("Bash".into(), 10)],
            profile_applications: 1,
            tee_persisted_count: 0,
            trigram_queries: 0,
            phrase_matches: 0,
            ttl_skips: 0,
            cleanup_deleted: 0,
        };
        let human = r.render_human();
        assert!(human.contains("touring tantivy gain"));
        assert!(human.contains("tokens_saved_estimated"));
        assert!(human.contains("Bash"));
        assert!(human.contains("bytes/4"));
    }
}
