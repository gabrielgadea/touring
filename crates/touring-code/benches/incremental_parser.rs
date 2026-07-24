//! Criterion benchmarks for IncrementalParser — full vs incremental parse.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_code::ast::languages::Lang;
use touring_code::ast::parser::IncrementalParser;

/// Generate a synthetic Rust file with `n` functions.
fn generate_rust_source(n: usize) -> String {
    let mut src = String::with_capacity(n * 60);
    for i in 0..n {
        src.push_str(&format!(
            "fn func_{}(x: i32) -> i32 {{\n    x + {}\n}}\n\n",
            i, i
        ));
    }
    src
}

/// Generate a synthetic Python file with `n` functions.
fn generate_python_source(n: usize) -> String {
    let mut src = String::with_capacity(n * 40);
    for i in 0..n {
        src.push_str(&format!("def func_{}(x):\n    return x + {}\n\n", i, i));
    }
    src
}

fn bench_full_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_full_parse");

    for &line_count in &[50usize, 200, 500] {
        let source = generate_rust_source(line_count / 4); // ~4 lines per fn

        group.bench_with_input(
            BenchmarkId::new("rust_lines", line_count),
            &source,
            |b, src| {
                let mut parser = IncrementalParser::new();
                b.iter(|| parser.parse_full(black_box(src), Lang::Rust).unwrap());
            },
        );
    }

    for &line_count in &[50usize, 200, 500] {
        let source = generate_python_source(line_count / 3);

        group.bench_with_input(
            BenchmarkId::new("python_lines", line_count),
            &source,
            |b, src| {
                let mut parser = IncrementalParser::new();
                b.iter(|| parser.parse_full(black_box(src), Lang::Python).unwrap());
            },
        );
    }

    group.finish();
}

fn bench_incremental_vs_full(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_incremental_vs_full");

    let source_v1 = generate_rust_source(125); // ~500 lines
    // Small edit: change one function name near the middle
    let mut source_v2 = source_v1.clone();
    // Find "func_60" and replace with "func_60_modified"
    source_v2 = source_v2.replacen("func_60", "func_60_modified", 1);

    // Find the byte offset of the edit
    let edit_start = source_v1.find("func_60").unwrap_or(0);
    let old_len = "func_60".len();
    let new_len = "func_60_modified".len();

    let edit = tree_sitter::InputEdit {
        start_byte: edit_start,
        old_end_byte: edit_start + old_len,
        new_end_byte: edit_start + new_len,
        start_position: tree_sitter::Point::new(0, 0), // approximate
        old_end_position: tree_sitter::Point::new(0, old_len),
        new_end_position: tree_sitter::Point::new(0, new_len),
    };

    // Benchmark full re-parse of modified source
    group.bench_function("full_reparse_500_lines", |b| {
        let mut parser = IncrementalParser::new();
        b.iter(|| {
            parser
                .parse_full(black_box(&source_v2), Lang::Rust)
                .unwrap()
        });
    });

    // Benchmark incremental parse with cached tree
    group.bench_function("incremental_500_lines", |b| {
        b.iter_batched(
            || {
                let mut parser = IncrementalParser::new();
                parser
                    .parse_and_cache("bench.rs", &source_v1, Lang::Rust)
                    .unwrap();
                parser
            },
            |mut parser| {
                parser
                    .parse_incremental(
                        black_box("bench.rs"),
                        black_box(&source_v2),
                        black_box(&edit),
                    )
                    .unwrap()
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_cache_lookup(c: &mut Criterion) {
    let source = generate_rust_source(50);

    c.bench_function("parser_cache_lookup", |b| {
        let mut parser = IncrementalParser::new();
        parser
            .parse_and_cache("cached.rs", &source, Lang::Rust)
            .unwrap();

        b.iter(|| parser.cached_tree(black_box("cached.rs")).is_some());
    });
}

fn bench_parse_and_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser_parse_and_cache");

    for &fn_count in &[10usize, 50, 125] {
        let source = generate_rust_source(fn_count);

        group.bench_with_input(BenchmarkId::new("rust_fns", fn_count), &source, |b, src| {
            let mut parser = IncrementalParser::new();
            let path = format!("bench_{fn_count}.rs");
            b.iter(|| {
                parser
                    .parse_and_cache(black_box(&path), black_box(src), Lang::Rust)
                    .unwrap()
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_full_parse,
    bench_incremental_vs_full,
    bench_cache_lookup,
    bench_parse_and_cache,
);
criterion_main!(benches);
