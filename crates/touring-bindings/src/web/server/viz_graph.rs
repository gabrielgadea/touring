//! Workspace dependency-graph enrichment helpers for the `/api/viz/workspace`
//! endpoint — extracted from `web/server/mod.rs` (F-9). All `pub(super)` so the
//! parent module glob-imports them; the viz handlers call them unchanged.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

pub(super) fn normalize_and_deduplicate_nodes(graph: &mut Value, workspace_root: &std::path::Path) {
    if let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_array_mut) {
        let mut seen = std::collections::HashMap::new();
        let mut new_nodes: Vec<Value> = Vec::new();
        for mut n in std::mem::take(nodes) {
            if let Some(id_val) = n.get("id").and_then(Value::as_str) {
                let mut id = id_val.to_string();
                if id.starts_with("crates/") {
                    id = workspace_root.join(&id).to_string_lossy().to_string();
                    if let Some(obj) = n.as_object_mut() {
                        obj.insert("id".into(), Value::String(id.clone()));
                        if obj.get("crate").and_then(Value::as_str) == Some("external") {
                            obj.insert("crate".into(), Value::String(crate_of(&id)));
                        }
                    }
                }
                if let Some(existing_idx) = seen.get(&id) {
                    let existing: &mut serde_json::Value = &mut new_nodes[*existing_idx];
                    if let (Some(e_obj), Some(n_obj)) = (existing.as_object_mut(), n.as_object()) {
                        let mut n_fi = 0;
                        if let Some(v) = n_obj.get("fan_in")
                            && let Some(num) = v.as_u64()
                        {
                            n_fi = num;
                        }
                        let mut e_fi = 0;
                        if let Some(v) = e_obj.get("fan_in")
                            && let Some(num) = v.as_u64()
                        {
                            e_fi = num;
                        }
                        if e_fi + n_fi > 0 {
                            e_obj.insert("fan_in".into(), Value::Number((e_fi + n_fi).into()));
                        }

                        let mut n_fo = 0;
                        if let Some(v) = n_obj.get("fan_out")
                            && let Some(num) = v.as_u64()
                        {
                            n_fo = num;
                        }
                        let mut e_fo = 0;
                        if let Some(v) = e_obj.get("fan_out")
                            && let Some(num) = v.as_u64()
                        {
                            e_fo = num;
                        }
                        if e_fo + n_fo > 0 {
                            e_obj.insert("fan_out".into(), Value::Number((e_fo + n_fo).into()));
                        }

                        let mut n_orphan = true;
                        if let Some(v) = n_obj.get("is_orphan")
                            && let Some(b) = v.as_bool()
                        {
                            n_orphan = b;
                        }
                        let mut e_orphan = true;
                        if let Some(v) = e_obj.get("is_orphan")
                            && let Some(b) = v.as_bool()
                        {
                            e_orphan = b;
                        }
                        e_obj.insert("is_orphan".into(), Value::Bool(e_orphan && n_orphan));
                    }
                } else {
                    seen.insert(id.clone(), new_nodes.len());
                    new_nodes.push(n);
                }
            }
        }
        *nodes = new_nodes;
    }
}

/// Walk every `crates/<X>/src/**/*.rs` under `workspace_root` and append
/// any file not already present in the graph's `nodes` array as a bare
/// node (no edges). Each appended node carries the deterministic shape
/// used by the dashboard renderer:
///
/// ```text
/// { id, label, quality_score:null, fan_in:null, fan_out:null,
///   is_orphan:true, has_unsafe:false, is_test:<heuristic> }
/// ```
///
/// `is_test` is derived from the path: any segment named `tests`, any
/// filename starting with `test_` or ending with `_test.rs` is a test
/// file. When `include_tests=false`, test files are *omitted* from the
/// merge (they may still be present from the upstream CLI output).
pub(super) fn enrich_workspace_graph(
    graph: &mut Value,
    workspace_root: &std::path::Path,
    include_tests: bool,
) {
    let crates_dir = workspace_root.join("crates");
    if !crates_dir.exists() {
        return;
    }

    let known: std::collections::HashSet<String> = graph
        .get("nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.get("id").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut additions: Vec<Value> = Vec::new();
    let mut stack = vec![crates_dir.clone()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip target/, .git/, and other noise.
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if matches!(name, "target" | ".git" | "node_modules" | "dist" | "vendor") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let id = path.to_string_lossy().to_string();
            if known.contains(&id) {
                continue;
            }
            let label = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let path_str = id.as_str();
            let is_test = path_str.contains("/tests/")
                || path_str.contains("/benches/")
                || label.starts_with("test_")
                || label.ends_with("_test.rs")
                || label == "tests.rs";
            if !include_tests && is_test {
                continue;
            }
            additions.push(serde_json::json!({
                "id":            id,
                "label":         label,
                "crate":         crate_of(&id),
                "quality_score": Value::Null,
                "fan_in":        Value::Null,
                "fan_out":       Value::Null,
                "is_orphan":     true,
                "has_unsafe":    false,
                "is_test":       is_test,
            }));
        }
    }

    if let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_array_mut) {
        // Backfill `crate` on nodes that came from the upstream CLI without it.
        for n in nodes.iter_mut() {
            if n.get("crate").is_none()
                && let Some(id) = n.get("id").and_then(Value::as_str)
            {
                let cn = crate_of(id);
                if let Some(obj) = n.as_object_mut() {
                    obj.insert("crate".into(), Value::String(cn));
                }
            }
        }
        nodes.extend(additions);
    }
}

/// Extract the workspace crate name from a path like
/// `/.../rust/crates/touring-web/src/lib.rs` → `"touring-web"`. Returns
/// "external" for anything outside `crates/<X>/`.
pub(super) fn crate_of(path: &str) -> String {
    if let Some(idx) = path.find("/crates/") {
        let after = &path[idx + "/crates/".len()..];
        if let Some(end) = after.find('/') {
            return after[..end].to_string();
        }
    }
    "external".into()
}

/// Tag every existing edge with `kind: "import"` (file-level wiring) and
/// compute `isCrossCrate` from the source/target node crate fields.
/// Edges that came from the CLI may not have a kind — we backfill it
/// without overwriting any existing taxonomy.
pub(super) fn tag_existing_edges(graph: &mut Value, workspace_root: &std::path::Path) {
    let crate_by_id: std::collections::HashMap<String, String> = graph
        .get("nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|n| {
                    let id = n.get("id").and_then(Value::as_str)?;
                    let cn = n.get("crate").and_then(Value::as_str).unwrap_or("external");
                    Some((id.to_string(), cn.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(edges) = graph.get_mut("edges").and_then(Value::as_array_mut) {
        for e in edges.iter_mut() {
            let mut from = e
                .get("from")
                .or_else(|| e.get("source"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut to = e
                .get("to")
                .or_else(|| e.get("target"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            if from.starts_with("crates/") {
                from = workspace_root.join(&from).to_string_lossy().to_string();
            }
            if to.starts_with("crates/") {
                to = workspace_root.join(&to).to_string_lossy().to_string();
            }

            let from_crate = crate_by_id
                .get(&from)
                .cloned()
                .unwrap_or_else(|| crate_of(&from));
            let to_crate = crate_by_id
                .get(&to)
                .cloned()
                .unwrap_or_else(|| crate_of(&to));
            let cross = from_crate != to_crate;
            if let Some(obj) = e.as_object_mut() {
                obj.insert("from".into(), Value::String(from.clone()));
                obj.insert("to".into(), Value::String(to.clone()));
                obj.entry("kind".to_string())
                    .or_insert_with(|| Value::String("import".into()));
                obj.insert("isCrossCrate".into(), Value::Bool(cross));
                obj.insert("from_crate".into(), Value::String(from_crate));
                obj.insert("to_crate".into(), Value::String(to_crate));
            }
        }
    }
}

/// Improve labels for common filenames like mod.rs, lib.rs, and main.rs
/// to include their parent directory (e.g. "blast_radius/mod.rs").
pub(super) fn refine_node_labels(graph: &mut Value) {
    if let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_array_mut) {
        for n in nodes.iter_mut() {
            if let Some(id) = n.get("id").and_then(Value::as_str)
                && (id.ends_with("/mod.rs") || id.ends_with("/lib.rs") || id.ends_with("/main.rs"))
            {
                let p = std::path::Path::new(id);
                if let Some(parent) = p
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                {
                    let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    let new_label = format!("{}/{}", parent, file_name);
                    if let Some(obj) = n.as_object_mut() {
                        obj.insert("label".into(), Value::String(new_label));
                    }
                }
            }
        }
    }
}

/// Add cargo workspace member-to-member dependency edges so the graph
/// shows the architectural skeleton, not just file-level imports. Edges
/// run between each crate's anchor file (`src/lib.rs` if present, else
/// `src/main.rs`, else any first .rs file in the crate).
///
/// Source: `touring ast workspace-info` (cargo_metadata) — fields
/// `packages[*].{name, manifest_path, dependencies[]}`. We filter
/// `dependencies` to only keep names that match other workspace members.
pub(super) fn enrich_crate_deps(graph: &mut Value, workspace_root: &std::path::Path) {
    // Probe cargo metadata via touring CLI (already in PATH).
    let output = std::process::Command::new("touring")
        .current_dir(workspace_root)
        .args(["ast", "workspace-info"])
        .output();
    let info: Value = match output {
        Ok(o) if o.status.success() => match serde_json::from_slice(&o.stdout) {
            Ok(v) => v,
            Err(_) => return,
        },
        _ => return,
    };
    let packages = match info.get("packages").and_then(Value::as_array) {
        Some(p) => p,
        None => return,
    };

    // Build set of workspace member names + map name → anchor file path.
    let workspace_members: std::collections::HashSet<String> = packages
        .iter()
        .filter(|p| {
            p.get("is_workspace_member")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|p| p.get("name").and_then(Value::as_str).map(String::from))
        .collect();

    // Map of node ids by crate, picking an anchor file (lib.rs > main.rs > first).
    let nodes = match graph.get("nodes").and_then(Value::as_array) {
        Some(n) => n,
        None => return,
    };
    let mut anchors: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for n in nodes {
        let id = n.get("id").and_then(Value::as_str).unwrap_or("");
        let cn = n.get("crate").and_then(Value::as_str).unwrap_or("external");
        if cn == "external" {
            continue;
        }
        let prio = if id.ends_with("/src/lib.rs") {
            0
        } else if id.ends_with("/src/main.rs") {
            1
        } else if id.ends_with("/lib.rs") {
            2
        } else if id.ends_with("/main.rs") {
            3
        } else if id.ends_with("/mod.rs") {
            4
        } else {
            9
        };
        let entry = anchors
            .entry(cn.to_string())
            .or_insert_with(|| id.to_string());
        if prio < anchor_prio(entry) {
            *entry = id.to_string();
        }
    }

    // Collect new crate-dep edges.
    let mut new_edges: Vec<Value> = Vec::new();
    for pkg in packages {
        let name = match pkg.get("name").and_then(Value::as_str) {
            Some(n) => n,
            _ => continue,
        };
        if !workspace_members.contains(name) {
            continue;
        }
        let from_anchor = match anchors.get(name) {
            Some(a) => a.clone(),
            _ => continue,
        };
        let deps = match pkg.get("dependencies").and_then(Value::as_array) {
            Some(d) => d,
            None => continue,
        };
        for d in deps {
            let dn = match d.as_str() {
                Some(s) => s,
                None => continue,
            };
            if !workspace_members.contains(dn) {
                continue;
            }
            if dn == name {
                continue;
            }
            let to_anchor = match anchors.get(dn) {
                Some(a) => a.clone(),
                _ => continue,
            };
            new_edges.push(serde_json::json!({
                "from":         from_anchor,
                "to":           to_anchor,
                "kind":         "crate-dep",
                "isCrossCrate": true,
                "from_crate":   name,
                "to_crate":     dn,
            }));
        }
    }

    if let Some(edges) = graph.get_mut("edges").and_then(Value::as_array_mut) {
        edges.extend(new_edges);
    }
}

/// Backfill `is_external: true` on every node whose `crate_of(id)` resolves
/// to "external" (or whose id is prefixed with `ext:`). Without this pass,
/// 32+ Python scripts and orphan `<name>.rs` single-file crates leak into
/// the inner Pauling shells because the frontend only respects `is_external`
/// flag set by `enrich_external_deps`.
pub(super) fn backfill_external_flag(graph: &mut Value) {
    if let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_array_mut) {
        for n in nodes.iter_mut() {
            let id = n
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let crate_name = crate_of(&id);
            let already = n
                .get("is_external")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let is_ext = already || crate_name == "external" || id.starts_with("ext:");
            if is_ext && let Some(obj) = n.as_object_mut() {
                obj.insert("is_external".into(), Value::Bool(true));
            }
        }
    }
}

/// Recompute fan_in / fan_out for every node after enrichment, then derive
/// a `core_score ∈ [-1, +1]` describing dependency gravity:
///
///   `core_score = (fan_in_norm) - (fan_out_norm)` where both are normalised
///   against the per-graph max so a single hot node doesn't crush the rest.
///
/// Reading guide:
///   * `core_score → +1`  — file is heavily *imported* and itself imports
///     little. It is **CORE**: type defs, error types,
///     `lib.rs` of foundational crates. Lives in the
///     Pauling **nucleus**.
///   * `core_score → -1`  — file imports many things but nobody depends on
///     it. It is **LEAF/EDGE**: CLI handlers, integration
///     tests, `main.rs`. Lives on the **outer pearl**.
///   * `core_score ≈ 0`  — balanced intermediate; populates middle shells.
///
/// Edge filter: only `imports` and `module-decl` edges count. Synthetic
/// `external-dep` and `crate-dep` would fan-out-bias every workspace node;
/// `symbol-ref` is ignored to avoid double-counting the file-level import.
pub(super) fn compute_core_scores(graph: &mut Value) {
    use std::collections::HashMap;
    let mut fan_in: HashMap<String, u32> = HashMap::new();
    let mut fan_out: HashMap<String, u32> = HashMap::new();

    if let Some(edges) = graph.get("edges").and_then(Value::as_array) {
        for e in edges {
            let kind = e.get("kind").and_then(Value::as_str).unwrap_or("imports");
            // Only true *file imports* contribute to dependency gravity.
            // crate-dep is a synthetic anchor edge; external-dep points to
            // peripheral super-nodes; symbol-ref over-counts.
            if !matches!(kind, "imports" | "module-decl") {
                continue;
            }
            if let (Some(f), Some(t)) = (
                e.get("from").and_then(Value::as_str),
                e.get("to").and_then(Value::as_str),
            ) {
                *fan_out.entry(f.to_string()).or_insert(0) += 1;
                *fan_in.entry(t.to_string()).or_insert(0) += 1;
            }
        }
    }

    let max_in = (*fan_in.values().max().unwrap_or(&1)).max(1) as f64;
    let max_out = (*fan_out.values().max().unwrap_or(&1)).max(1) as f64;

    if let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_array_mut) {
        for n in nodes.iter_mut() {
            let id = n
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let fi = *fan_in.get(&id).unwrap_or(&0);
            let fo = *fan_out.get(&id).unwrap_or(&0);
            // Use sqrt to soften the long-tail bias — a node with 100 imports
            // should not utterly dominate one with 30; both belong on the
            // periphery just at different concentric radii.
            let fi_n = (fi as f64).sqrt() / max_in.sqrt();
            let fo_n = (fo as f64).sqrt() / max_out.sqrt();
            let core_score = fi_n - fo_n; // ∈ approximately [-1, +1]
            if let Some(obj) = n.as_object_mut() {
                obj.insert("computed_fan_in".into(), serde_json::json!(fi));
                obj.insert("computed_fan_out".into(), serde_json::json!(fo));
                obj.insert("core_score".into(), serde_json::json!(core_score));
            }
        }
    }
}

pub(super) fn anchor_prio(id: &str) -> i32 {
    if id.ends_with("/src/lib.rs") {
        0
    } else if id.ends_with("/src/main.rs") {
        1
    } else if id.ends_with("/lib.rs") {
        2
    } else if id.ends_with("/main.rs") {
        3
    } else if id.ends_with("/mod.rs") {
        4
    } else {
        9
    }
}

/// Parse `mod <name>;` declarations in lib.rs/main.rs/mod.rs files and
/// emit edges parent→child for each resolved submodule. Captures hierarchy
/// the wiring scan misses (it only records `use` imports, not `mod`).
///
/// Resolves `mod foo;` in `<dir>/parent.rs` to either `<dir>/foo.rs` or
/// `<dir>/foo/mod.rs`. Inline `mod foo { ... }` is intentionally ignored:
/// inline modules don't introduce a separate file.
pub(super) fn enrich_module_decls(graph: &mut Value, _workspace_root: &std::path::Path) {
    use std::collections::HashSet;
    let node_ids: HashSet<String> = graph
        .get("nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.get("id").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let existing_edges: HashSet<(String, String)> = graph
        .get("edges")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let f = e.get("from").and_then(Value::as_str)?;
                    let t = e.get("to").and_then(Value::as_str)?;
                    Some((f.to_string(), t.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();

    let mod_re = match regex::Regex::new(
        r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;",
    ) {
        Ok(re) => re,
        Err(_) => return,
    };

    let mut new_edges: Vec<Value> = Vec::new();
    for parent_id in &node_ids {
        let parent_path = std::path::Path::new(parent_id);
        let pname = parent_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if !matches!(pname, "lib.rs" | "main.rs" | "mod.rs") {
            continue;
        }
        let parent_dir = match parent_path.parent() {
            Some(p) => p,
            None => continue,
        };
        let content = match std::fs::read_to_string(parent_path) {
            Ok(c) => c,
            _ => continue,
        };

        for cap in mod_re.captures_iter(&content) {
            let mod_name = match cap.get(1) {
                Some(m) => m.as_str(),
                _ => continue,
            };
            let cand1 = parent_dir.join(format!("{}.rs", mod_name));
            let cand2 = parent_dir.join(mod_name).join("mod.rs");
            let resolved = if cand1.exists() {
                cand1
            } else if cand2.exists() {
                cand2
            } else {
                continue;
            };
            let child_id = resolved.to_string_lossy().to_string();
            if !node_ids.contains(&child_id) {
                continue;
            }
            if existing_edges.contains(&(parent_id.clone(), child_id.clone())) {
                continue;
            }

            let p_crate = crate_of(parent_id);
            let c_crate = crate_of(&child_id);
            new_edges.push(serde_json::json!({
                "from":         parent_id,
                "to":           child_id,
                "kind":         "module-decl",
                "isCrossCrate": p_crate != c_crate,
                "from_crate":   p_crate,
                "to_crate":     c_crate,
            }));
        }
    }

    if let Some(edges) = graph.get_mut("edges").and_then(Value::as_array_mut) {
        edges.extend(new_edges);
    }
}

/// Aggregate per-(src_file, dst_file) symbol-level edges from
/// `~/.claude/rust/.claude/touring/knowledge.db` (table `wiring_map`).
/// For each existing edge, attach `symbol_count` (number of symbols flowing
/// through that edge) and `symbol_kinds` (top kinds, comma-joined). When
/// the edge does not yet exist, introduce a new edge with kind="symbol-ref".
///
/// This unlocks the 12,336-row symbol-level relation richness already
/// indexed in the daemon — the bare `viz workspace` only emits ~1,087 edges
/// because it dedupes by (src,dst) without preserving counts/kinds.
pub(super) fn enrich_symbol_relations(graph: &mut Value, workspace_root: &std::path::Path) {
    let kdb = workspace_root.join(".claude/touring/knowledge.db");
    if !kdb.exists() {
        return;
    }

    // Shell out to sqlite3 to keep the dep tree minimal. Returns rows of
    // `module_file<TAB>consumer_file<TAB>symbol_count<TAB>top_kind`.
    let query = "SELECT module_file, consumer_file, COUNT(*) AS cnt, \
                 (SELECT symbol_kind FROM wiring_map w2 \
                  WHERE w2.module_file = w1.module_file AND w2.consumer_file = w1.consumer_file \
                  GROUP BY symbol_kind ORDER BY COUNT(*) DESC LIMIT 1) AS top_kind \
                 FROM wiring_map w1 \
                 WHERE consumer_file IS NOT NULL \
                 GROUP BY module_file, consumer_file";
    let output = std::process::Command::new("sqlite3")
        .arg(&kdb)
        .arg("-separator")
        .arg("\t")
        .arg(query)
        .output();
    let stdout = match output {
        Ok(o) if o.status.success() => o.stdout,
        _ => return,
    };
    let text = match std::str::from_utf8(&stdout) {
        Ok(s) => s,
        _ => return,
    };

    // Index existing edges by (from, to) for symbol enrichment.
    let mut edge_idx: HashMap<(String, String), usize> = HashMap::new();
    if let Some(edges) = graph.get("edges").and_then(Value::as_array) {
        for (i, e) in edges.iter().enumerate() {
            if let (Some(f), Some(t)) = (
                e.get("from").and_then(Value::as_str),
                e.get("to").and_then(Value::as_str),
            ) {
                edge_idx.insert((f.to_string(), t.to_string()), i);
            }
        }
    }
    let node_ids: HashSet<String> = graph
        .get("nodes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.get("id").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut updates: Vec<(usize, u64, String)> = Vec::new();
    let mut new_edges: Vec<Value> = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let module_raw = parts[0];
        let consumer_raw = parts[1];
        let count: u64 = parts[2].parse().unwrap_or(0);
        let top_kind = parts.get(3).copied().unwrap_or("symbol").to_string();

        // wiring_map stores paths as either absolute or `crates/...` — normalize
        // to absolute against workspace_root.
        let module_abs = normalize_path(module_raw, workspace_root);
        let consumer_abs = normalize_path(consumer_raw, workspace_root);

        // wiring_map direction is module(=defines) → consumer(=imports).
        // The viz graph direction is consumer → module (importer points at
        // imported module). So we look up (consumer, module).
        let key = (consumer_abs.clone(), module_abs.clone());
        if let Some(idx) = edge_idx.get(&key) {
            updates.push((*idx, count, top_kind));
        } else if node_ids.contains(&consumer_abs) && node_ids.contains(&module_abs) {
            let p_crate = crate_of(&consumer_abs);
            let c_crate = crate_of(&module_abs);
            new_edges.push(serde_json::json!({
                "from":         consumer_abs,
                "to":           module_abs,
                "kind":         "symbol-ref",
                "isCrossCrate": p_crate != c_crate,
                "from_crate":   p_crate,
                "to_crate":     c_crate,
                "symbol_count": count,
                "top_kind":     top_kind,
            }));
        }
    }

    if let Some(edges) = graph.get_mut("edges").and_then(Value::as_array_mut) {
        for (idx, count, kind) in updates {
            if let Some(e) = edges.get_mut(idx).and_then(Value::as_object_mut) {
                e.insert("symbol_count".into(), serde_json::json!(count));
                e.insert("top_kind".into(), Value::String(kind));
            }
        }
        edges.extend(new_edges);
    }
}

/// Normalize a path that may be absolute or `crates/...`-relative against
/// the workspace root.
pub(super) fn normalize_path(p: &str, workspace_root: &std::path::Path) -> String {
    if p.starts_with('/') {
        p.to_string()
    } else {
        workspace_root.join(p).to_string_lossy().to_string()
    }
}

/// Add external crate dependencies as super-nodes prefixed with `ext:`.
/// Each workspace crate's anchor file gets edges to its non-workspace deps
/// (kind="external-dep"). This shows the architectural surface area —
/// what tokio/serde/axum/etc. each crate consumes — that the file-level
/// wiring scan never captures.
pub(super) fn enrich_external_deps(graph: &mut Value, workspace_root: &std::path::Path) {
    use std::collections::HashMap;
    let output = std::process::Command::new("touring")
        .current_dir(workspace_root)
        .args(["ast", "workspace-info"])
        .output();
    let info: Value = match output {
        Ok(o) if o.status.success() => match serde_json::from_slice(&o.stdout) {
            Ok(v) => v,
            Err(_) => return,
        },
        _ => return,
    };
    let packages = match info.get("packages").and_then(Value::as_array) {
        Some(p) => p,
        None => return,
    };

    // Workspace member set + map crate-name → anchor file from existing nodes.
    let workspace_members: std::collections::HashSet<String> = packages
        .iter()
        .filter(|p| {
            p.get("is_workspace_member")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|p| p.get("name").and_then(Value::as_str).map(String::from))
        .collect();

    let mut anchors: HashMap<String, String> = HashMap::new();
    if let Some(nodes) = graph.get("nodes").and_then(Value::as_array) {
        for n in nodes {
            let id = n.get("id").and_then(Value::as_str).unwrap_or("");
            let cn = n.get("crate").and_then(Value::as_str).unwrap_or("external");
            if cn == "external" || cn.starts_with("ext:") {
                continue;
            }
            let prio = anchor_prio(id);
            let entry = anchors
                .entry(cn.to_string())
                .or_insert_with(|| id.to_string());
            if prio < anchor_prio(entry) {
                *entry = id.to_string();
            }
        }
    }

    // Collect external dep names + edges.
    let mut ext_node_count: HashMap<String, u32> = HashMap::new();
    let mut new_edges: Vec<Value> = Vec::new();
    for pkg in packages {
        let name = match pkg.get("name").and_then(Value::as_str) {
            Some(n) => n,
            _ => continue,
        };
        if !workspace_members.contains(name) {
            continue;
        }
        let from_anchor = match anchors.get(name) {
            Some(a) => a.clone(),
            _ => continue,
        };
        let deps = match pkg.get("dependencies").and_then(Value::as_array) {
            Some(d) => d,
            None => continue,
        };
        for d in deps {
            let dn = match d.as_str() {
                Some(s) => s,
                _ => continue,
            };
            if workspace_members.contains(dn) {
                continue;
            } // workspace-internal handled by enrich_crate_deps
            if dn == name {
                continue;
            }
            *ext_node_count.entry(dn.to_string()).or_insert(0) += 1;
            let ext_id = format!("ext:{}", dn);
            new_edges.push(serde_json::json!({
                "from":         from_anchor,
                "to":           ext_id,
                "kind":         "external-dep",
                "isCrossCrate": true,
                "from_crate":   name,
                "to_crate":     format!("ext:{}", dn),
            }));
        }
    }

    // Materialize external nodes: only keep those used by ≥ 2 internal crates
    // to avoid clutter from dev-only single-crate deps. Each external node
    // is visually distinct (crate="external") so the frontend can color it.
    let mut new_nodes: Vec<Value> = Vec::new();
    for (name, fan_in) in &ext_node_count {
        if *fan_in < 2 {
            continue;
        }
        new_nodes.push(serde_json::json!({
            "id":            format!("ext:{}", name),
            "label":         name,
            "crate":         "external",
            "is_external":   true,
            "fan_in":        fan_in,
            "fan_out":       0,
            "is_orphan":     false,
            "has_unsafe":    false,
            "is_test":       false,
            "quality_score": Value::Null,
        }));
    }
    let kept_externals: std::collections::HashSet<String> = new_nodes
        .iter()
        .filter_map(|n| n.get("id").and_then(Value::as_str).map(String::from))
        .collect();
    new_edges.retain(|e| {
        e.get("to")
            .and_then(Value::as_str)
            .map(|t| !t.starts_with("ext:") || kept_externals.contains(t))
            .unwrap_or(true)
    });

    if !new_nodes.is_empty()
        && let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_array_mut)
    {
        nodes.extend(new_nodes);
    }
    if !new_edges.is_empty()
        && let Some(edges) = graph.get_mut("edges").and_then(Value::as_array_mut)
    {
        edges.extend(new_edges);
    }
}
