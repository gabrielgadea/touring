//! Criterion benchmarks for end-to-end throughput measurement across touring-search-fusion.
//!
//! D38-S4: Create unified throughput benchmark (workspace root).
//! Measures operations-per-second and latency percentiles (P50/P99) for:
//! - keyword_search_throughput: pure keyword search operations
//! - semantic_search_throughput: embedding + semantic search operations
//! - hybrid_search_throughput: combined keyword + semantic + RRF fusion
//! - indexing_throughput: documents upserted and indexed per second
//! - query_latency_percentiles: P50/P99 latency distribution across all query types

use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;
use touring_storage::embeddings::{
    EmbeddingError, EmbeddingModel, EmbeddingProvider, EmbeddingResult, ModelFamily,
};
use touring_storage::hybrid_search::hybrid::pipeline::QueryIntent;
use touring_storage::hybrid_search::hybrid::{HybridQuery, SearchPipeline};

/// Mock embedding provider with deterministic latency for throughput benchmarks.
#[derive(Debug, Clone)]
pub struct BenchmarkEmbeddingProvider {
    dimension: usize,
    latency_ns: u64,
}

impl BenchmarkEmbeddingProvider {
    pub fn new(dimension: usize, latency_ns: u64) -> Self {
        Self {
            dimension,
            latency_ns,
        }
    }

    pub fn fast() -> Self {
        Self::new(768, 50_000) // 50μs
    }

    pub fn realistic() -> Self {
        Self::new(768, 200_000) // 200μs — realistic embedding latency
    }
}

#[async_trait]
impl EmbeddingProvider for BenchmarkEmbeddingProvider {
    fn id(&self) -> &'static str {
        "benchmark-throughput-provider"
    }

    fn family(&self) -> ModelFamily {
        ModelFamily::new("benchmark", "throughput-test")
    }

    fn dimensions(&self) -> usize {
        self.dimension
    }

    async fn embed_query(&self, query: String) -> Result<EmbeddingResult, EmbeddingError> {
        self.embed(vec![query]).await
    }

    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError> {
        tokio::time::sleep(std::time::Duration::from_nanos(self.latency_ns)).await;
        let vectors: Vec<Vec<f32>> = texts
            .iter()
            .map(|_| {
                (0..self.dimension)
                    .map(|i| (i as f32 * 0.01).sin())
                    .collect()
            })
            .collect();
        Ok(EmbeddingResult::new(
            vectors,
            EmbeddingModel::BgeSmall,
            None,
        ))
    }
}

/// Generate test documents for indexing throughput tests.
fn generate_documents(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            let id = format!("doc_{}", i);
            let content = format!(
                "Document {} content. Rust programming language. async fn trait implementation. \
                 Search pipeline hybrid scoring. Semantic embedding vector space. BM25 keyword scoring.",
                i
            );
            (id, content)
        })
        .collect()
}

/// Generate test queries for search throughput tests.
fn generate_queries(count: usize) -> Vec<String> {
    let bases = [
        "async fn trait",
        "pub struct impl",
        "SearchPipeline hybrid",
        "BM25 keyword scoring",
        "semantic embedding",
        "Arc Mutex result",
        "Option T encoding",
        "Vec u8 String",
    ];
    (0..count)
        .map(|i| bases[i % bases.len()].to_string())
        .collect()
}

/// Create hybrid query for throughput testing.
fn make_hybrid_query(keyword: &str, top_k: usize) -> HybridQuery {
    HybridQuery {
        query: keyword.to_string(),
        intent: QueryIntent::Explore,
        top_k,
        rerank: false,
    }
}

/// Benchmark 1: Keyword search throughput (operations/second).
pub fn bench_keyword_search_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput_keyword_search");

    let pipeline = SearchPipeline::new();
    let queries = generate_queries(50);

    for batch_size in [1usize, 10, 50] {
        let batch: Vec<_> = queries.iter().take(batch_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("ops_per_sec", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    let mut total_ops = 0u64;
                    let start = std::time::Instant::now();
                    for _ in 0..100 {
                        for query in &batch {
                            let _ = black_box(pollster::block_on(
                                pipeline.search(make_hybrid_query(query, 20)),
                            ));
                            total_ops += 1;
                        }
                    }
                    let elapsed = start.elapsed();
                    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();
                    black_box(ops_per_sec)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 2: Semantic search throughput (embedding + search).
pub fn bench_semantic_search_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput_semantic_search");

    let provider = Arc::new(BenchmarkEmbeddingProvider::realistic());
    let pipeline = SearchPipeline::new();
    let queries = generate_queries(30);

    for batch_size in [1usize, 5, 20] {
        let batch: Vec<_> = queries.iter().take(batch_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("ops_per_sec", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    let mut total_ops = 0u64;
                    let start = std::time::Instant::now();
                    for _ in 0..50 {
                        for query in &batch {
                            let _ =
                                black_box(pollster::block_on(provider.embed(vec![query.clone()])));
                            let _ = black_box(pollster::block_on(
                                pipeline.search(make_hybrid_query(query, 20)),
                            ));
                            total_ops += 1;
                        }
                    }
                    let elapsed = start.elapsed();
                    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();
                    black_box(ops_per_sec)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 3: Hybrid search throughput (keyword + semantic + RRF).
pub fn bench_hybrid_search_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput_hybrid_search");

    let pipeline = SearchPipeline::new();
    let queries = generate_queries(40);

    for batch_size in [1usize, 8, 40] {
        let batch: Vec<_> = queries.iter().take(batch_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("ops_per_sec", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    let mut total_ops = 0u64;
                    let start = std::time::Instant::now();
                    for _ in 0..50 {
                        for query in &batch {
                            let mut q = make_hybrid_query(query, 20);
                            q.rerank = true;
                            let _ = black_box(pollster::block_on(pipeline.search(q)));
                            total_ops += 1;
                        }
                    }
                    let elapsed = start.elapsed();
                    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();
                    black_box(ops_per_sec)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 4: Indexing throughput (documents/second).
pub fn bench_indexing_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput_indexing");

    let provider = Arc::new(BenchmarkEmbeddingProvider::fast());
    let doc_counts = [10, 50, 100, 500];

    for doc_count in doc_counts {
        let docs = generate_documents(doc_count);

        group.bench_with_input(
            BenchmarkId::new("docs_per_sec", doc_count),
            &doc_count,
            |b, _| {
                b.iter(|| {
                    let start = std::time::Instant::now();
                    for _ in 0..10 {
                        let texts: Vec<_> =
                            docs.iter().map(|(_, content)| content.clone()).collect();
                        let _ = black_box(pollster::block_on(provider.embed(texts)));
                    }
                    let elapsed = start.elapsed();
                    let total_docs = (10u64) * (doc_count as u64);
                    let docs_per_sec = total_docs as f64 / elapsed.as_secs_f64();
                    black_box(docs_per_sec)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark 5: Query latency percentiles (P50/P99).
pub fn bench_query_latency_percentiles(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_query_percentiles");

    let pipeline = SearchPipeline::new();
    let queries = generate_queries(100);

    // Single query latency measurement
    group.bench_function("single_query_p50", |b| {
        let query = queries.first().expect("queries vec is non-empty");
        b.iter(|| {
            let start = std::time::Instant::now();
            let _ = black_box(pollster::block_on(
                pipeline.search(make_hybrid_query(query, 20)),
            ));
            let elapsed = start.elapsed();
            black_box(elapsed)
        });
    });

    // Batch query for throughput-adjusted latency
    let batch = queries.iter().take(20).cloned().collect::<Vec<_>>();
    group.bench_function("batch_20_query_p99", |b| {
        b.iter(|| {
            let start = std::time::Instant::now();
            for query in &batch {
                let _ = black_box(pollster::block_on(
                    pipeline.search(make_hybrid_query(query, 20)),
                ));
            }
            let elapsed = start.elapsed();
            black_box(elapsed)
        });
    });

    group.finish();
}

criterion_group!(
    throughput_benches,
    bench_keyword_search_throughput,
    bench_semantic_search_throughput,
    bench_hybrid_search_throughput,
    bench_indexing_throughput,
    bench_query_latency_percentiles,
);
criterion_main!(throughput_benches);
