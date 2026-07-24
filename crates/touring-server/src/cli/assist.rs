//! `assist` CLI commands — list, apply, and manage refactor assists.
//!
//! Subcommands:
//! - `touring assist list-kinds`          — list all available assist kinds
//! - `touring assist applicable <file>:<line>:<col>` — show applicable assists at a position
//! - `touring assist apply <kind> <file> <range>`    — apply an assist
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` + `parse_global_flags` to clap derive.
//! All helper functions (parse_cursor_spec, parse_range, cursor_to_range, read_file_or_stdin)
//! are preserved verbatim (G6). The `-j`/`--json` flag is a top-level field in AssistCli so
//! it does not bleed into subcommand positional args. Subcommand name "list-kinds" uses hyphen.

use std::path::Path;

use touring_assists::{ALL_HANDLERS, AssistContext, Assists};

use super::common::json_to_stdout;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring assist",
    bin_name = "touring assist",
    about = "Refactor assists: list-kinds (default), applicable, apply",
    disable_help_subcommand = true
)]
struct AssistCli {
    /// Emit JSON output.
    #[arg(short = 'j', long = "json", global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Option<AssistCmd>,
}

#[derive(Subcommand, Debug)]
enum AssistCmd {
    /// List all available assist kinds (default).
    #[command(name = "list-kinds")]
    ListKinds,
    /// Show applicable assists at a cursor position.
    Applicable {
        /// Cursor spec in the form `<file>:<line>:<col>`.
        spec: String,
    },
    /// Apply an assist to a file range.
    Apply {
        /// Assist kind ID (from list-kinds).
        kind: String,
        /// File path to apply the assist to.
        file: String,
        /// Byte range in the form start..end (e.g. 10..25).
        range: String,
    },
}

/// Entry point: `touring assist [subcommand]`
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match AssistCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    let json = cli.json;

    match cli.cmd.unwrap_or(AssistCmd::ListKinds) {
        AssistCmd::ListKinds => run_list_kinds(json),
        AssistCmd::Applicable { spec } => {
            if spec.is_empty() {
                eprintln!("usage: touring assist applicable <file>:<line>:<col>");
                return Err(anyhow::anyhow!("missing file:line:col argument"));
            }
            run_applicable(&spec, json)
        }
        AssistCmd::Apply { kind, file, range } => {
            if kind.is_empty() || file.is_empty() || range.is_empty() {
                eprintln!("usage: touring assist apply <kind> <file> <range>");
                eprintln!("  range format: start..end (e.g. 10..25)");
                return Err(anyhow::anyhow!("missing arguments"));
            }
            run_apply(&kind, &file, &range, json)
        }
    }
}

// ── `list-kinds` ──────────────────────────────────────────────────────────────

fn run_list_kinds(json: bool) -> anyhow::Result<()> {
    let kinds: Vec<_> = ALL_HANDLERS
        .iter()
        .map(|(id, _)| {
            serde_json::json!({
                "id": id,
                "group": "Refactoring",
            })
        })
        .collect();

    if json {
        json_to_stdout(&serde_json::to_string_pretty(&kinds)?);
    } else {
        println!("Available assist kinds:");
        for (id, _) in ALL_HANDLERS {
            println!("  {}", id);
        }
    }
    Ok(())
}

// ── `applicable` ───────────────────────────────────────────────────────────────

fn run_applicable(spec: &str, json: bool) -> anyhow::Result<()> {
    let (file_path, line, col) = parse_cursor_spec(spec)?;
    let content = read_file_or_stdin(&file_path)?;
    let range = cursor_to_range(&content, line, col);

    let ctx = AssistContext::new(1usize, &file_path, &content, range);
    let mut assists = Assists::new();
    for (_, handler) in ALL_HANDLERS {
        handler(&mut assists, &ctx);
    }

    let results: Vec<_> = assists
        .finish()
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "label": a.label,
                "group": a.group.0,
                "file_id": a.target.file_id,
                "range": [a.target.range.start, a.target.range.end],
            })
        })
        .collect();

    if json {
        json_to_stdout(&serde_json::to_string_pretty(&results)?);
    } else if results.is_empty() {
        println!("No applicable assists.");
    } else {
        println!("Applicable assists at {}:{}:{}", file_path, line, col);
        for r in &results {
            println!("  {}: {}", r["id"], r["label"]);
        }
    }
    Ok(())
}

// ── `apply` ────────────────────────────────────────────────────────────────────

fn run_apply(kind: &str, file_path: &str, range_spec: &str, json: bool) -> anyhow::Result<()> {
    let (start, end) = parse_range(range_spec)?;
    let content = read_file_or_stdin(file_path)?;
    let range = start..end;

    let ctx = AssistContext::new(1usize, file_path, &content, range.clone());

    // Find the handler
    let handler = ALL_HANDLERS
        .iter()
        .find(|(id, _)| *id == kind)
        .map(|(_, h)| *h)
        .ok_or_else(|| anyhow::anyhow!("unknown assist kind: {kind}"))?;

    let mut assists = Assists::new();
    handler(&mut assists, &ctx);

    let finished = assists.finish();
    if finished.is_empty() {
        if json {
            println!("{{\"applied\": false, \"reason\": \"no assist found\"}}");
        } else {
            println!(
                "No assist of kind '{}' is applicable at the given location.",
                kind
            );
        }
        return Ok(());
    }

    // Apply the first applicable assist
    let assist = &finished[0];
    let source_change = assist.source_change.evaluate();
    use touring_generator::source_change::TextEdit;
    let changes: Vec<(usize, TextEdit)> = source_change
        .edits()
        .map(|(k, v)| (*k, v.clone()))
        .collect();

    if json {
        let mut edits: Vec<serde_json::Value> = Vec::new();
        for (file_id, edit) in &changes {
            let mut indels_json: Vec<serde_json::Value> = Vec::new();
            let indels_iter = edit.indels();
            for i in indels_iter {
                indels_json.push(serde_json::json!({
                    "delete": [i.delete.start, i.delete.end],
                    "insert": i.insert,
                }));
            }
            edits.push(serde_json::json!({
                "file_id": file_id,
                "indels": indels_json,
            }));
        }
        let response = serde_json::json!({
            "applied": true,
            "kind": assist.id,
            "label": assist.label,
            "edits": edits,
        });
        json_to_stdout(&serde_json::to_string_pretty(&response)?);
    } else {
        println!("Applied '{}': {}", assist.id, assist.label);
        for (file_id, edit) in &changes {
            let indels_iter = edit.indels();
            for i in indels_iter {
                println!(
                    "  file {}: {}..{} → \"{}\"",
                    file_id, i.delete.start, i.delete.end, i.insert
                );
            }
        }
    }

    Ok(())
}

// ── Helpers (preserved verbatim from original) ─────────────────────────────────

fn parse_cursor_spec(spec: &str) -> anyhow::Result<(String, usize, usize)> {
    // Format: /path/to/file.rs:line:col  or  file.rs:line:col
    // The path may contain colons on Unix (/usr/local:custom/path), so we split
    // from the right but only if both parts after the split are numeric.
    let spec = spec.trim();

    // Try splitting at the last colon where BOTH parts are numeric (line:col)
    // If that fails, treat the whole thing as a path with no cursor info.
    let (path_part, pos_part) = if let Some(last_colon_pos) = spec.rfind(':') {
        let after_colon = &spec[last_colon_pos + 1..];
        if after_colon.parse::<usize>().is_ok() {
            // This looks like line:col, find the second-to-last colon
            let before_last = &spec[..last_colon_pos];
            if let Some(prev_colon_pos) = before_last.rfind(':') {
                let after_prev = &before_last[prev_colon_pos + 1..];
                if after_prev.parse::<usize>().is_ok() {
                    // We have path:line:col
                    let path = &spec[..prev_colon_pos];
                    let line: usize = after_prev.parse().unwrap_or(0);
                    let col: usize = after_colon.parse().unwrap_or(0);
                    return Ok((
                        path.to_string(),
                        line.saturating_sub(1),
                        col.saturating_sub(1),
                    ));
                }
            }
            // Only one colon and it's numeric → just line number, col = 0
            let line: usize = after_colon.parse().unwrap_or(0);
            return Ok((before_last.to_string(), line.saturating_sub(1), 0));
        }
        // Colon but after it isn't fully numeric - might be a path with colon
        (spec, "")
    } else {
        (spec, "")
    };

    // If no line:col found, use whole path with cursor at 0:0
    if pos_part.is_empty() {
        return Ok((path_part.to_string(), 0, 0));
    }

    let (line, col) = pos_part
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("cursor spec must be file:line:col, got: {spec}"))?;
    let line: usize = line
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid line: {line}"))?;
    let col: usize = col
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid col: {col}"))?;
    Ok((
        path_part.to_string(),
        line.saturating_sub(1),
        col.saturating_sub(1),
    ))
}

fn parse_range(spec: &str) -> anyhow::Result<(usize, usize)> {
    let spec = spec.trim();
    let Some((start_s, end_s)) = spec.split_once("..") else {
        return Err(anyhow::anyhow!("range must be start..end, got: {spec}"));
    };
    let start: usize = start_s
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid range start: {start_s}"))?;
    let end: usize = end_s
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid range end: {end_s}"))?;
    Ok((start, end))
}

fn cursor_to_range(content: &str, line: usize, col: usize) -> std::ops::Range<usize> {
    let mut offset: usize = 0;
    for (i, line_str) in content.lines().enumerate() {
        if i == line {
            let start = offset.saturating_add(col);
            let end = start.saturating_add(line_str.len().saturating_sub(col));
            return start..end;
        }
        offset = offset.saturating_add(line_str.len()).saturating_add(1);
    }
    0..content.len()
}

fn read_file_or_stdin(path: &str) -> anyhow::Result<String> {
    if path.is_empty() || path == "-" {
        // Read from stdin
        return Ok(std::io::read_to_string(std::io::stdin())?);
    }
    if !Path::new(path).exists() {
        return Err(anyhow::anyhow!("file not found: {path}"));
    }
    Ok(std::fs::read_to_string(path)?)
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    AssistCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn parse(args: &[&str]) -> AssistCli {
        AssistCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_assist_defaults_to_list_kinds() {
        let cli = parse(&["assist"]);
        assert!(cli.cmd.is_none()); // run() maps None -> ListKinds
    }

    #[test]
    fn explicit_list_kinds_parses() {
        let cli = parse(&["assist", "list-kinds"]);
        assert!(matches!(cli.cmd, Some(AssistCmd::ListKinds)));
    }

    #[test]
    fn json_flag_consumed_before_subcommand() {
        let cli = parse(&["assist", "-j", "list-kinds"]);
        assert!(cli.json);
        assert!(matches!(cli.cmd, Some(AssistCmd::ListKinds)));
    }

    #[test]
    fn applicable_parses_spec() {
        let cli = parse(&["assist", "applicable", "src/foo.rs:10:5"]);
        let Some(AssistCmd::Applicable { spec }) = cli.cmd else {
            panic!("expected Applicable");
        };
        assert_eq!(spec, "src/foo.rs:10:5");
    }

    #[test]
    fn applicable_missing_spec_is_parse_error() {
        assert!(AssistCli::try_parse_from(["assist", "applicable"]).is_err());
    }

    #[test]
    fn apply_parses_kind_file_range() {
        let cli = parse(&[
            "assist",
            "apply",
            "extract_function",
            "src/foo.rs",
            "10..25",
        ]);
        let Some(AssistCmd::Apply { kind, file, range }) = cli.cmd else {
            panic!("expected Apply");
        };
        assert_eq!(kind, "extract_function");
        assert_eq!(file, "src/foo.rs");
        assert_eq!(range, "10..25");
    }

    #[test]
    fn apply_missing_args_is_parse_error() {
        assert!(AssistCli::try_parse_from(["assist", "apply"]).is_err());
        assert!(AssistCli::try_parse_from(["assist", "apply", "kind"]).is_err());
        assert!(AssistCli::try_parse_from(["assist", "apply", "kind", "file"]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(AssistCli::try_parse_from(["assist", "frobnicate"]).is_err());
    }

    #[test]
    fn list_kinds_runs_without_error() {
        let args = s(&["touring", "assist", "list-kinds"]);
        let result = run(&args);
        assert!(result.is_ok(), "list-kinds should not error: {result:?}");
    }

    // ── Helper unit tests ──────────────────────────────────────────────────────

    #[test]
    fn parse_cursor_spec_path_line_col() {
        let (path, line, col) = parse_cursor_spec("src/foo.rs:10:5").unwrap();
        assert_eq!(path, "src/foo.rs");
        assert_eq!(line, 9); // 1-based → 0-based
        assert_eq!(col, 4);
    }

    #[test]
    fn parse_cursor_spec_path_only() {
        let (path, line, col) = parse_cursor_spec("src/foo.rs").unwrap();
        assert_eq!(path, "src/foo.rs");
        assert_eq!(line, 0);
        assert_eq!(col, 0);
    }

    #[test]
    fn parse_cursor_spec_path_and_line_only() {
        let (path, line, col) = parse_cursor_spec("src/foo.rs:3").unwrap();
        assert_eq!(path, "src/foo.rs");
        assert_eq!(line, 2); // 1-based → 0-based
        assert_eq!(col, 0);
    }

    #[test]
    fn parse_range_valid() {
        let (start, end) = parse_range("10..25").unwrap();
        assert_eq!(start, 10);
        assert_eq!(end, 25);
    }

    #[test]
    fn parse_range_invalid_no_dotdot() {
        assert!(parse_range("10-25").is_err());
    }

    #[test]
    fn parse_range_invalid_non_numeric() {
        assert!(parse_range("abc..25").is_err());
    }

    #[test]
    fn cursor_to_range_first_line() {
        let content = "hello world\nsecond line\n";
        let r = cursor_to_range(content, 0, 0);
        assert_eq!(r, 0..11);
    }

    #[test]
    fn cursor_to_range_second_line() {
        let content = "hello world\nsecond line\n";
        let r = cursor_to_range(content, 1, 0);
        assert_eq!(r, 12..23);
    }

    #[test]
    fn read_file_or_stdin_missing_file_errors() {
        let result = read_file_or_stdin("/nonexistent/path/file.rs");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file not found"));
    }
}
