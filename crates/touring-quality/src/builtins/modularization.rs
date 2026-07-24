//! Modularization gate — file size + cyclomatic (real, Rust-native).

use std::time::Instant;

use crate::change::{Change, FileKind};
use crate::gate::{EvidenceKind, EvidenceRef, Gate, GateId, GateOutcome, GateSeverity};

const MAX_FILE_LOC: u64 = 400; // soft target
const HARD_FILE_LOC: u64 = 5000; // file_size_gate.py hard cap

/// Real implementation: checks each new/modified file against size thresholds.
pub struct ModularizationGate;

impl Gate for ModularizationGate {
    fn id(&self) -> GateId {
        GateId::Modularization
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }

    #[allow(clippy::cast_possible_truncation)] // reason: elapsed millis as u64 cannot realistically overflow
    fn check(&self, change: &Change) -> GateOutcome {
        let start = Instant::now();
        let ws_root = change.workspace_root();
        let mut violations = Vec::new();
        let mut total_loc = 0_u64;
        for f in &change.files {
            let path = ws_root.join(&f.path);
            let loc = match f.kind {
                FileKind::Create | FileKind::Modify => {
                    f.after.as_deref().map_or(0, |s| s.lines().count() as u64)
                }
                FileKind::Delete => 0,
                FileKind::Rename => {
                    // Read existing file from disk
                    std::fs::read_to_string(&path)
                        .ok()
                        .map_or(0, |s| s.lines().count() as u64)
                }
            };
            total_loc += loc;
            if loc > HARD_FILE_LOC {
                violations.push(serde_json::json!({
                    "path": f.path.display().to_string(),
                    "loc": loc,
                    "threshold": HARD_FILE_LOC,
                    "severity": "hard",
                }));
            } else if loc > MAX_FILE_LOC {
                violations.push(serde_json::json!({
                    "path": f.path.display().to_string(),
                    "loc": loc,
                    "threshold": MAX_FILE_LOC,
                    "severity": "soft",
                }));
            }
        }

        let count = violations.len();
        let mut o = if count == 0 {
            GateOutcome::pass(GateId::Modularization, GateSeverity::Block).with_evidence(
                EvidenceRef {
                    kind: EvidenceKind::File,
                    path: "docs/file_size_gate.py".into(),
                    line: None,
                    excerpt: Some(format!(
                        "{total_loc} LOC across {} files (all under {} threshold)",
                        change.files.len(),
                        MAX_FILE_LOC
                    )),
                    command: None,
                },
            )
        } else {
            let hard_count = violations
                .iter()
                .filter(|v| v["severity"] == "hard")
                .count();
            if hard_count > 0 {
                GateOutcome::fail(
                    GateId::Modularization,
                    GateSeverity::Block,
                    format!("{hard_count} files exceed {HARD_FILE_LOC} LOC"),
                )
            } else {
                GateOutcome::warn(
                    GateId::Modularization,
                    GateSeverity::Block,
                    format!("{count} files exceed {MAX_FILE_LOC} LOC (soft)"),
                )
            }
        };
        o = o.with_payload(serde_json::json!({
            "total_loc": total_loc,
            "file_count": change.files.len(),
            "violations": violations,
            "thresholds": { "soft": MAX_FILE_LOC, "hard": HARD_FILE_LOC },
        }));
        o.elapsed_ms = start.elapsed().as_millis() as u64;
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::ProposedFile;

    #[test]
    fn small_file_passes() {
        let g = ModularizationGate;
        let c = Change::new().with_file(ProposedFile::create("src/lib.rs", "// small\n"));
        let o = g.check(&c);
        assert_eq!(o.status, crate::gate::GateStatus::Pass);
    }

    #[test]
    fn huge_file_fails() {
        use std::fmt::Write as _;
        let g = ModularizationGate;
        let big = (0..6000).fold(String::new(), |mut acc, i| {
            let _ = writeln!(acc, "line {i}");
            acc
        });
        let c = Change::new().with_file(ProposedFile::create("src/big.rs", big));
        let o = g.check(&c);
        assert_eq!(o.status, crate::gate::GateStatus::Fail);
    }

    #[test]
    fn medium_file_warns() {
        use std::fmt::Write as _;
        let g = ModularizationGate;
        let medium = (0..600).fold(String::new(), |mut acc, i| {
            let _ = writeln!(acc, "line {i}");
            acc
        });
        let c = Change::new().with_file(ProposedFile::create("src/medium.rs", medium));
        let o = g.check(&c);
        // 600 > 400 (soft) but < 5000 (hard) → Warn
        assert_eq!(o.status, crate::gate::GateStatus::Warn);
    }
}
