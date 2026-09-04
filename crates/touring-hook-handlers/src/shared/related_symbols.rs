//! A3 (2026-09-02) — `RelatedSymbolsLayer`: the REACTIVE form of
//! `related_symbols` for a file about to be written.
//!
//! v1 is deterministic and precision-first: for every item the PROPOSED
//! content DECLARES (`struct`/`fn`/`class`/`def`/…), the symbol index is asked
//! whether that exact name is already defined elsewhere. A hit is the
//! homonym VP-Scout chain 4 warns about — the author is about to create a
//! second `Config`, a second `parse_args` — surfaced BEFORE the write, with
//! the file:line to reuse instead. The ANN "similar sites" route the strategy
//! named for TIER-3 stacks on top of this once an embedding index of code
//! sites exists; nothing here forecloses it.
//!
//! Like the S3 layer, the index lookup runs at construction (`for_write`)
//! because pipeline layers are `'static` and cannot borrow the runtime.

use crate::shared::signal_pipeline::{ProposedChange, SignalContext, SignalLayer};
use touring_code::ast::Lang;

/// Score of a homonym signal: advisory, above the antipattern noise floor.
const RELATED_SYMBOL_SCORE: f32 = 0.7;

/// Names an item declaration introduces in `content` for `lang`.
///
/// Rust: `struct`/`enum`/`trait`/`type`/`union`/`fn`; Python: `class`/`def`;
/// TypeScript/JavaScript: `class`/`function`/`interface`/`type`/`enum`.
/// Other languages: empty (no declaration grammar known here).
///
/// Token-based on purpose (a draft may not parse yet); the only false-positive
/// shape — the keyword inside a string literal — can only cause one extra
/// index lookup, never a fabricated location.
pub fn declared_item_names(lang: Lang, content: &str) -> Vec<String> {
    let keywords: &[&str] = match lang {
        Lang::Rust => &["struct", "enum", "trait", "type", "union", "fn"],
        Lang::Python => &["class", "def"],
        Lang::TypeScript | Lang::JavaScript => &["class", "function", "interface", "type", "enum"],
        _ => return Vec::new(),
    };
    let tokens = identifier_tokens(content);
    let mut out: Vec<String> = Vec::new();
    for pair in tokens.windows(2) {
        let (keyword, name) = (pair[0], pair[1]);
        if keywords.contains(&keyword)
            && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
            && !out.iter().any(|o| o == name)
        {
            out.push(name.to_string());
        }
    }
    out
}

/// Homonym signals for a proposed file.
///
/// `lookup` receives one declared name and returns the `(file, line)` of every
/// DEFINITION the index knows for it; definitions inside `rel_path` itself are
/// discarded (rewriting a file re-declares its own items). Short and generic
/// names (`main`, `new`, `run`, …) never reach the lookup.
pub fn homonym_signals<F>(rel_path: &str, content: &str, mut lookup: F) -> Vec<(f32, String)>
where
    F: FnMut(&str) -> Vec<(String, usize)>,
{
    let Some(lang) = Lang::from_path(std::path::Path::new(rel_path)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in declared_item_names(lang, content)
        .into_iter()
        .filter(|n| is_specific_name(n))
        .take(MAX_NAMES)
    {
        let elsewhere: Vec<(String, usize)> = lookup(&name)
            .into_iter()
            .filter(|(file, _)| !same_file(file, rel_path))
            .collect();
        let Some((first_file, first_line)) = elsewhere.first() else {
            continue;
        };
        let more = if elsewhere.len() > 1 {
            format!(" (+{} more)", elsewhere.len() - 1)
        } else {
            String::new()
        };
        out.push((
            RELATED_SYMBOL_SCORE,
            format!(
                "[related] `{name}` already defined in {first_file}:{first_line}{more} — homonym \
                 (VP-Scout chain 4): reuse it or pick a distinct name"
            ),
        ));
    }
    out
}

/// Declared names looked up per file — bounds the index cost of a large draft.
const MAX_NAMES: usize = 8;

/// Names too generic to be a homonym worth reporting: every crate has a
/// `new`, a `run`, a `Config`. The index would answer with dozens of files
/// and the signal would be noise.
const GENERIC_NAMES: &[&str] = &[
    "main", "new", "run", "test", "tests", "setup", "init", "default", "build", "from", "into",
    "load", "save", "parse", "render", "handle", "apply", "execute", "call", "open", "close",
    "start", "stop", "reset", "clear", "update", "insert", "remove", "get", "set", "next",
    "iter", "name", "kind", "value", "data", "index", "config", "state", "error", "result",
    "context", "options", "client", "server", "handler", "service", "manager", "helper",
    "utils", "util", "args", "cli", "app", "item", "entry", "node", "edge",
];

/// PascalCase names need ≥ 4 chars, snake/camel names ≥ 6; generic and
/// `test_*` names never qualify.
fn is_specific_name(name: &str) -> bool {
    let pascal = name.chars().next().is_some_and(char::is_uppercase);
    let min_len = if pascal { 4 } else { 6 };
    name.len() >= min_len
        && !name.starts_with("test_")
        && !GENERIC_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

/// Relative and absolute spellings of one file compare equal by suffix.
pub(crate) fn same_file(a: &str, b: &str) -> bool {
    a == b || a.ends_with(&format!("/{b}")) || b.ends_with(&format!("/{a}"))
}

/// Every `[A-Za-z0-9_]+` run in `content`, in order.
pub(crate) fn identifier_tokens(content: &str) -> Vec<&str> {
    content
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

/// The pipeline layer. Built per `pre_write` call with the homonyms resolved.
pub struct RelatedSymbolsLayer {
    signals: Vec<(f32, String)>,
}

impl RelatedSymbolsLayer {
    /// Build the layer for a proposed WRITE of `rel_path` with `content`.
    pub fn for_write<F>(rel_path: &str, content: &str, lookup: F) -> Self
    where
        F: FnMut(&str) -> Vec<(String, usize)>,
    {
        Self {
            signals: homonym_signals(rel_path, content, lookup),
        }
    }

    /// `true` when the layer will emit nothing.
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

impl SignalLayer for RelatedSymbolsLayer {
    fn name(&self) -> &'static str {
        "related_symbols"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        if !matches!(ctx.proposed, Some(ProposedChange::Write { .. })) {
            return Vec::new();
        }
        self.signals.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn defs_elsewhere(name: &str) -> Vec<(String, usize)> {
        match name {
            "TfIdfVectorizer" => vec![
                ("crates/touring-code/src/tfidf.rs".to_string(), 12),
                ("crates/legacy/src/vec.rs".to_string(), 40),
            ],
            "parse_manifest" => vec![("crates/touring-cli/src/manifest.rs".to_string(), 88)],
            _ => Vec::new(),
        }
    }

    #[test]
    fn declared_item_names_cover_rust_python_and_typescript_declarations() {
        let rust = "pub struct Foo;\nenum Bar { A }\npub(crate) fn parse_manifest() {}\ntrait Baz {}\ntype Qux = u8;\nlet x = Other::new();\n";
        let names = declared_item_names(Lang::Rust, rust);
        for n in ["Foo", "Bar", "parse_manifest", "Baz", "Qux"] {
            assert!(names.iter().any(|d| d == n), "{n} missing in {names:?}");
        }
        assert!(!names.iter().any(|d| d == "Other"), "{names:?}");

        let py = "class Loader:\n    def load(self):\n        pass\n\ndef parse_manifest(path):\n    return path\n";
        let names = declared_item_names(Lang::Python, py);
        assert_eq!(names, vec!["Loader", "load", "parse_manifest"], "{names:?}");

        let ts = "export class Store {}\nfunction parseManifest() {}\ninterface Options {}\ntype Id = string;\nenum Mode { A }\n";
        let names = declared_item_names(Lang::TypeScript, ts);
        assert_eq!(names, vec!["Store", "parseManifest", "Options", "Id", "Mode"], "{names:?}");

        assert!(declared_item_names(Lang::Markdown, "# struct Foo").is_empty());
    }

    #[test]
    fn flags_a_declared_name_already_defined_in_another_file_with_its_location() {
        let content = "pub struct TfIdfVectorizer;\n\npub fn parse_manifest(s: &str) -> u8 { s.len() as u8 }\n";
        let signals = homonym_signals("crates/touring-cli/src/new.rs", content, defs_elsewhere);
        assert_eq!(signals.len(), 2, "{signals:?}");
        let (score, text) = &signals[0];
        assert!((*score - RELATED_SYMBOL_SCORE).abs() < f32::EPSILON);
        assert!(text.starts_with("[related] `TfIdfVectorizer`"), "{text}");
        assert!(text.contains("crates/touring-code/src/tfidf.rs:12"), "{text}");
        assert!(text.contains("(+1 more)"), "{text}");
        assert!(signals[1].1.contains("`parse_manifest`") && signals[1].1.contains("manifest.rs:88"));
    }

    #[test]
    fn ignores_the_file_itself_and_never_looks_up_short_or_generic_names() {
        let seen: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let content = "pub struct TfIdfVectorizer;\nfn main() {}\nfn new() {}\nfn run() {}\nstruct Ab;\n";
        let signals = homonym_signals("crates/touring-code/src/tfidf.rs", content, |name| {
            seen.borrow_mut().push(name.to_string());
            // the index knows the name — in THIS file (a rewrite re-declares it)
            vec![("crates/touring-code/src/tfidf.rs".to_string(), 12)]
        });
        assert!(signals.is_empty(), "own definitions are not homonyms: {signals:?}");
        assert_eq!(*seen.borrow(), vec!["TfIdfVectorizer".to_string()], "{:?}", seen.borrow());
    }

    #[test]
    fn layer_emits_only_for_a_proposed_write() {
        let content = "pub struct TfIdfVectorizer;\n";
        let layer = RelatedSymbolsLayer::for_write("src/new.rs", content, defs_elsewhere);
        assert!(!layer.is_empty());
        assert_eq!(layer.name(), "related_symbols");
        let write_ctx = SignalContext::new("src/new.rs", "")
            .with_hook("pre_write")
            .with_tool_name("Write")
            .with_proposed(ProposedChange::Write { content });
        assert_eq!(layer.enrich(&write_ctx).len(), 1);
        let edit_ctx = SignalContext::new("src/new.rs", "")
            .with_hook("pre_edit")
            .with_tool_name("Edit")
            .with_proposed(ProposedChange::Edit {
                old_string: "",
                new_string: content,
            });
        assert!(layer.enrich(&edit_ctx).is_empty());
    }
}
