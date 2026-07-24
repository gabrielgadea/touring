# touring-simd — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 9119

## Overview

SIMD-accelerated vector operations for Touring — 26 modules providing quantization, HNSW ANN, cosine similarity, matrix operations, GPU support, and ACO learning. Targets high-throughput vector computation for semantic search.

## Key Types

`ScalarQuantizer` | `BlockQuantizer` | `EmbeddingU4` | `SimdBackend` | `AcoPheromone`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 134 | Library entry, public API |
| `src/quantization.rs` | 1672 | — |
| `src/gpu/mod.rs` | 674 | — |
| `src/simd_utils/matrix.rs` | 641 | — |
| `src/statistics/reconciliation.rs` | 599 | — |
| `src/similarity/topk.rs` | 580 | — |
| `src/simd_utils/ops.rs` | 566 | — |
| `src/learning.rs` | 505 | — |
| `src/cortex.rs` | 441 | — |
| `src/similarity/cosine.rs` | 429 | — |
| `src/ann/hnsw.rs` | 416 | — |
| `src/financial.rs` | 410 | — |
| `src/similarity/distance.rs` | 352 | — |
| `src/similarity/jaccard.rs` | 255 | — |
| `src/statistics/drift.rs` | 240 | — |
| `src/statistics/ranking.rs` | 207 | — |
| `src/simd_utils/portable.rs` | 193 | — |
| `src/buffer_pool.rs` | 178 | — |
| `src/simd_utils/horizontal.rs` | 164 | — |
| `src/simd_utils/vector_ops.rs` | 146 | — |
| `src/simd_utils/mod.rs` | 113 | — |

## Key Features

- **SIMD quantization**: Vector quantization for memory-efficient storage
- **HNSW ANN**: Hierarchical navigable small world for approximate nearest neighbor
- **Cosine similarity**: SIMD-accelerated cosine computation
- **GPU support**: GPU-accelerated vector operations
- **ACO learning**: Ant Colony Optimization for vector search tuning

## Integration Points

- touring-vector-store: SIMD-accelerated vector operations
- touring-search-fusion: semantic similarity in hybrid search
- touring-learning: ACO-based vector search optimization

## Technology

Pure Rust. Portable SIMD (std::simd). No unsafe at crate level.
