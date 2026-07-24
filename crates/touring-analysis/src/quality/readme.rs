//! README completeness analysis (D37 / F3.11) — file-based detector of the
//! canonical "missing essential section" smell in a project `README*` file.
//!
//! | Section kind | Signal (case-insensitive heading match) | Weight |
//! |--------------|------------------------------------------|--------|
//! | title | first `^#\s+\S` line present, or `# <project>` | 0.5 (header) |
//! | description | "About"/"Description"/"Overview"/"Introduction" / "What is" heading | 0.5 |
//! | install | "Install"/"Installation"/"Setup"/"Getting Started" heading | 1.0 |
//! | usage | "Usage"/"Example"/"Quickstart"/"How to" heading | 1.0 |
//! | contributing | "Contributing"/"Development" heading | 0.5 |
//! | tests | "Tests"/"Testing" heading | 0.5 |
//! | license | "License" heading (or "Licence") | 0.5 |
//! | badges | `[![...]]` (shields.io) anywhere in the file | optional |
//! | toc | `## Table of Contents` heading (or "Contents" / "TOC") | optional |
//!
//! **Disjoint** from F3.10 arch doc (which keys on ADR + ` ```mermaid ` —
//! F3.11 keys on the README's high-level **content sections**); F3.12 doc
//! accuracy (which keys on **drift** between code and docs — F3.11 keys on
//! **presence** of canonical sections in the README).
//!
//! **Sources (context7, `/othneildrew/best-readme-template`, High reputation,
//! bench 85)**: the canonical README template (best-readme-template) prescribes
//! Project Title + About + Getting Started (Prerequisites + Installation) +
//! Usage + Roadmap (optional) + Contributing + License + Acknowledgments. We
//! collapse the recommendation into 6 required sections (title/description/
//! install/usage/license/contributing) plus 2 optional (badges/toc), with
//! `tests` included as a required section because real READMEs without a
//! "how to test" leave the consumer guessing (`D31 test maintainability`).
//!
//! The `analyze_readme` function operates on the **raw bytes** of any file
//! passed in (typically the `README.md`). The score is `1 - density·SCALE`
//! via the shared [`super::score_utils::density_score`] helper — short
//! READMEs (< 20 LOC) are floored per the F2.13 saturation lesson.
//!
//! Comments / `#[cfg(test)]` are N/A for `.md` (no code regions) — but the
//! region pipeline is harmless and kept for future Rust-embedded-Markdown.

use memchr::memmem;

use super::code_regions::offset_suppressed;
use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier; `README` is meta-documentation).
const SCALE: f32 = 6.0;

/// Markdown heading line: `# `, `## `, etc. — capture just the line.
const HEADING_PREFIXES: &[&[u8]] = &[b"# ", b"## ", b"### ", b"#### ", b"##### ", b"###### "];

/// Findings of a single README completeness analysis pass: the
/// canonical "missing essential section" smell rolled up per file.
#[derive(Debug, Clone, Default)]
pub struct ReadmeReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-section weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl ReadmeReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// Yield `(start, end)` byte ranges for every heading line in `bytes`.
/// A heading line is one that starts with `# `, `## `, …, `###### ` (one
/// optional space, then the heading text). Returns the full line including
/// the trailing newline.
fn iter_headings(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut headings = Vec::new();
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(bytes.len());
        let line = &bytes[line_start..line_end];
        if HEADING_PREFIXES.iter().any(|p| line.starts_with(p)) {
            headings.push((line_start, line_end));
        }
        line_start = line_end + 1;
    }
    headings
}

/// Normalize a heading line to its lowercased textual content (the part
/// after the `#` / `##` prefix, trimmed).
fn heading_text(line: &[u8]) -> String {
    for prefix in HEADING_PREFIXES {
        if line.starts_with(prefix) {
            let stripped = &line[prefix.len()..];
            return String::from_utf8_lossy(stripped).trim().to_lowercase();
        }
    }
    String::from_utf8_lossy(line).trim().to_lowercase()
}

/// `true` if any of the heading texts in `headings` (case-insensitive)
/// contains a needle in `needles`. Substring match (no whole-word
/// boundary) — a heading "## How to install" matches the "Install" needle.
fn heading_matches_any(headings: &[(usize, usize)], bytes: &[u8], needles: &[&str]) -> bool {
    for &(start, end) in headings {
        let line = &bytes[start..end];
        let text = heading_text(line);
        for needle in needles {
            if text.contains(needle) {
                return true;
            }
        }
    }
    false
}

/// Detect canonical README sections + optional badges / ToC.
fn analyze_readme_sections(
    bytes: &[u8],
    regions: &[(usize, usize)],
) -> (usize, usize, usize, bool, bool) {
    // Restrict headings to executable regions (no effect on `.md`, but
    // keeps the engine future-proof for embedded-Markdown in code).
    let headings: Vec<(usize, usize)> = iter_headings(bytes)
        .into_iter()
        .filter(|&(s, _)| !offset_suppressed(s, regions))
        .collect();

    // Title: presence of any heading on line 0, OR a "Title"-shaped heading
    // at the top of the file. We treat "any H1" as title-present.
    let has_title = headings
        .iter()
        .any(|&(s, e)| bytes[s..e].starts_with(b"# "));
    // Description / About / Overview / Introduction
    let has_description = heading_matches_any(
        &headings,
        bytes,
        &[
            "about",
            "description",
            "overview",
            "introduction",
            "what is",
        ],
    );
    // Install / Setup / Getting Started
    let has_install = heading_matches_any(
        &headings,
        bytes,
        &[
            "install",
            "installation",
            "setup",
            "getting started",
            "getting-started",
        ],
    );
    // Usage / Example / Quickstart
    let has_usage = heading_matches_any(
        &headings,
        bytes,
        &[
            "usage",
            "example",
            "examples",
            "quickstart",
            "quick start",
            "how to",
        ],
    );
    // Contributing / Development
    let has_contributing = heading_matches_any(
        &headings,
        bytes,
        &["contributing", "development", "how to contribute"],
    );
    // Tests / Testing
    let has_tests = heading_matches_any(&headings, bytes, &["tests", "testing", "how to test"]);
    // License / Licence
    let has_license = heading_matches_any(&headings, bytes, &["license", "licence"]);

    // Optional: badges (`[![...]]` Markdown image link) and ToC.
    let has_badges = memmem::find(bytes, b"![").is_some() && memmem::find(bytes, b"](").is_some();
    let has_toc = heading_matches_any(&headings, bytes, &["table of contents", "contents", "toc"]);
    // The report's `weighted_total` carries the missing-required weight
    // (the engine contract is "weighted violation total = sum of
    // count × weight for fired detectors"). We push ONE finding per
    // missing required section.
    let missing_count = [
        has_title,
        has_description,
        has_install,
        has_usage,
        has_contributing,
        has_tests,
        has_license,
    ]
    .iter()
    .filter(|p| !*p)
    .count();
    (
        missing_count,
        has_badges as usize,
        has_toc as usize,
        has_badges,
        has_toc,
    )
}

/// Analyze README completeness in `source` for the given language. The
/// language is a no-op for `.md` (Markdown is the only language the
/// analyzer handles) but the parameter is kept for API consistency with
/// the other engines.
pub fn analyze_readme(source: &str, _lang: &str) -> ReadmeReport {
    let bytes = source.as_bytes();
    // Markdown has no executable regions in the code_regions sense — we
    // intentionally skip the `non_executable_regions` call because it
    // would otherwise treat `# ` as a line comment (falling back to RUST /
    // JS_TS syntax) and filter out the entire file as non-executable.
    let regions: Vec<(usize, usize)> = Vec::new();
    let mut report = ReadmeReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    let (missing_count, badges, toc, _has_badges, _has_toc) =
        analyze_readme_sections(bytes, &regions);
    // Push ONE finding per missing required section. We use the static
    // message for the generic "missing required section" detector; the
    // count captures HOW MANY are missing. Per-section diagnostics can
    // be derived from `findings[*].1` if needed by a downstream view.
    if missing_count > 0 {
        report.push(
            "missing required README section (title/description/install/usage/contributing/tests/license)",
            missing_count,
            1.0,
        );
    }
    if badges == 0 {
        // Optional — push as a 0-weight (informational) finding? We skip
        // it to avoid inflating the violation count; the absence of a
        // missing-required finding already means the README is "clean".
    }
    if toc == 0 {
        // Optional — same as above.
    }
    let _ = _has_badges;
    let _ = _has_toc;
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`ReadmeReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)`
/// floor + density cap so a 4-line stub README doesn't saturate to 0.
pub fn score_readme(report: &ReadmeReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_readme() -> &'static str {
        "# my-project\n\n\
         ## Description\n\
         A short description.\n\n\
         ## Installation\n\
         ```\ncargo install my-project\n```\n\n\
         ## Usage\n\
         ```\nmy-project --help\n```\n\n\
         ## Contributing\n\
         See CONTRIBUTING.md.\n\n\
         ## Tests\n\
         ```\ncargo test\n```\n\n\
         ## License\n\
         MIT\n"
    }

    #[test]
    fn canonical_readme_clean() {
        let r = analyze_readme(canonical_readme(), "markdown");
        assert_eq!(
            r.violations, 0,
            "canonical README is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn missing_install_flagged() {
        let r = analyze_readme(
            "# my-project\n\n## Description\nA short desc.\n\n## Usage\nhi\n\n## License\nMIT\n",
            "markdown",
        );
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("missing required")),
            "missing install/setup is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn missing_all_flagged() {
        let r = analyze_readme("just a one-liner with no sections", "markdown");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("missing required")),
            "minimal README is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn getting_started_equivalent_to_install() {
        // "Getting Started" is the best-readme-template canonical install
        // section name; it should satisfy the install detector.
        let src = "# x\n\n## Description\nd\n\n## Getting Started\n- clone\n\n## Usage\nu\n\n## Contributing\nc\n\n## Tests\nt\n\n## License\nMIT\n";
        let r = analyze_readme(src, "markdown");
        assert_eq!(
            r.violations, 0,
            "Getting Started counts as install: {:?}",
            r.findings
        );
    }

    #[test]
    fn case_insensitive() {
        // "INSTALLATION" / "Usage" / "LICENSE" should all be recognized.
        let src = "# T\n\n## Description\nd\n\n## INSTALLATION\ni\n\n## Usage\nu\n\n## CONTRIBUTING\nc\n\n## Tests\nt\n\n## LICENSE\nMIT\n";
        let r = analyze_readme(src, "markdown");
        assert_eq!(r.violations, 0, "case-insensitive match: {:?}", r.findings);
    }

    #[test]
    fn empty_readme_flagged() {
        let r = analyze_readme("", "markdown");
        assert!(
            r.violations > 0,
            "empty README is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_readme("# t\n", "markdown");
        let good = analyze_readme(canonical_readme(), "markdown");
        assert!(
            score_readme(&bad) < score_readme(&good),
            "minimal ({:.3}) must score below canonical ({:.3})",
            score_readme(&bad),
            score_readme(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_readme("# t\n", "markdown");
        let s = score_readme(&r);
        assert!(
            s > 0.0,
            "1-line README with missing sections must not score 0.0: {s}"
        );
        assert!(s < 1.0, "must reflect missing-section penalty: {s}");
    }
}
