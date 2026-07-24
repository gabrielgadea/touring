//! Tool-output storage bridge — run a sandbox subprocess and persist the result
//! to the `tool_outputs` Tantivy index.
//!
//! S-13 cross-audit (2026-06-06): relocated out of `gateway::sandbox_executor`.
//! This is *tool-output storage* — a parent-crate concern consumed by
//! `tool_output_router`, not the CEG sandbox runner. Keeping it here (instead of
//! inside the gateway) removes the `gateway -> {tantivy_index, compression_profiles}`
//! edges that the wholesale `sandbox_executor` relocation had surfaced, so the
//! gateway`s `sandbox_executor` is now purely the CEG sandbox. Gated on
//! `tantivy-fts` at the module declaration (lib.rs).

use crate::gateway::sandbox_executor::{
    SandboxConfig, SandboxError, SandboxResult, execute_in_sandbox_blocking, redact_secrets,
};
use serde_json::Value;

/// Converts a [`SandboxResult`] into the schema persisted in the
/// `tool_outputs` Tantivy index ([`crate::tantivy_index::ToolOutputDoc`]).
pub fn sandbox_result_to_doc(
    result: &SandboxResult,
    tool_name: &str,
) -> crate::tantivy_index::ToolOutputDoc {
    // NEW-1 — pass tool_name so derive_summary can dispatch to the right
    // per-command compression profile.
    let summary = derive_summary_with_tool(result, tool_name);
    let stored = result
        .stored_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    crate::tantivy_index::ToolOutputDoc {
        content_hash: result.content_hash.clone(),
        tool_name: tool_name.to_string(),
        summary,
        full_output_path: stored,
        exit_code: i64::from(result.exit_code),
        output_bytes: result.output_bytes,
        was_truncated: result.was_truncated,
        stored_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        // I-06 — sandbox doesn't currently surface raw tool_args here;
        // ctx_index callers can populate via direct ToolOutputDoc construction.
        tool_args: None,
    }
}

/// Derives a human-readable summary from the captured output bytes.
///
/// NEW-1 — Build a compressed summary from a sandbox result, dispatching
/// to per-command compression profiles based on `tool_name`.
///
/// Reads at most 512 bytes from the persisted file (when available) and
/// returns lossy UTF-8. Falls back to structural placeholders for
/// timeout-fallback or missing stored_path. When `tool_name` is empty the
/// dispatch falls through to passthrough (no profile match → raw text).
///
/// `sandbox_result_to_doc` invokes this with the real tool_name; tests
/// may pass an empty string to exercise the passthrough path.
pub fn derive_summary_with_tool(result: &SandboxResult, tool_name: &str) -> String {
    if result.exit_code == -2 {
        return format!(
            "<sandbox_timeout fallback exit=-2 truncated={}>",
            result.was_truncated
        );
    }
    let Some(path) = &result.stored_path else {
        return format!(
            "<no stored_path bytes={} truncated={}>",
            result.output_bytes, result.was_truncated
        );
    };
    let buf = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return format!("<read_error path={}>", path.display()),
    };
    let take = buf.len().min(512);
    let raw = String::from_utf8_lossy(&buf[..take]).to_string();
    // NEW-1 — apply per-command compression profile FIRST (drops noise
    // before truncation/redaction). Composes with I-04/I-12 pipeline:
    // compress → redact → snippet → store.
    let compressed =
        crate::compression_profiles::compress_for(tool_name, &serde_json::Value::Null, &raw);
    // I-12 — redact secrets before persisting summary so credentials don't
    // leak into the LLM's ctx_retrieve view.
    redact_secrets(&compressed)
}

/// Production-grade entry point: runs the subprocess, persists output to
/// disk, and stores the resulting [`crate::tantivy_index::ToolOutputDoc`]
/// in the global Tantivy `tool_outputs` index.
///
/// This is the single function the PreToolUse router calls on a
/// `RouteToSandbox` decision — it owns the full closure of D2.2 + D2.3.
pub fn execute_and_store(
    tool_name: &str,
    original_args: Value,
    config: SandboxConfig,
) -> Result<SandboxResult, SandboxError> {
    let result = execute_in_sandbox_blocking(tool_name, original_args, config)?;
    if let Some(idx) = crate::tantivy_index::global_tool_outputs() {
        let doc = sandbox_result_to_doc(&result, tool_name);
        if let Err(e) = idx.store_tool_output(&doc) {
            tracing::warn!(
                target: "touring::sandbox",
                hash = %result.content_hash,
                err = %e,
                "tool_outputs index store failed (continuing — non-fatal)"
            );
        }
    }
    Ok(result)
}
