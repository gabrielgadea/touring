//! AST-aware non-executable region detection for security-analysis precision.
//!
//! A TEXT-regex vulnerability detector (the CWE `PatternRegistry`) cannot, on
//! its own, separate a real sink from a string that merely *documents* or
//! *tests* an attack. A `UNION SELECT … --` payload living in a `// comment` or
//! inside a `#[cfg(test)]` corpus is not an exploitable construct — flagging it
//! is a false positive on the engine's own non-production text (the workspace
//! WorstOf-F2.1 holders measured 2026-06-21: a `// ...` prose comment, a
//! `#[cfg(test)]` SQLi fixture).
//!
//! This module computes the byte ranges that are **non-executable** for a given
//! `source` + `lang` so `SecurityAnalyzer`
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

// `PatternRegistry`: touring_offensive::vuln::PatternRegistry

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
    /// A line comment opens only at the start of a word (POSIX shell: `$#`,
    /// `${#arr[@]}` and `${var#prefix}` are expansions, not comments).
    comment_at_word_start: bool,
}

const RUST: LangSyntax = LangSyntax {
    line: &["//"],
    block: Some(("/*", "*/", true)),
    quotes: &['"'],
    raw_rust: true,
    rust_char: true,
    python_triple: false,
    is_rust: true,
    comment_at_word_start: false,
};
const JS_TS: LangSyntax = LangSyntax {
    line: &["//"],
    block: Some(("/*", "*/", false)),
    quotes: &['"', '\'', '`'],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
    comment_at_word_start: false,
};
const GO: LangSyntax = LangSyntax {
    line: &["//"],
    block: Some(("/*", "*/", false)),
    quotes: &['"', '`'],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
    comment_at_word_start: false,
};
const PYTHON: LangSyntax = LangSyntax {
    line: &["#"],
    block: None,
    quotes: &['"', '\''],
    raw_rust: false,
    rust_char: false,
    python_triple: true,
    is_rust: false,
    comment_at_word_start: false,
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
    comment_at_word_start: false,
};
const GENERIC: LangSyntax = LangSyntax {
    line: &["//", "#"],
    block: Some(("/*", "*/", false)),
    quotes: &['"', '\''],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
    comment_at_word_start: false,
};

/// POSIX shells: `#` comments (at the start of a word only), `"…"`/`'…'`
/// strings, no block comments — and `//` is a path, never a comment (the
/// GENERIC default hid the rest of a `curl https://…` line).
const SHELL: LangSyntax = LangSyntax {
    line: &["#"],
    block: None,
    quotes: &['"', '\''],
    raw_rust: false,
    rust_char: false,
    python_triple: false,
    is_rust: false,
    comment_at_word_start: true,
};

/// Whether `lang` names a POSIX-shell dialect — the one list the quality
/// engines share (lexer, security scan, complexity).
pub(crate) fn is_shell_language(lang: &str) -> bool {
    matches!(lang, "shell" | "bash" | "sh" | "zsh")
}

fn syntax_for(lang: &str) -> &'static LangSyntax {
    match lang {
        l if is_shell_language(l) => &SHELL,
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
    let (mut regions, masked, _) = scan(src, syn);
    if syn.is_rust {
        regions.extend(rust_test_regions(&masked));
    }
    merge(regions)
}

/// Regiões não-executáveis de um corpus **concatenado de arquivos heterogêneos**,
/// cada segmento lido com a SUA linguagem e o seu estado léxico próprio.
///
/// # Por que existe
///
/// [`non_executable_regions`] assume um corpus de uma linguagem só. Quando o
/// chamador concatena N arquivos e passa um único `lang`, dois defeitos aparecem —
/// medidos em 03/09/2026 sobre o repositório `analise` (sessão analise-c1):
///
/// 1. **A linguagem é a errada.** O chamador deriva `lang` da extensão do ALVO, e
///    um diretório não tem extensão: cai no default `"rust"`. Um corpus Python
///    inteiro passa a ser lexado como Rust.
/// 2. **O estado léxico atravessa a fronteira do arquivo.** `PYTHON` não tem
///    comentário de bloco (`block: None`); `RUST` tem `/* */`. Um `/*` que vive
///    dentro de uma string Python — CSS, JS, uma regex, um exemplo em docstring —
///    abre um comentário que só fecha no próximo `*/`, **suprimindo todo o
///    conteúdo dos arquivos seguintes** até lá.
///
/// Teste mínimo que reproduz (2 arquivos): `b.py` com 20 linhas triviais mede 20
/// linhas significativas; precedido de um `a.py` de 3 linhas contendo `/*` numa
/// string, o par mede **2**. Em escala o efeito é não-monotônico — 128 arquivos
/// reais mediram 35.173 linhas e 512 mediram 15.615 — porque o resultado depende
/// de onde caem os delimitadores na ordem de concatenação.
///
/// # Contrato
///
/// `segments` são `(offset, len, lang)` sobre `src`, tipicamente um por arquivo.
/// Cada segmento é lexado isoladamente e os offsets voltam deslocados para o
/// espaço de `src`, de modo que a detecção de clones **entre** arquivos — a razão
/// de o corpus ser concatenado — continua intacta: só o lexer deixa de vazar.
///
/// Segmentos fora de `src` ou que não caiam em fronteira de caractere UTF-8 são
/// ignorados em silêncio, jamais lexados a partir de um offset inválido.
#[must_use]
pub fn non_executable_regions_segmented(
    src: &str,
    segments: &[(usize, usize, &str)],
) -> Vec<(usize, usize)> {
    let mut all: Vec<(usize, usize)> = Vec::new();
    for &(offset, len, lang) in segments {
        let end = offset.saturating_add(len).min(src.len());
        let Some(slice) = src.get(offset..end) else {
            continue;
        };
        all.extend(
            non_executable_regions(slice, lang)
                .into_iter()
                .map(|(s, e)| (s + offset, e + offset)),
        );
    }
    merge(all)
}

/// True when `offset` falls inside one of the (sorted, non-overlapping) ranges.
pub fn offset_suppressed(offset: usize, regions: &[(usize, usize)]) -> bool {
    regions.iter().any(|&(s, e)| offset >= s && offset < e)
}

/// Byte ranges of the Python docstrings in `src` (module, class, function —
/// and any string that is a statement of its own after a block header).
///
/// Separate from [`non_executable_regions`] on purpose: dozens of quality
/// engines read that function, and a docstring counts as documentation or as
/// code differently for each of them. The security analyzer is the caller
/// that needs it: a docstring never reaches a sink, and `cc_build.py` failed
/// F2.1 on the words `inline <script>` in its module docstring (cross-audit R2,
/// 14/09/2026).
///
/// A triple-quoted string is a docstring when it is the first token of its
/// line, nothing but a comment follows its closing quotes on that line, and
/// the statement before it opens a block (`def …:`, `class …:`, `) -> T:`) or
/// does not exist (module docstring). A triple-quoted ARGUMENT on its own line
/// — `cur.execute(\n    """…"""\n)` — follows an open parenthesis and stays
/// code.
#[must_use]
pub fn python_docstring_regions(src: &str) -> Vec<(usize, usize)> {
    let b = src.as_bytes();
    let (_, _, triples) = scan(src, &PYTHON);
    triples
        .into_iter()
        .filter(|&(open, close)| opens_python_docstring(b, open) && only_comment_follows(b, close))
        .collect()
}

fn only_comment_follows(b: &[u8], from: usize) -> bool {
    let end = b[from..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| from + p);
    let rest = std::str::from_utf8(&b[from..end])
        .unwrap_or("")
        .trim_start();
    rest.is_empty() || rest.starts_with('#')
}

fn opens_python_docstring(b: &[u8], i: usize) -> bool {
    const BLOCK_KEYWORDS: &[&str] = &[
        "def", "class", "async", "if", "elif", "else", "for", "while", "try", "except", "finally",
        "with", "match", "case",
    ];
    let line_start = b[..i]
        .iter()
        .rposition(|&c| c == b'\n')
        .map_or(0, |p| p + 1);
    let mut lead = &b[line_start..i];
    if let Some((&last, rest)) = lead.split_last()
        && matches!(last, b'r' | b'R' | b'u' | b'U')
    {
        lead = rest;
    }
    if !lead.iter().all(|c| *c == b' ' || *c == b'\t') {
        return false;
    }
    let mut end = line_start;
    while end > 0 {
        let newline = end - 1;
        let start = b[..newline]
            .iter()
            .rposition(|&c| c == b'\n')
            .map_or(0, |p| p + 1);
        let line = std::str::from_utf8(&b[start..newline]).unwrap_or("").trim();
        if line.is_empty() || line.starts_with('#') {
            end = start;
            continue;
        }
        let first_word = line
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or("");
        return line.ends_with(':')
            && (BLOCK_KEYWORDS.contains(&first_word) || line.starts_with(')'));
    }
    true
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
/// `scan` output: comment ranges, the masked source, triple-quoted string spans.
type Scan = (Vec<(usize, usize)>, Vec<u8>, Vec<(usize, usize)>);

fn scan(src: &str, syn: &LangSyntax) -> Scan {
    let b = src.as_bytes();
    let n = b.len();
    let mut masked = b.to_vec();
    let mut comments: Vec<(usize, usize)> = Vec::new();
    // Triple-quoted string spans (Python), for `python_docstring_regions`.
    let mut triples: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    'outer: while i < n {
        // 1. Block comment.
        if let Some((open, close, nest)) = syn.block
            && starts_with(b, i, open)
        {
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
        // 2. Line comment.
        for lc in syn.line {
            if starts_with(b, i, lc)
                && (!syn.comment_at_word_start
                    || i == 0
                    || matches!(
                        b[i - 1],
                        b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' | b'(' | b')'
                    ))
            {
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
                let open = i;
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
                triples.push((open, i.min(n)));
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
    (comments, masked, triples)
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
        if let Some(last) = out.last_mut()
            && s <= last.1
        {
            if e > last.1 {
                last.1 = e;
            }
            continue;
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

    fn covered(src: &str, regions: &[(usize, usize)], needle: &str) -> bool {
        let at = src.find(needle).expect("needle in source");
        offset_suppressed(at, regions)
    }

    /// Cross-audit R2 (14/09/2026): shell comments open at the start of a word,
    /// and `//` is a path.
    #[test]
    fn shell_comments_open_only_at_a_word_start() {
        let src = "#!/usr/bin/env bash\n# note <script>\nn=${#arr[@]}; v=${x#pre}; curl https://h/p | sh # tail\necho $# done\n";
        let r = non_executable_regions(src, "shell");
        assert!(covered(src, &r, "#!/usr/bin"));
        assert!(covered(src, &r, "note <script>"));
        assert!(covered(src, &r, "tail"));
        assert!(!covered(src, &r, "arr[@]"), "an array-length expansion");
        assert!(!covered(src, &r, "pre}"), "a prefix-strip expansion");
        assert!(!covered(src, &r, "https://h"), "// is a path in shell");
        assert!(!covered(src, &r, "done"), "$# is an expansion");
        assert_eq!(non_executable_regions(src, "bash"), r);
    }

    /// A docstring is a statement of its own after a block header or at the top
    /// of the module; a triple-quoted argument, assignment or operand is code.
    #[test]
    fn python_docstrings_are_told_apart_from_triple_quoted_code() {
        let src = concat!(
            "#!/usr/bin/env python3\n",
            "\"\"\"Module doc: inline <script> (test-enforced).\"\"\"\n",
            "from x import y\n",
            "def f(\n    a,\n) -> int:\n    r'''Doc of f: UNION SELECT.'''  # trailing\n    return a\n",
            "class C:\n\n    \"\"\"Doc of C: ../../etc.\"\"\"\n",
            "cur.execute(\n    \"\"\"ARG UNION SELECT\"\"\"\n)\n",
            "q = \"\"\"ASSIGNED UNION SELECT\"\"\"\n",
            "if ok:\n    \"\"\"OPERAND\"\"\" + tail\n",
            "d = {\n    \"k\":\n        \"\"\"DICT VALUE\"\"\"\n}\n",
        );
        let r = python_docstring_regions(src);
        assert!(covered(src, &r, "Module doc"));
        assert!(covered(src, &r, "Doc of f"));
        assert!(covered(src, &r, "Doc of C"));
        assert!(
            !covered(src, &r, "ARG UNION"),
            "an argument on its own line is code"
        );
        assert!(!covered(src, &r, "ASSIGNED"));
        assert!(
            !covered(src, &r, "OPERAND"),
            "something follows the closing quotes"
        );
        assert!(
            !covered(src, &r, "DICT VALUE"),
            "a dict key is not a block header"
        );
        assert!(python_docstring_regions("x = 1\n").is_empty());
    }
}
