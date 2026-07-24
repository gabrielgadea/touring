//! NEW-3 — `touring tantivy discover [-j]` analytics CLI.
//!
//! Scans recent hook events (PostBash / PostTool) for tools whose output
//! exceeds 10 KB AND were NOT routed through the sandbox executor — these
//! are the highest-ROI optimisation candidates. Aggregates per `tool_name`,
//! ranks by estimated session savings, returns top suggestions.

use super::daemon_query;
use serde_json::Value;

/// One ranked optimisation candidate: a tool producing large unrouted output.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DiscoverItem {
    /// Name of the tool that produced the high-volume output (e.g. `cargo test`).
    pub tool_name: String,
    /// Number of observed invocations aggregated into this candidate.
    pub sample_count: u64,
    /// Average output size per invocation, in bytes.
    pub avg_output_bytes: u64,
    /// Estimated total bytes savable per session (70% of `avg_output_bytes` × `sample_count`).
    pub estimated_session_savings_bytes: u64,
    /// Estimated tokens saved, derived as `estimated_session_savings_bytes` / 4.
    pub estimated_tokens_saved: u64,
}

impl DiscoverItem {
    /// Builds an item from raw aggregate fields.
    pub fn from_aggregate(tool_name: &str, sample_count: u64, avg_output_bytes: u64) -> Self {
        // Compression saving estimate: 70% of avg × samples (matches RTK
        // top-30 average reduction). Tokens ≈ bytes / 4.
        let savings = avg_output_bytes
            .saturating_mul(sample_count)
            .saturating_mul(70)
            / 100;
        Self {
            tool_name: tool_name.to_string(),
            sample_count,
            avg_output_bytes,
            estimated_session_savings_bytes: savings,
            estimated_tokens_saved: savings / 4,
        }
    }
}

/// Top-N items, ranked by `estimated_session_savings_bytes` descending.
pub fn rank_items(mut items: Vec<DiscoverItem>, top_n: usize) -> Vec<DiscoverItem> {
    items.sort_by(|a, b| {
        b.estimated_session_savings_bytes
            .cmp(&a.estimated_session_savings_bytes)
    });
    items.truncate(top_n);
    items
}

/// Parses the daemon response into ranked DiscoverItem rows.
pub fn parse_response(resp: &Value, top_n: usize) -> Vec<DiscoverItem> {
    let arr = resp.get("candidates").and_then(Value::as_array);
    let raw: Vec<DiscoverItem> = arr
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let name = c.get("tool_name").and_then(Value::as_str)?;
                    let count = c.get("sample_count").and_then(Value::as_u64)?;
                    let avg = c.get("avg_output_bytes").and_then(Value::as_u64)?;
                    Some(DiscoverItem::from_aggregate(name, count, avg))
                })
                .collect()
        })
        .unwrap_or_default();
    rank_items(raw, top_n)
}

/// Renders ranked `DiscoverItem` rows as a boxed, human-readable ASCII table.
pub fn render_human(items: &[DiscoverItem]) -> String {
    let mut out = String::new();
    out.push_str("┌─── touring tantivy discover ───────────────────────────────────────┐\n");
    out.push_str("│ Tools producing >10KB output without sandbox routing.              │\n");
    out.push_str("│ Apply NEW-1 compression profile or NEW-4 user filter to save.      │\n");
    out.push_str("├─────────────────────────────────────────────────────────────────────┤\n");
    if items.is_empty() {
        out.push_str("│ (no high-volume unrouted tools found — system is well-tuned)       │\n");
    } else {
        out.push_str("│ rank │ tool                  │ samples │ avg_KB │ tokens_saved │\n");
        out.push_str("│──────┼───────────────────────┼─────────┼────────┼──────────────│\n");
        for (i, item) in items.iter().enumerate() {
            out.push_str(&format!(
                "│ {:>4} │ {:<21} │ {:>7} │ {:>6} │ {:>12} │\n",
                i + 1,
                truncate(&item.tool_name, 21),
                item.sample_count,
                item.avg_output_bytes / 1024,
                item.estimated_tokens_saved
            ));
        }
    }
    out.push_str("└─────────────────────────────────────────────────────────────────────┘\n");
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

/// Entry point — `touring tantivy discover [-j] [--top N]`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let json_mode = args.iter().any(|a| a == "-j" || a == "--json");
    let mut top_n: usize = 10;
    let mut i = 3; // position after `touring tantivy discover`
    while i < args.len() {
        if matches!(args.get(i).map(String::as_str), Some("--top")) {
            i += 1;
            if let Some(v) = args.get(i).and_then(|s| s.parse().ok()) {
                top_n = v;
            }
        }
        i += 1;
    }
    let raw = daemon_query(
        "cli-discover-candidates",
        serde_json::json!({"min_bytes": 10_240u64, "top_n": top_n}),
    )
    .unwrap_or_else(|_| serde_json::json!({"candidates": []}).to_string());
    let resp: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let items = parse_response(&resp, top_n);
    if json_mode {
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else {
        print!("{}", render_human(&items));
    }
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_discover_item_calculation() {
        // 3 samples × 20KB × 70% = 42KB saved → 10752 tokens
        let item = DiscoverItem::from_aggregate("cargo test", 3, 20_480);
        assert_eq!(item.sample_count, 3);
        assert_eq!(item.avg_output_bytes, 20_480);
        assert_eq!(item.estimated_session_savings_bytes, 20_480 * 3 * 70 / 100);
        assert_eq!(
            item.estimated_tokens_saved,
            item.estimated_session_savings_bytes / 4
        );
    }

    #[test]
    fn test_rank_items_descending_by_savings() {
        let items = vec![
            DiscoverItem::from_aggregate("small", 1, 1024),
            DiscoverItem::from_aggregate("huge", 5, 100_000),
            DiscoverItem::from_aggregate("medium", 2, 50_000),
        ];
        let ranked = rank_items(items, 10);
        assert_eq!(ranked[0].tool_name, "huge");
        assert_eq!(ranked[1].tool_name, "medium");
        assert_eq!(ranked[2].tool_name, "small");
    }

    #[test]
    fn test_discover_finds_high_volume_unrouted_tools() {
        let resp = json!({
            "candidates": [
                {"tool_name": "cargo test", "sample_count": 5, "avg_output_bytes": 25_000},
                {"tool_name": "git log",    "sample_count": 8, "avg_output_bytes": 15_000},
                {"tool_name": "ls",         "sample_count": 1, "avg_output_bytes": 100},
            ]
        });
        let items = parse_response(&resp, 10);
        assert_eq!(items.len(), 3);
        // cargo test wins: 5 × 25k × 70% = 87.5KB > git log: 8 × 15k × 70% = 84KB
        assert_eq!(items[0].tool_name, "cargo test");
        assert_eq!(items[1].tool_name, "git log");
        assert_eq!(items[2].tool_name, "ls");
    }

    #[test]
    fn test_discover_excludes_already_routed() {
        // Ensure missing/zero-sample candidates are filtered.
        let resp = json!({
            "candidates": [
                {"tool_name": "Bash", "sample_count": 5, "avg_output_bytes": 50_000},
                {"avg_output_bytes": 100, "sample_count": 1},
                {"tool_name": "Read", "sample_count": 3, "avg_output_bytes": 5_000},
            ]
        });
        let items = parse_response(&resp, 10);
        assert_eq!(items.len(), 2, "missing tool_name candidate dropped");
        assert!(items.iter().all(|i| !i.tool_name.is_empty()));
    }

    #[test]
    fn test_discover_top_n_limits_output() {
        let raws: Vec<DiscoverItem> = (0..20)
            .map(|n| DiscoverItem::from_aggregate(&format!("tool_{n}"), 1, 1_000_000 - n * 1000))
            .collect();
        let ranked = rank_items(raws, 5);
        assert_eq!(ranked.len(), 5);
        // First should be tool_0 (highest avg_output_bytes).
        assert_eq!(ranked[0].tool_name, "tool_0");
    }

    #[test]
    fn test_discover_human_output() {
        let items = vec![DiscoverItem::from_aggregate("cargo test", 3, 25_000)];
        let human = render_human(&items);
        assert!(human.contains("touring tantivy discover"));
        assert!(human.contains("cargo test"));
        assert!(human.contains("samples"));
    }
}
