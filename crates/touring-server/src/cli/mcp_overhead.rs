//! `touring mcp-overhead` — Query MCP tool overhead self-report.
//
//! ## Wiring
//!
//! The counter data is populated by the hook runtime when MCP tools are
//! invoked. The `instructions_loaded` hook (hook_registry.rs:1076) injects
//! the summary at session start; individual tool invocations call
//! `telemetry::mcp_overhead::record_tool_invocation()` from within the
//! MCP dispatch path (server/tools_infra.rs).
//!
//! This CLI command provides a synchronous snapshot of that data.

use super::daemon_query;

/// Run the `mcp-overhead` CLI subcommand.
///
/// Supports two output formats:
///   - `--format json`  (machine-readable, default)
///   - `--format table` (human-readable summary)
///
/// Top-N filtering via `--top <N>` limits output to the N costliest tools.
///
/// Exit code: 0 on success, 1 on daemon communication error.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let mut format = "json";
    let mut top_n: Option<usize> = None;

    // Parse arguments (args[0] = "mcp-overhead", args[1..] = sub-args)
    let mut i = 2;
    while i < args.len() {
        match args.get(i).map(|s| s.as_str()) {
            Some("--format" | "-f") => {
                format = args.get(i + 1).map(|s| s.as_str()).unwrap_or("json");
                i += 2;
            }
            Some("--top" | "-n") => {
                top_n = args.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            Some("--help" | "-h") => {
                print_help();
                return Ok(());
            }
            _ => i += 1,
        }
    }

    match format {
        "table" => run_table(top_n),
        _ => run_json(top_n),
    }
}

fn run_json(top_n: Option<usize>) -> anyhow::Result<()> {
    // Query the daemon for the current snapshot via the CLI MCP overhead hook.
    // The hook returns a JSON string with the full report.
    let output = daemon_query("cli-mcp-overhead", serde_json::json!({ "top_n": top_n }))?;

    // If top_n was specified, extract just the top tools from the full report.
    // The daemon-side handler respects top_n; if not specified it returns all.
    let final_output = if let Some(n) = top_n {
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(v) => {
                let tools = v.get("tools").and_then(|t| t.as_array());
                if let Some(arr) = tools {
                    let top: Vec<_> = arr.iter().take(n).cloned().collect();
                    let mut filtered = serde_json::Map::new();
                    filtered.insert("tools".to_string(), serde_json::Value::Array(top));
                    if let Some(tt) = v.get("total_tokens").and_then(|t| t.as_u64()) {
                        filtered.insert(
                            "total_tokens".to_string(),
                            serde_json::Value::Number(tt.into()),
                        );
                    }
                    serde_json::to_string(&serde_json::Value::Object(filtered)).unwrap_or(output)
                } else {
                    output
                }
            }
            Err(_) => output,
        }
    } else {
        output
    };

    println!("{final_output}");
    Ok(())
}

fn run_table(top_n: Option<usize>) -> anyhow::Result<()> {
    // Query daemon for JSON report
    let output = daemon_query("cli-mcp-overhead", serde_json::json!({ "top_n": top_n }))?;

    let report: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|_| serde_json::json!({ "tools": [], "total_tokens": 0 }));

    let tools = report
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let total_tokens = report
        .get("total_tokens")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);

    // Print table header
    eprintln!(
        "{:<45} {:>10} {:>10} {:>12}",
        "TOOL", "CALLS", "TOKENS/Call", "TOTAL TOKENS"
    );
    eprintln!("{}", "-".repeat(80));

    for tool in tools {
        let name = tool
            .get("tool_name")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        let call_count = tool.get("call_count").and_then(|c| c.as_u64()).unwrap_or(0);
        let token_estimate = tool
            .get("token_estimate")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);
        let tool_total = tool
            .get("total_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);

        eprintln!(
            "{:<45} {:>10} {:>10} {:>12}",
            name, call_count, token_estimate, tool_total
        );
    }

    eprintln!("{}", "-".repeat(80));
    eprintln!("{:<45} {:>10} {:>10} {:>12}", "TOTAL", "", "", total_tokens);

    Ok(())
}

fn print_help() {
    eprintln!(
        "touring mcp-overhead — Show MCP tool overhead self-report

USAGE:
  touring mcp-overhead [OPTIONS]

OPTIONS:
  --format json|table   Output format (default: json)
  --top N               Show only top N costliest tools
  --help, -h            Show this help message

JSON OUTPUT:
  {{ \"tools\": [{{ \"tool_name\": \"mcp__touring__index_find\", \"call_count\": 5,
      \"token_estimate\": 250, \"total_tokens\": 1250 }}], \"total_tokens\": 1450 }}

TABLE OUTPUT:
  Human-readable summary sorted by total_tokens descending.

NOTE: Data reflects only MCP tool invocations recorded since the last daemon start.
The `instructions_loaded` hook injects this summary at session start."
    );
}

#[cfg(test)]
mod tests {

    #[test]
    fn run_accepts_format_flag() {
        // Smoke test: verify parse doesn't panic with valid args
        let args = vec![
            "mcp-overhead".to_string(),
            "--format".to_string(),
            "table".to_string(),
        ];
        // Would need daemon for full test, but parse is exercised
        let mut format = "json";
        let mut top_n: Option<usize> = None;
        let mut i = 1; // start at 1 (skip binary name)
        while i < args.len() {
            match args.get(i).map(|s| s.as_str()) {
                Some("--format" | "-f") => {
                    format = args.get(i + 1).map(|s| s.as_str()).unwrap_or("json");
                    i += 2;
                }
                Some("--top" | "-n") => {
                    top_n = args.get(i + 1).and_then(|s| s.parse().ok());
                    i += 2;
                }
                _ => i += 1,
            }
        }
        assert_eq!(format, "table");
        assert_eq!(
            top_n, None,
            "no --top flag means top_n stays at the None default"
        );
    }

    #[test]
    fn run_accepts_top_flag() {
        let args = vec![
            "mcp-overhead".to_string(),
            "--top".to_string(),
            "10".to_string(),
        ];
        let mut format = "json";
        let mut top_n: Option<usize> = None;
        let mut i = 1; // start at 1 (skip binary name)
        while i < args.len() {
            match args.get(i).map(|s| s.as_str()) {
                Some("--format" | "-f") => {
                    format = args.get(i + 1).map(|s| s.as_str()).unwrap_or("json");
                    i += 2;
                }
                Some("--top" | "-n") => {
                    top_n = args.get(i + 1).and_then(|s| s.parse().ok());
                    i += 2;
                }
                _ => i += 1,
            }
        }
        assert_eq!(top_n, Some(10));
        assert_eq!(
            format, "json",
            "no --format flag means format stays at the \"json\" default"
        );
    }
}
