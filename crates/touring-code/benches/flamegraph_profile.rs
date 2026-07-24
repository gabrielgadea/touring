//! CPU flamegraph-generating benches for touring-ast hot paths.
//!
//! Runs Criterion benchmarks over the parse / symbol extraction /
//! quality analysis pipelines with [`pprof::criterion::PProfProfiler`]
//! hooked in. The output is a `profile.pb.data` + SVG flamegraph in
//! `target/criterion/<bench_name>/profile/` on every run.
//!
//! # Usage
//!
//! ```bash
//! cargo bench -p touring-ast --bench flamegraph_profile
//! # Open the flamegraph:
//! open target/criterion/flamegraph_extract_rust/profile/flamegraph.svg
//! ```
//!
//! Only builds on Unix-like systems (pprof requires signal handlers).
//! Gated to release profile via the standard `[[bench]]` harness.

#![cfg(unix)]

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use pprof::criterion::{Output, PProfProfiler};
use touring_code::ast::{
    analyze_quality, extract_symbols, languages::Lang, rust_semantic::RustSemanticReport,
};

// ─── Fixtures ──────────────────────────────────────────────────────────

/// A synthetic medium-sized Rust file — 50 functions, each with an arithmetic body.
fn medium_rust_source() -> String {
    let mut s = String::new();
    for i in 0..50 {
        s.push_str(&format!(
            "pub fn func_{i}<T: Clone + Default>(x: T) -> T {{\n    let _ = {i};\n    x.clone()\n}}\n\n"
        ));
    }
    s
}

/// A synthetic medium Python file.
fn medium_python_source() -> String {
    let mut s = String::new();
    for i in 0..50 {
        s.push_str(&format!(
            "def func_{i}(x):\n    y = x + {i}\n    return y * 2\n\n"
        ));
    }
    s
}

// ─── Benchmarks ────────────────────────────────────────────────────────

fn bench_extract_rust(c: &mut Criterion) {
    let src = medium_rust_source();
    c.bench_function("flamegraph_extract_rust", |b| {
        b.iter(|| {
            let syms = extract_symbols(black_box(&src), Lang::Rust).expect("extract");
            black_box(syms);
        });
    });
}

fn bench_extract_python(c: &mut Criterion) {
    let src = medium_python_source();
    c.bench_function("flamegraph_extract_python", |b| {
        b.iter(|| {
            let syms = extract_symbols(black_box(&src), Lang::Python).expect("extract");
            black_box(syms);
        });
    });
}

fn bench_quality_rust(c: &mut Criterion) {
    let src = medium_rust_source();
    c.bench_function("flamegraph_quality_rust", |b| {
        b.iter(|| {
            let report = analyze_quality(black_box(&src), Lang::Rust);
            black_box(report);
        });
    });
}

fn bench_rust_semantic(c: &mut Criterion) {
    let src = medium_rust_source();
    c.bench_function("flamegraph_rust_semantic", |b| {
        b.iter(|| {
            let report = RustSemanticReport::from_source(black_box(&src)).expect("valid rust");
            black_box(report);
        });
    });
}

// ─── Criterion wiring with pprof profiler ─────────────────────────────

fn profile_config() -> Criterion {
    Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)))
}

criterion_group! {
    name = flamegraph_benches;
    config = profile_config();
    targets = bench_extract_rust, bench_extract_python, bench_quality_rust, bench_rust_semantic
}

criterion_main!(flamegraph_benches);
