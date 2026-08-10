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
    // Re-recording the retrieval baseline is a DELIBERATE act, never a side effect of
    // running the suite: the whole value of the clause is that "the number moved"
    // cannot be resolved by quietly moving the target with it.
    let update_baseline = filtered.iter().any(|a| a == "--update-baseline");
    if benchmark == "all" || benchmark == "blast" {
        results.push(run_blast_benchmark());
    }
    if benchmark == "all" || benchmark == "search" {
        results.push(run_search_benchmark(update_baseline));
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

/// One curated retrieval case: a query and the symbol ids that answer it.
#[derive(Debug, serde::Deserialize)]
struct RetrievalCase {
    id: String,
    tier: String,
    query: String,
    /// `path/to/file.rs::Symbol` — ground truth, verified unique in the index.
    expected: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RetrievalFixture {
    cases: Vec<RetrievalCase>,
}

/// Recall/MRR aggregate over a set of cases.
#[derive(Debug, Default, Clone, Serialize)]
struct RecallStats {
    cases: usize,
    r_at_1: f64,
    r_at_5: f64,
    r_at_20: f64,
    mrr: f64,
}

impl RecallStats {
    /// Fold one case's first-hit rank (1-based; `None` = miss) into the aggregate.
    fn accumulate(hits: &[Option<usize>]) -> Self {
        let n = hits.len();
        if n == 0 {
            return Self::default();
        }
        let at = |k: usize| {
            hits.iter().filter(|h| matches!(h, Some(r) if *r <= k)).count() as f64 / n as f64
        };
        Self {
            cases: n,
            r_at_1: at(1),
            r_at_5: at(5),
            r_at_20: at(20),
            mrr: hits
                .iter()
                .map(|h| h.map_or(0.0, |r| 1.0 / r as f64))
                .sum::<f64>()
                / n as f64,
        }
    }
}

/// Rank (1-based) of the first `expected` id in `ranked`, or `None` for a miss.
///
/// **Any-hit semantics**: a `multi_hop` case lists several legitimate answers, so the
/// case counts as answered when *any* of them surfaces — scoring it as "all must
/// appear" would punish a correct answer for not being exhaustive.
fn first_hit_rank(ranked: &[String], expected: &[String]) -> Option<usize> {
    ranked
        .iter()
        .position(|got| expected.iter().any(|e| e == got))
        .map(|i| i + 1)
}

/// Resolve the fixture path: `$TOURING_RETRIEVAL_FIXTURE`, else `bench/retrieval.json`
/// under the current directory.
fn retrieval_fixture_path() -> std::path::PathBuf {
    std::env::var_os("TOURING_RETRIEVAL_FIXTURE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("bench/retrieval.json"))
}

/// Retrieval-quality benchmark — R@1 / R@5 / R@20 / MRR against curated ground truth.
///
/// # What replaced what, and why (2026-08-07)
///
/// The prior implementation asserted three hard-coded cases against
/// `cli-index-find` — an **exact keyed lookup** — while its own comment claimed to
/// "test RRF hybrid search". Two of the three were tautological (asking an index
/// whether it contains a symbol it indexed), the ranked search path was never
/// exercised at all, and the gate passed at `accuracy >= 0.50`, i.e. **while getting
/// one of three wrong**. It reported a number that could not fall.
///
/// This version queries `cli-tantivy-search` — the real ranked path — over a curated
/// fixture whose every `expected` was verified to be a *unique* definition in
/// production code, and scores it with the standard IR metrics so a regression in
/// ranking is visible as a number rather than as an unchanged "pass".
///
/// # Verdict by baseline, not by an arbitrary bar
///
/// There is no universal "good" R@5. The honest gate is **regression**: compare
/// against `bench/retrieval-baseline.json` with an explicit tolerance
/// (`TOURING_RETRIEVAL_EPSILON`, default 0.02) and re-record only through an
/// explicit `--update-baseline`. With no baseline on disk the status is
/// `baseline_missing` — never a silent pass, and never a failure that blocks a
/// first run on a measurement nothing has yet been compared to.
fn run_search_benchmark(update_baseline: bool) -> BenchmarkResult {
    let start = std::time::Instant::now();
    let fixture_path = retrieval_fixture_path();

    let fixture: RetrievalFixture = match std::fs::read_to_string(&fixture_path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(f) => f,
        Err(e) => {
            return BenchmarkResult {
                name: "search_quality".to_string(),
                // Fail-CLOSED: an unreadable fixture means the retrieval quality is
                // UNKNOWN. Reporting "pass" here would be the exact lie this rewrite
                // exists to remove.
                status: "fail".to_string(),
                metrics: serde_json::json!({
                    "error": format!("fixture unreadable at {}: {e}", fixture_path.display()),
                    "hint": "set TOURING_RETRIEVAL_FIXTURE or create bench/retrieval.json",
                }),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let mut per_tier: std::collections::BTreeMap<String, Vec<Option<usize>>> = Default::default();
    let mut all_hits: Vec<Option<usize>> = Vec::new();
    let mut misses: Vec<&str> = Vec::new();

    for case in &fixture.cases {
        let ranked: Vec<String> = super::daemon_query(
            "cli-tantivy-search",
            serde_json::json!({"query": case.query, "limit": 20}),
        )
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|hit| {
            let f = hit.get("file_path")?.as_str()?;
            let n = hit.get("symbol_name")?.as_str()?;
            Some(format!("{f}::{n}"))
        })
        .collect();

        let rank = first_hit_rank(&ranked, &case.expected);
        if rank.is_none() {
            misses.push(&case.id);
        }
        all_hits.push(rank);
        per_tier.entry(case.tier.clone()).or_default().push(rank);
    }

    let overall = RecallStats::accumulate(&all_hits);
    let tiers: std::collections::BTreeMap<String, RecallStats> = per_tier
        .iter()
        .map(|(t, h)| (t.clone(), RecallStats::accumulate(h)))
        .collect();

    let (status, baseline_note) = compare_to_baseline(&overall, update_baseline);

    BenchmarkResult {
        name: "search_quality".to_string(),
        status,
        metrics: serde_json::json!({
            "fixture": fixture_path.display().to_string(),
            "overall": overall,
            "by_tier": tiers,
            // The misses are the point of the exercise: a benchmark that only reports
            // the aggregate hides exactly the cases worth fixing.
            "missed_case_ids": misses,
            "baseline": baseline_note,
        }),
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Compare `now` against the recorded baseline; optionally re-record it.
///
/// Returns `(status, note)` where status is `pass` / `fail` / `baseline_missing`
/// / `baseline_updated`.
///
/// The tolerance exists because retrieval metrics move with the corpus: re-indexing
/// adds documents and shifts BM25 scores by fractions of a percent. Without an
/// epsilon the gate would flap on noise and get disabled — which is how a gate dies.
/// Re-recording is deliberate and explicit (`--update-baseline`), so "the number
/// moved because the model changed" stays distinguishable from "the number moved and
/// someone quietly rewrote the target".
fn compare_to_baseline(now: &RecallStats, update: bool) -> (String, serde_json::Value) {
    let path = std::path::Path::new("bench/retrieval-baseline.json");
    let epsilon: f64 = std::env::var("TOURING_RETRIEVAL_EPSILON")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.02);

    if update {
        let payload = serde_json::json!({
            "recorded_at": chrono_now(),
            "epsilon": epsilon,
            "metrics": now,
        });
        return match std::fs::write(
            path,
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        ) {
            Ok(()) => (
                "baseline_updated".to_string(),
                serde_json::json!({"written": path.display().to_string()}),
            ),
            Err(e) => (
                "fail".to_string(),
                serde_json::json!({"error": format!("baseline write failed: {e}")}),
            ),
        };
    }

    let Some(prev) = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    else {
        return (
            "baseline_missing".to_string(),
            serde_json::json!({
                "hint": "run `touring eval --benchmark search --update-baseline` to record",
            }),
        );
    };

    let get = |k: &str| prev.pointer(&format!("/metrics/{k}")).and_then(|v| v.as_f64());
    let mut regressions = Vec::new();
    for (key, current) in [
        ("r_at_1", now.r_at_1),
        ("r_at_5", now.r_at_5),
        ("r_at_20", now.r_at_20),
        ("mrr", now.mrr),
    ] {
        if let Some(before) = get(key)
            && current < before - epsilon
        {
            regressions.push(serde_json::json!({
                "metric": key, "baseline": before, "now": current,
                "delta": current - before,
            }));
        }
    }

    let note = serde_json::json!({
        "path": path.display().to_string(),
        "epsilon": epsilon,
        "recorded_at": prev.get("recorded_at"),
        "regressions": regressions,
    });
    if regressions.is_empty() {
        ("pass".to_string(), note)
    } else {
        ("fail".to_string(), note)
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

#[cfg(test)]
mod retrieval_metric_tests {
    use super::{RecallStats, first_hit_rank};

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// Any-hit: a multi_hop case is answered when ANY expected id surfaces.
    ///
    /// Requiring all of them would score a correct answer as a miss for not being
    /// exhaustive — the metric would then punish precisely the behaviour it wants.
    #[test]
    fn any_expected_id_counts_as_a_hit_at_its_rank() {
        let ranked = ids(&["a.rs::A", "b.rs::B", "c.rs::C"]);
        assert_eq!(first_hit_rank(&ranked, &ids(&["b.rs::B"])), Some(2));
        // second expected present, first absent → rank of whichever appears
        assert_eq!(first_hit_rank(&ranked, &ids(&["z.rs::Z", "c.rs::C"])), Some(3));
        // earliest hit wins when several expected are present
        assert_eq!(first_hit_rank(&ranked, &ids(&["c.rs::C", "a.rs::A"])), Some(1));
    }

    #[test]
    fn a_miss_is_none_not_zero() {
        // Rank 0 would silently become 1/0 in MRR; `None` forces the caller to
        // decide, and `accumulate` maps it to a 0.0 contribution explicitly.
        assert_eq!(first_hit_rank(&ids(&["a.rs::A"]), &ids(&["q.rs::Q"])), None);
        assert_eq!(first_hit_rank(&[], &ids(&["a.rs::A"])), None);
    }

    #[test]
    fn recall_at_k_is_cumulative_and_mrr_is_the_reciprocal_mean() {
        // ranks 1, 3, miss  → R@1 = 1/3, R@5 = 2/3, MRR = (1 + 1/3 + 0)/3
        let s = RecallStats::accumulate(&[Some(1), Some(3), None]);
        assert_eq!(s.cases, 3);
        assert!((s.r_at_1 - 1.0 / 3.0).abs() < 1e-9, "r_at_1={}", s.r_at_1);
        assert!((s.r_at_5 - 2.0 / 3.0).abs() < 1e-9, "r_at_5={}", s.r_at_5);
        assert!((s.mrr - (1.0 + 1.0 / 3.0) / 3.0).abs() < 1e-9, "mrr={}", s.mrr);
    }

    #[test]
    fn a_hit_past_k_does_not_count_at_k() {
        // rank 6 counts at 20 but NOT at 5 — the boundary the old benchmark had no
        // notion of, since it only asked "found / not found".
        let s = RecallStats::accumulate(&[Some(6)]);
        assert_eq!(s.r_at_1, 0.0);
        assert_eq!(s.r_at_5, 0.0);
        assert_eq!(s.r_at_20, 1.0);
        assert!((s.mrr - 1.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn an_empty_case_set_is_zero_not_a_division_by_zero() {
        let s = RecallStats::accumulate(&[]);
        assert_eq!(s.cases, 0);
        assert_eq!(s.mrr, 0.0);
        assert_eq!(s.r_at_1, 0.0);
    }

    /// The shipped fixture must stay loadable and well-formed — a broken fixture
    /// makes the benchmark fail closed, but a *silently degenerate* one (zero cases,
    /// duplicate ids, an expected id without `::`) would make it pass on nothing.
    #[test]
    fn the_shipped_fixture_is_well_formed() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../bench/retrieval.json");
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return; // fixture lives at the workspace root; skip if scoped out
        };
        let f: super::RetrievalFixture =
            serde_json::from_str(&raw).expect("fixture must parse");
        assert!(f.cases.len() >= 30, "too few cases: {}", f.cases.len());
        let mut seen = std::collections::HashSet::new();
        for c in &f.cases {
            assert!(seen.insert(&c.id), "duplicate case id: {}", c.id);
            assert!(!c.query.trim().is_empty(), "{} has an empty query", c.id);
            assert!(!c.expected.is_empty(), "{} has no expected id", c.id);
            for e in &c.expected {
                assert!(e.contains("::"), "{}: expected id lacks `::`: {e}", c.id);
            }
            assert!(
                ["exact", "concept", "multi_hop"].contains(&c.tier.as_str()),
                "{}: unknown tier {}", c.id, c.tier
            );
            // A concept query that contains the symbol name is an exact query in
            // disguise — it would inflate the concept tier with lexical hits.
            if c.tier == "concept" {
                for e in &c.expected {
                    let sym = e.rsplit("::").next().unwrap_or("");
                    assert!(
                        !c.query.contains(sym),
                        "{}: concept query leaks the symbol name `{sym}`", c.id
                    );
                }
            }
        }
    }
}
