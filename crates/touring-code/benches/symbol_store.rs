//! Criterion benchmarks for SymbolStore SQLite queries.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use tempfile::TempDir;
use touring_code::ast::graph::{DependencyEdge, SymbolLocation};
use touring_code::ast::store::SymbolStore;

fn make_symbol(name: &str, file_path: &str, line: usize) -> SymbolLocation {
    SymbolLocation {
        symbol_name: name.to_string(),
        file_path: file_path.to_string(),
        line,
        column: 0,
        is_definition: true,
        kind: None,
    }
}

fn make_edge(from: &str, to: &str, symbols: &[&str]) -> DependencyEdge {
    DependencyEdge {
        from: from.to_string(),
        to: to.to_string(),
        symbols: symbols.iter().map(|s| s.to_string()).collect(),
        co_edit_weight: 0.0,
    }
}

/// Create a store pre-populated with `n` symbols across multiple files.
fn populated_store(n: usize) -> (TempDir, SymbolStore) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("bench_symbols.db");
    let store = SymbolStore::new(&db_path).unwrap();

    for i in 0..n {
        store
            .upsert_symbol(&make_symbol(
                &format!("symbol_{i}"),
                &format!("file_{}.py", i / 10),
                (i % 100) + 1,
            ))
            .unwrap();
    }

    for i in 0..n / 10 {
        store
            .upsert_dependency(&make_edge(
                &format!("file_{i}.py"),
                &format!("file_{}.py", (i + 1) % (n / 10).max(1)),
                &["helper"],
            ))
            .unwrap();
    }

    (tmp, store)
}

fn bench_upsert_symbol(c: &mut Criterion) {
    let mut group = c.benchmark_group("symbol_store_upsert");

    for &count in &[10usize, 100] {
        group.bench_with_input(BenchmarkId::new("symbols", count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let tmp = TempDir::new().unwrap();
                    let db_path = tmp.path().join("bench.db");
                    let store = SymbolStore::new(&db_path).unwrap();
                    (tmp, store)
                },
                |(_tmp, store)| {
                    for i in 0..n {
                        store
                            .upsert_symbol(&make_symbol(&format!("sym_{i}"), "test.py", i + 1))
                            .unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_find_symbol(c: &mut Criterion) {
    let (_tmp, store) = populated_store(1000);

    c.bench_function("symbol_store_find_symbol", |b| {
        b.iter(|| store.find_symbol(black_box("symbol_500")).unwrap());
    });
}

fn bench_search_symbols(c: &mut Criterion) {
    let (_tmp, store) = populated_store(1000);

    let mut group = c.benchmark_group("symbol_store_search");

    group.bench_function("prefix_symbol_5", |b| {
        b.iter(|| store.search_symbols(black_box("symbol_5")).unwrap());
    });

    group.bench_function("prefix_symbol_99", |b| {
        b.iter(|| store.search_symbols(black_box("symbol_99")).unwrap());
    });

    group.finish();
}

fn bench_find_symbols_in_file(c: &mut Criterion) {
    let (_tmp, store) = populated_store(1000);

    c.bench_function("symbol_store_find_in_file", |b| {
        b.iter(|| store.find_symbols_in_file(black_box("file_5.py")).unwrap());
    });
}

fn bench_upsert_dependency(c: &mut Criterion) {
    let mut group = c.benchmark_group("symbol_store_upsert_dep");

    for &count in &[10usize, 50] {
        group.bench_with_input(BenchmarkId::new("edges", count), &count, |b, &n| {
            b.iter_batched(
                || {
                    let tmp = TempDir::new().unwrap();
                    let db_path = tmp.path().join("bench.db");
                    let store = SymbolStore::new(&db_path).unwrap();
                    (tmp, store)
                },
                |(_tmp, store)| {
                    for i in 0..n {
                        store
                            .upsert_dependency(&make_edge(
                                &format!("f_{i}.py"),
                                &format!("f_{}.py", (i + 1) % n.max(1)),
                                &["dep"],
                            ))
                            .unwrap();
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_stats(c: &mut Criterion) {
    let (_tmp, store) = populated_store(1000);

    c.bench_function("symbol_store_stats", |b| {
        b.iter(|| store.stats().unwrap());
    });
}

criterion_group!(
    benches,
    bench_upsert_symbol,
    bench_find_symbol,
    bench_search_symbols,
    bench_find_symbols_in_file,
    bench_upsert_dependency,
    bench_stats,
);
criterion_main!(benches);
