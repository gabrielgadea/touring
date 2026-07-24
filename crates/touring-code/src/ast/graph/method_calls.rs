//! Method-call and associated-function-call extraction.
//!
//! Complements `imports.rs`: where imports capture STATIC graph edges
//! (`use foo::Bar;`), this module captures DYNAMIC dispatch sites
//! (`obj.method(...)`, `Foo::new(...)`) that the import-based wiring
//! map cannot see.
//!
//! Returns a deduplicated set of identifier names called somewhere in
//! the source — callers cross-reference those names against producer
//! rows in the wiring_map to record dispatch-time consumer edges.
//!
//! ## Why this exists (2026-05-11 audit)
//!
//! Before this module, the orphan_count diagnostic included ~2925
//! method producers (43% of the total) that were called in many files
//! but registered no consumer rows — because the only consumer-tracking
//! signal was `use foo::method;`, which is invalid Rust for methods.
//! Removing those false positives required walking the AST for call
//! sites and matching by symbol name (loose, name-only match, accepted
//! risk: a method called `clone` will wire every `pub fn clone` in the
//! workspace; that is the conservative direction — fewer false orphans,
//! never more).

use std::collections::HashSet;

use streaming_iterator::StreamingIterator;

use crate::ast::languages::Lang;

/// Extract every identifier that appears as a method name or as an
/// associated-function name in a call expression.
///
/// Returns an unordered, deduplicated set of identifier strings. Empty
/// vector for languages whose call-site wiring is already captured by
/// imports, or when the tree-sitter parse fails.
///
/// The returned identifiers are NOT validated against `is_valid_rust_ident`
/// here — tree-sitter only matches the grammar's `field_identifier` /
/// `identifier` productions, so the strings are already well-formed.
///
/// # Performance
///
/// Single tree-sitter pass per file with a streaming `QueryCursor`.
/// Allocations are dominated by the HashSet; typical input (~5-50 unique
/// method names per file) consumes microseconds.
pub fn extract_method_calls(source: &str, lang: Lang) -> Vec<String> {
    let query_src = lang.method_call_query_file();
    if query_src.is_empty() {
        return Vec::new();
    }

    let Some(names) = extract_method_calls_inner(source, lang, query_src) else {
        return Vec::new();
    };
    names.into_iter().collect()
}

fn extract_method_calls_inner(
    source: &str,
    lang: Lang,
    query_src: &str,
) -> Option<HashSet<String>> {
    use tree_sitter::{Parser, Query, QueryCursor};

    let mut parser = Parser::new();
    parser.set_language(&lang.tree_sitter_language()).ok()?;
    let tree = parser.parse(source, None)?;

    let ts_lang = lang.tree_sitter_language();
    let query = Query::new(&ts_lang, query_src).ok()?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut names: HashSet<String> = HashSet::new();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            if let Ok(text) = capture.node.utf8_text(source.as_bytes()) {
                if !text.is_empty() {
                    names.insert(text.to_string());
                }
            }
        }
    }
    Some(names)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn extracts_field_method_call() {
        let src = r#"
fn caller(obj: &Foo) {
    obj.do_thing();
}
"#;
        let names = extract_method_calls(src, Lang::Rust);
        assert!(
            names.contains(&"do_thing".to_string()),
            "expected do_thing in {:?}",
            names
        );
    }

    #[test]
    fn extracts_associated_fn_call() {
        let src = r#"
fn caller() -> Foo {
    Foo::new()
}
"#;
        let names = extract_method_calls(src, Lang::Rust);
        assert!(
            names.contains(&"new".to_string()),
            "expected new in {:?}",
            names
        );
    }

    #[test]
    fn extracts_chained_calls() {
        let src = r#"
fn caller(v: Vec<i32>) -> usize {
    v.iter().filter(|x| **x > 0).count()
}
"#;
        let names = extract_method_calls(src, Lang::Rust);
        assert!(names.contains(&"iter".to_string()));
        assert!(names.contains(&"filter".to_string()));
        assert!(names.contains(&"count".to_string()));
    }

    #[test]
    fn deduplicates_repeated_calls() {
        let src = r#"
fn caller(a: &Foo, b: &Foo) {
    a.do_it();
    b.do_it();
    a.do_it();
}
"#;
        let names = extract_method_calls(src, Lang::Rust);
        // HashSet collected then exposed as Vec — count `do_it` occurrences.
        let do_it_count = names.iter().filter(|n| n.as_str() == "do_it").count();
        assert_eq!(do_it_count, 1, "duplicates not deduped: {:?}", names);
    }

    #[test]
    fn returns_empty_for_unsupported_language() {
        let names = extract_method_calls("x = 1", Lang::Python);
        assert!(names.is_empty());
    }

    #[test]
    fn returns_empty_for_files_with_no_calls() {
        let src = "struct Foo { x: i32 }\n";
        let names = extract_method_calls(src, Lang::Rust);
        assert!(names.is_empty(), "expected no calls, got {:?}", names);
    }

    #[test]
    fn captures_generic_method_call() {
        let src = r#"
fn caller<T>(v: &Vec<T>) {
    v.iter::<i32>().collect::<Vec<_>>();
}
"#;
        let names = extract_method_calls(src, Lang::Rust);
        assert!(names.contains(&"iter".to_string()));
        assert!(names.contains(&"collect".to_string()));
    }
}
