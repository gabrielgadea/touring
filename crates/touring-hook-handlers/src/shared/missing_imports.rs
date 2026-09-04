//! S3 (2026-09-02) — `MissingImportsLayer`: predicts E0412/E0433 ("cannot find
//! type" / "failed to resolve") for a Rust file ABOUT TO BE WRITTEN.
//!
//! `pre_edit` already has `detect_unresolved_types`, but it reads the current
//! file's imports from the FileKnowledgeDB — a file that does not exist yet has
//! no row there, so `pre_write` had nothing. Here the imports come from the
//! PROPOSED content itself (`extract_imports_resolved`, the parser behind the
//! until-now orphan `ImportResolver::detect_missing_imports`), and the known
//! types come from ONE indexed lookup of the unresolved names
//! (`find_pub_symbols_by_name`) instead of the full-map scan `all_pub_symbols`
//! costs on a 190k-row wiring map.
//!
//! The DB step runs at construction (`for_write`) because pipeline layers are
//! `'static` and cannot borrow the runtime — the same shape as the up-front
//! signals of `pre_write`. The layer still exists as a layer so `LayerMetrics`
//! measures it by name (F9 feeds on that).

use crate::shared::signal_pipeline::{ProposedChange, SignalContext, SignalLayer};
use touring_code::ast::import_resolver::declared_type_names;
use touring_code::ast::wiring::suggest_imports_for;
use touring_code::ast::{Lang, extract_imports_resolved};

/// Score of an import suggestion: advisory — a missing `use` is a compile
/// error the author sees seconds later, not a security event — but above the
/// antipattern noise floor so the budget keeps it.
const MISSING_IMPORT_SCORE: f32 = 0.75;

/// PascalCase type references in `content` that nothing in `content` brings
/// into scope (imports incl. grouped/aliased/glob, own declarations, builtins,
/// path-qualified uses all excluded).
pub fn unresolved_rust_types(content: &str) -> Vec<String> {
    let resolver = extract_imports_resolved(content, Lang::Rust);
    let local = declared_type_names(content);
    resolver.unresolved_type_references(content, &local)
}

/// Import suggestions for a proposed Rust file.
///
/// `lookup` receives the unresolved names and returns `(symbol_name,
/// module_file)` producer rows; it is NOT called when nothing is unresolved,
/// so a clean file costs zero DB work. Returns one `(score, text)` per name
/// that has a known producer; names the map does not know stay silent (the
/// map is the precision guard — a stray capitalised word in a string literal
/// cannot become a suggestion unless a pub symbol carries that exact name).
pub fn missing_import_signals<F>(rel_path: &str, content: &str, lookup: F) -> Vec<(f32, String)>
where
    F: FnOnce(&[String]) -> Vec<(String, String)>,
{
    if !rel_path.ends_with(".rs") || content.trim().is_empty() {
        return Vec::new();
    }
    let unresolved = unresolved_rust_types(content);
    if unresolved.is_empty() {
        return Vec::new();
    }
    let known = lookup(&unresolved);
    if known.is_empty() {
        return Vec::new();
    }
    suggest_imports_for(&unresolved, &known, Some(rel_path))
        .into_iter()
        .map(|s| {
            (
                MISSING_IMPORT_SCORE,
                format!(
                    "[import] `{name}` is used but nothing brings it into scope — add `use {module}::{name};` ({reason})",
                    name = s.symbol_name,
                    module = s.source_module,
                    reason = s.reason
                ),
            )
        })
        .collect()
}

/// The pipeline layer. Built per `pre_write` call with the suggestions already
/// resolved (see module docs for why the DB step is at construction).
pub struct MissingImportsLayer {
    signals: Vec<(f32, String)>,
}

impl MissingImportsLayer {
    /// Build the layer for a proposed WRITE of `rel_path` with `content`.
    pub fn for_write<F>(rel_path: &str, content: &str, lookup: F) -> Self
    where
        F: FnOnce(&[String]) -> Vec<(String, String)>,
    {
        Self {
            signals: missing_import_signals(rel_path, content, lookup),
        }
    }

    /// `true` when the layer will emit nothing (clean file or unknown names).
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

impl SignalLayer for MissingImportsLayer {
    fn name(&self) -> &'static str {
        "missing_imports"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        let is_rust_write = matches!(ctx.proposed, Some(ProposedChange::Write { .. }))
            && ctx.file_path.ends_with(".rs");
        if !is_rust_write {
            return Vec::new();
        }
        self.signals.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const KNOWN: &str = "crates/touring-code/src/tfidf.rs";

    fn known_tfidf(names: &[String]) -> Vec<(String, String)> {
        names
            .iter()
            .filter(|n| n.as_str() == "TfIdfVectorizer")
            .map(|n| (n.clone(), KNOWN.to_string()))
            .collect()
    }

    #[test]
    fn flags_a_known_pub_type_used_without_import_in_a_proposed_new_rs_file() {
        let content = "pub fn build() -> TfIdfVectorizer {\n    TfIdfVectorizer::default()\n}\n";
        let signals = missing_import_signals("crates/touring-cli/src/main.rs", content, known_tfidf);
        assert_eq!(signals.len(), 1, "{signals:?}");
        let (score, text) = &signals[0];
        assert!((*score - MISSING_IMPORT_SCORE).abs() < f32::EPSILON);
        assert!(text.starts_with("[import] `TfIdfVectorizer`"), "{text}");
        assert!(
            text.contains("use touring_code::tfidf::TfIdfVectorizer;"),
            "crate-aware path expected: {text}"
        );
    }

    #[test]
    fn silent_when_the_content_imports_declares_or_qualifies_the_type() {
        let never = |names: &[String]| -> Vec<(String, String)> {
            panic!("lookup must not run for a resolved file, got {names:?}")
        };
        let cases = [
            "use crate::tfidf::TfIdfVectorizer;\nfn f() -> TfIdfVectorizer { TfIdfVectorizer::default() }\n",
            "use crate::tfidf::{Other, TfIdfVectorizer};\nfn f() -> TfIdfVectorizer { Other::new() }\n",
            "pub struct TfIdfVectorizer;\nfn f() -> TfIdfVectorizer { TfIdfVectorizer }\n",
            "fn f() { let v = crate::tfidf::TfIdfVectorizer::default(); let _ = v; }\n",
            "use crate::tfidf::*;\nfn f() -> TfIdfVectorizer { TfIdfVectorizer::default() }\n",
        ];
        for content in cases {
            assert!(
                missing_import_signals("src/x.rs", content, never).is_empty(),
                "expected silence for:\n{content}"
            );
        }
    }

    #[test]
    fn lookup_receives_only_the_unresolved_names_and_a_map_miss_stays_silent() {
        let seen: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let content = "use crate::a::Bar;\nfn f(b: Bar) -> Foo { Foo::from(b) }\n";
        let signals = missing_import_signals("src/x.rs", content, |names| {
            seen.borrow_mut().extend(names.iter().cloned());
            Vec::new()
        });
        assert!(signals.is_empty(), "a name the map does not know is not a suggestion");
        assert_eq!(*seen.borrow(), vec!["Foo".to_string()], "Bar is imported");
    }

    #[test]
    fn non_rust_files_and_empty_content_never_reach_the_lookup() {
        let never = |_: &[String]| -> Vec<(String, String)> { panic!("no lookup") };
        assert!(missing_import_signals("src/x.py", "x: Foo = Foo()", never).is_empty());
        assert!(missing_import_signals("src/x.rs", "   \n", never).is_empty());
    }

    #[test]
    fn layer_emits_only_for_a_proposed_rust_write() {
        let content = "fn f() -> TfIdfVectorizer { TfIdfVectorizer::default() }\n";
        let layer = MissingImportsLayer::for_write("src/x.rs", content, known_tfidf);
        assert!(!layer.is_empty());
        assert_eq!(layer.name(), "missing_imports");

        let write_ctx = SignalContext::new("src/x.rs", "")
            .with_hook("pre_write")
            .with_tool_name("Write")
            .with_proposed(ProposedChange::Write { content });
        assert_eq!(layer.enrich(&write_ctx).len(), 1);

        let edit_ctx = SignalContext::new("src/x.rs", "")
            .with_hook("pre_edit")
            .with_tool_name("Edit")
            .with_proposed(ProposedChange::Edit {
                old_string: "",
                new_string: content,
            });
        assert!(
            layer.enrich(&edit_ctx).is_empty(),
            "an Edit fragment has its imports on disk — pre_edit's detector owns that case"
        );
        assert!(layer.enrich(&SignalContext::new("src/x.rs", "")).is_empty());
    }
}
