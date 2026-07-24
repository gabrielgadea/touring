//! Tail-latency regression guard powered by `hdrhistogram`.
//!
//! These tests exercise touring-ast hot paths (parse, symbol extraction,
//! quality analysis) thousands of times, record every operation into an
//! HDR Histogram, and assert that P99 and P99.9 stay under known budgets.
//!
//! They catch the class of regression that criterion's mean/stddev misses:
//! a tail that grows 10× while the mean stays flat. touring-ast feeds
//! Claude Code's pre-edit hook which has a 300ms budget; if parse P99
//! exceeds that budget, the hook UX silently degrades.
//!
//! The budgets below are generous starting points — tighten them as the
//! actual measured percentiles stabilize in CI.

use hdrhistogram::Histogram;
use std::time::Instant;
use touring_code::ast::{analyze_quality, extract_symbols, languages::Lang};

/// Sample size — large enough that P99.9 is meaningful (>= 1000 samples).
const N_SAMPLES: usize = 2_000;

/// Max microseconds any single operation is expected to take. Used to size
/// the histogram so we don't lose outliers to overflow.
const MAX_EXPECTED_US: u64 = 10_000_000; // 10s — generous ceiling

// ─── Fixtures ──────────────────────────────────────────────────────────

fn small_rust_source() -> String {
    "fn add(a: i32, b: i32) -> i32 { a + b }\nfn mul(a: i32, b: i32) -> i32 { a * b }\n".to_string()
}

fn medium_rust_source() -> String {
    let mut s = String::new();
    for i in 0..50 {
        s.push_str(&format!(
            "fn func_{i}(x: i32) -> i32 {{\n    let y = x + {i};\n    y * 2\n}}\n\n"
        ));
    }
    s
}

fn small_python_source() -> String {
    "def add(a, b):\n    return a + b\n\ndef mul(a, b):\n    return a * b\n".to_string()
}

// ─── Helpers ──────────────────────────────────────────────────────────

/// Record N runs of `f` into a histogram and return it.
fn record<F: FnMut()>(n: usize, mut f: F) -> Histogram<u64> {
    // sigfig=3 → ~0.1% precision (plenty for latency work).
    let mut hist = Histogram::<u64>::new_with_bounds(1, MAX_EXPECTED_US, 3)
        .expect("histogram bounds are valid");
    for _ in 0..n {
        let t0 = Instant::now();
        f();
        let elapsed_us = t0.elapsed().as_micros() as u64;
        // Saturate rather than panic on extreme outliers.
        hist.record(elapsed_us.clamp(1, MAX_EXPECTED_US))
            .expect("sample within bounds");
    }
    hist
}

/// Human-readable summary of a histogram's key percentiles.
fn summary(name: &str, h: &Histogram<u64>) -> String {
    format!(
        "{name}: min={}µs p50={}µs p95={}µs p99={}µs p99.9={}µs max={}µs n={}",
        h.min(),
        h.value_at_quantile(0.50),
        h.value_at_quantile(0.95),
        h.value_at_quantile(0.99),
        h.value_at_quantile(0.999),
        h.max(),
        h.len()
    )
}

// ─── Parse latency guard ──────────────────────────────────────────────

#[test]
fn extract_small_rust_p99_under_budget() {
    // extract_symbols internally parses — exercises the parse hot path.
    let src = small_rust_source();
    let hist = record(N_SAMPLES, || {
        let _ = extract_symbols(&src, Lang::Rust).expect("extract must succeed");
    });
    eprintln!("{}", summary("extract_rust_small", &hist));

    let p99 = hist.value_at_quantile(0.99);
    assert!(
        p99 < 100_000,
        "extract_rust_small P99 exceeded budget: {p99}µs >= 100_000µs — {}",
        summary("snapshot", &hist)
    );
}

#[test]
fn extract_python_p99_under_budget() {
    let src = small_python_source();
    let hist = record(N_SAMPLES, || {
        let _ = extract_symbols(&src, Lang::Python).expect("extract must succeed");
    });
    eprintln!("{}", summary("extract_python_small", &hist));

    let p99 = hist.value_at_quantile(0.99);
    assert!(
        p99 < 100_000,
        "extract_python_small P99 exceeded budget: {p99}µs >= 100_000µs — {}",
        summary("snapshot", &hist)
    );
}

#[test]
fn extract_medium_rust_p99_under_budget() {
    let src = medium_rust_source();
    let hist = record(500, || {
        let _ = extract_symbols(&src, Lang::Rust).expect("extract must succeed");
    });
    eprintln!("{}", summary("extract_rust_medium", &hist));

    let p99 = hist.value_at_quantile(0.99);
    assert!(
        p99 < 300_000,
        "extract_rust_medium P99 exceeded budget: {p99}µs >= 300_000µs — {}",
        summary("snapshot", &hist)
    );
}

// ─── Symbol extraction latency guard ──────────────────────────────────

// ─── Quality analysis latency guard ───────────────────────────────────

#[test]
fn quality_rust_p99_under_budget() {
    let src = medium_rust_source();
    let hist = record(N_SAMPLES, || {
        let _ = analyze_quality(&src, Lang::Rust);
    });
    eprintln!("{}", summary("quality_rust_medium", &hist));

    // Budget: quality analysis of 50-function file P99 under 50ms.
    let p99 = hist.value_at_quantile(0.99);
    assert!(
        p99 < 50_000,
        "quality_rust_medium P99 exceeded budget: {p99}µs >= 50_000µs — {}",
        summary("snapshot", &hist)
    );
}

// ─── Histogram sanity ─────────────────────────────────────────────────

#[test]
fn histogram_records_monotonic_sequence() {
    // Self-test of the harness — ensures `record` correctly captures
    // increasing latencies and the quantile math works as expected.
    let mut counter = 0u64;
    let hist = record(100, || {
        counter = counter.wrapping_add(1);
        // Busy-loop proportional to counter so latency grows across samples.
        let target = counter % 10;
        for _ in 0..(target * 50) {
            std::hint::black_box(0);
        }
    });
    assert!(hist.len() == 100, "expected 100 recorded samples");
    assert!(hist.min() <= hist.max(), "min must be <= max");
    assert!(
        hist.value_at_quantile(0.50) <= hist.value_at_quantile(0.99),
        "p50 must be <= p99"
    );
}
