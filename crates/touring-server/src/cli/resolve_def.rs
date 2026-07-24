//! `touring resolve-def <file>:<line>:<col> [-j]` — Resolve a position to its definition.
//!
//! ## Output format (JSON)
//!
//! ```json
//! {
//!   "kind": "Function",
//!   "name": "foo",
//!   "source_range": { "start": [10, 1], "end": [15, 2] },
//!   "source_file": "foo.rs",
//!   "definition_id": { "file_id": 0, "symbol_index": 42 }
//! }
//! ```

use super::daemon_query;

/// Entry point for the `touring resolve-def` CLI handler — parses the
/// `<file>:<line>:<col>` position, reads the file source, dispatches the
/// `cli-resolve-def` daemon query, and prints either the raw JSON
/// (`-j`/`--json`) or a human-readable `<kind> <name> (definition at <range>)`
/// summary of the resolved symbol definition.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let is_json = args.iter().any(|s| s == "-j" || s == "--json");

    // Handle --help
    if args.iter().any(|s| s == "--help" || s == "-h") {
        print_help(is_json);
        return Ok(());
    }

    let pos_arg = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("Usage: touring resolve-def <file>:<line>:<col> [-j]"))?;

    let (file, line, col) = parse_position(pos_arg)?;
    let source = std::fs::read_to_string(&file)
        .map_err(|e| anyhow::anyhow!("Cannot read file '{}': {}", file, e))?;

    let payload = serde_json::json!({
        "file": file,
        "line": line,
        "column": col,
        "source": source
    });

    let output = daemon_query("cli-resolve-def", payload)?;

    if is_json {
        println!("{output}");
    } else {
        // Human-readable format
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output) {
            let kind = parsed.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
            let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let rng = parsed.get("source_range");
            println!(
                "{} {} (definition at {})",
                kind,
                name,
                rng.map(|r| r.to_string()).unwrap_or_default()
            );
        } else {
            println!("{output}");
        }
    }
    Ok(())
}

/// Parse "file.rs:10:5" into (path, line, col)
pub fn parse_position(s: &str) -> anyhow::Result<(String, usize, usize)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Position must be <file>:<line>:<col>, got: {}", s);
    }
    if parts[0].is_empty() || parts[1].is_empty() || parts[2].is_empty() {
        anyhow::bail!("Position must be <file>:<line>:<col>, got: {}", s);
    }
    let file = parts[0].to_string();
    let line = parts[1]
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("Invalid line number: {}", parts[1]))?;
    let col = parts[2]
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("Invalid column: {}", parts[2]))?;
    Ok((file, line, col))
}

/// Print help text
fn print_help(_is_json: bool) {
    println!("touring resolve-def <file>:<line>:<col> [-j]");
    println!("  Resolve a file:line:col position to its symbol definition");
    println!("  -j, --json   Output JSON");
}
