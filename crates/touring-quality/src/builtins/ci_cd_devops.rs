//! CI/CD & DevOps gate — root hygiene (real, lightweight).

use std::time::Instant;

use crate::change::Change;
use crate::gate::{EvidenceKind, EvidenceRef, Gate, GateId, GateOutcome, GateSeverity};

/// Real implementation: detects stray workspace-root files (.tf-bak, .rlib, etc.).
pub struct CiCdDevopsGate;

const STRAY_PATTERNS: &[&str] = &["*.tf-bak.*", "*.rlib", "debug_js.js", "*.swp", "nohup.out"];

impl Gate for CiCdDevopsGate {
    fn id(&self) -> GateId {
        GateId::CiCdDevops
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }
    // reason: gate-check elapsed millis always fit in u64; truncation is impossible in practice.
    #[allow(clippy::cast_possible_truncation)]
    fn check(&self, change: &Change) -> GateOutcome {
        let start = Instant::now();
        let root = change.workspace_root();
        let mut stray = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                for pat in STRAY_PATTERNS {
                    if glob_match(pat, &name) {
                        stray.push(name.to_string());
                        break;
                    }
                }
            }
        }
        let count = stray.len();
        let mut o = if count == 0 {
            GateOutcome::pass(GateId::CiCdDevops, GateSeverity::Block).with_evidence(EvidenceRef {
                kind: EvidenceKind::Command,
                path: "docs/root_hygiene_gate.py".into(),
                line: None,
                excerpt: None,
                command: Some("python3 docs/root_hygiene_gate.py --check".into()),
            })
        } else {
            GateOutcome::fail(
                GateId::CiCdDevops,
                GateSeverity::Block,
                format!("{count} stray artifacts: {stray:?}"),
            )
        };
        o = o.with_payload(serde_json::json!({"stray": stray, "count": count}));
        o.elapsed_ms = start.elapsed().as_millis() as u64;
        o
    }
}

/// Tiny glob matcher. Supports:
/// - `*.ext` — match by extension
/// - `prefix.*` — match by prefix
/// - `pat.*.ext` — match by middle
fn glob_match(pattern: &str, name: &str) -> bool {
    if pattern == name {
        return true;
    }
    // *.ext — endswith .ext
    if let Some(rest) = pattern.strip_prefix("*.")
        && !rest.contains('*')
    {
        return name.ends_with(&format!(".{rest}"));
    }
    // Handle patterns with multiple stars (e.g. "*.tf-bak.*")
    let p_segs: Vec<&str> = pattern.split('*').filter(|s| !s.is_empty()).collect();
    if p_segs.is_empty() {
        return true; // pattern is all stars
    }
    // All non-star segments must appear in name, in order
    let mut idx = 0;
    for seg in &p_segs {
        match name[idx..].find(seg) {
            Some(pos) => idx = pos + seg.len(),
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glob_basic() {
        assert!(glob_match("*.rlib", "foo.rlib"));
        assert!(glob_match("*.tf-bak.*", "x.tf-bak.123"));
        assert!(!glob_match("*.rlib", "foo.txt"));
    }
}
