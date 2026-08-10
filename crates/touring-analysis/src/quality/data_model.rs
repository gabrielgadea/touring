//! Data model (D10 / F1.10) — polyglot "make illegal states unrepresentable"
//! and primitive-obsession smells.
//!
//! D10 asks whether the data is modelled in *types* — `enum`s for mutually
//! exclusive states, newtypes for domain values, `Option` for absence — rather
//! than in raw primitives. The idiomatic *good* form (an `enum Status`, a
//! `struct UserId(u64)`, `bitflags`) cannot be proven present by a scanner, so
//! this engine detects the high-confidence **anti-patterns** the Rust API
//! Guidelines name directly: "use a newtype for static distinctions" and "use
//! types instead of `bool` or `Option` for arguments to convey meaning"
//! (`type-safety.html`, `dependability.html`).
//!
//! Three structural detectors, all language-aware:
//!   1. **Stringly-typed domain field** — a binding named `status`/`state`/
//!      `kind`/`mode`/… typed as a raw string (`String`/`string`/`str`) instead
//!      of a domain `enum`/newtype (primitive obsession; C-NEWTYPE).
//!   2. **Type-erasure escape** — `any` (TS), `interface{}` (Go), `Object`
//!      container (Java), `void*` (C/C++), `Any` (Python): the type model is
//!      discarded at the boundary.
//!   3. **Boolean-flag explosion** — an aggregate type (Rust / Go `struct`) with
//!      ≥3 `bool` fields: the 2^n combinations encode states that cannot all be
//!      valid → an `enum` or `bitflags` (Rust API Guidelines: "bitflags for sets
//!      of flags").
//!
//! | Lang | Anti-patterns | Idiomatic alternative |
//! |------|---------------|-----------------------|
//! | Rust | `status: String`, ≥3 `: bool` in a struct | `enum Status`, `bitflags!` |
//! | Python | `status: str`, `: Any` / `-> Any` | `enum.Enum`, a concrete type |
//! | TS / JS | `status: string`, `: any` / `as any` / `<any>` | a string-literal union, the real type |
//! | Go | `Status string`, `interface{}`, ≥3 struct `bool` | a typed `const`/iota, a concrete type |
//! | Java | `String status`, `<Object>` / `Object[]` | an `enum`, a generic |
//! | C / C++ | `string status`, `void*` | an `enum class`, a concrete pointer |
//!
//! It is disjoint from F1.9 api-design (contract surface: `Result<_,String>`,
//! getters, missing `Debug`), F1.11 design-patterns (GoF/ownership), F4.4
//! modernization, and F2.2 input-validation by construction — F1.10 scores the
//! *data shape*. (`-> String` returns and `Result<_, String>` errors are not
//! flagged here: the adjacent identifier is not a domain field name.) Comments
//! and `#[cfg(test)]`/test regions are excluded via `super::code_regions`.
//! Rolls up as `AggKind::WeightedLoc`. ADVISORY-tier. Zero non-std deps beyond
//! `memchr`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// Canonical language bucket (collapses extension aliases).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Python,
    TsJs,
    Go,
    Java,
    Cpp,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "python" | "py" => Lang::Python,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" => Lang::TsJs,
        "go" => Lang::Go,
        "java" => Lang::Java,
        "cpp" | "c++" | "cc" | "cxx" | "c" | "h" | "hpp" => Lang::Cpp,
        _ => Lang::Other,
    }
}

/// Domain concepts that are almost always a *closed set* — modelling them as a
/// raw string is primitive obsession (they want an `enum`/newtype). Curated to
/// exclude genuinely free-text fields (`name`, `title`, `description`,
/// `message`, `path`, `url`, …) so the detector stays high-precision.
const DOMAIN_WORDS: &[&[u8]] = &[
    b"status",
    b"state",
    b"kind",
    b"mode",
    b"phase",
    b"role",
    b"level",
    b"priority",
    b"severity",
    b"category",
    b"direction",
    b"color",
    b"colour",
];

fn is_domain_word(w: &[u8]) -> bool {
    // ASCII case-insensitive compare (fields are ASCII identifiers; covers Go's
    // exported `Status` as well as Rust's `status`).
    DOMAIN_WORDS
        .iter()
        .any(|d| d.len() == w.len() && d.iter().zip(w).all(|(a, b)| *a == b.to_ascii_lowercase()))
}

#[inline]
fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `true` if `bytes[idx]` exists and is an identifier char (word-boundary test).
#[inline]
fn ident_at(bytes: &[u8], idx: usize) -> bool {
    bytes.get(idx).is_some_and(|&c| is_ident(c))
}

/// The identifier immediately to the left of `start`, skipping interleaved
/// whitespace and `:` separators (so `status: String` and `Status  string`
/// both yield `status`). `None` if no identifier precedes.
fn ident_before(bytes: &[u8], start: usize) -> Option<&[u8]> {
    let mut i = start;
    while i > 0 {
        let c = bytes[i - 1];
        if c == b' ' || c == b'\t' || c == b':' {
            i -= 1;
        } else {
            break;
        }
    }
    let end = i;
    while i > 0 && is_ident(bytes[i - 1]) {
        i -= 1;
    }
    if i < end { Some(&bytes[i..end]) } else { None }
}

/// The identifier immediately to the right of `end`, skipping whitespace (so
/// `String status` / `std::string  status` yield `status`). `None` if none.
fn ident_after(bytes: &[u8], end: usize) -> Option<&[u8]> {
    let mut i = end;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && is_ident(bytes[i]) {
        i += 1;
    }
    if start < i {
        Some(&bytes[start..i])
    } else {
        None
    }
}

/// String-type token for `lang` + whether the field/var name comes *after* the
/// type (`true`: `String status`) or *before* (`false`: `status: String`).
fn string_type_token(lang: Lang) -> Option<(&'static [u8], bool)> {
    match lang {
        Lang::Rust => Some((b"String", false)),
        Lang::TsJs => Some((b"string", false)),
        Lang::Python => Some((b"str", false)),
        Lang::Go => Some((b"string", false)),
        Lang::Java => Some((b"String", true)),
        Lang::Cpp => Some((b"string", true)),
        Lang::Other => None,
    }
}

/// Detector 1 — stringly-typed domain fields: a `DOMAIN_WORDS` identifier typed
/// as a raw string. The type token must be a whole word (so `String` ≠
/// `StringBuilder`, `MyString`, `to_string`), and the adjacent identifier must
/// be an exact domain word (so `name: String` / `getStatus()` are not flagged).
fn stringly_typed_domain(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    let Some((tok, look_after)) = string_type_token(lang) else {
        return 0;
    };
    let mut n = 0;
    for off in memmem::find_iter(bytes, tok) {
        if offset_suppressed(off, regions) {
            continue;
        }
        // Whole-word: neither side is an identifier char (allows `:`/`<`/space).
        if ident_at(bytes, off.wrapping_sub(1)) {
            continue;
        }
        let after = off + tok.len();
        if ident_at(bytes, after) {
            continue;
        }
        let name = if look_after {
            ident_after(bytes, after)
        } else {
            ident_before(bytes, off)
        };
        if name.is_some_and(is_domain_word) {
            n += 1;
        }
    }
    n
}

/// Count occurrences of a word-like `needle` (not suppressed) whose *trailing*
/// char is a word boundary — so `: any` ≠ `: anything`, `-> Any` ≠ `Anybody`.
fn count_word(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .filter(|&off| !ident_at(bytes, off + needle.len()))
        .count()
}

/// Count occurrences of a literal `needle` (not suppressed). Used for needles
/// that already end in a non-identifier char (`interface{}`, `void*`, `<any>`).
fn count_lit(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// Detector 2 — type-erasure escapes (the type model is discarded at a
/// boundary). Each form is the canonical "lost type" smell for its language.
fn type_erasure(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    match lang {
        Lang::TsJs => {
            count_word(bytes, regions, b": any")
                + count_word(bytes, regions, b"as any")
                + count_lit(bytes, regions, b"<any>")
        }
        Lang::Go => {
            count_lit(bytes, regions, b"interface{}") + count_lit(bytes, regions, b"interface {}")
        }
        Lang::Java => {
            count_lit(bytes, regions, b"<Object>")
                + count_lit(bytes, regions, b", Object>")
                + count_lit(bytes, regions, b"Object[]")
        }
        Lang::Cpp => count_lit(bytes, regions, b"void*") + count_lit(bytes, regions, b"void *"),
        Lang::Python => {
            count_word(bytes, regions, b": Any") + count_word(bytes, regions, b"-> Any")
        }
        Lang::Rust | Lang::Other => 0,
    }
}

/// Find `[body_start, body_end)` ranges of aggregate-type bodies for `lang`
/// (Rust `struct … { … }`, Go `… struct { … }`). Only these two languages have
/// method-free struct bodies where byte-level bool-field counting is precise;
/// Java/C++/TS class bodies interleave methods and are deferred.
fn find_struct_blocks(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> Vec<(usize, usize)> {
    let header: &[u8] = match lang {
        Lang::Rust => b"struct ",
        Lang::Go => b"struct {",
        _ => return Vec::new(),
    };
    let mut blocks = Vec::new();
    for off in memmem::find_iter(bytes, header) {
        if offset_suppressed(off, regions) {
            continue;
        }
        // Word boundary before the `struct` keyword (so `my_struct`/`restructure`
        // do not match — the trailing space already excludes the latter).
        if ident_at(bytes, off.wrapping_sub(1)) {
            continue;
        }
        // Locate the opening brace of the body.
        let open = if lang == Lang::Go {
            off + header.len() - 1 // header already includes `{`
        } else {
            // Rust: scan forward for `{`, but bail on `;`/`(` (unit/tuple struct).
            let mut j = off + header.len();
            let mut found = None;
            while j < bytes.len() {
                let c = bytes[j];
                if !offset_suppressed(j, regions) {
                    if c == b'{' {
                        found = Some(j);
                        break;
                    }
                    if c == b';' || c == b'(' {
                        break;
                    }
                }
                j += 1;
            }
            match found {
                Some(b) => b,
                None => continue,
            }
        };
        // Brace-match to the closing brace.
        let mut depth = 0usize;
        let mut k = open;
        let mut close = None;
        while k < bytes.len() {
            if !offset_suppressed(k, regions) {
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
            }
            k += 1;
        }
        if let Some(c) = close {
            blocks.push((open + 1, c));
        }
    }
    blocks
}

/// Count whole-word `bool` *field* tokens within `[s, e)` (absolute offsets, so
/// region suppression and word boundaries are exact).
fn count_bool_fields(bytes: &[u8], regions: &[(usize, usize)], s: usize, e: usize) -> usize {
    let body = &bytes[s..e];
    let mut n = 0;
    for off in memmem::find_iter(body, b"bool") {
        let abs = s + off;
        if offset_suppressed(abs, regions) {
            continue;
        }
        if abs > 0 && is_ident(bytes[abs - 1]) {
            continue;
        }
        if ident_at(bytes, abs + 4) {
            continue;
        }
        n += 1;
    }
    n
}

/// Detector 3 — boolean-flag explosion: a struct with ≥3 `bool` fields.
fn bool_flag_explosion(bytes: &[u8], regions: &[(usize, usize)], lang: Lang) -> usize {
    if lang != Lang::Rust && lang != Lang::Go {
        return 0;
    }
    find_struct_blocks(bytes, regions, lang)
        .into_iter()
        .filter(|&(s, e)| count_bool_fields(bytes, regions, s, e) >= 3)
        .count()
}

/// Per-file data-model analysis (parallel shape to [`super::idioms::IdiomReport`]).
pub type DataModelReport = crate::quality::SmellReport;

/// Analyze data-model anti-patterns for `lang`. Unknown languages yield an
/// empty report (no model → no findings → score 1.0).
#[must_use]
pub fn analyze_data_model(source: &str, lang: &str) -> DataModelReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();
    let canon = canonical_lang(lang);
    let mut report = DataModelReport {
        total_lines: source.lines().count(),
        ..DataModelReport::default()
    };

    report.push(
        "stringly-typed domain field (status/state/kind/… : String) -> a domain `enum` or newtype (C-NEWTYPE)",
        stringly_typed_domain(bytes, &regions, canon),
        1.0,
    );
    report.push(
        "type-erasure escape (`any`/`interface{}`/`Object<>`/`void*`/`Any`) -> a concrete type",
        type_erasure(bytes, &regions, canon),
        1.0,
    );
    report.push(
        "≥3 `bool` fields in one type (combinatorial illegal states) -> an `enum` or `bitflags`",
        bool_flag_explosion(bytes, &regions, canon),
        2.0,
    );

    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// D10 data-model score: `1 - density * SCALE`, where density is the weighted
/// anti-pattern count per production line. A type-modelled file is `1.0`. SCALE
/// is the style-tier `6.0` (ADVISORY); `WeightedLoc` roll-up.
#[must_use]
pub fn score_data_model(report: &DataModelReport) -> f32 {
    const SCALE: f32 = 6.0;
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Rust ──────────────────────────────────────────────────────────────────
    #[test]
    fn rust_stringly_typed_flagged_enum_clean() {
        let bad = analyze_data_model(
            "struct S {\n    status: String,\n    kind: String,\n}\n",
            "rust",
        );
        assert!(
            bad.violations >= 2,
            "status+kind String must flag: {:?}",
            bad.findings
        );
        let good = analyze_data_model(
            "struct S {\n    status: Status,\n    kind: Kind,\n}\n",
            "rust",
        );
        assert_eq!(
            good.violations, 0,
            "enum-typed fields are clean: {:?}",
            good.findings
        );
        assert!((score_data_model(&good) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rust_free_text_field_not_flagged() {
        // `name`/`title`/`message` are legitimately strings — not domain words.
        let r = analyze_data_model(
            "struct S { name: String, title: String, message: String }\n",
            "rust",
        );
        assert_eq!(
            r.violations, 0,
            "free-text string fields are not a smell: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_string_builder_whole_word_guard() {
        // `StringBuilder`/`to_string`/`String::new` must not match the `String` token.
        let r = analyze_data_model(
            "let status = String::new();\nlet x: StringBuilder = b;\nlet s = v.to_string();\n",
            "rust",
        );
        assert_eq!(
            r.violations, 0,
            "non-field `String` occurrences excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_bool_explosion_flagged_two_clean() {
        let three = analyze_data_model(
            "struct Flags {\n    is_a: bool,\n    is_b: bool,\n    is_c: bool,\n}\n",
            "rust",
        );
        assert!(
            three.violations >= 1,
            "3 bool fields must flag: {:?}",
            three.findings
        );
        assert!(
            three.findings.iter().any(|(m, _)| m.contains("bool")),
            "evidence mentions bool explosion: {:?}",
            three.findings
        );
        let two = analyze_data_model("struct P { open: bool, dirty: bool }\n", "rust");
        assert_eq!(
            two.violations, 0,
            "2 bool fields are acceptable: {:?}",
            two.findings
        );
    }

    #[test]
    fn rust_bool_explosion_weight_is_two() {
        let r = analyze_data_model(
            "struct F {\n    a: bool,\n    b: bool,\n    c: bool,\n}\n",
            "rust",
        );
        // One explosion at weight 2.0.
        assert!(
            (r.weighted_total - 2.0).abs() < 1e-6,
            "explosion weight is 2.0: {}",
            r.weighted_total
        );
    }

    // ── Python ────────────────────────────────────────────────────────────────
    #[test]
    fn python_str_and_any_flagged() {
        let bad = analyze_data_model("class S:\n    status: str\n    payload: Any\n", "python");
        assert!(
            bad.violations >= 2,
            "status:str + :Any must flag: {:?}",
            bad.findings
        );
        let good = analyze_data_model("class S:\n    status: Status\n    name: str\n", "python");
        assert_eq!(
            good.violations, 0,
            "enum status + free-text name clean: {:?}",
            good.findings
        );
    }

    // ── TypeScript / JavaScript ─────────────────────────────────────────────────
    #[test]
    fn ts_any_and_string_flagged_union_clean() {
        let bad = analyze_data_model(
            "interface S {\n  status: string;\n  data: any;\n}\n",
            "typescript",
        );
        assert!(
            bad.violations >= 2,
            "status:string + :any must flag: {:?}",
            bad.findings
        );
        let good = analyze_data_model(
            "interface S {\n  status: \"on\" | \"off\";\n  name: string;\n}\n",
            "typescript",
        );
        assert_eq!(
            good.violations, 0,
            "literal union + free-text name clean: {:?}",
            good.findings
        );
    }

    // ── Go ──────────────────────────────────────────────────────────────────────
    #[test]
    fn go_string_interface_and_bool_explosion() {
        let bad = analyze_data_model(
            "type S struct {\n    Status string\n    Meta interface{}\n    A bool\n    B bool\n    C bool\n}\n",
            "go",
        );
        // Status string + interface{} + bool explosion (≥3).
        assert!(
            bad.violations >= 3,
            "Go data-model smells must flag: {:?}",
            bad.findings
        );
    }

    // ── Java ──────────────────────────────────────────────────────────────────
    #[test]
    fn java_object_container_and_status_flagged() {
        let bad = analyze_data_model(
            "class S {\n    String status;\n    List<Object> items;\n}\n",
            "java",
        );
        assert!(
            bad.violations >= 2,
            "String status + List<Object> must flag: {:?}",
            bad.findings
        );
        let good = analyze_data_model(
            "class S {\n    String name;\n    List<Item> items;\n}\n",
            "java",
        );
        assert_eq!(
            good.violations, 0,
            "free-text name + typed list clean: {:?}",
            good.findings
        );
    }

    // ── C / C++ ─────────────────────────────────────────────────────────────────
    #[test]
    fn cpp_voidptr_and_string_status_flagged() {
        let bad = analyze_data_model(
            "struct S {\n    std::string status;\n    void* payload;\n};\n",
            "cpp",
        );
        assert!(
            bad.violations >= 2,
            "string status + void* must flag: {:?}",
            bad.findings
        );
        let good = analyze_data_model(
            "struct S {\n    std::string name;\n    Widget* payload;\n};\n",
            "cpp",
        );
        assert_eq!(
            good.violations, 0,
            "free-text name + typed pointer clean: {:?}",
            good.findings
        );
    }

    // ── Cross-cutting ──────────────────────────────────────────────────────────
    #[test]
    fn comments_and_tests_excluded() {
        // The smells live only in a comment and a #[cfg(test)] module.
        let src = "// status: String and void* here are just docs\nfn prod() -> bool { true }\n#[cfg(test)]\nmod tests {\n    struct T { status: String, a: bool, b: bool, c: bool }\n}\n";
        let r = analyze_data_model(src, "rust");
        assert_eq!(
            r.violations, 0,
            "comment/test smells excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn unknown_language_is_empty() {
        let r = analyze_data_model("status: String  void*  interface{}  : any", "haskell");
        assert_eq!(r.violations, 0);
        assert!((score_data_model(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_density() {
        let mk = |w: f32| DataModelReport {
            weighted_total: w,
            total_lines: 100,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for w in [0.0, 1.0, 3.0, 6.0, 12.0] {
            let s = score_data_model(&mk(w));
            assert!(s <= prev, "more data-model debt must not raise the score");
            prev = s;
        }
    }
}
