//! Component boundaries (D07 / F1.7) — encapsulation surface analysis.
//!
//! Measures how much of a module's surface is exposed — the canonical signal of
//! a *leaky abstraction* per the Rust API Guidelines "Future proofing" checklist
//! (sealed traits, **private struct fields**, newtypes, minimal public surface):
//!
//! 1. **Private struct fields (C-STRUCT-PRIVATE)** — `pub` fields leak the
//!    representation and prevent invariant enforcement; the strongest, least
//!    ambiguous boundary signal. A struct with all-`pub` fields is an exposed
//!    data bag, not an abstraction.
//! 2. **Public surface ratio** — proportion of top-level items that are truly
//!    `pub` (vs `pub(crate)`/`pub(super)`/private). A module that exposes
//!    everything has no boundary.
//! 3. **Restricted-visibility credit** — `pub(crate)`/`pub(super)`/`pub(in ..)`
//!    items are *not* surface leaks; they are deliberate encapsulation, so they
//!    are excluded from the exposure numerator.
//!
//! This replaces a substring stub that counted only lines beginning with
//! `pub fn`/`pub struct`/… — it was blind to `pub(crate)` (counted as neither
//! leak nor encapsulation) and to `pub` struct fields entirely, and used an
//! arbitrary `pub_count / 50` threshold that punished any large public API.
//!
//! Zero non-std dependencies: a brace-aware line scanner reusing
//! `super::code_regions` to skip comments and `#[cfg(test)]` regions.
//!
//! **Scope note**: this is an *intra-file* surface measure. D07's cross-module
//! "pub symbol with zero consumers → re-encapsulate" check is a *wiring* concern
//! (F1.8 / `touring wiring impact`), out of scope for a per-file scanner. Inline
//! submodule (`mod x { … }`) items and tuple-struct fields are not classified
//! (the common case is one module per file with named-field structs).

use super::code_regions::{non_executable_regions, offset_suppressed};

/// Visibility class of a top-level item or struct field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vis {
    /// Full `pub` — the exposed module surface.
    Public,
    /// `pub(crate)` / `pub(super)` / `pub(in ..)` — deliberate encapsulation.
    Restricted,
    /// Inherited (private).
    Private,
}

/// Boundary / encapsulation analysis for a source buffer.
#[derive(Debug, Clone, Default)]
pub struct BoundaryReport {
    /// Top-level items with full `pub` visibility (the exposed surface).
    pub public_items: usize,
    /// Top-level items with restricted visibility (`pub(crate)`/`pub(super)`/`pub(in)`).
    pub restricted_items: usize,
    /// Top-level items that are private (inherited visibility).
    pub private_items: usize,
    /// Struct fields declared full `pub` (C-STRUCT-PRIVATE violation).
    pub pub_fields: usize,
    /// Total named struct fields seen (pub + restricted + private).
    pub struct_fields: usize,
    /// `public_items / (public + restricted + private)` in `[0, 1]`.
    pub exposure_ratio: f64,
}

impl BoundaryReport {
    /// Total classified top-level items.
    #[must_use]
    pub fn total_items(&self) -> usize {
        self.public_items + self.restricted_items + self.private_items
    }
}

/// Split off a leading visibility prefix, returning its class and the remainder.
fn vis_prefix(trimmed: &str) -> (Vis, &str) {
    let Some(rest) = trimmed.strip_prefix("pub") else {
        return (Vis::Private, trimmed);
    };
    match rest.chars().next() {
        // `pub(crate)` / `pub(super)` / `pub(in path)`
        Some('(') => match rest.find(')') {
            Some(close) => (Vis::Restricted, rest[close + 1..].trim_start()),
            None => (Vis::Restricted, ""),
        },
        // `pub <item>`
        Some(c) if c.is_whitespace() => (Vis::Public, rest.trim_start()),
        // bare `pub` at end of line (e.g. a wrapped declaration)
        None => (Vis::Public, ""),
        // `pubfoo` — not a visibility keyword
        _ => (Vis::Private, trimmed),
    }
}

/// Classify a column-0 line as a top-level item, returning its visibility and
/// whether it begins a named-field struct/union. Returns `None` for non-items
/// (`use`, `impl`, attributes, `extern crate`, expressions).
/// Classify a top-level item's visibility, dispatching per language. Only Rust
/// tracks struct fields (the second tuple element `is_struct`); other languages
/// return `is_struct = false` so field-tracking (Rust `pub x: T`) never
/// contaminates a non-Rust score — the polyglot signal is item-level exposure
/// (P-F, 2026-07-03: closes the Rust-`pub`-heuristic-on-non-Rust silent pass).
fn classify_item(trimmed: &str, lang: &str) -> Option<(Vis, bool)> {
    match lang {
        "python" => classify_item_python(trimmed),
        "typescript" | "ts" | "javascript" | "js" => classify_item_tsjs(trimmed),
        "go" => classify_item_go(trimmed),
        "java" => classify_item_java(trimmed),
        _ => classify_item_rust(trimmed),
    }
}

fn classify_item_rust(trimmed: &str) -> Option<(Vis, bool)> {
    let (vis, rest) = vis_prefix(trimmed);
    const QUALIFIERS: &[&str] = &["async", "unsafe", "default", "auto", "extern"];
    for word in rest.split_whitespace() {
        if word.starts_with('"') {
            continue; // `extern "C"` ABI string
        }
        match word.trim_end_matches('!') {
            "fn" | "enum" | "trait" | "mod" | "const" | "static" | "type" | "macro_rules"
            | "macro" => return Some((vis, false)),
            "struct" | "union" => return Some((vis, true)),
            "crate" => return None, // `extern crate foo;`
            w if QUALIFIERS.contains(&w) => continue,
            _ => return None, // first significant token is not an item keyword
        }
    }
    None
}

/// TypeScript / JavaScript: `export` (incl. `export default`) marks the public
/// module surface. Counts declared items only (class/interface/function/type/
/// enum), not top-level statements or `const`/`let`/`var` bindings.
fn classify_item_tsjs(trimmed: &str) -> Option<(Vis, bool)> {
    let (is_pub, rest) = match trimmed.strip_prefix("export default ") {
        Some(r) => (true, r),
        None => match trimmed.strip_prefix("export ") {
            Some(r) => (true, r),
            None => (false, trimmed),
        },
    };
    let kw = strip_ts_modifiers(rest)
        .split(|c: char| c.is_whitespace() || c == '(' || c == '<')
        .next()?;
    if !matches!(kw, "class" | "interface" | "function" | "type" | "enum") {
        return None;
    }
    Some((if is_pub { Vis::Public } else { Vis::Private }, false))
}

/// Strip leading TS/JS declaration modifiers (`declare`/`abstract`/`async`/
/// `const`/`default`) so the item keyword surfaces.
fn strip_ts_modifiers(rest: &str) -> &str {
    let mut rest = rest.trim_start();
    for modifier in ["declare ", "abstract ", "async ", "const ", "default "] {
        if let Some(stripped) = rest.strip_prefix(modifier) {
            rest = stripped.trim_start();
        }
    }
    rest
}

/// Python: no keyword visibility — a leading underscore marks the private
/// convention. Counts top-level `def` / `class` only.
fn classify_item_python(trimmed: &str) -> Option<(Vis, bool)> {
    let rest = trimmed
        .strip_prefix("async def ")
        .or_else(|| trimmed.strip_prefix("def "))
        .or_else(|| trimmed.strip_prefix("class "))?;
    let name = rest
        .split(|c: char| c == '(' || c == ':' || c == '[' || c.is_whitespace())
        .next()?;
    if name.is_empty() {
        return None;
    }
    let vis = if name.starts_with('_') {
        Vis::Private
    } else {
        Vis::Public
    };
    Some((vis, false))
}

/// Go: an item is exported iff its identifier is capitalized. Counts top-level
/// `func` / `type` / `var` / `const` (a method's receiver group is skipped).
fn classify_item_go(trimmed: &str) -> Option<(Vis, bool)> {
    let rest = ["func ", "type ", "var ", "const "]
        .iter()
        .find_map(|kw| trimmed.strip_prefix(kw))?;
    let name = go_item_name(rest)?;
    let vis = if name.chars().next()?.is_uppercase() {
        Vis::Public
    } else {
        Vis::Private
    };
    Some((vis, false))
}

/// The declared name in a Go item line, skipping a leading method receiver
/// group `(r *T)`.
fn go_item_name(rest: &str) -> Option<&str> {
    let rest = rest.trim_start();
    let rest = if rest.starts_with('(') {
        rest[rest.find(')')? + 1..].trim_start()
    } else {
        rest
    };
    let name = rest
        .split(|c: char| c == '(' || c == '[' || c == '{' || c.is_whitespace())
        .next()?;
    (!name.is_empty()).then_some(name)
}

/// Java: modifier-based visibility (`public` / `protected` / `private`;
/// package-private = restricted). Counts type declarations and, at column 0,
/// explicitly-modified members — a rough surface heuristic (Java has no module
/// keyword); F1.7 is advisory so imprecision never blocks.
fn classify_item_java(trimmed: &str) -> Option<(Vis, bool)> {
    let is_type = trimmed.contains("class ")
        || trimmed.contains("interface ")
        || trimmed.contains("enum ")
        || trimmed.contains("record ");
    let has_modifier = trimmed.starts_with("public ")
        || trimmed.starts_with("private ")
        || trimmed.starts_with("protected ");
    if !is_type && !has_modifier {
        return None;
    }
    let vis = if trimmed.starts_with("public ") {
        Vis::Public
    } else if trimmed.starts_with("private ") {
        Vis::Private
    } else {
        Vis::Restricted // protected or package-private
    };
    Some((vis, false))
}

/// Classify a struct-body line as a named field, returning whether it is `pub`.
/// Returns `None` for attributes, doc lines, nested braces, etc.
fn classify_field(trimmed: &str) -> Option<bool> {
    let (vis, rest) = vis_prefix(trimmed);
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == 0 {
        return None; // does not start with an identifier
    }
    let after = rest[i..].trim_start();
    // a field is `ident: Type` — a single colon, never `ident::` (a path).
    if after.starts_with(':') && !after.starts_with("::") {
        Some(vis == Vis::Public)
    } else {
        None
    }
}

/// Net brace balance of a code line (`{` minus `}`).
fn brace_delta(line: &str) -> i32 {
    line.matches('{').count() as i32 - line.matches('}').count() as i32
}

/// Line-by-line boundary scanner state (kept small to bound the per-line CC).
#[derive(Default)]
struct Scanner {
    report: BoundaryReport,
    in_struct: bool,
    struct_depth: i32,
}

impl Scanner {
    /// Feed one non-empty, non-suppressed production line.
    fn feed(&mut self, content: &str, lead_ws: usize, trimmed: &str, lang: &str) {
        if self.in_struct {
            self.feed_struct_body(content, trimmed);
        } else if lead_ws == 0 {
            // Top-level items live at column 0 (one module per file).
            self.feed_top_level(content, trimmed, lang);
        }
    }

    fn feed_struct_body(&mut self, content: &str, trimmed: &str) {
        if let Some(is_pub) = classify_field(trimmed) {
            self.report.struct_fields += 1;
            if is_pub {
                self.report.pub_fields += 1;
            }
        }
        self.struct_depth += brace_delta(content);
        if self.struct_depth <= 0 {
            self.in_struct = false;
        }
    }

    fn feed_top_level(&mut self, content: &str, trimmed: &str, lang: &str) {
        let Some((vis, is_struct)) = classify_item(trimmed, lang) else {
            return;
        };
        match vis {
            Vis::Public => self.report.public_items += 1,
            Vis::Restricted => self.report.restricted_items += 1,
            Vis::Private => self.report.private_items += 1,
        }
        if is_struct && content.contains('{') {
            self.struct_depth = brace_delta(content);
            self.in_struct = self.struct_depth > 0; // single-line `struct X {}` stays closed
        }
    }
}

/// Analyze intra-file component boundaries. `lang` selects the comment/string
/// lexer for `code_regions` (`"rust"`, `"python"`, …); the visibility model is
/// Rust-specific, so non-Rust input yields a vacuous (all-zero) report.
#[must_use]
pub fn analyze_boundaries(source: &str, lang: &str) -> BoundaryReport {
    let regions = non_executable_regions(source, lang);
    let mut sc = Scanner::default();
    let mut line_start = 0usize;

    for raw_line in source.split_inclusive('\n') {
        let content = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let lead_ws = content.len() - content.trim_start().len();
        let suppressed = offset_suppressed(line_start + lead_ws, &regions);
        line_start += raw_line.len();
        let trimmed = content.trim();
        // comment / `#[cfg(test)]` regions and blank lines carry no surface.
        if !suppressed && !trimmed.is_empty() {
            sc.feed(content, lead_ws, trimmed, lang);
        }
    }

    let mut r = sc.report;
    let total = r.total_items();
    r.exposure_ratio = if total > 0 {
        r.public_items as f64 / total as f64
    } else {
        0.0
    };
    r
}

/// D07 boundary score. Two signals, weighted by how unambiguous they are:
///
/// * **field leak** (C-STRUCT-PRIVATE) — fraction of struct fields that are
///   `pub`; costs up to 0.4.
/// * **exposure** (moderate) — only penalised above 60%, since a public API
///   module legitimately exposes `pub` functions; costs up to 0.2.
///
/// The two combine so that a *pure* public data bag (all-`pub` fields **and**
/// high item exposure, e.g. a lone `pub struct Config { pub a, pub b }`) fails
/// (~0.4), while a result/DTO type mixed in among private logic (`pub` fields
/// but low item exposure) lands in advisory Silver rather than Fail. The field
/// weight was calibrated from `0.5` → `0.4` after scoring real files: `pub`
/// fields on public result types (`*Report`, `DimScore`) are idiomatic Rust and
/// should be a *mild* advisory smell, not a per-file Fail (an intra-file scanner
/// cannot tell a deliberate DTO from an invariant-bearing struct; D07 is ADVISORY).
///
/// An empty / re-export-only file (no classified items) is a vacuous pass.
#[must_use]
pub fn score_boundaries(report: &BoundaryReport) -> f32 {
    if report.total_items() == 0 {
        return 1.0;
    }
    let field_leak = if report.struct_fields > 0 {
        report.pub_fields as f32 / report.struct_fields as f32
    } else {
        0.0
    };
    let exposure = report.exposure_ratio as f32;
    let field_penalty = field_leak * 0.4;
    let exposure_penalty = (exposure - 0.6).max(0.0) * 0.5;
    (1.0 - field_penalty - exposure_penalty).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_encapsulated_module_scores_high() {
        let src = "\
pub fn api() {}
fn helper() {}
pub(crate) fn internal() {}
struct Inner {
    secret: u32,
    cache: String,
}
";
        let r = analyze_boundaries(src, "rust");
        assert_eq!(r.public_items, 1, "only `api` is pub");
        assert_eq!(r.restricted_items, 1, "`internal` is pub(crate)");
        assert_eq!(r.private_items, 2, "`helper` + `struct Inner`");
        assert_eq!(r.pub_fields, 0);
        assert_eq!(r.struct_fields, 2);
        assert!(score_boundaries(&r) > 0.95, "got {}", score_boundaries(&r));
    }

    // ── P-F (2026-07-03): polyglot visibility — a non-Rust file no longer
    //    reads as 0 items → silent 1.0. Each language uses its own visibility
    //    model (export / capitalization / `_` convention / modifier).

    #[test]
    fn typescript_export_is_the_public_surface() {
        let src = "\
export class PublicApi {}
export function helper() {}
class InternalOnly {}
interface Shape {}
";
        let r = analyze_boundaries(src, "typescript");
        assert_eq!(r.public_items, 2, "exported class + function");
        assert_eq!(r.private_items, 2, "non-exported class + interface");
        assert_eq!(r.restricted_items, 0);
        assert!(
            r.total_items() > 0,
            "must not read as empty (no silent 1.0)"
        );
    }

    #[test]
    fn python_underscore_convention_is_private() {
        let src = "\
def public_fn():
    pass

def _private_fn():
    pass

class Model:
    pass
";
        let r = analyze_boundaries(src, "python");
        assert_eq!(r.public_items, 2, "public_fn + Model");
        assert_eq!(r.private_items, 1, "_private_fn");
    }

    #[test]
    fn go_capitalization_is_export() {
        let src = "\
func Exported() {}
func internal() {}
type PublicType struct {}
func (r *T) Method() {}
";
        let r = analyze_boundaries(src, "go");
        assert_eq!(
            r.public_items, 3,
            "Exported + PublicType + Method (capitalized)"
        );
        assert_eq!(r.private_items, 1, "internal");
    }

    #[test]
    fn java_modifier_is_visibility() {
        let src = "\
public class Service {}
private class Impl {}
";
        let r = analyze_boundaries(src, "java");
        assert_eq!(r.public_items, 1, "public class");
        assert_eq!(r.private_items, 1, "private class");
    }

    #[test]
    fn polyglot_high_exposure_scores_below_rust_silent_pass() {
        // Before P-F this TS file read as 0 items → score 1.0 (silent pass).
        // Now every symbol is exported → high exposure → a real, lower score.
        let all_public = "\
export class A {}
export class B {}
export function c() {}
export function d() {}
export type E = number;
";
        let r = analyze_boundaries(all_public, "typescript");
        assert_eq!(r.public_items, 5);
        assert!(
            (r.exposure_ratio - 1.0).abs() < 1e-9,
            "all-exported → exposure 1.0, got {}",
            r.exposure_ratio
        );
        assert!(
            score_boundaries(&r) < 1.0,
            "high non-Rust exposure must score below the old silent 1.0"
        );
    }

    #[test]
    fn pub_field_data_bag_is_penalised() {
        // The C-STRUCT-PRIVATE anti-pattern: a struct that is all public fields.
        let src = "\
pub struct Config {
    pub host: String,
    pub port: u16,
    pub retries: u8,
}
";
        let r = analyze_boundaries(src, "rust");
        assert_eq!(r.pub_fields, 3);
        assert_eq!(r.struct_fields, 3);
        assert_eq!(r.public_items, 1);
        let s = score_boundaries(&r);
        assert!(s < 0.5, "all-pub-field bag should warn/fail, got {s}");
    }

    #[test]
    fn restricted_visibility_is_not_a_leak() {
        // `pub(crate)` everywhere is disciplined encapsulation, not exposure.
        let src = "\
pub(crate) fn a() {}
pub(crate) fn b() {}
pub(crate) struct C {
    x: u32,
}
";
        let r = analyze_boundaries(src, "rust");
        assert_eq!(r.public_items, 0);
        assert_eq!(r.restricted_items, 3);
        assert!((r.exposure_ratio - 0.0).abs() < 1e-9);
        assert!((score_boundaries(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn public_api_of_functions_is_not_over_punished() {
        // A `lib.rs`-style public API (all pub fns, no pub fields) is legitimate.
        let src = "\
pub fn one() {}
pub fn two() {}
pub fn three() {}
pub fn four() {}
";
        let r = analyze_boundaries(src, "rust");
        assert_eq!(r.public_items, 4);
        assert!((r.exposure_ratio - 1.0).abs() < 1e-9);
        let s = score_boundaries(&r);
        assert!(s >= 0.8, "pure public fn API should pass, got {s}");
    }

    #[test]
    fn pub_crate_field_is_not_counted_as_leak() {
        let src = "\
pub struct S {
    pub(crate) cached: u32,
    private_seed: u64,
}
";
        let r = analyze_boundaries(src, "rust");
        assert_eq!(r.struct_fields, 2);
        assert_eq!(
            r.pub_fields, 0,
            "pub(crate) field is encapsulated, not a leak"
        );
    }

    #[test]
    fn impl_methods_and_use_are_not_top_level_items() {
        let src = "\
pub use crate::other::Thing;
pub struct T;
impl T {
    pub fn method(&self) {}
    fn helper(&self) {}
}
";
        let r = analyze_boundaries(src, "rust");
        // Only `pub struct T` counts; `pub use`, `impl`, and methods do not.
        assert_eq!(r.public_items, 1);
        assert_eq!(r.total_items(), 1);
    }

    #[test]
    fn comments_and_test_modules_excluded() {
        let src = "\
// pub fn commented_out() {}
pub fn real() {}
#[cfg(test)]
mod tests {
    pub struct TestBag {
        pub field: u32,
    }
}
";
        let r = analyze_boundaries(src, "rust");
        assert_eq!(
            r.public_items, 1,
            "only the real pub fn; comment + test excluded"
        );
        assert_eq!(
            r.pub_fields, 0,
            "test-region pub field is not production surface"
        );
    }

    #[test]
    fn qualified_fns_are_classified() {
        let src = "\
pub async fn a() {}
pub unsafe fn b() {}
pub const fn c() -> u32 { 0 }
const MAX: u32 = 9;
";
        let r = analyze_boundaries(src, "rust");
        assert_eq!(r.public_items, 3, "async/unsafe/const fn all pub");
        assert_eq!(r.private_items, 1, "the bare const item");
    }

    #[test]
    fn empty_and_reexport_only_file_is_vacuous_pass() {
        assert!((score_boundaries(&analyze_boundaries("", "rust")) - 1.0).abs() < 1e-6);
        let reexports = "pub use crate::a::Foo;\npub use crate::b::Bar;\n";
        let r = analyze_boundaries(reexports, "rust");
        assert_eq!(r.total_items(), 0);
        assert!((score_boundaries(&r) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_is_monotone_in_field_leak() {
        let mk = |pubf: usize| BoundaryReport {
            public_items: 1,
            struct_fields: 4,
            pub_fields: pubf,
            exposure_ratio: 1.0,
            ..Default::default()
        };
        let mut prev = 2.0f32;
        for pubf in [0, 1, 2, 3, 4] {
            let s = score_boundaries(&mk(pubf));
            assert!(s <= prev, "more pub fields must not raise score");
            prev = s;
        }
    }
}
