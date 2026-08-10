//! API design (D09 / F1.9) — polyglot public-API **contract** conformance.
//!
//! D09 asks whether the public surface of a module is well-designed: typed
//! errors, idiomatic naming, encapsulation, ergonomic construction. The real
//! oracle differs per language (Rust API Guidelines, Effective Go, Effective
//! C++, PEP 8); this engine approximates a high-confidence **subset** of each
//! across **7 languages**. It is deliberately disjoint from `super::idioms`
//! (F4.1): idioms scores *local style* (`.len()==0`, `==null`), api-design
//! scores the *public contract* (error types, getter naming, field exposure).
//!
//! | Lang | Oracle | Contract smells detected |
//! |------|--------|--------------------------|
//! | Rust | Rust API Guidelines | `Result<_, String>` (C-GOOD-ERR), `pub fn get_*` (C-GETTER), `into_*(&self)` / `as_*(self)` (C-CONV), public type without `Debug` (C-DEBUG), `new()` with >5 params (C-BUILDER) |
//! | Python | PEP 8 / API gotchas | mutable default argument (`=[]`/`={}`), `raise Exception(..)` (broad), `def` with >5 positional params |
//! | TypeScript / JavaScript | ESLint design | `throw "string"` (not an `Error`), function with >4 params (→ options object) |
//! | Go | Effective Go | `GetX()` getter prefix (Go getters drop `Get`), `panic(..)` in library code |
//! | Java | encapsulation | `public` field (not encapsulated), `throws Exception`/`Throwable` (broad) |
//! | C++ | Effective C++ | function-like macro `#define X(...)` (→ inline function, Item 2) |
//!
//! Comments and `#[cfg(test)]`/test regions are excluded via
//! `super::code_regions`. A per-file scanner cannot replace a type-aware API
//! linter, so it catches a high-precision subset; F1.9 is WARN (advisory), not
//! BLOCK. Replaces a stub that counted `pub fn`/`pub struct`/`pub trait` and
//! *penalised a wide public surface* — an anti-metric (a large, well-designed
//! API scored worse than a tiny bad one). Zero non-std deps beyond `memchr`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// A pure-substring API-contract smell: literal `needle`, human `message`, and
/// `weight` reflecting how strongly the language's API oracle would flag it.
struct ApiNeedle {
    needle: &'static [u8],
    message: &'static str,
    weight: f32,
}

/// Canonical language bucket (collapses extension aliases). `TsJs(true)` is
/// TypeScript (generics → angle-bracket-aware parameter counting); `false` is
/// JavaScript.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    TsJs(bool),
    Go,
    Java,
    Cpp,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        "typescript" | "ts" | "tsx" => Lang::TsJs(true),
        "javascript" | "js" | "jsx" => Lang::TsJs(false),
        "go" => Lang::Go,
        "java" => Lang::Java,
        "cpp" | "c++" | "cc" | "cxx" | "c" | "h" | "hpp" => Lang::Cpp,
        _ => Lang::Other,
    }
}

// ── Pure-substring needles (the structural smells live in analyze_<lang>) ─────
const PYTHON_API_NEEDLES: &[ApiNeedle] = &[
    ApiNeedle {
        needle: b"raise Exception(",
        message: "`raise Exception(..)` -> raise a specific/custom exception type",
        weight: 0.8,
    },
    ApiNeedle {
        needle: b"raise BaseException(",
        message: "`raise BaseException(..)` -> raise a specific exception type",
        weight: 0.8,
    },
];

const JSTS_API_NEEDLES: &[ApiNeedle] = &[
    ApiNeedle {
        needle: b"throw \"",
        message: "`throw \"...\"` -> throw an `Error` subtype, not a string literal",
        weight: 0.9,
    },
    ApiNeedle {
        needle: b"throw '",
        message: "`throw '...'` -> throw an `Error` subtype, not a string literal",
        weight: 0.9,
    },
    ApiNeedle {
        needle: b"throw `",
        message: "throw a template string -> throw an `Error` subtype, not a string",
        weight: 0.9,
    },
];

const GO_API_NEEDLES: &[ApiNeedle] = &[ApiNeedle {
    needle: b"panic(",
    message: "`panic(..)` in library code -> return an `error` (Go error convention)",
    weight: 0.5,
}];

const JAVA_API_NEEDLES: &[ApiNeedle] = &[
    ApiNeedle {
        needle: b"throws Exception",
        message: "`throws Exception` -> declare a specific checked exception",
        weight: 0.7,
    },
    ApiNeedle {
        needle: b"throws Throwable",
        message: "`throws Throwable` -> declare a specific exception type",
        weight: 0.7,
    },
];

fn api_needles_for(lang: Lang) -> &'static [ApiNeedle] {
    match lang {
        Lang::Python => PYTHON_API_NEEDLES,
        Lang::TsJs(_) => JSTS_API_NEEDLES,
        Lang::Go => GO_API_NEEDLES,
        Lang::Java => JAVA_API_NEEDLES,
        _ => &[],
    }
}

/// Per-file API-design analysis (parallel shape to [`super::idioms::IdiomReport`]).
pub type ApiDesignReport = crate::quality::SmellReport;

// ── Low-level byte helpers (all UTF-8-safe: operate on &[u8], never slice str) ─

/// Read an ASCII identifier (`[A-Za-z0-9_]`) starting at `start`.
fn read_ident(bytes: &[u8], start: usize) -> &[u8] {
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    &bytes[start..end]
}

/// Trim leading/trailing ASCII whitespace from a byte slice (avoids relying on
/// the 1.80-stabilised `<[u8]>::trim_ascii`).
fn trim_ascii_ws(s: &[u8]) -> &[u8] {
    let mut a = 0;
    let mut b = s.len();
    while a < b && s[a].is_ascii_whitespace() {
        a += 1;
    }
    while b > a && s[b - 1].is_ascii_whitespace() {
        b -= 1;
    }
    &s[a..b]
}

/// First index at/after `i` that is not a space or tab.
fn first_nonspace_after(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i
}

/// Index of the first `(` at/after `from`, stopping at end-of-line (`None` if
/// the line has no `(`).
fn paren_on_line(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => return Some(i),
            b'\n' => return None,
            _ => i += 1,
        }
    }
    None
}

/// Does the signature's return type (`-> T` on the same line, starting the scan
/// at `from`) denote an *owned* value (not a `&`-reference)? A missing `->`
/// before end-of-line is treated as non-owned (conservative: do not flag).
fn returns_owned_on_line(bytes: &[u8], from: usize) -> bool {
    let mut i = from;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'\n' => return false,
            b'-' if bytes[i + 1] == b'>' => {
                let r = first_nonspace_after(bytes, i + 2);
                return bytes.get(r) != Some(&b'&');
            }
            _ => i += 1,
        }
    }
    false
}

/// Count top-level parameters in the list whose opening `(` is at `open`.
/// Balances `()`/`[]`/`{}` (and `<>` when `angle`, for generics). `()` is 0.
fn count_params(bytes: &[u8], open: usize, angle: bool) -> usize {
    let mut paren = 0i32;
    let mut brack = 0i32;
    let mut brace = 0i32;
    let mut ang = 0i32;
    let mut commas = 0usize;
    let mut saw = false;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => paren += 1,
            b')' => {
                paren -= 1;
                if paren == 0 {
                    break;
                }
            }
            b'[' => brack += 1,
            b']' => brack -= 1,
            b'{' => brace += 1,
            b'}' => brace -= 1,
            b'<' if angle => ang += 1,
            b'>' if angle => ang = (ang - 1).max(0),
            b',' if paren == 1 && brack == 0 && brace == 0 && ang == 0 => commas += 1,
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ if paren >= 1 => saw = true,
            _ => {}
        }
        i += 1;
    }
    if saw { commas + 1 } else { 0 }
}

/// Return the last top-level generic argument of the `<...>` whose `<` is at
/// `lt` (e.g. `Result<T, E>` -> `E`). Balances nested `<>`/`()`/`[]`.
fn last_generic_arg(bytes: &[u8], lt: usize) -> Option<&[u8]> {
    let mut ang = 0i32;
    let mut paren = 0i32;
    let mut brack = 0i32;
    let mut last_start = lt + 1;
    let mut i = lt;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => ang += 1,
            b'>' => {
                ang -= 1;
                if ang == 0 {
                    return Some(trim_ascii_ws(&bytes[last_start..i]));
                }
            }
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'[' => brack += 1,
            b']' => brack -= 1,
            b',' if ang == 1 && paren == 0 && brack == 0 => last_start = i + 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// If `trimmed` declares a fully-public type, return its name (`pub struct Foo`
/// -> `Foo`). `pub(crate)`/private are intentionally excluded (C-DEBUG is about
/// the externally-visible surface).
fn pub_type_name(trimmed: &str) -> Option<&str> {
    for kw in ["pub struct ", "pub enum ", "pub union "] {
        if let Some(rest) = trimmed.strip_prefix(kw) {
            let name = rest
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Count signatures whose parameter list (the `(` ending `keyword`, or the
/// first `(` after it on the line) holds more than `threshold` params. Shared
/// by Rust `new`, Python `def`, and JS/TS `function`/`constructor`.
fn wide_params_on_keyword(
    source: &str,
    regions: &[(usize, usize)],
    keyword: &[u8],
    angle: bool,
    threshold: usize,
    skip_self: bool,
) -> usize {
    let bytes = source.as_bytes();
    let mut n = 0usize;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        let lb = chunk.as_bytes();
        let mut search = 0usize;
        while let Some(rel) = memmem::find(&lb[search..], keyword) {
            let kpos = search + rel;
            if let Some(prel) = lb[kpos..].iter().position(|&b| b == b'(') {
                let paren = line_off + kpos + prel;
                let mut params = count_params(bytes, paren, angle);
                if skip_self && params > 0 {
                    let first = first_nonspace_after(bytes, paren + 1);
                    let id = read_ident(bytes, first);
                    if id == b"self" || id == b"cls" {
                        params -= 1;
                    }
                }
                if params > threshold {
                    n += 1;
                }
            }
            search = kpos + keyword.len();
            if search >= lb.len() {
                break;
            }
        }
    }
    n
}

// ── Rust structural detectors ────────────────────────────────────────────────

/// `Result<_, String>` in any signature — String is not a `std::error::Error`
/// (C-GOOD-ERR). Inspects the *last* generic arg so `Result<String, MyErr>`
/// (String is the Ok type) is not flagged.
fn rust_string_errors(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    for off in memmem::find_iter(bytes, b"Result<") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let lt = off + b"Result<".len() - 1;
        if last_generic_arg(bytes, lt) == Some(b"String".as_slice()) {
            n += 1;
        }
    }
    n
}

/// `pub fn get_*` getters (C-GETTER: a getter for `first` is `first()`, not
/// `get_first()`). The conventional `get`/`get_mut`/`get_unchecked`/`get_ref`/
/// `get_or_*`/`get_many`/`get_disjoint`/`get_raw` family is allowlisted.
fn rust_getter_prefix(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    const ALLOW: &[&[u8]] = &[
        b"mut",
        b"unchecked",
        b"ref",
        b"or",
        b"many",
        b"disjoint",
        b"raw",
    ];
    let mut n = 0;
    for off in memmem::find_iter(bytes, b"pub fn get_") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let suffix = read_ident(bytes, off + b"pub fn get_".len());
        let first_seg = suffix.split(|&b| b == b'_').next().unwrap_or(suffix);
        if !ALLOW.contains(&first_seg) {
            n += 1;
        }
    }
    n
}

/// C-CONV: `into_*` must consume `self` by value (so `into_*(&self)` is a
/// naming bug); `as_*` should be a cheap borrow (so `as_*(self)` is one).
fn rust_conv_violations(bytes: &[u8], regions: &[(usize, usize)]) -> (usize, usize) {
    let mut into_v = 0;
    let mut as_v = 0;
    for off in memmem::find_iter(bytes, b"pub fn into_") {
        if offset_suppressed(off, regions) {
            continue;
        }
        if let Some(p) = paren_on_line(bytes, off) {
            let f = first_nonspace_after(bytes, p + 1);
            if bytes.get(f) == Some(&b'&') {
                into_v += 1;
            }
        }
    }
    for off in memmem::find_iter(bytes, b"pub fn as_") {
        if offset_suppressed(off, regions) {
            continue;
        }
        if let Some(p) = paren_on_line(bytes, off) {
            let f = first_nonspace_after(bytes, p + 1);
            // `as_*(self)` consuming `self` is only a real C-CONV misuse when it
            // returns an *owned* value (that is really an `into_*`). The common
            // `as_*(self) -> &'static str` on a `Copy` fieldless enum borrows
            // static data and is idiomatic, so it must not be flagged.
            if bytes.get(f) != Some(&b'&')
                && bytes[f..].starts_with(b"self")
                && returns_owned_on_line(bytes, p)
            {
                as_v += 1;
            }
        }
    }
    (into_v, as_v)
}

/// Fully-public `struct`/`enum`/`union` lacking `Debug` (C-DEBUG: "all public
/// types should implement Debug"). Considers a `#[derive(.. Debug ..)]` in the
/// contiguous attribute block above (single- or multi-line) and a manual
/// `impl .. Debug for <Name>` anywhere in the file.
fn rust_missing_debug(source: &str, regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    let mut attr_blob = String::new();
    let mut in_attr = false;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        let trimmed = chunk.trim();
        if offset_suppressed(line_off, regions) {
            attr_blob.clear();
            in_attr = false;
            continue;
        }
        if in_attr {
            attr_blob.push_str(trimmed);
            if trimmed.contains(']') {
                in_attr = false;
            }
            continue;
        }
        if trimmed.starts_with("#[") || trimmed.starts_with("#!") {
            attr_blob.push_str(trimmed);
            if !trimmed.contains(']') {
                in_attr = true;
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            // doc / comment / blank: attributes may straddle, so keep the blob.
            continue;
        }
        if let Some(name) = pub_type_name(trimmed) {
            let has_debug =
                attr_blob.contains("Debug") || source.contains(&format!("Debug for {name}"));
            if !has_debug {
                n += 1;
            }
        }
        attr_blob.clear();
        in_attr = false;
    }
    n
}

// ── Go / Java / C++ / Python structural detectors ────────────────────────────

/// `func .. GetX()` getter prefix — Effective Go: a getter for `owner` is
/// `Owner()`, never `GetOwner()`. Requires `Get` to start an identifier and be
/// followed by an uppercase letter and (eventually) `(`.
fn go_getter_prefix(source: &str, regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        let lb = chunk.as_bytes();
        if memmem::find(lb, b"func ").is_none() {
            continue;
        }
        for pos in memmem::find_iter(lb, b"Get") {
            let after = pos + 3;
            if lb.get(after).map(u8::is_ascii_uppercase) != Some(true) {
                continue;
            }
            if pos > 0 && (lb[pos - 1].is_ascii_alphanumeric() || lb[pos - 1] == b'_') {
                continue;
            }
            let id = read_ident(lb, pos);
            if lb.get(pos + id.len()) == Some(&b'(') {
                n += 1;
                break;
            }
        }
    }
    n
}

/// `public` instance field — Java encapsulation: expose behaviour, not state.
/// `public static final` constants and methods (`(`) are excluded.
fn java_public_fields(source: &str, regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        let t = chunk.trim();
        if !t.starts_with("public ") || !t.ends_with(';') || t.contains('(') {
            continue;
        }
        if t.contains("static final")
            || t.contains("class ")
            || t.contains("interface ")
            || t.contains("enum ")
            || t.contains("record ")
        {
            continue;
        }
        n += 1;
    }
    n
}

/// Function-like macro `#define X(...)` — Effective C++ Item 2: prefer an
/// inline function (type-safe, scoped, debuggable). Object-like macros
/// (`#define PI 3.14`, include guards) have a space or end before `(` and are
/// not flagged.
fn cpp_function_macros(source: &str, regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        if let Some(rest) = chunk.trim_start().strip_prefix("#define ") {
            let rb = rest.as_bytes();
            let id = read_ident(rb, 0);
            if !id.is_empty() && rb.get(id.len()) == Some(&b'(') {
                n += 1;
            }
        }
    }
    n
}

/// Mutable default argument (`def f(x=[])` / `={}` / `=set()` / …) — the
/// classic Python API gotcha (the default is shared across calls).
fn py_mutable_defaults(source: &str, regions: &[(usize, usize)]) -> usize {
    let mut n = 0;
    let mut offset = 0usize;
    for chunk in source.split_inclusive('\n') {
        let line_off = offset;
        offset += chunk.len();
        if offset_suppressed(line_off, regions) {
            continue;
        }
        let t = chunk.trim_start();
        if !(t.starts_with("def ") || t.starts_with("async def ")) {
            continue;
        }
        let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        if ["=[]", "={}", "=set()", "=dict()", "=list()"]
            .iter()
            .any(|p| compact.contains(p))
        {
            n += 1;
        }
    }
    n
}

// ── Per-language assembly ────────────────────────────────────────────────────

fn analyze_rust(
    source: &str,
    bytes: &[u8],
    regions: &[(usize, usize)],
    report: &mut ApiDesignReport,
) {
    report.push(
        "`Result<_, String>` -> a typed error implementing `std::error::Error` (C-GOOD-ERR)",
        rust_string_errors(bytes, regions),
        1.0,
    );
    report.push(
        "`get_` prefix on a getter -> drop it: `fn x()` not `fn get_x()` (C-GETTER)",
        rust_getter_prefix(bytes, regions),
        1.0,
    );
    let (into_v, as_v) = rust_conv_violations(bytes, regions);
    report.push(
        "`into_*` takes `&self` -> `into_*` must consume `self` by value (C-CONV)",
        into_v,
        0.8,
    );
    report.push(
        "`as_*` consumes `self` -> `as_*` should borrow (`&self`) for a free view (C-CONV)",
        as_v,
        0.6,
    );
    report.push(
        "public type without `Debug` -> `#[derive(Debug)]` (C-DEBUG)",
        rust_missing_debug(source, regions),
        0.5,
    );
    report.push(
        "`new()` with >5 params -> consider a builder (C-BUILDER)",
        wide_params_on_keyword(source, regions, b"pub fn new(", true, 5, false),
        0.5,
    );
}

fn analyze_python(source: &str, regions: &[(usize, usize)], report: &mut ApiDesignReport) {
    report.push(
        "mutable default argument (`=[]`/`={}`) -> default to `None`, build inside (API gotcha)",
        py_mutable_defaults(source, regions),
        1.0,
    );
    report.push(
        "`def` with >5 positional params -> group into a dataclass / keyword-only args",
        wide_params_on_keyword(source, regions, b"def ", false, 5, true),
        0.5,
    );
}

fn analyze_jsts(
    source: &str,
    regions: &[(usize, usize)],
    is_ts: bool,
    report: &mut ApiDesignReport,
) {
    let wide = wide_params_on_keyword(source, regions, b"function ", is_ts, 4, false)
        + wide_params_on_keyword(source, regions, b"constructor(", is_ts, 4, false);
    report.push(
        "function with >4 params -> pass a single options object",
        wide,
        0.5,
    );
}

fn analyze_go(source: &str, regions: &[(usize, usize)], report: &mut ApiDesignReport) {
    report.push(
        "`GetX()` getter prefix -> Go getters drop `Get` (use `X()`) (Effective Go)",
        go_getter_prefix(source, regions),
        1.0,
    );
}

fn analyze_java(source: &str, regions: &[(usize, usize)], report: &mut ApiDesignReport) {
    report.push(
        "`public` field -> encapsulate (private field + accessor)",
        java_public_fields(source, regions),
        0.7,
    );
}

fn analyze_cpp(source: &str, regions: &[(usize, usize)], report: &mut ApiDesignReport) {
    report.push(
        "function-like macro `#define X(...)` -> prefer an inline function (Effective C++ Item 2)",
        cpp_function_macros(source, regions),
        0.6,
    );
}

/// Analyze public-API contract smells for `lang`. Unknown languages yield an
/// empty report (no API-design model → no findings → score 1.0).
#[must_use]
pub fn analyze_api_design(source: &str, lang: &str) -> ApiDesignReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let mut report = ApiDesignReport {
        total_lines: source.lines().count(),
        ..ApiDesignReport::default()
    };
    let canon = canonical_lang(lang);

    for rule in api_needles_for(canon) {
        let count = memmem::find_iter(bytes, rule.needle)
            .filter(|&off| !offset_suppressed(off, &regions))
            .count();
        report.push(rule.message, count, rule.weight);
    }

    match canon {
        Lang::Rust => analyze_rust(source, bytes, &regions, &mut report),
        Lang::Python => analyze_python(source, &regions, &mut report),
        Lang::TsJs(is_ts) => analyze_jsts(source, &regions, is_ts, &mut report),
        Lang::Go => analyze_go(source, &regions, &mut report),
        Lang::Java => analyze_java(source, &regions, &mut report),
        Lang::Cpp => analyze_cpp(source, &regions, &mut report),
        Lang::Other => {}
    }

    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D09 API-design score: `1 - density * SCALE`, where density is the weighted
/// smell count per production line. A clean public surface is `1.0`; contract
/// debt accumulates linearly. WARN-tier (advisory), so a heavily mis-designed
/// API may land below 0.5.
#[must_use]
pub fn score_api_design(report: &ApiDesignReport) -> f32 {
    const SCALE: f32 = 6.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rust ──────────────────────────────────────────────────────────────────
    #[test]
    fn rust_idiomatic_api_is_clean() {
        let src = "#[derive(Debug)]\npub struct Config {\n    name: String,\n}\nimpl Config {\n    pub fn name(&self) -> &str { &self.name }\n    pub fn parse(s: &str) -> Result<Config, ParseError> { todo!() }\n}\n";
        let r = analyze_api_design(src, "rust");
        assert_eq!(
            r.violations, 0,
            "idiomatic api has no findings: {:?}",
            r.findings
        );
        assert!((score_api_design(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rust_string_error_flagged() {
        let bad = analyze_api_design("pub fn f() -> Result<(), String> { Ok(()) }\n", "rust");
        assert!(
            bad.violations >= 1,
            "Result<_, String> must flag: {:?}",
            bad.findings
        );
        // String as the Ok type must NOT flag.
        let ok = analyze_api_design(
            "pub fn f() -> Result<String, MyError> { todo!() }\n",
            "rust",
        );
        assert_eq!(
            ok.violations, 0,
            "String Ok-type is fine: {:?}",
            ok.findings
        );
    }

    #[test]
    fn rust_getter_prefix_flagged_but_conventional_allowed() {
        let bad = analyze_api_design(
            "impl S {\n    pub fn get_name(&self) -> &str { &self.name }\n}\n",
            "rust",
        );
        assert!(
            bad.violations >= 1,
            "get_name must flag: {:?}",
            bad.findings
        );
        let ok = analyze_api_design(
            "impl S {\n    pub fn get_mut(&mut self) -> &mut V { todo!() }\n    pub fn get_or_insert(&mut self) -> &mut V { todo!() }\n}\n",
            "rust",
        );
        assert_eq!(
            ok.violations, 0,
            "get_mut/get_or_* are conventional: {:?}",
            ok.findings
        );
    }

    #[test]
    fn rust_conv_violations_flagged() {
        // into_ that borrows, as_ that consumes
        let bad = analyze_api_design(
            "impl S {\n    pub fn into_inner(&self) -> V { todo!() }\n    pub fn as_thing(self) -> V { todo!() }\n}\n",
            "rust",
        );
        assert!(
            bad.violations >= 2,
            "into_(&self) + as_(self) must flag: {:?}",
            bad.findings
        );
        // Correct conventions: into_ consumes, as_ borrows.
        let ok = analyze_api_design(
            "impl S {\n    pub fn into_inner(self) -> V { todo!() }\n    pub fn as_str(&self) -> &str { todo!() }\n}\n",
            "rust",
        );
        assert_eq!(
            ok.violations, 0,
            "correct conv conventions: {:?}",
            ok.findings
        );
    }

    #[test]
    fn rust_as_self_returning_reference_is_benign() {
        // A `Copy` fieldless enum's `as_str(self) -> &'static str` borrows static
        // data and is idiomatic — it must NOT be flagged (only a consuming `as_*`
        // returning an *owned* value is the real `into_*` misuse).
        let ok = analyze_api_design(
            "impl E {\n    pub fn as_str(self) -> &'static str { \"x\" }\n}\n",
            "rust",
        );
        assert_eq!(
            ok.violations, 0,
            "as_(self) -> &'static is benign: {:?}",
            ok.findings
        );
    }

    #[test]
    fn rust_missing_debug_flagged() {
        let bad = analyze_api_design("pub struct Bare {\n    x: u8,\n}\n", "rust");
        assert!(
            bad.violations >= 1,
            "pub struct without Debug must flag: {:?}",
            bad.findings
        );
        let derived = analyze_api_design(
            "#[derive(Debug, Clone)]\npub struct Ok2 {\n    x: u8,\n}\n",
            "rust",
        );
        assert_eq!(
            derived.violations, 0,
            "derived Debug is fine: {:?}",
            derived.findings
        );
        let manual = analyze_api_design(
            "pub struct Conn {\n    id: u64,\n}\nimpl std::fmt::Debug for Conn {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }\n}\n",
            "rust",
        );
        assert_eq!(
            manual.violations, 0,
            "manual Debug impl counts: {:?}",
            manual.findings
        );
    }

    #[test]
    fn rust_wide_constructor_flagged() {
        let bad = analyze_api_design(
            "impl S {\n    pub fn new(a: u8, b: u8, c: u8, d: u8, e: u8, f: u8) -> S { todo!() }\n}\n",
            "rust",
        );
        assert!(
            bad.violations >= 1,
            ">5-param new must flag: {:?}",
            bad.findings
        );
        let ok = analyze_api_design(
            "impl S {\n    pub fn new(a: u8, b: u8) -> S { todo!() }\n}\n",
            "rust",
        );
        assert_eq!(ok.violations, 0, "small new is fine: {:?}", ok.findings);
    }

    // ── Python ────────────────────────────────────────────────────────────────
    #[test]
    fn python_mutable_default_flagged() {
        let bad = analyze_api_design("def f(x=[]):\n    return x\n", "python");
        assert!(
            bad.violations >= 1,
            "mutable default must flag: {:?}",
            bad.findings
        );
        let ok = analyze_api_design("def f(x=None):\n    return x or []\n", "python");
        assert_eq!(ok.violations, 0, "None default is fine: {:?}", ok.findings);
    }

    #[test]
    fn python_broad_raise_flagged() {
        let bad = analyze_api_design("def f():\n    raise Exception('boom')\n", "python");
        assert!(
            bad.violations >= 1,
            "raise Exception must flag: {:?}",
            bad.findings
        );
    }

    #[test]
    fn python_wide_params_skips_self() {
        // 5 params after self -> not > 5 -> ok
        let ok = analyze_api_design(
            "class C:\n    def m(self, a, b, c, d, e):\n        pass\n",
            "python",
        );
        assert_eq!(
            ok.violations, 0,
            "5 params (excl self) is fine: {:?}",
            ok.findings
        );
        // 6 params after self -> flag
        let bad = analyze_api_design(
            "class C:\n    def m(self, a, b, c, d, e, f):\n        pass\n",
            "python",
        );
        assert!(
            bad.violations >= 1,
            "6 params (excl self) must flag: {:?}",
            bad.findings
        );
    }

    // ── TypeScript / JavaScript ─────────────────────────────────────────────────
    #[test]
    fn ts_throw_string_flagged_not_error() {
        let bad = analyze_api_design("function f() { throw \"boom\"; }\n", "typescript");
        assert!(
            bad.violations >= 1,
            "throw string must flag: {:?}",
            bad.findings
        );
        let ok = analyze_api_design(
            "function f() { throw new Error(\"boom\"); }\n",
            "typescript",
        );
        assert_eq!(
            ok.violations, 0,
            "throw new Error is fine: {:?}",
            ok.findings
        );
    }

    #[test]
    fn ts_wide_params_flagged() {
        let bad = analyze_api_design(
            "function f(a: number, b: number, c: number, d: number, e: number) {}\n",
            "typescript",
        );
        assert!(
            bad.violations >= 1,
            ">4 params must flag: {:?}",
            bad.findings
        );
        // generics in params must not break the count
        let ok = analyze_api_design(
            "function f(m: Map<string, number>, n: number) {}\n",
            "typescript",
        );
        assert_eq!(
            ok.violations, 0,
            "2 params with generics is fine: {:?}",
            ok.findings
        );
    }

    // ── Go ──────────────────────────────────────────────────────────────────────
    #[test]
    fn go_getter_prefix_flagged() {
        let bad = analyze_api_design("func (r *R) GetOwner() string { return r.owner }\n", "go");
        assert!(
            bad.violations >= 1,
            "GetOwner must flag: {:?}",
            bad.findings
        );
        let ok = analyze_api_design("func (r *R) Owner() string { return r.owner }\n", "go");
        assert_eq!(
            ok.violations, 0,
            "Owner() is the Go convention: {:?}",
            ok.findings
        );
    }

    #[test]
    fn go_panic_flagged() {
        let bad = analyze_api_design("func f() { panic(\"no\") }\n", "go");
        assert!(
            bad.violations >= 1,
            "panic in lib must flag: {:?}",
            bad.findings
        );
    }

    // ── Java ──────────────────────────────────────────────────────────────────
    #[test]
    fn java_public_field_flagged_but_constant_allowed() {
        let bad = analyze_api_design("public class C {\n    public int x;\n}\n", "java");
        assert!(
            bad.violations >= 1,
            "public field must flag: {:?}",
            bad.findings
        );
        let constant = analyze_api_design(
            "public class C {\n    public static final int MAX = 10;\n}\n",
            "java",
        );
        assert_eq!(
            constant.violations, 0,
            "public constant is fine: {:?}",
            constant.findings
        );
        let method = analyze_api_design(
            "public class C {\n    public int getX() { return x; }\n}\n",
            "java",
        );
        assert_eq!(
            method.violations, 0,
            "a method is not a field: {:?}",
            method.findings
        );
    }

    #[test]
    fn java_broad_throws_flagged() {
        let bad = analyze_api_design("public void f() throws Exception {}\n", "java");
        assert!(
            bad.violations >= 1,
            "throws Exception must flag: {:?}",
            bad.findings
        );
    }

    // ── C++ ─────────────────────────────────────────────────────────────────────
    #[test]
    fn cpp_function_macro_flagged_not_constant() {
        let bad = analyze_api_design("#define MAX(a, b) ((a) > (b) ? (a) : (b))\n", "cpp");
        assert!(
            bad.violations >= 1,
            "function-like macro must flag: {:?}",
            bad.findings
        );
        let ok = analyze_api_design("#define PI 3.14159\n#define GUARD_H\n", "cpp");
        assert_eq!(
            ok.violations, 0,
            "object-like macro / guard is fine: {:?}",
            ok.findings
        );
    }

    // ── Cross-cutting ──────────────────────────────────────────────────────────
    #[test]
    fn comments_and_tests_excluded() {
        // The smells live only in a comment and a #[cfg(test)] module.
        let src = "// pub fn get_x and Result<(), String> mentioned here\nfn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    pub fn get_y() {}\n}\n";
        let r = analyze_api_design(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment/test smells excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn unknown_language_is_empty() {
        let r = analyze_api_design("public int x; def f(x=[]) Result<(), String>", "haskell");
        assert_eq!(r.violations, 0);
        assert!((score_api_design(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| ApiDesignReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_api_design(&mk(w));
            assert!(s <= prev, "more contract debt must not raise the score");
            prev = s;
        }
    }
}
