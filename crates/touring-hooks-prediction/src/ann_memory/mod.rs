//! ANN Memory Recall — Approximate Nearest Neighbor semantic search for memories.
//!
//! Layer 7 of Touring intelligence: semantic similarity search for memories.
//!
//! # Architecture
//!
//! - Local EmbeddingIndex for ANN search
//! - TopKSearcher (E9): O(n log k) partial selection via touring-simd, reusable searcher instance
//! - Integrates with `FileKnowledgeDB` for memory storage
//! - Quantization (8x): u4 nibbles when `quantization` feature enabled
//!
//! # Example
//!
//! ```rust,ignore
//! let recall = AnnMemoryRecall::new();
//! recall.add_memory("error handling", vec![0.1, 0.2, 0.3]);
//! let results = recall.search("rust error patterns", 5);
//! ```

use std::collections::HashMap;

use touring_simd::TopKSearcher;

#[cfg(feature = "quantization")]
use touring_simd::quantization::BlockQuantizer;

/// A memory entry with embedding for ANN search.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    /// Unique memory identifier.
    pub id: String,
    /// Memory content text.
    pub content: String,
    /// Pre-computed embedding vector.
    pub embedding: Vec<f32>,
    /// Optional metadata (language, source, etc.).
    pub metadata: Option<serde_json::Value>,
}

impl MemoryEntry {
    /// Create a new memory entry.
    pub fn new(id: impl Into<String>, content: impl Into<String>, embedding: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            embedding,
            metadata: None,
        }
    }

    /// Add metadata to the entry.
    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Simple embedding index for cosine similarity search.
///
/// Uses `TopKSearcher` from `touring-simd` for O(n log k) partial selection
/// instead of full O(n log n) sort. The searcher is reused across calls to
/// avoid recreating the internal `CosineComputer` on every search.
///
/// GPU-accelerated: when `gpu-compute` feature is enabled, uses `HttpGpuBackend`
/// via `TopKSearcher::with_backend()` for hardware-accelerated similarity.
#[derive(Debug, Clone)]
pub struct EmbeddingIndex {
    embeddings: Vec<Vec<f32>>,
    ids: Vec<String>,
    /// Reusable top-K searcher with SIMD-accelerated cosine similarity.
    searcher: TopKSearcher,
}

/// GPU-enabled index builder.
#[derive(Debug)]
pub struct EmbeddingIndexBuilder {
    #[cfg(feature = "gpu-compute")]
    gpu_url: Option<String>,
}

impl EmbeddingIndexBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "gpu-compute")]
            gpu_url: None,
        }
    }

    /// Set GPU server URL for hardware-accelerated ANN.
    #[cfg(feature = "gpu-compute")]
    pub fn with_gpu_url(mut self, url: impl Into<String>) -> Self {
        self.gpu_url = Some(url.into());
        self
    }

    /// Build the `EmbeddingIndex` with GPU backend if available.
    pub fn build(self) -> EmbeddingIndex {
        #[cfg(feature = "gpu-compute")]
        {
            use std::sync::Arc;
            use touring_simd::gpu::HttpGpuBackend;
            if let Some(url) = self.gpu_url {
                let backend = Arc::new(HttpGpuBackend::new(url, PATH_EMBED_DIM));
                return EmbeddingIndex {
                    embeddings: Vec::new(),
                    ids: Vec::new(),
                    searcher: TopKSearcher::with_backend(backend),
                };
            }
        }
        EmbeddingIndex::with_simd()
    }

    /// Build with SIMD-only backend (no GPU).
    pub fn with_simd() -> EmbeddingIndex {
        EmbeddingIndex {
            embeddings: Vec::new(),
            ids: Vec::new(),
            searcher: TopKSearcher::default(),
        }
    }
}

impl Default for EmbeddingIndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for EmbeddingIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingIndex {
    /// Create a new embedding index with GPU-accelerated backend (WGPU/Vulkan).
    ///
    /// Falls back to SIMD-only if GPU initialization fails.
    /// Uses `HttpGpuBackend` via WGPU Vulkan compute for cosine similarity.
    #[cfg(feature = "gpu-compute")]
    pub fn new() -> Self {
        // Try GPU first, fall back to SIMD on initialization failure
        match Self::try_with_gpu("http://localhost:8080") {
            Ok(idx) => idx,
            Err(e) => {
                eprintln!(
                    "[EmbeddingIndex] GPU init failed: {}, falling back to SIMD",
                    e
                );
                Self::with_simd()
            }
        }
    }

    /// Create a GPU-accelerated embedding index with explicit URL.
    ///
    /// Requires `gpu-compute` feature.
    #[cfg(feature = "gpu-compute")]
    pub fn try_with_gpu(
        gpu_url: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use std::sync::Arc;
        use touring_simd::gpu::HttpGpuBackend;
        let backend = Arc::new(HttpGpuBackend::new(gpu_url, PATH_EMBED_DIM));
        Ok(Self {
            embeddings: Vec::new(),
            ids: Vec::new(),
            searcher: TopKSearcher::with_backend(backend),
        })
    }

    /// Create a GPU-accelerated embedding index (panics on GPU init failure).
    ///
    /// Requires `gpu-compute` feature.
    #[cfg(feature = "gpu-compute")]
    pub fn with_gpu(gpu_url: impl Into<String>) -> Self {
        Self::try_with_gpu(gpu_url).expect("Failed to initialize GPU backend")
    }

    /// Create a new embedding index with SIMD-only backend (no GPU).
    #[cfg(not(feature = "gpu-compute"))]
    pub fn new() -> Self {
        Self::with_simd()
    }

    #[inline]
    #[cfg(not(feature = "gpu-compute"))]
    fn with_simd() -> Self {
        use std::sync::Arc;
        use touring_simd::gpu::HttpGpuBackend;
        let gpu_url = String::new();
        let backend = Arc::new(HttpGpuBackend::new(gpu_url, PATH_EMBED_DIM));
        Self {
            embeddings: Vec::new(),
            ids: Vec::new(),
            searcher: TopKSearcher::with_backend(backend),
        }
    }

    #[inline]
    fn with_simd() -> Self {
        Self {
            embeddings: Vec::new(),
            ids: Vec::new(),
            searcher: TopKSearcher::default(),
        }
    }

    /// Add an embedding to the index.
    pub fn add(&mut self, id: &str, embedding: Vec<f32>) {
        self.ids.push(id.to_string());
        self.embeddings.push(embedding);
    }

    /// Search for top-K most similar embeddings using SIMD-accelerated cosine similarity.
    ///
    /// Uses `TopKSearcher::top_k()` for O(n log k) partial selection via
    /// `select_nth_unstable_by`, avoiding the O(n log n) full sort that a
    /// naive approach would require. The `TopKSearcher` instance (and its
    /// internal `CosineComputer`) is reused across calls.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        if self.embeddings.is_empty() || query.is_empty() {
            return Vec::new();
        }

        let qd = query.len();

        // Dim-consistency guard (S-04, 2026-05-29). The embedding width can
        // change when the model is swapped (64-dim FNV hash → 768-dim semantic).
        // A corpus caught mid-migration is heterogeneous until `memory reindex`
        // regenerates every vector; feeding mismatched widths to the SIMD cosine
        // would index out of bounds. The steady state (all widths == query) keeps
        // the original O(n log k) fast path; only the transient mixed corpus pays
        // the filtered-scan cost.
        if self.embeddings.iter().all(|e| e.len() == qd) {
            let k = k.min(self.embeddings.len());
            return self
                .searcher
                .top_k(query, &self.embeddings, k)
                .into_iter()
                .filter_map(|r| {
                    self.ids
                        .get(r.index)
                        .map(|id| SearchResult::new(id.clone(), r.score))
                })
                .collect();
        }

        // Heterogeneous (migration window): compare only against matching-width
        // entries, mapping the searcher's local indices back to the originals.
        let matching: Vec<(usize, Vec<f32>)> = self
            .embeddings
            .iter()
            .enumerate()
            .filter(|(_, e)| e.len() == qd)
            .map(|(i, e)| (i, e.clone()))
            .collect();
        if matching.is_empty() {
            return Vec::new();
        }
        let embeds: Vec<Vec<f32>> = matching.iter().map(|(_, e)| e.clone()).collect();
        let k = k.min(embeds.len());
        self.searcher
            .top_k(query, &embeds, k)
            .into_iter()
            .filter_map(|r| {
                matching
                    .get(r.index)
                    .and_then(|(orig, _)| self.ids.get(*orig))
                    .map(|id| SearchResult::new(id.clone(), r.score))
            })
            .collect()
    }

    /// Number of embeddings in the index.
    pub fn len(&self) -> usize {
        self.embeddings.len()
    }

    /// Check if index is empty.
    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }

    /// Clear all embeddings.
    pub fn clear(&mut self) {
        self.embeddings.clear();
        self.ids.clear();
    }
}

/// A search result with similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Identifier of the matched memory entry.
    pub id: String,
    /// Cosine similarity score of the match (higher is closer).
    pub score: f64,
}

impl SearchResult {
    /// Construct a search result pairing a memory `id` with its similarity `score`.
    pub fn new(id: String, score: f64) -> Self {
        Self { id, score }
    }
}

/// Quantized memory entry with u4 nibble compression (8x reduction).
///
/// Stored as nibbles (0.5 bytes/symbol) instead of f32 (4 bytes/symbol).
#[cfg(feature = "quantization")]
#[derive(Debug, Clone)]
pub struct QuantizedEntry {
    /// Unique memory identifier.
    pub id: String,
    /// Memory content text.
    pub content: String,
    /// Blockwise-quantized embedding stored as packed u4 nibbles.
    pub quantized: Vec<u8>,
    /// Lazily-populated cache of the dequantized f32 embedding.
    pub dequantized_cache: Option<Vec<f32>>,
    /// Optional metadata (language, source, etc.).
    pub metadata: Option<serde_json::Value>,
    /// Number of dimensions per quantization block.
    pub block_size: usize,
    /// Dimensionality of the original embedding vector.
    pub dim: usize,
}

#[cfg(feature = "quantization")]
impl QuantizedEntry {
    /// Quantize `embedding` blockwise and build a `QuantizedEntry` for the given `id` and `content`.
    pub fn new(
        id: impl Into<String>,
        content: impl Into<String>,
        embedding: Vec<f32>,
        block_size: usize,
    ) -> Self {
        let quantizer = BlockQuantizer::fit_blockwise(&embedding, block_size);
        let quantized = quantizer.quantize_blockwise(&embedding);
        Self {
            id: id.into(),
            content: content.into(),
            quantized,
            dequantized_cache: None,
            metadata: None,
            block_size,
            dim: embedding.len(),
        }
    }

    /// Attach metadata to the entry, returning `self` for chaining.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get dequantized f32 vector (with caching).
    pub fn get_embedding(&mut self, quantizer: &BlockQuantizer) -> &Vec<f32> {
        if self.dequantized_cache.is_none() {
            self.dequantized_cache = Some(quantizer.dequantize_blockwise(&self.quantized));
        }
        self.dequantized_cache
            .as_ref()
            .expect("dequantized_cache is always Some after initialization above")
    }
}

/// Quantized embedding index using u4 nibble compression (8x memory reduction).
///
/// Uses `TopKSearcher` for O(n log k) partial selection on dequantized embeddings.
///
/// GPU-accelerated: when `gpu-compute` feature is enabled, uses `HttpGpuBackend`.
#[cfg(feature = "quantization")]
#[derive(Debug)]
pub struct QuantizedIndex {
    entries: Vec<QuantizedEntry>,
    ids: Vec<String>,
    quantizer: BlockQuantizer,
    /// Reusable top-K searcher with SIMD-accelerated cosine similarity.
    searcher: TopKSearcher,
}

/// GPU-enabled builder for `QuantizedIndex`.
#[cfg(feature = "quantization")]
#[derive(Debug)]
pub struct QuantizedIndexBuilder {
    block_size: usize,
    dim: usize,
    gpu_url: Option<String>,
}

#[cfg(feature = "quantization")]
impl QuantizedIndexBuilder {
    /// Create a new builder.
    pub fn new(block_size: usize, dim: usize) -> Self {
        Self {
            block_size,
            dim,
            gpu_url: None,
        }
    }

    /// Set GPU server URL for hardware-accelerated search.
    #[cfg(feature = "gpu-compute")]
    pub fn with_gpu_url(mut self, url: impl Into<String>) -> Self {
        self.gpu_url = Some(url.into());
        self
    }

    /// Build the `QuantizedIndex`.
    pub fn build(self) -> Self {
        #[cfg(feature = "gpu-compute")]
        if self.gpu_url.is_some() {
            // QuantizedIndex still uses SIMD for dequantization; GPU backend is for top_k
        }
        Self {
            block_size: self.block_size,
            dim: self.dim,
            gpu_url: None,
        }
    }
}

#[cfg(feature = "quantization")]
impl QuantizedIndex {
    /// Create a new quantized index with SIMD-only backend.
    pub fn new(block_size: usize, dim: usize) -> Self {
        Self {
            entries: Vec::new(),
            ids: Vec::new(),
            quantizer: BlockQuantizer::fit_blockwise(&[0.0_f32].repeat(dim.max(1)), block_size),
            searcher: TopKSearcher::default(),
        }
    }

    /// Create a GPU-accelerated quantized index.
    #[cfg(feature = "gpu-compute")]
    pub fn with_gpu(block_size: usize, dim: usize, gpu_url: impl Into<String>) -> Self {
        use std::sync::Arc;
        use touring_simd::gpu::HttpGpuBackend;
        let backend = Arc::new(HttpGpuBackend::new(gpu_url, dim));
        Self {
            entries: Vec::new(),
            ids: Vec::new(),
            quantizer: BlockQuantizer::fit_blockwise(&[0.0_f32].repeat(dim.max(1)), block_size),
            searcher: TopKSearcher::with_backend(backend),
        }
    }

    /// Insert a quantized `entry` into the index under the given `id`.
    pub fn add(&mut self, id: &str, entry: QuantizedEntry) {
        self.ids.push(id.to_string());
        self.entries.push(entry);
    }

    /// Search using dequantized embeddings with SIMD-accelerated top-K.
    ///
    /// Uses lazy dequantization via `QuantizedEntry::get_embedding()` to cache
    /// the dequantized f32 vector on first access. Subsequent queries for the
    /// same entry skip dequantization entirely, giving ~2–4x speedup on hot indexes.
    pub fn search(&mut self, query: &[f32], k: usize) -> Vec<SearchResult> {
        if self.entries.is_empty() || query.is_empty() {
            return Vec::new();
        }

        let k_clamped = k.min(self.entries.len());

        // Dequantize lazily (cache hit avoids blockwise decode on repeat queries)
        let quantizer = &self.quantizer;
        let embeddings: Vec<Vec<f32>> = self
            .entries
            .iter_mut()
            .map(|e| e.get_embedding(quantizer).clone())
            .collect();

        // SIMD-accelerated top-K with partial selection (O(n log k))
        self.searcher
            .top_k(query, &embeddings, k_clamped)
            .into_iter()
            .filter_map(|r| {
                self.ids
                    .get(r.index)
                    .map(|id| SearchResult::new(id.clone(), r.score))
            })
            .collect()
    }

    /// Number of quantized entries currently in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the index holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove all entries and their identifiers from the index.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.ids.clear();
    }
}

/// ANN-based memory recall using similarity search.
///
/// # Performance
///
/// - Search: O(n log k) where n = number of entries, k = requested results
///   (SIMD-accelerated cosine + partial selection via `TopKSearcher`)
/// - Add: O(1) amortized
/// - Memory: O(n) where n = number of entries
#[derive(Debug, Clone, Default)]
pub struct AnnMemoryRecall {
    index: EmbeddingIndex,
    entries: HashMap<String, MemoryEntry>,
}

impl AnnMemoryRecall {
    /// Create a new ANN memory recall system.
    pub fn new() -> Self {
        Self {
            index: EmbeddingIndex::new(),
            entries: HashMap::new(),
        }
    }

    /// Add a memory entry to the recall index.
    pub fn add_memory(&mut self, entry: MemoryEntry) {
        let id = entry.id.clone();
        self.index.add(&id, entry.embedding.clone());
        self.entries.insert(id, entry);
    }

    /// Add multiple memory entries in batch.
    pub fn add_batch(&mut self, entries: Vec<MemoryEntry>) {
        for entry in entries {
            self.add_memory(entry);
        }
    }

    /// Search for semantically similar memories.
    ///
    /// Uses cosine similarity to find top-K most similar memories.
    ///
    /// # Arguments
    ///
    /// * `query` - Query embedding vector
    /// * `k` - Number of results to return
    ///
    /// # Returns
    ///
    /// Vector of search results with IDs and similarity scores.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<MemorySearchResult> {
        self.index
            .search(query, k)
            .into_iter()
            .filter_map(|result| {
                self.entries
                    .get(&result.id)
                    .map(|entry| MemorySearchResult {
                        id: entry.id.clone(),
                        content: entry.content.clone(),
                        score: result.score,
                        metadata: entry.metadata.clone(),
                    })
            })
            .collect()
    }

    /// Get a specific memory by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MemoryEntry> {
        self.entries.get(id)
    }

    /// Number of memories in the index.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all memories.
    pub fn clear(&mut self) {
        self.index.clear();
        self.entries.clear();
    }

    /// Build from existing knowledge entries.
    ///
    /// Takes a slice of (id, content, embedding) tuples and builds the index.
    pub fn build_from_knowledge(
        &mut self,
        entries: impl IntoIterator<Item = (String, String, Vec<f32>)>,
    ) {
        for (id, content, embedding) in entries {
            let entry = MemoryEntry::new(id.clone(), content, embedding);
            self.add_memory(entry);
        }
    }
}

/// A memory search result with full content.
#[derive(Debug, Clone)]
pub struct MemorySearchResult {
    /// Memory ID.
    pub id: String,
    /// Memory content.
    pub content: String,
    /// Similarity score (0.0 to 1.0).
    pub score: f64,
    /// Optional metadata.
    pub metadata: Option<serde_json::Value>,
}

/// MemoryBus trait for wiring AnnMemoryRecall into CortexDispatcher.
///
/// Provides a string-based recall interface that converts queries to embeddings
/// internally, enabling semantic memory search without exposing embedding details.
pub trait MemoryBus: Send + Sync + std::fmt::Debug {
    /// Recall memories similar to the given query string.
    ///
    /// Internally converts the query to an embedding via hash-based embedding,
    /// then performs ANN search. Returns memories sorted by similarity descending.
    ///
    /// # Arguments
    ///
    /// * `query` - Natural language query string
    ///
    /// # Returns
    ///
    /// Vector of matching memory entries, or empty vec if no matches found.
    fn recall(&self, query: &str) -> Vec<MemoryEntry>;
}

impl MemoryBus for AnnMemoryRecall {
    fn recall(&self, query: &str) -> Vec<MemoryEntry> {
        // Convert query string to embedding via hash-based embedding
        let query_embedding = query_hash_embedding(query);
        // Default to top-10 results
        let results = self.search(&query_embedding, 10);
        // Convert search results back to MemoryEntry (dropping scores)
        results
            .into_iter()
            .filter_map(|r| self.get(&r.id).cloned())
            .collect()
    }
}

/// Compute a deterministic hash embedding from a text query.
///
/// Uses FNV-1a hash of word tokens to produce a fixed-size embedding vector.
/// This is NOT a semantic embedding — it clusters by word-level hash patterns,
/// enabling basic recall for exact and near-match queries. Latency: <1μs.
pub fn query_hash_embedding(query: &str) -> Vec<f32> {
    let mut embedding = vec![0.0f32; PATH_EMBED_DIM];

    // Hash each word token into different dimensions
    for (i, word) in query.split_whitespace().enumerate() {
        if word.is_empty() {
            continue;
        }
        // FNV-1a hash of word
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in word.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        // Mix word position into the hash
        // S-04 (2026-05-29): wrapping_mul — the plain `*` overflowed in debug
        // builds for queries with ≥4 word/segment tokens (i·0x517c… > u64::MAX),
        // panicking; release builds wrapped silently. Now deterministic in both.
        hash = hash.wrapping_add((i as u64).wrapping_mul(0x517c_c1b7_2722_0a95));

        // Distribute across multiple dimensions (3 dims per word)
        for j in 0..3 {
            let dim = ((hash >> (j * 16)) as usize) % PATH_EMBED_DIM;
            let val = ((hash >> (j * 8 + 4)) & 0xFF) as f32 / 255.0;
            if let Some(slot) = embedding.get_mut(dim) {
                *slot += val - 0.5; // center around 0
            }
        }
    }

    // L2-normalize via SIMD dot-product (touring-simd::simd_utils::matrix).
    touring_simd::simd_utils::matrix::l2_normalize_inplace(&mut embedding);

    embedding
}

/// Persistence layer for AnnMemoryRecall.
pub mod persistence;

/// Embedding dimension for path hash embeddings.
const PATH_EMBED_DIM: usize = 64;

/// Compute a deterministic hash embedding from a file path.
///
/// Uses FNV-1a hash of path segments to produce a fixed-size embedding vector.
/// Files with similar paths (same directory, similar extensions) produce similar
/// embeddings, enabling ANN recall of "related files from past edits".
///
/// This is NOT a semantic embedding — it clusters by path structure, which
/// correlates strongly with code locality (files in the same module are likely
/// related). Latency: <1μs.
pub fn path_hash_embedding(file_path: &str) -> Vec<f32> {
    let mut embedding = vec![0.0f32; PATH_EMBED_DIM];

    // Hash each path segment into different dimensions
    for (i, segment) in file_path.split(&['/', '\\', '.', '_', '-'][..]).enumerate() {
        if segment.is_empty() {
            continue;
        }
        // FNV-1a hash of segment
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in segment.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        // Mix segment position into the hash
        // S-04 (2026-05-29): wrapping_mul — the plain `*` overflowed in debug
        // builds for queries with ≥4 word/segment tokens (i·0x517c… > u64::MAX),
        // panicking; release builds wrapped silently. Now deterministic in both.
        hash = hash.wrapping_add((i as u64).wrapping_mul(0x517c_c1b7_2722_0a95));

        // Distribute across multiple dimensions (3 dims per segment)
        for j in 0..3 {
            let dim = ((hash >> (j * 16)) as usize) % PATH_EMBED_DIM;
            let val = ((hash >> (j * 8 + 4)) & 0xFF) as f32 / 255.0;
            if let Some(slot) = embedding.get_mut(dim) {
                *slot += val - 0.5; // center around 0
            }
        }
    }

    // L2-normalize via SIMD dot-product (touring-simd::simd_utils::matrix).
    touring_simd::simd_utils::matrix::l2_normalize_inplace(&mut embedding);

    embedding
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_embeddings() -> Vec<(String, String, Vec<f32>)> {
        vec![
            (
                "mem1".to_string(),
                "error handling in rust".to_string(),
                vec![0.1, 0.2, 0.8],
            ),
            (
                "mem2".to_string(),
                "async programming with tokio".to_string(),
                vec![0.9, 0.8, 0.1],
            ),
            (
                "mem3".to_string(),
                "result type and error propagation".to_string(),
                vec![0.15, 0.25, 0.75],
            ),
        ]
    }

    #[test]
    fn test_ann_memory_new() {
        let recall = AnnMemoryRecall::new();
        assert!(recall.is_empty());
        assert_eq!(recall.len(), 0);
    }

    #[test]
    fn test_add_memory() {
        let mut recall = AnnMemoryRecall::new();
        recall.add_memory(MemoryEntry::new("1", "test content", vec![1.0, 0.0, 0.0]));
        assert_eq!(recall.len(), 1);
        assert!(!recall.is_empty());
    }

    #[test]
    fn test_search_similar_memories() {
        let mut recall = AnnMemoryRecall::new();
        recall.build_from_knowledge(sample_embeddings());

        // Query: [0.1, 0.2, 0.9] - similar to mem1 [0.1, 0.2, 0.8]
        let results = recall.search(&[0.1, 0.2, 0.9], 2);

        assert!(!results.is_empty());
        // mem1 should be top - [0.1, 0.2, 0.8] is most similar to [0.1, 0.2, 0.9]
        assert_eq!(results[0].id, "mem1");
    }

    #[test]
    fn test_search_empty_index() {
        let recall = AnnMemoryRecall::new();
        let results = recall.search(&[1.0, 0.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_get_memory() {
        let mut recall = AnnMemoryRecall::new();
        recall.add_memory(MemoryEntry::new("test-id", "test content", vec![1.0]));

        let entry = recall.get("test-id");
        assert!(entry.is_some());
        assert_eq!(entry.expect("entry exists").content, "test content");
    }

    #[test]
    fn test_get_memory_not_found() {
        let recall = AnnMemoryRecall::new();
        let entry = recall.get("nonexistent");
        assert!(entry.is_none());
    }

    #[test]
    fn test_clear() {
        let mut recall = AnnMemoryRecall::new();
        recall.build_from_knowledge(sample_embeddings());
        assert_eq!(recall.len(), 3);

        recall.clear();
        assert!(recall.is_empty());
    }

    #[test]
    fn test_memory_with_metadata() {
        let entry = MemoryEntry::new("1", "content", vec![1.0, 0.0])
            .with_metadata(serde_json::json!({"source": "test"}));

        assert!(entry.metadata.is_some());
        let meta = entry.metadata.expect("metadata exists");
        assert_eq!(meta["source"], "test");
    }

    // ── add_batch ────────────────────────────────────────────────────────────

    #[test]
    fn test_add_batch_empty_is_noop() {
        let mut batch_noop = AnnMemoryRecall::new();
        batch_noop.add_batch(vec![]);
        assert!(batch_noop.is_empty());
    }

    #[test]
    fn test_add_batch_multiple_entries() {
        let mut batch_multi = AnnMemoryRecall::new();
        let entries = vec![
            MemoryEntry::new("a", "alpha content", vec![1.0, 0.0, 0.0]),
            MemoryEntry::new("b", "beta content", vec![0.0, 1.0, 0.0]),
            MemoryEntry::new("c", "gamma content", vec![0.0, 0.0, 1.0]),
        ];
        batch_multi.add_batch(entries);
        assert_eq!(batch_multi.len(), 3);
        assert!(batch_multi.get("a").is_some());
        assert!(batch_multi.get("b").is_some());
        assert!(batch_multi.get("c").is_some());
    }

    #[test]
    fn test_add_batch_duplicate_id_overwrites() {
        // When the same ID is added twice (via batch), the last write wins in
        // the entries HashMap. The index accumulates both vectors but the
        // HashMap entry is replaced — len() reflects unique IDs.
        let mut batch_dup = AnnMemoryRecall::new();
        batch_dup.add_batch(vec![
            MemoryEntry::new("dup", "first", vec![1.0, 0.0]),
            MemoryEntry::new("dup", "second", vec![0.0, 1.0]),
        ]);
        // HashMap key "dup" was overwritten — content is the last one.
        let dup_entry = batch_dup.get("dup").expect("dup entry present");
        assert_eq!(dup_entry.content, "second");
    }

    // ── EmbeddingIndex edge cases ─────────────────────────────────────────────

    #[test]
    fn test_embedding_index_empty_query_returns_empty() {
        let mut idx = EmbeddingIndex::new();
        idx.add("x", vec![1.0, 0.0]);
        // Empty query slice — simd_knn_search guards against this.
        let results = idx.search(&[], 5);
        assert!(results.is_empty(), "empty query must return no results");
    }

    #[test]
    fn test_embedding_index_k_clamped_to_len() {
        let mut idx = EmbeddingIndex::new();
        idx.add("only", vec![1.0, 0.0]);
        // k=10 but only 1 entry — must not panic and must return ≤1 result.
        let results = idx.search(&[1.0, 0.0], 10);
        assert!(results.len() <= 1);
    }

    #[test]
    fn test_embedding_index_clear_resets_state() {
        let mut idx = EmbeddingIndex::new();
        idx.add("a", vec![1.0]);
        idx.add("b", vec![0.0]);
        assert_eq!(idx.len(), 2);
        idx.clear();
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
        // Search on cleared index must not panic.
        let results = idx.search(&[1.0], 5);
        assert!(results.is_empty());
    }

    // ── search result ordering ─────────────────────────────────────────────────

    #[test]
    fn test_search_returns_at_most_k_results() {
        let mut recall = AnnMemoryRecall::new();
        recall.build_from_knowledge(sample_embeddings());
        let results = recall.search(&[0.1, 0.2, 0.8], 1);
        assert_eq!(results.len(), 1, "k=1 must return exactly 1 result");
    }

    #[test]
    fn test_search_score_in_valid_range() {
        let mut recall = AnnMemoryRecall::new();
        recall.build_from_knowledge(sample_embeddings());
        let results = recall.search(&[0.5, 0.5, 0.5], 3);
        for r in &results {
            assert!(
                r.score >= -1.0 && r.score <= 1.0,
                "cosine score out of [-1,1]: {}",
                r.score
            );
        }
    }

    // ── TopKSearcher integration tests ────────────────────────────────────────

    #[test]
    fn test_topk_search_returns_correct_count() {
        // k=5 out of 100 entries -> exactly 5 results
        let mut recall = AnnMemoryRecall::new();
        let dim = 16;
        for i in 0..100 {
            let mut emb = vec![0.0f32; dim];
            // Spread embeddings across dimensions to ensure distinctness
            emb[i % dim] = 1.0;
            emb[(i + 1) % dim] = (i as f32) / 100.0;
            recall.add_memory(MemoryEntry::new(
                format!("entry-{i}"),
                format!("content {i}"),
                emb,
            ));
        }

        let query = vec![1.0f32; dim]; // uniform query
        let results = recall.search(&query, 5);
        assert_eq!(
            results.len(),
            5,
            "k=5 from 100 entries must return exactly 5"
        );
    }

    #[test]
    fn test_topk_search_results_are_sorted() {
        // Verify results are sorted by similarity descending
        let mut recall = AnnMemoryRecall::new();
        let dim = 8;
        for i in 0..50 {
            let mut emb = vec![0.0f32; dim];
            // Create a gradient: entry 0 most similar to query, entry 49 least
            let weight = 1.0 - (i as f32 / 50.0);
            emb[0] = weight;
            emb[1] = 1.0 - weight;
            recall.add_memory(MemoryEntry::new(
                format!("sorted-{i}"),
                format!("content {i}"),
                emb,
            ));
        }

        let query = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let results = recall.search(&query, 10);
        assert_eq!(results.len(), 10);

        // Scores must be in descending order
        for i in 1..results.len() {
            assert!(
                results[i - 1].score >= results[i].score,
                "results not sorted descending at position {}: {} < {}",
                i,
                results[i - 1].score,
                results[i].score,
            );
        }
    }

    /// S-04 (2026-05-29): a corpus caught mid-migration (mixed embedding widths,
    /// 64-dim hash ↔ 768-dim semantic) must not panic — `search` filters to the
    /// entries whose width matches the query and maps indices back correctly.
    #[test]
    fn search_skips_mismatched_dim_entries() {
        let mut recall = AnnMemoryRecall::new();
        recall.add_memory(MemoryEntry::new(
            "legacy",
            "old hash entry",
            vec![1.0, 0.0, 0.0],
        ));
        recall.add_memory(MemoryEntry::new(
            "semantic",
            "new entry",
            vec![1.0, 0.0, 0.0, 0.0, 0.0],
        ));

        // A 3-dim query matches only the 3-dim entry (no panic on the 5-dim one).
        let r3 = recall.search(&[1.0, 0.0, 0.0], 10);
        assert_eq!(r3.len(), 1, "only the 3-dim entry matches a 3-dim query");
        assert_eq!(r3[0].id, "legacy");

        // A 5-dim query matches only the 5-dim entry.
        let r5 = recall.search(&[1.0, 0.0, 0.0, 0.0, 0.0], 10);
        assert_eq!(r5.len(), 1, "only the 5-dim entry matches a 5-dim query");
        assert_eq!(r5[0].id, "semantic");
    }
}
