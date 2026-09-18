//! Common serialization templates used across touring crates.
//!
//! Each template provides zero-copy serialization via rkyv with optional byte validation.
//! All types get a `CheckBytes` impl to detect corruption on deserialization —
//! automatic since rkyv 0.8 whenever the `bytecheck` feature is on (the 0.7
//! opt-in `#[archive(check_bytes)]` no longer exists).
//!
//! Note: Archived types (`Archived*`) do NOT implement Clone because their backing
//! `ArchivedString`, `ArchivedVec<T>`, etc. do not implement Clone in rkyv 0.7.
//! Consumers should deserialize to the original type if Clone is needed.

use rkyv::{Archive, Deserialize, Serialize};

// ── Hook Event Templates ────────────────────────────────────────────────────

/// Event record for RL learning — tool outcome tracking.
///
/// Used by: `touring-intelligence::rl::aco::esaa`.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(compare(PartialEq))]
#[rkyv(derive(Debug))]
pub struct ArchivedEventRecord {
    /// Session identifier.
    pub session_id: String,
    /// Tool that was invoked.
    pub tool_name: String,
    /// Outcome string (e.g., "success", "error", "timeout").
    pub outcome: String,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// CILA level at invocation.
    pub cila_level: u8,
    /// Wall-clock timestamp in milliseconds.
    pub timestamp_ms: u64,
}

// ── Symbol & Index Templates ─────────────────────────────────────────────────

/// Index snapshot of dependency edges.
///
/// Used by: `touring-hooks-core::dependency_cache`.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedIndexSnapshot {
    /// Dependency edges as `(from_path, to_path)` string pairs.
    pub edges: Vec<(String, String)>,
    /// Total node count at snapshot time.
    pub node_count: usize,
    /// Schema version for forward compatibility.
    pub schema_version: u32,
}

// ── CRDT Graph Templates ────────────────────────────────────────────────────

/// CRDT edge for zero-copy graph snapshots.
///
/// Used by: `touring-intelligence::rl::memory::crdt_graph`.
#[derive(Archive, Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[rkyv(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash))]
pub struct ArchivedCrdtEdge {
    /// Source node id of the edge.
    pub from: u64,
    /// Destination node id of the edge.
    pub to: u64,
    /// Semantic relationship label carried by the edge.
    pub label: String,
}

/// Node weight entry for CRDT graph.
///
/// Used by: `touring-intelligence::rl::memory::crdt_graph`.
#[derive(Archive, Serialize, Deserialize, Debug, PartialEq)]
#[rkyv(derive(Debug, PartialEq))]
pub struct ArchivedNodeWeight {
    /// Human-readable label of the weighted node.
    pub label: String,
    /// Weight score reflecting the node's accumulated importance.
    pub score: f64,
    /// Unix-epoch timestamp of the last weight update.
    pub updated_at: u64,
}

/// Snapshot of the full CRDT graph state.
///
/// Used by: `touring-intelligence::rl::memory::crdt_graph`.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedGraphSnapshot {
    /// Ids of all nodes present in the graph.
    pub nodes: Vec<u64>,
    /// All directed edges between nodes.
    pub edges: Vec<ArchivedCrdtEdge>,
    /// Per-node weight entries keyed by node id.
    pub weights: Vec<(u64, ArchivedNodeWeight)>,
}
