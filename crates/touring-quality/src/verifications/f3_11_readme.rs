//! F3.11 — README Completeness verifier (D37).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_readme`] — a file-based detector of
//! the canonical "missing essential section" smell in a project `README*`
//! file:
//!
//! | Required section | Heading needles (case-insensitive) |
//! |------------------|--------------------------------------|
//! | title            | first `# …` line |
//! | description      | "About"/"Description"/"Overview"/"Introduction"/"What is" |
//! | install          | "Install"/"Installation"/"Setup"/"Getting Started" |
//! | usage            | "Usage"/"Example"/"Quickstart"/"How to" |
//! | contributing     | "Contributing"/"Development" |
//! | tests            | "Tests"/"Testing" |
//! | license          | "License"/"Licence" |
//!
//! Optional (informational, no penalty): badges (`[![…](…)`), ToC.
//!
//! Disjoint from F3.10 arch doc (which keys on `docs/adr/` + ` ```mermaid ` —
//! F3.11 keys on the README's high-level **content sections**); F3.12 doc
//! accuracy (which keys on **drift** between code and docs — F3.11 keys on
//! **presence** of canonical sections).
//!
//! **Sources (context7, `/othneildrew/best-readme-template`, High reputation,
//! bench 85)**: the canonical README template prescribes Project Title +
//! About + Getting Started + Usage + Contributing + License + Acknowledgments.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled `##` /
//! `Install` / `License` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.11 verifier — README Completeness.
#[allow(non_camel_case_types)]
pub struct F3_11_Readme;

impl Verification for F3_11_Readme {
    fn id(&self) -> DimId {
        DimId::F3_11
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_readme_dim(target)
    }
}

// ── Real engine: README completeness ────────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_readme_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_readme, score_readme};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.11: detector own source (readme needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let (raw, present) = crate::verifications::read_artifact_source(
        target,
        crate::verifications::ArtifactClass::Readme,
    );
    if !present {
        return Ok((
            crate::verifications::ARTIFACT_ABSENT_CAP,
            "F3.11: no README* found in scope — repository has no README \
             (absent-artifact cap, not diluted density)"
                .to_string(),
        ));
    }
    let r = analyze_readme(&raw, "markdown");
    let value = score_readme(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F3.11: {} README completeness gap(s) over {} lines — score={value:.3} \
         (touring-analysis analyze_readme: missing required README section \
         (title/description/install/usage/contributing/tests/license)){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the README needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: section-density heuristic ─────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_readme_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let has_install = raw.contains("## Install") || raw.contains("## Installation");
    let has_usage = raw.contains("## Usage") || raw.contains("## Example");
    let has_license = raw.contains("## License");
    let required = (has_install as u8 + has_usage as u8 + has_license as u8) as f32;
    let value = (required / 3.0).min(1.0);
    let evidence = format!(
        "{required:.0}/3 essential sections (Install/Usage/License) present over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full README analysis)"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_ext(content: &str, suffix: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_readme_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".md");
        let s = F3_11_Readme.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_readme_empty_file() {
        let f = write_temp("");
        let s = F3_11_Readme.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Canonical README scores higher than minimal one-liner.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_canonical_scores_higher_than_minimal() {
        let canonical = "# t\n\n## Description\nd\n\n## Installation\ni\n\n## Usage\nu\n\n## Contributing\nc\n\n## Tests\nt\n\n## License\nMIT\n";
        let minimal = "just a one-liner";
        let f_canonical = write_temp_ext(canonical, ".md");
        let f_minimal = write_temp_ext(minimal, ".md");
        let sc = F3_11_Readme.check(f_canonical.path()).expect("check");
        let sm = F3_11_Readme.check(f_minimal.path()).expect("check");
        assert!(
            sc.value > sm.value,
            "canonical ({}) must score above minimal ({})",
            sc.value,
            sm.value
        );
    }

    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/readme.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
