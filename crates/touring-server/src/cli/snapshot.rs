//! `touring snapshot create|list|diff|status` — Graph snapshot management.
//!
//! Captures the current graph state, stores it as JSON, and provides
//! diff between any two snapshots.
//!
//! ## Snapshot storage
//! Snapshots are stored as JSON files in `~/.claude/touring/snapshots/`.
//! Each snapshot contains:
//!   - name, timestamp, project_root
//!   - graph metadata (nodes, edges, community info)
//!   - wiring metrics (orphan count, integration scores)
//!   - index stats (symbol count, indexed files)
//!
//! Wave P3-1.3 W5a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The dispatch contract is unchanged — `run` still
//! receives the full argv slice; clap parses `args[1..]`.
//!
//! G6 payload byte-compatible: every `daemon_query` call keeps exactly the
//! same hook name and JSON keys as the original implementation.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring snapshot",
    bin_name = "touring snapshot",
    about = "Graph snapshot management: list (default), create, diff, status",
    disable_help_subcommand = true
)]
struct SnapshotCli {
    #[command(subcommand)]
    cmd: Option<SnapshotCmd>,
}

#[derive(Subcommand, Debug)]
enum SnapshotCmd {
    /// Capture the current graph state as a named snapshot.
    Create {
        /// Snapshot name (alphanumeric and dashes only).
        name: String,
    },
    /// List all available snapshots (default).
    List,
    /// Show a read-only preview of the current graph data without persisting.
    Status,
    /// Diff two snapshots.
    Diff {
        /// First snapshot name.
        name_a: String,
        /// Second snapshot name.
        name_b: String,
    },
}

/// Snapshot directory.
fn snapshot_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/gabrielgadea".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join("touring")
        .join("snapshots")
}

/// Ensure snapshot directory exists.
fn ensure_snapshot_dir() -> anyhow::Result<PathBuf> {
    let dir = snapshot_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Snapshot metadata + graph data.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotData {
    /// User-provided name.
    pub name: String,
    /// ISO-8601 creation timestamp.
    pub timestamp: String,
    /// Project root when snapshot was taken.
    pub project_root: String,
    /// Total node count.
    pub node_count: usize,
    /// Total edge count.
    pub edge_count: usize,
    /// Community count if available.
    pub community_count: usize,
    /// Orphan symbol count.
    pub orphan_count: usize,
    /// Index symbol count.
    pub index_symbol_count: usize,
    /// Integration score average (0.0-1.0).
    pub integration_score_avg: f64,
    /// Full graph nodes as JSON strings (serialized GraphNode).
    pub nodes_json: Vec<String>,
    /// Full graph edges as JSON strings (serialized GraphEdge).
    pub edges_json: Vec<String>,
}

impl SnapshotData {
    /// Load a snapshot by name.
    fn load(name: &str) -> anyhow::Result<Self> {
        let path = snapshot_dir().join(format!("{name}.json"));
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("failed to parse snapshot '{}': {}", name, e))
    }

    /// Save snapshot to disk.
    fn save(&self) -> anyhow::Result<PathBuf> {
        let dir = ensure_snapshot_dir()?;
        let path = dir.join(format!("{}.json", self.name));
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(path)
    }
}

/// Collect graph metadata from the daemon — bundles 4 daemon queries
/// (wiring_status, wiring_modules, index_status, ast_blast on root) into
/// a single JSON envelope. Used by `run_create` (avoiding duplicated
/// daemon round-trips) and by `run_status` (read-only snapshot preview
/// without persisting to disk).
pub fn collect_graph_data() -> anyhow::Result<serde_json::Value> {
    let wiring_status = daemon_query("cli-wiring-status", serde_json::json!({}))?;
    let wiring_modules = daemon_query("cli-wiring-modules", serde_json::json!({}))?;
    let index_status = daemon_query("cli-index-status", serde_json::json!({}))?;
    let graph_communities = daemon_query("cli-ast-blast", serde_json::json!({"file_path": ""}))?;

    let wiring: serde_json::Value = serde_json::from_str(&wiring_status).unwrap_or_default();
    let modules: serde_json::Value = serde_json::from_str(&wiring_modules).unwrap_or_default();
    let index: serde_json::Value = serde_json::from_str(&index_status).unwrap_or_default();
    let graph: serde_json::Value = serde_json::from_str(&graph_communities).unwrap_or_default();

    Ok(serde_json::json!({
        "wiring": wiring,
        "modules": modules,
        "index": index,
        "graph": graph,
    }))
}

/// Extract a u64 field from JSON, returning 0 if missing.
fn extract_u64_field(value: &serde_json::Value, key: &str) -> usize {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as usize
}

/// Extract an f64 field from JSON, returning 0.0 if missing.
fn extract_f64_field(value: &serde_json::Value, key: &str) -> f64 {
    value.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

/// Extract array length from a JSON field, returning 0 if missing or not an array.
fn extract_array_len(value: &serde_json::Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

/// Compute average integration score from modules array. Uses
/// `extract_f64_field` per-element to keep the f64 coercion logic
/// centralized (single source of truth for "missing → 0.0").
fn compute_integration_avg(modules: &serde_json::Value) -> f64 {
    modules
        .get("modules")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let scores: Vec<f64> = arr
                .iter()
                .map(|m| extract_f64_field(m, "integration_score"))
                .filter(|s| *s != 0.0 || arr.iter().any(|m| m.get("integration_score").is_some()))
                .collect();
            if scores.is_empty() {
                0.0
            } else {
                scores.iter().sum::<f64>() / scores.len() as f64
            }
        })
        .unwrap_or(0.0)
}

/// Serialize JSON array nodes to a `Vec<String>`.
fn extract_json_strings(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|n| serde_json::to_string(n).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `snapshot create <name>` — capture current graph state.
fn run_create(name: &str) -> anyhow::Result<()> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        anyhow::bail!("invalid snapshot name: use alphanumeric and dashes only");
    }

    let project_root = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let timestamp = chrono_lite_timestamp();

    // Single envelope replaces 4 inline daemon_query calls — keeps schema
    // canonical and exposes the same JSON shape to `snapshot status`.
    let envelope = collect_graph_data()?;
    let wiring = envelope.get("wiring").cloned().unwrap_or_default();
    let modules = envelope.get("modules").cloned().unwrap_or_default();
    let index = envelope.get("index").cloned().unwrap_or_default();
    let graph = envelope.get("graph").cloned().unwrap_or_default();

    // Extract counts using helpers
    let orphan_count = extract_u64_field(&wiring, "orphan_count");
    let index_symbol_count = extract_u64_field(&index, "symbol_count");
    let node_count = extract_array_len(&graph, "nodes");
    let edge_count = extract_array_len(&graph, "edges");
    let community_count = extract_array_len(&modules, "communities");
    let integration_score_avg = compute_integration_avg(&modules);

    // Serialize nodes/edges as JSON strings
    let nodes_json = extract_json_strings(&graph, "nodes");
    let edges_json = extract_json_strings(&graph, "edges");

    let snapshot = SnapshotData {
        name: name.to_string(),
        timestamp: timestamp.clone(),
        project_root: project_root.clone(),
        node_count,
        edge_count,
        community_count,
        orphan_count,
        index_symbol_count,
        integration_score_avg,
        nodes_json,
        edges_json,
    };

    let path = snapshot.save()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "snapshot_created": name,
            "path": path.to_string_lossy(),
            "timestamp": timestamp,
            "node_count": node_count,
            "edge_count": edge_count,
            "orphan_count": orphan_count,
            "index_symbol_count": index_symbol_count,
            "integration_score_avg": integration_score_avg,
        }))?
    );

    Ok(())
}

/// `snapshot list` — list all available snapshots.
fn run_list() -> anyhow::Result<()> {
    let dir = snapshot_dir();
    if !dir.exists() {
        println!("[]");
        return Ok(());
    }

    let mut snapshots: Vec<serde_json::Value> = Vec::new();

    let entries = fs::read_dir(&dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && let Ok(content) = fs::read_to_string(&path)
            && let Ok(data) = serde_json::from_str::<SnapshotData>(&content)
        {
            snapshots.push(serde_json::json!({
                "name": data.name,
                "timestamp": data.timestamp,
                "project_root": data.project_root,
                "node_count": data.node_count,
                "edge_count": data.edge_count,
                "orphan_count": data.orphan_count,
                "index_symbol_count": data.index_symbol_count,
                "integration_score_avg": data.integration_score_avg,
            }));
        }
    }

    // Sort by timestamp descending (newest first)
    snapshots.sort_by(|a, b| {
        let ts_a = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let ts_b = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        ts_b.cmp(ts_a)
    });

    println!("{}", serde_json::to_string_pretty(&snapshots)?);
    Ok(())
}

/// `snapshot diff <name_a> <name_b>` — diff two snapshots.
fn run_diff(name_a: &str, name_b: &str) -> anyhow::Result<()> {
    if name_a.is_empty() || name_b.is_empty() {
        anyhow::bail!("usage: touring snapshot diff <name_a> <name_b>");
    }

    let snap_a = SnapshotData::load(name_a)?;
    let snap_b = SnapshotData::load(name_b)?;

    #[derive(serde::Serialize)]
    struct Diff {
        name_a: String,
        name_b: String,
        timestamp_a: String,
        timestamp_b: String,
        delta_node_count: i64,
        delta_edge_count: i64,
        delta_orphan_count: i64,
        delta_index_symbol_count: i64,
        delta_integration_score_avg: f64,
        nodes_added: Vec<String>,
        nodes_removed: Vec<String>,
        edges_added: Vec<String>,
        edges_removed: Vec<String>,
    }

    let set_a_nodes: HashSet<_> = snap_a.nodes_json.iter().collect();
    let set_b_nodes: HashSet<_> = snap_b.nodes_json.iter().collect();
    let set_a_edges: HashSet<_> = snap_a.edges_json.iter().collect();
    let set_b_edges: HashSet<_> = snap_b.edges_json.iter().collect();

    let nodes_added: Vec<_> = set_b_nodes
        .difference(&set_a_nodes)
        .cloned()
        .cloned()
        .collect();
    let nodes_removed: Vec<_> = set_a_nodes
        .difference(&set_b_nodes)
        .cloned()
        .cloned()
        .collect();
    let edges_added: Vec<_> = set_b_edges
        .difference(&set_a_edges)
        .cloned()
        .cloned()
        .collect();
    let edges_removed: Vec<_> = set_a_edges
        .difference(&set_b_edges)
        .cloned()
        .cloned()
        .collect();

    let diff = Diff {
        name_a: snap_a.name,
        name_b: snap_b.name,
        timestamp_a: snap_a.timestamp,
        timestamp_b: snap_b.timestamp,
        delta_node_count: snap_b.node_count as i64 - snap_a.node_count as i64,
        delta_edge_count: snap_b.edge_count as i64 - snap_a.edge_count as i64,
        delta_orphan_count: snap_b.orphan_count as i64 - snap_a.orphan_count as i64,
        delta_index_symbol_count: snap_b.index_symbol_count as i64
            - snap_a.index_symbol_count as i64,
        delta_integration_score_avg: snap_b.integration_score_avg - snap_a.integration_score_avg,
        nodes_added,
        nodes_removed,
        edges_added,
        edges_removed,
    };

    println!("{}", serde_json::to_string_pretty(&diff)?);
    Ok(())
}

/// Lightweight timestamp (no chrono dep needed).
fn chrono_lite_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let mins = (remaining % 3600) / 60;
    let secs_in_day = remaining % 60;
    // Approximate date from Unix epoch (2026-05-01 is ~17600 days from 1970-01-01)
    let year = 1970 + (days / 365);
    let leap_days = (days / 146097) * 4 + (days % 146097) / 36524;
    let yday = days % 365 - leap_days;
    let month = (12 * yday + 5) / 367;
    let mday = yday - (367 * month - 367) / 12 + 1;
    let month = if month > 11 { 11 } else { month };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month + 1,
        mday,
        hours,
        mins,
        secs_in_day
    )
}

/// `snapshot status` — read-only preview of current graph data without
/// persisting to disk. Useful for inspecting daemon state before deciding
/// whether to create a snapshot.
fn run_status() -> anyhow::Result<()> {
    let envelope = collect_graph_data()?;
    let wiring = envelope.get("wiring").cloned().unwrap_or_default();
    let modules = envelope.get("modules").cloned().unwrap_or_default();
    let index = envelope.get("index").cloned().unwrap_or_default();
    let graph = envelope.get("graph").cloned().unwrap_or_default();

    let preview = serde_json::json!({
        "orphan_count": extract_u64_field(&wiring, "orphan_count"),
        "index_symbol_count": extract_u64_field(&index, "symbol_count"),
        "node_count": extract_array_len(&graph, "nodes"),
        "edge_count": extract_array_len(&graph, "edges"),
        "community_count": extract_array_len(&modules, "communities"),
        "integration_score_avg": compute_integration_avg(&modules),
    });

    println!("{}", serde_json::to_string_pretty(&preview)?);
    Ok(())
}

/// Entry point for `touring snapshot <subcommand>`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match SnapshotCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(SnapshotCmd::List) {
        SnapshotCmd::Create { name } => run_create(&name),
        SnapshotCmd::List => run_list(),
        SnapshotCmd::Status => run_status(),
        SnapshotCmd::Diff { name_a, name_b } => run_diff(&name_a, &name_b),
    }
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    SnapshotCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> SnapshotCli {
        SnapshotCli::try_parse_from(args).expect("args should parse")
    }

    // ── defaults ──────────────────────────────────────────────────────────────

    #[test]
    fn bare_snapshot_defaults_to_list() {
        let cli = parse(&["snapshot"]);
        assert!(cli.cmd.is_none()); // run() maps None -> List
    }

    // ── subcommand parsing ────────────────────────────────────────────────────

    #[test]
    fn create_parses_name() {
        let cli = parse(&["snapshot", "create", "my-snap"]);
        let Some(SnapshotCmd::Create { name }) = cli.cmd else {
            panic!("expected Create");
        };
        assert_eq!(name, "my-snap");
    }

    #[test]
    fn create_requires_name() {
        // clap reports error when required positional is missing
        assert!(SnapshotCli::try_parse_from(["snapshot", "create"]).is_err());
    }

    #[test]
    fn list_subcommand_parses() {
        let cli = parse(&["snapshot", "list"]);
        assert!(matches!(cli.cmd, Some(SnapshotCmd::List)));
    }

    #[test]
    fn status_subcommand_parses() {
        let cli = parse(&["snapshot", "status"]);
        assert!(matches!(cli.cmd, Some(SnapshotCmd::Status)));
    }

    #[test]
    fn diff_parses_two_names() {
        let cli = parse(&["snapshot", "diff", "snap-a", "snap-b"]);
        let Some(SnapshotCmd::Diff { name_a, name_b }) = cli.cmd else {
            panic!("expected Diff");
        };
        assert_eq!(name_a, "snap-a");
        assert_eq!(name_b, "snap-b");
    }

    #[test]
    fn diff_requires_both_names_via_clap() {
        assert!(SnapshotCli::try_parse_from(["snapshot", "diff", "snap-a"]).is_err());
        assert!(SnapshotCli::try_parse_from(["snapshot", "diff"]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(SnapshotCli::try_parse_from(["snapshot", "frobnicate"]).is_err());
    }

    // ── pure-logic tests (preserved from original) ────────────────────────────

    #[test]
    fn snapshot_dir_path() {
        let dir = snapshot_dir();
        assert!(dir.to_string_lossy().contains(".claude"));
        assert!(dir.to_string_lossy().contains("touring"));
        assert!(dir.to_string_lossy().contains("snapshots"));
    }

    #[test]
    fn timestamp_format() {
        let ts = chrono_lite_timestamp();
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
        assert_eq!(ts.len(), 20); // YYYY-MM-DDTHH:MM:SSZ
    }

    #[test]
    fn snapshot_data_serde_roundtrip() {
        let data = SnapshotData {
            name: "test-snap".to_string(),
            timestamp: "2026-05-01T12:00:00Z".to_string(),
            project_root: "/home/gabrielgadea".to_string(),
            node_count: 10,
            edge_count: 20,
            community_count: 3,
            orphan_count: 5,
            index_symbol_count: 1000,
            integration_score_avg: 0.85,
            nodes_json: vec!["{\"id\":\"a\"}".to_string()],
            edges_json: vec!["{\"from\":\"a\",\"to\":\"b\"}".to_string()],
        };
        let json = serde_json::to_string(&data).expect("serialization should work");
        let loaded: SnapshotData =
            serde_json::from_str(&json).expect("deserialization should work");
        assert_eq!(loaded.name, "test-snap");
        assert_eq!(loaded.node_count, 10);
        assert_eq!(loaded.nodes_json.len(), 1);
    }

    #[test]
    fn invalid_snapshot_name_rejected() {
        // Names with slashes should be rejected
        let result = run_create("foo/bar");
        assert!(result.is_err());
        let result = run_create("foo\\bar");
        assert!(result.is_err());
    }

    #[test]
    fn diff_requires_both_names() {
        let result = run_diff("", "b");
        assert!(result.is_err());
        let result = run_diff("a", "");
        assert!(result.is_err());
    }
}
