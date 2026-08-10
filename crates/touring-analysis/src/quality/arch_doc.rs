//! Architecture documentation analysis (D36 / F3.10) — detector of the
//! canonical "missing architecture documentation" smell in a project
//! (ADR + Mermaid diagrams + C4 levels).
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | missing-madr-status | no `## Status` heading | `.md` (ADR file) |
//! | missing-madr-context | no `## Context` heading | `.md` (ADR file) |
//! | missing-madr-decision | no `## Decision` heading | `.md` (ADR file) |
//! | missing-madr-consequences | no `## Consequences` heading | `.md` (ADR file) |
//! | no-mermaid-diagram | no ` ```mermaid ` code block in the file | any |
//!
//! **Disjoint** from F3.8 inline doc (which keys on `///` proximity on
//! source items — F3.10 keys on ` ```mermaid ` blocks + MADR sections in
//! `.md`); F3.11 README completeness (which keys on the README's high-level
//! sections — F3.10 keys on ADR-specific MADR sections); F3.13 changelog
//! format (which keys on `[Unreleased]` + versioned sections — F3.10 keys
//! on Status/Context/Decision/Consequences MADR sections).
//!
//! **Sources (context7, `/mermaid-js/mermaid`, High reputation, bench 91.75)**:
//! Mermaid is the canonical architecture-diagram language for Markdown
//! (embedded as ` ```mermaid ` fenced code blocks — renderable in GitHub,
//! docs.rs, and any markdown viewer). The MADR template (context7 MADR
//! `madr.md`) prescribes four required sections — `## Status` (one of
//! Proposed/Accepted/Deprecated/Superseded), `## Context` (the problem and
//! forces), `## Decision` (the chosen option), `## Consequences` (positive
//! + negative trade-offs) — and optionally `## Alternatives` for rejected
//! options. ADRs without these sections can't be reasoned about later: the
//! "why" is the most expensive part to reconstruct from a code archaeology
//! dig.
//!
//! As with the README + CHANGELOG engines, `non_executable_regions` is
//! intentionally bypassed — Markdown has no code regions in the `code_regions`
//! sense, and treating `# ` as a line comment would filter out the entire
//! file as non-executable.

use memchr::memmem;

use super::code_regions::offset_suppressed;
use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier; arch doc is meta-documentation).
const SCALE: f32 = 6.0;

/// Mermaid fence (the canonical Mermaid-in-Markdown trigger).
const MERMAID_FENCE: &[u8] = b"```mermaid";

/// MADR section headings (lowercased text after the `## ` prefix, since
/// `h2_text_lower` strips the prefix). Match is `starts_with` so
/// "## Status: accepted" still satisfies the "Status" needle.
const MADR_SECTIONS: &[(&[u8], &str)] = &[
    (b"status", "Status"),
    (b"context", "Context"),
    (b"decision", "Decision"),
    (b"consequences", "Consequences"),
];

/// Findings of a single architecture-documentation analysis pass:
/// the canonical MADR-template structural conformance + Mermaid-diagram
/// presence rolled up per file.
pub type ArchDocReport = crate::quality::SmellReport;

/// `true` if `line` is a H2 heading (`## <text>` — first non-whitespace is
/// `##` followed by a space). The match is case-insensitive (we lowercase).
fn is_h2_heading(line: &[u8]) -> bool {
    let mut i = 0;
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    // Two `#`s followed by a space.
    i + 2 < line.len() && line[i] == b'#' && line[i + 1] == b'#' && line[i + 2] == b' '
}

/// Extract the heading text after the `## ` prefix, lowercased. Caller
/// has already verified this is an H2. Returns an owned `Vec<u8>` so the
/// caller can compare against the lowercased MADR section names without
/// dealing with lifetime / borrow issues (callers store these in a `Vec`).
fn h2_text_lower(line: &[u8]) -> Vec<u8> {
    let mut i = 0;
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    let rest = &line[i + 3..]; // skip `## `
    let end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
    let mut out = rest[..end].to_vec();
    out.make_ascii_lowercase();
    out
}

/// Analyze architecture documentation in `source` for the given language.
/// `lang` decides whether to check MADR-template sections: those only make
/// sense for Markdown files (ADRs / architecture overviews are `.md`). For
/// source-code files (`.rs` / `.py` / `.ts` etc.) we still count embedded
/// ` ```mermaid ` code blocks but skip the MADR section checks — a `.rs`
/// file legitimately has no `## Status` / `## Context` / `## Decision`
/// sections.
pub fn analyze_arch_doc(source: &str, lang: &str) -> ArchDocReport {
    let bytes = source.as_bytes();
    // Markdown has no code regions in the `code_regions` sense — bypass
    // (same rationale as the README + CHANGELOG engines: treating `#` as
    // a line comment would filter out the entire file).
    let regions: Vec<(usize, usize)> = Vec::new();
    let mut report = ArchDocReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    // First pass: collect H2 heading texts and mermaid-block count.
    let mut h2_sections: Vec<Vec<u8>> = Vec::new();
    let mut mermaid_blocks = 0usize;
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(bytes.len());
        let line = &bytes[line_start..line_end];
        if !offset_suppressed(line_start, &regions) {
            if is_h2_heading(line) {
                h2_sections.push(h2_text_lower(line));
            }
            // Mermaid fence: ` ```mermaid ` anywhere on the line (opening
            // fence). We count opening fences only — the closing fence is
            // a plain ` ``` ` and we don't double-count.
            if memmem::find(line, MERMAID_FENCE).is_some() {
                mermaid_blocks += 1;
            }
        }
        line_start = line_end + 1;
    }
    // MADR sections missing? Only check for Markdown files (ADRs / arch
    // overviews are `.md`). A `.rs` file legitimately has no MADR
    // sections — gating on `lang == "markdown"` avoids false-positive
    // MADR-missing smells on every source file in the workspace.
    if lang == "markdown" || lang == "md" {
        for (needle, label) in MADR_SECTIONS {
            let present = h2_sections.iter().any(|s| s.starts_with(needle));
            if !present {
                report.push(
                    match *label {
                        "Status" => "missing MADR ## Status section (Proposed/Accepted/Deprecated/Superseded)",
                        "Context" => "missing MADR ## Context section (problem + forces)",
                        "Decision" => "missing MADR ## Decision section (the chosen option)",
                        "Consequences" => "missing MADR ## Consequences section (positive + negative trade-offs)",
                        _ => "missing MADR section",
                    },
                    1,
                    0.5,
                );
            }
        }
    }
    // Mermaid diagrams — flagged when zero are present. We weight this
    // lower than a single missing MADR section because not every doc
    // benefits from a diagram; the MADR-template missing-section is
    // strictly worse than "no diagram yet".
    if mermaid_blocks == 0 {
        report.push(
            "no ```mermaid architecture diagram in file (consider C4 / sequence / flow)",
            1,
            0.3,
        );
    }
    let _ = regions; // reserved for future use (e.g., embedded code in ADR)
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score an [`ArchDocReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// + density cap so a 4-line stub ADR doesn't saturate the score to 0.
pub fn score_arch_doc(report: &ArchDocReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_adr() -> &'static str {
        "# ADR-001: Adopt proptest for property-based testing\n\n\
         ## Status\n\n\
         Accepted (2026-06-20).\n\n\
         ## Context\n\n\
         We have many example-based tests but no property coverage.\n\n\
         ## Decision\n\n\
         We will use proptest for new modules and the existing fuzz targets.\n\n\
         ## Consequences\n\n\
         Positive: better edge-case coverage. Negative: proptest book says\n\
         property tests complement, not replace, example tests — both needed.\n\n\
         ```mermaid\n\
         graph LR\n\
           A[Example tests] --> B[proptest]\n\
         ```\n"
    }

    #[test]
    fn canonical_adr_clean() {
        let r = analyze_arch_doc(canonical_adr(), "markdown");
        assert_eq!(r.violations, 0, "canonical ADR is clean: {:?}", r.findings);
    }

    #[test]
    fn missing_status_flagged() {
        let src = "# ADR\n\n## Context\nfoo\n\n## Decision\nbar\n\n## Consequences\nbaz\n";
        let r = analyze_arch_doc(src, "markdown");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("Status")),
            "missing Status is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn missing_all_madr_sections_flagged() {
        let r = analyze_arch_doc("# ADR\n\nJust a title, no sections.\n", "markdown");
        // 4 missing MADR sections + 1 missing mermaid = 5 findings.
        assert_eq!(
            r.violations, 5,
            "4 MADR sections + 1 mermaid smell: {:?}",
            r.findings
        );
    }

    #[test]
    fn case_insensitive_madr_match() {
        // "## status" (lowercase) should still match.
        let src = "# X\n\n## status\ns\n\n## context\nc\n\n## decision\nd\n\n## consequences\ncc\n\n```mermaid\ngraph TD\nA-->B\n```\n";
        let r = analyze_arch_doc(src, "markdown");
        assert_eq!(r.violations, 0, "lowercase MADR matches: {:?}", r.findings);
    }

    #[test]
    fn mermaid_diagram_recognized() {
        let src = "# X\n\n## Status\ns\n\n## Context\nc\n\n## Decision\nd\n\n## Consequences\ncc\n\n\
                  ```mermaid\ngraph TD\nA-->B\n```\n";
        let r = analyze_arch_doc(src, "markdown");
        assert_eq!(r.violations, 0, "mermaid present: {:?}", r.findings);
    }

    #[test]
    fn mermaid_in_source_counted() {
        // Source file (not .md) — only mermaid matters.
        let src = "/// Mermaid:\n/// ```mermaid\n/// graph TD\n///   A-->B\n/// ```\n";
        let r = analyze_arch_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "mermaid in source doc counts: {:?}",
            r.findings
        );
    }

    #[test]
    fn empty_doc_flagged() {
        let r = analyze_arch_doc("", "markdown");
        assert!(r.violations > 0, "empty doc is flagged: {:?}", r.findings);
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_arch_doc("# X\n\nJust a paragraph.\n", "markdown");
        let good = analyze_arch_doc(canonical_adr(), "markdown");
        assert!(
            score_arch_doc(&bad) < score_arch_doc(&good),
            "minimal ({:.3}) must score below canonical ({:.3})",
            score_arch_doc(&bad),
            score_arch_doc(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_arch_doc("# X\n", "markdown");
        let s = score_arch_doc(&r);
        assert!(
            s > 0.0,
            "stub ADR with multiple missing sections must not score 0.0: {s}"
        );
        assert!(s < 1.0, "must reflect missing-section penalty: {s}");
    }
}
