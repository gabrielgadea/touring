//! Module segments of a Rust path — the traversal that names a module.
//!
//! Only the LEAF of a path was ever credited (`Z` in `use crate::a::b::Z`), so a
//! module that serves as a namespace was an orphan by construction: 643 of them
//! in this workspace, 392 with a real traversal one line away (measured by the
//! analise session, 18/09/2026). Option B, Gabriel's decision of 19/09/2026:
//! every segment that names a module credits that module.
//!
//! Paths come from `use` declarations (each leaf of a group, `crate`/`super`/
//! `self` included) and from inline paths in code — `touring_bindings::desktop::
//! app::AppState::build_app(cc)` was the first evidence for 160 of 561 traversed
//! modules. Reading the tree, not the text, keeps comments and strings out: the
//! analise's first regex counted a doc comment saying "long-orphaned" as a use.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::languages::Lang;
use crate::ast::parser::parse_bounded;

/// Every module path a Rust file names, longest prefixes included and
/// deduplicated: `use crate::a::b::Z` yields `crate::a` and `crate::a::b`.
///
/// The first segment (the crate, `crate`, `super`, `self`) is never a module
/// symbol of its own, and the last segment of an inline path is the item, never
/// a module. A path that only a re-export names is left out unless the file also
/// uses the symbol it forwards — the rule of 18/09/2026.
#[must_use]
pub fn rust_module_paths(source: &str) -> Vec<String> {
    let Some(tree) = parse_rust(source) else {
        return Vec::new();
    };
    let used = super::imports::rust_names_used_outside_use(source);
    // A module this file declares is named by its BARE name (`mod x;` then
    // `x::f()`), where every other path starts at a crate or a keyword. An
    // inline `mod x { … }` is named the same way, so all three shapes count.
    let declared = rust_declared_module_names(source);
    let mut paths: BTreeSet<String> = BTreeSet::new();

    // `use` declarations: the extractor already composes the full module path of
    // every leaf, groups expanded.
    for import in super::imports::extract_imports(source, Lang::Rust) {
        for path in import_paths(&import) {
            paths.extend(prefixes(&path, false, &declared));
        }
    }
    for import in super::imports::rust_reexports(source) {
        let forwarded = import
            .symbols
            .iter()
            .all(|symbol| !used.contains(symbol.as_str()));
        if forwarded {
            for path in import_paths(&import) {
                for prefix in prefixes(&path, false, &declared) {
                    paths.remove(&prefix);
                }
            }
        }
    }

    // Inline paths in code, the outermost of each chain.
    let bytes = source.as_bytes();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "use_declaration" {
            continue;
        }
        let scoped = matches!(node.kind(), "scoped_identifier" | "scoped_type_identifier");
        let nested = node
            .parent()
            .is_some_and(|p| matches!(p.kind(), "scoped_identifier" | "scoped_type_identifier"));
        if scoped
            && !nested
            && let Ok(text) = node.utf8_text(bytes)
        {
            paths.extend(prefixes(text, true, &declared));
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }

    // Inside a macro the grammar keeps raw tokens, so none of the walk above
    // sees `super::activity::run(args)` in a `vec![…]` — 47 modules of
    // `touring-server`'s command table (19/09/2026).
    for path in super::method_calls::macro_paths(&tree, source) {
        paths.extend(prefixes(&path, true, &declared));
    }
    paths.into_iter().collect()
}

/// The module paths one import can name: its own, and each lowercase symbol
/// appended to it.
///
/// `use crate::terminal_job_cache;` reaches the extractor as the path `crate`
/// plus the symbol — the module IS the symbol there, and only the appended form
/// names it. A symbol that is a function instead resolves to no file, so the
/// filesystem, not a guess, decides.
fn import_paths(import: &super::imports::ImportInfo) -> Vec<String> {
    let mut out = vec![import.module_path.clone()];
    out.extend(
        import
            .symbols
            .iter()
            .filter(|symbol| symbol.chars().next().is_some_and(char::is_lowercase))
            .map(|symbol| format!("{}::{symbol}", import.module_path)),
    );
    out
}

/// The module prefixes of one path. `drop_last` leaves the item out of an inline
/// path (`crate::a::b::f()` names the modules `a` and `b`, never `f`); a `use`
/// path is already the module path of its leaf.
///
/// A segment that starts uppercase is a type, and everything after it belongs to
/// the type, so the walk stops there.
fn prefixes(path: &str, drop_last: bool, declared: &BTreeSet<String>) -> Vec<String> {
    let segments: Vec<&str> = path.split("::").filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // The first segment is a crate or a keyword — unless this file declares it,
    // and then the path names its own child module (`use a::Z;` beside `mod a;`).
    if declared.contains(segments[0]) {
        out.push(segments[0].to_string());
    }
    if segments.len() < 2 {
        return out;
    }
    let mut last = segments.len();
    if drop_last {
        last -= 1;
    }
    for i in 1..last {
        let segment = segments[i];
        if segment
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase() || c == '_' && segment.to_uppercase() == segment)
        {
            break;
        }
        out.push(segments[..=i].join("::"));
    }
    out
}

/// How a Rust file declares one of its child modules — which is how the child's
/// file is found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleDeclaration {
    /// `mod x;` — the file is the one Cargo's layout names.
    Elsewhere,
    /// `#[path = "…"] mod x;` — the attribute names the file, and the layout
    /// names nothing. Relative to the declaring module's directory.
    AtPath(String),
    /// `mod x { … }` — the module's items live in the declaring file itself.
    Inline,
}

/// Every child module a Rust file declares, and how each one's file is found.
///
/// [`rust_declared_child_modules`] answers only "does this file declare it",
/// which is all the bare-name rule needs. Resolving `crate::x` forward from the
/// crate root needs the other two shapes as well: `#[path]` (12 modules of this
/// workspace, all of touring-cli's `cli_handlers_*`) and an inline body.
#[must_use]
pub fn rust_module_declarations(source: &str) -> BTreeMap<String, ModuleDeclaration> {
    let mut out = BTreeMap::new();
    let Some(tree) = parse_rust(source) else {
        return out;
    };
    let bytes = source.as_bytes();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "mod_item"
            && let Some(name) = node.child_by_field_name("name")
            && let Ok(text) = name.utf8_text(bytes)
        {
            let declaration = if node.child_by_field_name("body").is_some() {
                ModuleDeclaration::Inline
            } else if let Some(attribute) = path_attribute(node, bytes) {
                ModuleDeclaration::AtPath(attribute)
            } else {
                ModuleDeclaration::Elsewhere
            };
            out.insert(text.to_string(), declaration);
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    out
}

/// The file a `#[path = "…"]` on this item names. The grammar files an outer
/// attribute either inside the item or as the sibling right before it, so both
/// places are read.
fn path_attribute(node: tree_sitter::Node<'_>, bytes: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let inside = node.children(&mut cursor);
    // The climb STOPS at the first non-attribute: a run of `#[path] mod x;`
    // declarations would otherwise hand each module the attribute of the one
    // above it.
    let before = std::iter::successors(node.prev_sibling(), tree_sitter::Node::prev_sibling)
        .take_while(|sibling| sibling.kind() == "attribute_item");
    inside
        .filter(|child| child.kind() == "attribute_item")
        .chain(before)
        .find_map(|attribute| {
            let text = attribute.utf8_text(bytes).ok()?;
            let inner = text
                .trim()
                .strip_prefix("#[")
                .and_then(|rest| rest.strip_suffix(']'))?;
            let (key, value) = inner.split_once('=')?;
            (key.trim() == "path").then(|| value.trim().trim_matches('"').to_string())
        })
}

/// Every child module a Rust file declares, in ALL three shapes — the set that
/// answers "is this first segment a module of THIS file".
///
/// [`rust_declared_child_modules`] answers a narrower question (which children
/// live in another file) and leaves an inline `mod x { … }` out on purpose, since
/// an inline module has no file to find. For NAMING, though, it is as nameable as
/// any other: `pub mod compute { … }` beside `compute::f()` is the parent using
/// its own child — the `internal_only` class of 19/09/2026 — and four modules
/// stayed orphans because the narrower set was the one asked.
#[must_use]
pub fn rust_declared_module_names(source: &str) -> BTreeSet<String> {
    rust_module_declarations(source).into_keys().collect()
}

/// The child modules a Rust file declares (`mod x;`, `pub mod x;`), with no
/// body: those live in another file, and this file is their declarer.
#[must_use]
pub fn rust_declared_child_modules(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(tree) = parse_rust(source) else {
        return out;
    };
    let bytes = source.as_bytes();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "mod_item"
            && node.child_by_field_name("body").is_none()
            && let Some(name) = node.child_by_field_name("name")
            && let Ok(text) = name.utf8_text(bytes)
        {
            out.insert(text.to_string());
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    out
}

fn parse_rust(source: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&Lang::Rust.tree_sitter_language())
        .ok()?;
    parse_bounded(&mut parser, source, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_use_path_names_every_module_it_crosses() {
        let paths = rust_module_paths("use crate::a::b::Z;\nfn f(z: Z) -> Z { z }\n");
        assert_eq!(paths, ["crate::a", "crate::a::b"]);
    }

    #[test]
    fn a_group_and_a_relative_path_name_their_modules() {
        let paths = rust_module_paths(
            "use crate::a::{b::Z, c::W};\nuse super::sib::T;\n\nfn f(z: Z, w: W, t: T) {}\n",
        );
        assert_eq!(
            paths,
            ["crate::a", "crate::a::b", "crate::a::c", "super::sib"]
        );
    }

    /// 160 of 561 traversed modules had an inline path as their first evidence.
    #[test]
    fn an_inline_path_names_its_modules_and_stops_at_the_type() {
        let paths = rust_module_paths(
            "fn build() {\n    touring_bindings::desktop::app::AppState::build_app(cc);\n    crate::a::run();\n}\n",
        );
        assert_eq!(
            paths,
            [
                "crate::a",
                "touring_bindings::desktop",
                "touring_bindings::desktop::app"
            ]
        );
    }

    #[test]
    fn a_comment_or_string_names_nothing() {
        let paths = rust_module_paths(
            "// crate::ghost::run();\n/// long-orphaned: crate::ghost2::X\nfn f() { let s = \"crate::ghost3::Y\"; }\n",
        );
        assert!(paths.is_empty(), "{paths:?}");
    }

    /// The rule of 18/09: a path only a re-export names credits nothing; one the
    /// file also uses does.
    #[test]
    fn a_pure_reexport_path_names_no_module() {
        let forwarded = rust_module_paths("pub use crate::a::b::Z;\n");
        assert!(forwarded.is_empty(), "{forwarded:?}");

        let used = rust_module_paths("pub use crate::a::b::Z;\n\npub const ALL: [Z; 1] = [Z];\n");
        assert_eq!(used, ["crate::a", "crate::a::b"]);
    }

    /// The parent that uses its own child in the same file (5 cases measured).
    /// Gabriel, 19/09/2026: this edge is `internal_only`, not a consumer.
    #[test]
    fn a_child_module_this_file_declares_is_named_by_its_bare_name() {
        let paths = rust_module_paths("mod keys;\n\nfn f() {\n    keys::build();\n}\n");
        assert_eq!(paths, ["keys"]);
        // The same shape through a `use`: `mod a;` beside `use a::Z;`.
        assert_eq!(
            rust_module_paths("mod a;\nuse a::Z;\n\nfn f(z: Z) {}\n"),
            ["a"]
        );
        // Without the declaration the first segment is a crate, never a module.
        assert!(rust_module_paths("fn f() { keys::build(); }\n").is_empty());
    }

    /// The shape of `touring-server`'s command table: 47 modules named inside a
    /// `vec![…]`, where the grammar keeps only raw tokens.
    #[test]
    fn a_path_inside_a_macro_names_its_modules() {
        let paths = rust_module_paths(
            "fn table() -> Vec<D> {\n    vec![\n        D {\n            name: \"activity\",\n            policy: ErrorPolicy::ExitOnError,\n            handler: |args| super::activity::run(args),\n        },\n        D {\n            name: \"ast\",\n            policy: ErrorPolicy::ExitOnError,\n            handler: |args| touring_hooks::prompt_enhance::compose(args),\n        },\n    ]\n}\n",
        );
        assert_eq!(
            paths,
            ["super::activity", "touring_hooks::prompt_enhance"],
            "a type path (`ErrorPolicy::ExitOnError`) names no module"
        );
    }

    /// `format!("{}", a::b::f())` — the macro the 30.4.57 fix was measured on.
    #[test]
    fn a_macro_path_stops_at_the_item_and_at_a_type() {
        assert_eq!(
            rust_module_paths("fn f() { let s = format!(\"{}\", crate::a::b::run()); }\n"),
            ["crate::a", "crate::a::b"]
        );
        assert!(
            rust_module_paths("fn f() { assert_eq!(Foo::Bar, x); }\n").is_empty(),
            "a two-segment type path names no module"
        );
    }

    /// `use crate::terminal_job_cache;` — the module IS the imported symbol.
    #[test]
    fn a_bare_module_import_names_the_module() {
        assert_eq!(
            rust_module_paths(
                "use crate::terminal_job_cache;\n\nfn f() { terminal_job_cache::get(); }\n"
            ),
            ["crate::terminal_job_cache"]
        );
        // A re-export of the same shape still credits nothing.
        assert!(
            rust_module_paths("pub use crate::terminal_job_cache;\n").is_empty(),
            "the rule of 18/09 holds for the appended form too"
        );
    }

    #[test]
    fn a_module_declaration_says_where_its_file_is() {
        let declarations = rust_module_declarations(
            "#[path = \"cli/handlers/dispatch.rs\"]\npub mod cli_handlers;\nmod plain;\npub mod inline { pub fn f() {} }\n",
        );
        assert_eq!(
            declarations.get("cli_handlers"),
            Some(&ModuleDeclaration::AtPath(
                "cli/handlers/dispatch.rs".into()
            ))
        );
        assert_eq!(
            declarations.get("plain"),
            Some(&ModuleDeclaration::Elsewhere)
        );
        assert_eq!(declarations.get("inline"), Some(&ModuleDeclaration::Inline));
    }

    /// touring-cli's `lib.rs` declares 12 `#[path]` modules in a row: the climb
    /// for the attribute has to stop at the first non-attribute, or each module
    /// inherits the file of the one above it.
    #[test]
    fn a_run_of_path_declarations_keeps_each_attribute_with_its_own_module() {
        let declarations = rust_module_declarations(
            "#[path = \"cli/handlers/dispatch.rs\"]\npub mod cli_handlers;\n#[path = \"cli/handlers/decompose.rs\"]\npub mod cli_handlers_decompose;\npub mod plain;\n",
        );
        assert_eq!(
            declarations.get("cli_handlers"),
            Some(&ModuleDeclaration::AtPath(
                "cli/handlers/dispatch.rs".into()
            ))
        );
        assert_eq!(
            declarations.get("cli_handlers_decompose"),
            Some(&ModuleDeclaration::AtPath(
                "cli/handlers/decompose.rs".into()
            ))
        );
        assert_eq!(
            declarations.get("plain"),
            Some(&ModuleDeclaration::Elsewhere),
            "the module after the run carries no attribute of its own"
        );
    }

    /// The parent using its own INLINE child (`pub mod compute { … }` +
    /// `compute::f()`): 4 modules stayed orphans because the narrower
    /// declared-children set, which leaves inline out by design, was the one
    /// asked. Gabriel, 19/09/2026: this edge is `internal_only`.
    #[test]
    fn an_inline_child_this_file_declares_is_named_by_its_bare_name() {
        assert_eq!(
            rust_module_paths(
                "pub mod compute {\n    pub fn f() {}\n}\n\nfn g() {\n    compute::f();\n}\n"
            ),
            ["compute"]
        );
        // The declaration is what makes it nameable — without it the first
        // segment is a crate, exactly as before.
        assert!(rust_module_paths("fn g() { compute::f(); }\n").is_empty());
    }

    #[test]
    fn declared_module_names_cover_the_three_shapes() {
        let source =
            "#[path = \"a/b.rs\"]\nmod at_path;\nmod elsewhere;\nmod inline { pub fn f() {} }\n";
        assert_eq!(
            rust_declared_module_names(source)
                .into_iter()
                .collect::<Vec<_>>(),
            ["at_path", "elsewhere", "inline"]
        );
        assert_eq!(
            rust_declared_child_modules(source)
                .into_iter()
                .collect::<Vec<_>>(),
            ["at_path", "elsewhere"],
            "the narrower set still answers only 'which child lives in another file'"
        );
    }

    #[test]
    fn declared_child_modules_are_the_ones_living_in_another_file() {
        let mods = rust_declared_child_modules(
            "mod a;\npub mod b;\nmod inline { pub fn f() {} }\npub(crate) mod c;\n",
        );
        assert_eq!(
            mods.into_iter().collect::<Vec<_>>(),
            ["a", "b", "c"],
            "an inline `mod` block declares nothing in another file"
        );
    }
}
