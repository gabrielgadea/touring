//! Clone Detection Tools — structural clone detection via touring_code::ast::symbols::find_clones.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::server::params::DetailLevel;

use touring_code::ast::languages::Lang;
use touring_code::ast::symbols::{Symbol, find_clones};

/// Parameters for `touring_detect_clones` — detect structural clone groups in a symbol collection.
#[derive(Debug, Clone, Deserialize)]
pub struct DetectClonesParams {
    /// Path to scan (optional, defaults to workspace root).
    pub path: Option<String>,
    /// Minimum similarity threshold (0.0-1.0, default 0.5).
    #[serde(default)]
    pub min_similarity: Option<f32>,
    /// Output verbosity level.
    #[serde(default)]
    pub detail_level: Option<DetailLevel>,
}

/// Response for clone detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectClonesResponse {
    /// Total number of clone groups found.
    pub clone_group_count: usize,
    /// Clone groups with file:line:col, similarity score, and code snippets.
    pub clone_groups: Vec<CloneGroup>,
    /// Summary statistics.
    pub stats: CloneStats,
}

/// A group of symbols that are structural clones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneGroup {
    /// Structural hash bucket (indicates similarity).
    pub hash_bucket: u64,
    /// Number of members in this clone group.
    pub member_count: usize,
    /// Members of the clone group.
    pub members: Vec<CloneMember>,
}

/// A single symbol within a clone group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneMember {
    /// Symbol name.
    pub name: String,
    /// Symbol kind (function, method, class, etc.).
    pub kind: String,
    /// Full file path.
    pub file: String,
    /// Line number (1-indexed).
    pub line: u32,
    /// Column number (1-indexed, if available).
    pub col: Option<u32>,
    /// Signature (function signature or class definition).
    pub signature: String,
    /// Code snippet (truncated to 200 chars).
    pub snippet: String,
}

/// Statistics about the clone detection run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloneStats {
    /// Total symbols scanned.
    pub symbols_scanned: usize,
    /// Total clone groups found (groups with >= 2 members).
    pub clone_groups_found: usize,
    /// Total clone members across all groups.
    pub total_clone_members: usize,
    /// Average group size.
    pub avg_group_size: f64,
}

// ── Internal helpers ─────────────────────────────────────────────────────

const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules", ".next", "dist"];

/// Collect all Rust source files under the given root path.
fn collect_rust_files(root: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_files_recursive(root, &mut files);
    files
}

fn collect_rust_files_recursive(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            collect_rust_files_recursive(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

/// A symbol enriched with its source file path.
struct FileSymbol {
    file: String,
    symbol: Symbol,
}

/// Extract all symbols from all Rust files, enriching with file path.
fn extract_all_symbols(workspace_root: &std::path::Path) -> Vec<FileSymbol> {
    let rust_files = collect_rust_files(workspace_root);
    let mut all = Vec::new();

    for file_path in rust_files {
        let file_str = file_path.to_string_lossy().to_string();
        if let Ok(content) = std::fs::read_to_string(&file_path) {
            if let Ok(symbols) = touring_code::ast::symbols::extract_symbols(&content, Lang::Rust) {
                for sym in symbols {
                    all.push(FileSymbol {
                        file: file_str.clone(),
                        symbol: sym,
                    });
                }
            }
        }
    }
    all
}

/// Transform raw clone groups into CloneGroup DTOs using file path tracking.
fn transform_clone_groups(
    clone_groups_raw: Vec<Vec<Symbol>>,
    all_symbols: &[FileSymbol],
    min_similarity: f32,
) -> (Vec<CloneGroup>, usize) {
    // Build index: symbol -> file path
    let file_by_sig: std::collections::HashMap<*const Symbol, &str> = all_symbols
        .iter()
        .map(|fs| (&fs.symbol as *const Symbol, fs.file.as_str()))
        .collect();

    let mut total_members = 0;
    let groups: Vec<CloneGroup> = clone_groups_raw
        .into_iter()
        .map(|group| transform_single_group(group, &file_by_sig))
        .filter(|group| {
            let similarity_proxy = (group.member_count as f32) / 10.0;
            similarity_proxy >= min_similarity || group.member_count >= 3
        })
        .inspect(|group| total_members += group.member_count)
        .collect();

    (groups, total_members)
}

/// Transform a single clone group using symbol index for file lookup.
fn transform_single_group(
    group: Vec<Symbol>,
    file_by_sig: &std::collections::HashMap<*const Symbol, &str>,
) -> CloneGroup {
    let hash_bucket = group.first().and_then(|s| s.structural_hash).unwrap_or(0);
    let member_count = group.len();

    let members: Vec<CloneMember> = group
        .into_iter()
        .map(|sym| {
            let file = file_by_sig
                .get(&(&sym as *const Symbol))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            CloneMember {
                name: sym.name,
                kind: sym.kind.as_str().to_string(),
                file,
                line: sym.line as u32,
                col: Some(sym.column as u32),
                signature: sym.signature.clone(),
                snippet: truncate_snippet(&sym.signature),
            }
        })
        .collect();

    CloneGroup {
        hash_bucket,
        member_count,
        members,
    }
}

/// Truncate snippet to 200 chars.
fn truncate_snippet(signature: &str) -> String {
    if signature.len() > 200 {
        format!("{}...", &signature[..200])
    } else {
        signature.to_string()
    }
}

/// Compute average group size.
fn compute_avg_group_size(total_members: usize, group_count: usize) -> f64 {
    if group_count > 0 {
        total_members as f64 / group_count as f64
    } else {
        0.0
    }
}

/// Build empty response for early return.
fn empty_response() -> DetectClonesResponse {
    DetectClonesResponse {
        clone_group_count: 0,
        clone_groups: vec![],
        stats: CloneStats {
            symbols_scanned: 0,
            clone_groups_found: 0,
            total_clone_members: 0,
            avg_group_size: 0.0,
        },
    }
}

// ── Main implementation ──────────────────────────────────────────────────

/// Detect structural clone groups in the codebase.
///
/// Uses `touring_code::ast::symbols::find_clones()` which groups symbols by
/// `structural_hash` (bucket-based: kind + param_count + complexity_bucket + line_bucket).
/// Returns groups with >= 2 members as clone groups.
///
/// Returns [`crate::ServerError::CloneDetect`] on analysis failure.
pub fn detect_clones_impl(
    params: DetectClonesParams,
) -> Result<DetectClonesResponse, crate::ServerError> {
    let min_similarity = params.min_similarity.unwrap_or(0.5).clamp(0.0, 1.0);

    let workspace_root = params
        .path
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let all_symbols = extract_all_symbols(&workspace_root);
    if all_symbols.is_empty() {
        return Ok(empty_response());
    }

    let symbols_scanned = all_symbols.len();
    // Extract plain symbols for find_clones
    let plain: Vec<Symbol> = all_symbols.iter().map(|fs| fs.symbol.clone()).collect();
    let mut symbols_for_clones = plain;
    let clone_groups_raw = find_clones(&mut symbols_for_clones);

    let clone_groups_found = clone_groups_raw.len();
    let (clone_groups, total_clone_members) =
        transform_clone_groups(clone_groups_raw, &all_symbols, min_similarity);
    let avg_group_size = compute_avg_group_size(total_clone_members, clone_groups.len());

    Ok(DetectClonesResponse {
        clone_group_count: clone_groups.len(),
        clone_groups,
        stats: CloneStats {
            symbols_scanned,
            clone_groups_found,
            total_clone_members,
            avg_group_size,
        },
    })
}
