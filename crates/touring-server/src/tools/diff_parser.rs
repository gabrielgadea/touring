//! Diff Parser — Function-level change mapping from git diff output.
//!
//! Parses unified diff format to extract changed line ranges per file,
//! then maps those ranges to AST symbols for per-function risk scoring.
//! Inspired by code-review-graph's `changes.py::parse_git_diff_ranges()`.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

/// Builtin symbol names filtered from blast radius to reduce noise.
/// Inspired by code-review-graph's `_BUILTIN_CALL_NAMES` (~150 entries).
/// "Who calls .map()?" returns hundreds of useless hits.
static BUILTIN_NOISE: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        // Rust builtins
        "new",
        "default",
        "clone",
        "fmt",
        "drop",
        "from",
        "into",
        "as_ref",
        "as_mut",
        "deref",
        "deref_mut",
        "unwrap",
        "expect",
        "ok",
        "err",
        "map",
        "and_then",
        "or_else",
        "unwrap_or",
        "unwrap_or_default",
        "unwrap_or_else",
        "is_some",
        "is_none",
        "is_ok",
        "is_err",
        "iter",
        "into_iter",
        "collect",
        "filter",
        "find",
        "any",
        "all",
        "fold",
        "for_each",
        "enumerate",
        "zip",
        "chain",
        "take",
        "skip",
        "push",
        "pop",
        "insert",
        "remove",
        "contains",
        "get",
        "len",
        "is_empty",
        "sort",
        "sort_by",
        "extend",
        "drain",
        "clear",
        "to_string",
        "to_owned",
        "as_str",
        "as_bytes",
        "trim",
        "starts_with",
        "ends_with",
        "contains",
        "replace",
        "split",
        "join",
        "parse",
        "display",
        // Python builtins
        "__init__",
        "__str__",
        "__repr__",
        "__eq__",
        "__hash__",
        "__enter__",
        "__exit__",
        "__len__",
        "__iter__",
        "__next__",
        "append",
        "extend",
        "keys",
        "values",
        "items",
        "update",
        "print",
        "len",
        "range",
        "enumerate",
        "zip",
        "map",
        "filter",
        "isinstance",
        "hasattr",
        "getattr",
        "setattr",
        // JS/TS builtins
        "forEach",
        "reduce",
        "indexOf",
        "log",
        "warn",
        "error",
        "setTimeout",
        "setInterval",
        "fetch",
        "then",
        "catch",
        "JSON",
        "stringify",
        "addEventListener",
        "querySelector",
    ]
    .into_iter()
    .collect()
});

/// A changed line range from a git diff hunk.
#[derive(Debug, Clone, Serialize)]
pub struct DiffHunk {
    /// Path of the file the hunk belongs to.
    pub file_path: String,
    /// First line of the changed range (1-based).
    pub start_line: u32,
    /// Last line of the changed range (1-based).
    pub end_line: u32,
    /// Kind of change the hunk represents.
    pub change_type: ChangeType,
}

/// Type of change in a diff hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    /// Lines were added.
    Added,
    /// Existing lines were modified.
    Modified,
    /// Lines were removed.
    Deleted,
}

/// A symbol affected by changes with overlap information.
#[derive(Debug, Clone, Serialize)]
pub struct AffectedSymbol {
    /// Name of the affected symbol.
    pub name: String,
    /// File the symbol is defined in.
    pub file_path: String,
    /// First line of the symbol's definition.
    pub line_start: u32,
    /// Last line of the symbol's definition.
    pub line_end: u32,
    /// Symbol kind (e.g. `fn`, `struct`, `enum`).
    pub kind: String,
    /// Fraction of the symbol's lines that were changed (0.0-1.0).
    pub overlap_ratio: f64,
}

/// Parse unified diff text into file → line-range hunks.
///
/// Handles the `@@ -old,count +new,count @@` hunk header format
/// and `+++ b/path` file headers.
pub fn parse_unified_diff(diff_text: &str) -> Vec<DiffHunk> {
    let file_re = Regex::new(r"^\+\+\+ b/(.+)$").expect("valid regex");
    let hunk_re = Regex::new(r"^@@ .+? \+(\d+)(?:,(\d+))? @@").expect("valid regex");

    let mut hunks = Vec::new();
    let mut current_file: Option<String> = None;

    for line in diff_text.lines() {
        if let Some(caps) = file_re.captures(line) {
            current_file = Some(caps[1].to_string());
            continue;
        }

        if let Some(caps) = hunk_re.captures(line) {
            if let Some(ref file) = current_file {
                let start: u32 = caps[1].parse().unwrap_or(0);
                let count: u32 = caps
                    .get(2)
                    .map(|m| m.as_str().parse().unwrap_or(1))
                    .unwrap_or(1);

                let (end, change_type) = if count == 0 {
                    (start, ChangeType::Deleted)
                } else {
                    (start + count - 1, ChangeType::Modified)
                };

                hunks.push(DiffHunk {
                    file_path: file.clone(),
                    start_line: start,
                    end_line: end,
                    change_type,
                });
            }
        }
    }

    hunks
}

/// Check if a symbol name is a builtin that should be filtered from blast radius.
pub fn is_builtin_noise(name: &str) -> bool {
    BUILTIN_NOISE.contains(name)
}

/// Map diff hunks to overlapping symbols.
///
/// For each file in the hunks, queries the symbol list and checks
/// which symbols have line ranges overlapping the changed ranges.
/// `symbols_fn` is a callback that returns symbols for a given file path.
/// Builtin noise names (map, filter, clone, etc.) are excluded.
pub fn map_hunks_to_symbols<F>(hunks: &[DiffHunk], symbols_fn: F) -> Vec<AffectedSymbol>
where
    F: Fn(&str) -> Vec<(String, String, u32, u32)>, // (name, kind, line_start, line_end)
{
    let mut affected = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for hunk in hunks {
        let symbols = symbols_fn(&hunk.file_path);
        for (name, kind, sym_start, sym_end) in &symbols {
            // Check line range overlap
            if hunk.start_line <= *sym_end && hunk.end_line >= *sym_start {
                let key = format!("{}::{}", hunk.file_path, name);
                if seen.insert(key) {
                    // Compute overlap ratio
                    let overlap_start = hunk.start_line.max(*sym_start);
                    let overlap_end = hunk.end_line.min(*sym_end);
                    let overlap_lines = (overlap_end as f64 - overlap_start as f64 + 1.0).max(0.0);
                    let symbol_lines = (*sym_end as f64 - *sym_start as f64 + 1.0).max(1.0);
                    let ratio = (overlap_lines / symbol_lines).clamp(0.0, 1.0);

                    affected.push(AffectedSymbol {
                        name: name.clone(),
                        file_path: hunk.file_path.clone(),
                        line_start: *sym_start,
                        line_end: *sym_end,
                        kind: kind.clone(),
                        overlap_ratio: ratio,
                    });
                }
            }
        }
    }

    affected
}

/// Extract only the lines relevant to affected symbols from source code.
///
/// Given a file's lines and a list of affected symbols, returns a compact
/// string with 2 lines of context before each symbol and 1 line after,
/// separated by "..." for non-contiguous ranges. Overlapping ranges are merged.
///
/// Inspired by code-review-graph's `_extract_relevant_lines()`.
pub fn extract_relevant_lines(
    lines: &[&str],
    symbols: &[(u32, u32)], // (line_start, line_end) — 1-indexed
    max_total_lines: usize,
) -> String {
    if symbols.is_empty() || lines.is_empty() {
        // Fallback: show first 20 lines
        return lines
            .iter()
            .take(20)
            .enumerate()
            .map(|(i, l)| format!("{}: {}", i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");
    }

    // Build ranges with context (2 before, 1 after)
    let mut ranges: Vec<(usize, usize)> = symbols
        .iter()
        .map(|(start, end)| {
            let s = (*start as usize).saturating_sub(3); // 2 lines before (0-indexed)
            let e = (*end as usize).min(lines.len()); // 1 line after
            (s, e)
        })
        .collect();

    // Sort and merge overlapping ranges
    ranges.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 + 1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }

    // Build output with "..." separators
    let mut parts: Vec<String> = Vec::new();
    let mut total_lines = 0;
    for (i, (start, end)) in merged.iter().enumerate() {
        if i > 0 {
            parts.push("...".to_string());
        }
        for line_idx in *start..*end {
            if let Some(line) = lines.get(line_idx) {
                if total_lines >= max_total_lines {
                    break;
                }
                parts.push(format!("{}: {}", line_idx + 1, line));
                total_lines += 1;
            }
        }
    }

    if total_lines >= max_total_lines {
        parts.push(format!(
            "... ({} more lines truncated)",
            lines.len() - total_lines
        ));
    }

    parts.join("\n")
}

/// Run `git diff --unified=0` and parse the output.
///
/// Returns hunks for all changed files relative to `base` ref.
/// Safe: validates base ref, uses list args (no shell=True), has timeout.
pub fn git_diff_hunks(repo_root: &Path, base: &str) -> Result<Vec<DiffHunk>, DiffHunkError> {
    // Validate base ref (allowlist pattern)
    let safe_ref = Regex::new(r"^[A-Za-z0-9_.~^/@{}\-]+$").expect("valid regex");
    if !safe_ref.is_match(base) {
        return Err(format!("Invalid git ref rejected: {}", base).into());
    }

    let output = std::process::Command::new("git")
        .args(["diff", "--unified=0", base, "--"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("git diff failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git diff error (rc={}): {}",
            output.status,
            &stderr[..stderr.len().min(200)]
        )
        .into());
    }

    let diff_text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_unified_diff(&diff_text))
}

/// Error from [`git_diff_hunks`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DiffHunkError(pub String);

impl From<String> for DiffHunkError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DIFF: &str = r#"diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,5 @@ fn main() {
+    let x = 1;
+    let y = 2;
     println!("hello");
@@ -30,0 +32,2 @@ fn helper() {
+    // new code
+    do_something();
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5,2 +5,3 @@ pub fn foo() {
+    bar();
     baz();
"#;

    #[test]
    fn test_parse_unified_diff_multi_file() {
        let hunks = parse_unified_diff(SAMPLE_DIFF);
        assert_eq!(hunks.len(), 3, "Should find 3 hunks: {:?}", hunks);
        assert_eq!(hunks[0].file_path, "src/main.rs");
        assert_eq!(hunks[1].file_path, "src/main.rs");
        assert_eq!(hunks[2].file_path, "src/lib.rs");
    }

    #[test]
    fn test_parse_hunk_line_ranges() {
        let hunks = parse_unified_diff(SAMPLE_DIFF);
        // First hunk: +10,5 → lines 10-14
        assert_eq!(hunks[0].start_line, 10);
        assert_eq!(hunks[0].end_line, 14);
        // Second hunk: +32,2 → lines 32-33
        assert_eq!(hunks[1].start_line, 32);
        assert_eq!(hunks[1].end_line, 33);
    }

    #[test]
    fn test_parse_deletion_hunk() {
        let diff = "--- a/f.rs\n+++ b/f.rs\n@@ -10,3 +10,0 @@ fn x() {\n";
        let hunks = parse_unified_diff(diff);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].change_type, ChangeType::Deleted);
    }

    #[test]
    fn test_map_hunks_overlap() {
        let hunks = vec![DiffHunk {
            file_path: "src/main.rs".to_string(),
            start_line: 12,
            end_line: 14,
            change_type: ChangeType::Modified,
        }];

        let symbols_fn = |_path: &str| -> Vec<(String, String, u32, u32)> {
            vec![
                ("main".to_string(), "Function".to_string(), 1, 20),
                ("helper".to_string(), "Function".to_string(), 25, 40),
            ]
        };

        let affected = map_hunks_to_symbols(&hunks, symbols_fn);
        assert_eq!(affected.len(), 1, "Only main() overlaps");
        assert_eq!(affected[0].name, "main");
        // overlap: lines 12-14 in a 1-20 function = 3/20 = 0.15
        assert!((affected[0].overlap_ratio - 0.15).abs() < 0.01);
    }

    #[test]
    fn test_map_hunks_no_overlap() {
        let hunks = vec![DiffHunk {
            file_path: "src/main.rs".to_string(),
            start_line: 50,
            end_line: 55,
            change_type: ChangeType::Modified,
        }];

        let symbols_fn = |_: &str| -> Vec<(String, String, u32, u32)> {
            vec![("main".to_string(), "Function".to_string(), 1, 20)]
        };

        let affected = map_hunks_to_symbols(&hunks, symbols_fn);
        assert!(affected.is_empty(), "No symbol overlaps line 50-55");
    }

    #[test]
    fn test_map_hunks_full_overlap() {
        let hunks = vec![DiffHunk {
            file_path: "f.rs".to_string(),
            start_line: 1,
            end_line: 10,
            change_type: ChangeType::Modified,
        }];

        let symbols_fn = |_: &str| -> Vec<(String, String, u32, u32)> {
            vec![("small_fn".to_string(), "Function".to_string(), 3, 7)]
        };

        let affected = map_hunks_to_symbols(&hunks, symbols_fn);
        assert_eq!(affected.len(), 1);
        assert!(
            (affected[0].overlap_ratio - 1.0).abs() < 0.01,
            "Full overlap should be 1.0"
        );
    }

    #[test]
    fn test_map_hunks_deduplicates() {
        // Two hunks in same file overlapping same function
        let hunks = vec![
            DiffHunk {
                file_path: "f.rs".to_string(),
                start_line: 5,
                end_line: 7,
                change_type: ChangeType::Modified,
            },
            DiffHunk {
                file_path: "f.rs".to_string(),
                start_line: 8,
                end_line: 10,
                change_type: ChangeType::Modified,
            },
        ];

        let symbols_fn = |_: &str| -> Vec<(String, String, u32, u32)> {
            vec![("func".to_string(), "Function".to_string(), 1, 20)]
        };

        let affected = map_hunks_to_symbols(&hunks, symbols_fn);
        assert_eq!(affected.len(), 1, "Should deduplicate same symbol");
    }

    #[test]
    fn test_invalid_git_ref_rejected() {
        let result = git_diff_hunks(Path::new("/tmp"), "$(rm -rf /)");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid git ref"));
    }

    #[test]
    fn test_extract_relevant_lines_basic() {
        let lines: Vec<&str> = (0..50).map(|_| "code here").collect();
        let symbols = vec![(10, 15), (30, 35)];
        let result = extract_relevant_lines(&lines, &symbols, 100);
        assert!(
            result.contains("..."),
            "Should have separator between ranges"
        );
        assert!(
            result.contains("8:"),
            "Should have 2 lines context before line 10"
        );
        assert!(result.contains("35:"), "Should include line 35");
    }

    #[test]
    fn test_extract_relevant_lines_merge_overlapping() {
        let lines: Vec<&str> = (0..30).map(|_| "x").collect();
        let symbols = vec![(10, 15), (13, 20)]; // overlapping
        let result = extract_relevant_lines(&lines, &symbols, 100);
        // Should merge into one range, no "..." separator
        let separator_count = result.matches("...").count();
        assert_eq!(separator_count, 0, "Overlapping ranges should merge");
    }

    #[test]
    fn test_extract_relevant_lines_empty_symbols() {
        let lines: Vec<&str> = (0..50).map(|_| "code").collect();
        let result = extract_relevant_lines(&lines, &[], 100);
        assert!(result.contains("1:"), "Fallback shows first lines");
    }

    #[test]
    fn test_extract_relevant_lines_max_limit() {
        let lines: Vec<&str> = (0..100).map(|_| "long line").collect();
        let symbols = vec![(1, 50), (60, 90)];
        let result = extract_relevant_lines(&lines, &symbols, 10);
        let line_count = result.lines().filter(|l| !l.starts_with("...")).count();
        assert!(line_count <= 10, "Should respect max_total_lines limit");
    }

    #[test]
    fn test_is_builtin_noise() {
        assert!(is_builtin_noise("map"), "map is builtin noise");
        assert!(is_builtin_noise("clone"), "clone is builtin noise");
        assert!(is_builtin_noise("new"), "new is builtin noise");
        assert!(is_builtin_noise("__init__"), "__init__ is builtin noise");
        assert!(
            !is_builtin_noise("authenticate"),
            "authenticate is NOT noise"
        );
        assert!(
            !is_builtin_noise("parse_config"),
            "parse_config is NOT noise"
        );
    }
}
