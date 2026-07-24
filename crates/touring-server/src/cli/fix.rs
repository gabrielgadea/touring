//! `touring fix <code> <file>` — Apply a named assist to fix a diagnostic.
//!
//! D.3 (RFC-100): Given a diagnostic code + file, looks up the applicable
//! assist and applies it via the assist framework.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`. No daemon_query — uses
//! touring_assists directly.

use std::path::Path;

use clap::Parser;
use touring_assists::{ALL_HANDLERS, AssistContext, Assists};

use super::common::json_to_stdout;

#[derive(Parser, Debug)]
#[command(
    name = "touring fix",
    bin_name = "touring fix",
    about = "Apply a named assist to fix an RFC-100 diagnostic code in a file",
    disable_help_subcommand = true
)]
struct FixCli {
    /// RFC-100 diagnostic code (e.g. Q-201, W-100, Q-220, M-500, B-301).
    code: String,
    /// Source file to apply the fix to.
    file: String,
    /// Output fix details as JSON.
    #[arg(short = 'j', long = "json")]
    json: bool,
}
/// D.3.3 CLI — apply a fix assist for a diagnostic code.
#[allow(clippy::needless_borrow, clippy::type_complexity)]
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match FixCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    let file_path = cli.file.as_str();
    let code = cli.code.as_str();
    if !Path::new(file_path).exists() {
        return Err(anyhow::anyhow!("file not found: {file_path}"));
    }
    let assist_id: &str = match code {
        "Q-201" | "W-100" | "W-101" | "W-102" | "W-103" => "auto_wire",
        "Q-220" => "format_rust_preserve",
        "M-500" | "M-510" | "M-520" | "M-530" => "auto_import",
        "B-301" => "extract_function",
        other => {
            eprintln!("no fix known for code: {other}");
            return Err(anyhow::anyhow!("unknown diagnostic code: {other}"));
        }
    };
    let content = std::fs::read_to_string(file_path)?;
    let range = 0..content.len();
    let ctx = AssistContext::new(1usize, file_path, &content, range.clone());
    let mut assists = Assists::new();
    let handler = ALL_HANDLERS
        .iter()
        .find(|(id, _)| *id == assist_id)
        .map(|(_, h)| *h)
        .ok_or_else(|| anyhow::anyhow!("assist not registered: {assist_id}"))?;
    handler(&mut assists, &ctx);
    let finished = assists.finish();
    if finished.is_empty() {
        if cli.json {
            println!("{{\"applied\": false, \"reason\": \"no assist applicable\"}}");
        } else {
            println!("No assist of kind '{assist_id}' is applicable in {file_path}");
        }
        return Ok(());
    }
    let assist = &finished[0];
    let source_change = assist.source_change.evaluate();
    use touring_generator::source_change::TextEdit;
    let changes: Vec<(usize, TextEdit)> = source_change
        .edits()
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    if cli.json {
        let mut edits: Vec<serde_json::Value> = Vec::new();
        for (file_id, edit) in &changes {
            let indels_json: Vec<_> = edit
                .indels()
                .map(|i| {
                    serde_json::json!(
                        { "delete" : [i.delete.start, i.delete.end], "insert" : i.insert,
                        }
                    )
                })
                .collect();
            edits.push(serde_json::json!({ "file_id" : file_id, "indels" : indels_json, }));
        }
        let response = serde_json::json!(
            { "applied" : true, "kind" : assist.id, "label" : assist.label, "edits" :
            edits, }
        );
        json_to_stdout(&serde_json::to_string_pretty(&response)?);
    } else {
        println!("Applied '{}': {}", assist.id, assist.label);
        for (file_id, edit) in &changes {
            for i in edit.indels() {
                println!(
                    "  file {}: {}..{} → \"{}\"",
                    file_id, i.delete.start, i.delete.end, i.insert
                );
            }
        }
    }
    Ok(())
}
/// Maps an RFC-100 diagnostic code to the corresponding assist ID.
/// Returns None if no fix is available for the code.
pub fn assist_id_for_code(code: &str) -> Option<&'static str> {
    match code {
        "Q-201" | "W-100" | "W-101" | "W-102" | "W-103" => Some("auto_wire"),
        "Q-220" => Some("format_rust_preserve"),
        "M-500" | "M-510" | "M-520" | "M-530" => Some("auto_import"),
        "B-301" => Some("extract_function"),
        _ => None,
    }
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    FixCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assist_id_for_code_auto_wire() {
        for code in &["Q-201", "W-100", "W-101", "W-102", "W-103"] {
            assert_eq!(assist_id_for_code(code), Some("auto_wire"), "code {code}");
        }
    }
    #[test]
    fn assist_id_for_code_format_rust_preserve() {
        assert_eq!(assist_id_for_code("Q-220"), Some("format_rust_preserve"));
    }
    #[test]
    fn assist_id_for_code_auto_import() {
        for code in &["M-500", "M-510", "M-520", "M-530"] {
            assert_eq!(assist_id_for_code(code), Some("auto_import"), "code {code}");
        }
    }
    #[test]
    fn assist_id_for_code_extract_function() {
        assert_eq!(assist_id_for_code("B-301"), Some("extract_function"));
    }
    #[test]
    fn assist_id_for_code_unknown() {
        assert_eq!(assist_id_for_code("X-999"), None);
        assert_eq!(assist_id_for_code("Q-000"), None);
    }
    #[test]
    fn assist_id_all_rfc100_codes_covered() {
        let codes_with_fixes = vec![
            "Q-201", "Q-220", "W-100", "W-101", "W-102", "W-103", "M-500", "M-510", "M-520",
            "M-530", "B-301",
        ];
        for code in codes_with_fixes {
            let result = assist_id_for_code(code);
            assert!(
                result.is_some(),
                "RFC-100 code {code} should have a known assist_id, got None",
            );
        }
        assert_eq!(assist_id_for_code("B-300"), None);
    }
    #[test]
    fn cli_parses_code_and_file() {
        fn s(parts: &[&str]) -> Vec<String> {
            parts.iter().map(|p| p.to_string()).collect()
        }
        let cli = FixCli::try_parse_from(s(&["fix", "Q-201", "src/lib.rs"]).iter()).unwrap();
        assert_eq!(cli.code, "Q-201");
        assert_eq!(cli.file, "src/lib.rs");
        assert!(!cli.json);
    }
    #[test]
    fn cli_json_flag() {
        fn s(parts: &[&str]) -> Vec<String> {
            parts.iter().map(|p| p.to_string()).collect()
        }
        let cli = FixCli::try_parse_from(s(&["fix", "Q-201", "src/lib.rs", "-j"]).iter()).unwrap();
        assert!(cli.json);
    }
    #[test]
    fn cli_missing_args_is_rejected() {
        fn s(parts: &[&str]) -> Vec<String> {
            parts.iter().map(|p| p.to_string()).collect()
        }
        assert!(FixCli::try_parse_from(s(&["fix"]).iter()).is_err());
        assert!(FixCli::try_parse_from(s(&["fix", "Q-201"]).iter()).is_err());
    }
}
