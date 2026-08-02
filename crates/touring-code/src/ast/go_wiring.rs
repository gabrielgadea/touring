//! Package-aware wiring extraction for Go (P-H of the polyglot-parity plan).
//!
//! Go breaks the file-keyed wiring model every other language uses: an `import`
//! names a **package** (a directory of files), never a symbol, and usage is a
//! selector `pkg.Foo()` — so a producer's `.go` file path never JOINs a
//! consumer's import. P-G resolved this at the storage layer by admitting a
//! synthetic `"go:<import-path>"` key namespace (see
//! `touring_storage::knowledge_wiring`). This module is the **feeder** half:
//! it derives that key and extracts the producer/consumer edges keyed by it, so
//! Go participates in orphan detection without the false-orphan risk that made
//! it deferred (`docs/2026-07-03-polyglot-parity-plan.md` §11).
//!
//! Two asymmetric halves — both must be emitted together (producer-only would
//! make every export a false orphan):
//!
//! - **Producer** (`extract_go_exports`) — a package's exported (Capitalized)
//!   top-level `func`/`type`/`const`/`var`, keyed by the file's package
//!   import-path (`go_package_key_for_file`). The import-path is
//!   `<go.mod module>/<dir-of-file-relative-to-go.mod>`.
//! - **Consumer** (`extract_go_consumer_edges`) — for each `import` (with its
//!   local alias) and each selector `alias.Symbol` where `Symbol` is exported,
//!   the edge `(go:<import-path>, Symbol)`. The import statement carries the
//!   *literal* import-path, so — unlike the producer — the consumer needs no
//!   `go.mod` lookup; the two keys match by construction.
//!
//! Gated behind the `more-languages` feature (the Go grammar is), so a
//! `--no-default-features` build of `touring-code` compiles without it.

use tree_sitter::{Node, Parser};

use crate::ast::languages::Lang;

/// An exported top-level Go declaration (the producer side).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoExport {
    /// The exported identifier (always starts with an uppercase letter).
    pub name: String,
    /// `"function"` | `"type"` | `"const"` | `"var"`.
    pub kind: &'static str,
}

/// A consumer edge: this file uses `symbol` from the package keyed by
/// `package_key` (already `"go:<import-path>"` form, ready for `record_consumer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoConsumerEdge {
    /// `"go:<import-path>"` — matches the producer key by construction.
    pub package_key: String,
    /// The exported symbol referenced via `alias.Symbol`.
    pub symbol: String,
}

/// True if a Go identifier is *exported* — Go's visibility rule is purely
/// lexical: an identifier is exported iff its first letter is uppercase.
fn is_exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

/// Strip the surrounding quotes from a Go string literal node's text
/// (`"path/pkg"` → `path/pkg`; also tolerates raw backtick strings).
fn unquote(literal: &str) -> &str {
    literal.trim_matches('"').trim_matches('`')
}

/// Parse `source` with the Go grammar, returning the tree (or `None` on a
/// grammar/load failure — the caller then contributes no Go wiring, never an
/// error, mirroring the fail-open feeder).
fn parse_go(source: &str) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser.set_language(&Lang::Go.tree_sitter_language()).ok()?;
    parser.parse(source, None)
}

/// Collect the `name:`-field children of a spec node. A single `var_spec` /
/// `const_spec` can declare several names (`const A, B = ...`), each attached
/// under the `name` field, so `child_by_field_name` (first-only) is
/// insufficient — walk every child and test its field name.
fn name_field_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for (i, child) in node.children(&mut cursor).enumerate() {
        if node.field_name_for_child(i as u32) == Some("name") {
            names.push(child);
        }
    }
    names
}

/// Push `name_node`'s text as an export of `kind`, filtered to exported (i.e.
/// Capitalized) identifiers.
fn push_export(out: &mut Vec<GoExport>, name_node: Node, kind: &'static str, bytes: &[u8]) {
    if let Ok(text) = name_node.utf8_text(bytes) {
        if is_exported(text) {
            out.push(GoExport {
                name: text.to_string(),
                kind,
            });
        }
    }
}

/// Collect the exported names declared by every `spec_kind` spec under a
/// `type`/`const`/`var` declaration. Handles both grouped
/// (`const ( A = 1; B = 2 )`) and multi-name (`var A, B = 1, 2`) forms.
fn collect_specs(
    out: &mut Vec<GoExport>,
    decl: Node,
    spec_kind: &str,
    kind: &'static str,
    bytes: &[u8],
) {
    let mut cursor = decl.walk();
    for spec in decl.children(&mut cursor) {
        if spec.kind() == spec_kind {
            for name in name_field_children(spec) {
                push_export(out, name, kind, bytes);
            }
        }
    }
}

/// Extract the exported top-level declarations of a Go source file (producers).
///
/// Only **direct children** of `source_file` are package-level declarations; a
/// `var` inside a function body is not part of the package API, so the walk is
/// depth-1 (no full-tree recursion). Methods (`method_declaration`) are bound
/// to a receiver value, not reached via `pkg.Method`, so they are intentionally
/// excluded — the selector-based consumer side only ever references the
/// package-level names collected here.
pub fn extract_go_exports(source: &str) -> Vec<GoExport> {
    let Some(tree) = parse_go(source) else {
        return Vec::new();
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();
    let mut exports = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    push_export(&mut exports, name, "function", bytes);
                }
            }
            // `type ( A struct{}; B int )` and `type X struct{}` both nest
            // `type_spec` children under the declaration.
            "type_declaration" => collect_specs(&mut exports, child, "type_spec", "type", bytes),
            "const_declaration" => collect_specs(&mut exports, child, "const_spec", "const", bytes),
            "var_declaration" => collect_specs(&mut exports, child, "var_spec", "var", bytes),
            _ => {}
        }
    }
    exports
}

/// Build the `alias → import-path` map from a file's `import` declarations.
///
/// Default alias = the import-path's last segment (`"a/b/svc"` → `svc`). An
/// explicit alias (`import s "a/b/svc"`) overrides it. Dot-imports
/// (`import . "..."`, which merge names into file scope) and blank imports
/// (`import _ "..."`, side-effect only) carry no attributable selector and are
/// skipped.
fn import_alias_map(root: Node, bytes: &[u8]) -> Vec<(String, String)> {
    let mut aliases = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "import_spec" {
            let Some(path_node) = node.child_by_field_name("path") else {
                continue;
            };
            let Ok(path_lit) = path_node.utf8_text(bytes) else {
                continue;
            };
            let import_path = unquote(path_lit).to_string();
            if import_path.is_empty() {
                continue;
            }
            // Optional explicit alias in the `name` field.
            let alias = match node.child_by_field_name("name") {
                Some(name_node) => match name_node.utf8_text(bytes) {
                    // `.` (dot-import) and `_` (blank) are not real aliases.
                    Ok(".") | Ok("_") => continue,
                    Ok(a) if !a.is_empty() => a.to_string(),
                    _ => default_alias(&import_path),
                },
                None => default_alias(&import_path),
            };
            aliases.push((alias, import_path));
            continue;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    aliases
}

/// The default package alias for an import-path: its final path segment.
fn default_alias(import_path: &str) -> String {
    import_path
        .rsplit('/')
        .next()
        .unwrap_or(import_path)
        .to_string()
}

/// Extract the consumer edges of a Go source file: every `alias.Symbol`
/// selector whose `alias` matches an import and whose `Symbol` is exported,
/// mapped to `(go:<import-path>, Symbol)`. Edges are de-duplicated so a symbol
/// used many times contributes one consumer row.
pub fn extract_go_consumer_edges(source: &str) -> Vec<GoConsumerEdge> {
    let Some(tree) = parse_go(source) else {
        return Vec::new();
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let aliases = import_alias_map(root, bytes);
    if aliases.is_empty() {
        return Vec::new();
    }

    let mut edges: Vec<GoConsumerEdge> = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "selector_expression" {
            if let (Some(operand), Some(field)) = (
                node.child_by_field_name("operand"),
                node.child_by_field_name("field"),
            ) {
                // The operand must be a bare identifier equal to an import
                // alias — `a.b.C` (operand is itself a selector) is a field
                // access on a value, not a package reference.
                if operand.kind() == "identifier" {
                    if let (Ok(alias), Ok(symbol)) =
                        (operand.utf8_text(bytes), field.utf8_text(bytes))
                    {
                        if is_exported(symbol) {
                            if let Some((_, import_path)) = aliases.iter().find(|(a, _)| a == alias)
                            {
                                let edge = GoConsumerEdge {
                                    package_key: format!("go:{import_path}"),
                                    symbol: symbol.to_string(),
                                };
                                if !edges.contains(&edge) {
                                    edges.push(edge);
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    edges
}

/// Compose the producer key for a Go package.
///
/// `module_path` = the `go.mod` `module` line; `package_rel_dir` = the file's
/// directory relative to the `go.mod` directory (`""` for a file in the module
/// root). Result: `"go:<module>[/<rel-dir>]"`. Path separators are normalized
/// to `/` so a Windows-style rel-dir still keys identically to the consumer's
/// forward-slash import-path.
pub fn go_package_key(module_path: &str, package_rel_dir: &str) -> String {
    let rel = package_rel_dir.replace('\\', "/");
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        format!("go:{module_path}")
    } else {
        format!("go:{module_path}/{rel}")
    }
}

/// Derive the producer key for a Go file on disk by locating its enclosing
/// `go.mod` (walking up from the file's directory), reading its `module`
/// import-path, and joining the file's relative directory.
///
/// Returns `None` when no `go.mod` is found above the file (a loose `.go`
/// snippet outside any module) or the `go.mod` declares no `module` line — in
/// both cases the file has no derivable import-path and contributes no producer
/// rows (never a false key).
pub fn go_package_key_for_file(abs_file_path: &str) -> Option<String> {
    let path = std::path::Path::new(abs_file_path);
    let file_dir = path.parent()?;

    // Walk up to the nearest go.mod.
    let mut module_dir = file_dir;
    let go_mod = loop {
        let candidate = module_dir.join("go.mod");
        if candidate.is_file() {
            break candidate;
        }
        module_dir = module_dir.parent()?;
    };

    let content = std::fs::read_to_string(&go_mod).ok()?;
    let module_path = crate::ast::manifest::go_module_path(&content)?;

    let rel_dir = file_dir.strip_prefix(module_dir).ok()?;
    Some(go_package_key(&module_path, &rel_dir.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Producer extraction ────────────────────────────────────────────────

    #[test]
    fn extracts_exported_top_level_decls_only() {
        let src = r#"
package svc

import "fmt"

func Handler() {}          // exported func
func helper() {}           // unexported — skipped

type Config struct{}       // exported type
type internal struct{}     // unexported — skipped

const MaxRetries = 3       // exported const
const timeout = 30         // unexported — skipped

var Registry = 0           // exported var
var counter = 0            // unexported — skipped

func Outer() {
    var Local = 1          // NOT top-level — skipped
    _ = Local
    fmt.Println(Local)
}
"#;
        let exports = extract_go_exports(src);
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Handler"), "exported func; got {names:?}");
        assert!(names.contains(&"Config"), "exported type; got {names:?}");
        assert!(
            names.contains(&"MaxRetries"),
            "exported const; got {names:?}"
        );
        assert!(names.contains(&"Registry"), "exported var; got {names:?}");
        // Negatives.
        assert!(!names.contains(&"helper"), "unexported func excluded");
        assert!(!names.contains(&"internal"), "unexported type excluded");
        assert!(!names.contains(&"timeout"), "unexported const excluded");
        assert!(!names.contains(&"counter"), "unexported var excluded");
        assert!(!names.contains(&"Local"), "function-local var excluded");
    }

    #[test]
    fn export_kinds_are_tagged() {
        let src = "package p\nfunc F(){}\ntype T struct{}\nconst C = 1\nvar V = 2\n";
        let exports = extract_go_exports(src);
        let kind_of = |n: &str| exports.iter().find(|e| e.name == n).map(|e| e.kind);
        assert_eq!(kind_of("F"), Some("function"));
        assert_eq!(kind_of("T"), Some("type"));
        assert_eq!(kind_of("C"), Some("const"));
        assert_eq!(kind_of("V"), Some("var"));
    }

    #[test]
    fn grouped_and_multi_name_specs() {
        let src = r#"
package p
type (
    Alpha struct{}
    Beta  int
)
var A, B = 1, 2
"#;
        let names: Vec<String> = extract_go_exports(src)
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.contains(&"Alpha".to_string()));
        assert!(names.contains(&"Beta".to_string()));
        assert!(
            names.contains(&"A".to_string()),
            "multi-name var A; got {names:?}"
        );
        assert!(
            names.contains(&"B".to_string()),
            "multi-name var B; got {names:?}"
        );
    }

    // ── Consumer extraction ────────────────────────────────────────────────

    #[test]
    fn selector_edges_resolve_alias_to_import_path() {
        let src = r#"
package app

import (
    "mymod/pkg/svc"
    h "mymod/pkg/http"
    _ "mymod/pkg/sideeffect"
)

func run() {
    svc.Handler()          // default alias `svc` → mymod/pkg/svc
    h.Serve()              // explicit alias `h`  → mymod/pkg/http
    svc.lowercase()        // unexported symbol → skipped
    local.Method()         // `local` is not an import → skipped
}
"#;
        let edges = extract_go_consumer_edges(src);
        let has = |key: &str, sym: &str| {
            edges
                .iter()
                .any(|e| e.package_key == key && e.symbol == sym)
        };
        assert!(
            has("go:mymod/pkg/svc", "Handler"),
            "default-alias edge; got {edges:?}"
        );
        assert!(
            has("go:mymod/pkg/http", "Serve"),
            "explicit-alias edge; got {edges:?}"
        );
        assert!(
            !edges.iter().any(|e| e.symbol == "lowercase"),
            "unexported selector excluded"
        );
        assert!(
            !edges.iter().any(|e| e.symbol == "Method"),
            "non-import operand excluded"
        );
        // Blank import contributes no alias → no edge from it.
        assert!(
            !edges.iter().any(|e| e.package_key.contains("sideeffect")),
            "blank import contributes no consumer edge"
        );
    }

    #[test]
    fn duplicate_selectors_dedupe() {
        let src = r#"
package app
import "m/p"
func a() { p.Foo(); p.Foo(); p.Foo() }
"#;
        let edges = extract_go_consumer_edges(src);
        let foo_count = edges.iter().filter(|e| e.symbol == "Foo").count();
        assert_eq!(
            foo_count, 1,
            "repeated selector yields one edge; got {edges:?}"
        );
    }

    #[test]
    fn no_imports_no_edges() {
        let edges = extract_go_consumer_edges("package p\nfunc f(){ x.Y() }\n");
        assert!(edges.is_empty(), "no import → no attributable edge");
    }

    // ── Key derivation ─────────────────────────────────────────────────────

    #[test]
    fn package_key_joins_module_and_reldir() {
        assert_eq!(
            go_package_key("github.com/foo/bar", "pkg/svc"),
            "go:github.com/foo/bar/pkg/svc"
        );
        // File in module root → key is the bare module path.
        assert_eq!(
            go_package_key("github.com/foo/bar", ""),
            "go:github.com/foo/bar"
        );
        // Leading/trailing slashes and backslashes normalize.
        assert_eq!(go_package_key("m", "\\pkg\\svc\\"), "go:m/pkg/svc");
    }

    #[test]
    fn key_for_file_walks_up_to_go_mod() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("go.mod"), "module example.com/app\n\ngo 1.21\n")
            .expect("write go.mod");
        let svc_dir = root.join("internal/svc");
        std::fs::create_dir_all(&svc_dir).expect("mkdir");
        let file = svc_dir.join("handler.go");
        std::fs::write(&file, "package svc\nfunc H(){}\n").expect("write .go");

        let key = go_package_key_for_file(file.to_str().expect("utf8"));
        assert_eq!(
            key.as_deref(),
            Some("go:example.com/app/internal/svc"),
            "import-path = module + rel-dir"
        );

        // A file in the module root maps to the bare module key.
        let root_file = root.join("main.go");
        std::fs::write(&root_file, "package main\nfunc main(){}\n").expect("write main");
        assert_eq!(
            go_package_key_for_file(root_file.to_str().expect("utf8")).as_deref(),
            Some("go:example.com/app")
        );
    }

    #[test]
    fn key_for_file_none_without_go_mod() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let file = tmp.path().join("loose.go");
        std::fs::write(&file, "package p\n").expect("write");
        assert_eq!(go_package_key_for_file(file.to_str().expect("utf8")), None);
    }

    /// The crux of the package-aware model: a producer's key — derived from its
    /// on-disk `go.mod` location — must be the SAME string as the key a consumer
    /// derives from its literal `import` path. Only when they rendezvous does the
    /// file-keyed `wiring_map` JOIN resolve (`touring_storage::knowledge_wiring`),
    /// so this asserts the two independent derivations agree end-to-end.
    #[test]
    fn producer_key_and_consumer_key_rendezvous() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        std::fs::write(
            root.join("go.mod"),
            "module github.com/acme/app\n\ngo 1.21\n",
        )
        .expect("write go.mod");
        // Producer file lives in pkg/svc → import-path github.com/acme/app/pkg/svc.
        let svc_dir = root.join("pkg/svc");
        std::fs::create_dir_all(&svc_dir).expect("mkdir");
        let producer = svc_dir.join("handler.go");
        std::fs::write(
            &producer,
            "package svc\nfunc Handler() {}\ntype Config struct{}\n",
        )
        .expect("write producer");

        // Producer key derived from the file's go.mod location.
        let producer_key =
            go_package_key_for_file(producer.to_str().expect("utf8")).expect("producer key");

        // Consumer keys derived purely from its `import` + selectors — no go.mod.
        let consumer_src = r#"
package app
import "github.com/acme/app/pkg/svc"
func run() { svc.Handler() }
"#;
        let edges = extract_go_consumer_edges(consumer_src);
        let consumer_key = &edges.first().expect("one edge").package_key;

        assert_eq!(
            &producer_key, consumer_key,
            "producer key (from go.mod) must equal consumer key (from import) — else the JOIN never resolves"
        );
        assert_eq!(producer_key, "go:github.com/acme/app/pkg/svc");
        // And the exported Handler is what the consumer references.
        assert_eq!(edges[0].symbol, "Handler");
        // Config is exported (a producer) but unreferenced → the JOIN would leave
        // it an orphan (proven at the storage layer in polyglot_wiring_poc.rs).
        let exports = extract_go_exports("package svc\nfunc Handler() {}\ntype Config struct{}\n");
        assert!(exports.iter().any(|e| e.name == "Config"));
    }
}
