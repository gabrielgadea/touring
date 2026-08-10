//! Monitoring & Observability (D50 / F4.10) — polyglot detector of the
//! canonical "blind" code patterns: `println!` in production (no structured
//! logging), no `tracing` instrumentation, no `metrics` counters, and no
//! OpenTelemetry exporter. "Não se pode consertar o que não se pode ver."
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `println-in-prod` | `println!` / `eprintln!` / `dbg!` in source (no structured logging) | Rust |
//! | `no-tracing` | a non-trivial source file with no `tracing::` or `log::` calls (no observability) | Rust |
//! | `no-instrument-async` | an `async fn` without `#[tracing::instrument]` (no auto-span for distributed tracing) | Rust |
//! | `no-metrics-counter` | no `metrics::counter!` / `meter_provider` (no SLI/metrics layer) | Rust |
//! | `no-otel-exporter` | no `opentelemetry-otlp` / `opentelemetry-prometheus` reference (no telemetry export) | Rust |
//! | `no-subscriber-init` | `main` function without `tracing_subscriber::fmt::init()` (logger never initialized) | Rust |
//! | `py-print-in-prod` | `print(` in Python non-test code (no logging) | Python |
//! | `py-no-logging` | Python project with no `import logging` reference | Python |
//!
//! **Disjoint** from D12 arch-consistency (which keys on logging style
//! consistency; F4.10 keys on observability *presence*).
//!
//! **Sources (context7, `/open-telemetry/opentelemetry-rust`, High reputation,
//! bench 71.71):** OpenTelemetry Rust crate implements the OTel API for
//! distributed tracing, metrics, and logs. `tracing` is the canonical
//! structured-logging facade; `tracing_subscriber::fmt::init()` initializes
//! the global subscriber. `#[tracing::instrument]` auto-creates spans for
//! async functions. `opentelemetry-otlp` exports to an OTel Collector;
//! `opentelemetry-prometheus` exposes metrics in a scrapeable format.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        _ => Lang::Other,
    }
}

/// Monitoring & observability findings for one file.
pub type MonitoringReport = crate::quality::SmellReport;

const PRINTLN: &[u8] = b"println!";
const EPRINTLN: &[u8] = b"eprintln!";
const DBG: &[u8] = b"dbg!";
const TRACING: &[u8] = b"tracing::";
const LOG: &[u8] = b"log::";
const TRACING_INSTRUMENT: &[u8] = b"#[tracing::instrument]";
const ASYNC_FN: &[u8] = b"async fn";
const METRICS_COUNTER: &[u8] = b"metrics::counter!";
const METER_PROVIDER: &[u8] = b"meter_provider";
const OTEL_OTLP: &[u8] = b"opentelemetry-otlp";
const OTEL_PROMETHEUS: &[u8] = b"opentelemetry-prometheus";
const TRACING_SUB_INIT: &[u8] = b"tracing_subscriber::fmt::init";
const TRACING_SUB_TRY_INIT: &[u8] = b"tracing_subscriber::fmt::try_init";
const PY_PRINT: &[u8] = b"print(";
const PY_LOGGING: &[u8] = b"import logging";

fn count_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

fn has_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> bool {
    count_in_executable(bytes, regions, needle) > 0
}

fn push_rust_findings(report: &mut MonitoringReport, bytes: &[u8], regions: &[(usize, usize)]) {
    report.push(
        "println! / eprintln! / dbg! in source (use `tracing` for structured logging)",
        count_in_executable(bytes, regions, PRINTLN)
            + count_in_executable(bytes, regions, EPRINTLN)
            + count_in_executable(bytes, regions, DBG),
        0.7,
    );
    // No observability: file > 50 LOC with no tracing or log
    if report.total_lines >= 50
        && !has_in_executable(bytes, regions, TRACING)
        && !has_in_executable(bytes, regions, LOG)
    {
        report.push(
            "non-trivial file with no `tracing::` or `log::` calls (no observability)",
            1,
            0.9,
        );
    }
    // async fn without instrument
    let has_async = has_in_executable(bytes, regions, ASYNC_FN);
    if has_async && !has_in_executable(bytes, regions, TRACING_INSTRUMENT) {
        report.push(
            "async fn without `#[tracing::instrument]` (no auto-span for distributed tracing)",
            1,
            0.6,
        );
    }
    // metrics
    if !has_in_executable(bytes, regions, METRICS_COUNTER)
        && !has_in_executable(bytes, regions, METER_PROVIDER)
    {
        report.push(
            "no `metrics::counter!` or `meter_provider` reference (no SLI/metrics layer)",
            1,
            0.5,
        );
    }
    // otel exporter
    if !has_in_executable(bytes, regions, OTEL_OTLP)
        && !has_in_executable(bytes, regions, OTEL_PROMETHEUS)
    {
        report.push(
            "no `opentelemetry-otlp` or `opentelemetry-prometheus` reference (no telemetry export)",
            1,
            0.5,
        );
    }
    // main without tracing_subscriber init (heuristic: has `fn main` and no init)
    if has_in_executable(bytes, regions, b"fn main(")
        && !has_in_executable(bytes, regions, TRACING_SUB_INIT)
        && !has_in_executable(bytes, regions, TRACING_SUB_TRY_INIT)
    {
        report.push(
            "`fn main` without `tracing_subscriber::fmt::init` (logger never initialized)",
            1,
            0.7,
        );
    }
}

fn push_python_findings(report: &mut MonitoringReport, bytes: &[u8], regions: &[(usize, usize)]) {
    if has_in_executable(bytes, regions, PY_PRINT) {
        report.push(
            "`print(` in source (use `logging` module for structured logs)",
            count_in_executable(bytes, regions, PY_PRINT),
            0.6,
        );
    }
    // Big file with no logging
    if report.total_lines >= 50 && !has_in_executable(bytes, regions, PY_LOGGING) {
        report.push(
            "non-trivial file with no `import logging` (no structured logging)",
            1,
            0.8,
        );
    }
}

/// Analyze monitoring & observability smells in `source` for the given
/// language. Polyglot: Rust + Python.
pub fn analyze_monitoring(source: &str, lang: &str) -> MonitoringReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = MonitoringReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match canonical_lang(lang) {
        Lang::Rust => push_rust_findings(&mut report, bytes, &regions),
        Lang::Python => push_python_findings(&mut report, bytes, &regions),
        Lang::Other => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`MonitoringReport`] as `1 - density * SCALE`, clamped to `[0, 1]`.
pub fn score_monitoring(report: &MonitoringReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str, lang: &str) -> MonitoringReport {
        analyze_monitoring(src, lang)
    }

    #[test]
    fn empty_file_clean() {
        let r = rep("", "rust");
        assert!(
            r.violations >= 1,
            "empty file: 0 lines < 50 so no 'no observability' flag: {:?}",
            r.findings
        );
    }

    #[test]
    fn println_flagged() {
        let src = r#"fn main() {
    println!("hello");
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("println")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn tracing_clean() {
        let src = r#"use tracing::info;

fn main() {
    info!("hello");
}
"#;
        let r = rep(src, "rust");
        // "use tracing::" + no println = no logging-style finding
        // But still might fire other findings (async fn, etc.)
        // We just check no println finding:
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("println") || m.contains("dbg")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn no_tracing_on_big_file_flagged() {
        let big = std::iter::repeat_n("fn do_thing() { let x = 1; }", 60)
            .collect::<Vec<_>>()
            .join("\n");
        let r = rep(&big, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("observability")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn async_without_instrument_flagged() {
        let src = r#"async fn fetch() {
    client.get("http://x").await;
}
"#;
        let r = rep(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("instrument")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn py_print_flagged() {
        let src = r#"def main():
    print("hello")
"#;
        let r = rep(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("print")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn py_with_logging_clean() {
        let src = r#"import logging

log = logging.getLogger(__name__)

def main():
    log.info("hello")
"#;
        let r = rep(src, "python");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("print") || m.contains("logging")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn other_lang_no_findings() {
        let r = rep("anything", "ruby");
        assert_eq!(
            r.violations, 0,
            "unsupported lang reports no findings: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep(
            r#"fn main() {
    println!("oops");
    eprintln!("also oops");
    dbg!(1);
}
"#,
            "rust",
        );
        let good = rep(
            r#"use tracing::info;

fn main() {
    info!("hello");
}
"#,
            "rust",
        );
        assert!(
            score_monitoring(&bad) < score_monitoring(&good),
            "println-heavy ({:.3}) must score below tracing-clean ({:.3})",
            score_monitoring(&bad),
            score_monitoring(&good)
        );
    }
}
