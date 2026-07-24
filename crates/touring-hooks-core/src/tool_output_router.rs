//! D2.1 — ToolOutputRouter: classifies tools for sandbox routing.
//!
//! Part of D2 PreToolUse Output Router (P0, XL).
//! Decision: PassThrough vs RouteToSandbox based on estimated output size.
//!
//! Reuses: OutputCapture::CAPTURE_THRESHOLD (output size concept).

use serde_json::Value;

/// Routing decision for a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingDecision {
    /// Execute normally — output stays in-band.
    PassThrough,
    /// Execute in sandbox subprocess; return content_hash to LLM.
    RouteToSandbox,
}

/// Classifies a tool invocation routing path based on tool name + arguments.
///
/// Uses heuristics to estimate whether the tool will produce large output
/// that should be routed to sandbox execution + Tantivy storage.
///
/// # Arguments
/// * `tool_name` — e.g. "Bash", "Read", "Write"
/// * `tool_args` — parsed JSON arguments (from pre_tool_use input)
pub fn classify_tool_routing(tool_name: &str, tool_args: &Value) -> Option<RoutingDecision> {
    let threshold = crate::shared::feature_flags::routing_threshold_bytes();
    let estimated = estimate_output_size(tool_name, tool_args);
    if estimated > threshold {
        Some(RoutingDecision::RouteToSandbox)
    } else {
        Some(RoutingDecision::PassThrough)
    }
}

/// Estimates expected output bytes for a tool invocation.
///
/// Heuristic based on tool type and argument patterns:
/// - Bash with output-redirection args (grep -r, find, gh api, etc.) → large
/// - Read/Edit/Write → small (file content already in context)
/// - Grep/Glob with recursive flags → medium-large
///
/// # Arguments
/// * `tool_name` — e.g. "Bash", "Read", "Write"
/// * `tool_args` — parsed JSON arguments
pub fn estimate_output_size(tool_name: &str, tool_args: &Value) -> u64 {
    match tool_name {
        "Bash" => estimate_bash_output_size(tool_args),
        "Grep" => estimate_grep_output_size(tool_args),
        "Glob" => estimate_glob_output_size(tool_args),
        // Read/Edit/Write: small (file content in context, not large output)
        "Read" | "Edit" | "Write" | "WebFetch" => 512,
        _ => 1024, // default conservative small
    }
}

/// Heuristic for Bash tools with large-output indicators.
fn estimate_bash_output_size(args: &Value) -> u64 {
    let args_str = args.to_string();
    // Large-output patterns: recursive search, API calls, long listings
    let large_patterns = [
        "gh api ",
        "gh issue",
        "gh pr ",
        "git log",
        "git diff",
        "grep -r",
        "grep -l",
        "find .",
        "find /",
        "rg ",
        "ag ",
        "curl ",
        "wget ",
        "--json",
        "-l ",
        "-r ",
        "--recursive",
    ];
    let has_large = large_patterns.iter().any(|p| args_str.contains(p));
    if has_large {
        50_000 // heuristic: 50KB+ for large Bash commands
    } else {
        2048 // default: small
    }
}

/// Heuristic for Grep tool output size.
fn estimate_grep_output_size(args: &Value) -> u64 {
    let args_str = args.to_string();
    // Check for recursive flag in args string (-r, --recursive)
    let has_recursive_flag = args_str.contains("-r") || args_str.contains("--recursive");
    // Also check for `recursive: true` JSON field (field name + true value)
    let has_recursive_field =
        args_str.contains("\"recursive\":true") || args_str.contains("\"recursive\": true");
    let has_recursive = has_recursive_flag || has_recursive_field;
    let has_many = args_str.contains("-l") || args_str.contains("--files-with-matches");
    if has_recursive {
        30_000
    } else if has_many {
        10_000
    } else {
        2048
    }
}

/// Heuristic for Glob tool output size.
fn estimate_glob_output_size(args: &Value) -> u64 {
    let args_str = args.to_string();
    if args_str.contains("**") {
        20_000 // glob with recursion can hit many files
    } else {
        2048
    }
}

/// Builds sandbox-wrapped arguments for a tool invocation by **actually
/// running the subprocess** via the D2.2 executor and persisting the
/// captured output to the D2.3 Tantivy `tool_outputs` index.
///
/// On success the returned JSON contains the content_hash, summary and
/// exit_code so the LLM can address the cached output via
/// `touring_hooks::cli_handlers_mcp::ctx_retrieve` without re-running the tool.
///
/// On failure (subprocess error, missing index, feature disabled) the
/// function returns a JSON envelope with `ok: false` and the original
/// args echoed back — callers may then decide to fall back to direct
/// execution.
#[cfg(feature = "tantivy-fts")]
pub fn build_sandbox_wrapper_args(tool_name: &str, original_args: Value) -> Value {
    // S-13 cross-audit (2026-06-06): execute_and_store moved to the parent module
    // sandbox_output_store (tool-output storage); SandboxConfig stays in the gateway.
    use crate::sandbox_executor::SandboxConfig;
    use crate::sandbox_output_store::execute_and_store;
    let cfg = SandboxConfig {
        timeout_ms: crate::shared::feature_flags::sandbox_timeout_ms(),
        max_output_bytes: crate::shared::feature_flags::sandbox_max_output_bytes(),
        fallback_on_timeout: crate::shared::feature_flags::sandbox_fallback_on_timeout(),
    };
    match execute_and_store(tool_name, original_args.clone(), cfg) {
        Ok(res) => serde_json::json!({
            "_sandbox_routed": true,
            "ok": true,
            "tool_name": tool_name,
            "content_hash": res.content_hash,
            "exit_code": res.exit_code,
            "output_bytes": res.output_bytes,
            "was_truncated": res.was_truncated,
            "stored_path": res.stored_path
                .as_ref()
                .map(|p| p.display().to_string()),
        }),
        Err(e) => serde_json::json!({
            "_sandbox_routed": false,
            "ok": false,
            "error": e.to_string(),
            "original_args": original_args,
        }),
    }
}

/// Fallback when the `tantivy-fts` feature is disabled — preserves the
/// original API shape so call-sites stay feature-agnostic.
#[cfg(not(feature = "tantivy-fts"))]
pub fn build_sandbox_wrapper_args(_tool_name: &str, original_args: Value) -> Value {
    serde_json::json!({
        "_sandbox_routed": false,
        "ok": false,
        "error": "tantivy-fts feature disabled — sandbox storage unavailable",
        "original_args": original_args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_pass_through_small_bash() {
        let args = json!({"command": "echo hello"});
        let decision = classify_tool_routing("Bash", &args).unwrap();
        assert_eq!(decision, RoutingDecision::PassThrough);
    }

    #[test]
    fn test_route_large_bash_gh_api() {
        let args = json!({"command": "gh api repos"});
        let decision = classify_tool_routing("Bash", &args).unwrap();
        assert_eq!(decision, RoutingDecision::RouteToSandbox);
    }

    #[test]
    fn test_route_large_bash_grep_recursive() {
        let args = json!({"pattern": "TODO", "path": ".", "recursive": true});
        let decision = classify_tool_routing("Grep", &args).unwrap();
        assert_eq!(decision, RoutingDecision::RouteToSandbox);
    }

    #[test]
    fn test_pass_through_read() {
        let args = json!({"file_path": "src/main.rs"});
        let decision = classify_tool_routing("Read", &args).unwrap();
        assert_eq!(decision, RoutingDecision::PassThrough);
    }

    #[test]
    fn test_estimate_grep_recursive() {
        let args = json!({"pattern": "TODO", "path": ".", "recursive": true});
        let size = estimate_output_size("Grep", &args);
        assert!(size > 10_000);
    }

    #[test]
    fn test_estimate_grep_simple() {
        let args = json!({"pattern": "TODO", "path": "src"});
        let size = estimate_output_size("Grep", &args);
        assert!(size < 5000);
    }

    #[test]
    fn test_estimate_glob_recursive() {
        let args = json!({"pattern": "**/*.rs"});
        let size = estimate_output_size("Glob", &args);
        assert!(size > 10_000);
    }

    #[test]
    fn test_sandbox_wrapper_args_returns_envelope() {
        let original = json!({"command": "echo wrapper-shape-only"});
        let wrapped = build_sandbox_wrapper_args("Bash", original.clone());
        // Envelope must always carry these structural fields so callers
        // (HookResponse::ContextWithUpdatedInput) get a stable shape.
        assert!(wrapped.get("_sandbox_routed").is_some());
        assert!(wrapped.get("ok").is_some());
    }
}
