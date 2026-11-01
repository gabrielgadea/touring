//! CWEx E12 — Common Weakness Enumeration Patterns
//!
//! Implements the `PatternRegistry` with 10 vulnerability detectors covering
//! OWASP Top 10 and CWE Top 25 patterns.
//!
//! Each detector uses regex matching to identify vulnerable code constructs.

use crate::vuln::{VulnMatch, VulnerabilityPattern};
use regex::Regex;
use std::sync::OnceLock;

static SQLI_RE: OnceLock<Regex> = OnceLock::new();
static XSS_RE: OnceLock<Regex> = OnceLock::new();
static INERT_URL_BODY_RE: OnceLock<Regex> = OnceLock::new();
static CMDI_RE: OnceLock<Regex> = OnceLock::new();
static PATH_TRAV_RE: OnceLock<Regex> = OnceLock::new();
static INT_OVF_RE: OnceLock<Regex> = OnceLock::new();
static BUF_OVF_RE: OnceLock<Regex> = OnceLock::new();
static DESER_RE: OnceLock<Regex> = OnceLock::new();
static SSRF_RE: OnceLock<Regex> = OnceLock::new();
static LDAPI_RE: OnceLock<Regex> = OnceLock::new();
static XML_INJ_RE: OnceLock<Regex> = OnceLock::new();

// ---------------------------------------------------------------------------
// SQL Injection — CWE-89
// ---------------------------------------------------------------------------
/// Detects SQL injection constructs (CWE-89) such as tautologies and `UNION SELECT`.
#[derive(Debug, Clone, Copy)]
pub struct SqlInjectionPattern;

impl VulnerabilityPattern for SqlInjectionPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        let re = SQLI_RE.get_or_init(|| {
            // The injection arms use SAME-LINE whitespace `[ \t]` (not `\s`,
            // which the engine matches across NEWLINES, manufacturing false
            // positives over multi-line text). Two precision requirements,
            // both validated by tp/fp corpus:
            //  1. comment-injection needs a STRING-BREAK quote before `; --`:
            //     `'…; --` on ONE line. `\s*` would have (a) matched benign CLI
            //     help `"…state; --persist…"` (no quote) and (b) spanned a
            //     newline so a DDL `'id');` followed by a next-line SQL `--`
            //     comment (the embedded schema in touring-foundation) looked
            //     like injection — neither is SQL injection.
            //  2. the `' OR '` tautology stays on one line too (`'A'\nOR\n'B'`
            //     prose is not a payload).
            // Preserved TPs: `'; --`, `' OR 1=1; --`, `' OR '`, `UNION SELECT`.
            //  3. (14/09/2026, cross-audit) the `--` must END the injected clause —
            //     followed by whitespace, a quote or the end of the text. A CSS
            //     custom property in an f-string, `f"--bg:{lt['bg']};--fg:…"`,
            //     has a quote, `;` and `--` too, but an identifier follows it; it
            //     zeroed F2.1 on the dashboard builder.
            Regex::new(
                r#"('[ \t]*OR[ \t]*'|'[^'\n]{0,80};[ \t]*--(?:[ \t\r\n'"]|$)|UNION\s+SELECT)"#,
            )
            .expect("valid static regex")
        });
        re.find_iter(input)
            .map(|m| VulnMatch::new("SQLi".into(), (m.start(), m.end()), 9.8, 89))
            .collect()
    }
    fn name(&self) -> &str {
        "SQLi"
    }
    fn severity(&self) -> f32 {
        9.8
    }
    fn cwe_id(&self) -> u32 {
        89
    }
}

// ---------------------------------------------------------------------------
// Cross-Site Scripting — CWE-79
// ---------------------------------------------------------------------------
/// Detects cross-site scripting (CWE-79): `<script>` tags, `javascript:` URIs (the scheme, in any letter case, at the start of a URL value: not the word in prose, not a literal a scheme blocklist refuses, not an inert constant URL such as `javascript:void(0)`), and inline HTML event-handler attributes (lowercase DOM events).
#[derive(Debug, Clone, Copy)]
pub struct XssPattern;

impl VulnerabilityPattern for XssPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        let re = XSS_RE.get_or_init(|| {
            // CWE-79: a `<script` tag, a `javascript:` URI, or an HTML event
            // handler attribute assigned inline. The handler list is LOWERCASE
            // and curated to real DOM events (OWASP XSS Filter Evasion). Lowercase
            // is the precision key: real HTML attributes are lowercase
            // (`onerror=`), whereas React/JSX props are camelCase (`onClick=`) and
            // minified JS emits assignments like `oneMapping=`/`onUpdate=` — the
            // old broad `on\w+=` flagged 131 such benign workspace lines; the
            // curated lowercase list flags 0 in production code.
            // The `javascript:` URI arm forbids a SECOND colon after it so the
            // Rust path separator `::` is not flagged: the scheme is
            // `javascript:<body>` (or bare at end), whereas
            // `tree_sitter_javascript::LANGUAGE` / any `…javascript::…` path is
            // benign code. `(?:[^:]|$)` = next char is non-`:` OR end-of-input
            // (the `regex` crate has no lookahead; this is the linear-time
            // equivalent of `(?!:)` while still matching a bare `javascript:`).
            // The scheme arm folds ASCII case (`(?i-u:…)`): a URL scheme is
            // case-insensitive to the browser, so `JavaScript:` is the same sink.
            // `<script` and the `on…=` handlers stay lowercase-only on purpose:
            // `<Script>` is a framework component and `onClick=` a JSX prop
            // (measured 09/10/2026 over 12.762 source files: 150 with a
            // non-lowercase handler, almost all camelCase props).
            Regex::new(
                r"(<script[\s>/]|(?i-u:javascript):(?:[^:]|$)|\bon(error|load|click|dblclick|mouseover|mouseout|mouseenter|mouseleave|mousedown|mouseup|mousemove|focus|focusin|blur|submit|reset|change|input|select|keydown|keyup|keypress|toggle|animationstart|animationend|transitionend|pointerover|pointerenter|pointerdown|wheel|scroll|drag|dragstart|drop|copy|cut|paste|contextmenu|hashchange|popstate|pageshow|pagehide|beforeunload|resize|start|playing|canplay|loadstart|loadeddata|abort|invalid|search)\s*=)",
            )
            .expect("valid static regex")
        });
        /// `at` is inside an HTML tag opened before it and not yet closed:
        /// the last `<` before `at` is followed by a letter and no `>`
        /// follows it. `Vec<u8>` closes; `a < b` never opens.
        fn inside_open_tag(input: &str, at: usize) -> bool {
            let before = &input[..at];
            let Some(lt) = before.rfind('<') else {
                return false;
            };
            let opens_tag = before[lt + 1..]
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic());
            opens_tag && !before[lt..].contains('>')
        }
        re.find_iter(input)
            .filter(|m| {
                let text = m.as_str();
                // F2.1 (09/10/2026, touring-4f): `javascript:` is a URL SCHEME, in any
                // letter case. It is a sink only where a URL value begins
                // (`log("rota javascript: …")` talks ABOUT it) and when it is not the
                // literal a scheme BLOCKLIST names to refuse it. The open-tag rescue
                // below never applies to it: `a<b` in JS opened a phantom "tag" and
                // kept `u.startsWith('javascript:')` reported. Nor is an INERT constant URL
                // (`href="javascript:;"`, `void(0)`, `history.back()`): no untrusted
                // data reaches it (F2.1, 10/10/2026).
                if is_javascript_scheme(text) {
                    return starts_url_value(input, m.start())
                        && !is_scheme_blocklist_literal(input, m.start())
                        && !is_inert_constant_url(input, m.start());
                }
                // B3 (2026-09-02): the handler arm (`on<event>=`) is an HTML
                // attribute — a sink only INSIDE an open tag. Bare, it is a
                // keyword argument: `os.walk(top, onerror=…)` in a diagnostic
                // script was blocked by the P0 gate (F2.1) as XSS.
                // R2 (14/09/2026): `--file <script>` in usage text is the
                // placeholder of a flag argument, not a tag.
                // 25/09/2026 (touring-36): the TAINT question for a CONSTANT
                // handler (`onclick="rodar('preset')"`) is the CODE judge's,
                // not this pattern's — the concolic/payload path measures
                // INPUT, where the handler IS the attack. The code judge
                // post-filters constant handlers (`touring_analysis::quality::
                // security::is_constant_handler_attribute`).
                (text.starts_with("<script")
                    && !is_cli_placeholder(input, m.start(), m.end())
                    && !in_html_document_template(input, m.start()))
                    || inside_open_tag(input, m.start())
            })
            .map(|m| VulnMatch::new("XSS".into(), (m.start(), m.end()), 8.1, 79))
            .collect()
    }
    fn name(&self) -> &str {
        "XSS"
    }
    fn severity(&self) -> f32 {
        8.1
    }
    fn cwe_id(&self) -> u32 {
        79
    }
}

// ---------------------------------------------------------------------------
// Command Injection — CWE-78
// ---------------------------------------------------------------------------
/// Detects OS command injection (CWE-78): shell-metachar payloads and dynamic shell-exec (`execSync`/`exec` template interpolation, `os.system(f"…")`, `shell=True`).
#[derive(Debug, Clone, Copy)]
pub struct CmdInjectionPattern;

impl VulnerabilityPattern for CmdInjectionPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        let re = CMDI_RE.get_or_init(|| {
            // CWE-78: the shell-metachar payloads (`; rm`, `| ncat`, `&& curl`)
            // PLUS the dynamic shell-exec class the old pattern missed: a Node
            // `execSync`/`exec` of a TEMPLATE LITERAL with `${...}` interpolation,
            // a Python `os.system(f"…")` / string concat, and a subprocess with
            // `shell=True` / `{shell: true}`. The rule keys on the INTERPOLATION /
            // opt-in, so the safe forms do NOT match: a static `execSync("git
            // status")`, a JS `regex.exec(str)` (the prior false positive), the
            // safe `execFileSync` argv form, and the security tooling's own
            // `execSync($$$)` / `os.system($$$)` pattern-catalog literals. Rust
            // `Command::new("sh").arg("-c")` is intentionally NOT flagged — that
            // is the CEG sandbox executor running shell in isolation by design.
            Regex::new(
                r#"(;\s*rm\s|\|\s*ncat\s|&&\s*curl\s|\b(?:execSync|exec)\s*\(\s*`[^`]*\$\{|os\.system\s*\(\s*f["']|os\.system\s*\([^)]*\+|subprocess\.\w+\([^)]*shell\s*=\s*True|\bshell\s*:\s*true)"#,
            )
            .expect("valid static regex")
        });
        re.find_iter(input)
            .map(|m| VulnMatch::new("CMDi".into(), (m.start(), m.end()), 9.3, 78))
            .collect()
    }
    fn name(&self) -> &str {
        "CMDi"
    }
    fn severity(&self) -> f32 {
        9.3
    }
    fn cwe_id(&self) -> u32 {
        78
    }
}

// ---------------------------------------------------------------------------
// Path Traversal — CWE-22
// ---------------------------------------------------------------------------
/// True when a regex match at `match_start` sits inside a Rust compile-time
/// include macro argument (`include_str!`/`include_bytes!`/`concat!`/`env!`/
/// `option_env!`). Such a `../` is a build-time literal, never untrusted input,
/// so it is not a CWE-22 traversal. Bounded backward scan (the `regex` crate has
/// no lookbehind): the macro opener must be within 64 bytes and still open (no
/// `)` or newline between the opener and the match).
fn in_compile_time_include(input: &str, match_start: usize) -> bool {
    const MACROS: [&str; 5] = [
        "include_str!(",
        "include_bytes!(",
        "concat!(",
        "env!(",
        "option_env!(",
    ];
    let before = &input[..match_start];
    MACROS.iter().any(|m| {
        before.rfind(m).is_some_and(|pos| {
            let after_opener = &before[pos + m.len()..];
            after_opener.len() <= 64 && !after_opener.contains(')') && !after_opener.contains('\n')
        })
    })
}

/// True when a regex match at `match_start` sits inside a Markdown link target
/// — the `(...)` half of `[text](../../path)` — or a bare Markdown reference
/// definition (`[label]: ../../path`).
///
/// A relative link in prose is a *document* path resolved by a renderer, never
/// untrusted input reaching a filesystem API, so it is not a CWE-22 traversal.
/// Climbing two or more levels is idiomatic in `.github/`, `docs/` and monorepo
/// READMEs (`[SECURITY.md](../../SECURITY.md)`), which made this the single
/// largest FP source for this pattern: it fires on ANY file the scanner reads,
/// including `.md` and `.yml`, where CWE-22 cannot apply at all.
///
/// Bounded backward scan, mirroring [`in_compile_time_include`] (the `regex`
/// crate has no lookbehind): the opener must be within 256 bytes — enough for a
/// long link label — and still open (no `)` or newline in between).
fn in_markdown_link(input: &str, match_start: usize) -> bool {
    const MAX_LABEL: usize = 256;
    let before = &input[..match_start];
    // Inline link: `[label](target`
    let inline = before.rfind("](").is_some_and(|pos| {
        let after_opener = &before[pos + 2..];
        after_opener.len() <= MAX_LABEL
            && !after_opener.contains(')')
            && !after_opener.contains('\n')
    });
    // Reference definition: `[label]: target` (start of line, allowing indent)
    let reference = before.rfind("]: ").is_some_and(|pos| {
        let after_opener = &before[pos + 3..];
        after_opener.len() <= MAX_LABEL && !after_opener.contains('\n')
    });
    inline || reference
}

/// True when the match is the value of a `path:` field inside a macro invoked
/// with a brace body — `wit_bindgen::generate!({ path: "../../x.wit", … })`.
/// The macro reads that file while compiling; the literal never reaches a
/// filesystem call at run time (cross-audit R2: three `holon-wasm-components`
/// crates failed F2.1 on it). Bounded backward scan like
/// [`in_compile_time_include`]: the `!{`/`!({` opener within 512 bytes, with
/// no `}` between it and the field.
fn in_macro_path_field(input: &str, match_start: usize) -> bool {
    const MAX_SPAN: usize = 512;
    let before = &input[..match_start];
    let Some(key) = before
        .strip_suffix('"')
        .map(str::trim_end)
        .and_then(|b| b.strip_suffix(':'))
        .map(str::trim_end)
    else {
        return false;
    };
    let is_path_key = key.strip_suffix("path").is_some_and(|prefix| {
        !prefix
            .bytes()
            .last()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
    });
    is_path_key
        && ["!({", "!{"].iter().any(|opener| {
            before.rfind(opener).is_some_and(|pos| {
                let body = &before[pos..];
                body.len() <= MAX_SPAN && !body.contains('}')
            })
        })
}

/// True when the match is a symlink target in a directory listing:
/// `│   ├── touring -> ../../../bin/touring` (`tree`) or
/// `lrwxrwxrwx … touring -> ../../bin/touring` (`ls -l`). A listing documents a
/// layout; nothing opens the path it prints.
fn in_symlink_listing(input: &str, match_start: usize) -> bool {
    let line_start = input[..match_start].rfind('\n').map_or(0, |p| p + 1);
    let line = &input[line_start..match_start];
    let head = line.trim_start();
    line.ends_with("-> ") && (head.starts_with(['│', '├', '└']) || head.starts_with("lrwx"))
}

/// `at` is where a URL VALUE begins: the nearest non-blank byte before it is a
/// quote, `=` or `(` (an attribute or query parameter, a string, a call argument,
/// a Markdown link), or the line has nothing before it (a bare payload).
///
/// `javascript:` is a URL scheme and a scheme exists only at the start of the URL
/// it names. After a word it is prose: `log("rota javascript: …")` talks ABOUT the
/// scheme and the P0 gate (F2.1) blocked a Python file as XSS for it (touring-4f,
/// 09/10/2026). The test is positional on purpose — every sink this arm exists for
/// (`href="javascript:…"`, `location = 'javascript:' + x`, `window.open("javascript:…")`,
/// a payload input) puts the scheme at a value start, so no true positive moves.
/// A scheme BLOCKLIST literal (`url.startswith("javascript:")`) also starts a value;
/// [`is_scheme_blocklist_literal`] judges that one.
fn starts_url_value(input: &str, at: usize) -> bool {
    let before = input[..at].trim_end_matches([' ', '\t']);
    match before.bytes().next_back() {
        None | Some(b'\n') => true,
        Some(b) => matches!(b, b'"' | b'\'' | b'`' | b'=' | b'('),
    }
}

/// True when a regex match opens with the `javascript:` scheme in ANY letter case.
/// ASCII fold only: a URL scheme is case-insensitive to the browser, and ASCII.
fn is_javascript_scheme(text: &str) -> bool {
    text.as_bytes()
        .get(..11)
        .is_some_and(|head| head.eq_ignore_ascii_case(b"javascript:"))
}

/// Methods whose direct string argument is a scheme CHECK, not a URL in use.
const SCHEME_CHECK_CALLS: [&str; 6] = [
    "startswith",
    "startsWith",
    "endswith",
    "endsWith",
    "includes",
    "indexOf",
];
/// How far a blocklist judgement reads around the literal (a literal and its
/// neighbours are short; the bound keeps the cost per match constant).
const BLOCKLIST_WINDOW: usize = 2048;
/// Most members of a tuple/list walked back to its opening bracket.
const BLOCKLIST_MEMBERS: usize = 32;

/// The `javascript:` literal at `at` is DEFENSIVE code naming the scheme to refuse
/// it, not a URL in use (F2.1, 09/10/2026): `url.startswith("javascript:")`,
/// `u.startsWith('javascript:')`, `scheme == "javascript:"` (`===`, `!=`, `!==`, the
/// literal on either side), `s.startswith(("javascript:", "data:"))`, `x in
/// ("javascript:", …)`. Narrow and positional on purpose — the literal is
///
/// - the direct argument of [`SCHEME_CHECK_CALLS`], or a member of the tuple/list
///   handed to one;
/// - an operand of an equality comparison;
/// - a member of a tuple/list/set right after `in` (a `for … in` loop is not a
///   membership test and keeps reporting).
///
/// Every other position keeps reporting: `el.href = "javascript:" + x`,
/// `go("javascript:alert(1)")` on a line that also compares something, a ternary
/// branch. Not judged: `"javascript:" in url`, a constant list declared apart from
/// its use, a regex literal (`/^javascript:/`, which the value-start rule already
/// leaves alone).
fn is_scheme_blocklist_literal(input: &str, at: usize) -> bool {
    let b = input.as_bytes();
    let Some(open) = prev_non_blank(b, at, false).filter(|&q| is_quote(b[q])) else {
        return false;
    };
    is_equality_operand(b, open, at) || is_scheme_check_member(b, open)
}

fn is_quote(c: u8) -> bool {
    matches!(c, b'"' | b'\'' | b'`')
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Index of the last byte before `end` that is not a blank (`newlines`: nor a line break).
fn prev_non_blank(b: &[u8], end: usize, newlines: bool) -> Option<usize> {
    (end.saturating_sub(BLOCKLIST_WINDOW)..end)
        .rev()
        .find(|&i| !(matches!(b[i], b' ' | b'\t') || (newlines && matches!(b[i], b'\n' | b'\r'))))
}

/// `scheme == "javascript:"` / `!=` / `===` / `!==` (operator before the literal that
/// opens at `open`), or `"javascript:" == scheme` (operator after the closing quote).
fn is_equality_operand(b: &[u8], open: usize, at: usize) -> bool {
    if let Some(p) = prev_non_blank(b, open, false)
        && p >= 1
        && b[p] == b'='
        && matches!(b[p - 1], b'=' | b'!')
    {
        return true;
    }
    let quote = b[open];
    let tail = &b[at..b.len().min(at + BLOCKLIST_WINDOW)];
    let Some(close) = tail.iter().position(|&c| c == quote || c == b'\n') else {
        return false;
    };
    if tail[close] != quote {
        return false;
    }
    let after = tail[close + 1..].trim_ascii_start();
    after.starts_with(b"==") || after.starts_with(b"!=")
}

/// The literal that opens at `open` is a member of a tuple/list/set whose opening
/// bracket follows `in`, or is the direct argument (or a member of the tuple/list
/// argument) of a [`SCHEME_CHECK_CALLS`] call. Walks back over earlier string members.
fn is_scheme_check_member(b: &[u8], open: usize) -> bool {
    let mut pos = open;
    for _ in 0..BLOCKLIST_MEMBERS {
        let Some(p) = prev_non_blank(b, pos, true) else {
            return false;
        };
        match b[p] {
            b',' => {
                let Some(end) = prev_non_blank(b, p, true).filter(|&c| is_quote(b[c])) else {
                    return false;
                };
                let Some(start) = literal_start(b, end) else {
                    return false;
                };
                pos = start;
            }
            b'(' | b'[' | b'{' => return bracket_opens_scheme_check(b, p),
            _ => return false,
        }
    }
    false
}

/// Index of the quote that opens the literal closing at `close` (same line, unescaped).
fn literal_start(b: &[u8], close: usize) -> Option<usize> {
    let quote = b[close];
    (close.saturating_sub(BLOCKLIST_WINDOW)..close)
        .rev()
        .take_while(|&i| b[i] != b'\n')
        .find(|&i| b[i] == quote && (i == 0 || b[i - 1] != b'\\'))
}

/// The bracket at `bracket` holds a scheme blocklist: `in (`, `in [`, `in {` (not a
/// `for` target) or a [`SCHEME_CHECK_CALLS`] call, directly or with a tuple/list inside.
fn bracket_opens_scheme_check(b: &[u8], bracket: usize) -> bool {
    let Some(before) = prev_non_blank(b, bracket, true) else {
        return false;
    };
    let (word, word_start) = trailing_word(b, before);
    if word == "in" {
        return !follows_for_target(b, word_start);
    }
    if b[bracket] == b'(' && SCHEME_CHECK_CALLS.contains(&word) {
        return true;
    }
    b[before] == b'('
        && prev_non_blank(b, before, true)
            .is_some_and(|c| SCHEME_CHECK_CALLS.contains(&trailing_word(b, c).0))
}

/// The identifier that ends at `end` (inclusive) and where it starts; empty when
/// `b[end]` is not an identifier byte. Capped so one long run costs a constant.
fn trailing_word(b: &[u8], end: usize) -> (&str, usize) {
    let mut start = end + 1;
    while start > 0 && end + 1 - start < 64 && is_ident(b[start - 1]) {
        start -= 1;
    }
    // Identifier bytes are ASCII, so the slice is valid UTF-8.
    (std::str::from_utf8(&b[start..=end]).unwrap_or(""), start)
}

/// The `in` that starts at `in_start` belongs to a `for <target> in` loop or
/// comprehension (the text from the last `for` on the line is a bare target).
fn follows_for_target(b: &[u8], in_start: usize) -> bool {
    let floor = in_start.saturating_sub(BLOCKLIST_WINDOW);
    let line_start = b[floor..in_start]
        .iter()
        .rposition(|&c| c == b'\n')
        .map_or(floor, |p| floor + p + 1);
    let seg = &b[line_start..in_start];
    let Some(f) = (0..seg.len().saturating_sub(2)).rev().find(|&i| {
        seg[i..i + 3] == *b"for"
            && (i == 0 || !is_ident(seg[i - 1]))
            && seg.get(i + 3).is_none_or(|&c| !is_ident(c))
    }) else {
        return false;
    };
    seg[f + 3..]
        .iter()
        .all(|&c| is_ident(c) || matches!(c, b',' | b' ' | b'\t' | b'(' | b')'))
}

/// The whole value of the `javascript:` URL at `at` is an INERT constant (F2.1,
/// 10/10/2026): a no-op or a history/print navigation, no data in it.
/// `href="javascript:;"`, `href="javascript:void(0)"`, `javascript:history.back()` and
/// `javascript:this.print();` are in 16 captured HTML pages (portal reports, TSE, ANTT
/// legislation); no untrusted data reaches them, so there is no XSS to report.
///
/// Positional and closed on purpose: the value must be delimited (a quote, or an
/// unquoted attribute after `=`), its body must be one of the forms of
/// [`INERT_URL_BODY_RE`], and the value must not go on past the delimiter (`+`, `%`,
/// `.format(`, an adjacent literal). A constant that is not inert — `alert(1)`,
/// `alert(document.cookie)`, a call into page code — keeps reporting: a constant
/// payload and a constant site function look alike here, and `alert(…)` is the
/// canonical proof of concept. Not judged: a URL opened by `(` (Markdown link, CSS
/// `url(`) and the bare payload at the start of an input.
fn is_inert_constant_url(input: &str, at: usize) -> bool {
    let b = input.as_bytes();
    let Some(opener) = prev_non_blank(b, at, false) else {
        return false;
    };
    let body_start = at + "javascript:".len();
    let tail = &b[body_start..b.len().min(body_start + BLOCKLIST_WINDOW)];
    let (body, end) = if is_quote(b[opener]) {
        let Some(close) = tail.iter().position(|&c| c == b[opener]) else {
            return false;
        };
        // `\"javascript:void(0)\"` in a string the program writes: the backslash
        // belongs to the delimiter.
        let body = tail[..close].strip_suffix(b"\\").unwrap_or(&tail[..close]);
        (body, body_start + close + 1)
    } else if b[opener] == b'=' {
        let close = tail
            .iter()
            .position(|&c| c.is_ascii_whitespace() || matches!(c, b'>' | b'"' | b'\''))
            .unwrap_or(tail.len());
        // The delimiter ENDS the value: what the check below reads is what comes
        // AFTER it (a blank, `>`, a quote). Pointing at the delimiter itself made
        // the delimiter look like a continuation (`s = "x=javascript:;"`).
        (&tail[..close], (body_start + close + 1).min(b.len()))
    } else {
        return false;
    };
    !value_continues(b, end) && is_inert_url_body(body)
}

/// The value that ends at `end` CONTINUES: the data that reaches the URL is not in
/// the constant — a concatenation (`+`, `%`, `.format(`), an adjacent literal, a
/// comment or a line continuation in between.
///
/// When the next token is a statement boundary (`;`, `,`, `)`, `]`, `}`, `>`), the
/// end of the input, or another attribute (`nome=`), the value ENDED: the constant
/// is the whole value.
///
/// Anything else is NOT understood, and the conservative answer is that the value
/// continues: a scanner that fails OPEN (a sink missed) is worse than one that
/// reports too much.
fn value_continues(b: &[u8], end: usize) -> bool {
    let Some(corte) = b.get(end..) else {
        return false;
    };
    let Some(pos) = corte.iter().position(|c| !c.is_ascii_whitespace()) else {
        return false; // the end of the input: the value ended
    };
    let i = end + pos;
    let first = b[i];
    if matches!(first, b'+' | b'%' | b'.' | b'"' | b'\'') {
        return true;
    }
    // the statement ended (or the tag did): the constant is the whole value
    if matches!(first, b';' | b',' | b')' | b']' | b'}' | b'>') {
        return false;
    }
    if first == b'/' {
        // `//` or `/*`: a comment — the value may continue past it.
        // `/` before `>`: the tag ends — the value ended.
        return !matches!(b.get(i + 1).copied(), Some(b'>'));
    }
    if first.is_ascii_alphabetic() || first == b'_' {
        // an identifier: the prefix of a literal (an adjacent literal — the value
        // continues) or the name of the next attribute (the value ended)
        let mut j = i;
        while b
            .get(j)
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        {
            j += 1;
        }
        return match b.get(j).copied() {
            Some(b'"') | Some(b'\'') => true, // an adjacent literal
            Some(b'=') => false,              // the next attribute: the value ended
            _ => true,                        // not understood: the value may continue
        };
    }
    // not understood (a line continuation, a backtick, an operator…): the value
    // may continue — the sink is reported, never missed
    true
}

/// `body` (a `javascript:` URL's script, blanks ignored, one trailing `;` allowed) is
/// empty, `void(0)`, `history.back()`/`forward()`/`go()` with an optional integer
/// (`window.` prefix allowed) or `this.print()` — every body read in the captured pages
/// that carries no data.
fn is_inert_url_body(body: &[u8]) -> bool {
    let compact: Vec<u8> = body
        .iter()
        .copied()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    let Ok(text) = std::str::from_utf8(&compact) else {
        return false;
    };
    INERT_URL_BODY_RE
        .get_or_init(|| {
            Regex::new(
                r"^(?:;?|void(?:\(0\)|0);?|(?:window\.)?history\.(?:back|forward|go)\((?:-?\d{1,3})?\);?|this\.print\(\);?)$",
            )
            .expect("valid static regex")
        })
        .is_match(text)
}

/// True when the `<script` at `at` belongs to an HTML document the program
/// writes: an `<html` or `<!doctype html` opener precedes it, with no `</html>`
/// in between. A generated page's own scripts are that page's code. XSS is a
/// script injected into markup it does not belong to — and inside a template
/// the injection is decided by the escaping around each interpolation, which
/// this regex never evaluated. `client/omarchy/bin/cc_build.py` (every value
/// passes through `html.escape`) failed F2.1 on its page's `<script>{_JS}</script>`
/// (cross-audit R2, 14/09/2026). Bounded backward scan of 64 KiB.
fn in_html_document_template(input: &str, at: usize) -> bool {
    const WINDOW: usize = 64 * 1024;
    let mut from = at.saturating_sub(WINDOW);
    while !input.is_char_boundary(from) {
        from += 1;
    }
    let before = input[from..at].to_ascii_lowercase();
    let opener = before.rfind("<html").max(before.rfind("<!doctype html"));
    opener.is_some_and(|pos| !before[pos..].contains("</html>"))
}

/// True when the `<script>` at `start..end` is the placeholder of a
/// command-line flag in usage text — `touring run --file <script>` — rather
/// than an HTML tag: it follows a `--flag ` / `-f ` / `--flag=` token and no
/// script body starts right after the `>`.
fn is_cli_placeholder(input: &str, start: usize, end: usize) -> bool {
    if &input[start..end] != "<script>"
        || input[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '<')
    {
        return false;
    }
    let line_start = input[..start].rfind('\n').map_or(0, |p| p + 1);
    let line = &input[line_start..start];
    let Some(flag_part) = line.strip_suffix(' ').or_else(|| line.strip_suffix('=')) else {
        return false;
    };
    let token = flag_part.rsplit(char::is_whitespace).next().unwrap_or("");
    let name = token
        .strip_prefix("--")
        .or_else(|| token.strip_prefix('-'))
        .unwrap_or("");
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Detects path traversal (CWE-22): multi-level `../../` climbs and URL-encoded dot-dot sequences.
#[derive(Debug, Clone, Copy)]
pub struct PathTraversalPattern;

impl VulnerabilityPattern for PathTraversalPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        let re = PATH_TRAV_RE.get_or_init(|| {
            // CWE-22: the attack signature is a MULTI-LEVEL climb (`../../`, the
            // Windows `..\..\`) or ANY URL-encoded dot-dot (encoding has no benign
            // reason — it is an evasion signal). A single `../` is a normal
            // relative path (sibling import); the old single-`../` pattern flagged
            // 260 benign workspace lines, so requiring 2+ literal climbs cuts that
            // ~81%. Residual FP: compile-time `include_str!("../../x")` macro
            // literals — a future AST-aware pass excludes those (regex cannot:
            // the `regex` crate has no lookbehind).
            Regex::new(r"((\.\./){2,}|(\.\.\\){2,}|%2e%2e%2f|%2e%2e/|\.\.%2f|\.\.%5c)")
                .expect("valid static regex")
        });
        // Report the first genuine traversal, skipping `../` literals that sit
        // inside compile-time include macros (build-time constants, not CWE-22)
        // or Markdown link targets (document paths resolved by a renderer).
        re.find_iter(input)
            .filter(|m| {
                !in_compile_time_include(input, m.start())
                    && !in_markdown_link(input, m.start())
                    && !in_macro_path_field(input, m.start())
                    && !in_symlink_listing(input, m.start())
            })
            .map(|m| VulnMatch::new("PathTraversal".into(), (m.start(), m.end()), 8.0, 22))
            .collect()
    }
    fn name(&self) -> &str {
        "PathTraversal"
    }
    fn severity(&self) -> f32 {
        8.0
    }
    fn cwe_id(&self) -> u32 {
        22
    }
}

// ---------------------------------------------------------------------------
// Integer Overflow — CWE-190
// ---------------------------------------------------------------------------
/// Detects integer overflow boundary markers (CWE-190) — the 32-bit `INT_MAX` constant.
#[derive(Debug, Clone, Copy)]
pub struct IntegerOverflowPattern;

impl VulnerabilityPattern for IntegerOverflowPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        // CWE-190 needs dataflow (unchecked arithmetic near a boundary) which a
        // regex cannot do; this is a deliberately narrow PRESENCE heuristic for
        // the 32-bit overflow boundary `INT_MAX` / `0x7fffffff`. `\b` anchors
        // `INT_MAX` so `MY_INT_MAX`/`UINT_MAX` are not matched. (0 FP measured
        // over the workspace; Rust uses `i32::MAX`, not the C macro.)
        let re = INT_OVF_RE
            .get_or_init(|| Regex::new(r"(\bINT_MAX\b|0x7fffffff)").expect("valid static regex"));
        re.find_iter(input)
            .map(|m| VulnMatch::new("IntegerOverflow".into(), (m.start(), m.end()), 7.5, 190))
            .collect()
    }
    fn name(&self) -> &str {
        "IntegerOverflow"
    }
    fn severity(&self) -> f32 {
        7.5
    }
    fn cwe_id(&self) -> u32 {
        190
    }
}

// ---------------------------------------------------------------------------
// Buffer Overflow — CWE-121
// ---------------------------------------------------------------------------
/// Detects stack buffer overflow risk (CWE-121): calls to unbounded C string functions.
#[derive(Debug, Clone, Copy)]
pub struct BufferOverflowPattern;

impl VulnerabilityPattern for BufferOverflowPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        // CWE-121: unbounded C string functions, matched as CALLS (a trailing
        // `(`) rather than as bare mentions — so the word `sprintf` in prose/docs
        // is not flagged, only `sprintf(...)`. `\b` anchors the name so the safe
        // bounded `snprintf`/`strncpy` are not matched. Added the classic unsafe
        // `strcat`/`vsprintf`/`gets` for fuller CWE-121 coverage.
        let re = BUF_OVF_RE.get_or_init(|| {
            Regex::new(r"\b(strcpy|strcat|sprintf|vsprintf|gets)\s*\(").expect("valid static regex")
        });
        re.find_iter(input)
            .map(|m| VulnMatch::new("BufferOverflow".into(), (m.start(), m.end()), 9.1, 121))
            .collect()
    }
    fn name(&self) -> &str {
        "BufferOverflow"
    }
    fn severity(&self) -> f32 {
        9.1
    }
    fn cwe_id(&self) -> u32 {
        121
    }
}

// ---------------------------------------------------------------------------
// Deserialization — CWE-502
// ---------------------------------------------------------------------------
/// Detects unsafe deserialization constructs (CWE-502) such as `pickle.loads` and `yaml.load`.
#[derive(Debug, Clone, Copy)]
pub struct DeserializationPattern;

impl VulnerabilityPattern for DeserializationPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        let re = DESER_RE
            .get_or_init(|| Regex::new(r"(pickle\.loads|yaml\.load)").expect("valid static regex"));
        re.find_iter(input)
            .map(|m| VulnMatch::new("Deserialization".into(), (m.start(), m.end()), 9.0, 502))
            .collect()
    }
    fn name(&self) -> &str {
        "Deserialization"
    }
    fn severity(&self) -> f32 {
        9.0
    }
    fn cwe_id(&self) -> u32 {
        502
    }
}

// ---------------------------------------------------------------------------
// SSRF — CWE-918
// ---------------------------------------------------------------------------
/// Detects server-side request forgery (CWE-918): cloud-metadata endpoints and SSRF-only URL schemes.
#[derive(Debug, Clone, Copy)]
pub struct SsrfPattern;

impl VulnerabilityPattern for SsrfPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        let re = SSRF_RE.get_or_init(|| {
            // CWE-918: high-signal SSRF markers (OWASP SSRF Prevention deny-list):
            // the cloud-metadata endpoints (`169.254.169.254`, AWS/GCP metadata
            // domains — no benign reason for app code to name them) and the
            // SSRF-exclusive non-HTTP schemes (`gopher://`/`dict://`/`phar://`,
            // and a `file://` URL with `${...}` interpolation = user-controlled
            // local file in a request). Bare `http://127.0.0.1` and bare `file://`
            // were DROPPED — they flagged 37 benign workspace lines (the local
            // Touring daemon's own address + file-URI references); loopback/private
            // SSRF from untrusted input needs taint-tracking, not a substring.
            Regex::new(
                r#"(169\.254\.169\.254|metadata\.(amazonaws\.com|google\.internal)|gopher://|dict://|phar://|file://[^\s"'`]*\$\{)"#,
            )
            .expect("valid static regex")
        });
        re.find_iter(input)
            .map(|m| VulnMatch::new("SSRF".into(), (m.start(), m.end()), 8.6, 918))
            .collect()
    }
    fn name(&self) -> &str {
        "SSRF"
    }
    fn severity(&self) -> f32 {
        8.6
    }
    fn cwe_id(&self) -> u32 {
        918
    }
}

// ---------------------------------------------------------------------------
// LDAP Injection — CWE-90
// ---------------------------------------------------------------------------
/// Detects LDAP injection constructs (CWE-90) such as filter wildcards and `cn=` fragments.
#[derive(Debug, Clone, Copy)]
pub struct LdapInjectionPattern;

impl VulnerabilityPattern for LdapInjectionPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        // Precision history: a bare `\)` matched EVERY closing paren (flagged all
        // code, constant 0.220). It was tightened to `\*\)` — but a bare `*)` still
        // matches benign regex quantifier-close syntax `(…[A-Za-z0-9]*):` in any
        // source carrying a regex literal (e.g. file:line extractors), a universal
        // FP. Real CWE-90 injection needs LDAP FILTER CONTEXT around the wildcard:
        //   `=*)`  — closing an `attr=*` filter (e.g. `(name=*)`)
        //   `*)(` / `*))` — breaking out to inject/append a filter clause
        // A lone `*)` with no `=`/`(`/`)` neighbour is not a usable LDAP payload.
        // The `cn=` filter-fragment arm is preserved.
        //
        // 14/09/2026 (cross-audit D3): the breakout arm `\*\)[()]` still matched
        // every C pointer cast followed by a parenthesis — `(void *)(self)`,
        // `sizeof(char *))` — and flagged tree-sitter's `array.h` as CWE-90 at
        // 0.220. A filter breakout is followed by FILTER syntax: an operator
        // (`*)(|`, `*)(&`, `*)(!`) or an appended `attr=` clause (`*)(uid=`).
        //
        // Cross-audit R2 (14/09/2026): the `=*)` arm closed ANY `x=*)`. A shell
        // `case` glob (`--output=*)`) and prose (`(memory tier=*)`) are not a
        // filter; the filter is `(attr=*)`, opened right before the attribute.
        let re = LDAPI_RE.get_or_init(|| {
            Regex::new(r"(\([\w.-]+=\*\)|\*\)\([|&!]|\*\)\(\w+=|cn=)").expect("valid static regex")
        });
        re.find_iter(input)
            .map(|m| VulnMatch::new("LDAPi".into(), (m.start(), m.end()), 7.8, 90))
            .collect()
    }
    fn name(&self) -> &str {
        "LDAPi"
    }
    fn severity(&self) -> f32 {
        7.8
    }
    fn cwe_id(&self) -> u32 {
        90
    }
}

// ---------------------------------------------------------------------------
// XML Injection — CWE-91
// ---------------------------------------------------------------------------
/// Detects XML external-entity / injection (CWE-91/XXE): `<!ENTITY` declarations and DOCTYPE internal subsets.
#[derive(Debug, Clone, Copy)]
pub struct XmlInjectionPattern;

impl VulnerabilityPattern for XmlInjectionPattern {
    fn detect(&self, input: &str) -> Option<VulnMatch> {
        self.detect_every(input).into_iter().next()
    }

    fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        let re = XML_INJ_RE.get_or_init(|| {
            // CWE-91/XXE: the attack is a custom ENTITY declaration or a DOCTYPE
            // with an internal subset `[` (where external entities live — OWASP
            // XXE Prevention: "disallow DOCTYPE declarations"). Bare `<!DOCTYPE`
            // was DROPPED: `<!DOCTYPE html>` is a benign HTML5 doctype (it flagged
            // `touring-web/index.html`); bare `<![CDATA` is a legit XML construct,
            // not injection. Requiring `<!ENTITY` or a DOCTYPE-with-`[` keeps the
            // real XXE payload while clearing the HTML doctype FP.
            Regex::new(r"(<!ENTITY|<!DOCTYPE[^>]*\[)").expect("valid static regex")
        });
        re.find_iter(input)
            .map(|m| VulnMatch::new("XMLInjection".into(), (m.start(), m.end()), 7.2, 91))
            .collect()
    }
    fn name(&self) -> &str {
        "XMLInjection"
    }
    fn severity(&self) -> f32 {
        7.2
    }
    fn cwe_id(&self) -> u32 {
        91
    }
}

// ---------------------------------------------------------------------------
// PatternRegistry
// ---------------------------------------------------------------------------
/// Collection of registered vulnerability detectors run together against an input.
#[derive(Debug)]
pub struct PatternRegistry {
    patterns: Vec<Box<dyn VulnerabilityPattern>>,
}

impl PatternRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Registers a new pattern. The registry takes ownership of the box.
    pub fn register(&mut self, pattern: Box<dyn VulnerabilityPattern>) {
        self.patterns.push(pattern);
    }

    /// Runs all registered patterns against `input` and returns every match.
    pub fn detect_all(&self, input: &str) -> Vec<VulnMatch> {
        self.patterns
            .iter()
            .filter_map(|p| p.detect(input))
            .collect()
    }

    /// Every match of every registered pattern, pattern by pattern in input
    /// order. [`Self::detect_all`] keeps one match per pattern; a caller that
    /// filters matches by region needs them all.
    pub fn detect_every(&self, input: &str) -> Vec<VulnMatch> {
        self.patterns
            .iter()
            .flat_map(|p| p.detect_every(input))
            .collect()
    }

    /// Returns the number of registered patterns.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Returns true if no patterns are registered.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Returns a reference to the internal pattern list.
    ///
    /// Enables iteration over registered patterns without consuming the registry.
    pub fn patterns(&self) -> &Vec<Box<dyn VulnerabilityPattern>> {
        &self.patterns
    }

    /// Returns a registry pre-loaded with all 10 CWE patterns (OWASP Top 10 + CWE Top 25).
    pub fn all() -> Self {
        let mut reg = Self::new();
        reg.register(Box::new(SqlInjectionPattern));
        reg.register(Box::new(XssPattern));
        reg.register(Box::new(CmdInjectionPattern));
        reg.register(Box::new(PathTraversalPattern));
        reg.register(Box::new(IntegerOverflowPattern));
        reg.register(Box::new(BufferOverflowPattern));
        reg.register(Box::new(DeserializationPattern));
        reg.register(Box::new(SsrfPattern));
        reg.register(Box::new(LdapInjectionPattern));
        reg.register(Box::new(XmlInjectionPattern));
        reg
    }
}

impl Default for PatternRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqli_or_pattern() {
        let p = SqlInjectionPattern;
        // TP — tautology, comment-out, UNION-based.
        assert!(p.detect("' OR '1'='1").is_some());
        assert!(p.detect("'; --").is_some());
        assert!(p.detect("UNION SELECT").is_some());
        // FP — benign SQL / Rust code.
        assert!(p.detect("SELECT * FROM users").is_none());
        assert!(p.detect("let x = a || b;").is_none()); // Rust OR, no quotes
        assert!(p.detect("// comment -- here").is_none()); // `--` without leading `;`
        // Regression (14/09/2026): a CSS custom property after a dict subscript in
        // an f-string is not a comment injection — an identifier follows the `--`.
        assert!(
            p.detect(r#"f"--bg:{lt['background']};--fg:{lt['foreground']};""#)
                .is_none()
        );
        // The comment-out payloads it exists for still match.
        assert!(p.detect("admin'; -- ").is_some());
        assert!(p.detect("x' OR 1=1; --\n").is_some());
        assert!(p.detect("id = '1'; --'").is_some());
        assert_eq!(p.name(), "SQLi");
        assert_eq!(p.cwe_id(), 89);
    }

    #[test]
    fn test_xss_pattern() {
        let p = XssPattern;
        // TP — real XSS vectors (OWASP XSS Filter Evasion Cheat Sheet).
        assert!(p.detect("<script>").is_some());
        assert!(p.detect("<script src=\"evil.js\">").is_some());
        assert!(p.detect("javascript:").is_some());
        // B3 (02/09/2026): a bare `onerror=` fragment is no longer a sink — an
        // event-handler attribute executes only inside a tag; bare, it is the
        // shape of a keyword argument (`os.walk(top, onerror=…)`).
        assert!(p.detect("onerror=").is_none());
        assert!(p.detect("<img src=x onerror=alert(1)>").is_some());
        assert!(p.detect("<svg/onload=alert(1)>").is_some());
        assert!(p.detect("<body onload=\"evil()\">").is_some());
        assert!(p.detect("<a href=\"javascript:alert(1)\">").is_some());
        assert!(p.detect("<div onmouseover=\"steal()\">").is_some());
        assert!(p.detect("<details ontoggle=alert(1)>").is_some());
        assert!(p.detect("<input onfocus=alert(1) autofocus>").is_some());
        // FP — must NOT match: React/JSX camelCase props, minified-JS camelCase
        // assignments, and incidental `on...` words. (Precision: 0 prod FP over
        // the workspace; the old `on\w+=` flagged 131 such lines.)
        assert!(p.detect("<button onClick={handleClick}>").is_none());
        assert!(p.detect("onChange={setValue}").is_none());
        assert!(p.detect("onUpdate=callback").is_none()); // vendor JS (three.js)
        assert!(p.detect("oneMapping=value").is_none()); // minified
        assert!(p.detect("let onConstant = 5;").is_none());
        assert!(p.detect("const online = true;").is_none());
        assert!(p.detect("console.log(\"monitor=on\")").is_none());
        assert!(p.detect("<div class=\"foo\">").is_none());
        // B3 (02/09/2026): a keyword ARGUMENT named like a DOM event is not a
        // sink — `os.walk(top, onerror=…)` in a diagnostic script was blocked by
        // the P0 gate as XSS. Only an attribute inside an OPEN tag counts, and
        // the tag may have been opened on an earlier line.
        assert!(
            p.detect("for root, dirs, files in os.walk(path, onerror=lambda e: None):")
                .is_none()
        );
        assert!(p.detect("urlopen(url, onerror=handler)").is_none());
        assert!(p.detect("<img src=x\n     onerror=alert(1)>").is_some());
        assert!(p.detect("let v: Vec<u8> = f(onerror=1);").is_none());
        assert_eq!(p.name(), "XSS");
        assert_eq!(p.cwe_id(), 79);
    }

    /// F2.1 false positive (09/10/2026, touring-4f): a Python file whose only
    /// "XSS" was the words `javascript:` inside a log message was blocked by the
    /// P0 gate. `javascript:` is a URL scheme, and a scheme exists only where a
    /// URL VALUE begins — after a quote, `=`, `(`, or at the start of a line.
    /// After a word it is prose. Each clause of `starts_url_value` has a case
    /// below where it is the ONLY reason the match survives (mutation-checked).
    #[test]
    fn test_xss_javascript_scheme_is_a_url_not_prose() {
        let p = XssPattern;
        // FP — the scheme named in prose never starts a URL value.
        assert!(
            p.detect("log(\"rota javascript: handlers desativados\")")
                .is_none()
        );
        assert!(
            p.detect("print(f\"bloqueado: javascript: em {url}\")")
                .is_none()
        );
        assert!(p.detect("# see javascript: URLs below").is_none());
        assert!(p.detect("erro, javascript: nao permitido").is_none());
        // The Rust path separator stays benign (existing arm).
        assert!(p.detect("tree_sitter_javascript::LANGUAGE").is_none());
        // TP — every position where a URL value begins keeps failing.
        assert!(p.detect("javascript:alert(1)").is_some()); // payload input
        assert!(p.detect("x\njavascript:alert(1)").is_some()); // line start
        assert!(p.detect("<a href=\"javascript:alert(1)\">").is_some());
        assert!(p.detect("<a href='javascript:alert(1)'>").is_some());
        assert!(p.detect("<a href=javascript:alert(1)>").is_some()); // unquoted
        assert!(p.detect("<a href = \"  javascript:alert(1)\">").is_some()); // in a tag
        assert!(p.detect("go(\"  javascript:alert(1)\")").is_some()); // spaces skipped
        assert!(p.detect("go(\"\tjavascript:alert(1)\")").is_some()); // tab skipped
        assert!(
            p.detect("url = '/login?next=javascript:alert(1)'")
                .is_some()
        ); // `=` alone
        assert!(p.detect("location = 'javascript:' + x;").is_some());
        assert!(p.detect("window.open(\"javascript:alert(1)\")").is_some());
        assert!(p.detect("[clique](javascript:alert(1))").is_some());
        assert!(p.detect("html = f'<a href=\"javascript:{x}\">'").is_some());
        assert!(p.detect("el.href = `javascript:${x}`").is_some());
    }

    /// A URL scheme is case-insensitive to the browser: `JavaScript:`, `JAVASCRIPT:`
    /// and `jAvAsCrIpT:` execute exactly like the lowercase spelling (CWE-79
    /// bypass, 09/10/2026). Prose stays prose in any case. HTML entities
    /// (`&#106;avascript:`) and a tab inside the word are NOT decoded here.
    #[test]
    fn test_xss_javascript_scheme_is_case_insensitive() {
        let p = XssPattern;
        for scheme in ["JavaScript:", "JAVASCRIPT:", "jAvAsCrIpT:", "Javascript:"] {
            assert!(
                p.detect(&format!("{scheme}alert(1)")).is_some(),
                "{scheme} payload"
            );
            assert!(
                p.detect(&format!("<a href=\"{scheme}alert(1)\">"))
                    .is_some(),
                "{scheme} in an attribute"
            );
            assert!(
                p.detect(&format!("go(\" \t{scheme}alert(1)\")")).is_some(),
                "{scheme} after blanks inside the value"
            );
            assert!(
                p.detect(&format!("el.href = `{scheme}${{x}}`")).is_some(),
                "{scheme} in a template literal"
            );
        }
        assert!(
            p.detect("log(\"rota JavaScript: handlers desativados\")")
                .is_none()
        );
        assert!(
            p.detect("doc = 'TypeScript / JavaScript: export marks the API'")
                .is_none()
        );
        assert!(p.detect("tree_sitter_JavaScript::LANGUAGE").is_none());
    }

    /// F2.1 false positive (09/10/2026): a scheme BLOCKLIST names the scheme to refuse
    /// it — defensive code the gate should encourage, not block. Narrow and positional:
    /// the literal is the direct argument of `startswith(`/`startsWith(`/`endswith(`/
    /// `endsWith(`/`includes(`/`indexOf(` (or a member of the tuple/list passed to
    /// one), an operand of `==`/`===`/`!=`/`!==`, or a member of a tuple/list/set after
    /// `in`. A `for … in (…)` loop and every other position still report.
    #[test]
    fn test_xss_scheme_blocklist_literal_is_not_a_sink() {
        let p = XssPattern;
        for defensive in [
            "return url.lower().startswith(\"javascript:\")",
            "if (u.startsWith('javascript:')) return;",
            "if x.endswith(\"javascript:\"): pass",
            "if (u.endsWith('javascript:')) return;",
            "if (url.includes('javascript:')) throw e;",
            "if (url.indexOf('javascript:') === 0) throw e;",
            "if scheme == \"javascript:\":",
            "if (scheme === 'javascript:') {}",
            "if scheme != \"javascript:\":",
            "if (scheme !== 'javascript:') {}",
            "if \"javascript:\" == scheme:",
            "if 'javascript:' !== scheme {}",
            "s.lower().startswith((\"javascript:\", \"data:\"))",
            "s.lower().startswith((\"data:\", \"javascript:\"))",
            "if x in (\"javascript:\", \"data:\"):",
            "if x not in [\"data:\", \"javascript:\"]:",
            "if x in {\"javascript:\"}:",
            "if x in (\n    \"data:\",\n    \"javascript:\",\n):",
            "if (url.startsWith(`javascript:`)) {}",
            "if url.lower() == \"JavaScript:\":",
            // `a<b` opens a phantom "tag" for the old open-tag rescue; the guard must hold.
            "for (let i=0;i<n;i++) { if (u.startsWith('javascript:')) bad(); }",
            // Prose inside an attribute value is not a URL either.
            "<img alt=\"rota javascript: handlers\">",
        ] {
            assert!(
                p.detect(defensive).is_none(),
                "defensive code reported: {defensive}"
            );
        }
        for sink in [
            "if ok: el.href = \"javascript:\" + x",
            "x == 1 and go(\"javascript:alert(1)\")",
            "redirect(url if url.startswith(\"/\") else \"javascript:alert(1)\")",
            "el.href = x || 'javascript:alert(1)'",
            "img.src = ok ? \"a\" : \"javascript:alert(1)\"",
            "plugin(\"javascript:alert(1)\")",
            "render([\"data:\", \"javascript:alert(1)\"])",
            "x.includes(y) && go(\"javascript:alert(1)\")",
            "for u in (\"javascript:alert(1)\", \"b\"): render(u)",
            "[render(u) for u in (\"javascript:alert(1)\",)]",
            "<a href=javascript:alert(1)>",
            "<a href=\"JavaScript:alert(1)\">",
        ] {
            assert!(p.detect(sink).is_some(), "sink missed: {sink}");
        }
    }

    /// Scheme matches only (an `onclick=` next to the URL is the code judge's business).
    fn scheme_hits(src: &str) -> usize {
        XssPattern
            .detect_every(src)
            .iter()
            .filter(|m| is_javascript_scheme(&src[m.span.0..m.span.1]))
            .count()
    }

    /// F2.1 false positive (10/10/2026): a `javascript:` URL whose body is an INERT
    /// constant (no-op or history/print navigation, no data) is not XSS — no
    /// untrusted data reaches it. The real tag lines come from 16 captured HTML
    /// pages (portal reports, TSE, ANTT legislation); every body below was read there.
    #[test]
    fn test_xss_inert_constant_javascript_url_is_not_a_sink() {
        for inert in [
            r#"<a href="javascript:;" class="link-contraste" id="alto_contraste_id">"#,
            r#"<a href="javascript:;" id="voltar" class="float-right" onclick="window.close()">"#,
            r#"<a href="javascript:void(0)" class="contrast-event">"#,
            r#"<a class="report-error-button" data-toggle="modal" href="javascript:void(0);" >"#,
            r#"<a href="javascript:history.back()">Página Anterior</a>"#,
            r#"<a href="javascript:history.back(-1);">"#,
            r#"<a href="javascript:history.forward(1);" title="Avançar">"#,
            r#"<a class="btn btn-default" href="javascript:history.go(-1);" title="Voltar">"#,
            r#"<a href="javascript:window.history.back();">"#,
            r#"<a class="imprimir-salvar__link" href="javascript:this.print();" role="button">"#,
            // spellings of the same bodies
            r#"<a href='javascript:void(0)'>"#,
            r#"<a href="javascript:void 0">"#,
            r#"<a href="javascript: void( 0 ) ;">"#,
            r#"<a href="JavaScript:void(0)">"#,
            "<a href=javascript:;>",
            "<a href=javascript:void(0)>",
            "<a href=javascript:void(0) class=x>", // an unquoted value ends at a blank
            r#"s = "x=javascript:;""#,             // ... or at a quote
            "s = 'x=javascript:;'",
            "<a href=\"javascript:\n  void(0)\">", // a line break inside the value
            r#"<a href="javascript:">"#,
            // the same attribute inside a string the program writes
            r#"html = "<a href=\"javascript:void(0)\">x</a>""#,
            r#"html = f'<a href="javascript:void(0)">{name}</a>'"#,
            "html = `<a href=\"javascript:void(0)\">${name}</a>`",
            r#"url = "javascript:""#,
            r#"TOKENS = ["PORTARIA", "javascript:", "detalharAto"]"#,
        ] {
            assert_eq!(
                scheme_hits(inert),
                0,
                "inert constant URL reported: {inert}"
            );
        }
    }

    /// What keeps failing: a body with data flow (concatenation, template, format), a
    /// body that is not inert (`alert`, `eval`, a DOM read, a second statement) and the
    /// bare payload at the start of an input.
    #[test]
    fn test_xss_javascript_url_with_data_flow_or_payload_still_reports() {
        for sink in [
            "location = 'javascript:' + x;",
            r#"url = "javascript:void(0)" + suffix"#,
            r#"url = "javascript:" % x"#,
            r#"url = "javascript:".format(x)"#,
            r#"url = "javascript:" f"{x}""#,
            r#"url = "javascript:" "alert(1)""#,
            "url = 'javascript:' 'alert(1)'",
            r#"url = "javascript:"#, // never closed: the body is unknown
            r#"html = f'<a href="javascript:{x}">go</a>'"#,
            "el.href = `javascript:${x}`",
            r#"url = "javascript:%s" % x"#,
            r#"<a href="javascript:alert(1)">"#,
            r#"<a href="javascript:alert('XSS')">"#,
            r#"<a href="javascript:alert(document.cookie)">"#,
            r#"<a href="javascript:void(0);alert(1)">"#,
            r#"<a href="javascript:void(alert(1))">"#,
            r#"<a href="javascript:void(x)">"#,
            r#"<a href="javascript:history.go('http://evil.example')">"#,
            r#"<a href="javascript:history.go(x)">"#,
            r#"<a href="javascript:history.back()+x">"#,
            r#"<a href="javascript:this.print();alert(1)">"#,
            r#"<a href="javascript:eval(name)">"#,
            r#"<a href="javascript:document.location='http://evil.example'">"#,
            "<a href=javascript:alert(1)>",
            "<a href=javascript:void(0);alert(1)>",
            // the bare payload at the start of an input has no delimiter: it keeps reporting
            "javascript:void(0)",
            // `(` opens a body that holds its own parentheses: not judged, so it reports
            "[clique](javascript:void(0))",
        ] {
            assert!(scheme_hits(sink) > 0, "sink missed: {sink}");
        }
    }

    #[test]
    fn test_cmdi_pattern() {
        let p = CmdInjectionPattern;
        // TP — shell-metachar payloads (kept) + dynamic shell-exec class.
        assert!(p.detect("; rm -rf /").is_some());
        assert!(p.detect("| ncat localhost").is_some());
        assert!(p.detect("&& curl http://evil").is_some());
        // The real injection the old pattern MISSED: shell-exec of a template
        // literal with `${...}` interpolation.
        assert!(p.detect("execSync(`echo ${userInput}`)").is_some());
        assert!(p.detect("child_process.exec(`ls ${dir}`)").is_some());
        assert!(p.detect("os.system(f\"ping {host}\")").is_some());
        assert!(p.detect("os.system(\"rm \" + target)").is_some());
        assert!(p.detect("subprocess.call(cmd, shell=True)").is_some());
        assert!(
            p.detect("subprocess.check_output(args, shell=True)")
                .is_some()
        );
        assert!(p.detect("spawn(cmd, {shell: true})").is_some());
        // FP — must NOT match: the safe / static / tooling forms.
        assert!(p.detect("echo hello").is_none());
        assert!(p.detect("memberRegex.exec(str)").is_none()); // JS regex (was the FP)
        assert!(p.detect("execFileSync(\"find\", [\"-name\", x])").is_none()); // safe argv
        assert!(p.detect("execSync(\"git status\")").is_none()); // static literal, no interp
        assert!(p.detect("execSync(`git status`)").is_none()); // static template, no `${`
        assert!(p.detect("pattern: \"execSync($$$)\"").is_none()); // tooling catalog literal
        assert!(p.detect("pattern: \"os.system($$$)\"").is_none()); // tooling catalog literal
        assert!(p.detect("Command::new(\"sh\").arg(\"-c\")").is_none()); // CEG sandbox by design
        assert_eq!(p.name(), "CMDi");
        assert_eq!(p.cwe_id(), 78);
    }

    #[test]
    fn test_path_traversal() {
        let p = PathTraversalPattern;
        // TP — multi-level climb + any URL-encoded dot-dot.
        assert!(p.detect("../../etc/passwd").is_some());
        assert!(p.detect("../../../../etc/shadow").is_some());
        assert!(p.detect("..\\..\\windows\\system32").is_some());
        assert!(p.detect("%2e%2e%2f").is_some());
        assert!(p.detect("%2e%2e%2f%2e%2e%2f").is_some());
        assert!(p.detect("%2e%2e/etc").is_some());
        assert!(p.detect("..%2f..%2fetc").is_some());
        assert!(p.detect("..%5c..%5cwindows").is_some());
        // FP — single relative paths + compile-time include macros (build-time
        // literals, never untrusted input — CWE-22 needs taint, not a substring).
        assert!(p.detect("../sibling").is_none()); // single climb — normal import
        assert!(p.detect("import { x } from \"../utils\"").is_none());
        assert!(p.detect("include_str!(\"../templates/x\")").is_none()); // single-level macro
        assert!(
            p.detect("include_str!(\"../../templates/x.tera\")")
                .is_none()
        ); // 2-level macro literal
        assert!(
            p.detect("include_bytes!(\"../../../assets/logo.png\")")
                .is_none()
        );
        assert!(p.detect("concat!(\"../../\", name)").is_none());
        assert!(p.detect("./local/path").is_none());
        assert!(p.detect("/absolute/path").is_none());
        // FP — Markdown link targets. A relative link in prose is a document
        // path a renderer resolves, never untrusted input hitting a filesystem
        // API. Climbing 2+ levels is idiomatic in .github/, docs/ and monorepo
        // READMEs, and this pattern runs on .md/.yml too, where CWE-22 cannot
        // apply. (2026-08-02: this blocked writing .github/ISSUE_TEMPLATE.)
        assert!(p.detect("[SECURITY.md](../../SECURITY.md)").is_none());
        assert!(
            p.detect("value: follow [SECURITY.md](../../SECURITY.md) instead")
                .is_none()
        );
        assert!(
            p.detect("See [the guide](../../../docs/guide.md).")
                .is_none()
        );
        assert!(p.detect("[label]: ../../CONTRIBUTING.md").is_none()); // reference definition
        assert!(p.detect("//! [`Foo`]: ../../foo/struct.Foo.html").is_none()); // rustdoc link
        // TP PRESERVED — the link exclusion must not become a blanket bypass.
        // A traversal OUTSIDE the link target is still reported, even on the
        // same line as a legitimate link.
        assert!(
            p.detect("[ok](./safe.md) then open(\"../../etc/passwd\")")
                .is_some()
        );
        // A closed link target does not shield what follows it.
        assert!(p.detect("[a](../../b.md) ../../etc/passwd").is_some());
        // Newline ends the scan window: a link on the previous line does not
        // shield a traversal on the next one.
        assert!(
            p.detect("[a](../../b.md)\nopen(\"../../etc/shadow\")")
                .is_some()
        );
        assert!(p.detect("a..b").is_none()); // range, no slash
        // A genuine traversal still fires even when an include macro precedes it
        // (find_iter skips the suppressed macro literal, reports the real climb).
        assert!(
            p.detect("include_str!(\"../../ok\"); let evil = \"../../../etc/passwd\";")
                .is_some()
        );
        assert_eq!(p.name(), "PathTraversal");
        assert_eq!(p.cwe_id(), 22);
    }

    #[test]
    fn test_integer_overflow() {
        let p = IntegerOverflowPattern;
        // TP — the 32-bit overflow boundary.
        assert!(p.detect("INT_MAX").is_some());
        assert!(p.detect("0x7fffffff").is_some());
        assert!(p.detect("let limit = INT_MAX - 1;").is_some());
        // FP — `\b` anchors INT_MAX; unrelated numbers/identifiers don't match.
        assert!(p.detect("42").is_none());
        assert!(p.detect("MY_INT_MAX").is_none()); // \b — not the C macro
        assert!(p.detect("UINT_MAX").is_none()); // unsigned boundary, different name
        assert_eq!(p.name(), "IntegerOverflow");
        assert_eq!(p.cwe_id(), 190);
    }

    #[test]
    fn test_buffer_overflow() {
        let p = BufferOverflowPattern;
        // TP — calls to unbounded C string functions.
        assert!(p.detect("strcpy(dst, src)").is_some());
        assert!(p.detect("sprintf(buf, fmt)").is_some());
        assert!(p.detect("strcat(a, b)").is_some());
        assert!(p.detect("gets(buf)").is_some());
        assert!(p.detect("vsprintf(buf, fmt, ap)").is_some());
        // FP — bounded/safe variants and bare mentions (not calls).
        assert!(p.detect("strncpy(dst, src, n)").is_none()); // bounded
        assert!(p.detect("snprintf(buf, n, fmt)").is_none()); // bounded — `sprintf` not a substring
        assert!(p.detect("// avoid sprintf in new code").is_none()); // prose mention, no call
        assert!(p.detect("strncpy").is_none());
        assert_eq!(p.name(), "BufferOverflow");
        assert_eq!(p.cwe_id(), 121);
    }

    #[test]
    fn test_deserialization() {
        let p = DeserializationPattern;
        // TP — unsafe Python deserialization sinks.
        assert!(p.detect("pickle.loads(data)").is_some());
        assert!(p.detect("yaml.load(input)").is_some());
        // FP — safe variants.
        assert!(p.detect("json.loads(data)").is_none());
        assert!(p.detect("yaml.safe_load(input)").is_none()); // safe variant — not `yaml.load`
        assert_eq!(p.name(), "Deserialization");
        assert_eq!(p.cwe_id(), 502);
    }

    #[test]
    fn test_ssrf() {
        let p = SsrfPattern;
        // TP — cloud-metadata endpoints + SSRF-only schemes (OWASP deny-list).
        assert!(
            p.detect("http://169.254.169.254/latest/meta-data")
                .is_some()
        );
        assert!(
            p.detect("metadata.google.internal/computeMetadata")
                .is_some()
        );
        assert!(p.detect("metadata.amazonaws.com").is_some());
        assert!(p.detect("gopher://evil:11211/_stats").is_some());
        assert!(p.detect("dict://attacker:6379/info").is_some());
        assert!(p.detect("phar://archive.phar/x").is_some());
        assert!(p.detect("fetch(`file://${userPath}`)").is_some()); // interpolated file URL
        // FP — must NOT match: the local Touring daemon + benign file URIs/hosts
        // (bare loopback/file:// flagged 37 benign workspace lines).
        assert!(p.detect("http://127.0.0.1:19999").is_none()); // local daemon
        assert!(p.detect("file:///etc/hosts").is_none()); // bare file URI — dual-use
        assert!(p.detect("https://example.com").is_none());
        assert!(p.detect("http://localhost:8080").is_none());
        assert_eq!(p.name(), "SSRF");
        assert_eq!(p.cwe_id(), 918);
    }

    #[test]
    fn test_ldap_injection() {
        let p = LdapInjectionPattern;
        // TPs — LDAP injection needs filter context around the wildcard-close.
        assert!(p.detect("cn=Admin").is_some()); // cn= filter fragment
        assert!(p.detect("(name=*)").is_some()); // `=*)` closes an attr=* filter
        assert!(p.detect("admin*)(|(uid=*").is_some()); // `*)(` filter breakout
        // A lone `*)` with no filter neighbour is NOT a usable LDAP payload — it is
        // the regex quantifier-close `(…[A-Za-z0-9]*):` that flagged every regex
        // literal (the FILE_REF_RE in touring-ceg/.../summarize.rs, CWE-90 FP).
        assert!(p.detect("*)").is_none());
        assert!(p.detect(r"([A-Za-z0-9]*):(\d+)").is_none()); // regex source, not LDAP
        assert!(p.detect("normal text").is_none());
        // Regression (2026-06-21): a bare `\)` alternative previously matched
        // EVERY closing paren, flagging all code as LDAP injection (the constant
        // 0.220 SecurityAnalyzer hit). Plain parenthesised code must NOT match.
        assert!(p.detect("fn noop() {}").is_none());
        assert!(p.detect("let r = (*f)(x);").is_none());
        assert!(p.detect("compute(a, b)").is_none());
        assert!(p.detect("if (x) { y() }").is_none());
        // Regression (14/09/2026): C pointer casts followed by a parenthesis are
        // not a filter breakout — tree-sitter's `array.h` scored 0.220 on this.
        assert!(
            p.detect("(void *)(self)->contents, &(self)->capacity")
                .is_none()
        );
        assert!(p.detect("n = sizeof(char *));").is_none());
        assert!(p.detect("memcpy((char*)(dst), src, n);").is_none());
        // The breakouts it exists for still match: an operator or an `attr=` clause.
        assert!(p.detect("*)(&(objectClass=user").is_some());
        assert!(p.detect("x*)(uid=admin").is_some());
        assert_eq!(p.name(), "LDAPi");
        assert_eq!(p.cwe_id(), 90);
    }

    #[test]
    fn test_xml_injection() {
        let p = XmlInjectionPattern;
        // TP — XXE: entity declarations + DOCTYPE with an internal subset `[`.
        assert!(
            p.detect("<!ENTITY xxe SYSTEM \"file:///etc/passwd\">")
                .is_some()
        );
        assert!(p.detect("<!DOCTYPE foo [<!ENTITY x \"y\">]>").is_some());
        assert!(p.detect("<!DOCTYPE root [").is_some());
        // FP — benign HTML5 doctype + plain CDATA / XML (no entity, no subset).
        assert!(p.detect("<!DOCTYPE html>").is_none()); // HTML5 doctype (was FP)
        assert!(p.detect("<![CDATA[some data]]>").is_none()); // legit XML construct
        assert!(p.detect("<root>text</root>").is_none());
        assert_eq!(p.name(), "XMLInjection");
        assert_eq!(p.cwe_id(), 91);
    }

    #[test]
    fn test_registry_detect_all() {
        let mut reg = PatternRegistry::new();
        reg.register(Box::new(SqlInjectionPattern));
        reg.register(Box::new(XssPattern));

        let input = "' OR '1'='1' <script>alert(1)</script>";
        let matches = reg.detect_all(input);
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().any(|m| m.pattern_name == "SQLi"));
        assert!(matches.iter().any(|m| m.pattern_name == "XSS"));
    }

    #[test]
    fn test_registry_len_empty() {
        let reg = PatternRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_registry_default() {
        let reg = PatternRegistry::default();
        assert!(reg.is_empty());
    }

    #[test]
    fn test_severity_scores() {
        let patterns: Vec<Box<dyn VulnerabilityPattern>> = vec![
            Box::new(SqlInjectionPattern),
            Box::new(XssPattern),
            Box::new(CmdInjectionPattern),
            Box::new(PathTraversalPattern),
            Box::new(IntegerOverflowPattern),
            Box::new(BufferOverflowPattern),
            Box::new(DeserializationPattern),
            Box::new(SsrfPattern),
            Box::new(LdapInjectionPattern),
            Box::new(XmlInjectionPattern),
        ];

        for p in patterns {
            assert!((p.severity() - 0.0).abs() >= 0.0);
            assert!(p.severity() <= 10.0);
        }
    }

    /// Cross-audit R2 (14/09/2026): the seven F2.1 false positives of the
    /// workspace, each next to the payload of the same class that must still fire.
    #[test]
    fn r2_context_false_positives_and_their_true_positives() {
        let xss = XssPattern;
        // `cli_suggester.rs`: a flag placeholder in usage text.
        assert!(
            xss.detect("varredura única: `touring run --lang bash --file <script>` e")
                .is_none()
        );
        assert!(xss.detect("usage: tool -s <script> [args]").is_none());
        assert!(xss.detect("tool --file=<script>").is_none());
        assert!(xss.detect("--file <script>alert(1)</script>").is_some());
        assert!(
            xss.detect("html = '<p>' + name + '<script>steal()</script>'")
                .is_some()
        );
        assert!(xss.detect("<script src=x>").is_some());
        assert!(xss.detect("x <script>").is_some(), "no flag before it");
        // `cc_build.py`: the page's own script inside the document it generates.
        assert!(
            xss.detect("body = f\"\"\"<!DOCTYPE html>\n<html><body>{grid}\n<script>\n{_JS}\n</script>\n</body></html>\"\"\"")
                .is_none()
        );
        assert!(
            xss.detect("page = \"<HTML lang=en><script src=app.js></script>\"")
                .is_none()
        );
        assert!(
            xss.detect("doc = \"<html></html>\"; tail = \"<script>steal()</script>\"")
                .is_some(),
            "a script after the document closed is not the document's"
        );

        let path = PathTraversalPattern;
        // `holon-wasm-components/*/src/lib.rs`: the WIT file a macro reads while compiling.
        assert!(
            path.detect("wit_bindgen::generate!({\n    path: \"../../crates/touring-wasm/wit/holon-core.wit\",\n    world: \"x\",\n});")
                .is_none()
        );
        assert!(
            path.detect("let f = File::open(\"../../etc/passwd\");")
                .is_some()
        );
        assert!(
            path.detect("gen!({ path: \"a\" });\nlet cfg = Config { path: \"../../etc/passwd\" };")
                .is_some(),
            "a closed macro body does not shield a struct literal after it"
        );
        assert!(
            path.detect("generate!({\n    mypath: \"../../etc/passwd\",\n});")
                .is_some(),
            "only the `path` key"
        );
        // `generate_w0_premium_artifacts.py`: a symlink target in a tree listing.
        assert!(
            path.detect("│   ├── touring -> ../../../~/.touring/toolchains/1.0.0/bin/touring")
                .is_none()
        );
        assert!(
            path.detect("lrwxrwxrwx 1 u u 40 x touring -> ../../bin/touring")
                .is_none()
        );
        assert!(
            path.detect("fn f() -> String { \"../../etc/passwd\".into() }")
                .is_some()
        );
        assert!(
            path.detect("x -> ../../etc/passwd").is_some(),
            "an arrow alone is not a listing"
        );

        let ldap = LdapInjectionPattern;
        // `touring-quality-score`: a shell `case` glob; `w12_migration_tool.py`: prose.
        assert!(ldap.detect("    -o|--output|--output=*)").is_none());
        assert!(
            ldap.detect("- `~/.claude/memory/` (memory tier=*)")
                .is_none()
        );
        assert!(ldap.detect("(name=*)").is_some());
        assert!(ldap.detect("(&(objectClass=*)(uid=admin))").is_some());
        assert!(ldap.detect("filter = \"(mail=*)\"").is_some());
    }

    /// `detect` names the first match; `detect_every` names all of them, in
    /// order, so a caller that drops one (a comment) still sees the next.
    #[test]
    fn detect_every_returns_every_match_and_detect_the_first() {
        let input = "// <script>\nlet page = \"<script>steal()</script>\";";
        let all = XssPattern.detect_every(input);
        assert_eq!(all.len(), 2);
        assert!(all[0].span.0 < all[1].span.0);
        assert_eq!(XssPattern.detect(input).map(|m| m.span), Some(all[0].span));
        assert_eq!(
            PathTraversalPattern.detect_every("../../a ../../b").len(),
            2
        );
        assert!(SqlInjectionPattern.detect_every("nothing here").is_empty());

        let mut reg = PatternRegistry::new();
        reg.register(Box::new(XssPattern));
        reg.register(Box::new(SqlInjectionPattern));
        let every = reg.detect_every("<script> UNION SELECT <script>");
        assert_eq!(every.len(), 3);
        assert_eq!(reg.detect_all("<script> UNION SELECT <script>").len(), 2);
    }
}
