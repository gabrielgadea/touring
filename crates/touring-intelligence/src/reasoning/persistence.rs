//! Graph persistence backend — serde_json roundtrip for SemanticGraph snapshots.

use crate::reasoning::error::{CognitiveError, CognitiveResult};
use crate::reasoning::semantic_graph::{MemoryNode, SemanticEdge};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Serializable snapshot of the semantic graph state.
///
/// COG-5: Extended with `transition_matrix` and `q_values` for cross-session
/// RL learning. `#[serde(default)]` ensures backward compatibility with older
/// snapshots that lack these fields.
#[derive(Debug, Serialize, Deserialize)]
pub struct GraphSnapshot {
    /// All memory nodes captured in the snapshot.
    pub nodes: Vec<MemoryNode>,
    /// All semantic edges connecting the snapshot's nodes.
    pub edges: Vec<SemanticEdge>,
    /// COG-5: SessionPredictor transition counts (state → state → count).
    #[serde(default)]
    pub transition_matrix:
        std::collections::HashMap<String, std::collections::HashMap<String, u64>>,
    /// COG-5: Q-values from RL agent (action → value).
    #[serde(default)]
    pub q_values: std::collections::HashMap<String, f64>,
}

/// Persists SemanticGraph snapshots to disk using serde_json.
#[derive(Debug)]
pub struct GraphPersistence {
    path: PathBuf,
}

impl GraphPersistence {
    /// Create a new persistence backend targeting `path`.
    /// The file at `path` is created on `save()` — not on construction.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Save a snapshot to disk. Creates parent directories if needed.
    /// Returns the number of bytes written.
    pub fn save(&self, snapshot: &GraphSnapshot) -> CognitiveResult<usize> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(snapshot)
            .map_err(|e| CognitiveError::Serialization(e.to_string()))?;
        let bytes = json.len();
        std::fs::write(&self.path, &json)?;
        Ok(bytes)
    }

    /// Load a snapshot from disk.
    /// Returns `Ok(None)` if the file does not exist (first run).
    pub fn load(&self) -> CognitiveResult<Option<GraphSnapshot>> {
        let json = match std::fs::read_to_string(&self.path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CognitiveError::Io(e)),
        };
        let snapshot = serde_json::from_str(&json)
            .map_err(|e| CognitiveError::Serialization(e.to_string()))?;
        Ok(Some(snapshot))
    }

    /// Return the target path (for logging/diagnostics).
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::semantic_graph::{EdgeType, MemoryNode, NodeType, SemanticEdge};
    use tempfile::TempDir;

    fn make_snapshot() -> GraphSnapshot {
        GraphSnapshot {
            nodes: vec![MemoryNode {
                id: "test_node".to_string(),
                label: "Test".to_string(),
                node_type: NodeType::Symbol,
                embedding: vec![1.0, 0.0, 0.0],
                metadata: serde_json::json!({"key": "value"}),
                last_accessed: 0.0,
                access_count: 0,
            }],
            edges: vec![],
            transition_matrix: std::collections::HashMap::new(),
            q_values: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("graph.json");
        let p = GraphPersistence::new(path);

        let snapshot = make_snapshot();
        let bytes = p.save(&snapshot).unwrap();
        assert!(bytes > 0);

        let loaded = p.load().unwrap().unwrap();
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.nodes[0].id, "test_node");
        assert_eq!(loaded.edges.len(), 0);
    }

    #[test]
    fn test_load_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        let p = GraphPersistence::new(path);
        let result = p.load().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir
            .path()
            .join("subdir1")
            .join("subdir2")
            .join("graph.json");
        let p = GraphPersistence::new(path.clone());
        let snapshot = make_snapshot();
        p.save(&snapshot).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_path_accessor() {
        let p = GraphPersistence::new(std::path::PathBuf::from("/tmp/test.json"));
        assert_eq!(p.path().to_str().unwrap(), "/tmp/test.json");
    }

    #[test]
    fn test_save_with_edges() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("graph.json");
        let p = GraphPersistence::new(path);

        let snapshot = GraphSnapshot {
            nodes: vec![
                MemoryNode {
                    id: "a".to_string(),
                    label: "A".to_string(),
                    node_type: NodeType::File,
                    embedding: vec![],
                    metadata: serde_json::json!(null),
                    last_accessed: 0.0,
                    access_count: 0,
                },
                MemoryNode {
                    id: "b".to_string(),
                    label: "B".to_string(),
                    node_type: NodeType::Concept,
                    embedding: vec![],
                    metadata: serde_json::json!(null),
                    last_accessed: 0.0,
                    access_count: 0,
                },
            ],
            edges: vec![SemanticEdge {
                from_id: "a".to_string(),
                to_id: "b".to_string(),
                edge_type: EdgeType::References,
                weight: 0.9,
                created_at: 0.0,
            }],
            transition_matrix: std::collections::HashMap::new(),
            q_values: std::collections::HashMap::new(),
        };
        p.save(&snapshot).unwrap();
        let loaded = p.load().unwrap().unwrap();
        assert_eq!(loaded.edges.len(), 1);
        assert_eq!(loaded.edges[0].weight, 0.9);
    }

    // ── COG-5: TransitionMatrix + QTable persistence tests ────

    fn make_snapshot_with_predictor() -> GraphSnapshot {
        let mut tm = std::collections::HashMap::new();
        let mut inner = std::collections::HashMap::new();
        inner.insert("read".to_string(), 5_u64);
        tm.insert("write".to_string(), inner);

        let mut qv = std::collections::HashMap::new();
        qv.insert("action_a".to_string(), 0.75_f64);

        GraphSnapshot {
            nodes: vec![MemoryNode::new("n1", "Node1", NodeType::Symbol)],
            edges: vec![],
            transition_matrix: tm,
            q_values: qv,
        }
    }

    #[test]
    fn test_snapshot_with_transitions_roundtrip() {
        let dir = TempDir::new().unwrap();
        let p = GraphPersistence::new(dir.path().join("tm.json"));
        let original = make_snapshot_with_predictor();
        p.save(&original).unwrap();
        let loaded = p.load().unwrap().unwrap();
        assert_eq!(loaded.transition_matrix.len(), 1);
        assert_eq!(loaded.transition_matrix["write"]["read"], 5);
    }

    #[test]
    fn test_snapshot_with_q_values_roundtrip() {
        let dir = TempDir::new().unwrap();
        let p = GraphPersistence::new(dir.path().join("qv.json"));
        let original = make_snapshot_with_predictor();
        p.save(&original).unwrap();
        let loaded = p.load().unwrap().unwrap();
        assert_eq!(loaded.q_values.len(), 1);
        assert!((loaded.q_values["action_a"] - 0.75).abs() < 1e-10);
    }

    #[test]
    fn test_backward_compatible_load() {
        // Simulate an old snapshot without transition_matrix/q_values
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("old.json");
        let old_json = r#"{"nodes":[],"edges":[]}"#;
        std::fs::write(&path, old_json).unwrap();
        let p = GraphPersistence::new(path);
        let loaded = p.load().unwrap().unwrap();
        // serde(default) fills missing fields with empty collections
        assert!(loaded.transition_matrix.is_empty());
        assert!(loaded.q_values.is_empty());
        assert!(loaded.nodes.is_empty());
    }

    #[test]
    fn test_empty_predictor_serializes() {
        let dir = TempDir::new().unwrap();
        let p = GraphPersistence::new(dir.path().join("empty.json"));
        let snap = GraphSnapshot {
            nodes: vec![],
            edges: vec![],
            transition_matrix: std::collections::HashMap::new(),
            q_values: std::collections::HashMap::new(),
        };
        p.save(&snap).unwrap();
        let loaded = p.load().unwrap().unwrap();
        assert!(loaded.transition_matrix.is_empty());
        assert!(loaded.q_values.is_empty());
    }

    #[test]
    fn test_predictor_data_integrity_after_roundtrip() {
        let dir = TempDir::new().unwrap();
        let p = GraphPersistence::new(dir.path().join("integrity.json"));
        let original = make_snapshot_with_predictor();
        p.save(&original).unwrap();
        let loaded = p.load().unwrap().unwrap();
        // Q-value preserved with full precision
        let orig_val = original.q_values["action_a"];
        let loaded_val = loaded.q_values["action_a"];
        assert!((orig_val - loaded_val).abs() < 1e-15);
        // Transition count exact
        assert_eq!(
            original.transition_matrix["write"]["read"],
            loaded.transition_matrix["write"]["read"]
        );
    }
}
