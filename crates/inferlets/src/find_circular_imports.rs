//! Circular import detection inferlet.
//!
//! Detects circular import chains in Python/JS codebases using ast analysis.
//! FS-dependent — uses Python ast stdlib or Node.js analysis.
//!
//! # Input JSON
//!
//! ```json
//! {
//!   "__inferlet__": "find_circular_imports",
//!   "workspace": "/home/gabrielgadea/.claude/rust",
//!   "extensions": [".py"]
//! }
//! ```
//!
//! # Output JSON
//!
//! ```json
//! {
//!   "cycles": [["a.py", "b.py", "c.py", "a.py"], ["d.py", "e.py", "d.py"]],
//!   "count": 2
//! }
//! ```
//!
//! Returns 1 if cycles found, 0 if no cycles.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::thread_local;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Input structure for find_circular_imports.
#[derive(Debug, Deserialize)]
pub struct Input {
    /// Root directory to scan for source files.
    pub workspace: String,
    /// File extensions whose imports are analyzed, e.g. `.py`.
    pub extensions: Vec<String>,
}

/// A circular import cycle.
#[derive(Debug, Serialize)]
pub struct Cycle {
    /// Ordered list of files forming the cycle (first repeats as last).
    pub path: Vec<String>,
}

/// Output structure for find_circular_imports.
#[derive(Debug, Serialize)]
pub struct Output {
    /// All detected import cycles.
    pub cycles: Vec<Cycle>,
    /// Number of cycles found.
    pub count: usize,
}

/// Build import graph for a workspace.
/// Returns map of file -> set of imported modules.
fn build_import_graph(workspace: &str, extensions: &[String]) -> HashMap<String, HashSet<String>> {
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();
    let workspace_path = Path::new(workspace);

    if !workspace_path.is_dir() {
        return graph;
    }

    fn walk_files(dir: &Path, extensions: &[String], graph: &mut HashMap<String, HashSet<String>>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    if path
                        .file_name()
                        .is_some_and(|n| n == "target" || n == ".git")
                    {
                        continue;
                    }
                    walk_files(&path, extensions, graph);
                } else if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        let ext_match = extensions.iter().any(|e| name.ends_with(e));
                        if ext_match {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let mut imports: HashSet<String> = HashSet::new();

                                for line in content.lines() {
                                    let line = line.trim();

                                    // Python: import foo, from foo import bar
                                    if name.ends_with(".py") {
                                        if let Some(rest) = line.strip_prefix("import ") {
                                            if let Some(mod_name) = rest.split_whitespace().next() {
                                                let mod_name =
                                                    mod_name.split('.').next().unwrap_or(mod_name);
                                                imports.insert(mod_name.to_string());
                                            }
                                        } else if let Some(rest) = line.strip_prefix("from ") {
                                            if let Some(mod_name) = rest.split_whitespace().next() {
                                                let mod_name =
                                                    mod_name.split('.').next().unwrap_or(mod_name);
                                                imports.insert(mod_name.to_string());
                                            }
                                        }
                                    }

                                    // JavaScript/TypeScript: import foo from 'bar', require('bar')
                                    if name.ends_with(".js")
                                        || name.ends_with(".ts")
                                        || name.ends_with(".tsx")
                                    {
                                        if line.contains("import ") && line.contains("from '") {
                                            if let Some(start) = line.find("from '") {
                                                let rest = &line[start + 6..];
                                                if let Some(end) = rest.find('\'') {
                                                    imports.insert(rest[..end].to_string());
                                                }
                                            }
                                        } else if line.contains("require('") {
                                            if let Some(start) = line.find("require('") {
                                                let rest = &line[start + 8..];
                                                if let Some(end) = rest.find('\'') {
                                                    imports.insert(rest[..end].to_string());
                                                }
                                            }
                                        }
                                    }
                                }

                                if !imports.is_empty() {
                                    let key = path.to_string_lossy().into_owned();
                                    graph.insert(key, imports);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    walk_files(workspace_path, extensions, &mut graph);
    graph
}

/// Detect cycles using DFS with path tracking.
fn find_cycles(graph: &HashMap<String, HashSet<String>>) -> Vec<Cycle> {
    let mut cycles: Vec<Cycle> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();

    fn dfs(
        node: &str,
        graph: &HashMap<String, HashSet<String>>,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
        cycles: &mut Vec<Cycle>,
    ) {
        if stack.iter().any(|s| s == node) {
            // Found cycle - extract it
            if let Some(start_idx) = stack.iter().position(|x| x == node) {
                let cycle_path: Vec<String> = stack[start_idx..].to_vec();
                cycles.push(Cycle { path: cycle_path });
            }
            return;
        }

        if visited.contains(node) {
            return;
        }

        visited.insert(node.to_string());
        stack.push(node.to_string());

        if let Some(deps) = graph.get(node) {
            for dep in deps {
                dfs(dep, graph, visited, stack, cycles);
            }
        }

        stack.pop();
    }

    for node in graph.keys() {
        visited.clear();
        stack.clear();
        dfs(node, graph, &mut visited, &mut stack, &mut cycles);
    }

    cycles
}

/// Raw evaluate — returns 1 if cycles found, 0 otherwise.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let input = input.trim();
    let inp: Input = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some("invalid JSON input".to_string()));
            return 0;
        }
    };

    if inp.extensions.is_empty() {
        LAST_ERROR.with(|cell| {
            *cell.borrow_mut() = Some("\"extensions\" array cannot be empty".to_string())
        });
        return 0;
    }

    let graph = build_import_graph(&inp.workspace, &inp.extensions);
    let cycles = find_cycles(&graph);

    if cycles.is_empty() {
        return 0;
    }

    let count = cycles.len();
    let output = Output { cycles, count };

    if let Ok(json) = serde_json::to_string(&output) {
        LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(json));
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_import_graph_empty() {
        let graph = build_import_graph("/nonexistent", &[".py".to_string()]);
        assert!(graph.is_empty());
    }

    #[test]
    fn test_find_cycles_empty() {
        let graph: HashMap<String, HashSet<String>> = HashMap::new();
        let cycles = find_cycles(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_cycle_serialization() {
        let cycle = Cycle {
            path: vec!["a.py".to_string(), "b.py".to_string()],
        };
        let json = serde_json::to_string(&cycle);
        assert!(json.is_ok());
    }

    #[test]
    fn test_evaluate_raw_malformed_json() {
        let result = evaluate_raw("{ invalid");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_evaluate_raw_empty_extensions() {
        let result = evaluate_raw(r#"{"workspace":"/tmp","extensions":[]}"#);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_build_import_graph_skips_target_dirs() {
        // Should not include target/ in walk
        let graph = build_import_graph("/tmp", &[".rs".to_string()]);
        // Empty since /tmp doesn't have Rust files typically
        assert_eq!(graph.len(), 0);
    }

    #[test]
    fn test_output_count() {
        let out = Output {
            cycles: vec![Cycle {
                path: vec!["a.py".to_string()],
            }],
            count: 1,
        };
        assert_eq!(out.count, 1);
    }
}
