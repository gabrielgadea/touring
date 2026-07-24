//! Criterion benchmarks for context compilation and cache key generation.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_server::context_compiler::{
    CompactSummary, ContextCache, compile_context, generate_structured_summary,
};

fn bench_compile_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_compile");

    // Small context: 1 file, no extras
    group.bench_function("minimal", |b| {
        b.iter(|| {
            compile_context(
                black_box("fix bug"),
                black_box(&["parser.rs".to_string()]),
                black_box(&[]),
                black_box(&[]),
                black_box(&[]),
                black_box(2000),
            )
        });
    });

    // Medium context: 5 files, some gotchas and relations
    let files: Vec<String> = (0..5).map(|i| format!("file_{i}.rs")).collect();
    let gotchas: Vec<(String, String)> = (0..3)
        .map(|i| {
            (
                "warning".to_string(),
                format!("gotcha {i}: watch out for X"),
            )
        })
        .collect();
    let relations: Vec<String> = (0..5).map(|i| format!("mod_{i}")).collect();
    let errors: Vec<String> = (0..2).map(|i| format!("error_{i}")).collect();

    group.bench_function("medium", |b| {
        b.iter(|| {
            compile_context(
                black_box("refactor module"),
                black_box(&files),
                black_box(&gotchas),
                black_box(&relations),
                black_box(&errors),
                black_box(2000),
            )
        });
    });

    // Large context: 20 files, max gotchas/relations/errors
    let large_files: Vec<String> = (0..20).map(|i| format!("src/module_{i}/mod.rs")).collect();
    let large_gotchas: Vec<(String, String)> = (0..10)
        .map(|i| {
            (
                "critical".to_string(),
                format!(
                    "gotcha {i}: detailed description of potential issue {}",
                    "x".repeat(50)
                ),
            )
        })
        .collect();
    let large_relations: Vec<String> = (0..15).map(|i| format!("dependency_{i}")).collect();
    let large_errors: Vec<String> = (0..10).map(|i| format!("error pattern {i}")).collect();

    group.bench_function("large", |b| {
        b.iter(|| {
            compile_context(
                black_box("implement feature across modules"),
                black_box(&large_files),
                black_box(&large_gotchas),
                black_box(&large_relations),
                black_box(&large_errors),
                black_box(5000),
            )
        });
    });

    group.finish();
}

fn bench_cache_key(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_cache_key");

    for &file_count in &[1usize, 5, 20] {
        let files: Vec<String> = (0..file_count).map(|i| format!("file_{i}.rs")).collect();

        group.bench_with_input(BenchmarkId::new("files", file_count), &files, |b, files| {
            b.iter(|| {
                compile_context(
                    black_box("test"),
                    black_box(files),
                    black_box(&[]),
                    black_box(&[]),
                    black_box(&[]),
                    black_box(100),
                )
                .cache_key
            });
        });
    }

    group.finish();
}

fn bench_compact_summary(c: &mut Criterion) {
    let mut group = c.benchmark_group("compact_summary");

    let summary = CompactSummary {
        objective: "Implement S0 cache foundation for touring-server".to_string(),
        files_modified: vec!["context_compiler.rs".to_string(), "cache.rs".to_string()],
        errors: vec!["type mismatch in line 42".to_string()],
        pending_tasks: vec!["TODO: add more tests".to_string()],
        decisions: vec!["Decision: use FxHasher for determinism".to_string()],
        tool_history: vec!["Read".to_string(), "Edit".to_string(), "Bash".to_string()],
        code_snippets: vec!["fn compute() -> u64 { 42 }".to_string()],
    };

    for &budget in &[100usize, 500, 5000] {
        group.bench_with_input(
            BenchmarkId::new("budget", budget),
            &budget,
            |b, &max_chars| {
                b.iter(|| summary.to_context_string(black_box(max_chars)));
            },
        );
    }

    group.finish();
}

fn bench_generate_structured_summary(c: &mut Criterion) {
    let raw_context = "Objective: Implement S4 context compression\n\
        Modified: src/context_compiler.rs\n\
        EDIT: src/enrichment.rs\n\
        TODO: Add tests for edge cases\n\
        - [ ] Run clippy\n\
        Decision: Use char-based budget instead of token-based\n\
        This line contains error in the output\n\
        Some unrelated line that should be ignored\n\
        FAIL: test_budget exceeded\n\
        Another line for padding to make it more realistic\n\
        Objective: secondary task details\n\
        Modified: another_file.rs\n"
        .repeat(5);

    c.bench_function("generate_structured_summary", |b| {
        b.iter(|| generate_structured_summary(black_box(&raw_context)));
    });
}

fn bench_context_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_cache");

    // Bench cache miss then hit
    group.bench_function("miss_then_hit", |b| {
        let cache = ContextCache::new();
        // Prime the cache
        cache.compile_or_cached("fix", &["a.rs".into()], &[], &[], &[], 2000);

        b.iter(|| {
            cache.compile_or_cached(
                black_box("fix"),
                black_box(&["a.rs".to_string()]),
                black_box(&[]),
                black_box(&[]),
                black_box(&[]),
                black_box(2000),
            )
        });
    });

    // Bench cache with many entries
    group.bench_function("lookup_in_256_entries", |b| {
        let cache = ContextCache::new();
        for i in 0..256 {
            cache.compile_or_cached(
                &format!("intent_{i}"),
                &[format!("file_{i}.rs")],
                &[],
                &[],
                &[],
                2000,
            );
        }

        b.iter(|| {
            cache.compile_or_cached(
                black_box("intent_128"),
                black_box(&["file_128.rs".to_string()]),
                black_box(&[]),
                black_box(&[]),
                black_box(&[]),
                black_box(2000),
            )
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_compile_context,
    bench_cache_key,
    bench_compact_summary,
    bench_generate_structured_summary,
    bench_context_cache,
);
criterion_main!(benches);
