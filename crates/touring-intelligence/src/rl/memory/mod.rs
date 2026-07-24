//! Memory Subsystem — RLM, SemanticRecall, WorkingMemory, PatternCluster, and CRDT.
//!
//! Unified from touring/src/memory/ (2602 LOC) + rust-core working memory (254 LOC)
//!
//! # Modules
//!
//! - [`crdt_graph`] — CRDT semantic graph with delta-based sync and rkyv zero-copy persistence
//! - [`rle`] — Run-Length Encoding for hot-path delta compression (`CrdtDelta` fields)
//! - [`pattern_cluster`] — HNSW-based lazy clustering (ALT-B1): SIMD KNN, EMA centroid drift, threshold filtering

pub mod async_rlm;
pub mod crdt_graph;
#[cfg(feature = "u4-quantization")]
pub mod embedding_u4;
pub mod palace;
pub mod pattern_cluster;
pub mod recall;
pub mod recall_cache;
pub mod rlm;
pub mod tier_manager;
pub mod working;

#[cfg(feature = "hnsw-working-memory")]
pub mod hnsw_working;
pub mod rle;

// Re-exports for convenience
pub use async_rlm::AsyncRlmMemory;
pub use crdt_graph::{ActorId, CrdtDelta, CrdtEdge, CrdtNodeId, CrdtSemanticGraph, NodeWeight};
#[cfg(feature = "async-memory")]
pub use pattern_cluster::AsyncPatternClusterer;
pub use pattern_cluster::{
    ClusterError, ClusterMatch, ClusterMember, ClusterStats, ClusteredPattern, PatternCluster,
    PatternClusterer,
};
pub use recall::{ChunkMatch, RecallStats, SemanticRecall, rrf_fuse};
pub use recall_cache::{
    CacheStats as RecallCacheStats, RecallCache, RecallEntry as CachedRecallEntry,
};
pub use rlm::{GraphMeta, MemoryMatch, MemoryStats, MemoryTier, RlmMemory};
pub use working::{LruWorkingMemory, WorkingMemory};

pub use palace::{
    MAX_CLOSET_LEN, MAX_DRAWER_LEN, MAX_ROOM_LEN, MAX_WING_LEN, PalaceHierarchy, PalacePathError,
};

#[cfg(feature = "hnsw-working-memory")]
pub use hnsw_working::WorkingMemoryHnsw;

// NOTE: MemoryStore (unified RLM+Recall with auto-embedding) is NOT included here
// because it depends on `EmbeddingClient` which lives in touring-core (behind
// `gpu-embeddings` feature). That integration belongs in touring-server or
// touring-cognitive where async runtime + GPU client are available.
// NOTE(P5): wire MemoryStore to touring-server when async integration is ready.
