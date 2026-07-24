//! F3.13 — Changelog/Migration verifier (D39).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_changelog`] — a file-based detector of
//! the canonical "missing Keep-a-Changelog structural section" smell in a
//! project `CHANGELOG*` file:
//!
//! | Detector | Signal | Weight |
//! |----------|--------|--------|
//! | `missing-unreleased` | no `## [Unreleased]` section | 1.0 |
//! | `missing-versioned` | no `## [X.Y.Z] - YYYY-MM-DD` section | 1.0 |
//! | `missing-categories` | no `### Added` / `Changed` / `Fixed` / `Deprecated` / `Removed` / `Security` sub-heading | 0.5 |
//! | `missing-keep-a-changelog-link` | no `keepachangelog.com` URL in the file header | 0.5 |
//! | `missing-semver-link` | no `semver.org` URL in the file header | 0.5 |
//!
//! Disjoint from F3.12 doc accuracy (which keys on **drift** between code and
//! docs — F3.13 keys on the changelog's **structural conformance**).
//!
//! **Sources (context7, `/olivierlacan/keep-a-changelog`, High reputation,
//! bench 92.67)**: the canonical Keep-a-Changelog 2.0 format is a top-level
//! Changelog heading followed by an intro paragraph referencing
//! `keepachangelog.com` and `semver.org`. The body contains an `[Unreleased]`
//! section plus one or more versioned sections (`[X.Y.Z] - YYYY-MM-DD`), each
//! holding sub-headings for Added, Changed, Deprecated, Removed, Fixed, and
//! Security changes. Breaking changes are marked with a `Breaking` lead-in.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `## [Unreleased]` / `## [X.Y.Z]` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F3.13 verifier — Changelog/Migration.
#[allow(non_camel_case_types)]
pub struct F3_13_Changelog;

impl Verification for F3_13_Changelog {
    fn id(&self) -> DimId {
        DimId::F3_13
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_changelog_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: changelog format compliance ────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_changelog_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_changelog, score_changelog};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.13: detector own source (changelog needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let (raw, present) = crate::verifications::read_artifact_source(
        target,
        crate::verifications::ArtifactClass::Changelog,
    );
    if !present {
        return Ok(crate::verifications::absent_artifact_score(
            "F3.13",
            crate::verifications::ArtifactClass::Changelog,
        ));
    }
    let r = analyze_changelog(&raw, "markdown");
    let value = score_changelog(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F3.13: {} changelog gap(s) over {} lines — score={value:.3} \
         (touring-analysis analyze_changelog: missing [Unreleased] / missing versioned section / \
         missing ### Added-Changed-Fixed sub-heading / missing Keep-a-Changelog link / \
         missing SemVer link){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the changelog needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: section-density heuristic ──────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_changelog_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let has_unreleased = raw.contains("## [Unreleased]");
    let has_versioned =
        raw.contains("## [") && raw.contains("]") && !raw.contains("## [Unreleased]");
    let has_categories =
        raw.contains("### Added") || raw.contains("### Changed") || raw.contains("### Fixed");
    let required = (has_unreleased as u8 + has_versioned as u8 + has_categories as u8) as f32;
    let value = (required / 3.0).min(1.0);
    let evidence = format!(
        "{required:.0}/3 essential sections (Unreleased/versioned/category) present over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full changelog analysis)"
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
    fn test_changelog_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".md");
        let s = F3_13_Changelog.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_changelog_empty_file() {
        let f = write_temp("");
        let s = F3_13_Changelog.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Canonical CHANGELOG scores higher than minimal one-liner.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_canonical_scores_higher_than_minimal() {
        let canonical = "# Changelog\n\n[Keep a Changelog](https://keepachangelog.com/) / [SemVer](https://semver.org/)\n\n## [Unreleased]\n\n### Added\n- WIP\n\n## [1.0.0] - 2026-01-15\n\n### Added\n- First release\n";
        let minimal = "tiny";
        let f_canonical = write_temp_ext(canonical, ".md");
        let f_minimal = write_temp_ext(minimal, ".md");
        let sc = F3_13_Changelog.check(f_canonical.path()).expect("check");
        let sm = F3_13_Changelog.check(f_minimal.path()).expect("check");
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
            "/x/touring-analysis/src/quality/changelog.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
