//! CRDT-based Semantic Graph — convergent replicated graph for multi-agent scenarios.
//!
//! Implements OR-Set semantics with tombstone-based deletion and LWW (Last-Writer-Wins)
//! registers for node weights. This is a hand-rolled implementation optimized for
//! the Touring graph topology (nodes + labeled edges). The `crdts` workspace dependency
//! is available but not used here — its `Orswot` type targets different use cases
//! (set operations with vector clocks) that add complexity without benefit for our
//! graph-specific merge semantics.
//!
//! ## Persistence
//!
//! Supports persistence via `rkyv` + `memmap2`:
//! - [`CrdtSemanticGraph::save_to_mmap`] — serialize graph to memory-mapped file (atomic write via temp+rename)
//! - [`CrdtSemanticGraph::load_from_mmap`] — load from memory-mapped file (mmap read + owned deserialization)
//!
//! ## Zero-Copy Persistence
//!
//! Two zero-copy load paths are available (both use `memmap2::Mmap`):
//! - `CrdtSemanticGraph::load_from_mmap_zero_copy` — **safe path**, uses `rkyv::check_archived_root`
//!   to validate bytes before returning a reference to the archived representation
//! - `CrdtSemanticGraph::load_from_mmap_unchecked` — **unsafe path**, uses `rkyv::archived_root`
//!   directly for maximum performance; caller must guarantee the mmap bytes come from a trusted
//!   source (our own `save_to_mmap`)
//!
//! The mmap avoids a filesystem-to-heap copy during read. The zero-copy methods return
//! a reference to the archived representation without heap allocation.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// Identifier of a replica/agent making CRDT mutations.
pub type ActorId = u64;
/// Identifier of a node within the CRDT semantic graph.
pub type CrdtNodeId = u64;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[archive(check_bytes)]
/// Directed labeled edge between two nodes in the CRDT semantic graph.
pub struct CrdtEdge {
    /// Source node identifier.
    pub from: CrdtNodeId,
    /// Destination node identifier.
    pub to: CrdtNodeId,
    /// Semantic relationship label carried by this edge.
    pub label: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[archive(check_bytes)]
/// Last-Writer-Wins register holding a node's label and score with a timestamp.
pub struct NodeWeight {
    /// Human-readable label describing the node.
    pub label: String,
    /// Relevance/importance score for the node.
    pub score: f64,
    /// Logical timestamp used for LWW conflict resolution on merge.
    pub updated_at: u64,
}

/// Serializable snapshot of the CRDT graph state.
///
/// This is the type we actually persist — it mirrors `CrdtSemanticGraph` fields
/// but derives rkyv traits. We keep it separate so the main struct can stay
/// derive-free and ergonomic (e.g., no `Debug` constraints from rkyv on HashMap).
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub(crate) struct GraphSnapshot {
    nodes: Vec<CrdtNodeId>,
    edges: Vec<CrdtEdge>,
    weights: Vec<(CrdtNodeId, NodeWeight)>,
    removed: Vec<CrdtNodeId>,
}

/// Errors that can occur during CRDT graph persistence.
#[derive(Debug, thiserror::Error)]
pub enum CrdtPersistError {
    /// Underlying filesystem I/O failure during save or load.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The `rkyv` archive failed byte-level validation.
    #[error("rkyv validation failed: {0}")]
    Validation(String),
    /// The file is too small to contain a valid archive.
    #[error("file too small to contain valid archive ({size} bytes)")]
    FileTooSmall {
        /// Observed file size in bytes.
        size: u64,
    },
}

/// Convergent replicated semantic graph with OR-Set nodes, edges, and LWW weights.
#[derive(Debug)]
pub struct CrdtSemanticGraph {
    nodes: BTreeSet<CrdtNodeId>,
    edges: BTreeSet<CrdtEdge>,
    weights: HashMap<CrdtNodeId, NodeWeight>,
    removed: BTreeSet<CrdtNodeId>,
}

impl CrdtSemanticGraph {
    /// Creates an empty CRDT semantic graph.
    pub fn new() -> Self {
        Self {
            nodes: BTreeSet::new(),
            edges: BTreeSet::new(),
            weights: HashMap::new(),
            removed: BTreeSet::new(),
        }
    }

    /// Inserts (or revives a tombstoned) node with the given weight.
    pub fn add_node(&mut self, _actor: ActorId, id: CrdtNodeId, w: NodeWeight) {
        self.nodes.insert(id);
        self.removed.remove(&id);
        self.weights.insert(id, w);
    }

    /// Tombstones a node and drops its weight and incident edges.
    pub fn remove_node(&mut self, _actor: ActorId, id: CrdtNodeId) {
        self.nodes.remove(&id);
        self.removed.insert(id);
        self.weights.remove(&id);
        self.edges.retain(|e| e.from != id && e.to != id);
    }

    /// Inserts a labeled directed edge between two nodes.
    pub fn add_edge(
        &mut self,
        _actor: ActorId,
        from: CrdtNodeId,
        to: CrdtNodeId,
        label: impl Into<String>,
    ) {
        self.edges.insert(CrdtEdge {
            from,
            to,
            label: label.into(),
        });
    }

    /// Returns the set of currently live node identifiers.
    pub fn node_ids(&self) -> &BTreeSet<CrdtNodeId> {
        &self.nodes
    }

    /// Returns the set of edges currently in the graph.
    pub fn edge_list(&self) -> &BTreeSet<CrdtEdge> {
        &self.edges
    }

    /// Returns the weight register for a node, if present.
    pub fn get_weight(&self, id: CrdtNodeId) -> Option<&NodeWeight> {
        self.weights.get(&id)
    }

    /// Returns the number of live nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Updates a node's weight using LWW semantics (newer `updated_at` wins).
    pub fn update_weight(&mut self, id: CrdtNodeId, w: NodeWeight) {
        match self.weights.get(&id) {
            Some(existing) if w.updated_at >= existing.updated_at => {
                self.weights.insert(id, w);
            }
            None => {
                self.weights.insert(id, w);
            }
            _ => {}
        }
    }

    /// Merges another replica into this graph, converging node, edge, and weight state.
    pub fn merge(&mut self, other: &Self) {
        for &id in &other.nodes {
            if !self.removed.contains(&id) && !other.removed.contains(&id) {
                self.nodes.insert(id);
            }
        }
        for &id in &other.removed {
            self.removed.insert(id);
            self.nodes.remove(&id);
        }
        for e in &other.edges {
            if self.nodes.contains(&e.from) && self.nodes.contains(&e.to) {
                self.edges.insert(e.clone());
            }
        }
        for (id, w) in &other.weights {
            if self.nodes.contains(id) {
                self.update_weight(*id, w.clone());
            }
        }
    }

    // ── Persistence ────────────────────────────────────────────────────

    /// Convert to a serializable snapshot.
    fn to_snapshot(&self) -> GraphSnapshot {
        GraphSnapshot {
            nodes: self.nodes.iter().copied().collect(),
            edges: self.edges.iter().cloned().collect(),
            weights: self.weights.iter().map(|(&k, v)| (k, v.clone())).collect(),
            removed: self.removed.iter().copied().collect(),
        }
    }

    /// Restore from a deserialized snapshot.
    fn from_snapshot(snap: GraphSnapshot) -> Self {
        Self {
            nodes: snap.nodes.into_iter().collect(),
            edges: snap.edges.into_iter().collect(),
            weights: snap.weights.into_iter().collect(),
            removed: snap.removed.into_iter().collect(),
        }
    }

    /// Serialize the CRDT graph to a memory-mapped file.
    ///
    /// Uses rkyv zero-copy serialization. The file is written atomically
    /// (write to temp + rename) to prevent corruption on crash.
    pub fn save_to_mmap(&self, path: &Path) -> Result<(), CrdtPersistError> {
        use touring_rkyv::ser::Serializer;
        use touring_rkyv::ser::serializers::AllocSerializer;

        let snapshot = self.to_snapshot();

        // Serialize with rkyv (256-byte scratch space, grows as needed)
        let mut serializer = AllocSerializer::<256>::default();
        serializer
            .serialize_value(&snapshot)
            .map_err(|e| CrdtPersistError::Validation(format!("serialize: {e}")))?;
        let bytes = serializer.into_serializer().into_inner();

        // Atomic write: temp file → rename
        let parent = path.parent().unwrap_or(Path::new("."));
        let tmp_path = parent.join(format!(".crdt_tmp_{}.bin", std::process::id()));
        std::fs::write(&tmp_path, &bytes)?;
        std::fs::rename(&tmp_path, path)?;

        Ok(())
    }

    /// Load a CRDT graph from a memory-mapped file (owned deserialization).
    ///
    /// The file is memory-mapped for zero-copy access, then deserialized
    /// into an owned `CrdtSemanticGraph`.
    pub fn load_from_mmap(path: &Path) -> Result<Self, CrdtPersistError> {
        let file = std::fs::File::open(path)?;
        let metadata = file.metadata()?;
        if metadata.len() == 0 {
            return Err(CrdtPersistError::FileTooSmall { size: 0 });
        }

        // SAFETY: The file handle is valid (just opened and metadata checked above). The mmap
        // is read-only and we do not share the mapping with concurrent writers. The file was
        // written atomically (temp + rename in `save_to_mmap`), so partial reads cannot occur.
        // The mmap lifetime is local to this function; it is consumed by deserialization below.
        let mmap = unsafe { memmap2::Mmap::map(&file)? };

        // Validate and access the archived data
        let archived = rkyv::check_archived_root::<GraphSnapshot>(&mmap)
            .map_err(|e| CrdtPersistError::Validation(format!("validation: {e}")))?;

        // Deserialize into owned types
        let snapshot: GraphSnapshot =
            rkyv::Deserialize::deserialize(archived, &mut rkyv::Infallible)
                .map_err(|e| CrdtPersistError::Validation(format!("deserialize: {e}")))?;

        Ok(Self::from_snapshot(snapshot))
    }

    /// Zero-copy load via check_bytes validation (safe).
    #[cfg(test)]
    pub(crate) fn load_from_mmap_zero_copy(
        mmap: &memmap2::Mmap,
    ) -> Result<&ArchivedGraphSnapshot, CrdtPersistError> {
        rkyv::check_archived_root::<GraphSnapshot>(mmap.as_ref())
            .map_err(|e| CrdtPersistError::Validation(format!("check_archived_root: {e}")))
    }

    /// Unsafe zero-copy load — skips validation for max performance.
    ///
    /// # Safety
    ///
    /// Caller must guarantee the mmap bytes come from a trusted source
    /// (our own `save_to_mmap`) and that the mmap lifetime outlives
    /// the returned reference.
    #[cfg(test)]
    pub(crate) unsafe fn load_from_mmap_unchecked(mmap: &memmap2::Mmap) -> &ArchivedGraphSnapshot {
        // SAFETY: per this fn's contract, the caller guarantees the mmap bytes were
        // produced by our own `save_to_mmap` (trusted) and that the mmap outlives the
        // returned reference — the exact preconditions `archived_root` requires.
        unsafe { rkyv::archived_root::<GraphSnapshot>(mmap.as_ref()) }
    }
}

// ── Delta CRDT ──────────────────────────────────────────────────────────

/// Delta between two CRDT graph states — only the differences.
///
/// Used for P2P synchronization: instead of transferring the full graph state,
/// agents exchange deltas containing only what the other side is missing.
/// Deltas are idempotent — applying the same delta twice has no additional effect.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrdtDelta {
    /// Nodes present in source but absent in target, with their weights.
    pub added_nodes: Vec<(CrdtNodeId, NodeWeight)>,
    /// Nodes removed in source (tombstoned).
    pub removed_nodes: Vec<CrdtNodeId>,
    /// Edges present in source but absent in target.
    pub added_edges: Vec<CrdtEdge>,
    /// Edges present in target but removed in source (endpoints removed).
    pub removed_edges: Vec<(CrdtNodeId, CrdtNodeId)>,
    /// Weights updated in source with newer timestamps (LWW).
    pub updated_weights: Vec<(CrdtNodeId, NodeWeight)>,
    /// Unix timestamp (seconds) when this delta was created.
    pub timestamp: u64,
    /// Identifier of the agent/replica that produced this delta.
    pub source_id: String,
}

impl CrdtDelta {
    /// Returns `true` if this delta contains no changes.
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty()
            && self.updated_weights.is_empty()
    }

    /// Total number of individual changes in this delta.
    pub fn size(&self) -> usize {
        self.added_nodes.len()
            + self.removed_nodes.len()
            + self.added_edges.len()
            + self.removed_edges.len()
            + self.updated_weights.len()
    }

    /// Returns the RLE-encoded byte size for `removed_nodes` and `removed_edges`.
    ///
    /// Useful for estimating delta transfer cost before serialization.
    pub fn rle_encoded_size(&self) -> usize {
        let nodes_bytes = {
            let encoded = crate::rl::memory::rle::encode_u64(&self.removed_nodes);
            encoded.len()
        };
        let edges_bytes = {
            let encoded = crate::rl::memory::rle::encode_u64_pair(&self.removed_edges);
            encoded.len()
        };
        nodes_bytes.saturating_add(edges_bytes)
    }

    /// Returns the naive (uncompressed) byte size for all delta fields.
    ///
    /// Includes `added_nodes`, `removed_nodes`, `added_edges`, `removed_edges`,
    /// `updated_weights`, plus the fixed-size fields `timestamp` and `source_id`.
    pub fn naive_byte_size(&self) -> usize {
        use std::mem::size_of;
        // Vec fields: (len * elem_size) — CrdtNodeId = u64 (8 bytes)
        // added_nodes: Vec<(u64, NodeWeight)> — NodeWeight contains String so we use 24 as conservative inline estimate
        let node_weight_est = size_of::<u64>() + size_of::<f64>() + size_of::<u64>(); // label excluded (heap)
        let added_nodes_bytes = self.added_nodes.len() * (size_of::<u64>() + node_weight_est);
        let removed_nodes_bytes = self.removed_nodes.len() * size_of::<u64>();
        // CrdtEdge = (u64, u64, String) — we count just the IDs for the inline portion
        let edge_ids_bytes = size_of::<u64>() * 2;
        let added_edges_bytes = self.added_edges.len() * edge_ids_bytes;
        let removed_edges_bytes = self.removed_edges.len() * edge_ids_bytes;
        let updated_weights_bytes = self.updated_weights.len() * node_weight_est;
        // Fixed fields
        let timestamp_bytes = size_of::<u64>();
        let source_id_bytes = self.source_id.len();

        added_nodes_bytes
            + removed_nodes_bytes
            + added_edges_bytes
            + removed_edges_bytes
            + updated_weights_bytes
            + timestamp_bytes
            + source_id_bytes
    }

    /// Returns the compression ratio for RLE-able fields (`removed_nodes`, `removed_edges`).
    ///
    /// Ratio = `naive_byte_size` / `rle_encoded_size`.
    /// Values > 1.0 indicate RLE saves space; values < 1.0 indicate RLE overhead.
    pub fn rle_compression_ratio(&self) -> f64 {
        let naive = self.naive_byte_size() as f64;
        let rle = self.rle_encoded_size() as f64;
        if rle == 0.0 {
            return 1.0;
        }
        naive / rle
    }
}

impl CrdtSemanticGraph {
    /// Compute delta between `self` and `other` — what `other` has that `self` doesn't.
    ///
    /// The returned delta, when applied to `self` via `merge_delta`, brings `self`
    /// closer to convergence with `other`.
    pub fn delta(&self, other: &Self) -> CrdtDelta {
        let mut added_nodes = Vec::new();
        let mut removed_nodes = Vec::new();
        let mut added_edges = Vec::new();
        let mut removed_edges = Vec::new();
        let mut updated_weights = Vec::new();

        // Nodes in other but not in self (and not tombstoned in self)
        for &id in &other.nodes {
            if !self.nodes.contains(&id) {
                let weight = other.weights.get(&id).cloned().unwrap_or(NodeWeight {
                    label: String::new(),
                    score: 0.0,
                    updated_at: 0,
                });
                added_nodes.push((id, weight));
            }
        }

        // Nodes tombstoned in other but still alive in self
        for &id in &other.removed {
            if !self.removed.contains(&id) {
                removed_nodes.push(id);
            }
        }

        // Edges in other but not in self
        for edge in &other.edges {
            if !self.edges.contains(edge) {
                added_edges.push(edge.clone());
            }
        }

        // Edges in self whose endpoints were removed in other
        for edge in &self.edges {
            if (other.removed.contains(&edge.from) || other.removed.contains(&edge.to))
                && !self.removed.contains(&edge.from)
                && !self.removed.contains(&edge.to)
            {
                removed_edges.push((edge.from, edge.to));
            }
        }

        // Weights in other that are newer than self's (LWW)
        for (&id, other_w) in &other.weights {
            if let Some(self_w) = self.weights.get(&id)
                && other_w.updated_at > self_w.updated_at
            {
                updated_weights.push((id, other_w.clone()));
            }
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        CrdtDelta {
            added_nodes,
            removed_nodes,
            added_edges,
            removed_edges,
            updated_weights,
            timestamp,
            source_id: String::new(),
        }
    }

    /// Apply a delta to this graph — merge only the differences.
    ///
    /// This is idempotent: applying the same delta twice produces the same result.
    /// Respects CRDT semantics: removals win over concurrent additions (OR-Set with tombstones),
    /// and weight updates use LWW (Last-Writer-Wins) based on `updated_at`.
    pub fn merge_delta(&mut self, delta: &CrdtDelta) {
        // Apply removals first (tombstone semantics)
        for &id in &delta.removed_nodes {
            self.removed.insert(id);
            self.nodes.remove(&id);
            self.weights.remove(&id);
            self.edges.retain(|e| e.from != id && e.to != id);
        }

        // Remove edges whose endpoints are gone
        for &(from, to) in &delta.removed_edges {
            self.edges.retain(|e| !(e.from == from && e.to == to));
        }

        // Add nodes (skip if tombstoned)
        for (id, weight) in &delta.added_nodes {
            if !self.removed.contains(id) {
                self.nodes.insert(*id);
                // Only insert weight if not already present with newer timestamp
                self.update_weight(*id, weight.clone());
            }
        }

        // Add edges (only if both endpoints are alive)
        for edge in &delta.added_edges {
            if self.nodes.contains(&edge.from) && self.nodes.contains(&edge.to) {
                self.edges.insert(edge.clone());
            }
        }

        // Update weights (LWW)
        for (id, weight) in &delta.updated_weights {
            if self.nodes.contains(id) {
                self.update_weight(*id, weight.clone());
            }
        }
    }

    /// Generate a delta containing ALL of this graph's state (for initial sync).
    ///
    /// When a new replica joins, it can receive a `full_delta` from an existing
    /// replica and apply it via `merge_delta` to bootstrap its state.
    pub fn full_delta(&self, source_id: impl Into<String>) -> CrdtDelta {
        let added_nodes: Vec<(CrdtNodeId, NodeWeight)> = self
            .nodes
            .iter()
            .map(|&id| {
                let weight = self.weights.get(&id).cloned().unwrap_or(NodeWeight {
                    label: String::new(),
                    score: 0.0,
                    updated_at: 0,
                });
                (id, weight)
            })
            .collect();

        let removed_nodes: Vec<CrdtNodeId> = self.removed.iter().copied().collect();
        let added_edges: Vec<CrdtEdge> = self.edges.iter().cloned().collect();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        CrdtDelta {
            added_nodes,
            removed_nodes,
            added_edges,
            removed_edges: Vec::new(),
            updated_weights: Vec::new(),
            timestamp,
            source_id: source_id.into(),
        }
    }
}

impl Default for CrdtSemanticGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn w(l: &str, s: f64, t: u64) -> NodeWeight {
        NodeWeight {
            label: l.into(),
            score: s,
            updated_at: t,
        }
    }

    #[test]
    fn test_add() {
        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("A", 1.0, 1));
        g.add_node(1, 20, w("B", 2.0, 1));
        g.add_edge(1, 10, 20, "d");
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn test_rm() {
        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("A", 1.0, 1));
        g.add_node(1, 20, w("B", 2.0, 1));
        g.add_edge(1, 10, 20, "l");
        g.remove_node(1, 10);
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_merge() {
        let mut a = CrdtSemanticGraph::new();
        let mut b = CrdtSemanticGraph::new();
        a.add_node(1, 10, w("A", 1.0, 1));
        b.add_node(2, 20, w("B", 2.0, 1));
        a.merge(&b);
        assert_eq!(a.node_count(), 2);
    }

    #[test]
    fn test_lww() {
        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("old", 1.0, 100));
        g.update_weight(10, w("new", 2.0, 200));
        assert_eq!(g.get_weight(10).unwrap().label, "new");
    }

    #[test]
    fn test_lww_rej() {
        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("new", 1.0, 200));
        g.update_weight(10, w("old", 0.5, 100));
        assert_eq!(g.get_weight(10).unwrap().label, "new");
    }

    // ── P7.2 Persistence Tests ─────────────────────────────────────────

    #[test]
    fn test_crdt_save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("crdt.bin");

        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("A", 1.0, 100));
        g.add_node(1, 20, w("B", 2.0, 200));
        g.add_edge(1, 10, 20, "calls");
        g.save_to_mmap(&path).unwrap();

        let g2 = CrdtSemanticGraph::load_from_mmap(&path).unwrap();
        assert!(g2.node_ids().contains(&10));
        assert!(g2.node_ids().contains(&20));
        assert_eq!(g2.node_count(), 2);
        assert_eq!(g2.edge_count(), 1);
        assert_eq!(g2.get_weight(10).unwrap().label, "A");
        assert_eq!(g2.get_weight(20).unwrap().label, "B");

        // Verify edge content
        let edge = g2.edge_list().iter().next().unwrap();
        assert_eq!(edge.from, 10);
        assert_eq!(edge.to, 20);
        assert_eq!(edge.label, "calls");
    }

    #[test]
    fn test_crdt_save_load_with_removals() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("crdt_rm.bin");

        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("A", 1.0, 1));
        g.add_node(1, 20, w("B", 2.0, 1));
        g.remove_node(1, 10);
        g.save_to_mmap(&path).unwrap();

        let g2 = CrdtSemanticGraph::load_from_mmap(&path).unwrap();
        assert_eq!(g2.node_count(), 1);
        assert!(!g2.node_ids().contains(&10));
        assert!(g2.node_ids().contains(&20));
    }

    #[test]
    fn test_crdt_merge_preserves_both() {
        let mut g1 = CrdtSemanticGraph::new();
        g1.add_node(1, 10, w("A", 1.0, 1));

        let mut g2 = CrdtSemanticGraph::new();
        g2.add_node(2, 20, w("B", 2.0, 1));

        g1.merge(&g2);
        assert!(g1.node_ids().contains(&10));
        assert!(g1.node_ids().contains(&20));
        assert_eq!(g1.node_count(), 2);
    }

    #[test]
    fn test_crdt_merge_after_roundtrip() {
        let dir = TempDir::new().unwrap();
        let p1 = dir.path().join("instance1.bin");
        let p2 = dir.path().join("instance2.bin");

        // Instance 1: adds node A
        let mut g1 = CrdtSemanticGraph::new();
        g1.add_node(1, 10, w("A", 1.0, 100));
        g1.save_to_mmap(&p1).unwrap();

        // Instance 2: adds node B
        let mut g2 = CrdtSemanticGraph::new();
        g2.add_node(2, 20, w("B", 2.0, 200));
        g2.save_to_mmap(&p2).unwrap();

        // Load both and merge — multi-instance convergence
        let mut loaded1 = CrdtSemanticGraph::load_from_mmap(&p1).unwrap();
        let loaded2 = CrdtSemanticGraph::load_from_mmap(&p2).unwrap();
        loaded1.merge(&loaded2);

        assert_eq!(loaded1.node_count(), 2);
        assert!(loaded1.node_ids().contains(&10));
        assert!(loaded1.node_ids().contains(&20));
        assert_eq!(loaded1.get_weight(10).unwrap().label, "A");
        assert_eq!(loaded1.get_weight(20).unwrap().label, "B");
    }

    #[test]
    fn test_crdt_empty_graph_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.bin");

        let g = CrdtSemanticGraph::new();
        g.save_to_mmap(&path).unwrap();

        let g2 = CrdtSemanticGraph::load_from_mmap(&path).unwrap();
        assert_eq!(g2.node_count(), 0);
        assert_eq!(g2.edge_count(), 0);
    }

    #[test]
    fn test_crdt_load_nonexistent_file() {
        let result = CrdtSemanticGraph::load_from_mmap(Path::new("/tmp/nonexistent_crdt.bin"));
        assert!(result.is_err());
    }

    #[test]
    fn test_crdt_overwrite_save() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("overwrite.bin");

        // First save
        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("first", 1.0, 1));
        g.save_to_mmap(&path).unwrap();

        // Overwrite with different data
        let mut g2 = CrdtSemanticGraph::new();
        g2.add_node(1, 99, w("second", 9.0, 9));
        g2.save_to_mmap(&path).unwrap();

        // Load should reflect the latest save
        let loaded = CrdtSemanticGraph::load_from_mmap(&path).unwrap();
        assert_eq!(loaded.node_count(), 1);
        assert!(loaded.node_ids().contains(&99));
        assert_eq!(loaded.get_weight(99).unwrap().label, "second");
    }

    // ── S6: CrdtDelta P2P Tests ─────────────────────────────────────────

    #[test]
    fn test_delta_empty_graphs() {
        let a = CrdtSemanticGraph::new();
        let b = CrdtSemanticGraph::new();
        let delta = a.delta(&b);
        assert!(delta.is_empty());
        assert_eq!(delta.size(), 0);
    }

    #[test]
    fn test_delta_new_node_in_other() {
        let a = CrdtSemanticGraph::new();
        let mut b = CrdtSemanticGraph::new();
        b.add_node(2, 42, w("X", 5.0, 100));

        let delta = a.delta(&b);
        assert!(!delta.is_empty());
        assert_eq!(delta.added_nodes.len(), 1);
        assert_eq!(delta.added_nodes[0].0, 42);
        assert_eq!(delta.added_nodes[0].1.label, "X");
    }

    #[test]
    fn test_delta_symmetric_difference() {
        let mut a = CrdtSemanticGraph::new();
        a.add_node(1, 10, w("A", 1.0, 1));

        let mut b = CrdtSemanticGraph::new();
        b.add_node(2, 10, w("A", 1.0, 1));
        b.add_node(2, 20, w("B", 2.0, 1));

        // delta(a, b) = what b has that a doesn't = node 20
        let delta = a.delta(&b);
        assert_eq!(delta.added_nodes.len(), 1);
        assert_eq!(delta.added_nodes[0].0, 20);
    }

    #[test]
    fn test_merge_delta_applies_correctly() {
        let mut a = CrdtSemanticGraph::new();
        a.add_node(1, 10, w("A", 1.0, 1));

        let mut b = CrdtSemanticGraph::new();
        b.add_node(2, 10, w("A", 1.0, 1));
        b.add_node(2, 20, w("B", 2.0, 1));
        b.add_edge(2, 10, 20, "calls");

        let delta = a.delta(&b);
        a.merge_delta(&delta);

        assert_eq!(a.node_count(), 2);
        assert!(a.node_ids().contains(&20));
        assert_eq!(a.get_weight(20).unwrap().label, "B");
        assert_eq!(a.edge_count(), 1);
    }

    #[test]
    fn test_merge_delta_idempotent() {
        let mut a = CrdtSemanticGraph::new();
        a.add_node(1, 10, w("A", 1.0, 1));

        let mut b = CrdtSemanticGraph::new();
        b.add_node(2, 10, w("A", 1.0, 1));
        b.add_node(2, 20, w("B", 2.0, 1));

        let delta = a.delta(&b);
        a.merge_delta(&delta);
        let count_after_first = a.node_count();

        // Apply same delta again — should be idempotent
        a.merge_delta(&delta);
        assert_eq!(a.node_count(), count_after_first);
        assert_eq!(a.node_count(), 2);
    }

    #[test]
    fn test_full_delta_contains_all_nodes() {
        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("A", 1.0, 1));
        g.add_node(1, 20, w("B", 2.0, 2));
        g.add_edge(1, 10, 20, "depends");

        let fd = g.full_delta("agent-1");
        assert_eq!(fd.added_nodes.len(), 2);
        assert_eq!(fd.added_edges.len(), 1);
        assert!(fd.removed_nodes.is_empty());
        assert!(fd.removed_edges.is_empty());
        assert_eq!(fd.source_id, "agent-1");
    }

    #[test]
    fn test_delta_size() {
        let mut a = CrdtSemanticGraph::new();
        a.add_node(1, 10, w("A", 1.0, 1));

        let mut b = CrdtSemanticGraph::new();
        b.add_node(2, 20, w("B", 2.0, 1));
        b.add_node(2, 30, w("C", 3.0, 1));

        let delta = a.delta(&b);
        // 2 added nodes, 0 removed, 0 edges, 0 updated weights
        assert_eq!(delta.size(), 2);
    }

    #[test]
    fn test_delta_serialization_roundtrip() {
        let mut a = CrdtSemanticGraph::new();
        a.add_node(1, 10, w("A", 1.0, 100));

        let mut b = CrdtSemanticGraph::new();
        b.add_node(2, 10, w("A", 1.0, 100));
        b.add_node(2, 20, w("B", 2.0, 200));
        b.add_edge(2, 10, 20, "calls");

        let delta = a.delta(&b);
        let json = serde_json::to_string(&delta).unwrap();
        let deserialized: CrdtDelta = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.added_nodes.len(), delta.added_nodes.len());
        assert_eq!(deserialized.added_edges.len(), delta.added_edges.len());
        assert_eq!(deserialized.size(), delta.size());
        assert_eq!(deserialized.added_nodes[0].0, 20);
        assert_eq!(deserialized.added_nodes[0].1.label, "B");
    }

    // ── H1: Zero-Copy Tests ────────────────────────────────────────────

    #[test]
    fn test_zero_copy_safe_path() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("zc_safe.bin");

        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 10, w("A", 1.0, 100));
        g.add_node(1, 20, w("B", 2.0, 200));
        g.add_edge(1, 10, 20, "calls");
        g.save_to_mmap(&path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };

        let archived = CrdtSemanticGraph::load_from_mmap_zero_copy(&mmap).unwrap();
        assert_eq!(archived.nodes.len(), 2);
        assert_eq!(archived.edges.len(), 1);
    }

    #[test]
    fn test_zero_copy_safe_path_invalid() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("invalid.bin");
        std::fs::write(&path, b"not valid").unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };

        let result = CrdtSemanticGraph::load_from_mmap_zero_copy(&mmap);
        assert!(result.is_err());
    }

    #[test]
    fn test_unsafe_zero_copy() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("zc_unsafe.bin");

        let mut g = CrdtSemanticGraph::new();
        g.add_node(1, 42, w("X", 1.0, 999));
        g.save_to_mmap(&path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };

        let archived = unsafe { CrdtSemanticGraph::load_from_mmap_unchecked(&mmap) };
        assert_eq!(archived.nodes.len(), 1);
    }

    #[test]
    fn test_crdt_delta_rle_methods() {
        // CrdtNodeId = u64
        let delta = CrdtDelta {
            added_nodes: vec![(1, w("a", 1.0, 1)), (2, w("b", 2.0, 2))],
            removed_nodes: vec![5, 5, 5, 6, 6, 7], // runs of 5,5,5 / 6,6 / 7
            added_edges: vec![],
            removed_edges: vec![(1, 2), (1, 2), (1, 2), (3, 4)], // run of (1,2)
            updated_weights: vec![],
            timestamp: 100,
            source_id: "test".into(),
        };

        // naive_byte_size
        let naive = delta.naive_byte_size();
        assert!(naive > 0, "naive_byte_size must be > 0");

        // rle_encoded_size
        let rle_size = delta.rle_encoded_size();
        assert!(rle_size > 0, "rle_encoded_size must be > 0");

        // compression ratio
        let ratio = delta.rle_compression_ratio();
        // removed_nodes: 6 u64 = 48 bytes naive, RLE: [3×5][1×6][1×7] = 3×12 + 1×12 + 1×12 = 36 bytes
        // removed_edges: 4 pairs = 64 bytes naive, RLE: [3×(1,2)][1×(3,4)] = 2×20 = 40 bytes
        // Combined RLE overhead is 36+40=76 vs naive 48+64=112 → ratio > 1
        assert!(
            ratio > 1.0,
            "RLE should compress sequential IDs: ratio={ratio}"
        );

        // Empty delta
        let empty = CrdtDelta {
            added_nodes: vec![],
            removed_nodes: vec![],
            added_edges: vec![],
            removed_edges: vec![],
            updated_weights: vec![],
            timestamp: 0,
            source_id: String::new(),
        };
        assert_eq!(empty.rle_encoded_size(), 0);
        assert_eq!(empty.rle_compression_ratio(), 1.0);
    }
}
