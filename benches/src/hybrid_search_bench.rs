//! Criterion benchmarks for hybrid search (keyword + semantic) through touring-search-fusion.
//!
//! D38-S3: Create criterion hybrid search benchmark at workspace root.
//! Benchmarks measure the latency of hybrid search operations including
//! keyword search, semantic search with embedding, and RRF fusion of both result sets.

use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;
use touring_storage::embeddings::{
    EmbeddingError, EmbeddingModel, EmbeddingProvider, EmbeddingResult,
};
use touring_storage::hybrid_search::RrfFusion;
use touring_storage::hybrid_search::hybrid::pipeline::QueryIntent;
use touring_storage::hybrid_search::hybrid::{HybridConfig, HybridQuery, SearchPipeline};

/// Mock embedding provider for deterministic benchmark results.
///
/// Produces synthetic embeddings with consistent, predictable timing
/// suitable for benchmarking the hybrid search pipeline without real model overhead.
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
        "mock-hybrid-benchmark-provider"
    }

    fn family(&self) -> touring_storage::embeddings::ModelFamily {
        touring_storage::embeddings::ModelFamily::new("mock", "hybrid-benchmark")
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

/// Generates realistic code-search hybrid queries of varying complexity.
fn generate_hybrid_queries(n: usize) -> Vec<String> {
    let base_queries = [
        "async fn trait implementation",
        "pub struct Arc Mutex",
        "impl Display Result error",
        "HashMap Vec u8 optimization",
        "String conversion cost embedding",
        "semantic search hybrid pipeline",
        "BM25 keyword scoring",
        "cross encoder reranker",
        "reciprocal rank fusion",
        "embedding vector dimension",
        "resource governor limit",
        "candidates per path tuning",
        "final results count",
        "rrf k constant",
        "semantic weight distribution",
        "keyword weight tuning",
        "top k retrieval",
        "SearchPipeline async search",
        "EmbeddingProvider trait",
        "HybridQuery intent",
        "QueryIntent Explore Understand",
        "SearchResult rank fusion",
        "hybrid search benchmark",
        "fusion score calculation",
    ];
    let mut queries = Vec::with_capacity(n);
    for i in 0..n {
        let base = base_queries[i % base_queries.len()].to_string();
        if i >= base_queries.len() {
            queries.push(format!("{} {}", base, i % 50));
        } else {
            queries.push(base);
        }
    }
    queries
}

/// Creates a hybrid query that exercises both keyword and semantic paths.
fn make_hybrid_query(keyword: &str, top_k: usize, intent: QueryIntent) -> HybridQuery {
    HybridQuery {
        query: keyword.to_string(),
        intent,
        top_k,
        rerank: false,
    }
}

/// Benchmark: single hybrid search query (keyword + semantic) latency.
pub fn bench_hybrid_search_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_search_single");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig {
        keyword_weight: 0.5,
        semantic_weight: 0.5,
        rrf_k: 60.0,
        candidates_per_path: 100,
        final_results: 10,
        rerank_enabled: false,
        reranker_model: None,
    };
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_hybrid_queries(100);

    for query_size in [1usize, 5, 10, 20] {
        let batch: Vec<_> = queries.iter().take(query_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("hybrid", query_size),
            &query_size,
            |b, _| {
                b.iter(|| {
                    for query in &batch {
                        let fut =
                            pipeline.search(make_hybrid_query(query, 10, QueryIntent::Explore));
                        pollster::block_on(fut);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: batch hybrid search with shared pipeline.
pub fn bench_hybrid_search_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_search_batch");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_hybrid_queries(100);

    for batch_size in [10usize, 50, 100] {
        let batch: Vec<_> = queries.iter().take(batch_size).cloned().collect();

        group.bench_with_input(
            BenchmarkId::new("parallel", batch_size),
            &batch_size,
            |b, _| {
                b.iter(|| {
                    for query in &batch {
                        let fut =
                            pipeline.search(make_hybrid_query(query, 20, QueryIntent::Understand));
                        pollster::block_on(fut);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: end-to-end latency from HybridQuery to fused results.
pub fn bench_hybrid_e2e_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_e2e");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_hybrid_queries(200);

    // Warm up
    for query in queries.iter().take(20) {
        let fut = pipeline.search(make_hybrid_query(query, 10, QueryIntent::Explore));
        pollster::block_on(fut);
    }

    group.bench_function("e2e_latency", |b| {
        b.iter(|| {
            let query = &queries[42 % queries.len()];
            let fut = pipeline.search(make_hybrid_query(query, 20, QueryIntent::Understand));
            pollster::block_on(fut);
        });
    });

    group.finish();
}

/// Benchmark: P95 latency for hybrid search operations.
pub fn bench_hybrid_latency_p95(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_latency_p95");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_hybrid_queries(200);

    // Warm up
    for query in queries.iter().take(20) {
        let fut = pipeline.search(make_hybrid_query(query, 10, QueryIntent::Explore));
        pollster::block_on(fut);
    }

    group.bench_function("p95_latency", |b| {
        b.iter(|| {
            let query = &queries[42 % queries.len()];
            let fut = pipeline.search(make_hybrid_query(query, 20, QueryIntent::Understand));
            pollster::block_on(fut);
        });
    });

    group.finish();
}

/// Benchmark: throughput (queries per second) for hybrid search.
pub fn bench_hybrid_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_throughput");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_hybrid_queries(100);

    group.bench_function("qps", |b| {
        b.iter(|| {
            let mut count = 0;
            for query in queries.iter().take(50) {
                let fut = pipeline.search(make_hybrid_query(query, 10, QueryIntent::Explore));
                pollster::block_on(fut);
                count += 1;
            }
            black_box(count);
        });
    });

    group.finish();
}

/// Benchmark: hybrid search with different keyword/semantic weight configurations.
pub fn bench_hybrid_weight_configurations(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_weight_config");

    // Different weight configurations to test
    let weight_configs = [
        (0.9, 0.1, "keyword_heavy"),
        (0.7, 0.3, "keyword_preferred"),
        (0.5, 0.5, "balanced"),
        (0.3, 0.7, "semantic_preferred"),
        (0.1, 0.9, "semantic_heavy"),
    ];

    for (keyword_weight, semantic_weight, name) in weight_configs {
        let provider = Arc::new(MockEmbeddingProvider::fast());
        let config = HybridConfig {
            keyword_weight,
            semantic_weight,
            rrf_k: 60.0,
            candidates_per_path: 100,
            final_results: 10,
            rerank_enabled: false,
            reranker_model: None,
        };
        let pipeline = SearchPipeline::with_provider(config, provider);
        let queries = generate_hybrid_queries(50);

        group.bench_with_input(BenchmarkId::new(name, 0), &0usize, |b, _| {
            b.iter(|| {
                for query in queries.iter().take(10) {
                    let fut = pipeline.search(make_hybrid_query(query, 10, QueryIntent::Explore));
                    pollster::block_on(fut);
                }
            });
        });
    }

    group.finish();
}

/// Benchmark: RRF fusion latency for combined keyword + semantic results.
pub fn bench_rrf_fusion_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_rrf_fusion");

    let fusion = RrfFusion::new(60.0);

    // Pre-computed static lists for hybrid RRF fusion benchmarks
    static KEYWORD_DOCS: [&str; 50] = [
        "kw_doc_0",
        "kw_doc_1",
        "kw_doc_2",
        "kw_doc_3",
        "kw_doc_4",
        "kw_doc_5",
        "kw_doc_6",
        "kw_doc_7",
        "kw_doc_8",
        "kw_doc_9",
        "kw_doc_10",
        "kw_doc_11",
        "kw_doc_12",
        "kw_doc_13",
        "kw_doc_14",
        "kw_doc_15",
        "kw_doc_16",
        "kw_doc_17",
        "kw_doc_18",
        "kw_doc_19",
        "kw_doc_20",
        "kw_doc_21",
        "kw_doc_22",
        "kw_doc_23",
        "kw_doc_24",
        "kw_doc_25",
        "kw_doc_26",
        "kw_doc_27",
        "kw_doc_28",
        "kw_doc_29",
        "kw_doc_30",
        "kw_doc_31",
        "kw_doc_32",
        "kw_doc_33",
        "kw_doc_34",
        "kw_doc_35",
        "kw_doc_36",
        "kw_doc_37",
        "kw_doc_38",
        "kw_doc_39",
        "kw_doc_40",
        "kw_doc_41",
        "kw_doc_42",
        "kw_doc_43",
        "kw_doc_44",
        "kw_doc_45",
        "kw_doc_46",
        "kw_doc_47",
        "kw_doc_48",
        "kw_doc_49",
    ];

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
        // Keyword ranking (ascending)
        let keyword_slice: Vec<(&str, usize)> = KEYWORD_DOCS
            .iter()
            .take(list_size)
            .enumerate()
            .map(|(i, s)| (*s, i + 1))
            .collect();
        // Semantic ranking (descending - different order)
        let semantic_slice: Vec<(&str, usize)> = SEMANTIC_DOCS
            .iter()
            .take(list_size)
            .rev()
            .enumerate()
            .map(|(rev_i, s)| (*s, list_size - rev_i))
            .collect();
        let lists: Vec<&[(&str, usize)]> = vec![&keyword_slice, &semantic_slice];

        group.bench_with_input(
            BenchmarkId::new(format!("fuse_{}", list_size), 0),
            &list_size,
            |b, _| {
                b.iter(|| {
                    let _ = fusion.fuse(black_box(&lists), black_box(&[0.5f32, 0.5f32]));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: RRF fusion with different weight configurations.
pub fn bench_rrf_fusion_weight_sensitivity(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_rrf_weight_sensitivity");

    let fusion = RrfFusion::new(60.0);

    // Fixed lists for weight sensitivity test
    static DOCS: [&str; 20] = [
        "doc_0", "doc_1", "doc_2", "doc_3", "doc_4", "doc_5", "doc_6", "doc_7", "doc_8", "doc_9",
        "doc_10", "doc_11", "doc_12", "doc_13", "doc_14", "doc_15", "doc_16", "doc_17", "doc_18",
        "doc_19",
    ];

    let keyword_slice: Vec<(&str, usize)> =
        DOCS.iter().enumerate().map(|(i, s)| (*s, i + 1)).collect();
    let semantic_slice: Vec<(&str, usize)> = DOCS
        .iter()
        .enumerate()
        .rev()
        .map(|(rev_i, s)| (*s, DOCS.len() - rev_i))
        .collect();
    let lists: Vec<&[(&str, usize)]> = vec![&keyword_slice, &semantic_slice];

    let weight_configs = [
        ([0.9f32, 0.1f32], "kw_90_sem_10"),
        ([0.7f32, 0.3f32], "kw_70_sem_30"),
        ([0.5f32, 0.5f32], "kw_50_sem_50"),
        ([0.3f32, 0.7f32], "kw_30_sem_70"),
        ([0.1f32, 0.9f32], "kw_10_sem_90"),
    ];

    for (weights, name) in weight_configs {
        group.bench_with_input(BenchmarkId::new(name, 0), &0usize, |b, _| {
            b.iter(|| {
                let _ = fusion.fuse(black_box(&lists), black_box(&weights));
            });
        });
    }

    group.finish();
}

/// Benchmark: hybrid search with varying top_k (final results count).
pub fn bench_hybrid_topk_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_topk_scaling");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let pipeline = SearchPipeline::with_provider(config, provider);
    let queries = generate_hybrid_queries(50);

    for top_k in [5usize, 10, 20, 50, 100] {
        group.bench_with_input(BenchmarkId::new("topk", top_k), &top_k, |b, _| {
            b.iter(|| {
                for query in queries.iter().take(10) {
                    let fut =
                        pipeline.search(make_hybrid_query(query, top_k, QueryIntent::Explore));
                    pollster::block_on(fut);
                }
            });
        });
    }

    group.finish();
}

/// Benchmark: hybrid search intent variations (Explore vs Understand).
pub fn bench_hybrid_intent_variations(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_intent");

    let provider = Arc::new(MockEmbeddingProvider::fast());
    let config = HybridConfig::default();
    let queries = generate_hybrid_queries(50);

    for intent in [QueryIntent::Explore, QueryIntent::Understand] {
        let pipeline = SearchPipeline::with_provider(config.clone(), provider.clone());

        group.bench_with_input(
            BenchmarkId::new(format!("{:?}", intent), 0),
            &0usize,
            |b, _| {
                b.iter(|| {
                    for query in queries.iter().take(10) {
                        let fut = pipeline.search(make_hybrid_query(query, 10, intent));
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
    bench_hybrid_search_single,
    bench_hybrid_search_batch,
    bench_hybrid_e2e_latency,
    bench_hybrid_latency_p95,
    bench_hybrid_throughput,
    bench_hybrid_weight_configurations,
    bench_rrf_fusion_latency,
    bench_rrf_fusion_weight_sensitivity,
    bench_hybrid_topk_scaling,
    bench_hybrid_intent_variations,
);
criterion_main!(benches);

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity check: `RrfFusion::fuse_two` must be a pure, deterministic function
    /// of its inputs. Two identical fusions must agree exactly — giving F3.1 (test
    /// coverage) on this bench-only file without polluting the criterion harness
    /// with shared state. (Migrated 2026-06-29 to the current rank-based instance
    /// API — `fuse(&self, lists, weights)` / `fuse_two` — from the obsolete static
    /// score-based signature.)
    #[test]
    fn rrf_fusion_is_deterministic_for_fixed_inputs() {
        let fusion = RrfFusion::new(60.0);
        let list_a: &[(&str, usize)] = &[("alpha", 1), ("beta", 2)];
        let list_b: &[(&str, usize)] = &[("beta", 1), ("gamma", 2)];
        let r1 = fusion.fuse_two(list_a, list_b);
        let r2 = fusion.fuse_two(list_a, list_b);
        assert_eq!(r1.len(), r2.len(), "RRF result count must match");
        for (x, y) in r1.iter().zip(r2.iter()) {
            assert_eq!(x.0, y.0, "RRF order must match");
            assert!(
                (x.1 - y.1).abs() < 1e-6,
                "RRF score must match: {} vs {}",
                x.1,
                y.1
            );
        }
    }

    /// Core reciprocal-rank-fusion property: a document ranked #1 in *both* lists
    /// must outrank documents that appear in only one list. (Replaces the obsolete
    /// `MockProvider` test — that deterministic mock embedding provider was removed
    /// from `touring-storage`; this exercises the current pure fusion API instead.)
    #[test]
    fn rrf_fusion_rewards_documents_in_both_lists() {
        let fusion = RrfFusion::new(60.0);
        let list_a: &[(&str, usize)] = &[("shared", 1), ("only_a", 2)];
        let list_b: &[(&str, usize)] = &[("shared", 1), ("only_b", 2)];
        let fused = fusion.fuse_two(list_a, list_b);
        assert_eq!(
            fused.first().map(|(id, _)| id.as_str()),
            Some("shared"),
            "a doc ranked #1 in both lists must win the fusion"
        );
    }
}
