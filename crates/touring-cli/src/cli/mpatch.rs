//! CLI mpatch fuzzy-preview handler (`cli_mpatch_preview`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Feature-gated behind `mpatch-fuzzy`. The active implementation previews a
//! fuzzy patch against a file (with B-302 low-confidence-expansion diagnostics);
//! the `#[cfg(not(feature = "mpatch-fuzzy"))]` stub returns a "not enabled"
//! payload. Each gate carries its own imports so `cargo clippy --fix` (default
//! features) cannot strip the feature-only `std::path` import.

// Gated: only the `mpatch-fuzzy` implementation references these; ungated they
// are "unused" and `cargo clippy --fix` (default features) strips them —
// keep the cfg on each.
#[cfg(feature = "mpatch-fuzzy")]
use crate::runtime::HookRuntime;
#[cfg(feature = "mpatch-fuzzy")]
use std::path::{Path, PathBuf};

/// Handler: `touring mpatch preview` — preview a fuzzy patch against a file.
///
/// Payload: `{"file": "...", "patch": "...", "dry_run": bool}`. On no match,
/// returns `{"matched": false, "error": null}`.
#[cfg(feature = "mpatch-fuzzy")]
pub fn cli_mpatch_preview(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match payload.get("file").and_then(|v| v.as_str()) {
        Some(f) if !f.is_empty() => f,
        _ => {
            return serde_json::json!(
                { "matched" : false, "error" : "file field is required" }
            )
            .to_string();
        }
    };
    let patch = match payload.get("patch").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p,
        _ => {
            return serde_json::json!(
                { "matched" : false, "error" : "patch field is required" }
            )
            .to_string();
        }
    };
    let include_preview = payload
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let resolved = if Path::new(file_path).is_relative() {
        rt.project_root.join(file_path)
    } else {
        PathBuf::from(file_path)
    };
    let source = match std::fs::read_to_string(&resolved) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!(
                { "matched" : false, "error" : format!("read {}: {}", resolved.display(),
                e) }
            )
            .to_string();
        }
    };
    match crate::shared::mpatch_preview::preview_patch(&source, patch) {
        Some(p) => {
            let b302_diagnostic =
                touring_hooks_core::health_delta::emit_b302_if_low_confidence_expansion(
                    file_path, &source, &p,
                )
                .map(|delta| {
                    use touring_analysis::blast_radius::BlastWarning;
                    use touring_foundation::diagnostic::DiagnosticCode;
                    let finding = BlastWarning::PatchExpansion {
                        file: file_path.to_string(),
                        delta_bytes: delta.delta,
                        confidence: delta.confidence,
                    };
                    let diag = finding.to_diagnostic();
                    serde_json::json!(
                        { "code" : diag.code, "severity" : diag.severity.as_str(),
                        "message" : diag.message, "help" : diag.help, "delta_bytes" :
                        delta.delta, "confidence" : delta.confidence, "method" :
                        format!("{:?}", delta.method), }
                    )
                });
            let preview_text = if include_preview {
                Some(p.preview.clone())
            } else {
                None
            };
            serde_json::json!(
                { "matched" : p.matched, "method" : format!("{:?}", p.method),
                "confidence" : p.confidence, "preview" : preview_text, "b302_diagnostic"
                : b302_diagnostic, "error" : (serde_json::Value::Null) }
            )
            .to_string()
        }
        None => serde_json::json!(
            { "matched" : false, "b302_diagnostic" : (serde_json::Value::Null),
            "error" : (serde_json::Value::Null) }
        )
        .to_string(),
    }
}
/// Stub when `mpatch-fuzzy` is not enabled.
#[cfg(not(feature = "mpatch-fuzzy"))]
pub fn cli_mpatch_preview(
    _rt: &mut crate::runtime::HookRuntime,
    _payload: &serde_json::Value,
) -> String {
    serde_json::json!(
        { "matched" : false, "error" : "mpatch-fuzzy feature is not enabled" }
    )
    .to_string()
}
