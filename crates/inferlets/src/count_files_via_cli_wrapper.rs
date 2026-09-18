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

    if extensions.is_empty() || !workspace_path.is_dir() {
        return counts;
    }

    crate::fs_walk::for_each_file(workspace_path, &mut |path| {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        for ext in extensions {
            if name.ends_with(ext.as_str()) {
                *counts.entry(ext.clone()).or_insert(0) += 1;
            }
        }
    });
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

    /// No extension asked means nothing to count. It used to walk the whole
    /// `/tmp` of the machine to find that out.
    #[test]
    fn test_count_extensions_empty_extension_list() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "").expect("write");
        let counts = count_extensions(&dir.path().to_string_lossy(), &[]);
        assert!(counts.is_empty());
    }

    #[test]
    fn test_count_extensions_counts_each_asked_extension_outside_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::create_dir_all(root.join("target")).expect("mkdir");
        for f in ["src/a.rs", "src/b.rs", "src/c.py", "target/gen.rs"] {
            std::fs::write(root.join(f), "").expect("write");
        }
        let counts = count_extensions(
            &root.to_string_lossy(),
            &[".rs".to_string(), ".py".to_string()],
        );
        assert_eq!(counts.get(".rs"), Some(&2));
        assert_eq!(counts.get(".py"), Some(&1));
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
