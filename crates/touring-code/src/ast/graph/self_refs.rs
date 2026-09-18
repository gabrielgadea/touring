//! Same-file references: which of a file's own public symbols it uses itself.
//!
//! B4 (15/09/2026): Python had no consumer edge of any kind for a use inside the
//! declaring file — the three inferred extractors are Rust-only — so a constant
//! read by its own module and a constant nobody reads were the same row in the
//! wiring graph: an orphan. This pass records the self-use, and
//! `internal_only_symbols` is what then tells the two apart.
//!
//! The edge it produces is `ast_inferred`: a match inside one file, never a
//! resolved import.
//!
//! D9 (18/09/2026): Rust too. A method used only through `self.m()` or
//! `Self::m` read as an orphan, so the only honest ways to quiet the judge were
//! to invent an external caller or to drop the method. And both the rebuild and
//! the edit path write these edges through [`self_referenced_names`]: until then
//! only the rebuild did, so editing a file demoted its internal symbols to
//! orphans until the next rebuild.

use std::collections::BTreeSet;

use super::method_calls::{MacroToken, MacroVia, macro_token};
use crate::ast::languages::Lang;
use crate::ast::parser::parse_bounded;
use crate::ast::symbols::Symbol;

/// The public symbols of `symbols` that `source` itself uses, in `language` —
/// the one entry point of the rebuild and of the edit path, so the two can never
/// disagree about what `internal_only` means. Languages without a rule answer
/// empty.
#[must_use]
pub fn self_referenced_names(source: &str, symbols: &[Symbol], language: &str) -> BTreeSet<String> {
    match language {
        "python" => python_self_referenced_names(source, symbols),
        "rust" => rust_self_referenced_names(source, symbols),
        _ => BTreeSet::new(),
    }
}

/// Python: public symbols of `symbols` that `source` references outside their
/// own definition span.
///
/// The name node of the definition itself lives inside that span, so a symbol
/// that is merely declared is never reported; a module-level constant read from
/// a function below it is.
fn python_self_referenced_names(source: &str, symbols: &[Symbol]) -> BTreeSet<String> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&Lang::Python.tree_sitter_language())
        .is_err()
    {
        return BTreeSet::new();
    }
    let Some(tree) = parse_bounded(&mut parser, source, None) else {
        return BTreeSet::new();
    };

    // Identifier nodes only: a name inside a comment or a string is not a use.
    let mut uses: Vec<(usize, &str)> = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "identifier"
            && let Ok(text) = node.utf8_text(source.as_bytes())
        {
            uses.push((node.start_byte(), text));
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    symbols
        .iter()
        .filter(|sym| sym.is_public)
        .filter(|sym| {
            uses.iter().any(|(offset, text)| {
                *text == sym.name && (*offset < sym.start_byte || *offset >= sym.end_byte)
            })
        })
        .map(|sym| sym.name.clone())
        .collect()
}

/// How a Rust identifier is used at one place in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RustUse<'a> {
    /// On its own: a type, a const, a free function, a path's leading segment.
    Bare,
    /// `self.name` — a method (or field) of the receiver.
    OnSelf,
    /// `Path::name`, with the path reduced to its last type name.
    Scoped(&'a str),
}

/// Rust: public symbols of `symbols` that `source` uses outside their own span.
///
/// Python's rule — any identifier with the name — is wrong for Rust, whose
/// common method names are everywhere: `self.indels.is_empty()` calls
/// `Vec::is_empty`, and a bare `new` or `len` names a dozen things in any file.
/// So the evidence depends on what the symbol is:
/// - a METHOD (it has a `parent_name`) is used as `self.m(…)`, `Self::m` or
///   `Owner::m`, `Owner` being its impl type;
/// - anything else — free fn, type, const, static, trait, alias — as a bare
///   identifier with its name: never the field of `x.name`, never the name
///   segment of `a::name` (another module's item), and never the type an `impl`
///   header implements (declaring `impl Foo` is not a use of `Foo`).
fn rust_self_referenced_names(source: &str, symbols: &[Symbol]) -> BTreeSet<String> {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&Lang::Rust.tree_sitter_language()).is_err() {
        return BTreeSet::new();
    }
    let Some(tree) = parse_bounded(&mut parser, source, None) else {
        return BTreeSet::new();
    };
    let bytes = source.as_bytes();
    let mut uses: Vec<(usize, &str, RustUse)> = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        // A use inside a test item is the test's, not the file's: counting it
        // would file API that only tests call as `internal_only` instead of as
        // the orphan it is.
        if is_test_item(node, bytes) {
            continue;
        }
        uses.extend(rust_use(node, bytes));
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }

    symbols
        .iter()
        .filter(|sym| sym.is_public && is_used(sym, &uses))
        .map(|sym| sym.name.clone())
        .collect()
}

/// The use of a name that `node` is, if any: where it sits, the name, and how.
fn rust_use<'a>(node: tree_sitter::Node<'_>, bytes: &'a [u8]) -> Option<(usize, &'a str, RustUse<'a>)> {
    let text = |n: tree_sitter::Node<'_>| n.utf8_text(bytes).unwrap_or("");
    match node.kind() {
        "field_expression" => {
            let field = node.child_by_field_name("field")?;
            is_self_call(node).then(|| (field.start_byte(), text(field), RustUse::OnSelf))
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let path = node.child_by_field_name("path")?;
            let name = node.child_by_field_name("name")?;
            Some((name.start_byte(), text(name), RustUse::Scoped(base_type_name(text(path)))))
        }
        // A macro's arguments are raw tokens: the shape comes from the
        // neighbouring tokens (`self.m(…)`, `Self::m`, alone), never from a
        // parent node that is not there.
        "identifier" if node.parent().is_some_and(|p| p.kind() == "token_tree") => {
            let how = macro_rust_use(macro_token(node, bytes)?)?;
            Some((node.start_byte(), text(node), how))
        }
        "identifier" | "type_identifier" if stands_alone(node) => {
            Some((node.start_byte(), text(node), RustUse::Bare))
        }
        _ => None,
    }
}

/// `self.m(…)` — the field expression is the FUNCTION of a call on `self`.
/// `self.start.elapsed()` reads the FIELD `start`, and a method named like a
/// field is not used by it.
fn is_self_call(field_expression: tree_sitter::Node<'_>) -> bool {
    field_expression
        .child_by_field_name("value")
        .is_some_and(|value| value.kind() == "self")
        && field_expression.parent().is_some_and(|p| {
            p.kind() == "call_expression" && p.child_by_field_name("function") == Some(field_expression)
        })
}

/// The use one macro token makes: `self.m(…)`, a path, or alone. Reading a field
/// (`x.f` with no call) is no use of anything.
fn macro_rust_use(token: MacroToken<'_>) -> Option<RustUse<'_>> {
    match token.via {
        MacroVia::Dot { on_self: true } if token.called => Some(RustUse::OnSelf),
        MacroVia::Dot { .. } => None,
        MacroVia::Path { qualifier, .. } => Some(RustUse::Scoped(base_type_name(qualifier))),
        MacroVia::Alone => Some(RustUse::Bare),
    }
}

/// Whether one of `uses` uses `sym` outside its own span, the way its kind is
/// used (see [`rust_self_referenced_names`]).
fn is_used(sym: &Symbol, uses: &[(usize, &str, RustUse)]) -> bool {
    let outside = |offset: usize| offset < sym.start_byte || offset >= sym.end_byte;
    let owner = sym.parent_name.as_deref().map(base_type_name);
    uses.iter().any(|&(offset, name, how)| {
        name == sym.name
            && outside(offset)
            && match (owner, how) {
                (Some(_), RustUse::OnSelf) => true,
                (Some(owner), RustUse::Scoped(path)) => path == "Self" || path == owner,
                (None, RustUse::Bare) => true,
                _ => false,
            }
    })
}

/// Whether `node` is an item under `#[cfg(test)]` or `#[test]` — its attributes
/// are the `attribute_item` siblings right before it.
fn is_test_item(node: tree_sitter::Node, source: &[u8]) -> bool {
    if !matches!(node.kind(), "mod_item" | "function_item" | "impl_item") {
        return false;
    }
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                let attr: String = s
                    .utf8_text(source)
                    .unwrap_or("")
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if attr == "#[test]" || attr.starts_with("#[cfg(test)") {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => return false,
        }
        sibling = s.prev_sibling();
    }
    false
}

/// `a::b::Foo<T>` → `Foo`: the type a path or an impl header names.
fn base_type_name(path: &str) -> &str {
    let head = path.split('<').next().unwrap_or(path).trim();
    head.rsplit("::").next().unwrap_or(head).trim()
}

/// Whether an identifier stands on its own — not the `name` of a scoped path
/// (read as [`RustUse::Scoped`]), and not the type or trait an `impl` header
/// implements (directly, or as the base of a generic type).
fn stands_alone(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return true;
    };
    if matches!(parent.kind(), "scoped_identifier" | "scoped_type_identifier") {
        return parent.child_by_field_name("name") != Some(node);
    }
    let mut current = node;
    let mut up = Some(parent);
    while let Some(p) = up {
        match p.kind() {
            "generic_type" if p.child_by_field_name("type") == Some(current) => {
                current = p;
                up = p.parent();
            }
            "impl_item" => {
                return p.child_by_field_name("type") != Some(current)
                    && p.child_by_field_name("trait") != Some(current);
            }
            _ => return true,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::symbols::extract_symbols;

    fn rust_symbols(source: &str) -> Vec<Symbol> {
        extract_symbols(source, Lang::Rust).expect("rust symbols")
    }

    #[test]
    fn a_rust_method_counts_only_through_self_or_its_own_type() {
        let src = "pub struct TextEdit { indels: Vec<u8> }\n\nimpl TextEdit {\n    pub fn is_empty(&self) -> bool {\n        self.indels.is_empty()\n    }\n    pub fn len(&self) -> usize {\n        self.indels.len()\n    }\n    pub fn apply(&self) -> bool {\n        self.is_empty()\n    }\n    pub fn parse(s: &str) -> Option<u8> {\n        s.parse().ok()\n    }\n    pub fn from_text(s: &str) -> Option<u8> {\n        Self::parse(s)\n    }\n    pub fn twice(s: &str) -> Option<u8> {\n        TextEdit::from_text(s)\n    }\n}\n";
        let names = rust_self_referenced_names(src, &rust_symbols(src));
        assert!(names.contains("is_empty"), "self.is_empty(): {names:?}");
        assert!(names.contains("parse"), "Self::parse: {names:?}");
        assert!(names.contains("from_text"), "TextEdit::from_text: {names:?}");
        assert!(!names.contains("len"), "self.indels.len() is Vec::len: {names:?}");
        assert!(!names.contains("apply"), "nobody calls apply: {names:?}");
        // The path `TextEdit::from_text` NAMES the type — a use, as in Python; only
        // the `impl TextEdit` header is not (pinned by the next test).
        assert!(names.contains("TextEdit"), "{names:?}");
    }

    #[test]
    fn a_rust_type_or_const_counts_by_name_but_its_impl_header_does_not() {
        let src = "pub struct Used;\npub struct OnlyImplemented<T>(T);\npub const LIMIT: usize = 3;\npub const UNREAD: usize = 4;\n\nimpl<T> OnlyImplemented<T> {\n    pub fn build() -> Used {\n        let _ = LIMIT;\n        Used\n    }\n}\n\nfn elsewhere() -> usize {\n    other::UNREAD\n}\n";
        let names = rust_self_referenced_names(src, &rust_symbols(src));
        assert!(names.contains("Used"), "{names:?}");
        assert!(names.contains("LIMIT"), "{names:?}");
        assert!(!names.contains("OnlyImplemented"), "an impl header is not a use: {names:?}");
        assert!(!names.contains("UNREAD"), "`other::UNREAD` is another module's item: {names:?}");
        assert!(!names.contains("build"), "{names:?}");
    }

    #[test]
    fn a_field_named_like_a_method_is_not_a_call_of_it() {
        let src = "pub struct Timer { start: u64 }\n\nimpl Timer {\n    pub fn start(&self) -> u64 {\n        self.start\n    }\n    pub fn elapsed(&self) -> u64 {\n        self.start + 1\n    }\n}\n";
        let names = rust_self_referenced_names(src, &rust_symbols(src));
        assert!(!names.contains("start"), "`self.start` is the field: {names:?}");
    }

    /// A macro's arguments are tokens, not expressions: the same rules, read from
    /// the tokens around the name.
    #[test]
    fn inside_a_macro_the_same_rules_hold() {
        let src = "pub const LIMIT: u8 = 3;\npub struct Report { total: u8 }\n\nimpl Report {\n    pub fn total(&self) -> u8 { 1 }\n    pub fn label(&self) -> String { String::new() }\n    pub fn build() -> u8 { 2 }\n    pub fn show(&self) -> String {\n        format!(\"{} {} {} {}\", self.label(), Self::build(), self.total, LIMIT)\n    }\n}\n";
        let names = rust_self_referenced_names(src, &rust_symbols(src));
        assert!(names.contains("label"), "self.label() in format!: {names:?}");
        assert!(names.contains("build"), "Self::build() in format!: {names:?}");
        assert!(names.contains("LIMIT"), "a const in format!: {names:?}");
        assert!(!names.contains("total"), "`self.total` is the field: {names:?}");
    }

    #[test]
    fn a_use_inside_a_test_item_is_not_the_files_use() {
        let src = "pub const LIMIT: u8 = 1;\npub fn helper() -> u8 { 2 }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn t() {\n        assert_eq!(LIMIT + helper(), 3);\n    }\n}\n";
        let names = rust_self_referenced_names(src, &rust_symbols(src));
        assert!(names.is_empty(), "only a test reads them — orphans, not internal: {names:?}");
    }

    #[test]
    fn the_dispatcher_routes_by_language_and_is_empty_elsewhere() {
        let py = "PUB = 1\n\n\ndef f():\n    return PUB\n";
        let py_syms = extract_symbols(py, Lang::Python).expect("python symbols");
        assert!(self_referenced_names(py, &py_syms, "python").contains("PUB"));
        assert!(self_referenced_names(py, &py_syms, "markdown").is_empty());
    }

    fn symbols(source: &str) -> Vec<Symbol> {
        extract_symbols(source, Lang::Python).expect("python symbols")
    }

    #[test]
    fn a_module_constant_read_by_its_own_function_is_self_referenced() {
        let src = "PUB = 1\nMORTO = 2\n_PRIV = 3\n\n\ndef helper():\n    return PUB + _PRIV\n\n\ndef unused_helper():\n    return 0\n";
        let names = python_self_referenced_names(src, &symbols(src));
        assert!(names.contains("PUB"), "{names:?}");
        assert!(!names.contains("MORTO"), "nothing reads MORTO: {names:?}");
        assert!(
            !names.contains("_PRIV"),
            "a private symbol is not a producer row: {names:?}"
        );
        assert!(
            !names.contains("unused_helper"),
            "declaring is not using: {names:?}"
        );
    }

    #[test]
    fn a_function_called_by_another_function_of_the_same_file_is_self_referenced() {
        let src = "def leaf():\n    return 1\n\n\ndef root():\n    return leaf()\n";
        let names = python_self_referenced_names(src, &symbols(src));
        assert!(names.contains("leaf"), "{names:?}");
        assert!(!names.contains("root"), "{names:?}");
    }

    #[test]
    fn a_name_that_only_appears_in_a_comment_or_a_string_is_not_a_use() {
        let src = "PUB = 1\n\n\ndef helper():\n    # PUB is the answer\n    return \"PUB\"\n";
        let names = python_self_referenced_names(src, &symbols(src));
        assert!(names.is_empty(), "{names:?}");
    }
}
