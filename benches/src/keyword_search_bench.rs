//! Criterion benchmarks for keyword search through touring-search-fusion.
//!
//! D38-S1: Create criterion keyword search benchmark at workspace root.
//! Benchmarks measure the latency of keyword search operations via
//! SearchPipeline's search pipeline (keyword + semantic hybrid).

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_storage::hybrid_search::hybrid::{HybridConfig, HybridQuery, SearchPipeline};

/// Generates realistic code-search queries of varying complexity.
fn generate_queries(n: usize) -> Vec<String> {
    let base_queries = [
        "async fn trait",
        "pub struct",
        "impl Display",
        "Result<T, E>",
        "Arc<Mutex<T>>",
        "let mut",
        "Option<T>",
        "HashMap<K, V>",
        "Vec<u8>",
        "String",
        "touring index",
        "SearchPipeline",
        "RrfFusion",
        "hybrid search",
        "BM25 scoring",
        "semantic embedding",
        "keyword weight",
        "reciprocal rank",
        "cross-encoder reranker",
        "fusion score",
    ];
    let mut queries = Vec::with_capacity(n);
    for i in 0..n {
        let base = base_queries[i % base_queries.len()].to_string();
        if i >= base_queries.len() {
            queries.push(format!("{} {}", base, i % 10));
        } else {
            queries.push(base);
        }
    }
    queries
}

/// Creates a hybrid query from a keyword string.
fn make_query(keyword: &str, top_k: usize) -> HybridQuery {
    HybridQuery {
        query: keyword.to_string(),
        intent: touring_storage::hybrid_search::hybrid::pipeline::QueryIntent::Explore,
        top_k,
        rerank: false,
    }
}

pub fn bench_keyword_search_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("keyword_search_single");

    let pipeline = SearchPipeline::new();
    let queries = generate_queries(100);

    for query_size in [1usize, 5, 10, 20] {
        let batch: Vec<_> = queries.iter().take(query_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("hybrid", query_size),
            &query_size,
            |b, _| {
                b.iter(|| {
                    for query in &batch {
                        let fut = pipeline.search(make_query(query, 10));
                        pollster::block_on(fut);
                    }
                });
            },
        );
    }

    group.finish();
}

pub fn bench_keyword_search_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("keyword_search_batch");

    let pipeline = SearchPipeline::new();
    let queries = generate_queries(100);

    for batch_size in [10usize, 50, 100] {
        let batch: Vec<_> = queries.iter().take(batch_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("parallel", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    for query in &batch {
                        let fut = pipeline.search(make_query(query, 20));
                        pollster::block_on(fut);
                    }
                });
            },
        );
    }

    group.finish();
}

pub fn bench_rrf_fusion(c: &mut Criterion) {
    let mut group = c.benchmark_group("rrf_fusion");

    let fusion = touring_storage::hybrid_search::RrfFusion::new(60.0);

    // Pre-computed static lists for RRF fusion benchmarks
    static KEYWORD_DOCS: [&str; 50] = [
        "doc_0", "doc_1", "doc_2", "doc_3", "doc_4", "doc_5", "doc_6", "doc_7", "doc_8", "doc_9",
        "doc_10", "doc_11", "doc_12", "doc_13", "doc_14", "doc_15", "doc_16", "doc_17", "doc_18",
        "doc_19", "doc_20", "doc_21", "doc_22", "doc_23", "doc_24", "doc_25", "doc_26", "doc_27",
        "doc_28", "doc_29", "doc_30", "doc_31", "doc_32", "doc_33", "doc_34", "doc_35", "doc_36",
        "doc_37", "doc_38", "doc_39", "doc_40", "doc_41", "doc_42", "doc_43", "doc_44", "doc_45",
        "doc_46", "doc_47", "doc_48", "doc_49",
    ];

    for list_size in [10usize, 25, 50] {
        let keyword_slice: Vec<(&str, usize)> = KEYWORD_DOCS
            .iter()
            .take(list_size)
            .enumerate()
            .map(|(i, s)| (*s, i))
            .collect();
        // Semantic list is reversed
        let semantic_slice: Vec<(&str, usize)> = KEYWORD_DOCS
            .iter()
            .take(list_size)
            .rev()
            .enumerate()
            .map(|(rev_i, s)| (*s, list_size - 1 - rev_i))
            .collect();
        let lists: Vec<&[(&str, usize)]> = vec![&keyword_slice, &semantic_slice];

        group.bench_with_input(BenchmarkId::new("fuse", list_size), &list_size, |b, _| {
            b.iter(|| {
                let _ = fusion.fuse(black_box(&lists), black_box(&[1.0f32, 1.0f32]));
            });
        });
    }

    group.finish();
}

pub fn bench_keyword_latency_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("keyword_latency_p95");

    let pipeline = SearchPipeline::new();
    let queries = generate_queries(200);

    // Warm up
    for query in queries.iter().take(20) {
        let fut = pipeline.search(make_query(query, 10));
        pollster::block_on(fut);
    }

    group.bench_function("p95_latency", |b| {
        b.iter(|| {
            let query = &queries[42 % queries.len()];
            let fut = pipeline.search(make_query(query, 20));
            pollster::block_on(fut);
        });
    });

    group.finish();
}

pub fn bench_hybrid_config_variations(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_config");

    for keyword_weight in [0.2, 0.4, 0.6, 0.8] {
        let config = HybridConfig {
            keyword_weight,
            semantic_weight: 1.0 - keyword_weight,
            rrf_k: 60.0,
            candidates_per_path: 100,
            final_results: 10,
            rerank_enabled: false,
            reranker_model: None,
        };
        let pipeline = SearchPipeline::with_config(config);
        let queries = generate_queries(50);

        group.bench_with_input(
            BenchmarkId::new(format!("kw_{:.1}", keyword_weight), 0),
            &keyword_weight,
            |b, _| {
                b.iter(|| {
                    for query in queries.iter().take(10) {
                        let fut = pipeline.search(make_query(query, 10));
                        pollster::block_on(fut);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_keyword_search_single,
    bench_keyword_search_batch,
    bench_rrf_fusion,
    bench_keyword_latency_distribution,
    bench_hybrid_config_variations,
);
criterion_main!(benches);
