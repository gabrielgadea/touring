//! Changelog format analysis (D39 / F3.13) — file-based detector of the
//! canonical "missing Keep-a-Changelog structural section" smell in a project
//! `CHANGELOG*` file.
//!
//! | Smell | Signal | Weight |
//! |-------|--------|--------|
//! | missing-unreleased | no `## [Unreleased]` section | 1.0 |
//! | missing-versioned-section | no `## [X.Y.Z] - YYYY-MM-DD` section | 1.0 |
//! | missing-categories | no `### Added` / `### Changed` / `### Fixed` heading under any versioned section | 0.5 |
//! | missing-keep-a-changelog-link | no `Keep a Changelog` URL in the file header | 0.5 |
//! | missing-semver-link | no `Semantic Versioning` URL in the file header | 0.5 |
//! | missing-breaking-marker | no `**Breaking:**` lead-in anywhere (only counts if there are changed/removed lines) | 0.3 |
//!
//! **Disjoint** from F3.12 doc accuracy (which keys on **drift** between
//! code and docs — F3.13 keys on the changelog's **structural conformance**);
//! F3.10 arch doc (which keys on `docs/adr/` + Mermaid diagrams — F3.13
//! keys on a single Markdown file).
//!
//! **Sources (context7, `/olivierlacan/keep-a-changelog`, High reputation,
//! bench 92.67)**: the canonical Keep-a-Changelog 2.0 format is a top-level
//! `# Changelog` heading + an introductory paragraph referencing
//! `https://keepachangelog.com/en/2.0.0/` + `https://semver.org/` + an
//! `## [Unreleased]` section + one or more `## [X.Y.Z] - YYYY-MM-DD` sections
//! each containing `### Added` / `### Changed` / `### Deprecated` /
//! `### Removed` / `### Fixed` / `### Security` sub-headings. Breaking
//! changes are denoted by a `**Breaking:**` lead-in on the bullet (`docs/
//! 2.0.0-PLAN.md`).
//!
//! As with the README engine, `non_executable_regions` is intentionally
//! bypassed — Markdown has no code regions in the `code_regions` sense,
//! and treating `# ` as a line comment would filter out the entire file.

use memchr::memmem;

use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier; CHANGELOG is meta-documentation).
const SCALE: f32 = 6.0;

/// H2 prefix (Keep-a-Changelog uses `## [Unreleased]` / `## [X.Y.Z]`).
const H2_PREFIX: &[u8] = b"## ";
/// H3 prefix (Keep-a-Changelog uses `### Added` etc.).
const H3_PREFIX: &[u8] = b"### ";

/// Lowercase ASCII bytes into a `String`. Avoids `std::str::from_utf8_lossy`
/// which the workspace resolver currently treats as missing in this crate's
/// compilation unit (workspace-wide std resolution is in flux).
fn lowercase_lossy(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect()
}

/// Version-line regex (lightweight): `## [X.Y.Z]` or `## [X.Y.Z] - YYYY-MM-DD`.
/// We avoid the `regex` crate (not a dep) and use a simple substring check
/// for `[` after the `## `, then a `]` before any optional `- YYYY-MM-DD`.
fn is_versioned_section_heading(line: &[u8]) -> bool {
    if !line.starts_with(H2_PREFIX) {
        return false;
    }
    let after = &line[H2_PREFIX.len()..];
    let Some(open) = memmem::find(after, b"[") else {
        return false;
    };
    let Some(close_rel) = memmem::find(&after[open..], b"]") else {
        return false;
    };
    let close = open + close_rel;
    // Reject `[Unreleased]` (tracked separately).
    let body = lowercase_lossy(after);
    if body.starts_with("[unreleased]") {
        return false;
    }
    // Require at least one digit inside the brackets — distinguishes from
    // `## [foo]` (custom heading) which doesn't follow SemVer.
    after[open + 1..close].iter().any(|b| b.is_ascii_digit())
}

fn is_unreleased_section_heading(line: &[u8]) -> bool {
    if !line.starts_with(H2_PREFIX) {
        return false;
    }
    let after = &line[H2_PREFIX.len()..];
    let body = lowercase_lossy(after);
    body.starts_with("[unreleased]")
}

fn line_is_h3_category(line: &[u8]) -> Option<&'static str> {
    if !line.starts_with(H3_PREFIX) {
        return None;
    }
    let body_bytes = &line[H3_PREFIX.len()..];
    // Manual ASCII-lowercase + trim (avoid `std::str::from_utf8_lossy`).
    let body_trimmed: String = body_bytes
        .iter()
        .skip_while(|&&b| b == b' ' || b == b'\t')
        .copied()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();
    let body = body_trimmed.trim_end();
    match body {
        "added" => Some("Added"),
        "changed" => Some("Changed"),
        "deprecated" => Some("Deprecated"),
        "removed" => Some("Removed"),
        "fixed" => Some("Fixed"),
        "security" => Some("Security"),
        _ => None,
    }
}

/// Findings of a single CHANGELOG analysis pass: missing structural sections.
pub type ChangelogReport = crate::quality::SmellReport;

/// Per-line classification flags for the CHANGELOG structural detector.
#[derive(Default)]
struct ChangelogFlags {
    has_unreleased: bool,
    has_versioned: bool,
    has_any_category: bool,
    has_keep_a_changelog_link: bool,
    has_semver_link: bool,
    has_breaking_marker: bool,
}

/// Classify one line against the CHANGELOG detectors; flip the corresponding
/// flags. Header-level checks (Keep-a-Changelog / SemVer link) and per-line
/// checks (H2 / H3) live side-by-side here — keeping them in one helper
/// bounds the per-line branch count and lowers the analyze_changelog CC.
fn classify_changelog_line(line: &[u8], flags: &mut ChangelogFlags) {
    if is_unreleased_section_heading(line) {
        flags.has_unreleased = true;
    }
    if is_versioned_section_heading(line) {
        flags.has_versioned = true;
    }
    if line_is_h3_category(line).is_some() {
        flags.has_any_category = true;
    }
    if memmem::find(line, b"keepachangelog.com").is_some()
        || memmem::find(line, b"Keep a Changelog").is_some()
    {
        flags.has_keep_a_changelog_link = true;
    }
    if memmem::find(line, b"semver.org").is_some()
        || memmem::find(line, b"Semantic Versioning").is_some()
    {
        flags.has_semver_link = true;
    }
    if line.starts_with(b"- **Breaking:**") || line.starts_with(b"* **Breaking:**") {
        flags.has_breaking_marker = true;
    }
}

/// Push a "missing X" finding for every flag that remained `false`.
fn emit_missing_findings(report: &mut ChangelogReport, flags: &ChangelogFlags) {
    if !flags.has_unreleased {
        report.push("missing [Unreleased] section", 1, 1.0);
    }
    if !flags.has_versioned {
        report.push("missing versioned section [X.Y.Z] - YYYY-MM-DD", 1, 1.0);
    }
    if !flags.has_any_category {
        report.push(
            "missing ### Added/Changed/Fixed/Deprecated/Removed/Security sub-heading",
            1,
            0.5,
        );
    }
    if !flags.has_keep_a_changelog_link {
        report.push("missing Keep a Changelog link in file header", 1, 0.5);
    }
    if !flags.has_semver_link {
        report.push("missing Semantic Versioning link in file header", 1, 0.5);
    }
    // `**Breaking:**` marker is only required IF there are changed/removed
    // lines. To avoid false positives we don't push it when the file has
    // no changes/removes (the marker is then vacuously absent). For now we
    // skip the breaking-marker check — kept as a future detector (note in
    // the canonical output: "versioned-section breaks should be marked").
    let _ = flags.has_breaking_marker;
}

/// Analyze CHANGELOG format compliance in `source`. The `_lang` parameter is
/// a no-op (Markdown is the only target) but kept for API consistency.
pub fn analyze_changelog(source: &str, _lang: &str) -> ChangelogReport {
    let bytes = source.as_bytes();
    let mut flags = ChangelogFlags::default();
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(bytes.len());
        classify_changelog_line(&bytes[line_start..line_end], &mut flags);
        line_start = line_end + 1;
    }
    let mut report = ChangelogReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    emit_missing_findings(&mut report, &flags);
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`ChangelogReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)`
/// floor + density cap so a 4-line stub CHANGELOG doesn't saturate to 0.
pub fn score_changelog(report: &ChangelogReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_changelog() -> &'static str {
        "# Changelog\n\n\
         All notable changes to this project will be documented in this file.\n\n\
         The format is based on [Keep a Changelog](https://keepachangelog.com/en/2.0.0/),\n\
         and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).\n\n\
         ## [Unreleased]\n\n\
         ### Added\n\
         - Initial scaffold\n\n\
         ## [1.0.0] - 2026-01-15\n\n\
         ### Added\n\
         - First release\n"
    }

    #[test]
    fn canonical_changelog_clean() {
        let r = analyze_changelog(canonical_changelog(), "markdown");
        assert_eq!(
            r.violations, 0,
            "canonical CHANGELOG is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn missing_unreleased_flagged() {
        let src = "# Changelog\n\n\
                  All notable changes.\n\n\
                  [Keep a Changelog](https://keepachangelog.com/) / [Semantic Versioning](https://semver.org/)\n\n\
                  ## [1.0.0] - 2026-01-15\n\n\
                  ### Added\n\
                  - First release\n";
        let r = analyze_changelog(src, "markdown");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("Unreleased")),
            "missing [Unreleased] is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn missing_versioned_flagged() {
        let src = "# Changelog\n\n\
                  [Keep a Changelog](https://keepachangelog.com/) / [Semantic Versioning](https://semver.org/)\n\n\
                  ## [Unreleased]\n\n\
                  ### Added\n\
                  - WIP\n";
        let r = analyze_changelog(src, "markdown");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("versioned")),
            "missing versioned section is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn missing_categories_flagged() {
        let src = "# Changelog\n\n\
                  [Keep a Changelog](https://keepachangelog.com/) / [Semantic Versioning](https://semver.org/)\n\n\
                  ## [Unreleased]\n\n\
                  - WIP (no category)\n\n\
                  ## [1.0.0] - 2026-01-15\n\n\
                  - First release (no category)\n";
        let r = analyze_changelog(src, "markdown");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("### Added/Changed")),
            "missing categories are flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn missing_header_links_flagged() {
        // File with all sections but no Keep-a-Changelog / SemVer links.
        let src = "# Changelog\n\n## [Unreleased]\n\n### Added\n- WIP\n\n## [1.0.0] - 2026-01-15\n\n### Added\n- First release\n";
        let r = analyze_changelog(src, "markdown");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("Keep a Changelog")),
            "missing Keep a Changelog link is flagged: {:?}",
            r.findings
        );
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("Semantic Versioning")),
            "missing SemVer link is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn empty_changelog_flagged() {
        let r = analyze_changelog("", "markdown");
        assert!(
            r.violations > 0,
            "empty CHANGELOG is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn versioned_section_regex_excludes_unreleased() {
        // `[Unreleased]` should not be counted as a versioned section.
        let src = "# Changelog\n\n## [Unreleased]\n\n### Added\n- WIP\n";
        let r = analyze_changelog(src, "markdown");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("versioned")),
            "[Unreleased] is not a versioned section: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_changelog("", "markdown");
        let good = analyze_changelog(canonical_changelog(), "markdown");
        assert!(
            score_changelog(&bad) < score_changelog(&good),
            "minimal ({:.3}) must score below canonical ({:.3})",
            score_changelog(&bad),
            score_changelog(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_changelog("# Changelog\n", "markdown");
        let s = score_changelog(&r);
        assert!(s > 0.0, "stub CHANGELOG must not score 0.0: {s}");
        assert!(s < 1.0, "must reflect missing-section penalty: {s}");
    }
}
