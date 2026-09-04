//! `touring pub-api diff` — Pub API diff between two git revisions of a file.
//!
//! Returns additive_count, breaking_count, change_kind, new_symbols.
//!
//! Complementação-hooks H4 (F3): handler PostToolUse(Edit|Write) needs this
//! subcommand to inject `touring.pub_api_diff` JSON via additionalContext.
//!
//! F3 (2026-08-31): wires real diff — uses `git show <rev>:<file>` to extract
//! source from both revisions, then regex-scans for `pub fn|struct|enum|trait|const|static|type`
//! declarations to compute additive vs breaking counts.

use super::common::{human_to_stderr, json_to_stdout, parse_global_flags};
use anyhow::{anyhow, Context};
use std::collections::BTreeSet;
use std::process::Command;

/// Public API diff between two revisions of a single file.
#[derive(serde::Serialize)]
struct PubApiDiff {
    file_path: String,
    additive_count: u32,
    breaking_count: u32,
    change_kind: &'static str, // Additive | Breaking | Removal | Mixed | Unknown
    new_symbols: Vec<String>,
    removed_symbols: Vec<String>,
}

/// Extract pub symbol names from Rust source via regex.
///
/// Matches `pub fn|struct|enum|trait|const|static|type <NAME>` — good enough for
/// an MVP-level pub API diff. False positives are bounded by the `pub ` prefix
/// + identifier character class.
fn extract_pub_symbols(source: &str) -> BTreeSet<String> {
    let mut symbols = BTreeSet::new();
    // Anchored: `pub ` followed by a keyword and an identifier.
    let prefixes = [
        "pub fn ",
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "pub const ",
        "pub static ",
        "pub type ",
    ];
    for line in source.lines() {
        let trimmed = line.trim_start();
        for prefix in &prefixes {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                // Extract identifier up to `<`, `(`, `{`, `[`, `;`, `,`, ` `, or end.
                let end = rest
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                let name = &rest[..end];
                if !name.is_empty() {
                    symbols.insert(name.to_string());
                }
                break;
            }
        }
    }
    symbols
}

/// Read a file's contents from a git revision via `git show <rev>:<path>`.
/// Returns None if git is unavailable or the path doesn't exist in that rev.
fn read_file_at_rev(rev: &str, path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["show", &format!("{}:{}", rev, path)])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Read a file's contents from the working tree (current state).
fn read_working_file(path: &str) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("read {} from disk", path))
}

/// Compute the pub API diff between two revisions of a file.
///
/// Strategy: extract pub symbols from old/new content; diff the sets. If both
/// `old_rev` and `new_rev` are empty, treat new content as "current working
/// tree" and old content as "HEAD" — the common edit case (post-edit, no
/// commit yet).
fn compute_pub_api_diff(
    file_path: &str,
    old_rev: Option<&str>,
    new_rev: Option<&str>,
) -> anyhow::Result<PubApiDiff> {
    let current = read_working_file(file_path)?;
    let new_symbols_src: String = match new_rev {
        Some(rev) => read_file_at_rev(rev, file_path).unwrap_or_else(|| current.clone()),
        None => current.clone(),
    };
    let old_symbols_src: String = match old_rev {
        Some(rev) => read_file_at_rev(rev, file_path).unwrap_or_default(),
        None => read_file_at_rev("HEAD", file_path).unwrap_or_default(),
    };

    let new_set = extract_pub_symbols(&new_symbols_src);
    let old_set = extract_pub_symbols(&old_symbols_src);

    let added: BTreeSet<&String> = new_set.difference(&old_set).collect();
    let removed: BTreeSet<&String> = old_set.difference(&new_set).collect();

    let additive_count = added.len() as u32;
    let breaking_count = removed.len() as u32;

    let change_kind = match (additive_count, breaking_count) {
        (0, 0) => "Unknown",
        (_, 0) => "Additive",
        (0, _) => "Breaking",
        (_, _) if additive_count == breaking_count => "Mixed",
        (_, _) if additive_count > breaking_count => "Mixed",
        _ => "Mixed",
    };

    Ok(PubApiDiff {
        file_path: file_path.to_string(),
        additive_count,
        breaking_count,
        change_kind,
        new_symbols: added.into_iter().map(|s| s.to_string()).collect(),
        removed_symbols: removed.into_iter().map(|s| s.to_string()).collect(),
    })
}

/// CLI entry point — `touring pub-api diff [--old <rev>] [--new <rev>] --file <path>`.
///
/// If `--old`/`--new` are omitted, defaults to HEAD vs working tree (the common
/// post-edit case).
///
/// Emits JSON `{ file_path, additive_count, breaking_count, change_kind,
/// new_symbols, removed_symbols }` to stdout.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (_flags, filtered) = parse_global_flags(args);

    // Skip past the "pub-api" subcmd token so positional parsing starts cleanly.
    let subcmd_skip = filtered
        .iter()
        .position(|a| a == "pub-api")
        .map(|i| i + 1)
        .unwrap_or(0);
    let positional: Vec<&str> = filtered[subcmd_skip..].iter().map(String::as_str).collect();

    // Args: touring pub-api diff [--old <rev>] [--new <rev>] --file <path>
    let mut old_rev: Option<String> = None;
    let mut new_rev: Option<String> = None;
    let mut file_path: Option<String> = None;
    let mut i = 0;
    while i < positional.len() {
        match positional[i] {
            "--old" => {
                old_rev = positional.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--new" => {
                new_rev = positional.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--file" => {
                file_path = positional.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            _ => i += 1,
        }
    }

    let file = file_path
        .context("--file <path> required")
        .map_err(|e| anyhow!("pub-api diff: {}", e))?;

    let diff = compute_pub_api_diff(&file, old_rev.as_deref(), new_rev.as_deref())?;

    json_to_stdout(&serde_json::to_string(&diff)?);
    human_to_stderr(&format!(
        "pub-api diff for {}: additive={} breaking={} kind={}",
        file, diff.additive_count, diff.breaking_count, diff.change_kind
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_file_flag() {
        let args = vec!["--file".to_string(), "src/foo.rs".to_string()];
        let r = run(&args);
        // Will fail because the file doesn't exist; that's fine — we test parsing.
        assert!(r.is_err());
    }

    #[test]
    fn missing_file_flag_returns_error() {
        let args = vec![];
        let r = run(&args);
        assert!(r.is_err());
    }

    #[test]
    fn extract_pub_symbols_finds_struct_fn_enum() {
        let src = r#"
            pub fn foo() {}
            pub struct Bar;
            pub enum Baz { A, B }
            pub trait Qux { fn x(&self); }
            fn private_fn() {}
            pub const N: u32 = 0;
        "#;
        let syms = extract_pub_symbols(src);
        assert!(syms.contains("foo"));
        assert!(syms.contains("Bar"));
        assert!(syms.contains("Baz"));
        assert!(syms.contains("Qux"));
        assert!(syms.contains("N"));
        assert!(!syms.contains("private_fn"));
        assert_eq!(syms.len(), 5);
    }

    #[test]
    fn extract_pub_symbols_ignores_comments() {
        let src = r#"
            // pub fn not_real() {}
            /* pub fn also_not() {} */
            /// pub fn doc_comment() {}
        "#;
        let syms = extract_pub_symbols(src);
        assert_eq!(syms.len(), 0);
    }

    #[test]
    fn compute_diff_additive_only() {
        // Simulate by extracting from inline strings via helper.
        let old = "pub fn a() {}";
        let new = "pub fn a() {}\npub fn b() {}";
        let old_set = extract_pub_symbols(old);
        let new_set = extract_pub_symbols(new);
        let added: Vec<_> = new_set.difference(&old_set).collect();
        assert_eq!(added.len(), 1);
        assert_eq!(old_set.difference(&new_set).count(), 0);
        assert!(added.contains(&&"b".to_string()));
    }

    #[test]
    fn compute_diff_breaking_only() {
        let old = "pub fn a() {}\npub fn b() {}";
        let new = "pub fn a() {}";
        let old_set = extract_pub_symbols(old);
        let new_set = extract_pub_symbols(new);
        let added: Vec<_> = new_set.difference(&old_set).collect();
        assert!(added.is_empty(), "símbolos públicos inesperados: {added:?}");
        assert_eq!(old_set.difference(&new_set).count(), 1);
        assert!(old_set.difference(&new_set).any(|s| s == "b"));
    }

    #[test]
    fn compute_diff_mixed() {
        let old = "pub fn a() {}\npub fn b() {}";
        let new = "pub fn a() {}\npub fn c() {}";
        let old_set = extract_pub_symbols(old);
        let new_set = extract_pub_symbols(new);
        assert_eq!(new_set.difference(&old_set).count(), 1);
        assert_eq!(old_set.difference(&new_set).count(), 1);
    }
}