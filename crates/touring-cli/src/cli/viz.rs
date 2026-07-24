//! CLI viz handlers (`cli_viz_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::cli_handlers::{VizEdgeData, VizGraphData, VizNodeData};
use crate::runtime::HookRuntime;

/// Handler for `cli-viz-workspace` — full crate dependency graph.
pub fn cli_viz_workspace(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let graph = build_wiring_graph_data(db);
    serde_json::to_string(&graph).unwrap_or_default()
}
/// Handler for `cli-viz-cycles` — dependency cycles as graph.
pub fn cli_viz_cycles(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use crate::wiring::find_all_cycles;
    let min_depth = payload
        .get("min_depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(2) as usize;
    // PLT-2026-06-02: also respect `workspace_root` filter (back-compat: None).
    let workspace_root_filter: Option<String> = payload
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let all_cycles = find_all_cycles(&rt.ctx.knowledge, workspace_root_filter.as_deref(), false);
    let cycles: Vec<_> = all_cycles
        .into_iter()
        .filter(|c| c.depth >= min_depth)
        .collect();
    let mut nodes: Vec<VizNodeData> = Vec::new();
    let mut edges: Vec<VizEdgeData> = Vec::new();
    let mut seen_nodes: std::collections::HashSet<String> = std::collections::HashSet::new();
    for cycle in &cycles {
        for (i, module) in cycle.modules.iter().enumerate() {
            if seen_nodes.insert(module.clone()) {
                nodes.push(VizNodeData {
                    id: module.clone(),
                    label: std::path::Path::new(module)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| module.clone()),
                    quality_score: None,
                    fan_in: None,
                    fan_out: None,
                    is_orphan: false,
                    has_unsafe: false,
                });
            }
            if i < cycle.modules.len() - 1 {
                edges.push(VizEdgeData {
                    from: module.clone(),
                    to: cycle.modules[i + 1].clone(),
                    kind: "cycles".to_string(),
                });
            } else {
                edges.push(VizEdgeData {
                    from: module.clone(),
                    to: cycle.modules[0].clone(),
                    kind: "cycles".to_string(),
                });
            }
        }
    }
    let graph = VizGraphData { nodes, edges };
    serde_json::to_string(&graph).unwrap_or_default()
}
/// Handler for `cli-viz-orphans` — orphan symbols only.
pub fn cli_viz_orphans(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let orphans: Vec<(String, String)> = {
        let stmt_opt = db.conn_ref().prepare(
            "SELECT module_file, symbol_name FROM wiring_map
             WHERE module_file IS NOT NULL
               AND consumer_file IS NULL",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    let mut seen_modules: std::collections::HashSet<String> = std::collections::HashSet::new();
    let nodes: Vec<VizNodeData> = orphans
        .iter()
        .filter(|(module, _)| seen_modules.insert(module.clone()))
        .map(|(module, _)| VizNodeData {
            id: module.clone(),
            label: std::path::Path::new(module)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| module.clone()),
            quality_score: None,
            fan_in: Some(0),
            fan_out: Some(0),
            is_orphan: true,
            has_unsafe: false,
        })
        .collect();
    let graph = VizGraphData {
        nodes,
        edges: Vec::new(),
    };
    serde_json::to_string(&graph).unwrap_or_default()
}
/// Handler for `cli-viz-blast` — blast radius for a symbol.
pub fn cli_viz_blast(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    if symbol.is_empty() {
        return serde_json::json!({ "error" : "symbol name required for blast radius" })
            .to_string();
    }
    let module_file: Option<String> = {
        let stmt_opt = rt.ctx.knowledge.conn_ref().prepare(
            "SELECT DISTINCT module_file FROM wiring_map
             WHERE symbol_name = ? AND module_file IS NOT NULL",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([symbol], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).next())
                .unwrap_or(None)
        } else {
            None
        }
    };
    let Some(start_module) = module_file else {
        return serde_json::json!(
            { "error" : format!("symbol '{}' not found in wiring graph", symbol) }
        )
        .to_string();
    };
    use std::collections::{HashMap, HashSet, VecDeque};
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    let all_edges: Vec<(String, String)> = {
        let stmt_opt = rt.ctx.knowledge.conn_ref().prepare(
            "SELECT DISTINCT module_file, consumer_file FROM wiring_map
             WHERE module_file IS NOT NULL
               AND consumer_file IS NOT NULL
               AND module_file != consumer_file",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    for (m, c) in all_edges {
        adjacency.entry(m).or_default().push(c);
    }
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut nodes: Vec<VizNodeData> = Vec::new();
    let mut edges: Vec<VizEdgeData> = Vec::new();
    let mut fan_in_counts: HashMap<String, usize> = HashMap::new();
    let mut fan_out_counts: HashMap<String, usize> = HashMap::new();
    for (m, c) in adjacency.iter() {
        *fan_out_counts.entry(m.clone()).or_insert(0) += c.len();
        for consumer in c {
            *fan_in_counts.entry(consumer.clone()).or_insert(0) += 1;
        }
    }
    queue.push_back((start_module.clone(), 0));
    visited.insert(start_module.clone());
    while let Some((current, depth)) = queue.pop_front() {
        nodes.push(VizNodeData {
            id: current.clone(),
            label: std::path::Path::new(&current)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| current.clone()),
            quality_score: None,
            fan_in: fan_in_counts.get(&current).copied(),
            fan_out: fan_out_counts.get(&current).copied(),
            is_orphan: false,
            has_unsafe: false,
        });
        if let Some(next_modules) = adjacency.get(&current) {
            for next in next_modules {
                if visited.insert(next.clone()) {
                    queue.push_back((next.clone(), depth + 1));
                }
                edges.push(VizEdgeData {
                    from: current.clone(),
                    to: next.clone(),
                    kind: "imports".to_string(),
                });
            }
        }
    }
    let graph = VizGraphData { nodes, edges };
    serde_json::to_string(&graph).unwrap_or_default()
}
/// Handler for `cli-viz-wiring` — wiring/consumer graph (same as workspace).
pub fn cli_viz_wiring(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let graph = build_wiring_graph_data(db);
    serde_json::to_string(&graph).unwrap_or_default()
}
/// Handler for `cli-viz-feature` — symbols gated by a feature.
pub fn cli_viz_feature(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let feature = payload
        .get("feature")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if feature.is_empty() {
        return serde_json::json!(
            { "error" : "feature name required for feature gate graph" }
        )
        .to_string();
    }
    let raw_modules: Vec<String> = {
        let stmt_opt = rt.ctx.knowledge.conn_ref().prepare(
            "SELECT DISTINCT module_file FROM wiring_map
             WHERE module_file IS NOT NULL
               AND consumer_file IS NOT NULL
               AND module_file != consumer_file",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    let modules: Vec<String> = raw_modules
        .into_iter()
        .filter(|m| {
            m.contains(&format!("cfg(feature = \"{}\")", feature)) || m.contains("cfg(features = ")
        })
        .collect();
    let all_modules: Vec<String> = if modules.is_empty() {
        let stmt_opt =
            rt.ctx.knowledge.conn_ref().prepare(
                "SELECT DISTINCT module_file FROM wiring_map WHERE module_file IS NOT NULL",
            );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        modules.clone()
    };
    let nodes: Vec<VizNodeData> = all_modules
        .iter()
        .map(|m: &String| VizNodeData {
            id: m.clone(),
            label: std::path::Path::new(m)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| m.clone()),
            quality_score: None,
            fan_in: None,
            fan_out: None,
            is_orphan: false,
            has_unsafe: false,
        })
        .collect();
    let graph = VizGraphData {
        nodes,
        edges: Vec::new(),
    };
    serde_json::to_string(&graph).unwrap_or_default()
}
/// Build GraphData from the wiring_map table.
/// Nodes = DISTINCT module_file entries.
/// Edges = (module_file, consumer_file) pairs where module_file != consumer_file.
fn build_wiring_graph_data(db: &crate::knowledge::FileKnowledgeDB) -> VizGraphData {
    use std::collections::{HashMap, HashSet};
    let mut edges_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut fan_in_counts: HashMap<String, usize> = HashMap::new();
    let mut fan_out_counts: HashMap<String, usize> = HashMap::new();
    let mut all_nodes: HashSet<String> = HashSet::new();
    let mut orphan_candidates: HashSet<String> = HashSet::new();
    let rows: Vec<(String, String)> = {
        let stmt_opt = db.conn_ref().prepare(
            "SELECT DISTINCT module_file, consumer_file FROM wiring_map
             WHERE module_file IS NOT NULL
               AND consumer_file IS NOT NULL
               AND module_file != consumer_file",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    for (module, consumer) in &rows {
        all_nodes.insert(module.clone());
        all_nodes.insert(consumer.clone());
        edges_map
            .entry(module.clone())
            .or_default()
            .push(consumer.clone());
        *fan_out_counts.entry(module.clone()).or_insert(0) += 1;
        *fan_in_counts.entry(consumer.clone()).or_insert(0) += 1;
    }
    let stmt_orphans = db.conn_ref().prepare(
        "SELECT DISTINCT module_file FROM wiring_map
         WHERE module_file IS NOT NULL
           AND consumer_file IS NULL",
    );
    if let Ok(mut stmt) = stmt_orphans {
        if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
            for row in rows.filter_map(|r| r.ok()) {
                orphan_candidates.insert(row);
            }
        }
    }
    let mut all_modules: Vec<String> = all_nodes.into_iter().collect();
    all_modules.sort();
    let nodes: Vec<VizNodeData> = all_modules
        .iter()
        .map(|m| {
            let label = std::path::Path::new(m)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| m.clone());
            VizNodeData {
                id: m.clone(),
                label,
                quality_score: None,
                fan_in: fan_in_counts.get(m).copied(),
                fan_out: fan_out_counts.get(m).copied(),
                is_orphan: orphan_candidates.contains(m),
                has_unsafe: false,
            }
        })
        .collect();
    let edges: Vec<VizEdgeData> = rows
        .iter()
        .map(|(from, to)| VizEdgeData {
            from: from.clone(),
            to: to.clone(),
            kind: "imports".to_string(),
        })
        .collect();
    VizGraphData { nodes, edges }
}
