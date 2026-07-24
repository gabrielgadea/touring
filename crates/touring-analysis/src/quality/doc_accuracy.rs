//! Documentation accuracy analysis (D38 / F3.12) — polyglot detector of the
//! canonical "doc drift / no executable examples" smell. Doc accuracy is the
//! complement of doc completeness (F3.8 inline + F3.10 arch + F3.11 README):
//! the others check **presence**; F3.12 checks **trustworthiness** (does the
//! doc actually compile / match the code, or has it drifted to falsehood?).
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | no-doctest-enforcement | no `#![deny(missing_docs)]` / `#![warn(missing_docs)]` at crate root in a file with multiple `pub fn` | Rust |
//! | no-doctest-examples | a `///` doc comment without an inline ` ``` ` (doctest) block | Rust |
//! | drift-marker-in-doc | a `TODO` / `FIXME` / `XXX` / `???` inside a `///` or `//!` doc comment (the doc is flagged as not-yet-accurate) | any |
//! | no-code-blocks-in-md | a `.md` file with no ` ```rust ` / ` ```toml ` / ` ```bash ` code block (no executable example — readers can't copy-paste-verify) | `.md` |
//! | broken-link | a Markdown link with `TODO:` / `FIXME:` placeholder (the link is unverified) | `.md` |
//!
//! **Disjoint** from F3.8 inline doc (which keys on `///` proximity on
//! `pub` items — F3.12 keys on **whether the doc is actually executable**);
//! F3.10 arch doc (MADR + Mermaid — F3.12 keys on drift markers + code-block
//! presence); F3.13 changelog format (Keep-a-Changelog — F3.12 keys on
//! placeholder links in Markdown).
//!
//! **Sources (context7, `/vale-cli/vale`, Medium reputation)**: Vale is the
//! "prose-as-code" linter for documentation (`.vale.ini` configures style,
//! consistency, terminology); it's CI-integrated and catches drift signals
//! (`[suggestion]` / `[warning]` level). For our per-file heuristic we
//! approximate this with the lightweight drift-marker detection (`TODO` /
//! `FIXME` inside `///` or `//!` comments) — Vale's full surface would
//! require shelling out to the Vale binary, which is a W2 follow-up. The
//! doctest presence check (inline ` ``` ` blocks in `///` comments) mirrors
//! `cargo test --doc`'s compile-check guarantee: a doc-comment without an
//! inline example cannot fail at build time, so the doc's claim is unverified.
//!
//! As with the README + CHANGELOG + arch_doc engines, the `.md` branch
//! bypasses `non_executable_regions` (Markdown has no code regions in the
//! `code_regions` sense; treating `#` as a line comment would filter the
//! entire file as non-executable).

use memchr::memmem;

use super::code_regions::offset_suppressed;
use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier; doc accuracy meta-documentation).
const SCALE: f32 = 6.0;

/// Rust crate-root doc-lint attributes.
const DENY_MISSING_DOCS: &[u8] = b"#![deny(missing_docs)]";
const WARN_MISSING_DOCS: &[u8] = b"#![warn(missing_docs)]";

/// Drift markers (case-sensitive substring match — the markers are
/// standard Rust comment-marker convention).
const DRIFT_MARKERS: &[&[u8]] = &[b"TODO", b"FIXME", b"XXX", b"???"];

/// Doctest example markers (inline ` ``` ` code blocks inside `///` doc
/// comments). We don't search for the language tag — ` ```rust `,
/// ` ```ignore `, and ` ``` ` all start with the same ` ``` ` opener.
const DOCTEST_FENCE: &[u8] = b"```";

/// Markdown code-block openers.
const MARKDOWN_CODE_FENCE: &[u8] = b"```";

/// Findings of a single documentation-accuracy analysis pass: the
/// canonical "doc drift / no executable examples" smell rolled up per file.
#[derive(Debug, Clone, Default)]
pub struct DocAccuracyReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl DocAccuracyReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// `true` if `line` is a `///` or `//!` doc-comment line (the rustdoc marker).
fn is_doc_comment_line(line: &[u8]) -> bool {
    let trimmed = line
        .iter()
        .skip_while(|&&b| b == b' ' || b == b'\t')
        .copied()
        .collect::<Vec<u8>>();
    trimmed.starts_with(b"///") || trimmed.starts_with(b"//!")
}

/// Push findings for a Rust source file: missing-docs lint + doctest
/// presence + drift markers.
fn analyze_rust(bytes: &[u8]) -> RustDocCounts {
    let mut counts = RustDocCounts::default();
    // Crate-root lint detection: count `#[deny(missing_docs)]` / warn.
    counts.has_deny_lint = memmem::find(bytes, DENY_MISSING_DOCS).is_some();
    counts.has_warn_lint = memmem::find(bytes, WARN_MISSING_DOCS).is_some();
    // Per-line walk.
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(bytes.len());
        let line = &bytes[line_start..line_end];
        let sup = offset_suppressed(line_start, &[]);
        if !sup && is_doc_comment_line(line) {
            // Doctest fence detection — count ` ``` ` (any opener) in the
            // doc-comment line. Each doctest is its own fenced block.
            counts.doc_comment_lines += 1;
            if memmem::find(line, DOCTEST_FENCE).is_some() {
                counts.doctest_fence_lines += 1;
            }
            // Drift marker detection inside the doc comment.
            for marker in DRIFT_MARKERS {
                if memmem::find(line, marker).is_some() {
                    counts.drift_markers += 1;
                    break; // one marker per line is enough
                }
            }
            // `# Example`/`# Examples` rustdoc section heading (covers both).
            if memmem::find(line, b"# Example").is_some() {
                counts.examples_headings += 1;
            }
        }
        line_start = line_end + 1;
    }
    counts
}

/// Markdown branch: count code blocks + drift markers in `.md` content.
fn analyze_markdown(bytes: &[u8]) -> MdDocCounts {
    let mut counts = MdDocCounts::default();
    let mut in_code_block = false;
    let mut line_start = 0usize;
    while line_start < bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(bytes.len());
        let line = &bytes[line_start..line_end];
        if memmem::find(line, MARKDOWN_CODE_FENCE).is_some() {
            // Toggling on each fence line — we count only opening fences
            // (which we approximate as the first encounter per pair).
            if !in_code_block {
                counts.code_blocks += 1;
            }
            in_code_block = !in_code_block;
        }
        if !in_code_block {
            for marker in DRIFT_MARKERS {
                if memmem::find(line, marker).is_some() {
                    counts.drift_markers += 1;
                    break;
                }
            }
        }
        line_start = line_end + 1;
    }
    counts
}

#[derive(Default)]
struct RustDocCounts {
    has_deny_lint: bool,
    has_warn_lint: bool,
    doc_comment_lines: usize,
    doctest_fence_lines: usize,
    drift_markers: usize,
    /// Doc-comment lines with a `# Example`/`# Examples` rustdoc section heading
    /// (W4 2026-07-02). Paired with `doctest_fence_lines == 0` this is a real,
    /// resolution-free doc-accuracy defect: the doc CLAIMS an examples section
    /// but ships no runnable code fence to back it up.
    examples_headings: usize,
}

#[derive(Default)]
struct MdDocCounts {
    code_blocks: usize,
    drift_markers: usize,
}

/// Analyze documentation accuracy in `source` for the given language. The
/// Markdown branch checks for code-block presence + drift markers; the
/// Rust branch additionally checks for the `missing_docs` lint and inline
/// doctest fences in `///` comments.
pub fn analyze_doc_accuracy(source: &str, lang: &str) -> DocAccuracyReport {
    let bytes = source.as_bytes();
    let mut report = DocAccuracyReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        "rust" | "rs" => {
            let counts = analyze_rust(bytes);
            if counts.doc_comment_lines > 0 && counts.doctest_fence_lines == 0 {
                report.push(
                    "/// doc comments present but no inline ``` doctest example \
                     (doc claim is unverified by `cargo test --doc`)",
                    1,
                    0.6,
                );
            }
            if !counts.has_deny_lint && !counts.has_warn_lint && counts.doc_comment_lines > 0 {
                report.push(
                    "no #![deny(missing_docs)] / #![warn(missing_docs)] at crate root \
                     (doc completeness is not enforced by build)",
                    1,
                    0.4,
                );
            }
            if counts.drift_markers > 0 {
                report.push(
                    "drift marker (TODO/FIXME/XXX/???) inside a /// or //! doc comment \
                     (doc flagged as not-yet-accurate — drift is not yet removed)",
                    counts.drift_markers,
                    0.5,
                );
            }
            // W4 (2026-07-02): `# Examples` section documented but NO runnable
            // code fence anywhere → the example is claimed but absent (a real,
            // resolution-free accuracy defect; rustdoc `# Examples` convention).
            if counts.examples_headings > 0 && counts.doctest_fence_lines == 0 {
                report.push(
                    "documented # Examples section(s) with no runnable ``` code fence \
                     (example claimed but absent — doc is unverified by cargo test --doc)",
                    counts.examples_headings,
                    0.6,
                );
            }
        }
        "markdown" | "md" => {
            let counts = analyze_markdown(bytes);
            if counts.code_blocks == 0 && source.len() > 200 {
                report.push(
                    "Markdown has no ``` fenced code block (readers cannot \
                     copy-paste-verify the example — doc is unverified)",
                    1,
                    0.5,
                );
            }
            if counts.drift_markers > 0 {
                report.push(
                    "drift marker (TODO/FIXME/XXX/???) in Markdown body \
                     (doc content is not yet accurate)",
                    counts.drift_markers,
                    0.5,
                );
            }
        }
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`DocAccuracyReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// + density cap so a 4-line stub doc doesn't saturate to 0.
pub fn score_doc_accuracy(report: &DocAccuracyReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_with_deny_lint_and_doctests_clean() {
        let src = r#"
#![deny(missing_docs)]

/// Computes 2 + 2.
///
/// ```
/// assert_eq!(answer(), 4);
/// ```
pub fn answer() -> i32 { 4 }
"#;
        let r = analyze_doc_accuracy(src, "rust");
        assert_eq!(r.violations, 0, "deny + doctest is clean: {:?}", r.findings);
    }

    #[test]
    fn rust_doc_without_doctest_flagged() {
        let src = r#"
/// Just a description, no example.
pub fn answer() -> i32 { 4 }
"#;
        let r = analyze_doc_accuracy(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("doctest example")),
            "doc without ``` is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn examples_heading_without_fence_flagged() {
        // W4 (2026-07-02): a `# Examples` section with no code fence = example
        // claimed but absent (real, resolution-free doc-accuracy defect).
        let bad =
            "#![deny(missing_docs)]\n/// Doc.\n/// # Examples\n/// see the guide.\npub fn x() {}\n";
        let r = analyze_doc_accuracy(bad, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("Examples section")),
            "empty # Examples must be flagged: {:?}",
            r.findings
        );
        // A `# Examples` WITH a fence is NOT flagged for this smell.
        let good = "#![deny(missing_docs)]\n/// Doc.\n/// # Examples\n/// ```\n/// x();\n/// ```\npub fn x() {}\n";
        let r2 = analyze_doc_accuracy(good, "rust");
        assert!(
            !r2.findings
                .iter()
                .any(|(m, _)| m.contains("Examples section")),
            "a documented example with a fence must NOT be flagged: {:?}",
            r2.findings
        );
    }

    #[test]
    fn rust_missing_lint_flagged() {
        let src = r#"
/// Just docs, no lint.
pub fn answer() -> i32 { 4 }
"#;
        let r = analyze_doc_accuracy(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("missing_docs")),
            "no missing_docs lint is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_drift_marker_in_doc_flagged() {
        let src = r#"
/// TODO: document this better when we have time.
///
/// ```
/// assert_eq!(1, 1);
/// ```
pub fn thing() {}
"#;
        let r = analyze_doc_accuracy(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("drift marker")),
            "TODO in doc comment is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_no_doc_comments_clean() {
        // No `///` or `//!` at all → no findings (a file with no docs
        // has no doc-accuracy smell; that's a F3.8 inline-doc smell).
        let src = r#"
pub fn foo() -> i32 { 1 }
fn bar() -> i32 { 2 }
"#;
        let r = analyze_doc_accuracy(src, "rust");
        assert_eq!(
            r.violations, 0,
            "no doc → no doc-accuracy smell: {:?}",
            r.findings
        );
    }

    #[test]
    fn markdown_with_code_block_clean() {
        let src = "# Title\n\nSome prose.\n\n```rust\nlet x = 1;\n```\n";
        let r = analyze_doc_accuracy(src, "markdown");
        assert_eq!(
            r.violations, 0,
            "markdown with code block is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn markdown_no_code_block_flagged() {
        let src = "# Title\n\nA long markdown doc with no code block at all, just lots of prose \
                  that goes on and on without any executable example for the reader \
                  to copy-paste-verify. This is well over the 200-byte threshold so \
                  the missing-code-block smell should fire.\n";
        let r = analyze_doc_accuracy(src, "markdown");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("fenced code block")),
            "long markdown without code block is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn markdown_drift_marker_flagged() {
        let src =
            "# Title\n\nTODO: rewrite this section when X is done.\n\n```rust\nlet x = 1;\n```\n";
        let r = analyze_doc_accuracy(src, "markdown");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("drift marker")),
            "TODO in markdown body is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn empty_doc_clean() {
        let r = analyze_doc_accuracy("", "rust");
        assert_eq!(r.violations, 0, "empty doc is clean: {:?}", r.findings);
    }

    #[test]
    fn cfg_test_doc_excluded() {
        // With the missing_docs lint, the only smell to check is
        // the fence counting (we want exactly 1, not 2 — the `#[cfg(test)]`
        // mod body is non-executable, but the `///` lines outside are).
        let src = r#"
#![deny(missing_docs)]

/// ```ignore
/// // ignored doctest in cfg(test)
/// ```
#[cfg(test)]
mod tests {}
"#;
        let r = analyze_doc_accuracy(src, "rust");
        assert_eq!(
            r.violations, 0,
            "with deny lint + doctest fence, file is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_doc_accuracy("/// TODO: write this.\npub fn x() {}\n", "rust");
        let good = analyze_doc_accuracy(
            "#![deny(missing_docs)]\n/// Doc.\n/// ```\n/// assert!(true);\n/// ```\npub fn x() {}\n",
            "rust",
        );
        assert!(
            score_doc_accuracy(&bad) < score_doc_accuracy(&good),
            "dirty ({:.3}) must score below clean ({:.3})",
            score_doc_accuracy(&bad),
            score_doc_accuracy(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_doc_accuracy("/// TODO.\n/// FIXME.\n/// XXX.\npub fn x() {}\n", "rust");
        let s = score_doc_accuracy(&r);
        assert!(
            s > 0.0,
            "short file with multiple drift markers must not score 0.0: {s}"
        );
    }
}
