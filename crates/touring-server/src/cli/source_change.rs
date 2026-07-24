//! `touring source-change` — Apply or preview a transactional multi-file change.
//!
//! ## Subcommands
//!
//! - `preview --file <path> [-j]` — Dry-run: shadow-validate without writing
//! - `apply --file <path> [-j]`   — Commit all edits atomically, rollback on failure
//! - `validate --file <path> [-j]` — Alias for preview
//!
//! ## SourceChange JSON format
//!
//! ```json
//! {
//!   "edits": {
//!     "file_path_hash": [
//!       { "delete": [start, end], "insert": "text" }
//!     ]
//!   },
//!   "fs_edits": [
//!     { "CreateFile": { "path": "path/to/new.rs", "content": "..." } },
//!     { "OverwriteFile": { "path": "path/existing.rs", "content": "..." } },
//!     { "MoveFile": { "from": "a.rs", "to": "b.rs" } },
//!     { "DeleteFile": { "path": "dead_code.rs" } }
//!   ],
//!   "snippet": { "template": "fn $0()", "tab_stops": [] }
//! }
//! ```
//!
//! FileId = `hash(path) as usize` (same as typestate.rs commit path).
//!
//! Wave P3-1.3 W5a (2026-06-11): migrated from manual flag_value positional
//! parsing to clap derive. The dispatch contract is unchanged — `run` still
//! receives the full argv slice; clap parses `args[1..]` so the subcommand
//! name ("source-change") acts as the program name.
//!
//! G6 payload byte-compatible: no daemon_query calls in this module — pure
//! touring_generator library invocations are unchanged.

use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use touring_generator::{Applier, ApplyResult, FileId, Indel, SourceChange, TextEdit};

#[derive(Parser, Debug)]
#[command(
    name = "touring source-change",
    bin_name = "touring source-change",
    about = "Apply or preview a transactional multi-file change",
    disable_help_subcommand = true
)]
struct SourceChangeCli {
    /// Output results as JSON.
    #[arg(short = 'j', long = "json", global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Option<SourceChangeCmd>,
}

#[derive(Subcommand, Debug)]
enum SourceChangeCmd {
    /// Dry-run: shadow-validate the change without writing (default).
    Preview {
        /// Path to a SourceChange JSON file.
        #[arg(long)]
        file: String,
    },
    /// Alias for preview: shadow-validate without writing.
    Validate {
        /// Path to a SourceChange JSON file.
        #[arg(long)]
        file: String,
    },
    /// Commit all edits atomically; rollback on any failure.
    Apply {
        /// Path to a SourceChange JSON file.
        #[arg(long)]
        file: String,
    },
}

/// CLI entry point for `touring source-change` subcommand.
///
/// # Errors
///
/// Returns an error if the SourceChange JSON file cannot be read, parsed, or
/// if shadow validation / atomic commit fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match SourceChangeCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    let is_json = cli.json;
    let cmd = cli.cmd.unwrap_or(SourceChangeCmd::Preview {
        file: String::new(),
    });

    match cmd {
        SourceChangeCmd::Preview { file } | SourceChangeCmd::Validate { file } => {
            if file.is_empty() {
                anyhow::bail!("--file <path> required");
            }
            run_preview(&file, is_json)
        }
        SourceChangeCmd::Apply { file } => {
            if file.is_empty() {
                anyhow::bail!("--file <path> required");
            }
            run_apply(&file, is_json)
        }
    }
}

// ── preview (dry-run) ─────────────────────────────────────────────────────────

/// `touring source-change preview --file <path> [-j]`
fn run_preview(file_path: &str, is_json: bool) -> anyhow::Result<()> {
    let json_str = fs::read_to_string(file_path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", file_path, e))?;

    let source_change: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| anyhow::anyhow!("invalid JSON: {}", e))?;

    let (change, files, _paths) = build_source_change_and_files(&source_change, file_path)?;

    let applier = Applier::new();
    let validation = applier.shadow_validate(&change);

    match validation {
        ApplyResult::Valid => {
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "valid",
                        "files": files.len(),
                        "fs_ops": change.fs_edit_count(),
                        "snippet": change.snippet().is_some()
                    })
                );
            } else {
                println!("✓ Shadow validation passed — all edits are consistent");
                println!(
                    "  Files: {}, FS ops: {}",
                    files.len(),
                    change.fs_edit_count()
                );
                if change.snippet().is_some() {
                    println!("  Snippet: present");
                }
            }
        }
        ApplyResult::Invalid { errors } => {
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "invalid",
                        "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>()
                    })
                );
            } else {
                println!("✗ Shadow validation FAILED — {} error(s):", errors.len());
                for e in &errors {
                    println!("  • {}", e);
                }
            }
            std::process::exit(1);
        }
        ApplyResult::Committed { .. } | ApplyResult::RolledBack { .. } => {
            // Shouldn't happen from shadow_validate, but handle gracefully
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "unexpected",
                        "message": "shadow_validate returned commit result"
                    })
                );
            } else {
                println!("? Unexpected result from shadow_validate");
            }
        }
    }
    Ok(())
}

// ── apply (atomic commit) ──────────────────────────────────────────────────────

/// `touring source-change apply --file <path> [-j]`
fn run_apply(file_path: &str, is_json: bool) -> anyhow::Result<()> {
    let json_str = fs::read_to_string(file_path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", file_path, e))?;

    let source_change: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| anyhow::anyhow!("invalid JSON: {}", e))?;

    let (change, mut files, paths) = build_source_change_and_files(&source_change, file_path)?;

    let applier = Applier::new();

    // Phase 1: shadow validate
    let validation = applier.shadow_validate(&change);
    match validation {
        ApplyResult::Invalid { errors } => {
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "invalid",
                        "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>()
                    })
                );
            } else {
                println!("✗ Validation failed — {} error(s):", errors.len());
                for e in &errors {
                    println!("  • {}", e);
                }
            }
            std::process::exit(1);
        }
        ApplyResult::Valid => { /* continue to commit */ }
        ApplyResult::Committed { .. } | ApplyResult::RolledBack { .. } => {
            // Unexpected from shadow_validate, treat as invalid
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "unexpected",
                        "message": "shadow_validate returned commit result"
                    })
                );
            } else {
                println!("? Unexpected result from shadow_validate");
            }
            std::process::exit(1);
        }
    }

    // Phase 2: commit
    let path_for = |file_id: FileId| paths.get(&file_id).cloned();
    let result = applier.commit(&change, &mut files, path_for);

    match result {
        ApplyResult::Committed {
            files_written,
            fs_ops,
        } => {
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "committed",
                        "files_written": files_written,
                        "fs_ops": fs_ops
                    })
                );
            } else {
                println!("✓ Atomic commit succeeded");
                println!("  Files written: {}, FS ops: {}", files_written, fs_ops);
            }
        }
        ApplyResult::RolledBack {
            errors,
            partial_writes,
        } => {
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "rolled_back",
                        "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
                        "partial_writes": partial_writes
                    })
                );
            } else {
                println!("✗ Commit FAILED and was rolled back:");
                println!(
                    "  {} error(s), {} partial writes reverted",
                    errors.len(),
                    partial_writes
                );
                for e in &errors {
                    println!("  • {}", e);
                }
            }
            std::process::exit(1);
        }
        ApplyResult::Valid | ApplyResult::Invalid { .. } => {
            // Unexpected
            if is_json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "unexpected",
                        "message": "commit returned validation result"
                    })
                );
            } else {
                println!("? Unexpected result from commit");
            }
        }
    }
    Ok(())
}

// ── JSON parsing helpers ───────────────────────────────────────────────────────

#[allow(clippy::type_complexity)]
fn build_source_change_and_files(
    value: &serde_json::Value,
    origin_file: &str,
) -> anyhow::Result<(
    SourceChange,
    BTreeMap<FileId, String>,
    BTreeMap<FileId, PathBuf>,
)> {
    let mut change = SourceChange::new();
    let mut files: BTreeMap<FileId, String> = BTreeMap::new();
    let mut paths: BTreeMap<FileId, PathBuf> = BTreeMap::new();

    // Parse edits { file_path: [indels] }
    if let Some(edits) = value.get("edits").and_then(|v| v.as_object()) {
        for (file_path, indels_val) in edits {
            let file_id = path_to_file_id(file_path);

            // Read current file content from disk
            let disk_content = std::fs::read_to_string(file_path).unwrap_or_default();
            files.insert(file_id, disk_content);
            paths.insert(file_id, PathBuf::from(file_path));

            // Parse indels array
            let indels_array = indels_val
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("edits[{}] must be an array", file_path))?;

            let mut indels = Vec::new();
            for indel_val in indels_array {
                let delete = indel_val
                    .get("delete")
                    .and_then(|v| v.as_array())
                    .filter(|a| a.len() == 2);
                let insert = indel_val
                    .get("insert")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let (start, end) = match delete {
                    Some(arr) if arr.len() == 2 => {
                        let s = arr[0].as_u64().unwrap_or(0) as usize;
                        let e = arr[1].as_u64().unwrap_or(0) as usize;
                        (s, e)
                    }
                    _ => continue, // skip malformed entries
                };

                indels.push(Indel {
                    delete: start..end,
                    insert: insert.to_string(),
                });
            }

            if !indels.is_empty() {
                let text_edit = TextEdit::try_from_iter(indels)
                    .map_err(|e| anyhow::anyhow!("invalid TextEdit for {}: {}", file_path, e))?;
                change = change.with_edit(file_id, text_edit);
            }
        }
    }

    // Parse fs_edits
    if let Some(fs_edits) = value.get("fs_edits").and_then(|v| v.as_array()) {
        for fs_edit_val in fs_edits {
            if let Some(obj) = fs_edit_val.as_object() {
                // Each fs_edit is a single-key variant
                for (variant, payload) in obj {
                    match variant.as_str() {
                        "CreateFile" => {
                            if let Some(fields) = payload.as_object() {
                                let path = fields
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                let content = fields
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                use touring_generator::FileSystemEdit;
                                change = change.with_fs_edit(FileSystemEdit::CreateFile {
                                    path: std::path::PathBuf::from(path),
                                    content: content.to_string(),
                                });
                            }
                        }
                        "OverwriteFile" => {
                            if let Some(fields) = payload.as_object() {
                                let path = fields
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                let content = fields
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                use touring_generator::FileSystemEdit;
                                change = change.with_fs_edit(FileSystemEdit::OverwriteFile {
                                    path: std::path::PathBuf::from(path),
                                    content: content.to_string(),
                                });
                            }
                        }
                        "MoveFile" => {
                            if let Some(fields) = payload.as_object() {
                                let from = fields
                                    .get("from")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                let to = fields
                                    .get("to")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                use touring_generator::FileSystemEdit;
                                change = change.with_fs_edit(FileSystemEdit::MoveFile {
                                    from: std::path::PathBuf::from(from),
                                    to: std::path::PathBuf::from(to),
                                });
                            }
                        }
                        "DeleteFile" => {
                            if let Some(fields) = payload.as_object() {
                                let path = fields
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                use touring_generator::FileSystemEdit;
                                change = change.with_fs_edit(FileSystemEdit::DeleteFile {
                                    path: std::path::PathBuf::from(path),
                                });
                            }
                        }
                        _ => { /* unknown variant, skip */ }
                    }
                }
            }
        }
    }

    // Parse snippet
    if let Some(snippet_val) = value.get("snippet").and_then(|v| v.as_object()) {
        let template = snippet_val
            .get("template")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        use touring_generator::SnippetEdit;
        let tab_stops = snippet_val
            .get("tab_stops")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        let index = v.get("index")?.as_u64()? as u8;
                        let default_text =
                            v.get("default_text").and_then(|w| w.as_str()).unwrap_or("");
                        Some(touring_generator::TabStop {
                            index,
                            default_text: default_text.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let snippet_edit = SnippetEdit::with_tab_stops(
            template.to_string(),
            tab_stops,
            None, // cursor_position auto-parsed from $0 in template
        );
        change = change.with_snippet(snippet_edit);
    }

    let _ = origin_file; // unused but may be useful for relative path resolution
    Ok((change, files, paths))
}

/// Convert a file path string to a FileId via hash.
fn path_to_file_id(path: &str) -> FileId {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish() as FileId
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    SourceChangeCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── clap parsing ──────────────────────────────────────────────────────────

    fn parse(args: &[&str]) -> SourceChangeCli {
        SourceChangeCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_source_change_defaults_to_preview_none() {
        let cli = parse(&["source-change"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Preview with empty file
        assert!(!cli.json);
    }

    #[test]
    fn preview_with_file_flag() {
        let cli = parse(&["source-change", "preview", "--file", "/tmp/change.json"]);
        let Some(SourceChangeCmd::Preview { file }) = cli.cmd else {
            panic!("expected Preview");
        };
        assert_eq!(file, "/tmp/change.json");
    }

    #[test]
    fn validate_with_file_flag() {
        let cli = parse(&["source-change", "validate", "--file", "/tmp/change.json"]);
        let Some(SourceChangeCmd::Validate { file }) = cli.cmd else {
            panic!("expected Validate");
        };
        assert_eq!(file, "/tmp/change.json");
    }

    #[test]
    fn apply_with_file_flag() {
        let cli = parse(&["source-change", "apply", "--file", "/tmp/change.json"]);
        let Some(SourceChangeCmd::Apply { file }) = cli.cmd else {
            panic!("expected Apply");
        };
        assert_eq!(file, "/tmp/change.json");
    }

    #[test]
    fn json_flag_short() {
        let cli = parse(&["source-change", "preview", "--file", "/tmp/c.json", "-j"]);
        assert!(cli.json);
    }

    #[test]
    fn json_flag_long() {
        let cli = parse(&["source-change", "apply", "--file", "/tmp/c.json", "--json"]);
        assert!(cli.json);
    }

    #[test]
    fn json_flag_before_subcommand() {
        let cli = parse(&["source-change", "-j", "preview", "--file", "/tmp/c.json"]);
        assert!(cli.json);
        let Some(SourceChangeCmd::Preview { file }) = cli.cmd else {
            panic!("expected Preview");
        };
        assert_eq!(file, "/tmp/c.json");
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(SourceChangeCli::try_parse_from(["source-change", "frobnicate"]).is_err());
    }

    // ── domain logic (unchanged from original) ────────────────────────────────

    #[test]
    fn path_to_file_id_deterministic() {
        let id1 = path_to_file_id("src/main.rs");
        let id2 = path_to_file_id("src/main.rs");
        assert_eq!(id1, id2, "same path must produce same FileId");
    }

    #[test]
    fn path_to_file_id_different_paths() {
        let id1 = path_to_file_id("src/a.rs");
        let id2 = path_to_file_id("src/b.rs");
        assert_ne!(id1, id2, "different paths should produce different FileIds");
    }

    #[test]
    fn build_source_change_from_valid_json() {
        let json = serde_json::json!({
            "edits": {
                "src/main.rs": [
                    {"delete": [0, 5], "insert": "pub fn main() {"}
                ]
            },
            "fs_edits": [],
            "snippet": null
        });
        let (change, files, _paths) =
            build_source_change_and_files(&json, "/tmp/test.json").unwrap();
        assert_eq!(files.len(), 1);
        assert!(change.fs_edit_count() == 0);
    }

    #[test]
    fn build_source_change_with_fs_edits() {
        let json = serde_json::json!({
            "edits": {},
            "fs_edits": [
                {"CreateFile": {"path": "new.rs", "content": "// new file"}},
                {"DeleteFile": {"path": "old.rs"}}
            ]
        });
        let (change, _, _) = build_source_change_and_files(&json, "/tmp/test.json").unwrap();
        assert_eq!(change.fs_edit_count(), 2);
    }

    #[test]
    fn build_source_change_with_snippet() {
        let json = serde_json::json!({
            "edits": {},
            "fs_edits": [],
            "snippet": {
                "template": "fn $0() {\n    $BODY\n}",
                "tab_stops": [
                    {"index": 1, "default_text": "placeholder"}
                ]
            }
        });
        let (change, _, _) = build_source_change_and_files(&json, "/tmp/test.json").unwrap();
        assert!(change.snippet().is_some());
    }

    #[test]
    fn build_source_change_empty_json() {
        let json = serde_json::json!({});
        let (change, files, _) = build_source_change_and_files(&json, "/tmp/test.json").unwrap();
        assert_eq!(files.len(), 0);
        assert_eq!(change.fs_edit_count(), 0);
    }
}
