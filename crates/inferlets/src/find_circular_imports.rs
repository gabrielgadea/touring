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

    crate::fs_walk::for_each_file(workspace_path, &mut |path| {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        if !extensions.iter().any(|e| name.ends_with(e.as_str())) {
            return;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        let imports = file_imports(name, &content);
        if !imports.is_empty() {
            graph.insert(path.to_string_lossy().into_owned(), imports);
        }
    });
    graph
}

/// Modules a file imports, read line by line: Python `import`/`from`, and
/// JavaScript/TypeScript `from '…'`/`require('…')`.
fn file_imports(name: &str, content: &str) -> HashSet<String> {
    let python = name.ends_with(".py");
    let script = name.ends_with(".js") || name.ends_with(".ts") || name.ends_with(".tsx");
    let mut imports = HashSet::new();
    for line in content.lines().map(str::trim) {
        if python {
            let rest = line
                .strip_prefix("import ")
                .or_else(|| line.strip_prefix("from "));
            if let Some(mod_name) = rest.and_then(|r| r.split_whitespace().next()) {
                let top = mod_name.split('.').next().unwrap_or(mod_name);
                imports.insert(top.to_string());
            }
        }
        if script {
            let quoted = if line.contains("import ") && line.contains("from '") {
                line.find("from '").map(|s| &line[s + 6..])
            } else {
                line.find("require('").map(|s| &line[s + 9..])
            };
            if let Some(rest) = quoted
                && let Some(end) = rest.find('\'')
            {
                imports.insert(rest[..end].to_string());
            }
        }
    }
    imports
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

    /// It walked the machine's `/tmp` and asserted it held no importing file —
    /// true until something put one there. The tree is now the test's own.
    #[test]
    fn test_build_import_graph_skips_target_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("target")).expect("mkdir");
        std::fs::create_dir_all(root.join("pkg")).expect("mkdir");
        std::fs::write(root.join("target/gen.py"), "import os\n").expect("write");
        std::fs::write(root.join("pkg/mod.py"), "from os import path\n").expect("write");

        let graph = build_import_graph(&root.to_string_lossy(), &[".py".to_string()]);

        let keys: Vec<&String> = graph.keys().collect();
        assert_eq!(keys.len(), 1, "{keys:?}");
        assert!(keys[0].ends_with("pkg/mod.py"), "{keys:?}");
    }

    /// `require('` is nine characters; the slice skipped eight and every
    /// `require('x')` was recorded as the empty module name.
    #[test]
    fn file_imports_reads_python_and_script_forms() {
        let py = file_imports("a.py", "import os.path\nfrom json import loads\n");
        assert_eq!(py, HashSet::from(["os".to_string(), "json".to_string()]));
        let js = file_imports(
            "a.js",
            "import x from 'lodash';\nconst fs = require('fs');\n",
        );
        assert_eq!(js, HashSet::from(["lodash".to_string(), "fs".to_string()]));
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
