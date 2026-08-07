//! Real-workspace validation for `touring_analysis::quality::signal`.
//!
//! Demonstrates `compute_quality_signal` over an actual on-disk Rust
//! source tree. Walks the source tree, extracts:
//!   * file → file edges (use statements pointing to local modules)
//!   * per-file line counts
//!   * a coarse cyclomatic-complexity proxy per top-level fn (counts of
//!     `if/match/while/for/&&/||/?` tokens — purposely cheap, not exact;
//!     in production callers populate `function_cc` from `touring-ast`).
//!
//! Run with:
//!     cargo run -p touring-analysis --example quality_signal_real_workspace
//!     cargo run -p touring-analysis --example quality_signal_real_workspace -- <root>

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use touring_analysis::quality::signal::{FuncComplexity, Workspace, compute_quality_signal};

fn main() -> std::io::Result<()> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default_root = crate_root.join("src/quality/signal");
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or(default_root);

    eprintln!(
        "[quality_signal_real_workspace] scanning: {}",
        root.display()
    );

    let mut ws = Workspace::empty(&root);
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rs_files(&root, &mut files)?;
    eprintln!(
        "[quality_signal_real_workspace] discovered {} .rs files",
        files.len()
    );

    let module_index = build_module_index(&files, &root);

    let mut total_fns = 0usize;
    for file in &files {
        let content = fs::read_to_string(file)?;
        let rel = relativise(file, &root);

        ws.file_lines.insert(rel.clone(), content.lines().count());

        let imports = extract_local_imports(&content, &module_index);
        for target in &imports {
            ws.edges.push((rel.clone(), target.clone()));
            *ws.file_fan_out.entry(rel.clone()).or_default() += 1;
            *ws.file_fan_in.entry(target.clone()).or_default() += 1;
        }

        for (name, cc) in extract_function_cc(&content) {
            ws.function_cc.push(FuncComplexity {
                file: rel.clone(),
                func: name,
                cc,
            });
            total_fns += 1;
        }
    }

    eprintln!(
        "[quality_signal_real_workspace] edges={} nodes={} functions={}",
        ws.edge_count(),
        ws.node_count(),
        total_fns
    );

    let signal = compute_quality_signal(&ws);

    println!(
        "{}",
        serde_json::to_string_pretty(&signal).expect("serialize quality signal")
    );
    Ok(())
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
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
            idx.entry(leaf.to_string()).or_insert(rel.clone());
        }
    }
    idx
}

fn extract_local_imports(source: &str, module_index: &HashMap<String, String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("use crate::") {
            if let Some(target_module) = rest.split([':', ';', '{', ' ']).next()
                && let Some(file) = module_index.get(target_module)
            {
                out.push(file.clone());
            }
        } else if let Some(rest) = line.strip_prefix("use super::") {
            if let Some(target_module) = rest.split([':', ';', '{', ' ']).next()
                && let Some(file) = module_index.get(target_module)
            {
                out.push(file.clone());
            }
        } else if let Some(rest) = line.strip_prefix("mod ")
            && let Some(name) = rest.split([';', ' ', '{']).next()
            && let Some(file) = module_index.get(name)
        {
            out.push(file.clone());
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

        if !in_fn && let Some(name) = parse_top_level_fn(trimmed) {
            current = Some((name, 1));
            in_fn = true;
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

        if in_fn && let Some((_, cc)) = current.as_mut() {
            let inc = count_decision_tokens(trimmed);
            *cc = cc.saturating_add(inc);
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
