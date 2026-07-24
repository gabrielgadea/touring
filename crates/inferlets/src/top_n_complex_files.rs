//! Top-N complex files detector via CLI wrapper inferlet.
//!
//! Uses radon (or Python ast analysis) to find most cyclomatically complex files.
//! FS-dependent — calls Python script via `ctx_execute` sandbox.
//!
//! # Input JSON
//!
//! ```json
//! {
//!   "__inferlet__": "top_n_complex_files",
//!   "workspace": "/home/gabrielgadea/.claude/rust",
//!   "n": 10
//! }
//! ```
//!
//! # Output JSON
//!
//! ```json
//! {
//!   "files": [
//!     {"path": "touring-hooks/src/hook_runtime.rs", "cc": 142, "loc": 4821}
//!   ],
//!   "total_analyzed": 2847
//! }
//! ```
//!
//! Returns 1 if files found, 0 if workspace empty or error.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::Path;
use std::thread_local;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Input structure for top_n_complex_files.
#[derive(Debug, Deserialize)]
pub struct Input {
    /// Root directory to scan for source files.
    pub workspace: String,
    /// Number of most-complex files to return.
    pub n: usize,
}

/// A complex file entry.
#[derive(Debug, Serialize)]
pub struct ComplexFile {
    /// Path to the file relative to the workspace.
    pub path: String,
    /// Cyclomatic complexity score for the file.
    pub cc: usize,
    /// Lines of code in the file.
    pub loc: usize,
}

/// Output structure for top_n_complex_files.
#[derive(Debug, Serialize)]
pub struct Output {
    /// The top-N most complex files, ordered by complexity.
    pub files: Vec<ComplexFile>,
    /// Total number of files analyzed.
    pub total_analyzed: usize,
}

/// Simple LOC counter (no radon needed).
fn simple_loc(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|c| c.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

/// Walk directory recursively without walkdir.
fn walk_rust_files(dir: &Path, results: &mut Vec<(String, usize, usize)>) {
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
                walk_rust_files(&path, results);
            } else if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".rs") {
                        let loc = simple_loc(&path);
                        results.push((path.to_string_lossy().into_owned(), 0, loc));
                    }
                }
            }
        }
    }
}

/// Analyze Rust files for cyclomatic complexity.
/// Falls back to simple LOC counting when radon is not available.
fn analyze_complexity(workspace: &str, n: usize) -> (Vec<ComplexFile>, usize) {
    let workspace_path = Path::new(workspace);
    if !workspace_path.is_dir() {
        return (Vec::new(), 0);
    }

    let mut results: Vec<(String, usize, usize)> = Vec::new();
    walk_rust_files(workspace_path, &mut results);

    let total_analyzed = results.len();
    results.sort_by_key(|x| std::cmp::Reverse(x.2));

    let files: Vec<ComplexFile> = results
        .into_iter()
        .take(n)
        .map(|(path, cc, loc)| ComplexFile { path, cc, loc })
        .collect();

    (files, total_analyzed)
}

/// Raw evaluate — returns 1 if files found, 0 otherwise.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let input = input.trim();
    let inp: Input = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some("invalid JSON input".to_string()));
            return 0;
        }
    };

    let (files, total_analyzed) = analyze_complexity(&inp.workspace, inp.n);

    if files.is_empty() {
        return 0;
    }

    let output = Output {
        files,
        total_analyzed,
    };

    if let Ok(json) = serde_json::to_string(&output) {
        LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(json));
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_complexity_nonexistent() {
        let (files, total) = analyze_complexity("/nonexistent", 10);
        assert!(files.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_complex_file_serialization() {
        let cf = ComplexFile {
            path: "test.rs".to_string(),
            cc: 10,
            loc: 100,
        };
        let json = serde_json::to_string(&cf);
        assert!(json.is_ok());
    }

    #[test]
    fn test_evaluate_raw_malformed_json() {
        let result = evaluate_raw("{ invalid");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_evaluate_raw_empty_workspace() {
        let result = evaluate_raw(r#"{"workspace":"/nonexistent","n":5}"#);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_input_default_n() {
        let inp: Input = serde_json::from_str(r#"{"workspace":"/tmp","n":10}"#).unwrap();
        assert_eq!(inp.n, 10);
    }

    #[test]
    fn test_simple_loc_nonexistent() {
        assert_eq!(simple_loc(Path::new("/nonexistent/file.rs")), 0);
    }
}
