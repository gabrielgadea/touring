//! `touring quality-signal` — Workspace-level Sentrux quality signal.
//!
//! Walks `$PWD` (or `--root <path>`) for `.rs` files, builds a
//! [`Workspace`](touring_analysis::quality::Workspace) handle and
//! computes a [`WorkspaceQualitySignal`](touring_analysis::quality::WorkspaceQualitySignal).
//!
//! The score lives on the Sentrux 0..=10000 scale, derived as the
//! geometric mean of five normalized root-cause scores (modularity,
//! acyclicity, depth, equality, redundancy). The bottleneck field
//! identifies which root cause is dragging the score down — designed
//! to drive the next refactor decision via gradient descent.
//!
//! # Usage
//!
//! ```text
//! touring quality-signal                # human-readable summary on $PWD
//! touring quality-signal -j             # JSON for tooling / status pipelines
//! touring quality-signal --root <path>  # target a specific source tree
//! ```
//!
//! # Exit codes
//!
//! Always `0` — diagnostic output goes to stdout (JSON) or stderr
//! (human). Failure to walk the root is reported as a JSON error
//! object with `signal_0_10000 = 0` so callers can still aggregate.

use std::path::PathBuf;

use super::common::{flag_value, has_flag, human_to_stderr, json_to_stdout, parse_global_flags};

/// Run the `quality-signal` subcommand.
///
/// # Errors
///
/// Returns an `anyhow::Error` if JSON serialisation of the result fails.
/// Walk-time errors are surfaced in the JSON payload (and human output)
/// rather than as anyhow errors so the command stays exit-0 in the
/// `status -j` pipeline.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (flags, filtered) = parse_global_flags(args);

    let root: PathBuf = flag_value(&filtered, "--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let include_diagnostics = !has_flag(&filtered, "--no-diagnostics");

    let result = touring_analysis::quality::build_workspace_from_path(&root);

    let json_value = match result {
        Ok(ws) => {
            let signal = touring_analysis::quality::compute_quality_signal(&ws);
            let mut value = serde_json::to_value(&signal).unwrap_or_else(|_| serde_json::json!({}));
            if !include_diagnostics {
                if let Some(obj) = value.as_object_mut() {
                    obj.remove("diagnostics");
                }
            }
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "root".to_string(),
                    serde_json::json!(root.to_string_lossy().into_owned()),
                );
            }
            value
        }
        Err(err) => serde_json::json!({
            "root": root.to_string_lossy().into_owned(),
            "signal_0_10000": 0,
            "signal_normalized": 0.0,
            "bottleneck": "Tied",
            "error": err.to_string(),
        }),
    };

    if flags.json {
        let pretty = serde_json::to_string_pretty(&json_value)?;
        json_to_stdout(&pretty);
    } else {
        let summary = format_summary(&json_value);
        human_to_stderr(&summary);
    }

    Ok(())
}

fn format_summary(value: &serde_json::Value) -> String {
    let signal = json_u64(value, "signal_0_10000");
    let normalized = json_f64(value, "signal_normalized");
    let bottleneck = json_str(value, "bottleneck", "Tied");
    let root = json_str(value, "root", "(unknown)");

    let mut lines = vec![
        format!("touring quality-signal — {root}"),
        format!("  signal: {signal} / 10000   ({normalized:.2} / 1.00)"),
        format!("  bottleneck: {bottleneck}"),
    ];

    append_root_cause_lines(&mut lines, value);
    append_raw_summary_line(&mut lines, value);
    append_error_line(&mut lines, value);

    lines.join("\n")
}

fn append_root_cause_lines(lines: &mut Vec<String>, value: &serde_json::Value) {
    let Some(scores) = value.get("root_causes").and_then(|v| v.as_object()) else {
        return;
    };
    lines.push(String::from("  root causes:"));
    for key in [
        "modularity",
        "acyclicity",
        "depth",
        "equality",
        "redundancy",
    ] {
        let score = scores.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
        lines.push(format!("    {key:11} {score:.3}"));
    }
}

fn append_raw_summary_line(lines: &mut Vec<String>, value: &serde_json::Value) {
    let Some(raw) = value.get("raw").and_then(|v| v.as_object()) else {
        return;
    };
    let nodes = raw.get("total_nodes").and_then(|v| v.as_u64()).unwrap_or(0);
    let edges = raw.get("total_edges").and_then(|v| v.as_u64()).unwrap_or(0);
    let fns = raw
        .get("total_functions")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    lines.push(format!(
        "  workspace: {fns} functions / {edges} edges / {nodes} nodes"
    ));
}

fn append_error_line(lines: &mut Vec<String>, value: &serde_json::Value) {
    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
        lines.push(format!("  error: {error}"));
    }
}

fn json_u64(value: &serde_json::Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn json_f64(value: &serde_json::Value, key: &str) -> f64 {
    value.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str, default: &'a str) -> &'a str {
    value.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn defaults_to_current_dir_human() {
        let args = s(&["touring", "quality-signal"]);
        let result = run(&args);
        assert!(result.is_ok(), "run should not error: {result:?}");
    }

    #[test]
    fn json_flag_emits_json() {
        let args = s(&["touring", "quality-signal", "-j"]);
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn nonexistent_root_yields_error_payload() {
        let args = s(&[
            "touring",
            "quality-signal",
            "--root",
            "/nonexistent/path/that/does/not/exist",
            "-j",
        ]);
        let result = run(&args);
        assert!(result.is_ok(), "should still exit 0 on unreachable root");
    }

    #[test]
    fn format_summary_emits_score_line() {
        let payload = serde_json::json!({
            "signal_0_10000": 7122_u64,
            "signal_normalized": 0.7122,
            "bottleneck": "Modularity",
            "root": "/tmp",
            "root_causes": {
                "modularity": 0.0,
                "acyclicity": 1.0,
                "depth": 1.0,
                "equality": 0.99,
                "redundancy": 1.0,
            },
            "raw": {
                "total_nodes": 10,
                "total_edges": 15,
                "total_functions": 50,
            },
        });
        let s = format_summary(&payload);
        assert!(s.contains("7122 / 10000"), "missing score: {s}");
        assert!(s.contains("Modularity"), "missing bottleneck: {s}");
        assert!(s.contains("modularity"), "missing root cause label: {s}");
    }
}
