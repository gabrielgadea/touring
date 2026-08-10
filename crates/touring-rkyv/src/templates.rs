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

/// Hook event record for zero-copy IPC between Touring processes.
///
/// Used by: touring-learning ESAA event sourcing.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(compare(PartialEq))]
#[rkyv(derive(Debug))]
pub struct ArchivedHookEvent {
    /// Name of the hook that fired.
    pub hook_name: String,
    /// Monotonic timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Arbitrary payload as raw bytes.
    pub payload: Vec<u8>,
    /// CILA complexity level at time of hook.
    pub cila_level: u8,
}

/// Event record for RL learning — tool outcome tracking.
///
/// Used by: touring-learning ESAA event sourcing.
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

/// Symbol descriptor for zero-copy symbol index snapshots.
///
/// Used by: touring-index symbol store snapshots.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(compare(PartialEq))]
#[rkyv(derive(Debug))]
pub struct ArchivedSymbol {
    /// Symbol name (e.g., function name, struct name).
    pub name: String,
    /// Fully qualified module path.
    pub module_path: String,
    /// Source line number.
    pub line: u32,
}

/// Index snapshot of dependency edges.
///
/// Used by: touring-hooks dependency cache.
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

// ── RL Learning Templates ────────────────────────────────────────────────────

/// Snapshot of QTable learning parameters.
///
/// Used by: touring-learning RL Q-table persistence.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedLearningParamsSnapshot {
    /// Learning rate applied to each Q-value update.
    pub alpha: f64,
    /// Discount factor weighting future rewards.
    pub gamma: f64,
    /// Eligibility-trace decay factor for TD(λ).
    pub lambda: f64,
    /// Default Q-value assigned to unseen state-action pairs.
    pub initial_q: f64,
    /// Current exploration probability for ε-greedy action selection.
    pub epsilon: f64,
    /// Multiplicative decay applied to `epsilon` after each step.
    pub epsilon_decay: f64,
    /// Floor below which `epsilon` is never decayed.
    pub epsilon_min: f64,
}

/// Snapshot of QTable state for zero-copy persistence.
///
/// Used by: touring-learning RL Q-table persistence.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedQTableSnapshot {
    /// Q-values as `(state, action, value)` triples.
    pub q_values: Vec<(u64, u64, f64)>,
    /// Learning parameters.
    pub params: ArchivedLearningParamsSnapshot,
    /// Monotonic revision counter.
    pub revision: u64,
    /// Granular reward update count.
    pub granular_update_count: u64,
    /// Running sums per sub-reward dimension.
    pub reward_sums: [f64; 5],
}

/// Snapshot of a single LinUCB arm.
///
/// Used by: touring-learning LinUCB bandit persistence.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedLinUCBArmSnapshot {
    /// Flattened A_inv matrix (row-major, d*d elements).
    pub a_inv_flat: Vec<f64>,
    /// Reward-weighted feature vector (d elements).
    pub b: Vec<f64>,
    /// Number of pulls for this arm.
    pub pulls: u64,
    /// Cumulative reward for this arm.
    pub cumulative_reward: f64,
}

/// Snapshot of LinUCB bandit state.
///
/// Used by: touring-learning LinUCB bandit persistence.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedLinUCBSnapshot {
    /// Per-arm snapshots.
    pub arms: Vec<ArchivedLinUCBArmSnapshot>,
    /// Exploration parameter.
    pub alpha: f64,
    /// Feature dimensionality.
    pub d: usize,
}

// ── CRDT Graph Templates ────────────────────────────────────────────────────

/// CRDT edge for zero-copy graph snapshots.
///
/// Used by: touring-learning CRDT semantic graph.
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
/// Used by: touring-learning CRDT semantic graph.
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
/// Used by: touring-learning CRDT semantic graph persistence.
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

// ── Cognitive / GoT Templates ───────────────────────────────────────────────

/// Serializable snapshot of a single GoT node.
///
/// Used by: touring-cognitive GoT session snapshots.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedGotNodeSnapshot {
    /// Unique id of the GoT node.
    pub id: u64,
    /// Human-readable label of the thought node.
    pub label: String,
    /// Scalar weight ranking the node within the graph.
    pub weight: f64,
    /// Ids of this node's children in the thought graph.
    pub child_ids: Vec<u64>,
}

/// Zero-copy serializable snapshot of a complete GoT session.
///
/// Used by: touring-cognitive GoT session snapshots.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ArchivedGoTSnapshot {
    /// All thought nodes in the session graph.
    pub nodes: Vec<ArchivedGotNodeSnapshot>,
    /// Directed `(from, to)` edges connecting the nodes.
    pub edges: Vec<(u64, u64)>,
    /// Identifier of the session this snapshot belongs to.
    pub session_id: String,
    /// Schema version for forward compatibility.
    pub schema_version: u32,
}
