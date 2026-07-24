//! GoT Snapshot — rkyv zero-copy serializable snapshot of Graph of Thoughts state.
//!
//! Provides [`GoTSnapshot`] and [`GotNodeSnapshot`] structs that capture the
//! complete state of a [`GotEngine`] in a flat, pointer-free representation
//! suitable for zero-copy serialization via rkyv.
//!
//! # Architecture Note: Why NOT `touring_rkyv::templates`
//!
//! This module defines its own rkyv types rather than using
//! `touring_rkyv::templates::ArchivedGoTSnapshot` because:
//!
//! - **Semantic difference**: This `GoTSnapshot` captures *complete GoT engine
//!   state* (max_depth, beam_width, pheromone_trails, created_at_secs) for
//!   session pause/resume. The `ArchivedGoTSnapshot` template is a *minimal cross-crate
//!   IPC format* with only nodes, edges, session_id, and schema_version.
//! - **Engine coupling**: `GotNodeSnapshot` embeds engine-specific adjacency
//!   (child_ids) for runtime traversal, not suitable as a generic template.
//! - **Different schema lifecycle**: touring-cognitive owns this schema and bumps
//!   `SNAPSHOT_SCHEMA_VERSION` independently.
//!
//! The two types are **not interchangeable**. If cross-crate GoT data sharing is
//! needed, a new template should be added to touring-rkyv with a schema version
//! negotiated between both crates.
//!
//! # Design
//!
//! The snapshot is a **flat representation** — no `Arc`, `Box<dyn>`, or other
//! pointer types. All graph structure is encoded via string IDs and adjacency
//! lists. This enables:
//!
//! - Zero-copy deserialization via rkyv (`check_archived_root` + `Deserialize`)
//! - JSON fallback via serde for human-readable persistence
//! - Session pause/resume without losing GoT evaluation state

use std::collections::HashMap;

use crate::reasoning::got::GotEngine;

/// Error returned by [`GoTSnapshot`] serialization / deserialization (RBP-03).
/// `Display` is preserved byte-for-byte via the message carried in each variant.
#[derive(Debug, thiserror::Error)]
pub enum GoTSnapshotError {
    /// rkyv or JSON serialization failed.
    #[error("{0}")]
    Serialize(String),
    /// rkyv or JSON deserialization failed.
    #[error("{0}")]
    Deserialize(String),
    /// rkyv archive validation failed.
    #[error("{0}")]
    Validate(String),
    /// Snapshot schema version mismatch.
    #[error("{0}")]
    SchemaMismatch(String),
}

impl From<GoTSnapshotError> for String {
    fn from(e: GoTSnapshotError) -> Self {
        e.to_string()
    }
}

/// Current snapshot schema version. Bump on layout changes.
const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Serializable snapshot of a single GoT node.
#[derive(
    Debug,
    Clone,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct GotNodeSnapshot {
    /// Node ID (u64 serialized as string for rkyv compatibility).
    pub id: u64,
    /// Human-readable label.
    pub label: String,
    /// Heuristic weight.
    pub weight: f64,
    /// IDs of child nodes (adjacency list).
    pub child_ids: Vec<u64>,
}

/// Zero-copy serializable snapshot of a complete GoT session.
///
/// Captures the full graph structure (nodes + edges), engine configuration,
/// and session metadata. Designed for rkyv zero-copy persistence with JSON
/// fallback.
#[derive(
    Debug,
    Clone,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct GoTSnapshot {
    /// Schema version for forward-compatibility checks.
    pub version: u32,
    /// Session identifier.
    pub session_id: String,
    /// All nodes with their adjacency lists.
    pub nodes: Vec<GotNodeSnapshot>,
    /// Maximum exploration depth configured in the engine.
    pub max_depth: u32,
    /// Beam width configured in the engine.
    pub beam_width: u32,
    /// Pheromone trails (path_key -> strength), if pheromone layer was active.
    pub pheromone_trails: HashMap<String, f64>,
    /// Timestamp (seconds since UNIX epoch) when snapshot was created.
    pub created_at_secs: u64,
}

impl GoTSnapshot {
    /// Create a snapshot from the current engine state.
    ///
    /// Captures all nodes, edges, configuration, and pheromone trails.
    /// The `session_id` is caller-provided to identify this snapshot.
    pub fn from_engine(engine: &GotEngine, session_id: impl Into<String>) -> Self {
        let now = crate::reasoning::semantic_graph::now_epoch_secs() as u64;

        let nodes = engine
            .node_ids()
            .map(|id| {
                let (label, weight) = engine.node_info(id).unwrap_or_default();
                let child_ids = engine.children(id).unwrap_or_default();
                GotNodeSnapshot {
                    id,
                    label,
                    weight,
                    child_ids,
                }
            })
            .collect();

        Self {
            version: SNAPSHOT_SCHEMA_VERSION,
            session_id: session_id.into(),
            nodes,
            max_depth: engine.max_depth(),
            beam_width: engine.beam_width as u32,
            pheromone_trails: engine.pheromone_trails(),
            created_at_secs: now,
        }
    }

    /// Serialize to raw bytes via rkyv zero-copy.
    ///
    /// Uses a 4 KiB alignment buffer — adequate for typical GoT graphs.
    ///
    /// # Errors
    ///
    /// Returns a descriptive error if serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>, GoTSnapshotError> {
        rkyv::to_bytes::<_, 4096>(self)
            .map(|b| b.to_vec())
            .map_err(|e| {
                GoTSnapshotError::Serialize(format!("GoTSnapshot rkyv serialization failed: {e}"))
            })
    }

    /// Deserialize from raw rkyv bytes with validation.
    ///
    /// Validates the archived data via `check_archived_root` before
    /// deserializing, ensuring safety against corrupted data.
    ///
    /// # Errors
    ///
    /// Returns error on validation failure, schema mismatch, or deserialization failure.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GoTSnapshotError> {
        let archived = rkyv::check_archived_root::<Self>(bytes).map_err(|e| {
            GoTSnapshotError::Validate(format!("GoTSnapshot rkyv validation failed: {e}"))
        })?;

        if archived.version != SNAPSHOT_SCHEMA_VERSION {
            return Err(GoTSnapshotError::SchemaMismatch(format!(
                "GoTSnapshot schema mismatch: expected {SNAPSHOT_SCHEMA_VERSION}, got {}",
                archived.version
            )));
        }

        let snapshot: Self = rkyv::Deserialize::deserialize(archived, &mut rkyv::Infallible)
            .map_err(|e| {
                GoTSnapshotError::Deserialize(format!("GoTSnapshot deserialization failed: {e}"))
            })?;

        Ok(snapshot)
    }

    /// Serialize to JSON for human-readable persistence.
    ///
    /// # Errors
    ///
    /// Returns error on serialization failure.
    pub fn to_json(&self) -> Result<String, GoTSnapshotError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| GoTSnapshotError::Serialize(format!("GoTSnapshot to_json failed: {e}")))
    }

    /// Deserialize from JSON string.
    ///
    /// # Errors
    ///
    /// Returns error on parse failure.
    pub fn from_json(json: &str) -> Result<Self, GoTSnapshotError> {
        serde_json::from_str(json).map_err(|e| {
            GoTSnapshotError::Deserialize(format!("GoTSnapshot from_json failed: {e}"))
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::got::{GotEngine, GotNode};

    fn build_test_engine() -> GotEngine {
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "analyze", 1.0));
        engine.add_node(GotNode::new(2, "synthesize", 0.8));
        engine.add_node(GotNode::new(3, "evaluate", 0.6));
        engine.add_edge(1, 2);
        engine.add_edge(1, 3);
        engine
    }

    #[test]
    fn test_snapshot_roundtrip_rkyv() {
        let engine = build_test_engine();
        let snapshot = GoTSnapshot::from_engine(&engine, "test-session-1");

        let bytes = snapshot.to_bytes().expect("to_bytes should succeed");
        assert!(!bytes.is_empty(), "serialized bytes should not be empty");

        let restored = GoTSnapshot::from_bytes(&bytes).expect("from_bytes should succeed");
        assert_eq!(restored.session_id, "test-session-1");
        assert_eq!(restored.nodes.len(), 3);
        assert_eq!(restored.max_depth, 3);
        assert_eq!(restored.version, 1);
    }

    #[test]
    fn test_snapshot_from_empty_engine() {
        let engine = GotEngine::new(5);
        let snapshot = GoTSnapshot::from_engine(&engine, "empty");

        assert_eq!(snapshot.nodes.len(), 0);
        assert_eq!(snapshot.max_depth, 5);
        assert_eq!(snapshot.beam_width, 16); // default
        assert!(snapshot.pheromone_trails.is_empty());
    }

    #[test]
    fn test_snapshot_to_json() {
        let engine = build_test_engine();
        let snapshot = GoTSnapshot::from_engine(&engine, "json-test");

        let json = snapshot.to_json().expect("to_json should succeed");
        assert!(json.contains("json-test"));
        assert!(json.contains("analyze"));

        let restored = GoTSnapshot::from_json(&json).expect("from_json should succeed");
        assert_eq!(restored.session_id, "json-test");
        assert_eq!(restored.nodes.len(), 3);
    }

    #[test]
    fn test_snapshot_version_field() {
        let engine = GotEngine::new(1);
        let snapshot = GoTSnapshot::from_engine(&engine, "v-check");
        assert_eq!(snapshot.version, 1);
    }

    #[test]
    fn test_node_snapshot_child_ids() {
        let engine = build_test_engine();
        let snapshot = GoTSnapshot::from_engine(&engine, "edges");

        // Node 1 has children 2 and 3
        let node1 = snapshot.nodes.iter().find(|n| n.id == 1).expect("node 1");
        assert_eq!(node1.child_ids.len(), 2);
        assert!(node1.child_ids.contains(&2));
        assert!(node1.child_ids.contains(&3));

        // Nodes 2 and 3 have no children
        let node2 = snapshot.nodes.iter().find(|n| n.id == 2).expect("node 2");
        assert!(node2.child_ids.is_empty());
    }

    #[test]
    fn test_snapshot_beam_width_preserved() {
        let mut engine = GotEngine::new(2);
        engine.beam_width = 32;
        let snapshot = GoTSnapshot::from_engine(&engine, "beam");
        assert_eq!(snapshot.beam_width, 32);
    }

    #[test]
    fn test_snapshot_from_bytes_invalid_data() {
        let result = GoTSnapshot::from_bytes(&[0, 1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_snapshot_json_roundtrip_matches_rkyv() {
        let engine = build_test_engine();
        let snapshot = GoTSnapshot::from_engine(&engine, "dual");

        // rkyv roundtrip
        let rkyv_bytes = snapshot.to_bytes().expect("rkyv");
        let rkyv_restored = GoTSnapshot::from_bytes(&rkyv_bytes).expect("rkyv restore");

        // JSON roundtrip
        let json = snapshot.to_json().expect("json");
        let json_restored = GoTSnapshot::from_json(&json).expect("json restore");

        // Both should produce same logical state
        assert_eq!(rkyv_restored.session_id, json_restored.session_id);
        assert_eq!(rkyv_restored.nodes.len(), json_restored.nodes.len());
        assert_eq!(rkyv_restored.max_depth, json_restored.max_depth);
    }

    #[test]
    fn test_snapshot_created_at_is_nonzero() {
        let engine = GotEngine::new(1);
        let snapshot = GoTSnapshot::from_engine(&engine, "ts");
        assert!(snapshot.created_at_secs > 0, "timestamp should be nonzero");
    }

    #[test]
    fn test_snapshot_with_pheromone_engine() {
        let engine = GotEngine::new(3).with_beam_pheromone(0.1);
        let snapshot = GoTSnapshot::from_engine(&engine, "pheromone");
        // Empty pheromone trails (no reinforcement happened)
        assert!(snapshot.pheromone_trails.is_empty());
    }
}
