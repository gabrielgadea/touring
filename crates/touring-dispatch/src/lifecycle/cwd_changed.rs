//! `cwd-changed` hook handler + directory→generator pattern helpers.
//!
//! Fires when Claude Code's working directory changes (e.g. `cd` into a new
//! crate). Records the access event and emits wiring/generator hints so the
//! engineer is immediately oriented to the new scope. Extracted from
//! `lifecycle.rs` as part of FIX-3 modularization.

use serde_json::Value;

use crate::runtime::HookRuntime;

/// cwd-changed: record access + emit wiring + generator hint.
///
/// Tracks working directory changes so the knowledge graph can correlate
/// file accesses with directory context for better predictions. R23-S3
/// emits a wiring hint; R47-S1 additionally maps the new CWD to a
/// GeneratorKind so Claude Code can scaffold the appropriate artifact.
pub(crate) fn handle_cwd_changed(rt: &mut HookRuntime, input: &Value) -> String {
    let new_dir = input.get("new_cwd").and_then(|v| v.as_str()).unwrap_or("");

    if !new_dir.is_empty() {
        let _ = rt.ctx.knowledge.record_access(new_dir, "__cwd_changed__");
        tracing::debug!(dir = new_dir, "cwd changed — access recorded");
    }

    let wiring = cwd_wiring_hint(new_dir);
    if wiring.is_empty() {
        return String::new();
    }
    let gen_hint = maybe_generator_for_new_cwd(new_dir).unwrap_or_default();
    format!("{wiring}{gen_hint}")
}

/// Build a wiring + integration hint for a new working directory (R23-S3).
///
/// Suggests relevant `touring wiring` commands so the engineer can orient
/// themselves to the integration landscape of the new CWD immediately.
/// Returns empty string when `new_dir` is empty.
///
/// `pub(crate)` so inline tests in `lifecycle::tests` continue to reach
/// this helper via `super::cwd_wiring_hint` after the parent re-export.
pub(crate) fn cwd_wiring_hint(new_dir: &str) -> String {
    if new_dir.is_empty() {
        return String::new();
    }
    format!(
        "cwd-changed: now in {new_dir} | \
        run `touring wiring score {new_dir}` for integration score | \
        run `touring wiring suggest {new_dir}` for orphan opportunities"
    )
}

/// R47-S1: Map new CWD directory pattern to a GeneratorKind hint (CC≤2).
///
/// When a directory change (FileChanged / cwd-changed) is detected, suggests
/// the most relevant `touring generate render <Kind>` command based on the
/// directory name pattern. Closes the loop: CWD change → generator scaffold
/// → artifact creation via touring-generator.
fn maybe_generator_for_new_cwd(new_dir: &str) -> Option<String> {
    if new_dir.is_empty() {
        return None;
    }
    let kind = generator_kind_for_dir_pattern(new_dir)?;
    Some(format!(
        " | generator: run `touring generate render {kind}` to scaffold artifacts for {new_dir}"
    ))
}

/// Static pattern table: directory substring → GeneratorKind name (CC=2).
///
/// `pub(crate)` so inline tests can assert the mapping directly via
/// `super::generator_kind_for_dir_pattern`.
pub(crate) fn generator_kind_for_dir_pattern(new_dir: &str) -> Option<&'static str> {
    const PATTERNS: &[(&str, &str)] = &[
        ("/crates/", "rust_module"),
        ("/src", "rust_module"),
        ("/tests", "test"),
        ("/test", "test"),
        ("/migrations", "migration"),
        ("/scripts", "python_script"),
        ("/bin", "python_script"),
        ("/docs", "plan.md"),
        ("/doc", "plan.md"),
        ("/templates", "skill_document"),
        // R61-S1: infra/ops directory patterns → matching GeneratorKind
        ("/kubernetes", "k8s_manifest"),
        ("/k8s", "k8s_manifest"),
        ("/terraform", "terraform_module"),
        ("/ci", "ci_workflow"),
        ("/proto", "protobuf_schema"),
        ("/hooks", "hook_handler"),
        ("/api", "openapi_spec"),
    ];
    let lower = new_dir.to_lowercase();
    PATTERNS
        .iter()
        .find(|(pat, _)| lower.contains(pat))
        .map(|(_, kind)| *kind)
}
