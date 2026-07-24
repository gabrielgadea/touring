//! Criterion benchmarks for semantic search through touring-search-fusion.
//!
//! D38-S2: Create criterion semantic search benchmark at workspace root.
//! Benchmarks measure the latency of semantic search operations including
//! embedding computation and RRF fusion scoring.

use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;
use touring_storage::embeddings::{
    EmbeddingError, EmbeddingModel, EmbeddingProvider, EmbeddingResult,
};
use touring_storage::hybrid_search::hybrid::pipeline::QueryIntent;
use touring_storage::hybrid_search::hybrid::{HybridConfig, HybridQuery, SearchPipeline};

/// Mock embedding provider for deterministic benchmark results.
///
/// Produces synthetic embeddings with consistent, predictable timing
/// suitable for benchmarking the search pipeline without real model overhead.
#[derive(Debug, Clone)]
pub struct MockEmbeddingProvider {
    dimension: usize,
    latency_ns: u64,
}

impl MockEmbeddingProvider {
    /// Creates a new mock provider with specified dimension and per-call latency.
    pub fn new(dimension: usize, latency_ns: u64) -> Self {
        Self {
            dimension,
            latency_ns,
        }
    }

    /// Creates a fast mock provider (minimal latency).
    pub fn fast() -> Self {
        Self::new(768, 50_000) // 50us latency
    }

    /// Creates a slow mock provider (higher latency for stress testing).
    pub fn slow() -> Self {
        Self::new(768, 500_000) // 500us latency
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    fn id(&self) -> &'static str {
        "mock-benchmark-provider"
    }

    fn family(&self) -> touring_storage::embeddings::ModelFamily {
        touring_storage::embeddings::ModelFamily::new("mock", "benchmark")
    }

    fn dimensions(&self) -> usize {
        self.dimension
    }

    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError> {
        // Simulate latency
        tokio::time::sleep(std::time::Duration::from_nanos(self.latency_ns)).await;

        let vectors: Vec<Vec<f32>> = texts
            .iter()
            .map(|text| {
                // Deterministic hash-based embedding for reproducibility
                let hash = simple_hash(text.as_bytes());
                vec![(hash % 1000) as f32 / 1000.0; self.dimension]
            })
            .collect();

        Ok(EmbeddingResult::new(
            vectors,
            EmbeddingModel::BgeSmall,
            Some(texts.iter().map(|t| t.len()).sum()),
        ))
    }

    async fn embed_query(&self, text: String) -> Result<EmbeddingResult, EmbeddingError> {
        self.embed(vec![text]).await
    }
}

/// Simple hash function for deterministic embeddings.
fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Generates realistic code-search queries for semantic benchmarks.
fn generate_semantic_queries(n: usize) -> Vec<String> {
    let base_queries = [
        "async trait implementation",
        "pub fn search pipeline",
        "Arc Mutex wrapper pattern",
        "Result error handling",
        "HashMap lookup performance",
        "Vec append optimization",
        "String conversion cost",
        "embedding vector dimension",
        "semantic search latency",
        "reciprocal rank fusion",
        "cross encoder reranker",
        "BM25 keyword scoring",
        "hybrid search configuration",
        "resource governor limit",
        "candidates per path",
        "final results count",
        "rrf k constant tuning",
        "semantic weight distribution",
        "keyword weight tuning",
        "top k retrieval",
        "SearchPipeline async",
        "EmbeddingProvider trait",
        "HybridQuery intent",
        "QueryIntent Explore",
        "SearchResult rank",
    ];
    let mut queries = Vec::with_capacity(n);
    for i in 0..n {
        let base = base_queries[i % base_queries.len()].to_string();
        if i >= base_queries.len() {
            queries.push(format!("{} {}", base, i % 100));
        } else {
            queries.push(base);
        }
    }
    queries
}

/// Creates a hybrid query optimized for semantic search.
fn make_semantic_query(keyword: &str, top_k: usize) -> HybridQuery {
    HybridQuery {
        query: keyword.to_string(),
        intent: QueryIntent::Understand,
        top_k,
        rerank: false,
    }
}

/// Benchmark: single semantic search query latency.
pub fn bench_semantic_search_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_search_single");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig {
        keyword_weight: 0.3,
        semantic_weight: 0.7,
        rrf_k: 60.0,
        candidates_per_path: 100,
        final_results: 10,
        rerank_enabled: false,
        reranker_model: None,
    };
    let _pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_semantic_queries(100);

    for query_size in [1usize, 5, 10, 20] {
        let batch: Vec<_> = queries.iter().take(query_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("semantic", query_size),
            &query_size,
            |b, _| {
                b.iter(|| {
                    for query in &batch {
                        let provider = Arc::new(MockEmbeddingProvider::fast());
                        let config = HybridConfig {
                            keyword_weight: 0.3,
                            semantic_weight: 0.7,
                            rrf_k: 60.0,
                            candidates_per_path: 100,
                            final_results: 10,
                            rerank_enabled: false,
                            reranker_model: None,
                        };
                        let pipeline = SearchPipeline::with_provider(config, provider);
                        let fut = pipeline.search(make_semantic_query(query, 10));
                        pollster::block_on(fut);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: batch semantic search with shared pipeline.
pub fn bench_semantic_search_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_search_batch");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_semantic_queries(100);

    for batch_size in [10usize, 50, 100] {
        let batch: Vec<_> = queries.iter().take(batch_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("parallel", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    for query in &batch {
                        let fut = pipeline.search(make_semantic_query(query, 20));
                        pollster::block_on(fut);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: embedding computation latency in isolation.
pub fn bench_embedding_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedding_latency");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let queries = generate_semantic_queries(50);

    for batch_size in [1usize, 5, 10, 25] {
        let batch: Vec<_> = queries.iter().take(batch_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("embed", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    let provider = provider.clone();
                    let fut = async {
                        let texts: Vec<String> = batch.clone();
                        provider.embed(texts).await
                    };
                    // Result is intentionally discarded — this is a
                    // throughput bench, not a correctness bench. `let _`
                    // documents the intent and silences `unused_must_use`.
                    let _ = pollster::block_on(fut);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: P95 latency for semantic search operations.
pub fn bench_semantic_latency_p95(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_latency_p95");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_semantic_queries(200);

    // Warm up
    for query in queries.iter().take(20) {
        let fut = pipeline.search(make_semantic_query(query, 10));
        pollster::block_on(fut);
    }

    group.bench_function("p95_latency", |b| {
        b.iter(|| {
            let query = &queries[42 % queries.len()];
            let fut = pipeline.search(make_semantic_query(query, 20));
            pollster::block_on(fut);
        });
    });

    group.finish();
}

/// Benchmark: throughput (queries per second) for semantic search.
pub fn bench_semantic_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_throughput");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_semantic_queries(100);

    group.bench_function("qps", |b| {
        b.iter(|| {
            let mut count = 0;
            for query in queries.iter().take(50) {
                let fut = pipeline.search(make_semantic_query(query, 10));
                pollster::block_on(fut);
                count += 1;
            }
            black_box(count);
        });
    });

    group.finish();
}

/// Benchmark: semantic search with different embedding provider latencies.
pub fn bench_embedding_provider_latency_variations(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedding_provider_latency");

    let latencies = [
        ("fast_50us", MockEmbeddingProvider::new(768, 50_000)),
        ("medium_200us", MockEmbeddingProvider::new(768, 200_000)),
        ("slow_500us", MockEmbeddingProvider::new(768, 500_000)),
    ];

    for (name, provider) in latencies {
        let provider = Arc::new(provider);
        let config = HybridConfig::default();
        let pipeline = SearchPipeline::with_provider(config, provider);
        let queries = generate_semantic_queries(50);

        group.bench_with_input(BenchmarkId::new(name, 0), &0usize, |b, _| {
            b.iter(|| {
                for query in queries.iter().take(10) {
                    let fut = pipeline.search(make_semantic_query(query, 10));
                    pollster::block_on(fut);
                }
            });
        });
    }

    group.finish();
}

/// Benchmark: RRF fusion scoring with semantic results.
pub fn bench_semantic_rrf_fusion(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic_rrf_fusion");

    use touring_storage::hybrid_search::RrfFusion;

    let fusion = RrfFusion::new(60.0);

    // Pre-computed static lists for semantic RRF benchmarks
    static SEMANTIC_DOCS: [&str; 50] = [
        "sem_doc_0",
        "sem_doc_1",
        "sem_doc_2",
        "sem_doc_3",
        "sem_doc_4",
        "sem_doc_5",
        "sem_doc_6",
        "sem_doc_7",
        "sem_doc_8",
        "sem_doc_9",
        "sem_doc_10",
        "sem_doc_11",
        "sem_doc_12",
        "sem_doc_13",
        "sem_doc_14",
        "sem_doc_15",
        "sem_doc_16",
        "sem_doc_17",
        "sem_doc_18",
        "sem_doc_19",
        "sem_doc_20",
        "sem_doc_21",
        "sem_doc_22",
        "sem_doc_23",
        "sem_doc_24",
        "sem_doc_25",
        "sem_doc_26",
        "sem_doc_27",
        "sem_doc_28",
        "sem_doc_29",
        "sem_doc_30",
        "sem_doc_31",
        "sem_doc_32",
        "sem_doc_33",
        "sem_doc_34",
        "sem_doc_35",
        "sem_doc_36",
        "sem_doc_37",
        "sem_doc_38",
        "sem_doc_39",
        "sem_doc_40",
        "sem_doc_41",
        "sem_doc_42",
        "sem_doc_43",
        "sem_doc_44",
        "sem_doc_45",
        "sem_doc_46",
        "sem_doc_47",
        "sem_doc_48",
        "sem_doc_49",
    ];

    for list_size in [10usize, 25, 50] {
        let semantic_slice: Vec<(&str, usize)> = SEMANTIC_DOCS
            .iter()
            .take(list_size)
            .enumerate()
            .map(|(i, s)| (*s, i + 1))
            .collect();
        // Keyword list is reversed (different ranking)
        let keyword_slice: Vec<(&str, usize)> = SEMANTIC_DOCS
            .iter()
            .take(list_size)
            .rev()
            .enumerate()
            .map(|(rev_i, s)| (*s, list_size - rev_i))
            .collect();
        let lists: Vec<&[(&str, usize)]> = vec![&keyword_slice, &semantic_slice];

        group.bench_with_input(BenchmarkId::new("fuse", list_size), &list_size, |b, _| {
            b.iter(|| {
                let _ = fusion.fuse(
                    black_box(&lists),
                    black_box(&[0.3f32, 0.7f32]), // Semantic-heavy weights
                );
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_semantic_search_single,
    bench_semantic_search_batch,
    bench_embedding_latency,
    bench_semantic_latency_p95,
    bench_semantic_throughput,
    bench_embedding_provider_latency_variations,
    bench_semantic_rrf_fusion,
);
criterion_main!(benches);
