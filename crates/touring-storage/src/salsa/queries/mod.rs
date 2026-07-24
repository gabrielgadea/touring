//! Tracked queries for the touring-incremental-salsa POC.
//!
//! Three hot queries are implemented:
//! - `blast_radius_for_file` — transitive dependency count
//! - `wiring_chains_from` — symbol wiring chain tree
//! - `file_knowledge_extended` — extended metadata for a file

pub mod blast;
pub mod file_knowledge;
pub mod wiring;

// Re-exports
pub use blast::blast_radius_for_file;
pub use file_knowledge::file_knowledge_extended;
pub use wiring::wiring_chains_from;

// Result types shared across queries
use serde::{Deserialize, Serialize};

/// Unique file identifier for query result types.
///
/// Derives `salsa::Update` so it can appear inside tracked-query return types
/// (salsa 0.18 requires all memoized values to be `Update`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, salsa::Update)]
pub struct FileKey(u32);
impl FileKey {
    /// Construct a `FileKey` from a raw u32 file id.
    pub fn new(id: u32) -> Self {
        FileKey(id)
    }
}
impl From<u32> for FileKey {
    fn from(v: u32) -> Self {
        FileKey(v)
    }
}
impl From<FileKey> for u32 {
    fn from(f: FileKey) -> u32 {
        f.0
    }
}
impl Default for FileKey {
    fn default() -> Self {
        FileKey(u32::MAX)
    }
}

/// Blast radius result for a file.
///
/// Derives `salsa::Update` so it is a valid `#[salsa::tracked]` return type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, salsa::Update)]
pub struct BlastRadiusResult {
    /// The file this blast radius was computed for.
    pub file_id: FileKey,
    /// Files that directly consume a symbol defined in `file_id`.
    pub direct_deps: Vec<FileKey>,
    /// All files reachable transitively through the consumer graph.
    pub transitive_deps: Vec<FileKey>,
    /// Total number of transitive dependents.
    pub dep_count: usize,
    /// Maximum consumer-graph depth reached during the BFS walk.
    pub max_depth: usize,
}

/// Node in a wiring chain tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainNode {
    /// Name of the symbol at this node.
    pub symbol: String,
    /// File in which the symbol is defined.
    pub file: FileKey,
    /// Indices into [`ChainTree::nodes`] of this node's downstream children.
    pub children: Vec<usize>,
    /// Whether this node has no further downstream consumers.
    pub is_leaf: bool,
}

/// Wiring chain tree for a symbol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChainTree {
    /// Index into `nodes` of the root, or `None` for an empty tree.
    pub root: Option<usize>,
    /// Flat arena of chain nodes, referenced by index.
    pub nodes: Vec<ChainNode>,
}

/// Extended file knowledge metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtendedKnowledge {
    /// The file this metadata describes.
    pub file_id: FileKey,
    /// Cyclomatic complexity score for the file.
    pub complexity_score: f64,
    /// Cognitive complexity score for the file.
    pub cognitive_score: f64,
    /// Number of outgoing dependencies (symbols this file uses).
    pub fan_out: u32,
    /// Number of incoming dependencies (files that use this file).
    pub fan_in: u32,
    /// Graph modularity score reflecting how cohesive the file's community is.
    pub modularity_score: f64,
    /// Identifier of the graph community this file belongs to.
    pub community_id: u32,
    /// Whether the file has no consumers (an orphan).
    pub is_orphan: bool,
}
