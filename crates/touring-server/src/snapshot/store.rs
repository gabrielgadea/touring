//! Snapshot storage — serde_json serialization + metadata management.

use super::{Snapshot, SnapshotMetadata, SnapshotScope};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Snapshot metadata stored in SQLite-free JSON index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub name: String,
    pub created_at: String,
    pub scope: String,
    pub node_count: usize,
    pub edge_count: usize,
}

/// Scope of a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapshotScope {
    #[default]
    Workspace,
    Crate,
    File,
}

impl std::str::FromStr for SnapshotScope {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "workspace" => Ok(Self::Workspace),
            "crate" => Ok(Self::Crate),
            "file" => Ok(Self::File),
            _ => Err(format!("unknown scope '{}': use workspace, crate, or file", s)),
        }
    }
}

impl std::fmt::Display for SnapshotScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotScope::Workspace => write!(f, "workspace"),
            SnapshotScope::Crate => write!(f, "crate"),
            SnapshotScope::File => write!(f, "file"),
        }
    }
}

/// Storage backend for snapshots.
pub struct SnapshotStore {
    base_dir: PathBuf,
}

impl SnapshotStore {
    /// Create a new store at the given base directory.
    pub fn new(base_dir: &PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(base_dir)?;
        Ok(Self { base_dir: base_dir.clone() })
    }

    /// Save a snapshot to disk.
    pub fn save(&self, snapshot: &Snapshot) -> anyhow::Result<String> {
        let file_name = format!("{}.json", snapshot.name);
        let file_path = self.base_dir.join(&file_name);

        let encoded = serde_json::to_vec(snapshot)
            .map_err(|e| anyhow::anyhow!("serde_json serialize failed: {}", e))?;

        fs::write(&file_path, &encoded)?;

        // Update index
        self.update_index(snapshot)?;

        Ok(file_name)
    }

    /// Load a snapshot by name.
    pub fn load(&self, name: &str) -> anyhow::Result<Snapshot> {
        let file_path = self.base_dir.join(format!("{}.json", name));
        let data = fs::read(&file_path)?;
        let snapshot = serde_json::from_slice(&data)
            .map_err(|e| anyhow::anyhow!("serde_json deserialize failed: {}", e))?;
        Ok(snapshot)
    }

    /// List all available snapshots.
    pub fn list(&self) -> anyhow::Result<Vec<Snapshot>> {
        let index = self.load_index()?;
        let mut snapshots = Vec::new();

        for entry in index.entries {
            if let Ok(snap) = self.load(&entry.name) {
                snapshots.push(snap);
            }
        }

        Ok(snapshots)
    }

    /// Delete a snapshot by name.
    pub fn delete(&self, name: &str) -> anyhow::Result<()> {
        let file_path = self.base_dir.join(format!("{}.bin", name));
        if file_path.exists() {
            fs::remove_file(&file_path)?;
        }
        self.remove_from_index(name)?;
        Ok(())
    }

    /// Get the file path for a snapshot name.
    pub fn get_path(&self, name: &str) -> PathBuf {
        self.base_dir.join(format!("{}.bin", name))
    }

    // ─── Private helpers ────────────────────────────────────────────────────

    #[derive(Serialize, Deserialize, Default)]
    struct Index {
        entries: Vec<IndexEntry>,
    }

    #[derive(Serialize, Deserialize)]
    struct IndexEntry {
        name: String,
        created_at: String,
        scope: String,
        node_count: usize,
        edge_count: usize,
    }

    fn index_path(&self) -> PathBuf {
        self.base_dir.join("index.json")
    }

    fn load_index(&self) -> anyhow::Result<Index> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Index::default());
        }
        let data = fs::read(&path)?;
        let index: Index = serde_json::from_slice(&data)
            .map_err(|e| anyhow::anyhow!("index parse failed: {}", e))?;
        Ok(index)
    }

    fn update_index(&self, snapshot: &Snapshot) -> anyhow::Result<()> {
        let mut index = self.load_index()?;

        // Remove existing entry if present
        index.entries.retain(|e| e.name != snapshot.name);

        // Add new entry
        index.entries.push(IndexEntry {
            name: snapshot.name.clone(),
            created_at: snapshot.created_at.to_rfc3339(),
            scope: snapshot.scope.to_string(),
            node_count: snapshot.nodes.len(),
            edge_count: snapshot.edges.len(),
        });

        let data = serde_json::to_vec_pretty(&index)
            .map_err(|e| anyhow::anyhow!("index serialize failed: {}", e))?;
        fs::write(self.index_path(), data)?;
        Ok(())
    }

    fn remove_from_index(&self, name: &str) -> anyhow::Result<()> {
        let mut index = self.load_index()?;
        index.entries.retain(|e| e.name != name);
        let data = serde_json::to_vec_pretty(&index)?;
        fs::write(self.index_path(), data)?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_parsing_valid() {
        assert_eq!("workspace".parse::<SnapshotScope>().unwrap(), SnapshotScope::Workspace);
        assert_eq!("crate".parse::<SnapshotScope>().unwrap(), SnapshotScope::Crate);
        assert_eq!("file".parse::<SnapshotScope>().unwrap(), SnapshotScope::File);
    }

    #[test]
    fn test_scope_parsing_invalid() {
        assert!("invalid".parse::<SnapshotScope>().is_err());
    }

    #[test]
    fn test_scope_display() {
        assert_eq!(SnapshotScope::Workspace.to_string(), "workspace");
        assert_eq!(SnapshotScope::Crate.to_string(), "crate");
        assert_eq!(SnapshotScope::File.to_string(), "file");
    }
}