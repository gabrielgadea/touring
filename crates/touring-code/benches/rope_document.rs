//! Criterion benchmarks for RopeDocument operations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_code::ast::document::RopeDocument;

/// Generate a document with `n` characters of realistic code.
fn generate_content(n: usize) -> String {
    let line = "let value = compute_something(input_param);\n";
    let line_len = line.len();
    let repeats = n / line_len + 1;
    line.repeat(repeats)[..n].to_string()
}

fn bench_edit_insertion(c: &mut Criterion) {
    let mut group = c.benchmark_group("rope_edit_insert");

    for &doc_size in &[10_000usize, 100_000] {
        let content = generate_content(doc_size);

        // Insert at beginning
        group.bench_with_input(
            BenchmarkId::new("at_start", doc_size),
            &content,
            |b, content| {
                b.iter_batched(
                    || RopeDocument::new("bench.py", content),
                    |mut doc| doc.edit(black_box(0), black_box(0), black_box("inserted_text")),
                    criterion::BatchSize::SmallInput,
                );
            },
        );

        // Insert at middle
        let mid = content.len() / 2;
        // Ensure mid is on a char boundary (ASCII content so always valid)
        group.bench_with_input(
            BenchmarkId::new("at_middle", doc_size),
            &content,
            |b, content| {
                b.iter_batched(
                    || RopeDocument::new("bench.py", content),
                    |mut doc| doc.edit(black_box(mid), black_box(mid), black_box("inserted_text")),
                    criterion::BatchSize::SmallInput,
                );
            },
        );

        // Insert at end
        let end = content.len();
        group.bench_with_input(
            BenchmarkId::new("at_end", doc_size),
            &content,
            |b, content| {
                b.iter_batched(
                    || RopeDocument::new("bench.py", content),
                    |mut doc| doc.edit(black_box(end), black_box(end), black_box("inserted_text")),
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_edit_replacement(c: &mut Criterion) {
    let mut group = c.benchmark_group("rope_edit_replace");

    for &doc_size in &[10_000usize, 100_000] {
        let content = generate_content(doc_size);
        let mid = content.len() / 2;
        let replace_end = (mid + 20).min(content.len());

        group.bench_with_input(
            BenchmarkId::new("chars", doc_size),
            &content,
            |b, content| {
                b.iter_batched(
                    || RopeDocument::new("bench.py", content),
                    |mut doc| {
                        doc.edit(
                            black_box(mid),
                            black_box(replace_end),
                            black_box("replacement_text_here"),
                        )
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_byte_to_point(c: &mut Criterion) {
    let mut group = c.benchmark_group("rope_byte_to_point");

    for &doc_size in &[10_000usize, 100_000] {
        let content = generate_content(doc_size);
        let doc = RopeDocument::new("bench.py", &content);

        group.bench_with_input(BenchmarkId::new("chars", doc_size), &doc_size, |b, _| {
            let positions = [
                0,
                doc_size / 4,
                doc_size / 2,
                doc_size * 3 / 4,
                doc_size - 1,
            ];
            b.iter(|| {
                let mut sum = 0usize;
                for &pos in &positions {
                    let (row, col) = doc.byte_to_point(black_box(pos));
                    sum += row + col;
                }
                sum
            });
        });
    }

    group.finish();
}

fn bench_chunk_at_byte(c: &mut Criterion) {
    let mut group = c.benchmark_group("rope_chunk_at_byte");

    for &doc_size in &[10_000usize, 100_000] {
        let content = generate_content(doc_size);
        let doc = RopeDocument::new("bench.py", &content);

        group.bench_with_input(BenchmarkId::new("chars", doc_size), &doc_size, |b, _| {
            // Simulate tree-sitter callback: sequential chunk reads
            b.iter(|| {
                let mut offset = 0;
                let mut total_bytes = 0usize;
                while offset < doc_size {
                    let chunk = doc.chunk_at_byte(black_box(offset));
                    if chunk.is_empty() {
                        break;
                    }
                    total_bytes += chunk.len();
                    offset += chunk.len();
                }
                total_bytes
            });
        });
    }

    group.finish();
}

fn bench_sequential_edits(c: &mut Criterion) {
    let content = generate_content(50_000);

    c.bench_function("rope_100_sequential_edits", |b| {
        b.iter_batched(
            || RopeDocument::new("bench.py", &content),
            |mut doc| {
                for i in 0..100 {
                    let pos = (i * 400) % doc.len_bytes().max(1);
                    doc.edit(pos, pos, "x");
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_edit_insertion,
    bench_edit_replacement,
    bench_byte_to_point,
    bench_chunk_at_byte,
    bench_sequential_edits,
);
criterion_main!(benches);
