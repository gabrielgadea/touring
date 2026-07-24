//! CEG X7 extension: `touring-quality` BLOCK-by-default in Edit/Write.
//!
//! For tool calls that modify files (`Edit`, `Write`, `MultiEdit`), the
//! 17-gate harness computes the composite EliteScore and BLOCKs the call
//! if `tier < Gold`. The verdict is augmented with a `canonical_fix` that
//! includes the composite score, tier, and which gate(s) failed.
//!
//! ## Fail-open policy
//!
//! If the payload doesn't parse as JSON, or no `Change` can be constructed,
//! the call is **allowed** (no false positives — the other 6 CEG stages
//! still gate). The harness is a *complement*, not a replacement.
//!
//! ## W5 close (2026-06-26)
//!
//! Migrated from `touring_harness::*` to `touring_quality::*` (canonical home).

use serde_json::Value;
use touring_quality::{
    Change, FileKind, HarnessConfig, ProposedFile, builtin_default_gates, run_harness,
};

/// Tools that should be intercepted by the harness BLOCK-by-default gate.
const FILE_MUTATION_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Outcome of the harness check on a file-mutation tool call.
#[derive(Debug, Clone)]
pub enum HarnessVerdict {
    /// Allow — tier ≥ Gold (release-ready). Tool call proceeds.
    Allow {
        /// Composite EliteScore in `[0, 1]` (≥ 0.80 = Gold tier, release-ready).
        composite: f32,
        /// Human-readable tier name (`Diamond` / `Platinum` / `Gold`).
        tier: String,
        /// Rendered status badge string for display in CLI / MCP output.
        badge: String,
    },
    /// Deny — tier < Gold. Tool call BLOCKed. `canonical_fix` describes why.
    Deny {
        /// Composite EliteScore in `[0, 1]` that fell below the Gold threshold.
        composite: f32,
        /// Human-readable tier name (`Silver` / `Bronze` / `Unranked`).
        tier: String,
        /// Diagnostic describing the failing gate(s) and how to reach Gold.
        canonical_fix: String,
    },
    /// Pass-through (tool not in `FILE_MUTATION_TOOLS`, or payload unparseable,
    /// or change empty). Other CEG stages still gate.
    PassThrough,
}

/// Check a tool call against the 17-gate harness.
///
/// Returns `HarnessVerdict::Allow` / `Deny` for Edit/Write/MultiEdit/NotebookEdit
/// when the payload parses and the Change is non-empty; `PassThrough` otherwise.
#[must_use]
pub fn harness_block_for_tool(
    tool: &str,
    payload: &str,
    agent_id: Option<&str>,
    model: Option<&str>,
) -> HarnessVerdict {
    if !FILE_MUTATION_TOOLS.contains(&tool) {
        return HarnessVerdict::PassThrough;
    }
    let change = match parse_change(tool, payload) {
        Some(c) => c,
        None => return HarnessVerdict::PassThrough,
    };
    let change = if let Some(a) = agent_id {
        change.agent(a.to_string())
    } else {
        change
    };
    let change = if let Some(m) = model {
        change.model(m.to_string())
    } else {
        change
    };

    if change.files.is_empty() {
        return HarnessVerdict::PassThrough;
    }

    let gates = builtin_default_gates();
    let cfg = HarnessConfig::default();
    let score = run_harness(&change, &gates, &cfg);

    if score.is_release_ready() {
        HarnessVerdict::Allow {
            composite: score.composite,
            tier: format!("{:?}", score.tier),
            badge: score.badge(),
        }
    } else {
        let mut fix = format!(
            "EliteScore {:.4} < Gold tier (0.80) for tool={tool}. ",
            score.composite
        );
        for g in score.block_reasons() {
            fix.push_str(&format!(
                "\n  - {} BLOCK-fail: {}",
                g.gate_id.slug(),
                g.message
            ));
        }
        for g in score.warn_reasons() {
            fix.push_str(&format!("\n  - {} WARN: {}", g.gate_id.slug(), g.message));
        }
        HarnessVerdict::Deny {
            composite: score.composite,
            tier: format!("{:?}", score.tier),
            canonical_fix: fix,
        }
    }
}

/// Parse the tool payload into a `Change` with one `ProposedFile`.
///
/// Recognized JSON shapes:
/// - `{ "file_path": "src/lib.rs", "content": "..." }`  (Write / MultiEdit)
/// - `{ "path": "src/lib.rs", "after": "..." }`            (Edit)
/// - `{ "filepath": "...", "new_content": "..." }`           (MCP variant)
fn parse_change(_tool: &str, payload: &str) -> Option<Change> {
    let v: Value = serde_json::from_str(payload).ok()?;
    let path = v
        .get("file_path")
        .or_else(|| v.get("path"))
        .or_else(|| v.get("filepath"))
        .and_then(|p| p.as_str())
        .map(std::path::PathBuf::from)?;
    let after = v
        .get("content")
        .or_else(|| v.get("after"))
        .or_else(|| v.get("new_content"))
        .and_then(|c| c.as_str())
        .map(String::from)
        .unwrap_or_default();
    let before = v.get("before").and_then(|c| c.as_str()).map(String::from);
    let kind = if before.is_some() {
        FileKind::Modify
    } else {
        FileKind::Create
    };
    Some(Change::new().with_file(ProposedFile {
        path,
        before,
        after: Some(after),
        kind,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_mutation_tool_passes_through() {
        let v = harness_block_for_tool("Read", "{\"file_path\":\"/x\"}", None, None);
        assert!(matches!(v, HarnessVerdict::PassThrough));
    }

    #[test]
    fn edit_with_synthetic_change_passes_through() {
        // Empty payload → parse_change returns None → PassThrough.
        let v = harness_block_for_tool("Edit", "{}", None, None);
        assert!(matches!(v, HarnessVerdict::PassThrough));
    }

    #[test]
    fn edit_with_diamond_score_allows() {
        // The 17-gate default set gives 0.97 on synthetic changes → Allow.
        let payload = r#"{"file_path":"src/lib.rs","content":"// new"}"#;
        let v = harness_block_for_tool(
            "Edit",
            payload,
            Some("claude-opus-4-7"),
            Some("claude-opus-4-7"),
        );
        assert!(matches!(v, HarnessVerdict::Allow { .. }));
    }
}
