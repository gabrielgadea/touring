//! `touring rename <file>:<line>:<col> <new-name> [--apply] [-j]`
//!
//! Renames a symbol across all references. Uses SourceChange (B.5) for atomic multi-file edits.

use super::common::flag_value;
use super::daemon_query;

/// Entry point for the `touring rename` CLI handler — parses the
/// `<file>:<line>:<col>` position and the new name, dispatches the `cli-rename`
/// daemon query (dry-run unless `--apply` is given), and prints either the raw
/// JSON (`-j`/`--json`) or a human-readable `status: renamed to '<name>'
/// (<n> sites)` summary.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        print_help();
        return Ok(());
    }

    let apply = flag_value(args, "--apply").is_some();
    let is_json = args.iter().any(|s| s == "-j" || s == "--json");

    let pos_arg = args.get(2).ok_or_else(|| {
        anyhow::anyhow!("Usage: touring rename <file>:<line>:<col> <new-name> [--apply]")
    })?;
    let new_name = args.get(3).ok_or_else(|| {
        anyhow::anyhow!("Usage: touring rename <file>:<line>:<col> <new-name> [--apply]")
    })?;

    let (file, line, col) = parse_position(pos_arg)?;

    let payload = serde_json::json!({
        "file": file,
        "line": line,
        "column": col,
        "new_name": new_name,
        "apply": apply
    });

    let output = daemon_query("cli-rename", payload)?;

    if is_json {
        println!("{output}");
    } else if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output) {
        let status = parsed.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let sites = parsed
            .get("sites")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0);
        println!("{}: renamed to '{}' ({} sites)", status, new_name, sites);
    } else {
        println!("{output}");
    }
    Ok(())
}

fn print_help() {
    println!("touring rename <file>:<line>:<col> <new-name> [--apply]");
    println!("  Rename a symbol across all references");
    println!("  --apply   Apply changes (default: dry-run)");
    println!("  -j        JSON output");
}

fn parse_position(s: &str) -> anyhow::Result<(String, usize, usize)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Position must be <file>:<line>:<col>, got: {}", s);
    }
    let file = parts[0].to_string();
    let line = parts[1]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid line"))?;
    let col = parts[2]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid column"))?;
    Ok((file, line, col))
}
