//! Shared loop-body region finder for the quality engines.
//!
//! Several dimensions need "is X inside a loop body" analysis (F2.7 db-perf's
//! N+1 = a DB call in a loop; F2.8 memory's hot-path allocation = a `.to_vec()`/
//! `.to_owned()` in a loop). This module owns the single brace-matched /
//! indent-scoped loop-body finder so the logic is not duplicated across engines
//! (which the F1.3 Type-1 clone detector would itself flag). Comments /
//! `#[cfg(test)]` regions are honoured via the caller-supplied `regions`; string
//! literals (which `code_regions` deliberately does *not* suppress) are skipped
//! by the brace matcher so a `{`/`}` inside a string never miscounts depth.

use memchr::memmem;

use super::code_regions::offset_suppressed;

#[inline]
fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `true` if `bytes[idx]` exists and is an identifier char (word-boundary test).
#[inline]
fn ident_at(bytes: &[u8], idx: usize) -> bool {
    bytes.get(idx).is_some_and(|&c| is_ident(c))
}

/// Advance past a string or brace-containing char literal opening at `bytes[i]`,
/// returning the index just after it (or `i` unchanged if `bytes[i]` opens
/// nothing). This is what lets the brace matcher ignore `{`/`}`/`(`/`)` that live
/// inside a string literal — e.g. `log("close }")` — which `code_regions` does
/// *not* suppress (strings stay visible for the SQL/injection detectors). Handles
/// `"` / `` ` `` strings (with `\`-escapes) and `'{'`/`'}'` (also `b'{'`) chars.
fn skip_literal(bytes: &[u8], i: usize) -> usize {
    match bytes.get(i) {
        Some(b'"') | Some(b'`') => {
            let quote = bytes[i];
            let mut j = i + 1;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2, // skip the escaped char
                    c if c == quote => return j + 1,
                    _ => j += 1,
                }
            }
            j
        }
        // Char literal whose content is a brace: `'{'` / `'}'` (3 bytes).
        Some(b'\'')
            if matches!(bytes.get(i + 1), Some(b'{') | Some(b'}'))
                && bytes.get(i + 2) == Some(&b'\'') =>
        {
            i + 3
        }
        _ => i,
    }
}

/// `true` if the Rust `for ` at `[head_start, open)` opens a *loop* (`for pat in
/// iter {`) rather than a **trait impl** (`impl Trait for Type {`). A Rust loop
/// always has an ` in ` keyword before the body brace; a trait impl never does.
/// Only applied to Rust — Go/C/JS/Java have no `impl … for` and write loops as
/// `for (…)` or a bare `for` (Go), which are always loops.
fn rust_for_is_loop(bytes: &[u8], head_start: usize, open: usize) -> bool {
    head_start < open && memmem::find(&bytes[head_start..open], b" in ").is_some()
}

/// The `[body_start, body_end)` byte ranges of every `for`/`while` loop body in
/// `bytes`, with the comment/test ranges in `regions` skipped. Python (`py`)
/// selects indent-scoped bodies; everything else is brace-matched (paren-aware,
/// so a C/JS `for (i=0; i<n; i++)` and a Go 3-clause `for i := 0; i < n; i++ {`
/// both work). For Rust, a `for ` keyword that is actually a trait impl
/// (`impl Trait for Type {`) is excluded.
pub(crate) fn loop_bodies(
    bytes: &[u8],
    regions: &[(usize, usize)],
    lang: &str,
) -> Vec<(usize, usize)> {
    match lang {
        "python" | "py" => python_loop_blocks(bytes, regions),
        _ => brace_loop_blocks(bytes, regions, matches!(lang, "rust" | "rs")),
    }
}

/// Brace-language loop bodies. Paren-aware so the header parens of a C/JS
/// `for (…)` and the `;`-at-depth-0 of a Go 3-clause `for` both work, and a
/// closure brace inside the iterator expr (`for x in f(|y| { … }) {`) is skipped
/// (its `{` is at paren depth > 0). Braces inside string/char literals are
/// skipped via [`skip_literal`]. When `is_rust`, a `for ` that is a trait impl
/// (`impl … for Type {`, no ` in `) is not treated as a loop.
fn brace_loop_blocks(
    bytes: &[u8],
    regions: &[(usize, usize)],
    is_rust: bool,
) -> Vec<(usize, usize)> {
    // `loop { … }` and `loop{ … }` are Rust's infinite-loop keyword; missing
    // them here made F2.10's `unbuffered_read_loop` detector blind to
    // `loop { r.read_exact(&mut buf)… }`. Other brace languages don't have a
    // bare `loop` keyword (Go uses `for`, JS uses `while`/`for`, C uses
    // `for`/`while`), and the `is_rust` branch already gates language-specific
    // filtering, so adding the keyword only when the file is Rust is safe.
    const HEADERS: [&[u8]; 6] = [b"for ", b"for(", b"while ", b"while(", b"loop ", b"loop{"];
    let mut blocks = Vec::new();
    for header in HEADERS {
        for off in memmem::find_iter(bytes, header) {
            if offset_suppressed(off, regions) {
                continue;
            }
            // Word boundary before the keyword (so `before`/`myfor(` do not match).
            if ident_at(bytes, off.wrapping_sub(1)) {
                continue;
            }
            // Find the body-opening `{` at paren depth 0 (no `;` bail → Go 3-clause
            // `for` works; paren tracking → C/JS header parens and closure braces
            // in the iterator expression are handled; literals are skipped).
            let mut paren: i32 = if header.last() == Some(&b'(') { 1 } else { 0 };
            let mut j = off + header.len();
            let mut open = None;
            while j < bytes.len() {
                if offset_suppressed(j, regions) {
                    j += 1;
                    continue;
                }
                let after = skip_literal(bytes, j);
                if after > j {
                    j = after; // a string/char literal — its braces/parens don't count
                    continue;
                }
                match bytes[j] {
                    b'(' => paren += 1,
                    b')' => paren -= 1,
                    b'{' if paren <= 0 => {
                        open = Some(j);
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }
            let Some(open) = open else { continue };
            // A Rust `for ` keyword also opens a trait impl (`impl Trait for
            // Type {`) — only a loop if it has the ` in ` keyword.
            if is_rust && header == b"for " && !rust_for_is_loop(bytes, off + header.len(), open) {
                continue;
            }
            // Brace-match to the closing `}`.
            let mut depth = 0usize;
            let mut k = open;
            let mut close = None;
            while k < bytes.len() {
                if offset_suppressed(k, regions) {
                    k += 1;
                    continue;
                }
                let after = skip_literal(bytes, k);
                if after > k {
                    k = after;
                    continue;
                }
                match bytes[k] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(k);
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            if let Some(c) = close {
                blocks.push((open + 1, c));
            }
        }
    }
    blocks
}

/// Python loop bodies: the indented block after a statement-position `for`/`while`
/// header. The header must be the first token of its line (so a list/dict
/// comprehension `[f(x) for x in xs]` — `for` mid-line — is excluded). The body is
/// the run of subsequent lines more-indented than the header (blank lines kept).
fn python_loop_blocks(bytes: &[u8], regions: &[(usize, usize)]) -> Vec<(usize, usize)> {
    const HEADERS: [&[u8]; 2] = [b"for ", b"while "];
    let mut blocks = Vec::new();
    for header in HEADERS {
        for off in memmem::find_iter(bytes, header) {
            if offset_suppressed(off, regions) {
                continue;
            }
            let line_start = bytes[..off]
                .iter()
                .rposition(|&c| c == b'\n')
                .map_or(0, |p| p + 1);
            let header_indent = bytes[line_start..]
                .iter()
                .take_while(|&&c| c == b' ' || c == b'\t')
                .count();
            // Must be the first token of the line (excludes comprehensions).
            if off != line_start + header_indent {
                continue;
            }
            // End of the header line.
            let mut p = off;
            while p < bytes.len() && bytes[p] != b'\n' {
                p += 1;
            }
            if p >= bytes.len() {
                continue; // no body
            }
            let body_start = p + 1;
            let mut idx = body_start;
            let mut body_end = body_start;
            while idx < bytes.len() {
                let ls = idx;
                let mut le = idx;
                while le < bytes.len() && bytes[le] != b'\n' {
                    le += 1;
                }
                let line = &bytes[ls..le];
                let blank = line.iter().all(|&c| c == b' ' || c == b'\t');
                if blank {
                    body_end = le;
                    idx = le + 1;
                    continue;
                }
                let indent = line
                    .iter()
                    .take_while(|&&c| c == b' ' || c == b'\t')
                    .count();
                if indent > header_indent {
                    body_end = le;
                    idx = le + 1;
                } else {
                    break; // dedent → body ends
                }
            }
            if body_end > body_start {
                blocks.push((body_start, body_end));
            }
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::code_regions::non_executable_regions;

    fn bodies(src: &str, lang: &str) -> Vec<String> {
        let regions = non_executable_regions(src, lang);
        loop_bodies(src.as_bytes(), &regions, lang)
            .into_iter()
            .map(|(s, e)| src[s..e].to_string())
            .collect()
    }

    #[test]
    fn brace_for_body_extracted() {
        let b = bodies("for x in xs {\n    work(x);\n}\n", "rust");
        assert_eq!(b.len(), 1);
        assert!(b[0].contains("work(x)"), "body: {:?}", b);
    }

    #[test]
    fn go_three_clause_for_body_extracted() {
        // `;` at paren depth 0 must not abort the `{`-search; Go has no `impl for`.
        let b = bodies("for i := 0; i < n; i++ {\n    q(i)\n}\n", "go");
        assert_eq!(b.len(), 1, "Go 3-clause for body: {:?}", b);
    }

    #[test]
    fn closure_brace_in_iterator_not_misscoped() {
        let b = bodies(
            "for y in xs.iter().map(|y| { y + 1 }) {\n    real(y);\n}\n",
            "rust",
        );
        assert_eq!(b.len(), 1);
        assert!(
            b[0].contains("real(y)") && !b[0].contains("y + 1"),
            "body: {:?}",
            b
        );
    }

    #[test]
    fn python_indent_body_extracted_comprehension_excluded() {
        let b = bodies(
            "for u in users:\n    visit(u)\n    log(u)\nafter()\n",
            "python",
        );
        assert_eq!(b.len(), 1, "py loop body: {:?}", b);
        assert!(
            b[0].contains("visit(u)") && !b[0].contains("after()"),
            "body: {:?}",
            b
        );
        let c = bodies("rows = [f(x) for x in items]\n", "python");
        assert!(c.is_empty(), "comprehension is not a loop: {:?}", c);
    }

    #[test]
    fn comment_loop_excluded() {
        let b = bodies("// for x in xs { work() }\nfn f() {}\n", "rust");
        assert!(b.is_empty(), "commented loop excluded: {:?}", b);
    }

    #[test]
    fn string_brace_not_miscounted() {
        // A `{` inside a string literal must not extend the body past the real
        // `}` (regression: the unbalanced `{` used to engulf trailing code).
        let b = bodies(
            "for x in xs {\n    log(\"open brace {\");\n}\nlet y = z.to_vec();\n",
            "rust",
        );
        assert_eq!(b.len(), 1, "exactly one loop body: {:?}", b);
        assert!(
            b[0].contains("log(") && !b[0].contains("to_vec"),
            "body must stop at the real closing brace, not the string's: {:?}",
            b
        );
    }

    #[test]
    fn paren_and_brace_in_string_not_miscounted() {
        let b = bodies(
            "for s in xs.filter(|s| s.contains(\") {\")) {\n    use_it(s);\n}\n",
            "rust",
        );
        assert_eq!(b.len(), 1, "one body despite string parens/braces: {:?}", b);
        assert!(b[0].contains("use_it(s)"), "body: {:?}", b);
    }

    #[test]
    fn rust_trait_impl_for_not_a_loop() {
        // `impl Trait for Type {` has a `for ` keyword but is NOT a loop — its
        // body must not be scanned (regression: it engulfed `impl` bodies and
        // any `.to_vec()`/`.execute()` inside them = a false N+1 / hot-path).
        let b = bodies(
            "impl Iterator for MyT {\n    fn next(&mut self) { let v = x.to_vec(); }\n}\n",
            "rust",
        );
        assert!(
            b.is_empty(),
            "trait impl `for` must not be a loop body: {:?}",
            b
        );
        // But a real loop inside the impl IS found.
        let b2 = bodies(
            "impl T for U {\n    fn f(&self) {\n        for k in ks { q(k); }\n    }\n}\n",
            "rust",
        );
        assert_eq!(b2.len(), 1, "the real inner loop is found: {:?}", b2);
        assert!(b2[0].contains("q(k)"), "body: {:?}", b2);
    }

    #[test]
    fn rust_generic_trait_impl_for_array_not_a_loop() {
        // `impl Foo for [u8; 4] {` has a `;` in the type but no ` in ` → not a loop.
        let b = bodies(
            "impl Foo for [u8; 4] {\n    fn g() { y.to_owned(); }\n}\n",
            "rust",
        );
        assert!(
            b.is_empty(),
            "impl for an array type is not a loop: {:?}",
            b
        );
    }
}
