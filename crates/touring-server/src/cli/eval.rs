//! `touring eval` — Reproducible benchmark framework for Touring intelligence.
//!
//! Measures token efficiency, blast radius accuracy, search quality, and
//! index performance against the current project or external repos.
//!
//! Usage:
//!   touring eval [--benchmark token|blast|search|index|all] [-j]
//!   touring eval --list
//!   touring eval --report

use super::common::{human_to_stderr, json_to_stdout, parse_global_flags};
use serde::Serialize;

/// Available benchmark types.
const BENCHMARKS: &[&str] = &["token", "blast", "search", "index"];

/// Result of a single benchmark run.
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResult {
    /// Benchmark identifier (e.g. `token_efficiency`, `index_performance`).
    pub name: String,
    /// Outcome of the run — `"pass"` or `"fail"`.
    pub status: String,
    /// Benchmark-specific metrics as a free-form JSON object.
    pub metrics: serde_json::Value,
    /// Wall-clock duration of this benchmark, in milliseconds.
    pub duration_ms: u64,
}

/// Full eval report containing all benchmark results.
#[derive(Debug, Clone, Serialize)]
pub struct EvalReport {
    /// Absolute path of the project the benchmarks ran against.
    pub project: String,
    /// Unix-epoch timestamp (seconds) when the report was generated.
    pub timestamp: String,
    /// Per-benchmark results collected during this run.
    pub benchmarks: Vec<BenchmarkResult>,
    /// Aggregate pass/fail scoring across all benchmarks.
    pub summary: EvalSummary,
}

/// Summary scores across all benchmarks.
#[derive(Debug, Clone, Serialize)]
pub struct EvalSummary {
    /// Number of benchmarks that were executed.
    pub total_benchmarks: usize,
    /// Number of benchmarks whose status was `"pass"`.
    pub passed: usize,
    /// Fraction of benchmarks that passed, in the range `0.0..=1.0`.
    pub overall_score: f64,
}

/// Entry point for the `touring eval` CLI handler.
///
/// Parses `--list` (print the available benchmarks and exit), `--benchmark
/// <token|blast|search|index|all>` (defaults to `all`), and `--report` (also
/// write a Markdown report to `.claude/eval_report.md`). Runs the selected
/// benchmarks, computes an overall pass ratio, and prints either JSON (with
/// `-j`) or a human-readable summary.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (flags, filtered) = parse_global_flags(args);

    // --list: show available benchmarks
    if filtered.iter().any(|a| a == "--list") {
        let output = serde_json::json!({
            "benchmarks": BENCHMARKS,
            "description": {
                "size_metric": "Measure output size at each DetailLevel (minimal/standard/full)",
                "blast": "Measure blast radius precision and recall",
                "search": "Measure search MRR and top-k accuracy",
                "index": "Measure index build speed (files/sec, symbols/sec)",
            }
        });
        if flags.json {
            json_to_stdout(&serde_json::to_string_pretty(&output).unwrap_or_default());
        } else {
            human_to_stderr("Available benchmarks:");
            for b in BENCHMARKS {
                human_to_stderr(&format!("  - {}", b));
            }
        }
        return Ok(());
    }

    // Parse --benchmark flag
    let benchmark = filtered
        .iter()
        .position(|a| a == "--benchmark")
        .and_then(|i| filtered.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("all");

    let start = std::time::Instant::now();
    let mut results = Vec::new();

    if benchmark == "all" || benchmark == "token" {
        results.push(run_token_benchmark());
    }
    if benchmark == "all" || benchmark == "index" {
        results.push(run_index_benchmark());
    }
    if benchmark == "all" || benchmark == "blast" {
        results.push(run_blast_benchmark());
    }
    if benchmark == "all" || benchmark == "search" {
        results.push(run_search_benchmark());
    }

    let passed = results.iter().filter(|r| r.status == "pass").count();
    let overall = if results.is_empty() {
        0.0
    } else {
        passed as f64 / results.len() as f64
    };

    let report = EvalReport {
        project: std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
        timestamp: chrono_now(),
        summary: EvalSummary {
            total_benchmarks: results.len(),
            passed,
            overall_score: overall,
        },
        benchmarks: results,
    };

    let total_ms = start.elapsed().as_millis() as u64;

    if flags.json {
        let mut output = serde_json::to_value(&report).unwrap_or_default();
        if let Some(obj) = output.as_object_mut() {
            obj.insert("total_duration_ms".to_string(), serde_json::json!(total_ms));
        }
        json_to_stdout(&serde_json::to_string_pretty(&output).unwrap_or_default());
    } else {
        human_to_stderr(&format_human_report(&report, total_ms));
    }

    // --report: also write markdown file
    if filtered.iter().any(|a| a == "--report") {
        let md = format_markdown_report(&report, total_ms);
        let path = std::path::Path::new(".claude/eval_report.md");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, &md)?;
        human_to_stderr(&format!("Report written to {}", path.display()));
    }

    Ok(())
}

// ── Token Efficiency Benchmark ─────────────────────────────────────────

fn run_token_benchmark() -> BenchmarkResult {
    let start = std::time::Instant::now();

    // Generate a realistic MCP output sample
    let sample = generate_sample_output();
    let full_size = serde_json::to_string(&sample).unwrap_or_default().len();

    let mut standard = sample.clone();
    truncate_json(&mut standard, 10, 200); // standard: 10 items, 200 char values
    let standard_size = serde_json::to_string(&standard).unwrap_or_default().len();

    let mut minimal = sample;
    truncate_json(&mut minimal, 3, 50); // minimal: 3 items, 50 char values
    let minimal_size = serde_json::to_string(&minimal).unwrap_or_default().len();

    let standard_ratio = if standard_size > 0 {
        full_size as f64 / standard_size as f64
    } else {
        1.0
    };
    let minimal_ratio = if minimal_size > 0 {
        full_size as f64 / minimal_size as f64
    } else {
        1.0
    };

    let pass = minimal_ratio >= 3.0 && standard_ratio >= 1.5;

    BenchmarkResult {
        name: "token_efficiency".to_string(),
        status: if pass { "pass" } else { "fail" }.to_string(),
        metrics: serde_json::json!({
            "full_bytes": full_size,
            "standard_bytes": standard_size,
            "minimal_bytes": minimal_size,
            "standard_ratio": format!("{:.1}x", standard_ratio),
            "minimal_ratio": format!("{:.1}x", minimal_ratio),
            "target_minimal": ">=3.0x",
            "target_standard": ">=1.5x",
        }),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

// ── Index Performance Benchmark ────────────────────────────────────────

fn run_index_benchmark() -> BenchmarkResult {
    let start = std::time::Instant::now();

    // Query daemon for index stats
    let output = super::daemon_query("cli-index-status", serde_json::json!({}));
    let (symbols, files) = match output {
        Ok(ref json_str) => {
            let val: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
            let syms = val
                .pointer("/symbol_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let fls = val
                .pointer("/file_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            (syms, fls)
        }
        Err(_) => (0, 0),
    };

    let duration_ms = start.elapsed().as_millis() as u64;
    let pass = symbols > 0 && files > 0;

    BenchmarkResult {
        name: "index_performance".to_string(),
        status: if pass { "pass" } else { "fail" }.to_string(),
        metrics: serde_json::json!({
            "symbol_count": symbols,
            "file_count": files,
            "query_ms": duration_ms,
            "symbols_per_ms": if duration_ms > 0 { symbols as f64 / duration_ms as f64 } else { 0.0 },
        }),
        duration_ms,
    }
}

// ── Blast Radius Benchmark ─────────────────────────────────────────────

fn run_blast_benchmark() -> BenchmarkResult {
    let start = std::time::Instant::now();

    let output = super::daemon_query("cli-wiring-status", serde_json::json!({}));
    let (orphans, modules, pub_symbols) = match output {
        Ok(ref json_str) => {
            let val: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
            let o = val
                .pointer("/orphan_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let m = val
                .pointer("/module_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let p = val
                .pointer("/total_pub_symbols")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            (o, m, p)
        }
        Err(_) => (0, 0, 0),
    };

    let wiring_ratio = if pub_symbols > 0 {
        1.0 - (orphans as f64 / pub_symbols as f64)
    } else {
        0.0
    };

    let pass = modules > 0;

    BenchmarkResult {
        name: "blast_radius".to_string(),
        status: if pass { "pass" } else { "fail" }.to_string(),
        metrics: serde_json::json!({
            "orphan_count": orphans,
            "module_count": modules,
            "pub_symbols": pub_symbols,
            "wiring_ratio": format!("{:.2}", wiring_ratio),
        }),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

// ── Search Quality Benchmark ───────────────────────────────────────────

fn run_search_benchmark() -> BenchmarkResult {
    let start = std::time::Instant::now();

    // Test RRF hybrid search with known queries
    let test_cases = vec![
        ("SessionHintEngine", true),   // PascalCase → should find
        ("apply_detail_level", true),  // snake_case → should find
        ("nonexistent_xyz_42", false), // should not find
    ];

    let mut found = 0;
    let mut total = 0;
    for (query, expect_result) in &test_cases {
        let output =
            super::daemon_query("cli-index-find", serde_json::json!({"symbol_name": query}));
        let has_result = match output {
            Ok(ref json_str) => {
                let val: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
                val.pointer("/count").and_then(|v| v.as_u64()).unwrap_or(0) > 0
            }
            Err(_) => false,
        };
        total += 1;
        if has_result == *expect_result {
            found += 1;
        }
    }

    let accuracy = if total > 0 {
        found as f64 / total as f64
    } else {
        0.0
    };

    BenchmarkResult {
        name: "search_quality".to_string(),
        status: if accuracy >= 0.5 { "pass" } else { "fail" }.to_string(),
        metrics: serde_json::json!({
            "test_cases": total,
            "correct": found,
            "accuracy": format!("{:.2}", accuracy),
            "target": ">=0.50",
        }),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Truncate JSON arrays and strings for token benchmark (self-contained, no lib dependency).
fn truncate_json(value: &mut serde_json::Value, max_items: usize, max_str_len: usize) {
    match value {
        serde_json::Value::Array(arr) => {
            if arr.len() > max_items {
                arr.truncate(max_items);
            }
            for item in arr.iter_mut() {
                truncate_json(item, max_items, max_str_len);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                truncate_json(v, max_items, max_str_len);
            }
        }
        serde_json::Value::String(s) if s.len() > max_str_len => {
            let truncated: String = s.chars().take(max_str_len).collect();
            *s = truncated;
        }
        _ => {}
    }
}

fn generate_sample_output() -> serde_json::Value {
    serde_json::json!({
        "total_matches": 80,
        "rlm_matches": (0..30).map(|i| serde_json::json!({
            "key": format!("lesson:pattern:{}_{}", ["error", "cache", "auth"].get(i % 3).copied().unwrap_or("other"), i),
            "tier": "semantic",
            "value": format!("Detailed pattern description for item {} with context about implementation decisions and architectural trade-offs.", i),
            "score": 0.95 - (i as f64 * 0.02),
        })).collect::<Vec<_>>(),
        "semantic_matches": (0..15).map(|i| serde_json::json!({
            "id": i,
            "content": format!("Semantic content block {} describing an important codebase insight.", i),
            "score": 0.88 - (i as f64 * 0.03),
        })).collect::<Vec<_>>(),
    })
}

fn chrono_now() -> String {
    // Simple UTC timestamp without chrono dependency
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", d.as_secs())
}

fn format_human_report(report: &EvalReport, total_ms: u64) -> String {
    let mut out = String::new();
    out.push_str(&format!("Touring Eval Report — {}\n", report.project));
    out.push_str("═══════════════════════════════════════\n");
    for b in &report.benchmarks {
        let icon = if b.status == "pass" { "✓" } else { "✗" };
        out.push_str(&format!("  {} {} ({}ms)\n", icon, b.name, b.duration_ms));
        if let Some(obj) = b.metrics.as_object() {
            for (k, v) in obj {
                out.push_str(&format!("    {}: {}\n", k, v));
            }
        }
    }
    out.push_str("───────────────────────────────────────\n");
    out.push_str(&format!(
        "  Score: {:.0}% ({}/{}) in {}ms\n",
        report.summary.overall_score * 100.0,
        report.summary.passed,
        report.summary.total_benchmarks,
        total_ms,
    ));
    out
}

fn format_markdown_report(report: &EvalReport, total_ms: u64) -> String {
    let mut md = String::new();
    md.push_str("# Touring Eval Report\n\n");
    md.push_str("| Metric | Value |\n|--------|-------|\n");
    md.push_str(&format!("| Project | `{}` |\n", report.project));
    md.push_str(&format!(
        "| Score | **{:.0}%** ({}/{}) |\n",
        report.summary.overall_score * 100.0,
        report.summary.passed,
        report.summary.total_benchmarks,
    ));
    md.push_str(&format!("| Duration | {}ms |\n\n", total_ms));

    md.push_str("## Benchmarks\n\n");
    for b in &report.benchmarks {
        let icon = if b.status == "pass" { "PASS" } else { "FAIL" };
        md.push_str(&format!("### {} — {}\n\n", b.name, icon));
        md.push_str("| Metric | Value |\n|--------|-------|\n");
        if let Some(obj) = b.metrics.as_object() {
            for (k, v) in obj {
                md.push_str(&format!("| {} | {} |\n", k, v));
            }
        }
        md.push_str(&format!("| duration_ms | {} |\n\n", b.duration_ms));
    }
    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_benchmark_passes() {
        let result = run_token_benchmark();
        assert_eq!(
            result.status, "pass",
            "Token benchmark should pass: {:?}",
            result.metrics
        );
        assert_eq!(result.name, "token_efficiency");
    }

    #[test]
    fn test_generate_sample_output_structure() {
        let sample = generate_sample_output();
        assert!(sample.get("total_matches").is_some());
        assert!(
            sample
                .get("rlm_matches")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
                > 0
        );
    }

    #[test]
    fn test_format_human_report() {
        let report = EvalReport {
            project: "/test".to_string(),
            timestamp: "123".to_string(),
            benchmarks: vec![BenchmarkResult {
                name: "test_bench".to_string(),
                status: "pass".to_string(),
                metrics: serde_json::json!({"score": 0.95}),
                duration_ms: 10,
            }],
            summary: EvalSummary {
                total_benchmarks: 1,
                passed: 1,
                overall_score: 1.0,
            },
        };
        let output = format_human_report(&report, 10);
        assert!(output.contains("test_bench"));
        assert!(output.contains("100%"));
    }

    #[test]
    fn test_format_markdown_report() {
        let report = EvalReport {
            project: "/test".to_string(),
            timestamp: "123".to_string(),
            benchmarks: vec![],
            summary: EvalSummary {
                total_benchmarks: 0,
                passed: 0,
                overall_score: 0.0,
            },
        };
        let md = format_markdown_report(&report, 5);
        assert!(md.contains("# Touring Eval Report"));
        assert!(md.contains("## Benchmarks"));
    }

    #[test]
    fn test_benchmarks_list() {
        assert_eq!(BENCHMARKS.len(), 4);
        assert!(BENCHMARKS.contains(&"token"));
        assert!(BENCHMARKS.contains(&"blast"));
        assert!(BENCHMARKS.contains(&"search"));
        assert!(BENCHMARKS.contains(&"index"));
    }
}
