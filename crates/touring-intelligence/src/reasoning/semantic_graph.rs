//! Semantic memory graph backed by petgraph StableGraph.
//!
//! Uses StableGraph (not DiGraph) so that NodeIndex values remain valid
//! after node removal — critical for the DashMap<String, NodeIndex> cache.
//!
//! Lock ordering: SemanticGraph.graph (RwLock, L1) — held while traversing.

use crate::reasoning::persistence::GraphPersistence;
use dashmap::DashMap;
use petgraph::Direction;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use touring_simd::{TopKResult, TopKSearcher};

/// Calibrated monotonic epoch clock.
///
/// `SystemTime::now()` is NOT monotonic — it can jump forward or backward due
/// to NTP adjustments, system sleep/resume, or manual changes.  This is wrong
/// for duration / age calculations.
///
/// We calibrate once at startup: capture both `SystemTime::now()` and the
/// corresponding `Instant::now()` together in a single atomic init, then store
/// the epoch-ms that matches that instant.  Subsequent calls use only
/// `Instant` (guaranteed monotonic by the Rust stdlib) and add elapsed ms to
/// the fixed baseline.  This means we never read `SystemTime` after startup,
/// so NTP jumps and sleep/resume cannot affect the result.
///
/// Thread-safe: both values are set exactly once via `OnceLock`.
fn monotonic_epoch_ms() -> u64 {
    static CALIBRATED: std::sync::OnceLock<(u64, Instant)> = std::sync::OnceLock::new();

    let &(offset_ms, cal) = CALIBRATED.get_or_init(|| {
        let sys_now = SystemTime::now();
        let ins_now = Instant::now();
        let offset = sys_now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        (offset, ins_now)
    });

    let elapsed = cal.elapsed().as_millis() as u64;
    offset_ms.saturating_add(elapsed)
}

/// Returns the current epoch time as `f64` seconds (monotonic).
///
/// Replaces the previous `SystemTime::now().duration_since(UNIX_EPOCH)` pattern
/// which is not monotonic.  The returned value is guaranteed not to decrease
/// across calls within a single process run.
pub fn now_epoch_secs() -> f64 {
    monotonic_epoch_ms() as f64 / 1000.0
}

/// Base temporal decay half-life in seconds (7 days, aligned with touring-hooks).
/// COG-3: Used as the base for adaptive decay — frequently accessed nodes decay slower.
const BASE_DECAY_HALF_LIFE_SECS: f64 = 7.0 * 24.0 * 3600.0;

/// Type of memory node in the semantic graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum NodeType {
    /// A code symbol (function, type, etc.).
    Symbol,
    /// A source file.
    File,
    /// An abstract concept.
    Concept,
    /// A work/interaction session.
    Session,
}

impl std::fmt::Display for NodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symbol => f.write_str("symbol"),
            Self::File => f.write_str("file"),
            Self::Concept => f.write_str("concept"),
            Self::Session => f.write_str("session"),
        }
    }
}

impl NodeType {
    /// Returns all possible node types (for iteration/enumeration).
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[Self::Symbol, Self::File, Self::Concept, Self::Session]
    }
}

/// Type of directed edge in the semantic graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EdgeType {
    /// Source node references the target node.
    References,
    /// Source node contains the target node.
    Contains,
    /// Source and target are generically related.
    Related,
    /// Target follows the source in a temporal/ordered sequence.
    Sequence,
    /// Import relationship (from hooks file_relations).
    Imports,
    /// Co-edit relationship (frequently edited together).
    CoEdit,
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::References => f.write_str("references"),
            Self::Contains => f.write_str("contains"),
            Self::Related => f.write_str("related"),
            Self::Sequence => f.write_str("sequence"),
            Self::Imports => f.write_str("imports"),
            Self::CoEdit => f.write_str("co_edit"),
        }
    }
}

impl EdgeType {
    /// Returns all possible edge types (for iteration/enumeration).
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::References,
            Self::Contains,
            Self::Related,
            Self::Sequence,
            Self::Imports,
            Self::CoEdit,
        ]
    }

    /// Returns the default weight for this edge type.
    ///
    /// Higher weight = stronger relationship signal for graph traversal.
    #[must_use]
    pub fn default_weight(&self) -> f32 {
        match self {
            Self::References => 0.8,
            Self::Contains => 1.0,
            Self::Related => 0.5,
            Self::Sequence => 0.3,
            Self::Imports => 0.9,
            Self::CoEdit => 0.7,
        }
    }
}

/// A node in the semantic memory graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryNode {
    /// Unique identifier of the node.
    pub id: String,
    /// Human-readable label for the node.
    pub label: String,
    /// Category of the node (`NodeType`).
    pub node_type: NodeType,
    /// Optional embedding vector for similarity retrieval.
    pub embedding: Vec<f32>,
    /// Arbitrary JSON metadata attached to the node.
    pub metadata: serde_json::Value,
    /// Timestamp when this node was last accessed (epoch seconds).
    #[serde(default)]
    pub last_accessed: f64,
    /// Number of times this node has been accessed.
    #[serde(default)]
    pub access_count: u64,
}

impl MemoryNode {
    /// Create a new node with minimal fields (no embedding, empty metadata).
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>, node_type: NodeType) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            node_type,
            embedding: Vec::new(),
            metadata: serde_json::Value::Null,
            last_accessed: 0.0,
            access_count: 0,
        }
    }

    /// Returns `true` if this node has a non-empty embedding vector.
    #[must_use]
    pub fn has_embedding(&self) -> bool {
        !self.embedding.is_empty()
    }

    /// Returns the embedding dimensionality (0 if no embedding).
    #[must_use]
    pub fn embedding_dim(&self) -> usize {
        self.embedding.len()
    }

    /// Compute the temporal relevance score (0.0 to 1.0) based on
    /// time decay and access frequency.
    ///
    /// Uses exponential decay with the configured half-life, boosted
    /// by access count (log scale).
    /// COG-3: Compute adaptive half-life based on access frequency.
    ///
    /// Frequently accessed nodes decay slower: `base * ln(access_count + 1).max(1.0)`.
    /// A node with 0 accesses uses the base half-life (7 days).
    /// A node with 100 accesses gets ~4.6x longer half-life (~32 days).
    #[must_use]
    pub fn adaptive_half_life(&self) -> f64 {
        BASE_DECAY_HALF_LIFE_SECS * (1.0 + self.access_count as f64).ln().max(1.0)
    }

    /// Temporal relevance score (0.0 to 1.0) combining adaptive decay since
    /// `last_accessed` with a logarithmic access-frequency boost.
    #[must_use]
    pub fn relevance_score(&self, now_epoch: f64) -> f64 {
        let age_secs = (now_epoch - self.last_accessed).max(0.0);
        let half_life = self.adaptive_half_life();
        let decay = (-age_secs * std::f64::consts::LN_2 / half_life).exp();
        let frequency_boost = (1.0 + self.access_count as f64).ln() / 5.0;
        (decay + frequency_boost).min(1.0)
    }
}

impl std::fmt::Display for MemoryNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({}: {})", self.node_type, self.id, self.label)
    }
}

/// A directed edge between memory nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticEdge {
    /// Id of the source node.
    pub from_id: String,
    /// Id of the target node.
    pub to_id: String,
    /// Category of the edge (`EdgeType`).
    pub edge_type: EdgeType,
    /// Edge weight/strength.
    pub weight: f32,
    /// Timestamp when this edge was created/updated (epoch seconds).
    #[serde(default)]
    pub created_at: f64,
}

impl SemanticEdge {
    /// Create a new edge with the default weight for its type.
    #[must_use]
    pub fn new(from_id: impl Into<String>, to_id: impl Into<String>, edge_type: EdgeType) -> Self {
        let weight = edge_type.default_weight();
        Self {
            from_id: from_id.into(),
            to_id: to_id.into(),
            edge_type,
            weight,
            created_at: 0.0,
        }
    }

    /// Create a new edge with a custom weight.
    #[must_use]
    pub fn with_weight(
        from_id: impl Into<String>,
        to_id: impl Into<String>,
        edge_type: EdgeType,
        weight: f32,
    ) -> Self {
        Self {
            from_id: from_id.into(),
            to_id: to_id.into(),
            edge_type,
            weight,
            created_at: 0.0,
        }
    }
}

impl std::fmt::Display for SemanticEdge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} --[{}:{:.2}]--> {}",
            self.from_id, self.edge_type, self.weight, self.to_id
        )
    }
}

/// Semantic graph for associative memory retrieval.
///
/// Uses petgraph **StableGraph** for traversal + DashMap for O(1) id→index lookup.
/// StableGraph preserves NodeIndex stability after removal (unlike DiGraph),
/// which is critical because `node_index: DashMap` caches NodeIndex externally.
///
/// # Why this is NOT a moka cache (2026-04-16)
///
/// Unlike the W-TinyLFU caches in `touring-hooks::shared::moka_policies`,
/// `node_index` is a **storage index**, not a cache:
/// - Eviction by TTL or capacity would break `add_edge("from", "to")` when
///   either endpoint got silently evicted between lookup and edge insertion.
/// - The map's size is bounded by the graph itself (nodes == entries), so
///   capacity governance would add zero benefit.
/// - TOCTOU guards depend on `node_index` and the graph's L1 write lock
///   mutating in lock-step; an eviction policy outside the lock breaks the
///   invariant.
///
/// Keep this `DashMap` as-is; see also `touring-hooks::shared::job_registry`
/// for an analogous case (live `JoinHandle` lifecycle).
#[derive(Debug)]
pub struct SemanticGraph {
    /// L1 lock — acquire before graph mutation or traversal.
    graph: RwLock<StableGraph<MemoryNode, SemanticEdge>>,
    /// Lock-free concurrent index map — excluded from lock ordering.
    node_index: DashMap<String, NodeIndex>,
    _persistence: Arc<GraphPersistence>,
    /// ANN index for fast approximate nearest neighbor embedding search.
    /// Lazily built via `rebuild_ann_index()` when embeddings are available.
    ann_index: std::sync::Mutex<Option<crate::reasoning::ann_index::AnnIndex>>,
}

/// Error returned by [`SemanticGraph`] mutation methods (`add_node`, `add_edge`,
/// `remove_node`). Replaces the previous stringly-typed `Result<_, String>`;
/// `Display` is preserved byte-for-byte via the message carried in each variant.
#[derive(Debug, thiserror::Error)]
pub enum SemanticGraphError {
    /// The internal L1 `RwLock` was poisoned by a panicking writer.
    #[error("{0}")]
    PoisonedLock(String),
    /// Invalid edge request — a self-loop, or an endpoint node that does not exist.
    #[error("{0}")]
    Validation(String),
}

impl SemanticGraph {
    /// Create a new empty SemanticGraph.
    pub fn new(persistence: Arc<GraphPersistence>) -> Self {
        Self {
            graph: RwLock::new(StableGraph::new()),
            node_index: DashMap::new(),
            _persistence: persistence,
            ann_index: std::sync::Mutex::new(None),
        }
    }

    /// Add a node to the graph. Acquires L1 write lock.
    /// If a node with the same id already exists, updates it in place (upsert).
    ///
    /// # Concurrency safety
    /// Acquires L1 write lock FIRST, then checks/updates DashMap — eliminates
    /// TOCTOU race between "check DashMap" and "modify graph".
    pub fn add_node(&self, node: MemoryNode) -> Result<(), SemanticGraphError> {
        let id = node.id.clone();
        let mut g = self
            .graph
            .write()
            .map_err(|e| SemanticGraphError::PoisonedLock(format!("lock poisoned: {e}")))?;

        // Check DashMap under the write lock to prevent TOCTOU
        if let Some(existing_idx) = self.node_index.get(&id) {
            if let Some(existing_node) = g.node_weight_mut(*existing_idx) {
                existing_node.label = node.label;
                existing_node.node_type = node.node_type;
                if !node.embedding.is_empty() {
                    existing_node.embedding = node.embedding;
                }
                existing_node.metadata = node.metadata;
                existing_node.access_count += 1;
                existing_node.last_accessed = now_epoch_secs();
            }
            return Ok(());
        }

        let idx = g.add_node(node);
        self.node_index.insert(id, idx);
        Ok(())
    }

    /// Add a directed edge between two existing nodes.
    ///
    /// # Concurrency safety
    /// StableGraph guarantees index stability after removal, so indices
    /// read from DashMap remain valid even if other nodes are removed.
    /// Self-loops are rejected (returns Err).
    pub fn add_edge(&self, from: &str, to: &str, weight: f32) -> Result<(), SemanticGraphError> {
        if from == to {
            return Err(SemanticGraphError::Validation(format!(
                "self-loop not permitted: {from}"
            )));
        }
        let from_idx = self
            .node_index
            .get(from)
            .ok_or_else(|| SemanticGraphError::Validation(format!("node not found: {from}")))?;
        let to_idx = self
            .node_index
            .get(to)
            .ok_or_else(|| SemanticGraphError::Validation(format!("node not found: {to}")))?;
        let edge = SemanticEdge {
            from_id: from.to_string(),
            to_id: to.to_string(),
            edge_type: EdgeType::Related,
            weight,
            created_at: now_epoch_secs(),
        };
        let mut g = self
            .graph
            .write()
            .map_err(|e| SemanticGraphError::PoisonedLock(format!("lock poisoned: {e}")))?;
        g.add_edge(*from_idx, *to_idx, edge);
        Ok(())
    }

    /// Add a typed edge between two existing nodes.
    pub fn add_typed_edge(
        &self,
        from: &str,
        to: &str,
        edge_type: EdgeType,
        weight: f32,
    ) -> Result<(), SemanticGraphError> {
        if from == to {
            return Err(SemanticGraphError::Validation(format!(
                "self-loop not permitted: {from}"
            )));
        }
        let from_idx = self
            .node_index
            .get(from)
            .ok_or_else(|| SemanticGraphError::Validation(format!("node not found: {from}")))?;
        let to_idx = self
            .node_index
            .get(to)
            .ok_or_else(|| SemanticGraphError::Validation(format!("node not found: {to}")))?;
        let edge = SemanticEdge {
            from_id: from.to_string(),
            to_id: to.to_string(),
            edge_type,
            weight,
            created_at: now_epoch_secs(),
        };
        let mut g = self
            .graph
            .write()
            .map_err(|e| SemanticGraphError::PoisonedLock(format!("lock poisoned: {e}")))?;
        g.add_edge(*from_idx, *to_idx, edge);
        Ok(())
    }

    /// Remove a node and all its edges. Safe with StableGraph — other
    /// NodeIndex values remain valid after this operation.
    ///
    /// Returns the removed node, or None if the node was not found.
    ///
    /// # Concurrency safety
    /// Acquires L1 write lock FIRST, then removes from DashMap — eliminates
    /// TOCTOU race where another thread could re-insert the same ID between
    /// DashMap removal and graph removal.
    pub fn remove_node(&self, node_id: &str) -> Result<Option<MemoryNode>, SemanticGraphError> {
        // Acquire L1 write lock FIRST to prevent TOCTOU
        let mut g = self
            .graph
            .write()
            .map_err(|e| SemanticGraphError::PoisonedLock(format!("lock poisoned: {e}")))?;

        let idx = match self.node_index.get(node_id) {
            Some(i) => *i,
            None => return Ok(None),
        };
        // Remove from graph under L1 lock
        let removed = g.remove_node(idx);
        // Drop the DashMap entry only after graph removal succeeds
        self.node_index.remove(node_id);
        Ok(removed)
    }

    /// Retrieve top-k nodes by cosine similarity to the query embedding.
    /// Returns empty vec if graph is empty or embedding dimensions mismatch.
    ///
    /// Uses `TopKSearcher` for SIMD-accelerated O(n log k) retrieval.
    pub fn retrieve_by_embedding(&self, query: &[f32], k: usize) -> Vec<MemoryNode> {
        let g = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        if g.node_count() == 0 || query.is_empty() {
            return vec![];
        }

        // Build parallel vectors for TopK search
        let k = k.max(1);
        let candidates: Vec<MemoryNode> = g
            .node_weights()
            .filter(|n| !n.embedding.is_empty() && n.embedding.len() == query.len())
            .cloned()
            .collect();

        if candidates.is_empty() {
            return vec![];
        }

        let embeddings: Vec<Vec<f32>> = candidates.iter().map(|n| n.embedding.clone()).collect();
        let searcher = TopKSearcher::new(k);

        searcher
            .top_k(query, &embeddings, k)
            .into_iter()
            .filter_map(|TopKResult { index, score }| {
                candidates.get(index).map(|n| (score, n.clone()))
            })
            .map(|(_score, node)| node)
            .collect()
    }

    /// Rebuild the ANN index from all nodes with embeddings.
    ///
    /// Call this after bulk-inserting nodes to enable fast `retrieve_by_embedding_ann()`.
    /// The ANN index provides O(N/K * P) retrieval vs O(N) linear scan.
    pub fn rebuild_ann_index(&self) {
        let g = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return,
        };

        let entries: Vec<(String, Vec<f32>)> = g
            .node_weights()
            .filter(|n| !n.embedding.is_empty())
            .map(|n| (n.id.clone(), n.embedding.clone()))
            .collect();

        if entries.is_empty() {
            return;
        }

        let dim = match entries.first() {
            Some((_, emb)) => emb.len(),
            None => return, // unreachable due to is_empty check above
        };
        let mut index = crate::reasoning::ann_index::AnnIndex::new(dim);
        index.build(&entries);

        if let Ok(mut ann) = self.ann_index.lock() {
            *ann = Some(index);
        }
    }

    /// Fast approximate retrieval using ANN index (IVF-flat).
    ///
    /// Falls back to exact `retrieve_by_embedding()` if ANN index is not built.
    /// Call `rebuild_ann_index()` first for O(N/K * P) retrieval.
    pub fn retrieve_by_embedding_ann(&self, query: &[f32], k: usize) -> Vec<MemoryNode> {
        // Try ANN index first
        if let Ok(ann_guard) = self.ann_index.lock() {
            if let Some(ann) = ann_guard.as_ref() {
                let ann_results = ann.query(query, k);
                if !ann_results.is_empty() {
                    // Map IDs back to MemoryNodes
                    let g = match self.graph.read() {
                        Ok(g) => g,
                        Err(_) => return vec![],
                    };
                    return ann_results
                        .into_iter()
                        .filter_map(|(id, _score)| {
                            self.node_index
                                .get(&id)
                                .and_then(|idx| g.node_weight(*idx).cloned())
                        })
                        .collect();
                }
            }
        }

        // Fallback: exact linear scan
        self.retrieve_by_embedding(query, k)
    }

    /// Retrieve top-k nodes using attention-weighted scoring.
    ///
    /// Score = cosine_similarity × recency_factor × frequency_factor
    /// - recency_factor: exponential decay with 7-day half-life
    /// - frequency_factor: log2(2 + access_count) — minimum 1.0 so new nodes are not invisible
    ///
    /// This weights recent, frequently-accessed nodes higher.
    ///
    /// Uses two-phase retrieval:
    /// 1. TopKSearcher for SIMD-accelerated cosine similarity (O(n log k))
    /// 2. Attention re-scoring on top candidates for final ranking
    pub fn retrieve_attention_weighted(&self, query: &[f32], k: usize) -> Vec<MemoryNode> {
        let g = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        if g.node_count() == 0 || query.is_empty() {
            return vec![];
        }

        // Phase 1: Get top-k*2 candidates by cosine similarity (oversample for attention rerank)
        let oversample = k.saturating_mul(3).max(10);
        let candidates: Vec<MemoryNode> = g
            .node_weights()
            .filter(|n| !n.embedding.is_empty() && n.embedding.len() == query.len())
            .cloned()
            .collect();

        if candidates.is_empty() {
            return vec![];
        }

        let embeddings: Vec<Vec<f32>> = candidates.iter().map(|n| n.embedding.clone()).collect();
        let searcher = TopKSearcher::new(oversample);
        let now = now_epoch_secs();

        // Phase 1 + 2 merged: TopKSearcher candidate selection + attention re-scoring in single pass
        let mut rescored: Vec<(f32, MemoryNode)> = searcher
            .top_k(query, &embeddings, oversample)
            .into_iter()
            .filter_map(|TopKResult { index, score }| {
                let n = candidates.get(index)?;
                let recency = temporal_decay(n.last_accessed, now);
                let frequency = (2.0 + n.access_count as f64).log2() as f32;
                let attention_score = (score as f32) * recency * frequency;
                Some((attention_score, n.clone()))
            })
            .collect();

        rescored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        rescored.into_iter().take(k).map(|(_, n)| n).collect()
    }

    /// Return all edges originating from a node id.
    pub fn edges_from(&self, node_id: &str) -> Vec<SemanticEdge> {
        let idx = match self.node_index.get(node_id) {
            Some(i) => *i,
            None => return vec![],
        };
        let g = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        g.edges_directed(idx, Direction::Outgoing)
            .map(|e| e.weight().clone())
            .collect()
    }

    /// Return all edges targeting a node id (incoming).
    pub fn edges_to(&self, node_id: &str) -> Vec<SemanticEdge> {
        let idx = match self.node_index.get(node_id) {
            Some(i) => *i,
            None => return vec![],
        };
        let g = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        g.edges_directed(idx, Direction::Incoming)
            .map(|e| e.weight().clone())
            .collect()
    }

    /// Return all neighbor node IDs (outgoing direction).
    pub fn neighbors(&self, node_id: &str) -> Vec<String> {
        let idx = match self.node_index.get(node_id) {
            Some(i) => *i,
            None => return vec![],
        };
        let g = match self.graph.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        g.neighbors(idx)
            .filter_map(|ni| g.node_weight(ni).map(|n| n.id.clone()))
            .collect()
    }

    /// Apply temporal decay to all edges, removing those below threshold.
    ///
    /// Edges whose decayed weight drops below `min_weight` are removed.
    /// Returns the number of edges removed.
    pub fn decay_edges(&self, min_weight: f32) -> usize {
        let mut g = match self.graph.write() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        let now = now_epoch_secs();
        let edge_indices: Vec<_> = g.edge_indices().collect();
        let mut removed = 0;

        for edge_idx in edge_indices {
            if let Some(edge) = g.edge_weight(edge_idx) {
                let decayed = edge.weight * temporal_decay(edge.created_at, now);
                if decayed < min_weight {
                    g.remove_edge(edge_idx);
                    removed += 1;
                }
            }
        }
        removed
    }

    /// Touch a node (update access count + timestamp). Returns false if not found.
    pub fn touch(&self, node_id: &str) -> bool {
        let idx = match self.node_index.get(node_id) {
            Some(i) => *i,
            None => return false,
        };
        let mut g = match self.graph.write() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if let Some(node) = g.node_weight_mut(idx) {
            node.access_count += 1;
            node.last_accessed = now_epoch_secs();
            true
        } else {
            false
        }
    }

    /// Prune stale nodes that have not been accessed within `max_age`, keeping
    /// at most `max_nodes` in the graph. Nodes are evicted by staleness first
    /// (oldest `last_accessed`), then by count if the graph still exceeds `max_nodes`.
    ///
    /// Returns the number of nodes removed.
    ///
    /// Recommended defaults: `max_age = Duration::from_secs(3600)` (1 hour),
    /// `max_nodes = 10_000`.
    pub fn prune_stale_nodes(&self, max_age: std::time::Duration, max_nodes: usize) -> usize {
        let now = now_epoch_secs();
        let cutoff = now - max_age.as_secs_f64();
        let mut removed = 0;

        // Phase 1: Remove nodes not accessed within max_age.
        let stale_ids: Vec<String> = {
            let g = match self.graph.read() {
                Ok(g) => g,
                Err(_) => return 0,
            };
            g.node_weights()
                .filter(|n| n.last_accessed > 0.0 && n.last_accessed < cutoff)
                .map(|n| n.id.clone())
                .collect()
        };

        for id in &stale_ids {
            if self.remove_node(id).is_ok() {
                removed += 1;
            }
        }

        // Phase 2: If still over max_nodes, evict least-recently-accessed nodes.
        let current_count = self.node_count();
        if current_count > max_nodes {
            let excess = current_count - max_nodes;
            let mut candidates: Vec<(String, f64)> = {
                let g = match self.graph.read() {
                    Ok(g) => g,
                    Err(_) => return removed,
                };
                g.node_weights()
                    .map(|n| (n.id.clone(), n.last_accessed))
                    .collect()
            };
            // Sort by last_accessed ascending (oldest first).
            candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            for (id, _) in candidates.into_iter().take(excess) {
                if self.remove_node(&id).is_ok() {
                    removed += 1;
                }
            }
        }

        if removed > 0 {
            tracing::info!(
                removed,
                remaining = self.node_count(),
                "pruned stale nodes from semantic graph"
            );
        }

        removed
    }

    /// S3: Warm the retrieval cache by touching top-accessed nodes.
    ///
    /// Pre-computes neighbor lists for the most frequently accessed nodes,
    /// bringing their graph data into CPU cache. Also refreshes the
    /// `last_accessed` timestamp so decay scoring stays fresh.
    ///
    /// `hint` is an optional node ID to prioritize — if it exists, its
    /// neighbors are touched first.
    ///
    /// NOTE (ARCH-2): Current implementation touches existing nodes and fetches
    /// their neighbors to warm CPU cache. A full implementation would pre-compute
    /// and cache the actual neighbor subgraphs for top-accessed nodes, rather
    /// than just triggering individual lookups. The predictor_task.rs calls this
    /// every 500ms — the current implementation brings data into cache but does
    /// not perform expensive pre-computation.
    pub fn warm_cache(&self, hint: &str) {
        crate::reasoning::metrics::CognitiveMetrics::inc(
            &crate::reasoning::metrics::CognitiveMetrics::global().warm_cache_calls,
        );

        // Touch the hint node if it exists
        if !hint.is_empty() {
            self.touch(hint);
            // Pre-fetch neighbors to warm CPU cache
            let _ = self.neighbors(hint);
        }

        // Collect top-accessed nodes and touch them
        let top_nodes: Vec<String> = {
            let g = match self.graph.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            let mut nodes: Vec<(String, u64)> = g
                .node_weights()
                .map(|n| (n.id.clone(), n.access_count))
                .collect();
            nodes.sort_by_key(|b| std::cmp::Reverse(b.1));
            nodes.into_iter().take(16).map(|(id, _)| id).collect()
        };

        for node_id in &top_nodes {
            // Touch refreshes last_accessed; neighbors() warms the adjacency data
            self.touch(node_id);
            let _ = self.neighbors(node_id);
        }

        if !top_nodes.is_empty() {
            tracing::debug!(
                warmed = top_nodes.len(),
                hint = hint,
                "warmed semantic graph cache"
            );
        }
    }

    /// Return the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.read().map(|g| g.node_count()).unwrap_or(0)
    }

    /// Return the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.read().map(|g| g.edge_count()).unwrap_or(0)
    }

    /// Return true if a node with the given id exists.
    pub fn contains(&self, node_id: &str) -> bool {
        self.node_index.contains_key(node_id)
    }

    /// Get a node by id (clone).
    pub fn get_node(&self, node_id: &str) -> Option<MemoryNode> {
        let idx = self.node_index.get(node_id)?;
        let g = self.graph.read().ok()?;
        g.node_weight(*idx).cloned()
    }

    /// Compact the graph by removing low-value nodes beyond `max_nodes`.
    ///
    /// Scores each node by `(access_count + 1) × temporal_decay(last_accessed)`.
    /// Removes the lowest-scoring nodes until `node_count <= max_nodes`.
    /// Returns the count of removed nodes.
    #[tracing::instrument(skip(self))]
    pub fn compact(&self, max_nodes: usize) -> usize {
        let mut graph = match self.graph.write() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        let count = graph.node_count();
        if count <= max_nodes {
            return 0;
        }

        let now = chrono::Utc::now().timestamp() as f64;

        // Score each node
        let mut scored: Vec<(petgraph::stable_graph::NodeIndex, f64)> = graph
            .node_indices()
            .filter_map(|idx| {
                let node = graph.node_weight(idx)?;
                let decay = temporal_decay(node.last_accessed, now) as f64;
                let score = (node.access_count as f64 + 1.0) * decay;
                Some((idx, score))
            })
            .collect();

        // Sort ascending by score (lowest first = candidates for removal)
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let to_remove = count - max_nodes;
        let mut removed = 0;
        for (idx, _) in scored.iter().take(to_remove) {
            if let Some(node) = graph.node_weight(*idx) {
                self.node_index.remove(&node.id);
            }
            graph.remove_node(*idx);
            removed += 1;
        }

        tracing::debug!(removed, remaining = graph.node_count(), "graph compacted");
        removed
    }
}

/// Temporal decay factor: exp(-ln(2) × age / half_life).
/// Returns 1.0 for age=0 (just created), 0.5 at half_life, etc.
fn temporal_decay(created_at: f64, now: f64) -> f32 {
    if created_at <= 0.0 {
        return 1.0; // no timestamp → treat as fresh
    }
    let age_secs = (now - created_at).max(0.0);
    let decay = (-std::f64::consts::LN_2 * age_secs / BASE_DECAY_HALF_LIFE_SECS).exp();
    decay as f32
}

/// Cosine similarity between two equal-length slices. Returns 0.0 on zero/mismatched vectors.
#[cfg(test)]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    use touring_simd::{CosineComputer, CosineSimilarity};
    CosineComputer::new().cosine(a, b) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::persistence::GraphPersistence;
    use std::sync::Arc;

    fn make_graph() -> SemanticGraph {
        let p = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        SemanticGraph::new(p)
    }

    fn make_node(id: &str, embedding: Vec<f32>) -> MemoryNode {
        MemoryNode {
            id: id.to_string(),
            label: format!("label_{id}"),
            node_type: NodeType::Symbol,
            embedding,
            metadata: serde_json::json!({}),
            last_accessed: 0.0,
            access_count: 0,
        }
    }

    #[test]
    fn test_add_node_and_count() {
        let g = make_graph();
        assert_eq!(g.node_count(), 0);
        g.add_node(make_node("a", vec![1.0, 0.0, 0.0])).unwrap();
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn test_add_multiple_nodes() {
        let g = make_graph();
        g.add_node(make_node("a", vec![1.0, 0.0])).unwrap();
        g.add_node(make_node("b", vec![0.0, 1.0])).unwrap();
        g.add_node(make_node("c", vec![0.5, 0.5])).unwrap();
        assert_eq!(g.node_count(), 3);
    }

    #[test]
    fn test_upsert_node_updates_in_place() {
        let g = make_graph();
        g.add_node(make_node("a", vec![1.0, 0.0])).unwrap();
        assert_eq!(g.node_count(), 1);

        // Upsert: same id, different label
        let mut updated = make_node("a", vec![0.0, 1.0]);
        updated.label = "updated_label".to_string();
        g.add_node(updated).unwrap();

        // Still 1 node (updated in place, not duplicated)
        assert_eq!(g.node_count(), 1);
        let node = g.get_node("a").unwrap();
        assert_eq!(node.label, "updated_label");
        assert_eq!(node.access_count, 1); // incremented on upsert
    }

    #[test]
    fn test_add_edge_success() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        g.add_node(make_node("b", vec![])).unwrap();
        assert!(g.add_edge("a", "b", 1.0).is_ok());
    }

    #[test]
    fn test_add_edge_self_loop_rejected() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        let result = g.add_edge("a", "a", 1.0);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), SemanticGraphError::Validation(m) if m.contains("self-loop"))
        );
    }

    #[test]
    fn test_add_edge_missing_from_node() {
        let g = make_graph();
        g.add_node(make_node("b", vec![])).unwrap();
        let result = g.add_edge("missing", "b", 1.0);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), SemanticGraphError::Validation(m) if m.contains("node not found"))
        );
    }

    #[test]
    fn test_add_edge_missing_to_node() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        let result = g.add_edge("a", "missing", 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_node_stable_indices() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        g.add_node(make_node("b", vec![])).unwrap();
        g.add_node(make_node("c", vec![])).unwrap();
        g.add_edge("a", "b", 1.0).unwrap();
        g.add_edge("b", "c", 1.0).unwrap();

        // Remove middle node — StableGraph keeps indices valid
        let removed = g.remove_node("b").unwrap();
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "b");
        assert_eq!(g.node_count(), 2);

        // Node "c" should still be accessible (index not invalidated)
        assert!(g.contains("c"));
        assert!(g.get_node("c").is_some());

        // Edge from "a" to "b" should be gone (b was removed)
        let edges = g.edges_from("a");
        assert!(edges.is_empty());
    }

    #[test]
    fn test_remove_nonexistent_node() {
        let g = make_graph();
        let result = g.remove_node("ghost").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_retrieve_by_embedding_top_k() {
        let g = make_graph();
        g.add_node(make_node("a", vec![1.0, 0.0, 0.0])).unwrap();
        g.add_node(make_node("b", vec![0.0, 1.0, 0.0])).unwrap();
        g.add_node(make_node("c", vec![0.9, 0.1, 0.0])).unwrap();

        let results = g.retrieve_by_embedding(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
    }

    #[test]
    fn test_retrieve_empty_graph() {
        let g = make_graph();
        let results = g.retrieve_by_embedding(&[1.0, 0.0], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_retrieve_empty_query() {
        let g = make_graph();
        g.add_node(make_node("a", vec![1.0, 0.0])).unwrap();
        let results = g.retrieve_by_embedding(&[], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_retrieve_dimension_mismatch_filtered() {
        let g = make_graph();
        g.add_node(make_node("a", vec![1.0, 0.0])).unwrap();
        g.add_node(make_node("b", vec![1.0, 0.0, 0.0])).unwrap();
        let results = g.retrieve_by_embedding(&[1.0, 0.0, 0.0], 5);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "b");
    }

    #[test]
    fn test_edges_from_existing() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        g.add_node(make_node("b", vec![])).unwrap();
        g.add_node(make_node("c", vec![])).unwrap();
        g.add_edge("a", "b", 1.0).unwrap();
        g.add_edge("a", "c", 0.5).unwrap();
        let edges = g.edges_from("a");
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn test_edges_from_missing_node() {
        let g = make_graph();
        let edges = g.edges_from("nonexistent");
        assert!(edges.is_empty());
    }

    #[test]
    fn test_edges_to_incoming() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        g.add_node(make_node("b", vec![])).unwrap();
        g.add_edge("a", "b", 1.0).unwrap();
        let incoming = g.edges_to("b");
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].from_id, "a");
    }

    #[test]
    fn test_neighbors() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        g.add_node(make_node("b", vec![])).unwrap();
        g.add_node(make_node("c", vec![])).unwrap();
        g.add_edge("a", "b", 1.0).unwrap();
        g.add_edge("a", "c", 1.0).unwrap();
        let nbrs = g.neighbors("a");
        assert_eq!(nbrs.len(), 2);
    }

    #[test]
    fn test_touch_updates_access() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        let before = g.get_node("a").unwrap();
        assert_eq!(before.access_count, 0);

        g.touch("a");
        let after = g.get_node("a").unwrap();
        assert_eq!(after.access_count, 1);
        assert!(after.last_accessed > 0.0);
    }

    #[test]
    fn test_typed_edge() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        g.add_node(make_node("b", vec![])).unwrap();
        g.add_typed_edge("a", "b", EdgeType::Imports, 0.8).unwrap();
        let edges = g.edges_from("a");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge_type, EdgeType::Imports);
    }

    #[test]
    fn test_contains_and_get_node() {
        let g = make_graph();
        assert!(!g.contains("a"));
        g.add_node(make_node("a", vec![1.0])).unwrap();
        assert!(g.contains("a"));
        assert!(g.get_node("a").is_some());
        assert!(g.get_node("b").is_none());
    }

    #[test]
    fn test_edge_count() {
        let g = make_graph();
        g.add_node(make_node("a", vec![])).unwrap();
        g.add_node(make_node("b", vec![])).unwrap();
        assert_eq!(g.edge_count(), 0);
        g.add_edge("a", "b", 1.0).unwrap();
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let sim = cosine_similarity(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        let sim = cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let sim = cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_temporal_decay_values() {
        let now = 1000000.0;
        // Fresh: decay factor = 1.0
        assert!((temporal_decay(now, now) - 1.0).abs() < 1e-6);
        // Zero timestamp: treated as fresh
        assert!((temporal_decay(0.0, now) - 1.0).abs() < 1e-6);
        // At half-life: decay factor ≈ 0.5
        let half_life_ago = now - BASE_DECAY_HALF_LIFE_SECS;
        assert!((temporal_decay(half_life_ago, now) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_warm_cache_no_panic() {
        let g = make_graph();
        g.warm_cache("test hint");
    }

    // ── New tests for enhanced types ────────────────────────────────────────

    #[test]
    fn test_node_type_display() {
        assert_eq!(NodeType::Symbol.to_string(), "symbol");
        assert_eq!(NodeType::File.to_string(), "file");
        assert_eq!(NodeType::Concept.to_string(), "concept");
        assert_eq!(NodeType::Session.to_string(), "session");
    }

    #[test]
    fn test_node_type_all() {
        let all = NodeType::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&NodeType::Symbol));
        assert!(all.contains(&NodeType::File));
        assert!(all.contains(&NodeType::Concept));
        assert!(all.contains(&NodeType::Session));
    }

    #[test]
    fn test_edge_type_display() {
        assert_eq!(EdgeType::References.to_string(), "references");
        assert_eq!(EdgeType::CoEdit.to_string(), "co_edit");
        assert_eq!(EdgeType::Imports.to_string(), "imports");
    }

    #[test]
    fn test_edge_type_all() {
        let all = EdgeType::all();
        assert_eq!(all.len(), 6);
    }

    #[test]
    fn test_edge_type_default_weights() {
        assert!(EdgeType::Contains.default_weight() > EdgeType::Sequence.default_weight());
        assert!(EdgeType::Imports.default_weight() > EdgeType::Related.default_weight());
        for et in EdgeType::all() {
            let w = et.default_weight();
            assert!(w > 0.0 && w <= 1.0, "weight for {et} should be in (0, 1]");
        }
    }

    #[test]
    fn test_memory_node_new() {
        let node = MemoryNode::new("id1", "Label1", NodeType::File);
        assert_eq!(node.id, "id1");
        assert_eq!(node.label, "Label1");
        assert_eq!(node.node_type, NodeType::File);
        assert!(!node.has_embedding());
        assert_eq!(node.embedding_dim(), 0);
    }

    #[test]
    fn test_memory_node_display() {
        let node = MemoryNode::new("foo.rs", "foo", NodeType::File);
        let s = node.to_string();
        assert!(s.contains("file"), "Display should include node type: {s}");
        assert!(s.contains("foo.rs"), "Display should include id: {s}");
    }

    #[test]
    fn test_memory_node_relevance_score() {
        let now = 1_000_000.0;
        let mut node = MemoryNode::new("id", "label", NodeType::Symbol);
        node.last_accessed = now; // just accessed
        node.access_count = 10;
        let score = node.relevance_score(now);
        assert!(
            score > 0.0 && score <= 1.0,
            "relevance should be in (0, 1]: {score}"
        );

        // Old node should have lower relevance
        let mut old_node = MemoryNode::new("old", "old", NodeType::Symbol);
        old_node.last_accessed = now - 30.0 * 24.0 * 3600.0; // 30 days ago
        old_node.access_count = 1;
        let old_score = old_node.relevance_score(now);
        assert!(old_score < score, "old node should have lower relevance");
    }

    #[test]
    fn test_semantic_edge_new() {
        let edge = SemanticEdge::new("a", "b", EdgeType::Imports);
        assert_eq!(edge.from_id, "a");
        assert_eq!(edge.to_id, "b");
        assert_eq!(edge.weight, EdgeType::Imports.default_weight());
    }

    #[test]
    fn test_semantic_edge_with_weight() {
        let edge = SemanticEdge::with_weight("a", "b", EdgeType::CoEdit, 0.42);
        assert_eq!(edge.weight, 0.42);
    }

    #[test]
    fn test_semantic_edge_display() {
        let edge = SemanticEdge::new("src.rs", "lib.rs", EdgeType::References);
        let s = edge.to_string();
        assert!(s.contains("src.rs"), "Display should include from: {s}");
        assert!(s.contains("lib.rs"), "Display should include to: {s}");
        assert!(
            s.contains("references"),
            "Display should include edge type: {s}"
        );
    }

    #[test]
    fn test_node_type_serde_roundtrip() {
        for nt in NodeType::all() {
            let json = serde_json::to_string(nt).unwrap();
            let restored: NodeType = serde_json::from_str(&json).unwrap();
            assert_eq!(*nt, restored);
        }
    }

    #[test]
    fn test_edge_type_serde_roundtrip() {
        for et in EdgeType::all() {
            let json = serde_json::to_string(et).unwrap();
            let restored: EdgeType = serde_json::from_str(&json).unwrap();
            assert_eq!(*et, restored);
        }
    }

    // ── COG-3: Adaptive temporal decay tests ──────────────────

    #[test]
    fn test_adaptive_decay_increases_with_access() {
        let node_low = MemoryNode {
            access_count: 1,
            ..MemoryNode::new("a", "A", NodeType::Symbol)
        };
        let node_high = MemoryNode {
            access_count: 100,
            ..MemoryNode::new("b", "B", NodeType::Symbol)
        };
        // Higher access count → larger half-life → slower decay
        assert!(node_high.adaptive_half_life() > node_low.adaptive_half_life());
    }

    #[test]
    fn test_adaptive_decay_minimum_half_life() {
        // access_count=0 → ln(0+1)=0, max(0,1.0)=1.0 → half_life = BASE * 1.0
        let node = MemoryNode::new("a", "A", NodeType::Symbol);
        assert!((node.adaptive_half_life() - BASE_DECAY_HALF_LIFE_SECS).abs() < 1e-6);
    }

    #[test]
    fn test_zero_access_uses_base_half_life() {
        let node = MemoryNode {
            access_count: 0,
            ..MemoryNode::new("a", "A", NodeType::Symbol)
        };
        assert!((node.adaptive_half_life() - BASE_DECAY_HALF_LIFE_SECS).abs() < 1e-6);
    }

    #[test]
    fn test_relevance_score_adaptive_vs_fixed() {
        let now = 1_000_000.0_f64;
        let one_week_ago = now - BASE_DECAY_HALF_LIFE_SECS;

        // Node with 0 accesses (base half-life)
        let node_low = MemoryNode {
            access_count: 0,
            last_accessed: one_week_ago,
            ..MemoryNode::new("a", "A", NodeType::Symbol)
        };

        // Node with 50 accesses (longer half-life → higher relevance after same time)
        let node_high = MemoryNode {
            access_count: 50,
            last_accessed: one_week_ago,
            ..MemoryNode::new("b", "B", NodeType::Symbol)
        };

        // The high-access node should have higher relevance due to slower decay
        assert!(
            node_high.relevance_score(now) > node_low.relevance_score(now),
            "high-access node should have higher relevance: {} vs {}",
            node_high.relevance_score(now),
            node_low.relevance_score(now)
        );
    }
}
