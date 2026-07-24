//! AST-aware non-executable region detection for security-analysis precision.
//!
//! A TEXT-regex vulnerability detector (the CWE [`PatternRegistry`]) cannot, on
//! its own, separate a real sink from a string that merely *documents* or
//! *tests* an attack. A `UNION SELECT … --` payload living in a `// comment` or
//! inside a `#[cfg(test)]` corpus is not an exploitable construct — flagging it
//! is a false positive on the engine's own non-production text (the workspace
//! WorstOf-F2.1 holders measured 2026-06-21: a `// ...` prose comment, a
//! `#[cfg(test)]` SQLi fixture).
//!
//! This module computes the byte ranges that are **non-executable** for a given
//! `source` + `lang` so [`SecurityAnalyzer`](super::security::SecurityAnalyzer)
//! can drop any vulnerability match whose span starts inside one. The approach
//! mirrors the SAST gold standard: Semgrep disregards commented-out code
//! (`generic_comment_style`) and excludes test paths by default
//! (`.semgrepignore`: `test/`, `tests/`, `*_test.go`). Here the same intent is
//! applied at **in-file region granularity** — strictly more precise than path
//! exclusion, because a real sink in a production helper that happens to share a
//! file with a `#[cfg(test)]` module is still scanned.
//!
//! The scanner is a single forward pass, string-literal aware (so `//` inside
//! `"http://…"` is never mistaken for a comment, and a Rust `'"'` char literal
//! never opens a phantom string), with zero non-std dependencies. Production
//! string literals are deliberately **not** suppressed — injection lives in
//! strings, so a genuine `"… ; --"` in executable code remains a finding; the
//! complementary precision lever is the pattern regex itself (e.g. SQLi
//! requiring a quote-break), not region masking.

// [`PatternRegistry`]: touring_offensive::vuln::PatternRegistry

/// Per-language lexical syntax: the minimum needed to find comments without
/// misreading comment markers that live inside string or char literals.
struct LangSyntax {
    /// Line-comment opener(s) (e.g. `//`, `#`).
    line: &'static [&'static str],
    /// Block-comment `(open, close, nestable)` if the language has one.
    block: Option<(&'static str, &'static str, bool)>,
    /// String-delimiter characters; a `\`-escape continues to the matching delim.
    quotes: &'static [char],
    /// Rust raw-string literals `r"…"` / `r#"…"#` (no escape processing).
    raw_rust: bool,
    /// Rust char/byte literals `'x'` / `'\n'` — distinguished from lifetimes.
    rust_char: bool,
    /// Python triple-quoted strings `"""…"""` / `'''…'''`.
    python_triple: bool,
    /// Whether to additionally detect `#[cfg(test)]` / `#[test]` regions.
    is_rust: bool,
}

const RUST: LangSyntax = LangSyntax {
    line: &["//"],
    block: Some(("/*", "*/", true)),
    quotes: &['"'],
    raw_rust: true,
    rust_char: true,
    python_triple: false,
    is_rust: true,
};
const JS_TS: LangSyntax = LangSyntax {
    line: &["//"],
    block: Some(("/*", "*/", false)),
    quotes: &['"', '\'', '`'],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
};
const GO: LangSyntax = LangSyntax {
    line: &["//"],
    block: Some(("/*", "*/", false)),
    quotes: &['"', '`'],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
};
const PYTHON: LangSyntax = LangSyntax {
    line: &["#"],
    block: None,
    quotes: &['"', '\''],
    raw_rust: false,
    rust_char: false,
    python_triple: true,
    is_rust: false,
};
/// C / C++ / C-family: `//` and `/* */` are comments; `#` begins a
/// **preprocessor directive** (`#define`, `#include`), NOT a comment — so it
/// must stay out of `line` (the GENERIC default wrongly suppressed `#define`).
const CPP: LangSyntax = LangSyntax {
    line: &["//"],
    block: Some(("/*", "*/", false)),
    quotes: &['"', '\''],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
};
const GENERIC: LangSyntax = LangSyntax {
    line: &["//", "#"],
    block: Some(("/*", "*/", false)),
    quotes: &['"', '\''],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
};

fn syntax_for(lang: &str) -> &'static LangSyntax {
    match lang {
        "rust" => &RUST,
        "python" => &PYTHON,
        "javascript" | "typescript" => &JS_TS,
        "go" => &GO,
        "cpp" | "c++" | "cc" | "cxx" | "c" | "h" | "hpp" | "java" => &CPP,
        _ => &GENERIC,
    }
}

/// Returns the merged, sorted, non-overlapping byte ranges of `src` that are
/// non-executable (comments + Rust `#[cfg(test)]`/`#[test]` regions) and so must
/// not host a vulnerability finding.
pub fn non_executable_regions(src: &str, lang: &str) -> Vec<(usize, usize)> {
    let syn = syntax_for(lang);
    let (mut regions, masked) = scan(src, syn);
    if syn.is_rust {
        regions.extend(rust_test_regions(&masked));
    }
    merge(regions)
}

/// True when `offset` falls inside one of the (sorted, non-overlapping) ranges.
pub fn offset_suppressed(offset: usize, regions: &[(usize, usize)]) -> bool {
    regions.iter().any(|&(s, e)| offset >= s && offset < e)
}

fn starts_with(b: &[u8], i: usize, pat: &str) -> bool {
    let p = pat.as_bytes();
    i + p.len() <= b.len() && &b[i..i + p.len()] == p
}

/// Blank `len` bytes from `start` to spaces in `masked` (newlines preserved).
fn blank(masked: &mut [u8], start: usize, len: usize) {
    let end = (start + len).min(masked.len());
    for x in &mut masked[start..end] {
        if *x != b'\n' {
            *x = b' ';
        }
    }
}

/// Single forward pass: collect comment byte-ranges AND produce a `masked` copy
/// where comment, string, and char-literal interiors are blanked to spaces
/// (newlines kept) so downstream brace-matching never trips on a `{`/`#[cfg`
/// that lives inside a string or comment.
fn scan(src: &str, syn: &LangSyntax) -> (Vec<(usize, usize)>, Vec<u8>) {
    let b = src.as_bytes();
    let n = b.len();
    let mut masked = b.to_vec();
    let mut comments: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    'outer: while i < n {
        // 1. Block comment.
        if let Some((open, close, nest)) = syn.block {
            if starts_with(b, i, open) {
                let start = i;
                blank(&mut masked, i, open.len());
                i += open.len();
                let mut depth = 1usize;
                while i < n && depth > 0 {
                    if nest && starts_with(b, i, open) {
                        depth += 1;
                        blank(&mut masked, i, open.len());
                        i += open.len();
                    } else if starts_with(b, i, close) {
                        depth -= 1;
                        blank(&mut masked, i, close.len());
                        i += close.len();
                    } else {
                        if b[i] != b'\n' {
                            masked[i] = b' ';
                        }
                        i += 1;
                    }
                }
                comments.push((start, i));
                continue;
            }
        }
        // 2. Line comment.
        for lc in syn.line {
            if starts_with(b, i, lc) {
                let start = i;
                while i < n && b[i] != b'\n' {
                    masked[i] = b' ';
                    i += 1;
                }
                comments.push((start, i));
                continue 'outer;
            }
        }
        // 3. Rust raw string r#*"…"#* (no escape processing).
        if syn.raw_rust && b[i] == b'r' {
            let mut j = i + 1;
            let mut hashes = 0usize;
            while j < n && b[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < n && b[j] == b'"' {
                let body_start = j + 1;
                blank(&mut masked, i, body_start - i);
                i = body_start;
                loop {
                    if i >= n {
                        break;
                    }
                    if b[i] == b'"' {
                        let mut k = i + 1;
                        let mut h = 0usize;
                        while k < n && h < hashes && b[k] == b'#' {
                            h += 1;
                            k += 1;
                        }
                        if h == hashes {
                            blank(&mut masked, i, k - i);
                            i = k;
                            break;
                        }
                    }
                    if b[i] != b'\n' {
                        masked[i] = b' ';
                    }
                    i += 1;
                }
                continue;
            }
            // `r` was a normal identifier char; fall through.
        }
        // 4. Rust char/byte literal '_' / '\n' — distinguished from a lifetime.
        if syn.rust_char && b[i] == b'\'' {
            if i + 1 < n && b[i + 1] == b'\\' {
                // Escaped char literal: '\n', '\'', '\\', '\xNN', '\u{..}'.
                let mut k = i + 2;
                while k < n && b[k] != b'\'' {
                    if b[k] == b'\\' && k + 1 < n {
                        k += 2;
                    } else {
                        k += 1;
                    }
                }
                let end = if k < n { k + 1 } else { n };
                blank(&mut masked, i, end - i);
                i = end;
                continue;
            } else if i + 2 < n && b[i + 2] == b'\'' {
                // Single-char literal 'x' (incl. '"', '{', '}', '/').
                blank(&mut masked, i, 3);
                i += 3;
                continue;
            } else {
                // Lifetime / label ('a, 'static) — the quote is ordinary code.
                i += 1;
                continue;
            }
        }
        // 5. String literal.
        let c = b[i];
        if syn.quotes.contains(&(c as char)) {
            if syn.python_triple
                && (c == b'"' || c == b'\'')
                && i + 2 < n
                && b[i + 1] == c
                && b[i + 2] == c
            {
                // Python triple-quoted string / docstring.
                blank(&mut masked, i, 3);
                i += 3;
                loop {
                    if i + 2 < n && b[i] == c && b[i + 1] == c && b[i + 2] == c {
                        blank(&mut masked, i, 3);
                        i += 3;
                        break;
                    }
                    if i >= n {
                        break;
                    }
                    if b[i] != b'\n' {
                        masked[i] = b' ';
                    }
                    i += 1;
                }
                continue;
            }
            // Single-delimiter string with backslash escape.
            masked[i] = b' ';
            i += 1;
            while i < n {
                let d = b[i];
                if d == b'\\' {
                    masked[i] = b' ';
                    if i + 1 < n && b[i + 1] != b'\n' {
                        masked[i + 1] = b' ';
                    }
                    i += 2;
                    continue;
                }
                if d == c {
                    masked[i] = b' ';
                    i += 1;
                    break;
                }
                if d == b'\n' && c != b'`' {
                    // Unterminated single-line string: stop (don't swallow file).
                    break;
                }
                if d != b'\n' {
                    masked[i] = b' ';
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    (comments, masked)
}

/// Detect Rust `#[cfg(test)]` (positive) and `#[test]` regions over the masked
/// source, brace-matching the annotated item's body. `#[cfg(not(test))]` is
/// **production** and is intentionally NOT suppressed.
fn rust_test_regions(masked: &[u8]) -> Vec<(usize, usize)> {
    let n = masked.len();
    let mut regions: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < n {
        if starts_with(masked, i, "#[") {
            let attr_start = i;
            // Find the matching ']' (track nested '[' ']').
            let mut j = i + 2;
            let mut depth = 1usize;
            while j < n && depth > 0 {
                match masked[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            let collapsed: String = masked[attr_start..j.min(n)]
                .iter()
                .filter(|c| !c.is_ascii_whitespace())
                .map(|&c| c as char)
                .collect();
            let is_test = collapsed == "#[test]"
                || (collapsed.starts_with("#[cfg(")
                    && collapsed.contains("test")
                    && !collapsed.contains("not(test"));
            i = j;
            if is_test {
                // Scan for the item body opener; a ';' first means no block
                // (e.g. `#[cfg(test)] mod foo;` or `#[cfg(test)] use …;`).
                let mut k = i;
                let mut opener: Option<usize> = None;
                while k < n {
                    match masked[k] {
                        b'{' => {
                            opener = Some(k);
                            break;
                        }
                        b';' => break,
                        _ => k += 1,
                    }
                }
                if let Some(open) = opener {
                    let end = brace_match(masked, open);
                    regions.push((attr_start, end));
                    i = end;
                }
            }
            continue;
        }
        i += 1;
    }
    regions
}

/// Returns the byte index just past the `}` that closes the `{` at `open_idx`.
fn brace_match(masked: &[u8], open_idx: usize) -> usize {
    let n = masked.len();
    let mut depth = 0i32;
    let mut k = open_idx;
    while k < n {
        match masked[k] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
        k += 1;
    }
    n
}

/// Sort by start and coalesce overlapping/adjacent ranges.
fn merge(mut v: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    if v.is_empty() {
        return v;
    }
    v.sort_by_key(|r| r.0);
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(v.len());
    for (s, e) in v {
        if let Some(last) = out.last_mut() {
            if s <= last.1 {
                if e > last.1 {
                    last.1 = e;
                }
                continue;
            }
        }
        out.push((s, e));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte offset of the first occurrence of `needle` in `src`.
    fn off(src: &str, needle: &str) -> usize {
        src.find(needle).expect("needle present")
    }

    #[test]
    fn line_comment_is_suppressed() {
        let src = "let q = 1;\n// UNION SELECT * FROM t\nlet z = 2;\n";
        let r = non_executable_regions(src, "rust");
        assert!(offset_suppressed(off(src, "UNION SELECT"), &r));
        assert!(!offset_suppressed(off(src, "let q"), &r));
    }

    #[test]
    fn block_comment_is_suppressed() {
        let src = "fn f() { /* payload '; -- here */ let x = 1; }";
        let r = non_executable_regions(src, "rust");
        assert!(offset_suppressed(off(src, "'; --"), &r));
        assert!(!offset_suppressed(off(src, "let x"), &r));
    }

    #[test]
    fn slashes_inside_string_are_not_a_comment() {
        // The `//` in the URL must NOT open a comment; the real `//` after must.
        let src = "let u = \"http://host/path\"; // real ; -- comment\n";
        let r = non_executable_regions(src, "rust");
        // The URL's `//` region is not suppressed (it is a string, left intact).
        assert!(!offset_suppressed(off(src, "http"), &r));
        // The genuine trailing comment IS suppressed.
        assert!(offset_suppressed(off(src, "real ; --"), &r));
    }

    #[test]
    fn cfg_test_module_is_suppressed() {
        let src = "fn prod() { let a = 1; }\n#[cfg(test)]\nmod tests {\n    let s = \"UNION SELECT x FROM t\";\n}\n";
        let r = non_executable_regions(src, "rust");
        assert!(offset_suppressed(off(src, "UNION SELECT"), &r));
        assert!(!offset_suppressed(off(src, "let a"), &r));
    }

    #[test]
    fn cfg_not_test_is_production_and_not_suppressed() {
        // `#[cfg(not(test))]` is production-only code — must NOT be suppressed.
        let src = "#[cfg(not(test))]\nfn prod() { let s = \"UNION SELECT x\"; }\n";
        let r = non_executable_regions(src, "rust");
        assert!(!offset_suppressed(off(src, "UNION SELECT"), &r));
    }

    #[test]
    fn cfg_all_test_feature_is_suppressed() {
        let src = "#[cfg(all(test, feature = \"x\"))]\nmod t {\n  let s = \"UNION SELECT\";\n}\n";
        let r = non_executable_regions(src, "rust");
        assert!(offset_suppressed(off(src, "UNION SELECT"), &r));
    }

    #[test]
    fn bare_test_attr_fn_is_suppressed() {
        let src = "#[test]\nfn t() { let s = \"UNION SELECT\"; }\n";
        let r = non_executable_regions(src, "rust");
        assert!(offset_suppressed(off(src, "UNION SELECT"), &r));
    }

    #[test]
    fn production_string_is_not_suppressed() {
        // A vuln literal in executable code is NOT region-suppressed; the
        // pattern regex (e.g. SQLi quote-break) is the lever there, not regions.
        let src = "fn run() { let q = \"UNION SELECT a FROM t\"; }";
        let r = non_executable_regions(src, "rust");
        assert!(!offset_suppressed(off(src, "UNION SELECT"), &r));
    }

    #[test]
    fn rust_char_quote_literal_does_not_swallow_comment() {
        // `'"'` is a char literal — it must not open a string that hides the
        // real `//` comment after it.
        let src = "let c = '\"'; // real ; -- comment\nlet q = 1;\n";
        let r = non_executable_regions(src, "rust");
        assert!(offset_suppressed(off(src, "real ; --"), &r));
        assert!(!offset_suppressed(off(src, "let q"), &r));
    }

    #[test]
    fn rust_char_brace_literal_does_not_break_brace_match() {
        // `'{'` char literal inside a #[cfg(test)] fn must not unbalance braces.
        let src = "#[cfg(test)]\nfn t() { let c = '{'; let s = \"UNION SELECT\"; }\nfn prod() { let a = 1; }\n";
        let r = non_executable_regions(src, "rust");
        assert!(offset_suppressed(off(src, "UNION SELECT"), &r));
        assert!(!offset_suppressed(off(src, "let a"), &r));
    }

    #[test]
    fn python_hash_comment_is_suppressed() {
        let src = "q = 1  # payload '; -- DROP\nx = 2\n";
        let r = non_executable_regions(src, "python");
        assert!(offset_suppressed(off(src, "'; --"), &r));
        assert!(!offset_suppressed(off(src, "x = 2"), &r));
    }

    #[test]
    fn rust_raw_string_inner_markers_ignored() {
        let src = "let r = r#\"// not a comment ; -- still string\"#; let q = 1;";
        let r = non_executable_regions(src, "rust");
        // Nothing inside the raw string is a comment; `let q` stays executable.
        assert!(!offset_suppressed(off(src, "not a comment"), &r));
        assert!(!offset_suppressed(off(src, "let q"), &r));
    }

    #[test]
    fn offset_suppressed_boundaries() {
        let regions = vec![(5usize, 10usize)];
        assert!(!offset_suppressed(4, &regions));
        assert!(offset_suppressed(5, &regions));
        assert!(offset_suppressed(9, &regions));
        assert!(!offset_suppressed(10, &regions)); // half-open [s, e)
    }

    #[test]
    fn empty_source_has_no_regions() {
        assert!(non_executable_regions("", "rust").is_empty());
        assert!(non_executable_regions("let x = 1;", "rust").is_empty());
    }

    #[test]
    fn merge_coalesces_overlaps() {
        let merged = merge(vec![(0, 5), (3, 8), (20, 25), (6, 7)]);
        assert_eq!(merged, vec![(0, 8), (20, 25)]);
    }
}
