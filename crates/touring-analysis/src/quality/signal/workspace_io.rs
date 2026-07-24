//! Walk a path on disk and assemble a [`Workspace`] handle.
//!
//! This module is the bridge between a real Rust source tree and the
//! `compute_quality_signal` aggregator. It is shared by:
//!
//! * the `touring quality-signal` CLI command (`crates/touring-server/src/cli/quality_signal.rs`)
//! * the `cli-quality-signal` daemon handler (`crates/touring-hooks/src/cli_handlers.rs`)
//! * the optional `quality_signal_real_workspace` example
//!
//! The extraction is deliberately cheap and dependency-free: it does not
//! invoke `tree-sitter`, `syn`, or any AST parser. Callers that need
//! function-accurate cyclomatic complexity should populate `function_cc`
//! from `touring-ast` and skip this helper.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::types::{FuncComplexity, Workspace};

/// Errors that can be reported when assembling a [`Workspace`] from disk.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceIoError {
    /// The supplied root does not exist or is not readable.
    #[error("workspace root not found or unreadable: {path}: {source}")]
    NotFound {
        /// Path that failed to resolve.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// A specific file could not be read.
    #[error("failed reading {path}: {source}")]
    Read {
        /// File that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
}

/// Assemble a [`Workspace`] by walking `root` for `.rs` files.
///
/// The walker:
///
/// * Collects per-file line counts.
/// * Extracts file → file edges from `use crate::X`, `use super::X`, and
///   `mod X` statements (only when `X` resolves to another file in the
///   walk).
/// * Counts a coarse cyclomatic complexity proxy per top-level fn (counts
///   of `if/match/while/for/&&/||/?` tokens).
///
/// The walker skips `target/`, `.git/`, and `node_modules/` directories
/// to keep large repos snappy.
///
/// # Errors
///
/// Returns [`WorkspaceIoError::NotFound`] if `root` cannot be read, or
/// [`WorkspaceIoError::Read`] for individual files that fail.
pub fn build_workspace_from_path(root: &Path) -> Result<Workspace, WorkspaceIoError> {
    let mut ws = Workspace::empty(root);
    let mut files: Vec<PathBuf> = Vec::new();

    collect_rs_files(root, &mut files).map_err(|source| WorkspaceIoError::NotFound {
        path: root.to_path_buf(),
        source,
    })?;

    let module_index = build_module_index(&files, root);

    for file in &files {
        let content = fs::read_to_string(file).map_err(|source| WorkspaceIoError::Read {
            path: file.clone(),
            source,
        })?;
        let rel = relativise(file, root);

        ws.file_lines.insert(rel.clone(), content.lines().count());

        for target in extract_local_imports(&content, &module_index) {
            ws.edges.push((rel.clone(), target.clone()));
            *ws.file_fan_out.entry(rel.clone()).or_default() += 1;
            *ws.file_fan_in.entry(target).or_default() += 1;
        }

        for (name, cc) in extract_function_cc(&content) {
            ws.function_cc.push(FuncComplexity {
                file: rel.clone(),
                func: name,
                cc,
            });
        }
    }

    Ok(ws)
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if dir.is_file() {
        if dir.extension().is_some_and(|e| e == "rs") {
            out.push(dir.to_path_buf());
        }
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if matches!(
                path.file_name().and_then(|s| s.to_str()),
                Some("target" | ".git" | "node_modules")
            ) {
                continue;
            }
            collect_rs_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn relativise(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .into_owned()
}

fn build_module_index(files: &[PathBuf], root: &Path) -> HashMap<String, String> {
    let mut idx: HashMap<String, String> = HashMap::new();
    for file in files {
        let rel = relativise(file, root);
        let stem = rel
            .strip_suffix(".rs")
            .unwrap_or(&rel)
            .replace('/', "::")
            .replace("::mod", "");
        idx.insert(stem.clone(), rel.clone());
        if let Some(leaf) = stem.rsplit("::").next() {
            idx.entry(leaf.to_string()).or_insert_with(|| rel.clone());
        }
    }
    idx
}

fn extract_local_imports(source: &str, module_index: &HashMap<String, String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("use crate::") {
            if let Some(target) = rest.split([':', ';', '{', ' ']).next() {
                if let Some(file) = module_index.get(target) {
                    out.push(file.clone());
                }
            }
        } else if let Some(rest) = line.strip_prefix("use super::") {
            if let Some(target) = rest.split([':', ';', '{', ' ']).next() {
                if let Some(file) = module_index.get(target) {
                    out.push(file.clone());
                }
            }
        } else if let Some(rest) = line.strip_prefix("mod ") {
            if let Some(name) = rest.split([';', ' ', '{']).next() {
                if let Some(file) = module_index.get(name) {
                    out.push(file.clone());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn extract_function_cc(source: &str) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = Vec::new();
    let mut current: Option<(String, u32)> = None;
    let mut depth: i32 = 0;
    let mut in_fn = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if !in_fn {
            if let Some(name) = parse_top_level_fn(trimmed) {
                current = Some((name, 1));
                in_fn = true;
            }
        }

        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth <= 0 && in_fn {
                    if let Some((name, cc)) = current.take() {
                        out.push((name, cc));
                    }
                    in_fn = false;
                    depth = 0;
                }
            }
        }

        if in_fn {
            if let Some((_, cc)) = current.as_mut() {
                let inc = count_decision_tokens(trimmed);
                *cc = cc.saturating_add(inc);
            }
        }
    }
    out
}

fn parse_top_level_fn(line: &str) -> Option<String> {
    if !line.contains("fn ") {
        return None;
    }
    let stripped = line.trim_start_matches("pub ").trim_start();
    let stripped = stripped
        .trim_start_matches("pub(crate) ")
        .trim_start_matches("pub(super) ");
    let stripped = stripped.trim_start_matches("async ").trim_start();
    let stripped = stripped.trim_start_matches("const ").trim_start();
    let stripped = stripped.trim_start_matches("unsafe ").trim_start();
    let rest = stripped.strip_prefix("fn ")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

fn count_decision_tokens(line: &str) -> u32 {
    let mut n = 0u32;
    for kw in [
        " if ", " match ", " while ", " for ", " &&", " ||", "? ", " ? ",
    ] {
        n += line.matches(kw).count() as u32;
    }
    if line.starts_with("if ") || line.starts_with("match ") {
        n = n.saturating_add(1);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_tmp(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir parent");
        }
        let mut f = fs::File::create(&path).expect("create file");
        f.write_all(body.as_bytes()).expect("write body");
        path
    }

    #[test]
    fn missing_root_returns_not_found() {
        let bogus = PathBuf::from("/nonexistent/root/that/will/not/exist");
        let err = build_workspace_from_path(&bogus).unwrap_err();
        match err {
            WorkspaceIoError::NotFound { .. } => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn empty_dir_yields_empty_workspace() {
        let tmp = tempdir();
        let ws = build_workspace_from_path(tmp.path()).expect("walk");
        assert_eq!(ws.node_count(), 0);
        assert_eq!(ws.edge_count(), 0);
        assert_eq!(ws.function_count(), 0);
    }

    #[test]
    fn single_file_extracts_lines_and_function() {
        let tmp = tempdir();
        write_tmp(
            tmp.path(),
            "lib.rs",
            "fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n",
        );
        let ws = build_workspace_from_path(tmp.path()).expect("walk");
        // node_count is deduped over edge endpoints; with no edges it is 0.
        // Validate file discovery via file_lines and function extraction.
        assert_eq!(ws.file_lines.len(), 1, "expected 1 file_lines entry");
        assert_eq!(ws.function_count(), 1);
        assert!(ws.file_lines.values().any(|&n| n >= 3));
    }

    #[test]
    fn use_super_creates_edge() {
        let tmp = tempdir();
        write_tmp(tmp.path(), "lib.rs", "pub mod a;\npub mod b;\n");
        write_tmp(tmp.path(), "a.rs", "pub fn alpha() {}\n");
        write_tmp(
            tmp.path(),
            "b.rs",
            "use super::a;\npub fn beta() { a::alpha(); }\n",
        );
        let ws = build_workspace_from_path(tmp.path()).expect("walk");
        assert!(
            ws.edge_count() >= 1,
            "expected at least 1 edge, got {}",
            ws.edge_count()
        );
    }

    #[test]
    fn target_directory_is_skipped() {
        let tmp = tempdir();
        write_tmp(tmp.path(), "src/lib.rs", "fn ok() {}\n");
        write_tmp(
            tmp.path(),
            "target/debug/build/junk.rs",
            "fn ignored() {}\n",
        );
        let ws = build_workspace_from_path(tmp.path()).expect("walk");
        assert!(
            ws.function_cc.iter().all(|f| f.func != "ignored"),
            "target/ should be skipped"
        );
    }

    #[test]
    fn decision_tokens_are_counted() {
        let body = "fn branchy(x: u32) -> u32 {\n    if x > 0 { x } else { 0 }\n}\n";
        let tmp = tempdir();
        write_tmp(tmp.path(), "lib.rs", body);
        let ws = build_workspace_from_path(tmp.path()).expect("walk");
        let cc = ws
            .function_cc
            .iter()
            .find(|f| f.func == "branchy")
            .map(|f| f.cc);
        assert!(cc.is_some_and(|n| n >= 2), "expected CC >= 2, got {cc:?}");
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create tempdir")
    }
}
