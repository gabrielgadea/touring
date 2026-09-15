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
    extract_captures_where(source, lang, query_src, |_| true)
}

/// Runs `query_src` and keeps the text of every capture `keep` accepts.
fn extract_captures_where(
    source: &str,
    lang: Lang,
    query_src: &str,
    keep: impl Fn(tree_sitter::Node<'_>) -> bool,
) -> Option<HashSet<String>> {
    use tree_sitter::{Parser, Query, QueryCursor};

    let mut parser = Parser::new();
    parser.set_language(&lang.tree_sitter_language()).ok()?;
    let tree = crate::ast::parser::parse_bounded(&mut parser, source, None)?;

    let ts_lang = lang.tree_sitter_language();
    let query = Query::new(&ts_lang, query_src).ok()?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut names: HashSet<String> = HashSet::new();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            if !keep(capture.node) {
                continue;
            }
            if let Ok(text) = capture.node.utf8_text(source.as_bytes())
                && !text.is_empty()
            {
                names.insert(text.to_string());
            }
        }
    }
    Some(names)
}

/// True when `node` is the name a type declaration introduces
/// (`pub struct TfIdfVectorizer;`). Cross-audit 14/09/2026: the query's bare
/// `(type_identifier)` also matched that name, so every public type consumed
/// itself and no declared-but-unused type could ever be reported as an orphan.
fn is_declared_type_name(node: tree_sitter::Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        matches!(
            parent.kind(),
            "struct_item" | "enum_item" | "union_item" | "trait_item" | "type_item"
        ) && parent
            .child_by_field_name("name")
            .is_some_and(|name| name.id() == node.id())
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    /// The text scan walks BYTES but slices `&str`: every cut must land on a char
    /// boundary. Measured 13/09/2026: a Romanian `ș` right before `::` aborted
    /// the konverter daemon mid-rebuild ("start byte index 17 is not a char
    /// boundary; it is inside 'ș'"), and a text ending in `::` sliced past the end.
    #[test]
    fn the_text_scan_never_cuts_inside_a_character_or_past_the_end() {
        assert_eq!(
            scan_qualified_calls_text("fie că așa::ceva(1)"),
            Vec::<(String, String)>::new(),
            "`așa` is not an ASCII identifier; the scan must skip it, not panic"
        );
        assert_eq!(
            scan_qualified_calls_text("ș mod_a::run_b(x)"),
            vec![("mod_a".to_string(), "run_b".to_string())]
        );
        assert!(scan_qualified_calls_text("mod_a::ș(").is_empty());
        assert!(scan_qualified_calls_text("trailing mod_a::").is_empty());
        assert!(scan_qualified_calls_text("::").is_empty());
        assert_eq!(
            scan_qualified_calls_text("ção::x() então mod_c::go_d()"),
            vec![("mod_c".to_string(), "go_d".to_string())]
        );
    }

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

    /// W (2026-09-02): a `use`-imported pub fn called BARE — `apply_landlock(&p)`
    /// — is neither a `.method()` nor a `Mod::f()`, so neither pattern saw it and
    /// every such producer read as a false orphan after a full rebuild (65
    /// measured against the wave baseline, 36 of them with a cross-crate caller).
    #[test]
    fn extracts_free_function_calls() {
        let src = r#"
use super::enforce_linux::apply_landlock;
fn caller(p: &Profile) -> u8 {
    apply_landlock(p).level + parse::<u8>("1")
}
"#;
        let names = extract_method_calls(src, Lang::Rust);
        assert!(
            names.contains(&"apply_landlock".to_string()),
            "free call missing in {names:?}"
        );
        assert!(
            names.contains(&"parse".to_string()),
            "generic free call missing in {names:?}"
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

/// Type-position and const/variant reference extraction — the complement of
/// [`extract_method_calls`] for the wiring resolver's other blind spot
/// (follow-up G3, cross-audit 2026-08-12): a symbol used AS a type or const
/// (`&ParsedTag`, `tags::TAG_TABLES_DDL`, `Option<Facet>`) generates no call
/// expression, so the F9 method-dispatch pass never wires its consumers and
/// every type/const producer read as a false orphan.
/// Tree-sitter query for a call through a qualified path, keeping the qualifier.
///
/// Two shapes reach the same pair: `backup::run()` has an `identifier` path, and
/// `super::backup::run()` nests one `scoped_identifier` deeper. Both yield
/// `("backup", "run")`. The plain [`extract_method_calls`] query discards the
/// qualifier, which is why 130 producers named `run` were indistinguishable.
const QUALIFIED_CALL_QUERY: &str = r"
    (call_expression
      function: (scoped_identifier
        path: (identifier) @qualifier
        name: (identifier) @name))
    (call_expression
      function: (scoped_identifier
        path: (scoped_identifier name: (identifier) @qualifier)
        name: (identifier) @name))
";

/// Every qualified call in `source`, as `(qualifier, name)` pairs.
///
/// The pair is what makes by-name resolution decidable: `("backup", "run")`
/// resolves to the producer whose module file is `…/backup.rs` or
/// `…/backup/mod.rs`, while the bare name `run` matches 130 producers.
///
/// Returns an empty vector when the language has no tree-sitter grammar loaded
/// or the source does not parse — a blind spot is never an error here.
#[must_use]
pub fn extract_qualified_calls(source: &str, lang: Lang) -> Vec<(String, String)> {
    extract_qualified_calls_inner(source, lang).unwrap_or_default()
}

/// Captura o corpo de cada invocação de macro, onde a árvore não entra.
const MACRO_TOKEN_TREE_QUERY: &str = r"(macro_invocation (token_tree) @tt)";

/// Pares `(qualificador, nome)` colhidos textualmente dentro dos `token_tree`.
///
/// Só o formato mínimo de uma chamada qualificada é aceito — `ident::ident(` com
/// ambos em minúsculas, o que descarta caminhos de tipo (`Foo::Bar`) — e os
/// segmentos de caminho (`super`, `self`, `crate`) são recusados como
/// qualificadores, pelo mesmo motivo do caminho analisado.
fn collect_qualified_in_macros(
    source: &str,
    ts_lang: &tree_sitter::Language,
    tree: &tree_sitter::Tree,
    out: &mut Vec<(String, String)>,
) {
    use tree_sitter::{Query, QueryCursor};

    let Ok(query) = Query::new(ts_lang, MACRO_TOKEN_TREE_QUERY) else {
        return;
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let Ok(text) = capture.node.utf8_text(source.as_bytes()) else {
                continue;
            };
            for (qualifier, name) in scan_qualified_calls_text(text) {
                let pair = (qualifier, name);
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
    }
}

/// A varredura textual de `ident::ident(`, sem regex.
///
/// Separada para ser testável sozinha: é a única parte de [`extract_qualified_calls`]
/// que não passa pela árvore, e por isso a que mais precisa de prova.
fn scan_qualified_calls_text(text: &str) -> Vec<(String, String)> {
    fn is_lower_ident(s: &str) -> bool {
        !s.is_empty()
            && s.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(pos) = text[i..].find("::") {
        let sep = i + pos;
        // o identificador à esquerda de `::`
        // Past the last non-identifier char by ITS width: `+ 1` lands inside a
        // multi-byte char (`ș` before `::` aborted a daemon, 13/09/2026).
        let start = text[..sep]
            .char_indices()
            .rev()
            .find(|&(_, c)| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(0, |(p, c)| p + c.len_utf8());
        let qualifier = &text[start..sep];
        // o identificador à direita, e o parêntese que faz dele uma chamada
        let rest = &text[sep + 2..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        let after = rest[end..].trim_start();
        // Always move forward, by whole chars: an empty `name` steps over the next
        // char (not one byte), and a text ending in `::` stops at its end.
        let step = if end > 0 {
            end
        } else {
            rest.chars().next().map_or(0, char::len_utf8)
        };
        i = sep + 2 + step;
        if !after.starts_with('(') {
            continue;
        }
        if !is_lower_ident(qualifier) || !is_lower_ident(name) {
            continue;
        }
        // A letter right before the qualifier means it is the tail of a longer,
        // non-ASCII word (`așa::` yields `a`), not an identifier of its own.
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        if matches!(qualifier, "super" | "self" | "crate") {
            continue;
        }
        let _ = bytes;
        out.push((qualifier.to_string(), name.to_string()));
    }
    out
}

fn extract_qualified_calls_inner(source: &str, lang: Lang) -> Option<Vec<(String, String)>> {
    use tree_sitter::{Parser, Query, QueryCursor};

    let mut parser = Parser::new();
    parser.set_language(&lang.tree_sitter_language()).ok()?;
    let tree = parser.parse(source, None)?;
    let ts_lang = lang.tree_sitter_language();
    let query = Query::new(&ts_lang, QUALIFIED_CALL_QUERY).ok()?;

    // Capture indices, resolved once: a match carries both captures, and the
    // PAIRING is the whole point — collecting into a flat set would lose it.
    let names = query.capture_names();
    let qual_idx = names.iter().position(|n| *n == "qualifier")?;
    let name_idx = names.iter().position(|n| *n == "name")?;

    let mut out: Vec<(String, String)> = Vec::new();
    // O interior de uma invocação de macro é um `token_tree`, não código: a
    // tabela de comandos deste workspace tem 132 chamadas qualificadas dentro de
    // `vec![…]` e NENHUMA aparece como `call_expression`. Ali a varredura é
    // textual — e continua segura, porque o par só vira aresta se existir um
    // `<qualificador>.rs` que declare o nome como público.
    collect_qualified_in_macros(source, &ts_lang, &tree, &mut out);

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        let mut qualifier = None;
        let mut name = None;
        for capture in m.captures {
            let text = capture.node.utf8_text(source.as_bytes()).ok()?;
            if capture.index as usize == qual_idx {
                qualifier = Some(text.to_string());
            } else if capture.index as usize == name_idx {
                name = Some(text.to_string());
            }
        }
        if let (Some(q), Some(n)) = (qualifier, name)
            && !q.is_empty()
            && !n.is_empty()
            && q != "super"
            && q != "self"
            && q != "crate"
        {
            let pair = (q, n);
            if !out.contains(&pair) {
                out.push(pair);
            }
        }
    }
    Some(out)
}

///
/// Returns bare identifier names (deduplicated), same contract as
/// [`extract_method_calls`]: callers match them by name against producer
/// rows. `scoped_identifier` captures also fire on call paths
/// (`tags::derive_tags(...)`) — harmless: the consumer edge they would add
/// already exists from the call pass, and consumer recording is idempotent.
/// The name a declaration introduces is never a reference to itself.
pub fn extract_type_and_const_refs(source: &str, lang: Lang) -> Vec<String> {
    const TYPE_REF_QUERY: &str = r"
        (type_identifier) @ref
        (scoped_type_identifier name: (type_identifier) @ref)
        (scoped_identifier name: (identifier) @ref)
    ";
    if lang != Lang::Rust {
        return Vec::new();
    }
    let Some(mut names) = extract_captures_where(source, lang, TYPE_REF_QUERY, |node| {
        !is_declared_type_name(node) && !is_std_family_path_segment(node, source)
    }) else {
        return Vec::new();
    };
    // A name the file imports from another crate is decided by that import: the
    // resolver wires it when the crate is in the workspace, and a guess by name
    // can only land on a homonym (`criterion::Criterion` on a workspace
    // `Criterion`, cross-audit 14/09/2026, B12).
    for imported in names_imported_from_other_crates(source) {
        names.remove(&imported);
    }
    names.into_iter().collect()
}

/// Path roots that are never a workspace crate.
const STD_FAMILY_ROOTS: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];

/// True when `node` names the last segment of an inline path rooted in the
/// standard library (`std::path::Path` in a signature).
fn is_std_family_path_segment(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if !matches!(
        parent.kind(),
        "scoped_type_identifier" | "scoped_identifier"
    ) {
        return false;
    }
    parent
        .child_by_field_name("path")
        .and_then(|path| path_root(path, source))
        .is_some_and(|root| STD_FAMILY_ROOTS.contains(&root.as_str()))
}

/// The leftmost segment of a Rust path node (`a` in `a::b::c`).
fn path_root(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "scoped_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("path")
            .and_then(|path| path_root(path, source)),
        _ => node.utf8_text(source.as_bytes()).ok().map(str::to_string),
    }
}

/// Names brought in by `use` from a path whose root is another crate: neither
/// `crate`, `super` nor `self`, nor a module this file declares with `mod`. The
/// alias is what the code writes, so `use x::A as B` yields `B`.
fn names_imported_from_other_crates(source: &str) -> HashSet<String> {
    let mut parser = tree_sitter::Parser::new();
    let mut out = HashSet::new();
    if parser
        .set_language(&Lang::Rust.tree_sitter_language())
        .is_err()
    {
        return out;
    }
    let Some(tree) = parser.parse(source, None) else {
        return out;
    };
    let root = tree.root_node();
    let mut local_modules = HashSet::new();
    let mut uses = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "mod_item" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                {
                    local_modules.insert(name.to_string());
                }
            }
            "use_declaration" => uses.push(node),
            _ => {}
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    let is_other_crate =
        |root: &str| !matches!(root, "crate" | "super" | "self") && !local_modules.contains(root);
    for decl in uses {
        if let Some(argument) = decl.child_by_field_name("argument") {
            collect_use_names(argument, source, None, &is_other_crate, &mut out);
        }
    }
    out
}

/// Walks one `use` tree. `list_root` is the root of the enclosing
/// `path::{...}` list, which every item inside it inherits.
fn collect_use_names(
    node: tree_sitter::Node<'_>,
    source: &str,
    list_root: Option<&str>,
    is_other_crate: &dyn Fn(&str) -> bool,
    out: &mut HashSet<String>,
) {
    let text = |n: tree_sitter::Node<'_>| n.utf8_text(source.as_bytes()).ok().map(str::to_string);
    match node.kind() {
        "scoped_identifier" => {
            let root = list_root
                .map(str::to_string)
                .or_else(|| path_root(node, source));
            if root.as_deref().is_some_and(is_other_crate)
                && let Some(name) = node.child_by_field_name("name").and_then(text)
            {
                out.insert(name);
            }
        }
        // A bare item inside a list (`{A, B}`); a bare `use foo;` names a crate.
        "identifier" => {
            if list_root.is_some_and(is_other_crate)
                && let Some(name) = text(node)
            {
                out.insert(name);
            }
        }
        "use_as_clause" => {
            let root = list_root.map(str::to_string).or_else(|| {
                node.child_by_field_name("path")
                    .and_then(|path| path_root(path, source))
            });
            if root.as_deref().is_some_and(is_other_crate)
                && let Some(alias) = node.child_by_field_name("alias").and_then(text)
            {
                out.insert(alias);
            }
        }
        "scoped_use_list" => {
            let root = list_root.map(str::to_string).or_else(|| {
                node.child_by_field_name("path")
                    .and_then(|path| path_root(path, source))
            });
            if let (Some(root), Some(list)) = (root, node.child_by_field_name("list")) {
                collect_use_names(list, source, Some(&root), is_other_crate, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_use_names(child, source, list_root, is_other_crate, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod type_ref_tests {
    use super::*;

    #[test]
    fn extracts_type_and_const_refs() {
        let src = "use super::tags::{Facet, ParsedTag};\nconst D: &str = tags::TAG_TABLES_DDL;\nfn f(x: &ParsedTag) -> Option<Facet> { let v = tags::Facet::Kind; None }\n";
        let names = extract_type_and_const_refs(src, Lang::Rust);
        for want in ["ParsedTag", "Facet", "TAG_TABLES_DDL"] {
            assert!(names.iter().any(|n| n == want), "{want} missing: {names:?}");
        }
    }

    /// Cross-audit 14/09/2026: a declaration is not a use. The declared name
    /// was captured as a reference, so a public type consumed itself.
    #[test]
    fn a_declared_type_name_is_not_a_reference_to_itself() {
        let src = "pub struct TfIdfVectorizer;\npub enum Mode { A }\npub trait Scorer {}\npub type Alias = u8;\npub union Bits { a: u8 }\n";
        let names = extract_type_and_const_refs(src, Lang::Rust);
        for declared in ["TfIdfVectorizer", "Mode", "Scorer", "Alias", "Bits"] {
            assert!(
                !names.iter().any(|n| n == declared),
                "{declared} is only declared: {names:?}"
            );
        }
        let used = "pub struct TfIdfVectorizer;\nfn f(x: &TfIdfVectorizer) {}\n";
        let names = extract_type_and_const_refs(used, Lang::Rust);
        assert!(
            names.iter().any(|n| n == "TfIdfVectorizer"),
            "a use in a signature still counts: {names:?}"
        );
    }

    /// Cross-audit 14/09/2026 (B12): a type imported from another crate, or
    /// written as a std path, never becomes a by-name guess; workspace-relative
    /// imports and local modules keep feeding the inference.
    #[test]
    fn types_from_other_crates_are_left_to_the_import_resolver() {
        let src = "use std::path::Path;\nuse criterion::{Criterion, black_box as bb};\nuse super::tags::Facet;\nmod local;\nuse local::Thing;\nuse serde::Serialize as Ser;\nfn f(p: &Path, c: Criterion, x: Facet, t: Thing, q: std::collections::HashMap<u8, u8>, r: crate::x::Own, s: Ser) {}\n";
        let names = extract_type_and_const_refs(src, Lang::Rust);
        for kept in ["Facet", "Thing", "Own"] {
            assert!(names.iter().any(|n| n == kept), "{kept} missing: {names:?}");
        }
        for dropped in ["Path", "Criterion", "HashMap", "Ser"] {
            assert!(
                !names.iter().any(|n| n == dropped),
                "{dropped} belongs to another crate: {names:?}"
            );
        }
    }

    #[test]
    fn non_rust_yields_empty() {
        assert!(extract_type_and_const_refs("x = 1", Lang::Python).is_empty());
    }

    /// D2 (2026-09-02) — the qualifier is what makes the name decidable.
    ///
    /// Measured: the command table dispatches through closures that CALL the
    /// handler (`super::backup::run()`), so the call IS seen — but only the last
    /// segment was kept, and 130 producers are named `run`. Keeping the pair
    /// turns an unresolvable name into exactly one producer.
    #[test]
    fn extracts_the_qualifier_of_a_scoped_call() {
        let src = r#"
            fn table() {
                let _ = super::backup::run();
                let _ = cascade::run();
                let _ = crate::cli::sandbox_runtimes::sandbox_runtimes();
            }
        "#;
        let pairs = extract_qualified_calls(src, Lang::Rust);
        assert!(
            pairs.contains(&("backup".to_string(), "run".to_string())),
            "{pairs:?}"
        );
        assert!(
            pairs.contains(&("cascade".to_string(), "run".to_string())),
            "{pairs:?}"
        );
        assert!(
            pairs.contains(&(
                "sandbox_runtimes".to_string(),
                "sandbox_runtimes".to_string()
            )),
            "{pairs:?}"
        );
    }

    /// Path keywords are not modules: `super::f()` and `self::f()` carry no
    /// qualifier worth resolving, and emitting them would wire every file named
    /// `super.rs` — of which there are none, so the edge would simply be lost
    /// work. `crate::` is dropped for the same reason.
    #[test]
    fn path_keywords_are_not_qualifiers() {
        let src = "fn f() { let _ = super::g(); let _ = self::h(); let _ = crate::i(); }";
        let pairs = extract_qualified_calls(src, Lang::Rust);
        assert!(pairs.is_empty(), "{pairs:?}");
    }

    /// A bare call has no qualifier and belongs to the free-function pass (W1),
    /// not this one — no pair is invented for it.
    #[test]
    fn a_bare_call_yields_no_pair() {
        let pairs = extract_qualified_calls("fn f() { g(); }", Lang::Rust);
        assert!(pairs.is_empty(), "{pairs:?}");
    }

    /// A forma REAL da tabela de comandos: a chamada vive dentro de uma closure,
    /// dentro de um literal de struct, dentro de um `vec![]`. O teste anterior
    /// usava a chamada solta no corpo de uma fn, e os dois casos não são o mesmo
    /// nó de árvore — só este prova o caso que motivou D2.
    #[test]
    fn captures_a_qualified_call_inside_a_dispatch_closure() {
        let src = concat!(
            "fn hook_commands() -> Vec<CommandDescriptor> {\n",
            "    vec![\n",
            "        CommandDescriptor {\n",
            "            name: \"scan-pii\",\n",
            "            handler: |_args| super::pii::run(),\n",
            "        },\n",
            "        CommandDescriptor {\n",
            "            name: \"backup\",\n",
            "            handler: |args| super::backup::run(args),\n",
            "        },\n",
            "    ]\n",
            "}\n",
        );
        let pairs = extract_qualified_calls(src, Lang::Rust);
        assert!(
            pairs.contains(&("pii".to_string(), "run".to_string())),
            "{pairs:?}"
        );
        assert!(
            pairs.contains(&("backup".to_string(), "run".to_string())),
            "{pairs:?}"
        );
    }
}
