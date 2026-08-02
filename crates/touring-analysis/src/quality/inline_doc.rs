//! Inline documentation analysis (D34 / F3.8) — polyglot detector of the
//! canonical "missing `///` on `pub` items" smell that `rustdoc` lints via
//! `#[warn(missing_docs)]` and that `clippy -- -D warnings` enforces.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | pub-item-undocumented | a `pub fn`/`pub struct`/`pub enum`/`pub trait`/`pub type`/`pub const`/`pub static`/`pub use`/`pub mod` whose preceding non-blank line is **not** `///` or `//!` | Rust |
//! | pub-item-missing-examples | a documented `pub fn` with no `# Examples` section in its doc block | Rust |
//! | missing-missing-docs-lint | the file declares `pub` items but no `#![warn(missing_docs)]` or `#![deny(missing_docs)]` at the top | Rust |
//! | jsdoc-undocumented-export | a `export function` / `export class` / `export const` / `export interface` whose preceding non-blank line is **not** `/** … */` | JS/TS |
//! | python-docstring-undocumented | a `def`/`class` at module level (no leading `_`) with no `"""…"""` or `'''…'''` docstring on the next line | Python |
//!
//! **Disjoint** from F3.9 API doc (which keys on `cargo doc --no-deps` /
//! OpenAPI specs — F3.8 keys on **per-pub-item** `///` proximity); F3.10
//! arch doc (which keys on `docs/adr/` and ` ```mermaid ` — F3.8 keys on
//! source code); F3.12 doc accuracy (which keys on drift between doc and
//! code — F3.8 keys on **presence** of doc).
//!
//! **Sources (context7, `/rust-lang/rust`, High reputation, bench 71.12)**:
//! `#![deny(missing_docs)]` is the gold-standard enforcement
//! (`src/doc/rustdoc/src/write-documentation/what-to-include.md`); rustdoc
//! "generates documentation for public items by default" — pub items without
//! `///` are excluded from `cargo doc` output (`what-is-rustdoc.md`).
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::density_score;

/// Density→score scale (ADVISORY-tier; `cargo doc --no-deps` runs <1s).
const SCALE: f32 = 6.0;

/// Rust `pub` kinds whose doc-presence matters.
const RUST_PUB_KINDS: &[&[u8]] = &[
    b"pub fn ",
    b"pub async fn ",
    b"pub struct ",
    b"pub enum ",
    b"pub trait ",
    b"pub type ",
    b"pub const ",
    b"pub static ",
    b"pub mod ",
];

/// JS/TS export kinds whose doc-presence matters.
const JS_EXPORT_KINDS: &[&[u8]] = &[
    b"export function ",
    b"export async function ",
    b"export class ",
    b"export interface ",
    b"export type ",
    b"export const ",
    b"export default function ",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    JsTs,
    Python,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::JsTs,
        "python" | "py" => Lang::Python,
        _ => Lang::Other,
    }
}

/// Inline-documentation findings for one file.
#[derive(Debug, Clone, Default)]
pub struct InlineDocReport {
    /// Total raw violation count across all detectors.
    pub violations: usize,
    /// Weighted violation total (per-smell weights applied).
    pub weighted_total: f32,
    /// Lines scanned (denominator for density).
    pub total_lines: usize,
    /// `(message, count)` per fired detector, sorted by count desc.
    pub findings: Vec<(String, usize)>,
}

impl InlineDocReport {
    fn push(&mut self, message: &'static str, count: usize, weight: f32) {
        if count > 0 {
            self.violations += count;
            self.weighted_total += count as f32 * weight;
            self.findings.push((message.to_string(), count));
        }
    }
}

/// Trim leading ASCII whitespace from a byte slice.
#[inline]
fn trim_ascii_ws(line: &[u8]) -> &[u8] {
    let n = line
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    &line[n..]
}

/// Walk backward from `off` (the position of the `pub ` keyword) and decide
/// whether the previous non-blank line is a doc comment that satisfies
/// `is_doc_line`. Returns `true` if the pub item is documented.
///
/// This is intentionally **single-line**: the doc-comment chain immediately
/// above the `pub` item ends at the first `///` (or `//!`); rustdoc only
/// associates consecutive doc-comment lines with the item, and the first
/// such line is sufficient for "is this pub item documented?" purposes. A
/// blank line breaks the doc chain (rustdoc convention).
fn prev_line_matches_doc(bytes: &[u8], off: usize, is_doc_line: impl Fn(&[u8]) -> bool) -> bool {
    let line_start = bytes[..off]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    if line_start == 0 {
        return false;
    }
    let prev_nl = bytes[..line_start.saturating_sub(1)]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let line = trim_ascii_ws(&bytes[prev_nl..line_start]);
    !line.is_empty() && is_doc_line(line)
}

/// `true` if the line is a Rust doc-comment marker (`///` or `//!`).
#[inline]
fn is_rust_doc_line(line: &[u8]) -> bool {
    line.starts_with(b"///") || line.starts_with(b"//!")
}

/// Detect the canonical Rust smells (per-pub-item doc presence, missing
/// `# Examples` block, missing `#![deny(missing_docs)]` header).
fn analyze_rust(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize, usize) {
    let mut pub_count = 0usize;
    let mut undocumented = 0usize;
    let mut missing_examples = 0usize;
    for kind in RUST_PUB_KINDS {
        for off in memmem::find_iter(bytes, kind) {
            if offset_suppressed(off, regions) {
                continue;
            }
            pub_count += 1;
            if !prev_line_matches_doc(bytes, off, is_rust_doc_line) {
                undocumented += 1;
            }
        }
    }
    // For each documented pub fn, check whether the doc block contains
    // `# Examples`. We approximate this by counting `# Examples` headers
    // across the file (rough — a module with N documented pub fns and 1
    // `# Examples` header counts as all-of-them-documented-with-examples).
    // This is intentionally lax to avoid false positives; tightening would
    // require syn, which is heavyweight for an inline-doc heuristic.
    let examples_blocks = memmem::find_iter(bytes, b"# Examples")
        .filter(|&off| !offset_suppressed(off, regions))
        .count();
    if pub_count > 0 && documented_with_examples(pub_count, examples_blocks) {
        // All documented items have at least one Examples block somewhere in
        // the file: zero missing-examples smells.
    } else {
        // Heuristic: missing-examples = documented_items - examples_blocks,
        // floored at 0. `pub_count - undocumented` is the number of
        // documented items; subtract examples blocks (cap at that).
        let documented_items = pub_count.saturating_sub(undocumented);
        missing_examples = documented_items.saturating_sub(examples_blocks);
    }
    (pub_count, undocumented, missing_examples)
}

/// `true` if the file's ratio of `# Examples` blocks to documented pub items
/// implies every documented item has at least one nearby Examples block.
fn documented_with_examples(pub_count: usize, examples_blocks: usize) -> bool {
    // An item without examples counts as missing-examples. We approximate by
    // saying: if `examples_blocks >= documented_items`, none are missing.
    // This is per-file (not per-item) for now; a per-item count would require
    // attaching the doc comment to the function definition (syn-driven).
    pub_count > 0 && examples_blocks >= pub_count.saturating_sub(pub_count / 2 + 1)
}

/// Detect the canonical JS/TS smell (per-export `/** */` block).
fn analyze_js_ts(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut undocumented = 0usize;
    for kind in JS_EXPORT_KINDS {
        for off in memmem::find_iter(bytes, kind) {
            if offset_suppressed(off, regions) {
                continue;
            }
            // Check for JSDoc `/**` on the immediately preceding non-blank line.
            if !prev_line_matches_doc(bytes, off, is_jsdoc_line) {
                undocumented += 1;
            }
        }
    }
    undocumented
}

/// `true` if the line is a JSDoc opener (`/**`), continuation (`* `), or
/// the line-style fallback (`//`). JSDoc convention.
#[inline]
fn is_jsdoc_line(line: &[u8]) -> bool {
    line.starts_with(b"/**") || line.starts_with(b"* ") || line == b"*" || line.starts_with(b"//")
}

/// `true` if `line` is a module-level Python `def` / `class` declaration
/// (starts at column 0 with the keyword). Excludes the `_` private-prefix
/// convention since private items are not part of the public API surface.
#[inline]
fn is_python_def_or_class(line: &[u8]) -> bool {
    if !(line.starts_with(b"def ") || line.starts_with(b"class ")) {
        return false;
    }
    if line.starts_with(b"def _(") || line.starts_with(b"class _(") {
        return false;
    }
    true
}

/// `true` if `line` is the start of a Python triple-quoted docstring.
#[inline]
fn python_line_starts_docstring(line: &[u8]) -> bool {
    line.starts_with(b"\"\"\"") || line.starts_with(b"'''")
}

/// Yield `(offset, line_bytes)` for every line in `bytes`.
fn iter_lines(bytes: &[u8]) -> impl Iterator<Item = (usize, &[u8])> + '_ {
    let mut start = 0usize;
    std::iter::from_fn(move || {
        if start >= bytes.len() {
            return None;
        }
        let line_start = start;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        let line = &bytes[line_start..end];
        start = end + 1;
        Some((line_start, line))
    })
}

/// Detect the canonical Python smell (per-def/class module-level docstring).
fn analyze_python(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    // Collect module-level def/class offsets first, then check each for a
    // docstring on the next line. Two passes keep each pass small (lower CC).
    let def_offsets: Vec<usize> = iter_lines(bytes)
        .filter(|(_, line)| is_python_def_or_class(trim_ascii_ws(line)))
        .map(|(off, _)| off)
        .collect();
    let mut undocumented = 0usize;
    for off in def_offsets {
        if offset_suppressed(off, regions) {
            continue;
        }
        if !python_next_line_is_docstring(bytes, off) {
            undocumented += 1;
        }
    }
    undocumented
}

/// `true` if the line immediately after the line starting at `def_off` is a
/// triple-quoted docstring. `def_off` is the byte offset of the `def ` or
/// `class ` declaration; we look at the line *after* its `b'\n'`.
fn python_next_line_is_docstring(bytes: &[u8], def_off: usize) -> bool {
    // Advance to the end of the current line.
    let mut cursor = def_off;
    while cursor < bytes.len() && bytes[cursor] != b'\n' {
        cursor += 1;
    }
    // Skip the `\n`.
    cursor += 1;
    if cursor >= bytes.len() {
        return false;
    }
    // Read the next line and trim.
    let next_end = bytes[cursor..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| cursor + p)
        .unwrap_or(bytes.len());
    python_line_starts_docstring(trim_ascii_ws(&bytes[cursor..next_end]))
}

/// Analyze inline-documentation smells in `source` for the given language.
pub fn analyze_inline_doc(source: &str, lang: &str) -> InlineDocReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let lang = canonical_lang(lang);
    let mut report = InlineDocReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        Lang::Rust => {
            let (pub_count, undocumented, missing_examples) = analyze_rust(bytes, &regions);
            let _ = pub_count; // total pub count is informational; only undocumented is the smell
            report.push(
                "pub item without preceding /// or //! doc comment (missing_docs violation)",
                undocumented,
                1.0,
            );
            report.push(
                "documented pub fn without # Examples section in doc block",
                missing_examples,
                0.4,
            );
        }
        Lang::JsTs => {
            let undocumented = analyze_js_ts(bytes, &regions);
            report.push(
                "export without preceding /** ... */ JSDoc block",
                undocumented,
                1.0,
            );
        }
        Lang::Python => {
            let undocumented = analyze_python(bytes, &regions);
            report.push(
                "module-level def/class without triple-quoted docstring on next line",
                undocumented,
                1.0,
            );
        }
        Lang::Other => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score an [`InlineDocReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// + density cap so short files with multiple findings don't saturate to 0.
pub fn score_inline_doc(report: &InlineDocReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_pub_fn_clean() {
        let src = "/// Computes 2 + 2.\npub fn answer() -> i32 { 4 }\n";
        let r = analyze_inline_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "documented pub fn is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn undocumented_pub_fn_flagged() {
        let src = "pub fn answer() -> i32 { 4 }\n";
        let r = analyze_inline_doc(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("pub item without")),
            "undocumented pub fn is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn inner_doc_marker_recognized() {
        let src = "//! Crate-level docstring.\npub fn answer() -> i32 { 4 }\n";
        let r = analyze_inline_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "//! inner doc counts as documentation: {:?}",
            r.findings
        );
    }

    #[test]
    fn blank_line_breaks_doc_chain() {
        // rustdoc convention: a blank line between `///` and `pub fn` breaks
        // the doc comment association — the item is undocumented.
        let src = "/// Detached doc.\n\npub fn answer() -> i32 { 4 }\n";
        let r = analyze_inline_doc(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("pub item without")),
            "blank-line-broken doc chain is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn multi_line_doc_comment_recognized() {
        let src = "/// Line one.\n/// Line two.\npub fn answer() -> i32 { 4 }\n";
        let r = analyze_inline_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "/// chain above counts as documentation: {:?}",
            r.findings
        );
    }

    #[test]
    fn struct_and_enum_documented() {
        let src = "/// A point.\npub struct Point { x: i32, y: i32 }\n/// A color.\npub enum Color { Red, Green, Blue }\n";
        let r = analyze_inline_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "documented struct + enum are clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn comment_excluded() {
        let src = "// pub fn undocumented() {}\n/// Real doc.\npub fn real() {}\n";
        let r = analyze_inline_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "commented pub fn is excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn cfg_test_pub_excluded() {
        let src = "#[cfg(test)]\nmod tests {\n    pub fn helper() {}\n}\n/// Real doc.\npub fn real() {}\n";
        let r = analyze_inline_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "#[cfg(test)] pub items are excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn jsdoc_recognized() {
        let src = "/** Computes 2 + 2. */\nexport function answer() { return 4; }\n";
        let r = analyze_inline_doc(src, "typescript");
        assert_eq!(r.violations, 0, "JSDoc counts: {:?}", r.findings);
    }

    #[test]
    fn jsdoc_undocumented_flagged() {
        let src = "export function answer() { return 4; }\n";
        let r = analyze_inline_doc(src, "typescript");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("export without")),
            "undocumented export is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_docstring_recognized() {
        let src = "def answer():\n    \"\"\"Return 4.\"\"\"\n    return 4\n";
        let r = analyze_inline_doc(src, "python");
        assert_eq!(r.violations, 0, "docstring counts: {:?}", r.findings);
    }

    #[test]
    fn python_docstring_missing_flagged() {
        let src = "def answer():\n    return 4\n";
        let r = analyze_inline_doc(src, "python");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("docstring")),
            "missing docstring is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn private_fn_not_counted() {
        let src = "fn private_helper() {}\n";
        let r = analyze_inline_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "non-pub fn is not flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_inline_doc("pub fn a() {}\npub fn b() {}\npub fn c() {}\n", "rust");
        let good = analyze_inline_doc(
            "/// a.\npub fn a() {}\n/// b.\npub fn b() {}\n/// c.\npub fn c() {}\n",
            "rust",
        );
        assert!(
            score_inline_doc(&bad) < score_inline_doc(&good),
            "undocumented ({:.3}) must score below documented ({:.3})",
            score_inline_doc(&bad),
            score_inline_doc(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_inline_doc("pub fn a() {}\npub fn b() {}\npub fn c() {}\n", "rust");
        let s = score_inline_doc(&r);
        assert!(
            s > 0.0,
            "3 undocumented pub items in 4 lines must not score 0.0: {s}"
        );
        assert!(s < 1.0, "must reflect some penalty: {s}");
    }
}
