//! `touring graph snapshot` — Graph snapshot management.
//!
//! Creates, lists, diffs, and deletes named graph snapshots.
//! Snapshots serialize the current graph state for later comparison.
//!
//! ## Subcommands
//! - `create <name>` — Create a new snapshot
//! - `list` — List all snapshots
//! - `delete <name>` — Delete a snapshot
//! - `diff <a> <b>` — Diff two snapshots

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use chrono::{DateTime, Utc};

pub mod store;
pub mod diff;

pub use store::{Snapshot, SnapshotMetadata, SnapshotScope};
pub use diff::SnapshotDiff;

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot list entry for CLI output
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotListEntry {
    pub name: String,
    pub created_at: String,
    pub scope: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub file_path: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// CLI entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Run the snapshot subcommand dispatcher.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("list");

    match subcommand {
        "create" => run_create(args),
        "list" => run_list(args),
        "delete" => run_delete(args),
        "diff" => run_diff(args),
        _ => anyhow::bail!(
            "Unknown snapshot subcommand: '{}'. Use: create, list, delete, diff",
            subcommand
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Create
// ─────────────────────────────────────────────────────────────────────────────

fn run_create(args: &[String]) -> anyhow::Result<()> {
    let name = args.get(3).map(|s| s.as_str()).unwrap_or("default");
    let scope_str = get_flag_value(args, "--scope").unwrap_or("workspace");
    let scope = scope_str.parse().unwrap_or(SnapshotScope::Workspace);

    let snapshot_dir = get_snapshot_dir();
    std::fs::create_dir_all(&snapshot_dir)?;

    let metadata = store::SnapshotStore::new(&snapshot_dir)?;

    // Capture current graph state
    let graph_state = capture_graph_state(scope)?;
    let node_count = graph_state.nodes.len();
    let edge_count = graph_state.edges.len();

    let snapshot = Snapshot {
        name: name.to_string(),
        created_at: Utc::now(),
        scope,
        nodes: graph_state.nodes,
        edges: graph_state.edges,
        metadata: HashMap::new(),
    };

    let file_path = metadata.save(&snapshot)?;
    let entry = SnapshotListEntry {
        name: name.to_string(),
        created_at: snapshot.created_at.to_rfc3339(),
        scope: scope_str.to_string(),
        node_count,
        edge_count,
        file_path,
    };

    println!("{}", serde_json::to_string_pretty(&entry).unwrap_or_default());
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// List
// ─────────────────────────────────────────────────────────────────────────────

fn run_list(_args: &[String]) -> anyhow::Result<()> {
    let snapshot_dir = get_snapshot_dir();
    let store = store::SnapshotStore::new(&snapshot_dir)?;
    let snapshots = store.list()?;

    if snapshots.is_empty() {
        println!("[]");
        return Ok(());
    }

    let entries: Vec<SnapshotListEntry> = snapshots
        .into_iter()
        .map(|s| SnapshotListEntry {
            name: s.name.clone(),
            created_at: s.created_at.to_rfc3339(),
            scope: format!("{:?}", s.scope).to_lowercase(),
            node_count: s.nodes.len(),
            edge_count: s.edges.len(),
            file_path: s.name.clone(), // Will be resolved by store
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&entries).unwrap_or_default());
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Delete
// ─────────────────────────────────────────────────────────────────────────────

fn run_delete(args: &[String]) -> anyhow::Result<()> {
    let name = args.get(3).map(|s| s.as_str()).unwrap_or("");
    if name.is_empty() {
        anyhow::bail!("snapshot name required: touring graph snapshot delete <name>");
    }

    let snapshot_dir = get_snapshot_dir();
    let store = store::SnapshotStore::new(&snapshot_dir)?;
    store.delete(name)?;

    println!("{{\"deleted\": \"{}\"}}", name);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Diff
// ─────────────────────────────────────────────────────────────────────────────

fn run_diff(args: &[String]) -> anyhow::Result<()> {
    let name_a = args.get(3).map(|s| s.as_str()).unwrap_or("");
    let name_b = args.get(4).map(|s| s.as_str()).unwrap_or("");

    if name_a.is_empty() || name_b.is_empty() {
        anyhow::bail!("two snapshot names required: touring graph snapshot diff <a> <b>");
    }

    let snapshot_dir = get_snapshot_dir();
    let store = store::SnapshotStore::new(&snapshot_dir)?;

    let snap_a = store.load(name_a)?;
    let snap_b = store.load(name_b)?;

    let diff = SnapshotDiff::compute(&snap_a, &snap_b);

    let format = get_flag_value(args, "--format").unwrap_or("json");
    match format {
        "dot" => {
            let opts = super::visual::DotOpts::default();
            println!("{}", super::visual::diff_to_dot(&diff));
        }
        "mermaid" => {
            let opts = super::visual::MermaidOpts::default();
            println!("{}", super::visual::diff_to_mermaid(&diff));
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&diff).unwrap_or_default());
        }
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn get_snapshot_dir() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".claude").join("touring").join("snapshots"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/touring-snapshots"))
}

fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).map(|s| s.clone()))
}

/// Capture current graph state based on scope.
fn capture_graph_state(scope: SnapshotScope) -> anyhow::Result<super::visual::GraphData> {
    // For now, capture a minimal graph based on scope.
    // In full implementation, this would call daemon handlers.
    let nodes = match scope {
        SnapshotScope::Workspace => vec![
            super::visual::NodeData {
                id: "workspace".to_string(),
                label: "workspace".to_string(),
                quality_score: Some(0.8),
                fan_in: None,
                fan_out: None,
                is_orphan: false,
                has_unsafe: false,
                is_test: false,
                node_type: None,
            },
        ],
        SnapshotScope::Crate => vec![
            super::visual::NodeData {
                id: "crate".to_string(),
                label: "current-crate".to_string(),
                quality_score: Some(0.75),
                fan_in: None,
                fan_out: None,
                is_orphan: false,
                has_unsafe: false,
                is_test: false,
                node_type: None,
            },
        ],
        SnapshotScope::File => vec![
            super::visual::NodeData {
                id: "file".to_string(),
                label: "current-file".to_string(),
                quality_score: Some(0.7),
                fan_in: None,
                fan_out: None,
                is_orphan: false,
                has_unsafe: false,
                is_test: false,
                node_type: None,
            },
        ],
    };

    Ok(super::visual::GraphData {
        nodes,
        edges: vec![],
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_list_entry_serde() {
        let entry = SnapshotListEntry {
            name: "test".to_string(),
            created_at: "2026-05-01T00:00:00Z".to_string(),
            scope: "workspace".to_string(),
            node_count: 10,
            edge_count: 5,
            file_path: "/path/to/snapshot".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test"));
    }

    #[test]
    fn test_scope_parsing() {
        let scope: SnapshotScope = "workspace".parse().unwrap();
        assert!(matches!(scope, SnapshotScope::Workspace));

        let scope: SnapshotScope = "crate".parse().unwrap();
        assert!(matches!(scope, SnapshotScope::Crate));
    }
}