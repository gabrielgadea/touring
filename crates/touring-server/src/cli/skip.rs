//! CLI `skip` subcommand — list and validate skip regions in source files.
//!
//! Provides machine-readable JSON output for integration with editors,
//! linters, and the touring daemon's hook system.
//!
//! ```text
//! touring skip list <file>     — parse and list all skip regions
//! touring skip validate <file> — check if file can be parsed for skip regions
//! ```

use serde::Serialize;
use std::path::Path;
use std::{fs, io};

const USAGE: &str = "\
Usage:
  touring skip list <file>     — parse and list all skip regions as JSON
  touring skip validate <file> — check if a file can be parsed for skip regions
  touring skip -h|--help       — show this help and exit 0
";

/// Span in byte offsets within a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ByteSpan {
    /// Inclusive start byte offset of the span within the source file.
    pub start: u64,
    /// Exclusive end byte offset of the span within the source file.
    pub end: u64,
}

/// Syntactic form of a skip marker found in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipStyle {
    /// A multi-line region delimited by `// touring:skip-region` … `// touring:skip-end`.
    LineComment,
    /// A single-line `/* touring:skip-region */` block comment.
    BlockComment,
    /// A `#[touring::skip]` (or `#[touring(skip)]`) Rust attribute.
    RustAttribute,
}

/// A region of `file_path` marked to be skipped, with its byte `span` and marker `style`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkipRegion {
    /// Path of the file the region belongs to (filled in by the caller).
    pub file_path: String,
    /// Byte-offset span the region covers in the source.
    pub span: ByteSpan,
    /// The marker syntax that produced this region.
    pub style: SkipStyle,
}

/// Parse `// touring:skip-region` … `// touring:skip-end` line-comment markers
/// and `#[touring::skip]` Rust attributes from `source`.
///
/// This is a self-contained parser that duplicates the logic in
/// `touring-hooks::post_edit::parse_skip_regions` to avoid a dependency
/// on `touring-hooks` from `touring-server`.
fn parse_skip_regions(source: &str) -> Vec<SkipRegion> {
    let mut regions = Vec::new();
    let mut in_region = false;
    let mut region_start: Option<u64> = None;
    let mut line_cursor: u64 = 0;

    for line in source.lines() {
        let line_start = line_cursor;
        let line_end = line_cursor + line.len() as u64 + 1; // +1 for newline
        let trimmed = line.trim();

        // Rust attribute: #[touring::skip] or #[touring(skip)]
        if trimmed.starts_with('#')
            && (trimmed.contains("touring::skip") || trimmed.contains("touring(skip)"))
        {
            regions.push(SkipRegion {
                file_path: String::new(), // filled by caller
                span: ByteSpan {
                    start: line_start,
                    end: line_end,
                },
                style: SkipStyle::RustAttribute,
            });
            line_cursor = line_end;
            continue;
        }

        // Line comment: // touring:skip-region → start multi-line region
        if trimmed.starts_with("//")
            && trimmed.contains("touring:skip-region")
            && !trimmed.contains("touring:skip-end")
        {
            region_start = Some(line_end); // region starts AFTER this line
            in_region = true;
        } else if trimmed.starts_with("//") && trimmed.contains("touring:skip-end") && in_region {
            if let Some(start) = region_start.take() {
                regions.push(SkipRegion {
                    file_path: String::new(), // filled by caller
                    span: ByteSpan {
                        start,
                        end: line_start, // end BEFORE this end-marker line
                    },
                    style: SkipStyle::LineComment,
                });
            }
            in_region = false;
        }

        // Block comment: /* touring:skip-region */ anywhere on the line
        if let Some(cmt_start) = line.find("/*") {
            let cmt_trimmed = line[cmt_start..].trim_start();
            if cmt_trimmed.starts_with("/*")
                && cmt_trimmed.contains("touring:skip-region")
                && let Some(cmt_end) = cmt_trimmed[2..].find("*/")
            {
                let comment_start = line_cursor + cmt_start as u64;
                let comment_end = comment_start + cmt_end as u64 + 2;
                regions.push(SkipRegion {
                    file_path: String::new(), // filled by caller
                    span: ByteSpan {
                        start: comment_start,
                        end: comment_end,
                    },
                    style: SkipStyle::BlockComment,
                });
            }
        }

        line_cursor = line_end;
    }

    regions
}

/// Run the `skip` subcommand dispatcher.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // Runtime argv is `[binary, "skip", <sub>, ...]`, so the sub-subcommand lives
    // at index 2 (matching the `filters`/`ssr` handlers). Reading index 0 picked
    // up the binary path and broke every invocation — including `-h`/`--help`,
    // which fell through to the "unknown subcommand" arm and exited 1. (A2)
    match args.get(2).map(|s| s.as_str()) {
        Some("-h") | Some("--help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some("list") => skip_list(&args[3..]),
        Some("validate") => skip_validate(&args[3..]),
        None => {
            eprint!("{USAGE}");
            std::process::exit(1);
        }
        Some(cmd) => {
            eprintln!("unknown skip subcommand: {cmd}");
            eprint!("{USAGE}");
            std::process::exit(1);
        }
    }
}

/// `touring skip list <file>` — parse and list all skip regions as JSON.
fn skip_list(args: &[String]) -> anyhow::Result<()> {
    let file_path = args.first().map(Path::new).unwrap_or_else(|| {
        eprintln!("error: missing argument: <file>");
        std::process::exit(1);
    });

    let source = fs::read_to_string(file_path)
        .map_err(|e| anyhow::anyhow!("read error {}: {}", file_path.display(), e))?;

    let fp_str = file_path.to_string_lossy().to_string();
    let mut regions = parse_skip_regions(&source);
    for r in &mut regions {
        r.file_path = fp_str.clone();
    }

    let json = serde_json::json!({
        "file": fp_str,
        "region_count": regions.len(),
        "regions": regions,
    });

    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────

// A production fn is defined after this module — a pre-existing layout. Allow the
// lint here rather than reorder unrelated code in an R4-scoped change.
#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;

    fn sv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    // ── help flag (A2: -h/--help must exit 0 via Ok) ─────────────────────

    #[test]
    fn help_short_returns_ok() {
        // Real runtime layout: [binary, "skip", <flag>].
        assert!(run(&sv(&["touring", "skip", "-h"])).is_ok());
    }

    #[test]
    fn help_long_returns_ok() {
        assert!(run(&sv(&["touring", "skip", "--help"])).is_ok());
    }

    // ── core parser logic ─────────────────────────────────────────────────

    #[test]
    fn parse_regions_empty_source() {
        let regions = parse_skip_regions("");
        assert!(regions.is_empty());
    }

    #[test]
    fn parse_regions_rust_attribute() {
        let src = "#[touring::skip]\nfn foo() {}\n";
        let regions = parse_skip_regions(src);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].style, SkipStyle::RustAttribute);
    }

    #[test]
    fn parse_regions_paren_attribute() {
        let src = "#[touring(skip)]\nfn bar() {}\n";
        let regions = parse_skip_regions(src);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].style, SkipStyle::RustAttribute);
    }

    #[test]
    fn parse_regions_line_comment_region() {
        let src = "// touring:skip-region\nsome code\n// touring:skip-end\n";
        let regions = parse_skip_regions(src);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].style, SkipStyle::LineComment);
    }

    #[test]
    fn parse_regions_block_comment() {
        let src = "code /* touring:skip-region */ more\n";
        let regions = parse_skip_regions(src);
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].style, SkipStyle::BlockComment);
    }

    #[test]
    fn parse_regions_multiple() {
        let src = "#[touring::skip]\nfn a() {}\n#[touring(skip)]\nfn b() {}\n";
        let regions = parse_skip_regions(src);
        assert_eq!(regions.len(), 2);
    }

    #[test]
    fn parse_regions_unclosed_region_produces_no_entry() {
        let src = "// touring:skip-region\nsome code\n";
        let regions = parse_skip_regions(src);
        // Unclosed region: no end marker, so no LineComment region
        assert!(regions.is_empty());
    }
}

/// `touring skip validate <file>` — check if file is a valid Rust source that can
/// be parsed for skip regions.
fn skip_validate(args: &[String]) -> anyhow::Result<()> {
    let file_path = args.first().map(Path::new).unwrap_or_else(|| {
        eprintln!("error: missing argument: <file>");
        std::process::exit(1);
    });

    let source = match fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let json = serde_json::json!({
                "file": file_path.to_string_lossy(),
                "valid": false,
                "error": format!("file not found: {}", file_path.display()),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
            return Ok(());
        }
        Err(e) => {
            let json = serde_json::json!({
                "file": file_path.to_string_lossy(),
                "valid": false,
                "error": format!("read error: {}", e),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
            return Ok(());
        }
    };

    // Basic heuristics: must look like Rust source
    let is_rust = file_path.extension().map(|e| e == "rs").unwrap_or(false)
        || source.lines().take(10).any(|l| {
            l.trim().starts_with("use ")
                || l.trim().starts_with("fn ")
                || l.trim().starts_with("struct ")
                || l.trim().starts_with("enum ")
                || l.trim().starts_with("mod ")
                || l.trim().starts_with("impl ")
                || l.trim().starts_with("pub ")
                || l.trim().starts_with("//!")
                || l.trim().starts_with("/*")
        });

    let regions = parse_skip_regions(&source);

    let json = serde_json::json!({
        "file": file_path.to_string_lossy(),
        "valid": is_rust,
        "is_rust_file": file_path.extension().map(|e| e == "rs").unwrap_or(false),
        "has_skip_regions": regions.len() > 0,
        "region_count": regions.len(),
    });

    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}
