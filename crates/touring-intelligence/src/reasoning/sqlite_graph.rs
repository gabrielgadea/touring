//! S15: SQLite-backed persistent semantic graph.
//!
//! Replaces the JSON-based `GraphPersistence` with indexed SQLite tables
//! for nodes and edges. Benefits:
//! - **O(1) node lookup** by ID (vs O(N) JSON scan)
//! - **Incremental save/load** — no full serialization on every change
//! - **Partial loads** — load only the nodes/edges you need
//! - **Concurrent-safe** — SQLite WAL mode supports concurrent reads

use crate::reasoning::semantic_graph::{EdgeType, MemoryNode, NodeType, SemanticEdge};
use rusqlite::{Connection, params};
use std::path::Path;

/// SQLite-backed graph persistence.
///
/// Stores nodes and edges in two tables with indexed lookups.
/// Uses WAL journal mode for concurrent read access.
#[derive(Debug)]
pub struct SqliteGraphStore {
    conn: Connection,
}

impl SqliteGraphStore {
    /// Open or create a SQLite graph store at the given path.
    ///
    /// Creates tables and indices if they don't exist.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = if path.to_str() == Some(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -4000;
             PRAGMA mmap_size = 67108864;",
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                node_type TEXT NOT NULL,
                embedding BLOB,
                metadata TEXT NOT NULL DEFAULT '{}',
                last_accessed REAL NOT NULL DEFAULT 0.0,
                access_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS edges (
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                weight REAL NOT NULL,
                created_at REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY (from_id, to_id, edge_type)
            );
            CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
            CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
            CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);
            CREATE INDEX IF NOT EXISTS idx_nodes_access ON nodes(access_count DESC);",
        )?;

        Ok(Self { conn })
    }

    /// Insert or update a node.
    pub fn upsert_node(&self, node: &MemoryNode) -> Result<(), rusqlite::Error> {
        let emb_bytes: Vec<u8> = node
            .embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let metadata = node.metadata.to_string();
        let node_type = node.node_type.to_string();

        self.conn.execute(
            "INSERT INTO nodes (id, label, node_type, embedding, metadata, last_accessed, access_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                node_type = excluded.node_type,
                embedding = CASE WHEN length(excluded.embedding) > 0 THEN excluded.embedding ELSE nodes.embedding END,
                metadata = excluded.metadata,
                last_accessed = excluded.last_accessed,
                access_count = nodes.access_count + 1",
            params![
                node.id,
                node.label,
                node_type,
                emb_bytes,
                metadata,
                node.last_accessed,
                node.access_count as i64,
            ],
        )?;
        Ok(())
    }

    /// Load a node by ID.
    pub fn load_node(&self, id: &str) -> Result<Option<MemoryNode>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, label, node_type, embedding, metadata, last_accessed, access_count FROM nodes WHERE id = ?1")?;

        let result = stmt.query_row(params![id], |row| {
            let id: String = row.get(0)?;
            let label: String = row.get(1)?;
            let node_type_str: String = row.get(2)?;
            let emb_bytes: Vec<u8> = row.get(3)?;
            let metadata_str: String = row.get(4)?;
            let last_accessed: f64 = row.get(5)?;
            let access_count: u64 = row.get::<_, i64>(6)? as u64;

            let node_type = parse_node_type(&node_type_str);
            let embedding = bytes_to_f32_vec(&emb_bytes);
            // Clone id before moving into closure to avoid borrow-after-move
            let node_id = id.clone();
            let metadata = serde_json::from_str(&metadata_str).unwrap_or_else(|e| {
                tracing::warn!(error = %e, node_id = %node_id, "corrupt metadata in sqlite_graph node, falling back to null");
                serde_json::Value::Null
            });

            Ok(MemoryNode {
                id,
                label,
                node_type,
                embedding,
                metadata,
                last_accessed,
                access_count,
            })
        });

        match result {
            Ok(node) => Ok(Some(node)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Insert or update an edge.
    pub fn upsert_edge(&self, edge: &SemanticEdge) -> Result<(), rusqlite::Error> {
        let edge_type = edge.edge_type.to_string();
        self.conn.execute(
            "INSERT INTO edges (from_id, to_id, edge_type, weight, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(from_id, to_id, edge_type) DO UPDATE SET
                weight = excluded.weight,
                created_at = excluded.created_at",
            params![
                edge.from_id,
                edge.to_id,
                edge_type,
                edge.weight as f64,
                edge.created_at
            ],
        )?;
        Ok(())
    }

    /// Load all edges from a node.
    pub fn edges_from(&self, from_id: &str) -> Result<Vec<SemanticEdge>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, edge_type, weight, created_at FROM edges WHERE from_id = ?1",
        )?;
        let edges = stmt
            .query_map(params![from_id], |row| {
                Ok(SemanticEdge {
                    from_id: row.get(0)?,
                    to_id: row.get(1)?,
                    edge_type: parse_edge_type(&row.get::<_, String>(2)?),
                    weight: row.get::<_, f64>(3)? as f32,
                    created_at: row.get(4)?,
                })
            })?
            .filter_map(|r| match r {
                Ok(edge) => Some(edge),
                Err(e) => {
                    tracing::warn!(error = %e, from_id = %from_id, "failed to deserialize edge in sqlite_graph");
                    None
                }
            })
            .collect();
        Ok(edges)
    }

    /// Delete a node and all its edges.
    pub fn delete_node(&self, id: &str) -> Result<bool, rusqlite::Error> {
        self.conn.execute(
            "DELETE FROM edges WHERE from_id = ?1 OR to_id = ?1",
            params![id],
        )?;
        let deleted = self
            .conn
            .execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    /// Count total nodes.
    pub fn node_count(&self) -> Result<usize, rusqlite::Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get::<_, i64>(0))
            .map(|c| c as usize)
    }

    /// Count total edges.
    pub fn edge_count(&self) -> Result<usize, rusqlite::Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM edges", [], |row| row.get::<_, i64>(0))
            .map(|c| c as usize)
    }

    /// Load top-N most accessed nodes.
    pub fn top_accessed_nodes(&self, limit: usize) -> Result<Vec<MemoryNode>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, node_type, embedding, metadata, last_accessed, access_count
             FROM nodes ORDER BY access_count DESC LIMIT ?1",
        )?;
        let nodes = stmt
            .query_map(params![limit as i64], |row| {
                let id: String = row.get(0)?;
                let label: String = row.get(1)?;
                let node_type_str: String = row.get(2)?;
                let emb_bytes: Vec<u8> = row.get(3)?;
                let metadata_str: String = row.get(4)?;
                let last_accessed: f64 = row.get(5)?;
                let access_count: u64 = row.get::<_, i64>(6)? as u64;

                // Clone id before moving into closure to avoid borrow-after-move
                let node_id = id.clone();
                let metadata = serde_json::from_str(&metadata_str).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, node_id = %node_id, "corrupt metadata in sqlite_graph top_accessed, falling back to null");
                    serde_json::Value::Null
                });
                Ok(MemoryNode {
                    id,
                    label,
                    node_type: parse_node_type(&node_type_str),
                    embedding: bytes_to_f32_vec(&emb_bytes),
                    metadata,
                    last_accessed,
                    access_count,
                })
            })?
            .filter_map(|r| match r {
                Ok(node) => Some(node),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to deserialize node in sqlite_graph top_accessed");
                    None
                }
            })
            .collect();
        Ok(nodes)
    }

    /// Touch a node: increment access_count and update last_accessed.
    pub fn touch_node(&self, id: &str, now_epoch: f64) -> Result<bool, rusqlite::Error> {
        let updated = self.conn.execute(
            "UPDATE nodes SET access_count = access_count + 1, last_accessed = ?2 WHERE id = ?1",
            params![id, now_epoch],
        )?;
        Ok(updated > 0)
    }
}

/// Convert byte slice to \<Vec\>\<f32\> (little-endian).
fn bytes_to_f32_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
            f32::from_le_bytes(arr)
        })
        .collect()
}

/// Parse node type from string.
fn parse_node_type(s: &str) -> NodeType {
    match s {
        "symbol" => NodeType::Symbol,
        "file" => NodeType::File,
        "concept" => NodeType::Concept,
        "session" => NodeType::Session,
        _ => NodeType::Concept,
    }
}

/// Parse edge type from string.
fn parse_edge_type(s: &str) -> EdgeType {
    match s {
        "references" => EdgeType::References,
        "contains" => EdgeType::Contains,
        "related" => EdgeType::Related,
        "sequence" => EdgeType::Sequence,
        "imports" => EdgeType::Imports,
        "co_edit" => EdgeType::CoEdit,
        _ => EdgeType::Related,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_store() -> SqliteGraphStore {
        SqliteGraphStore::open(&PathBuf::from(":memory:")).unwrap()
    }

    #[test]
    fn test_create_store() {
        let store = make_store();
        assert_eq!(store.node_count().unwrap(), 0);
        assert_eq!(store.edge_count().unwrap(), 0);
    }

    #[test]
    fn test_upsert_and_load_node() {
        let store = make_store();
        let node = MemoryNode {
            id: "test_node".into(),
            label: "Test".into(),
            node_type: NodeType::Symbol,
            embedding: vec![1.0, 2.0, 3.0],
            metadata: serde_json::json!({"key": "value"}),
            last_accessed: 100.0,
            access_count: 5,
        };
        store.upsert_node(&node).unwrap();

        let loaded = store.load_node("test_node").unwrap().unwrap();
        assert_eq!(loaded.id, "test_node");
        assert_eq!(loaded.label, "Test");
        assert_eq!(loaded.embedding, vec![1.0, 2.0, 3.0]);
        assert_eq!(loaded.access_count, 5);
    }

    #[test]
    fn test_upsert_increments_access_count() {
        let store = make_store();
        let node = MemoryNode::new("n1", "Node1", NodeType::File);
        store.upsert_node(&node).unwrap();

        // Upsert again
        store.upsert_node(&node).unwrap();

        let loaded = store.load_node("n1").unwrap().unwrap();
        assert_eq!(
            loaded.access_count, 1,
            "upsert should increment access_count"
        );
    }

    #[test]
    fn test_load_nonexistent() {
        let store = make_store();
        assert!(store.load_node("ghost").unwrap().is_none());
    }

    #[test]
    fn test_upsert_and_load_edge() {
        let store = make_store();
        let node_a = MemoryNode::new("a", "A", NodeType::Symbol);
        let node_b = MemoryNode::new("b", "B", NodeType::Symbol);
        store.upsert_node(&node_a).unwrap();
        store.upsert_node(&node_b).unwrap();

        let edge = SemanticEdge::new("a", "b", EdgeType::References);
        store.upsert_edge(&edge).unwrap();

        let edges = store.edges_from("a").unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to_id, "b");
        assert_eq!(edges[0].edge_type, EdgeType::References);
    }

    #[test]
    fn test_delete_node() {
        let store = make_store();
        store
            .upsert_node(&MemoryNode::new("x", "X", NodeType::File))
            .unwrap();
        assert_eq!(store.node_count().unwrap(), 1);

        assert!(store.delete_node("x").unwrap());
        assert_eq!(store.node_count().unwrap(), 0);
    }

    #[test]
    fn test_delete_node_cascades_edges() {
        let store = make_store();
        store
            .upsert_node(&MemoryNode::new("a", "A", NodeType::Symbol))
            .unwrap();
        store
            .upsert_node(&MemoryNode::new("b", "B", NodeType::Symbol))
            .unwrap();
        store
            .upsert_edge(&SemanticEdge::new("a", "b", EdgeType::Related))
            .unwrap();
        assert_eq!(store.edge_count().unwrap(), 1);

        store.delete_node("a").unwrap();
        assert_eq!(store.edge_count().unwrap(), 0);
    }

    #[test]
    fn test_top_accessed_nodes() {
        let store = make_store();
        for i in 0..10 {
            let mut node = MemoryNode::new(format!("n{i}"), format!("Node{i}"), NodeType::Symbol);
            node.access_count = (10 - i) as u64;
            store.upsert_node(&node).unwrap();
        }

        let top = store.top_accessed_nodes(3).unwrap();
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].id, "n0"); // highest access_count
    }

    #[test]
    fn test_touch_node() {
        let store = make_store();
        store
            .upsert_node(&MemoryNode::new("t", "T", NodeType::File))
            .unwrap();
        store.touch_node("t", 999.0).unwrap();

        let node = store.load_node("t").unwrap().unwrap();
        assert_eq!(node.access_count, 1);
        assert_eq!(node.last_accessed, 999.0);
    }

    #[test]
    fn test_touch_nonexistent() {
        let store = make_store();
        assert!(!store.touch_node("ghost", 1.0).unwrap());
    }

    #[test]
    fn test_node_count_and_edge_count() {
        let store = make_store();
        store
            .upsert_node(&MemoryNode::new("a", "A", NodeType::Symbol))
            .unwrap();
        store
            .upsert_node(&MemoryNode::new("b", "B", NodeType::Symbol))
            .unwrap();
        store
            .upsert_edge(&SemanticEdge::new("a", "b", EdgeType::Imports))
            .unwrap();

        assert_eq!(store.node_count().unwrap(), 2);
        assert_eq!(store.edge_count().unwrap(), 1);
    }

    #[test]
    fn test_embedding_roundtrip() {
        let store = make_store();
        let emb: Vec<f32> = (0..128).map(|i| i as f32 * 0.01).collect();
        let mut node = MemoryNode::new("emb", "Embedded", NodeType::Concept);
        node.embedding = emb.clone();
        store.upsert_node(&node).unwrap();

        let loaded = store.load_node("emb").unwrap().unwrap();
        assert_eq!(loaded.embedding.len(), 128);
        for (a, b) in loaded.embedding.iter().zip(emb.iter()) {
            assert!((a - b).abs() < 1e-6, "embedding value mismatch: {a} vs {b}");
        }
    }
}
