//! Wave B (2026-04-20) — instruction-count benchmarks for the quality
//! metrics hot path.
//!
//! **Why this exists**: the existing `criterion` / `divan` suites measure
//! wall-clock time, which is noisy in CI runners with variable hardware.
//! `iai-callgrind` runs each benchmark *once* under Valgrind's Callgrind
//! and returns deterministic instruction / cache / branch counts. Pairs
//! with the hdrhistogram P99 latency guards already present in
//! `crates/touring-ast/tests/latency_p99_guard.rs` — that one catches
//! wall-clock regressions; this one catches *algorithmic* regressions
//! that could hide behind hardware variance.
//!
//! **Run requirements**: Valgrind binary on PATH. The bench compiles
//! without Valgrind, so developer machines can `cargo bench --no-run`
//! cleanly; the actual `iai_callgrind_runner` invocation runs only in
//! CI (Linux job with `apt-get install -y valgrind`).
//!
//! **Measured functions** (all in `touring_analysis::quality::complexity`):
//! - `estimate_complexity` — full-pipeline entrypoint
//! - `estimate_halstead` — Wave A1 operator/operand scanner
//! - `estimate_maintainability_index` — Wave A1.1 arithmetic closure
//!
//! **Fixture scale**: one `small` (single-fn snippet) and one `large`
//! (~3 KB realistic Rust source) per function under test, so regressions
//! in both constant-factor and scaling behaviour are visible.

use std::hint::black_box;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use touring_analysis::quality::complexity::estimate_complexity;
use touring_analysis::quality::{estimate_halstead, estimate_maintainability_index};

// ── Fixtures ──────────────────────────────────────────────────────────

/// Small fixture: single branching fn. Shakes out constant-factor work
/// (inventory iteration, HashSet allocation, degenerate guards).
const SMALL_SOURCE: &str = "fn add(a: i32, b: i32) -> i32 { if a > 0 { a + b } else { b } }";

/// Large fixture: ~3 KB of realistic Rust source with mixed constructs
/// (structs, impls, match, nested loops, error handling). Exercises the
/// scaling behaviour of `memchr::memmem` keyword scans and the
/// operand-identifier walker.
const LARGE_SOURCE: &str = include_str!("fixtures/quality_large.rs.txt");

// ── Benchmarks ────────────────────────────────────────────────────────

#[library_benchmark]
#[bench::small(SMALL_SOURCE)]
#[bench::large(LARGE_SOURCE)]
fn bench_estimate_complexity(source: &str) -> touring_analysis::quality::ComplexityMetrics {
    black_box(estimate_complexity(black_box(source), black_box("rust")))
}

#[library_benchmark]
#[bench::small(SMALL_SOURCE)]
#[bench::large(LARGE_SOURCE)]
fn bench_estimate_halstead(source: &str) -> touring_analysis::quality::HalsteadMetrics {
    black_box(estimate_halstead(black_box(source), black_box("rust")))
}

// MI is pure arithmetic over precomputed inputs — this benchmark guards
// against accidental allocation or redundant `ln` calls sneaking into
// the formula. (Line comment — iai-callgrind's `#[library_benchmark]`
// rejects doc comments on the target fn.)
#[library_benchmark]
fn bench_maintainability_index() -> f64 {
    black_box(estimate_maintainability_index(
        black_box(1500.0),
        black_box(12),
        black_box(250),
    ))
}

library_benchmark_group!(
    name = quality_metrics;
    benchmarks =
        bench_estimate_complexity,
        bench_estimate_halstead,
        bench_maintainability_index
);

main!(library_benchmark_groups = quality_metrics);
