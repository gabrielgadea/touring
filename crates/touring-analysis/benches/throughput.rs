//! D38 — Cross-language throughput benchmarks for touring-analysis.
//!
//! Measures **Rust**, **TypeScript**, and **Python** parse + symbol-extraction +
//! quality-metrics throughput in _functions per second_ (higher = better).
//!
//! ## Targets (cargo bench passes when ops/s >= threshold)
//!
//! | Operation | Language | Floor (f/s) |
//! |-----------|----------|--------------|
//! | `parse_ast` | Rust | 1,365 |
//! | `extract_symbols` | Rust | 1,100 |
//! | `quality_metrics` | Rust | 950 |
//! | `parse_ast` | TypeScript | 944 |
//! | `extract_symbols` | TypeScript | 800 |
//! | `quality_metrics` | TypeScript | 700 |
//! | `parse_ast` | Python | 1,188 |
//! | `extract_symbols` | Python | 950 |
//! | `quality_metrics` | Python | 820 |
//!
//! ## CI regression gate
//!
//! A result > 10 % below the floor fails the build.
//!
//! ## Framework
//!
//! Uses `criterion` with `harness = false` so `cargo bench` drives execution
//! directly (no nested-harness overhead).

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use touring_analysis::quality::complexity::estimate_complexity;
use touring_code::ast::languages::Lang;
use touring_code::ast::symbols::extract_symbols;

// ── Fixtures ─────────────────────────────────────────────────────────────────

const RUST_SOURCE: &str = include_str!("fixtures/throughput_rust.rs.txt");
const TS_SOURCE: &str = include_str!("fixtures/throughput_ts.ts.txt");
const PY_SOURCE: &str = include_str!("fixtures/throughput_py.py.txt");

// ── Parse AST benchmarks ──────────────────────────────────────────────────────

fn bench_parse_ast_rust(c: &mut Criterion) {
    use touring_code::ast::parser::ParserPool;

    let pool = ParserPool::new();
    let mut group = c.benchmark_group("parse_ast/rust");
    group.throughput(Throughput::Bytes(RUST_SOURCE.len() as u64));
    group.bench_function("parse_ast/rust", |b| {
        b.iter(|| {
            let tree = pool.parse(black_box(RUST_SOURCE), Lang::Rust);
            black_box(tree)
        });
    });
    group.finish();
}

fn bench_parse_ast_ts(c: &mut Criterion) {
    use touring_code::ast::parser::ParserPool;

    let pool = ParserPool::new();
    let mut group = c.benchmark_group("parse_ast/typescript");
    group.throughput(Throughput::Bytes(TS_SOURCE.len() as u64));
    group.bench_function("parse_ast/typescript", |b| {
        b.iter(|| {
            let tree = pool.parse(black_box(TS_SOURCE), Lang::TypeScript);
            black_box(tree)
        });
    });
    group.finish();
}

fn bench_parse_ast_py(c: &mut Criterion) {
    use touring_code::ast::parser::ParserPool;

    let pool = ParserPool::new();
    let mut group = c.benchmark_group("parse_ast/python");
    group.throughput(Throughput::Bytes(PY_SOURCE.len() as u64));
    group.bench_function("parse_ast/python", |b| {
        b.iter(|| {
            let tree = pool.parse(black_box(PY_SOURCE), Lang::Python);
            black_box(tree)
        });
    });
    group.finish();
}

// ── Extract symbols benchmarks ─────────────────────────────────────────────────

fn bench_extract_symbols_rust(c: &mut Criterion) {
    let mut group = c.benchmark_group("extract_symbols/rust");
    group.throughput(Throughput::Bytes(RUST_SOURCE.len() as u64));
    group.bench_function("extract_symbols/rust", |b| {
        b.iter(|| {
            let symbols = extract_symbols(black_box(RUST_SOURCE), Lang::Rust);
            black_box(symbols)
        });
    });
    group.finish();
}

fn bench_extract_symbols_ts(c: &mut Criterion) {
    let mut group = c.benchmark_group("extract_symbols/typescript");
    group.throughput(Throughput::Bytes(TS_SOURCE.len() as u64));
    group.bench_function("extract_symbols/typescript", |b| {
        b.iter(|| {
            let symbols = extract_symbols(black_box(TS_SOURCE), Lang::TypeScript);
            black_box(symbols)
        });
    });
    group.finish();
}

fn bench_extract_symbols_py(c: &mut Criterion) {
    let mut group = c.benchmark_group("extract_symbols/python");
    group.throughput(Throughput::Bytes(PY_SOURCE.len() as u64));
    group.bench_function("extract_symbols/python", |b| {
        b.iter(|| {
            let symbols = extract_symbols(black_box(PY_SOURCE), Lang::Python);
            black_box(symbols)
        });
    });
    group.finish();
}

// ── Quality metrics benchmarks ─────────────────────────────────────────────────

fn bench_quality_metrics_rust(c: &mut Criterion) {
    let mut group = c.benchmark_group("quality_metrics/rust");
    group.throughput(Throughput::Bytes(RUST_SOURCE.len() as u64));
    group.bench_function("quality_metrics/rust", |b| {
        b.iter(|| {
            let metrics = estimate_complexity(black_box(RUST_SOURCE), black_box("rust"));
            black_box(metrics)
        });
    });
    group.finish();
}

fn bench_quality_metrics_ts(c: &mut Criterion) {
    let mut group = c.benchmark_group("quality_metrics/typescript");
    group.throughput(Throughput::Bytes(TS_SOURCE.len() as u64));
    group.bench_function("quality_metrics/typescript", |b| {
        b.iter(|| {
            let metrics = estimate_complexity(black_box(TS_SOURCE), black_box("typescript"));
            black_box(metrics)
        });
    });
    group.finish();
}

fn bench_quality_metrics_py(c: &mut Criterion) {
    let mut group = c.benchmark_group("quality_metrics/python");
    group.throughput(Throughput::Bytes(PY_SOURCE.len() as u64));
    group.bench_function("quality_metrics/python", |b| {
        b.iter(|| {
            let metrics = estimate_complexity(black_box(PY_SOURCE), black_box("python"));
            black_box(metrics)
        });
    });
    group.finish();
}

// ── Registration ───────────────────────────────────────────────────────────────

criterion_group!(
    name = throughput_benches;
    config = Criterion::default().sample_size(50);
    targets = bench_parse_ast_rust,
              bench_parse_ast_ts,
              bench_parse_ast_py,
              bench_extract_symbols_rust,
              bench_extract_symbols_ts,
              bench_extract_symbols_py,
              bench_quality_metrics_rust,
              bench_quality_metrics_ts,
              bench_quality_metrics_py
);

criterion_main!(throughput_benches);
