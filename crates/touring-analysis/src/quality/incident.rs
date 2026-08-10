//! Incident Response (D51 / F4.11) — repository-shape detector of the
//! canonical "we are not ready for an incident" smells. When something
//! breaks at 3am, a runbook is the difference between 5 min and 5 hours
//! of downtime.
//!
//! | Smell | Signal |
//! |-------|--------|
//! | `no-runbook-dir` | project has no `docs/runbooks/`, `runbook/`, or `RUNBOOK.md` |
//! | `no-postmortem-dir` | project has no `docs/postmortems/`, `postmortem/`, or `POSTMORTEM.md` |
//! | `no-rollback-doc` | project has no `ROLLBACK.md`, `docs/rollback.md`, or `RUNBOOK.md` |
//! | `no-oncall-doc` | project has no `ONCALL.md`, `RUNBOOK.md`, or `docs/oncall.md` |
//! | `no-severity-defs` | no `SEV-1` / `SEV-2` / `SEV-3` mentions in the workspace |
//! | `no-blameless-mention` | no `blameless` / `no-blame` / `no blame` reference (postmortem culture) |
//!
//! **Disjoint** from D34 inline-doc (which keys on `///` rustdoc;
//! F4.11 keys on incident-response artifacts at the repo level) and
//! F3.10 arch-doc (which keys on ADR / Mermaid; F4.11 keys on
//! runbook / postmortem).
//!
//! **Sources (context7, `/websites/response_pagerduty`):** PagerDuty
//! Incident Response process emphasizes: "Postmortems need to be
//! blameless. If someone made a mistake, you just spent lots of money
//! training them to never do it again. You can't fire your way to
//! reliability." SEV-1/SEV-2/SEV-3 definitions are part of the
//! incident-commander process.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

const SCALE: f32 = 6.0;

/// Incident-response findings for one file (or aggregate across the workspace).
pub type IncidentReport = crate::quality::SmellReport;

const RUNBOOK_DIR_1: &[u8] = b"docs/runbooks";
const RUNBOOK_DIR_2: &[u8] = b"runbook/";
const RUNBOOK_FILE: &[u8] = b"RUNBOOK.md";
const POSTMORTEM_DIR_1: &[u8] = b"docs/postmortems";
const POSTMORTEM_DIR_2: &[u8] = b"postmortem/";
const POSTMORTEM_FILE: &[u8] = b"POSTMORTEM.md";
const ROLLBACK_FILE_1: &[u8] = b"ROLLBACK.md";
const ROLLBACK_FILE_2: &[u8] = b"docs/rollback.md";
const ONCALL_FILE_1: &[u8] = b"ONCALL.md";
const ONCALL_FILE_2: &[u8] = b"docs/oncall.md";
const SEV_1: &[u8] = b"SEV-1";
const SEV_2: &[u8] = b"SEV-2";
const SEV_3: &[u8] = b"SEV-3";
const BLAMELESS: &[u8] = b"blameless";
const NO_BLAME_1: &[u8] = b"no-blame";
const NO_BLAME_2: &[u8] = b"no blame";

fn has_in_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> bool {
    memmem::find_iter(bytes, needle).any(|off| !offset_suppressed(off, regions))
}

fn detect_no_runbook(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, RUNBOOK_DIR_1)
        || has_in_executable(bytes, regions, RUNBOOK_DIR_2)
        || has_in_executable(bytes, regions, RUNBOOK_FILE)
    {
        0
    } else {
        1
    }
}

fn detect_no_postmortem(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, POSTMORTEM_DIR_1)
        || has_in_executable(bytes, regions, POSTMORTEM_DIR_2)
        || has_in_executable(bytes, regions, POSTMORTEM_FILE)
        || has_in_executable(bytes, regions, RUNBOOK_FILE)
    {
        // RUNBOOK.md often doubles as a runbook + postmortem template.
        0
    } else {
        1
    }
}

fn detect_no_rollback(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, ROLLBACK_FILE_1)
        || has_in_executable(bytes, regions, ROLLBACK_FILE_2)
        || has_in_executable(bytes, regions, RUNBOOK_FILE)
    {
        0
    } else {
        1
    }
}

fn detect_no_oncall(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, ONCALL_FILE_1)
        || has_in_executable(bytes, regions, ONCALL_FILE_2)
        || has_in_executable(bytes, regions, RUNBOOK_FILE)
    {
        0
    } else {
        1
    }
}

fn detect_no_severity_defs(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_1 = has_in_executable(bytes, regions, SEV_1);
    let has_2 = has_in_executable(bytes, regions, SEV_2);
    let has_3 = has_in_executable(bytes, regions, SEV_3);
    // Need at least 2 of 3 to claim "severity defs present"
    let count = (has_1 as u8) + (has_2 as u8) + (has_3 as u8);
    if count >= 2 { 0 } else { 1 }
}

fn detect_no_blameless(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    if has_in_executable(bytes, regions, BLAMELESS)
        || has_in_executable(bytes, regions, NO_BLAME_1)
        || has_in_executable(bytes, regions, NO_BLAME_2)
    {
        0
    } else {
        1
    }
}

/// Analyze incident-response smells in `source` (typically a `.md` file or
/// a concatenated workspace dump). The lang parameter is ignored -- this
/// engine is shape-based.
pub fn analyze_incident(source: &str, _lang: &str) -> IncidentReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, "rust");
    let mut report = IncidentReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    report.push(
        "no runbook dir/file (no `docs/runbooks/`, `runbook/`, or `RUNBOOK.md`)",
        detect_no_runbook(bytes, &regions),
        0.9,
    );
    report.push(
        "no postmortem dir/file (no `docs/postmortems/`, `postmortem/`, or `POSTMORTEM.md`)",
        detect_no_postmortem(bytes, &regions),
        0.7,
    );
    report.push(
        "no rollback doc (no `ROLLBACK.md` or `docs/rollback.md`)",
        detect_no_rollback(bytes, &regions),
        0.8,
    );
    report.push(
        "no on-call doc (no `ONCALL.md` or `docs/oncall.md`)",
        detect_no_oncall(bytes, &regions),
        0.7,
    );
    report.push(
        "no severity definitions (need at least 2 of SEV-1 / SEV-2 / SEV-3 in workspace)",
        detect_no_severity_defs(bytes, &regions),
        0.6,
    );
    report.push(
        "no blameless / no-blame postmortem culture reference",
        detect_no_blameless(bytes, &regions),
        0.6,
    );
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`IncidentReport`] as `1 - density * SCALE`, clamped to `[0, 1]`.
pub fn score_incident(report: &IncidentReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(src: &str) -> IncidentReport {
        analyze_incident(src, "rust")
    }

    #[test]
    fn empty_workspace_no_artifacts() {
        let r = rep("");
        // Empty source has no runbook / postmortem / rollback / oncall / severity / blameless
        assert!(r.violations >= 1, "empty: {:?} ", r.findings);
    }

    #[test]
    fn runbook_present_clean() {
        let src = r#"
# Path: docs/runbooks/db-failover.md

## SEV-1

Steps to recover from DB failover.
This is a blameless runbook.
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("runbook")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn runbook_file_clean() {
        let src = r#"# RUNBOOK.md

## SEV-1
Steps.

## SEV-2
Steps.

## blameless
Notes.
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("runbook")
                || m.contains("postmortem")
                || m.contains("blameless")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn no_severity_defs_flagged() {
        let src = r#"
# Some doc without SEV-* mentions
"#;
        let r = rep(src);
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("severity")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn two_severities_clean() {
        let src = r#"
# Doc
SEV-1
SEV-2
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("severity")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn blameless_clean() {
        let src = r#"
# Postmortem template
This is a blameless postmortem.
"#;
        let r = rep(src);
        assert!(
            !r.findings.iter().any(|(m, _)| m.contains("blameless")),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = rep("# Just a title\n");
        let good = rep(r#"
# docs/runbooks/db-failover.md

## SEV-1
Steps to recover.

## SEV-2
## SEV-3

This is a blameless postmortem.
"#);
        assert!(
            score_incident(&bad) < score_incident(&good),
            "untuned ({:.3}) must score below tuned ({:.3})",
            score_incident(&bad),
            score_incident(&good)
        );
    }
}
