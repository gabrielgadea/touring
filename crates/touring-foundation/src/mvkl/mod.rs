//! D39 — Multi-Resolution Knowledge Layer (MVKL)
//!
//! Provides layered knowledge representation for code analysis:
//! - **L0 (File Index)**: Raw token-level indexing of source files
//! - **L1 (Parsed Defs)**: Parsed definitions (functions, structs, traits)
//! - **L2 (Semantic Graph)**: Semantic relationships between definitions
//!
//! # Architecture
//!
//! ```text
//! Artifact (source file)
//!      |
//!      v
//! +-----+-----+----+
//! | L0  | L1  | L2 |
//! |file |parsed|semantic
//! |index|defs |graph
//! +-----+-----+----+
//!      |
//!      v
//!  QueryResult
//! ```

pub mod layer0_file_index;
pub mod layer1_parsed_defs;
pub mod layer2_semantic_graph;

// Re-exports
pub use layer0_file_index::{FileIndex, FileIndexEntry};
pub use layer1_parsed_defs::{ParsedDef, ParsedDefKind, ParsedDefsIndex};
pub use layer2_semantic_graph::{SemanticGraph, SemanticRelation, SemanticRelationKind};

use serde::{Deserialize, Serialize};

/// Knowledge layer levels (L0, L1, L2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LayerLevel {
    /// L0: File-level index (raw tokens)
    L0,
    /// L1: Parsed definitions (functions, structs, traits)
    L1,
    /// L2: Semantic graph (relationships)
    L2,
}

/// Result of a knowledge layer query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Matching items at each layer.
    pub layer_results: Vec<LayerResult>,
    /// Total score across layers.
    pub score: f32,
}

/// Result for a single layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResult {
    /// The layer level.
    pub level: LayerLevel,
    /// Items found at this layer.
    pub items: Vec<serde_json::Value>,
    /// Layer-specific score.
    pub score: f32,
}

/// Artifact to be indexed (source code file).
#[derive(Debug, Clone)]
pub struct Artifact {
    /// File path.
    pub path: String,
    /// File content (raw text).
    pub content: String,
    /// Language/extension hint.
    pub language: Option<String>,
}

/// Knowledge layer trait - implementations must be Send + Sync safe.
pub trait KnowledgeLayer: Send + Sync {
    /// Query this layer with a search string and target layer.
    fn query(&self, query: &str, layer: LayerLevel) -> QueryResult;

    /// Index an artifact into this layer.
    fn index(&mut self, artifact: &Artifact) -> Result<(), String>;
}
