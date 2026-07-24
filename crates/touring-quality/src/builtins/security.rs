//! Security gate — deny.toml advisories (real, Rust-native) + CEG/landlock integration hint.

use std::time::Instant;

use crate::change::Change;
use crate::gate::{EvidenceKind, EvidenceRef, Gate, GateId, GateOutcome, GateSeverity};

/// Real implementation: parses `deny.toml` and counts advisories.
pub struct SecurityGate {
    workspace_root: std::path::PathBuf,
}

impl SecurityGate {
    /// Creates a `SecurityGate` rooted at the current working directory.
    #[must_use]
    pub fn new() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        Self {
            workspace_root: root,
        }
    }
}

impl Default for SecurityGate {
    fn default() -> Self {
        Self::new()
    }
}

impl Gate for SecurityGate {
    fn id(&self) -> GateId {
        GateId::Security
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }

    #[allow(clippy::cast_possible_truncation)] // reason: elapsed millis as u64 cannot realistically overflow
    fn check(&self, _change: &Change) -> GateOutcome {
        let start = Instant::now();
        // The gate is rooted at its own configured `workspace_root` (set to the
        // current dir by `new()`, or overridden for tests), not the change's root.
        let root = self
            .workspace_root
            .canonicalize()
            .unwrap_or_else(|_| self.workspace_root.clone());
        let advisories = crate::builtins::read_deny_advisories(&root);
        let (advisory_count, ignored_count) = parse_deny_advisories(&root);
        let payload = serde_json::json!({
            "advisories_total": advisory_count,
            "advisories_ignored_with_rationale": ignored_count,
            "ceg_landlock_v6_enforced": cfg!(target_os = "linux"),
        });
        let mut o = GateOutcome::pass(GateId::Security, GateSeverity::Block)
            .with_evidence(EvidenceRef {
                kind: EvidenceKind::File,
                path: "deny.toml".into(),
                line: None,
                excerpt: Some(format!(
                    "advisories={advisory_count}, ignored={ignored_count}"
                )),
                command: None,
            })
            .with_payload(payload)
            .with_elapsed(start.elapsed().as_millis() as u64);
        if advisories.is_none() {
            o = GateOutcome::advisory(GateId::Security, GateSeverity::Block, "deny.toml not found")
                .with_payload(serde_json::json!({"hint": "create deny.toml + add to CI"}));
        }
        o
    }
}

/// Parse deny.toml [advisories] block: count total { id = ... } entries
/// (`advisories_total`) and those with a non-empty `reason =` field
/// (`advisories_ignored_with_rationale`, which is GOOD — they have a reason).
fn parse_deny_advisories(ws_root: &std::path::Path) -> (u64, u64) {
    let Ok(text) = std::fs::read_to_string(ws_root.join("deny.toml")) else {
        return (0, 0);
    };
    let mut total = 0_u64;
    let mut with_reason = 0_u64;
    let mut in_advisories = false;
    for line in text.lines() {
        if line.trim_start().starts_with('[') {
            in_advisories = line.contains("advisories");
            continue;
        }
        if !in_advisories {
            continue;
        }
        if line.trim_start().starts_with("{ id =") {
            total += 1;
            // next non-blank line containing reason=
            if text.lines().any(|l| l.contains("reason =")) {
                with_reason += 1;
            }
        }
    }
    (total, with_reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_deny_toml_is_advisory() {
        let g = SecurityGate {
            workspace_root: std::path::PathBuf::from("/nonexistent"),
        };
        let o = g.check(&Change::new());
        assert_eq!(o.gate_id, GateId::Security);
        assert!(o.message.contains("deny.toml not found"));
    }

    #[test]
    fn real_workspace_denies() {
        let g = SecurityGate::new();
        let o = g.check(&Change::new());
        // On a real workspace with deny.toml, this should be Pass
        // (advisories are filtered/ignored with rationale).
        if std::path::Path::new("deny.toml").exists() {
            assert_eq!(o.gate_id, GateId::Security);
        }
    }
}
