//! Count files by extension via CLI wrapper inferlet.
//!
//! Uses walkdir or direct FS count for extension tallying.
//! FS-dependent — can use `ctx_execute` sandbox or direct walkdir.
//!
//! # Input JSON
//!
//! ```json
//! {
//!   "__inferlet__": "count_files_via_cli_wrapper",
//!   "workspace": "/home/gabrielgadea/.claude/rust",
//!   "extensions": [".rs", ".py"]
//! }
//! ```
//!
//! # Output JSON
//!
//! ```json
//! {
//!   "counts": {".rs": 2847, ".py": 412},
//!   "total": 3259
//! }
//! ```
//!
//! Returns 1 always (matching is determined by having any extensions to count).

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::thread_local;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Input structure for count_files_via_cli_wrapper.
#[derive(Debug, Deserialize)]
pub struct Input {
    /// Root directory to walk recursively when tallying files.
    pub workspace: String,
    /// File extensions (suffixes) to count, e.g. `.rs`, `.py`.
    pub extensions: Vec<String>,
}

/// Extension count entry.
#[derive(Debug, Serialize)]
pub struct ExtensionCount {
    /// The file extension this count applies to.
    pub extension: String,
    /// Number of files matching the extension.
    pub count: usize,
}

/// Output structure for count_files_via_cli_wrapper.
#[derive(Debug, Serialize)]
pub struct Output {
    /// Per-extension file counts keyed by extension string.
    pub counts: HashMap<String, usize>,
    /// Total number of matched files across all extensions.
    pub total: usize,
}

/// Count files recursively with given extensions in a workspace using std::fs.
fn count_extensions(workspace: &str, extensions: &[String]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let workspace_path = Path::new(workspace);

    if !workspace_path.is_dir() {
        return counts;
    }

    fn walk_dir(dir: &Path, extensions: &[String], counts: &mut HashMap<String, usize>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    // Skip target and .git directories
                    if path
                        .file_name()
                        .is_some_and(|n| n == "target" || n == ".git")
                    {
                        continue;
                    }
                    walk_dir(&path, extensions, counts);
                } else if path.is_file()
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                {
                    for ext in extensions {
                        if name.ends_with(ext) {
                            *counts.entry(ext.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }

    walk_dir(workspace_path, extensions, &mut counts);
    counts
}

/// Raw evaluate — returns 1 always.
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

    let counts = count_extensions(&inp.workspace, &inp.extensions);
    let total: usize = counts.values().sum();

    let output = Output { counts, total };

    if let Ok(json) = serde_json::to_string(&output) {
        LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(json));
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_extensions_nonexistent_workspace() {
        let counts = count_extensions("/nonexistent/workspace", &[".rs".to_string()]);
        assert!(counts.is_empty());
    }

    #[test]
    fn test_count_extensions_empty_workspace() {
        let counts = count_extensions("/tmp", &[]);
        assert!(counts.is_empty());
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
    fn test_output_serializes() {
        let mut counts = HashMap::new();
        counts.insert(".rs".to_string(), 10);
        let output = Output { counts, total: 10 };
        let json = serde_json::to_string(&output);
        assert!(json.is_ok());
    }

    #[test]
    fn test_extension_count_insert() {
        let mut counts = HashMap::new();
        counts.insert(".py".to_string(), 5);
        assert_eq!(counts.get(".py"), Some(&5));
    }
}
