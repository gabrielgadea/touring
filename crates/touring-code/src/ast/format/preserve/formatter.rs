//! PreservingFormatter — comment-preserving Rust formatter using prettyplease.
//!
//! v2 (2026-06-11, gotcha #49 root-cause fix): item boundaries now come from
//! real syn/proc-macro2 spans (`span-locations` feature) instead of a textual
//! keyword search. The old heuristic located items by scanning for `"fn "`,
//! `"struct "`, … from the previous item's end, which (a) landed AFTER doc
//! comments and visibility modifiers — duplicating them into the inter-item
//! gap (the infamous `pub /// …` compile error), (b) matched keywords inside
//! string literals and comments, and (c) treated macro items as zero-width
//! (`find("")`). A final semantic-equivalence gate re-parses the output and
//! compares canonical token streams: on ANY mismatch the original source is
//! returned unchanged — this formatter can no longer corrupt a file.

use syn::spanned::Spanned;

/// A formatter that preserves original whitespace and comments between AST nodes.
pub struct PreservingFormatter<'a> {
    source: &'a str,
}

impl<'a> PreservingFormatter<'a> {
    /// Create a formatter over the given source text.
    pub fn new(source: &'a str) -> Self {
        PreservingFormatter { source }
    }

    /// Format the source and return the preserved output.
    ///
    /// Fail-safe: returns the input unchanged when the source does not parse,
    /// when item spans are unavailable/non-monotonic, or when the formatted
    /// output is not semantically identical to the input.
    pub fn format(&mut self) -> String {
        let file = match syn::parse_file(self.source) {
            Ok(f) => f,
            Err(_) => return self.source.to_string(),
        };

        let mut out = String::new();
        let mut last_end = 0usize;

        for item in &file.items {
            // Real byte range of the item INCLUDING its attributes/doc
            // comments (they are part of the item's token stream).
            let range = item.span().byte_range();
            if range.start < last_end || range.end > self.source.len() || range.start >= range.end {
                // Span bookkeeping is off — never guess, never corrupt.
                tracing::warn!(
                    start = range.start,
                    end = range.end,
                    last_end,
                    "format_preserve: non-monotonic/out-of-bounds item span; \
                     returning source unchanged"
                );
                return self.source.to_string();
            }

            // Inter-item gap copied verbatim — free-standing `//` comments,
            // blank lines and the file's inner attributes/shebang survive.
            out.push_str(&self.source[last_end..range.start]);

            let synthetic = syn::File {
                shebang: None,
                attrs: Vec::new(),
                items: vec![item.clone()],
            };
            let formatted = prettyplease::unparse(&synthetic);
            out.push_str(formatted.trim_end());

            last_end = range.end;
        }

        if last_end < self.source.len() {
            out.push_str(&self.source[last_end..]);
        }

        // Equivalence gate: the output must parse to the same canonical
        // token stream as the input. Anything else → fail safe.
        if semantically_equal(self.source, &out) {
            out
        } else {
            tracing::warn!(
                "format_preserve: output failed the semantic-equivalence gate; \
                 returning source unchanged"
            );
            self.source.to_string()
        }
    }
}

/// True when both sources parse and their canonical (prettyplease) forms are
/// byte-identical — i.e. they are the same program modulo whitespace and
/// non-doc comments.
fn semantically_equal(a: &str, b: &str) -> bool {
    match (syn::parse_file(a), syn::parse_file(b)) {
        (Ok(fa), Ok(fb)) => prettyplease::unparse(&fa) == prettyplease::unparse(&fb),
        _ => false,
    }
}

/// Format Rust source with gap preservation.
pub fn format_preserve(source: &str) -> Result<String, crate::ast::surgery::SurgeryError> {
    let mut formatter = PreservingFormatter::new(source);
    Ok(formatter.format())
}

/// Check if formatting is idempotent on the given source.
pub fn is_idempotent(source: &str) -> bool {
    let first = format_preserve(source)
        .ok()
        .unwrap_or_else(|| source.to_string());
    let second = format_preserve(&first)
        .ok()
        .unwrap_or_else(|| first.clone());
    first == second
}

/// Check if a file has any `#[rustfmt::skip]` markers.
pub fn has_rustfmt_skip(source: &str) -> bool {
    source.contains("#[rustfmt::skip]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_preserve_normalizes() {
        let source = "fn  foo(  x : i32)->i32{x+1}";
        let formatted = format_preserve(source).expect("valid rust");
        assert!(formatted.contains("fn foo("), "should normalize fn foo");
        assert!(formatted.contains("i32)"), "should normalize params");
    }

    #[test]
    fn format_preserve_idempotent() {
        assert!(is_idempotent("fn foo() {}"));
        assert!(is_idempotent("fn foo() {}\nfn bar() {}"));
    }

    #[test]
    fn has_rustfmt_skip_detection() {
        assert!(has_rustfmt_skip("#[rustfmt::skip]\nfn foo() {}"));
        assert!(!has_rustfmt_skip("fn foo() {}"));
    }

    #[test]
    fn format_preserve_simple_fn() {
        let source = "fn hello() { 1 }";
        let formatted = format_preserve(source).expect("valid rust");
        assert!(formatted.contains("fn hello()"));
    }

    /// Gotcha #49 regression — the exact corruption shape observed 6× in
    /// production (2026-06-11): a doc-commented `pub struct` following a
    /// `const` came out as `pub /// Outcome …` (visibility duplicated into
    /// the gap, doc attr re-emitted after it) because the old keyword search
    /// found `"struct "` AFTER the doc comment and `pub`.
    #[test]
    fn gotcha49_doc_commented_pub_struct_after_const() {
        let source = "const DEFAULT_REPAIR_LIMIT: i64 = 200;\n\n\
                      /// Outcome of one repair run.\n\
                      pub struct RepairOutcome {\n    pub scanned: usize,\n}\n";
        let formatted = format_preserve(source).expect("valid rust");
        assert!(
            !formatted.contains("pub ///"),
            "doc comment must not be split from its item: {formatted}"
        );
        assert!(
            syn::parse_file(&formatted).is_ok(),
            "output must remain valid Rust: {formatted}"
        );
        assert_eq!(
            formatted.matches("Outcome of one repair run").count(),
            1,
            "doc comment must not be duplicated: {formatted}"
        );
    }

    /// Old heuristic matched item keywords inside string literals, shifting
    /// every subsequent boundary (the "interleaved strings" corruption).
    #[test]
    fn keyword_inside_string_literal_does_not_shift_boundaries() {
        let source = "fn a() {\n    let s = \"struct Fake { fn b \";\n}\n\n\
                      /// Real struct.\npub struct Real {\n    pub x: u8,\n}\n";
        let formatted = format_preserve(source).expect("valid rust");
        assert!(
            syn::parse_file(&formatted).is_ok(),
            "must parse: {formatted}"
        );
        assert_eq!(formatted.matches("struct Real").count(), 1);
        assert!(!formatted.contains("pub ///"), "no split docs: {formatted}");
    }

    /// `Item::Macro` had keyword "" → `find("") == 0` → zero-width item and
    /// catastrophic offset drift in macro-bearing files.
    #[test]
    fn top_level_macro_item_keeps_following_items_intact() {
        let source = "macro_rules! m {\n    () => {};\n}\n\n\
                      fn after_macro() -> u8 {\n    7\n}\n";
        let formatted = format_preserve(source).expect("valid rust");
        assert!(
            syn::parse_file(&formatted).is_ok(),
            "must parse: {formatted}"
        );
        assert_eq!(formatted.matches("fn after_macro").count(), 1);
        assert_eq!(formatted.matches("macro_rules! m").count(), 1);
    }

    /// The point of `--preserve`: free-standing `//` comments between items
    /// (which plain prettyplease drops) must survive.
    #[test]
    fn free_standing_comment_between_items_survives() {
        let source = "fn a() {}\n\n// load-bearing free comment\n\nfn b() {}\n";
        let formatted = format_preserve(source).expect("valid rust");
        assert!(
            formatted.contains("// load-bearing free comment"),
            "gap comments must be preserved: {formatted}"
        );
        assert!(syn::parse_file(&formatted).is_ok());
    }

    /// File-level inner attributes and module docs live in the leading gap
    /// and must come through verbatim.
    #[test]
    fn inner_attrs_and_module_doc_survive() {
        let source = "//! Module docs.\n#![allow(dead_code)]\n\nfn x() {}\n";
        let formatted = format_preserve(source).expect("valid rust");
        assert!(formatted.contains("//! Module docs."));
        assert!(formatted.contains("#![allow(dead_code)]"));
        assert!(syn::parse_file(&formatted).is_ok());
    }

    /// The fail-safe contract: for any parseable input, the output is
    /// semantically identical to the input (canonical forms match).
    #[test]
    fn output_is_always_semantically_equal_to_input() {
        let sources = [
            "const A: u8 = 1;\n/// d\npub struct S { pub x: u8 }\n",
            "use std::fmt;\n\nimpl fmt::Debug for () {}\n",
            "#[derive(Clone)]\npub enum E {\n    /// variant doc\n    V,\n}\n",
        ];
        for src in sources {
            let formatted = format_preserve(src).expect("valid rust");
            let a = prettyplease::unparse(&syn::parse_file(src).expect("in"));
            let b = prettyplease::unparse(&syn::parse_file(&formatted).expect("out"));
            assert_eq!(a, b, "semantic drift for input: {src}");
        }
    }
}
