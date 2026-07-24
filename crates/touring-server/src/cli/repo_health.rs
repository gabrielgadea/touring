//! `touring repo-health` — Wave R3 auto-generated executive Markdown report.
//!
//! Combines `repo-score` (Wave R1) + `kpi` (Wave R2) into a single human
//! readable Markdown document. Default output:
//! `~/.claude/rust/docs/repo-health.md`.
//!
//! # Examples
//!
//! ```bash
//! touring repo-health                      # write default path
//! touring repo-health --output /tmp/h.md   # custom path
//! touring repo-health --stdout             # print Markdown to stdout
//! ```

use super::daemon_query;

/// Run the `repo-health` CLI subcommand.
///
/// Flags:
/// - `--output <path>`: write Markdown to a custom file
/// - `--stdout`: print Markdown to stdout instead of writing a file
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable or the daemon
/// reports a failure response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let output_path = super::common::flag_value(args, "--output").unwrap_or("");
    let stdout_only = args.iter().any(|a| a == "--stdout");

    let mut payload = serde_json::Map::new();
    if !output_path.is_empty() {
        payload.insert(
            "output_path".to_string(),
            serde_json::Value::String(output_path.to_string()),
        );
    }
    if stdout_only {
        payload.insert("stdout".to_string(), serde_json::Value::Bool(true));
    }

    let output = daemon_query("cli-repo-health", serde_json::Value::Object(payload))?;

    if stdout_only {
        // Daemon returned `{"markdown": "...", "bytes": N}` — print just the markdown.
        let parsed: serde_json::Value =
            serde_json::from_str(&output).unwrap_or(serde_json::Value::Null);
        if let Some(md) = parsed.get("markdown").and_then(serde_json::Value::as_str) {
            println!("{md}");
            return Ok(());
        }
    }

    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn no_args_routes_to_daemon() {
        let args = s(&["touring", "repo-health"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                !msg.contains("Usage"),
                "no-args should not emit usage error: {msg}"
            );
        }
    }

    #[test]
    fn output_flag_extracted() {
        let args = s(&["touring", "repo-health", "--output", "/tmp/health.md"]);
        let extracted = super::super::common::flag_value(&args, "--output").unwrap_or("");
        assert_eq!(extracted, "/tmp/health.md");
    }

    #[test]
    fn stdout_flag_recognized() {
        let args = s(&["touring", "repo-health", "--stdout"]);
        let has_stdout = args.iter().any(|a| a == "--stdout");
        assert!(has_stdout);
    }
}
