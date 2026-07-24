//! Heredoc temporal-split detection — the classifier and gate vocabulary.
//! Phase **P1.6** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`), with the
//! registry completed by **P5.2**.
//!
//! P1.6 introduced this module carrying a `StagingRegistry` stub. P5.2
//! promoted the registry to its full, content-indexed, cross-session form in
//! [`gateway::staging_registry`](crate::gateway::staging_registry). This
//! module now owns the two pieces that registry consumes: the *classifier*
//! ([`classify_command`]) and the *gate vocabulary* ([`TemporalSplitSignal`],
//! [`StagedScript`], [`ReanalysisReason`], [`StagingGateDecision`]).
//!
//! ## The threat
//!
//! A script can be **written in one turn** and **executed in a later one**:
//!
//! ```text
//! turn N:    cat > /tmp/x.sh <<EOF ... EOF      # the gate sees a write
//! turn N+M:  bash /tmp/x.sh                     # the gate sees only "bash <path>"
//! ```
//!
//! At execution time the `bash /tmp/x.sh` invocation carries no code body, so
//! the static / VGP / sandbox analysis (CEG X2-X5) would have nothing to
//! inspect. This module closes that gap: [`classify_command`] sorts every
//! bash command into a [`TemporalSplitSignal`]; the
//! [`StagingRegistry`](crate::gateway::staging_registry::StagingRegistry)
//! records every staged script, and its `gate_decision_for_execution`
//! guarantees an execution either reuses a recorded verdict or is forced
//! through full re-analysis — the mitigation for the plan's risk **R9**.
//!
//! ## Classifier scope
//!
//! Detection is heuristic — a whitespace-tokenized scan. It reliably sorts the
//! common write / exec forms; a fuller AST-based classifier (shell quoting,
//! pipelines, command substitution) is a future refinement and is not required
//! by the registry, which keys on content hash rather than parsed structure.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File extensions the temporal-split detector treats as executable scripts.
const SCRIPT_EXTENSIONS: &[&str] = &["sh", "bash", "py", "js", "ts", "rb", "pl", "php"];

/// Program names recognized as script *runners* (the "run later" half).
const SCRIPT_RUNNERS: &[&str] = &[
    "bash", "sh", "dash", "zsh", "ksh", "python", "python3", "node", "ruby", "perl", "php",
];

/// What a single bash command does with respect to the heredoc
/// temporal-split threat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalSplitSignal {
    /// The command writes a script file — the "write now" half
    /// (`cat > x.sh <<EOF`, `echo ... > x.py`, `tee x.js`).
    WritesScript {
        /// The script path being written.
        path: PathBuf,
    },
    /// The command runs a script file — the "run later" half
    /// (`bash x.sh`, `python3 x.py`, `./x.sh`), the moment the gate must
    /// re-assert itself.
    ExecutesScript {
        /// The script path being run.
        path: PathBuf,
    },
    /// The command neither writes nor runs a script file.
    Neither,
}

/// A script recorded in the
/// [`StagingRegistry`](crate::gateway::staging_registry::StagingRegistry) at
/// write time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedScript {
    /// The path the script was written to.
    pub path: PathBuf,
    /// An opaque content hash captured at stage time (the caller computes it,
    /// e.g. blake3 of the file bytes). Lets the registry detect a script
    /// mutated between the write turn and the exec turn.
    pub content_hash: String,
    /// An opaque marker of the turn / session that staged the script.
    pub origin: String,
    /// The X2/X3 verdict from stage-time analysis: `Some(true)` = passed,
    /// `Some(false)` = flagged, `None` = never analysed when written (so it
    /// must be analysed at exec time).
    pub prior_verdict: Option<bool>,
}

/// Why an execution must be (re)analysed rather than trusting a prior verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReanalysisReason {
    /// The script is absent from the registry — a temporal-split write the
    /// gate never saw (or a foreign file). R9: force full analysis.
    Unregistered,
    /// The script is registered but its content hash differs from stage time
    /// — it was mutated between turns.
    ContentChanged,
    /// The script is registered and unchanged but was never analysed when
    /// staged — there is no prior verdict to reuse.
    NeverAnalysed,
}

/// The decision for a script that is about to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StagingGateDecision {
    /// The script is known, unchanged, and was analysed at stage time — the
    /// recorded verdict may be reused without re-running X2-X5.
    ReuseVerdict {
        /// The stage-time analysis verdict (`true` = passed).
        verdict: bool,
    },
    /// The script must be analysed before it is allowed to run.
    RequiresReanalysis {
        /// Why re-analysis is required.
        reason: ReanalysisReason,
    },
}

/// Return `Some(path)` when `token` names a file with a script extension.
fn as_script_path(token: &str) -> Option<PathBuf> {
    let path = Path::new(token);
    let ext = path.extension()?.to_str()?;
    if SCRIPT_EXTENSIONS.contains(&ext) {
        Some(path.to_path_buf())
    } else {
        None
    }
}

/// Detect a script-writing redirect (`>`, `>>`, `tee`) in `tokens`.
fn detect_script_write(tokens: &[&str]) -> Option<PathBuf> {
    for (i, &tok) in tokens.iter().enumerate() {
        let candidate: Option<&str> = if let Some(rest) = tok.strip_prefix(">>") {
            if rest.is_empty() {
                tokens.get(i + 1).copied()
            } else {
                Some(rest)
            }
        } else if let Some(rest) = tok.strip_prefix('>') {
            if rest.is_empty() {
                tokens.get(i + 1).copied()
            } else {
                Some(rest)
            }
        } else if tok == "tee" {
            tokens[i + 1..]
                .iter()
                .find(|t| !t.starts_with('-'))
                .copied()
        } else {
            None
        };
        if let Some(c) = candidate {
            if let Some(path) = as_script_path(c) {
                return Some(path);
            }
        }
    }
    None
}

/// Detect a script being run (runner invocation or a direct `./script`).
fn detect_script_run(tokens: &[&str]) -> Option<PathBuf> {
    let first = *tokens.first()?;

    // Direct execution: `./deploy.sh` or `/abs/path/run.py`.
    if first.starts_with("./") || first.starts_with('/') {
        if let Some(path) = as_script_path(first) {
            return Some(path);
        }
    }

    // Runner invocation: `bash x.sh`, `python3 x.py`, `/usr/bin/node app.js`.
    let runner = Path::new(first)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(first);
    if SCRIPT_RUNNERS.contains(&runner) {
        for &arg in &tokens[1..] {
            if arg.starts_with('-') {
                continue; // a flag (`-c`, `-u`, ...) — keep scanning
            }
            return as_script_path(arg); // the first non-flag argument decides
        }
    }
    None
}

/// Classify a bash command for the heredoc temporal-split threat.
///
/// Heuristic (P1.6): a whitespace-tokenized scan. When a command both writes
/// and runs a script, the write is reported — it is the security-relevant
/// half to record.
pub fn classify_command(command: &str) -> TemporalSplitSignal {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    if let Some(path) = detect_script_write(&tokens) {
        return TemporalSplitSignal::WritesScript { path };
    }
    if let Some(path) = detect_script_run(&tokens) {
        return TemporalSplitSignal::ExecutesScript { path };
    }
    TemporalSplitSignal::Neither
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged(path: &str, hash: &str, verdict: Option<bool>) -> StagedScript {
        StagedScript {
            path: PathBuf::from(path),
            content_hash: hash.to_string(),
            origin: "test-turn".to_string(),
            prior_verdict: verdict,
        }
    }

    // ── classify_command: writes ──

    #[test]
    fn classify_writes_heredoc() {
        assert_eq!(
            classify_command("cat > /tmp/x.sh <<EOF"),
            TemporalSplitSignal::WritesScript {
                path: PathBuf::from("/tmp/x.sh")
            }
        );
    }

    #[test]
    fn classify_writes_echo_redirect() {
        assert_eq!(
            classify_command("echo print > /tmp/gen.py"),
            TemporalSplitSignal::WritesScript {
                path: PathBuf::from("/tmp/gen.py")
            }
        );
    }

    #[test]
    fn classify_writes_append() {
        assert_eq!(
            classify_command("echo more >> /tmp/a.js"),
            TemporalSplitSignal::WritesScript {
                path: PathBuf::from("/tmp/a.js")
            }
        );
    }

    #[test]
    fn classify_writes_glued_redirect_token() {
        assert_eq!(
            classify_command("cat >/tmp/run.sh"),
            TemporalSplitSignal::WritesScript {
                path: PathBuf::from("/tmp/run.sh")
            }
        );
    }

    #[test]
    fn classify_writes_tee() {
        assert_eq!(
            classify_command("echo x | tee /tmp/build.rb"),
            TemporalSplitSignal::WritesScript {
                path: PathBuf::from("/tmp/build.rb")
            }
        );
    }

    // ── classify_command: runs ──

    #[test]
    fn classify_runs_runner() {
        assert_eq!(
            classify_command("bash /tmp/x.sh"),
            TemporalSplitSignal::ExecutesScript {
                path: PathBuf::from("/tmp/x.sh")
            }
        );
    }

    #[test]
    fn classify_runs_python_with_flag() {
        assert_eq!(
            classify_command("python3 -u /tmp/job.py"),
            TemporalSplitSignal::ExecutesScript {
                path: PathBuf::from("/tmp/job.py")
            }
        );
    }

    #[test]
    fn classify_runs_absolute_runner() {
        assert_eq!(
            classify_command("/usr/bin/node /srv/app.js"),
            TemporalSplitSignal::ExecutesScript {
                path: PathBuf::from("/srv/app.js")
            }
        );
    }

    #[test]
    fn classify_runs_direct() {
        assert_eq!(
            classify_command("./deploy.sh --prod"),
            TemporalSplitSignal::ExecutesScript {
                path: PathBuf::from("./deploy.sh")
            }
        );
    }

    // ── classify_command: neither ──

    #[test]
    fn classify_neither_plain_command() {
        assert_eq!(
            classify_command("ls -la /tmp"),
            TemporalSplitSignal::Neither
        );
    }

    #[test]
    fn classify_neither_nonscript_redirect() {
        // A redirect to a non-script file is not a temporal-split write.
        assert_eq!(
            classify_command("echo data > /tmp/output.txt"),
            TemporalSplitSignal::Neither
        );
    }

    // ── gate vocabulary serde ──

    #[test]
    fn vocabulary_serde_roundtrip() {
        let sig = TemporalSplitSignal::WritesScript {
            path: PathBuf::from("/tmp/x.sh"),
        };
        let sj = serde_json::to_string(&sig).expect("serialize");
        assert_eq!(
            serde_json::from_str::<TemporalSplitSignal>(&sj).expect("deserialize"),
            sig
        );

        let script = staged("/tmp/x.sh", "h", Some(true));
        let pj = serde_json::to_string(&script).expect("serialize");
        assert_eq!(
            serde_json::from_str::<StagedScript>(&pj).expect("deserialize"),
            script
        );

        let dec = StagingGateDecision::RequiresReanalysis {
            reason: ReanalysisReason::Unregistered,
        };
        let dj = serde_json::to_string(&dec).expect("serialize");
        assert_eq!(
            serde_json::from_str::<StagingGateDecision>(&dj).expect("deserialize"),
            dec
        );
    }
}
