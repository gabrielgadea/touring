//! Qualified Python use: a module reached through a name, then `name.Symbol`.
//!
//! The import-driven consumer pass only reads `from x import Nome`: a bare
//! `import x` carries no symbol, so it wrote no consumer row, and a project that
//! reaches its symbols through the module object recorded nothing at all. In the
//! analise, `google_maps_coleta.MatrizDeTransito` read as an orphan while
//! `programacao.py` used it twice through `gm.` (16/09/2026).
//!
//! B6 (16/09/2026) adds the other half, which is the larger one: `from pacote
//! import modulo [as alias]`. Measured on the analise with two independent
//! instruments — an AST sweep here and a per-occurrence census by the session
//! working in that repo — the from-import form accounts for the WHOLE remaining
//! blind spot: 53 of 53 symbols inside the judge's scope (77 of 77 occurrences
//! in that census) and 126 symbols across the project on disk. `import a.b`
//! accounts for zero in both.
//!
//! What this returns is `(module_path, symbol)` pairs — the caller resolves the
//! module to a file and decides what to record.

use std::collections::{BTreeSet, HashMap};

use crate::ast::languages::Lang;
use crate::ast::parser::parse_bounded;

/// A local name bound by an import, with the byte offset where that binding
/// appears.
///
/// The offset is not decoration. Python rebinds a name to the LAST import that
/// names it, and two forms can now compete for the same name (`import u` and
/// `from b import u`, or two `from`s of the same module name). This walk visits
/// nodes in stack order, which is not source order, so without a position the
/// winner would depend on traversal — the same input could yield either edge
/// between runs. Keying by position makes it deterministic AND correct.
type Bindings = HashMap<String, (usize, String)>;

/// Record `local → module`, keeping the binding that appears LAST in the source.
fn bind(aliases: &mut Bindings, local: &str, module: String, at: usize) {
    if aliases.get(local).is_none_or(|(prev, _)| at >= *prev) {
        aliases.insert(local.to_string(), (at, module));
    }
}

/// `(module_path, symbol)` for every `name.Symbol` whose `name` this file bound
/// with an `import` or a `from ... import`.
///
/// `import a.b` (no alias) binds `a`, so the use reads `a.b.Symbol` — a nested
/// attribute this pass deliberately leaves alone: the module path would be a
/// guess, and a wrong producer key is worse than a missing edge. A relative
/// `from . import x` is left alone for the same reason.
#[must_use]
pub fn python_qualified_uses(source: &str) -> Vec<(String, String)> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&Lang::Python.tree_sitter_language())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parse_bounded(&mut parser, source, None) else {
        return Vec::new();
    };
    let bytes = source.as_bytes();

    let mut aliases: Bindings = HashMap::new();
    let mut attributes: Vec<(String, String)> = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "import_statement" => collect_aliases(node, bytes, &mut aliases),
            "import_from_statement" => collect_from_aliases(node, bytes, &mut aliases),
            "attribute" => {
                if let Some(pair) = attribute_pair(node, bytes) {
                    attributes.push(pair);
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    attributes
        .into_iter()
        .filter_map(|(object, symbol)| {
            aliases
                .get(&object)
                .map(|(_, module)| (module.clone(), symbol.clone()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The names a `from x import Nome [as n]` brings in that the module never
/// references outside its import statements, as the ORIGINAL name (`Nome`),
/// the one the import extractor reports.
///
/// Such a name is a re-export (listed in `__all__`, forwarded by an
/// `__init__.py` or any façade) or a dead import; neither is a use of `Nome`
/// (decision of 18/09/2026, Gabriel: importing is not using). The analise
/// found `supersede` kept off the orphan list by a façade that only forwarded
/// it. A reference is an identifier in code: an attribute name (`obj.Nome`), a
/// keyword-argument name (`f(Nome=1)`) and a string (`__all__ = ["Nome"]`) are not.
#[must_use]
pub fn python_unreferenced_imports(source: &str) -> BTreeSet<String> {
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
    let bytes = source.as_bytes();
    let mut imported: Vec<(String, String)> = Vec::new();
    let mut referenced: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "import_from_statement" => {
                imported.extend(from_import_bindings(node, bytes));
                continue;
            }
            "import_statement" => continue,
            "identifier" if !names_a_member(node) => {
                if let Ok(text) = node.utf8_text(bytes) {
                    referenced.insert(text);
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    imported
        .into_iter()
        .filter(|(_, local)| !referenced.contains(local.as_str()))
        .map(|(original, _)| original)
        .collect()
}

/// `(original, local)` for each name a `from … import` binds: `a` → `(a, a)`,
/// `a as b` → `(a, b)`. A dotted name binds nothing usable here.
fn from_import_bindings(node: tree_sitter::Node, bytes: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children_by_field_name("name", &mut cursor) {
        let text = |n: Option<tree_sitter::Node>| n.and_then(|n| n.utf8_text(bytes).ok());
        let (original, local) = match child.kind() {
            "dotted_name" => (text(Some(child)), text(Some(child))),
            "aliased_import" => (
                text(child.child_by_field_name("name")),
                text(child.child_by_field_name("alias")),
            ),
            _ => (None, None),
        };
        if let (Some(original), Some(local)) = (original, local)
            && !original.contains('.')
        {
            out.push((original.to_string(), local.to_string()));
        }
    }
    out
}

/// `true` when an identifier names a member rather than a binding: the
/// attribute of `obj.Nome` or the name of a keyword argument `f(Nome=…)`.
fn names_a_member(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let field = match parent.kind() {
        "attribute" => "attribute",
        "keyword_argument" => "name",
        _ => return false,
    };
    parent
        .child_by_field_name(field)
        .is_some_and(|n| n.id() == node.id())
}

#[cfg(test)]
mod unreferenced_import_tests {
    use super::python_unreferenced_imports;

    /// 18/09/2026 (analise): a façade that only forwards `supersede` through
    /// `__all__` kept it off the orphan list.
    #[test]
    fn a_name_only_forwarded_or_never_touched_is_not_referenced() {
        let src = "from pkg.gravacao import supersede, grava\n\
                   from pkg.modelo import No as N, Aresta\n\
                   from pkg.util import d\n\
                   import json\n\
                   __all__ = [\"supersede\", \"grava\"]\n\n\
                   def f(obj):\n    return grava(N(), obj.d, g(d=1), json.dumps(obj.Aresta))\n";
        let unref: Vec<String> = python_unreferenced_imports(src).into_iter().collect();
        assert_eq!(
            unref,
            ["Aresta", "d", "supersede"],
            "an alias is judged by its local name; attribute and keyword names are not references"
        );
    }

    #[test]
    fn every_used_import_is_referenced() {
        let src = "from a import X\n\n\n@X.decorate\ndef f(y: X) -> X:\n    return y\n";
        assert!(python_unreferenced_imports(src).is_empty());
    }
}

/// The names an `import` statement binds: `import x` → `x`, `import x as gm` →
/// `gm`, both pointing at the module path the import names.
fn collect_aliases(node: tree_sitter::Node, bytes: &[u8], aliases: &mut Bindings) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "dotted_name" => {
                if let Ok(module) = child.utf8_text(bytes)
                    && !module.contains('.')
                {
                    bind(aliases, module, module.to_string(), child.start_byte());
                }
            }
            "aliased_import" => {
                let module = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok());
                let alias = child
                    .child_by_field_name("alias")
                    .and_then(|n| n.utf8_text(bytes).ok());
                if let (Some(module), Some(alias)) = (module, alias) {
                    bind(aliases, alias, module.to_string(), child.start_byte());
                }
            }
            _ => {}
        }
    }
}

/// The names a `from <package> import ...` statement binds: `from p import m` →
/// `m`, `from p import m as a` → `a`, both pointing at `p.m`.
///
/// The bound name is the alias when there is one and the imported name when
/// there is not, and that `or` is the whole point. A large part of this family
/// arrives with NO alias, and the UNIVERSE of the measurement matters as much as
/// the number: over the repository ON DISK, which is what the indexer walks, a
/// branch reading only `as <alias>` would miss 43 of 126 symbols (34.1%)
/// project-wide and 8 of 53 (15.1%) inside the judge's scope (16/09/2026). An
/// earlier figure of 13 (24.5%) was measured against a census whose universe was
/// a pre-computed consumer list, not the disk — true for the question it
/// answered, wrong for this one. Binding only the alias would have looked like a
/// fix and delivered a fraction of one.
///
/// Two shapes are refused, both because the module path would be a guess and a
/// wrong producer key is worse than a missing edge: a relative import (`from .
/// import x`, whose `module_name` is not a `dotted_name`) and a dotted imported
/// name. A name that turns out to be a SYMBOL rather than a module (`from pacote
/// import Classe`) needs no guard here — `pacote.Classe` resolves to no file and
/// the caller drops it, which is the same filter that already protects the
/// `import` branch.
fn collect_from_aliases(node: tree_sitter::Node, bytes: &[u8], aliases: &mut Bindings) {
    let Some(package) = node.child_by_field_name("module_name") else {
        return;
    };
    if package.kind() != "dotted_name" {
        return;
    }
    let Ok(package) = package.utf8_text(bytes) else {
        return;
    };
    let mut cursor = node.walk();
    for child in node.children_by_field_name("name", &mut cursor) {
        match child.kind() {
            "dotted_name" => {
                if let Ok(name) = child.utf8_text(bytes)
                    && !name.contains('.')
                {
                    bind(
                        aliases,
                        name,
                        format!("{package}.{name}"),
                        child.start_byte(),
                    );
                }
            }
            "aliased_import" => {
                let name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok());
                let alias = child
                    .child_by_field_name("alias")
                    .and_then(|n| n.utf8_text(bytes).ok());
                if let (Some(name), Some(alias)) = (name, alias)
                    && !name.contains('.')
                {
                    bind(
                        aliases,
                        alias,
                        format!("{package}.{name}"),
                        child.start_byte(),
                    );
                }
            }
            _ => {}
        }
    }
}

/// `gm.Nome` → `("gm", "Nome")`; anything whose object is not a bare name (a
/// call result, a nested attribute) yields nothing.
fn attribute_pair(node: tree_sitter::Node, bytes: &[u8]) -> Option<(String, String)> {
    let object = node.child_by_field_name("object")?;
    if object.kind() != "identifier" {
        return None;
    }
    let symbol = node.child_by_field_name("attribute")?;
    Some((
        object.utf8_text(bytes).ok()?.to_string(),
        symbol.utf8_text(bytes).ok()?.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_aliased_module_attribute_is_a_qualified_use() {
        let src = "import google_maps_coleta as gm\nimport json\n\n\ndef ler():\n    matriz = gm.MatrizDeTransito.ler()\n    return json.dumps(gm.lugares_validos())\n";
        let uses = python_qualified_uses(src);
        assert!(
            uses.contains(&(
                "google_maps_coleta".to_string(),
                "MatrizDeTransito".to_string()
            )),
            "{uses:?}"
        );
        assert!(
            uses.contains(&(
                "google_maps_coleta".to_string(),
                "lugares_validos".to_string()
            )),
            "{uses:?}"
        );
        // A module imported without an alias binds its own name.
        assert!(
            uses.contains(&("json".to_string(), "dumps".to_string())),
            "{uses:?}"
        );
    }

    #[test]
    fn an_attribute_of_something_that_was_never_imported_is_not_a_use() {
        let src =
            "import gm_like as gm\n\n\ndef f(obj):\n    return obj.MatrizDeTransito + gm.Real\n";
        let uses = python_qualified_uses(src);
        assert_eq!(
            uses,
            vec![("gm_like".to_string(), "Real".to_string())],
            "{uses:?}"
        );
    }

    #[test]
    fn a_dotted_import_without_alias_is_left_alone() {
        // `import a.b` binds `a`; the use reads `a.b.Symbol`, whose module path
        // this pass refuses to guess.
        let src = "import pacote.modulo\n\n\ndef f():\n    return pacote.modulo.Coisa()\n";
        assert!(python_qualified_uses(src).is_empty());
    }

    // ── B6: `from pacote import modulo [as alias]` ────────────────────────
    // The whole remaining blind spot, measured. The two tests that matter most
    // here are the negative controls: without them a green positive proves only
    // that SOMETHING produced the edge, not that this branch did.

    #[test]
    fn from_package_import_module_binds_the_module_with_no_alias() {
        let src = "from pacote import io_utils\n\n\ndef f():\n    return io_utils.PROCESSADO\n";
        assert_eq!(
            python_qualified_uses(src),
            vec![("pacote.io_utils".to_string(), "PROCESSADO".to_string())]
        );
    }

    #[test]
    fn from_package_import_module_as_alias_binds_the_alias() {
        let src = "from pacote import io_utils as io\n\n\ndef f():\n    return io.PROCESSADO\n";
        assert_eq!(
            python_qualified_uses(src),
            vec![("pacote.io_utils".to_string(), "PROCESSADO".to_string())]
        );
    }

    #[test]
    fn the_original_name_is_not_bound_when_the_import_renames_it() {
        // NEGATIVE CONTROL for the alias branch: Python leaves `io_utils`
        // unbound here, so an edge would mean we keyed on the imported name
        // instead of the alias. A positive test alone cannot tell the two apart.
        let src =
            "from pacote import io_utils as io\n\n\ndef f():\n    return io_utils.PROCESSADO\n";
        assert!(python_qualified_uses(src).is_empty());
    }

    #[test]
    fn a_module_imported_and_never_touched_is_not_a_use() {
        // NEGATIVE CONTROL for the whole pass: the import alone must not write
        // an edge, or every import in the project would become a consumer row.
        let src = "from pacote import io_utils\nimport json\n";
        assert!(python_qualified_uses(src).is_empty());
    }

    #[test]
    fn a_relative_from_import_is_left_alone() {
        // `from . import x` — the absolute path the resolver needs would be a
        // guess, exactly as with `import a.b`.
        let src = "from . import irmao\n\n\ndef f():\n    return irmao.Coisa\n";
        assert!(python_qualified_uses(src).is_empty());
    }

    #[test]
    fn a_dotted_imported_name_is_left_alone() {
        let src = "from pacote import sub.modulo\n\n\ndef f():\n    return sub.modulo.Coisa\n";
        assert!(python_qualified_uses(src).is_empty());
    }

    #[test]
    fn the_last_import_of_a_name_is_the_one_that_binds_it() {
        // Python rebinds; so must we. This also pins traversal order out of the
        // result — the walk is a stack, so without the byte offset either edge
        // could win between runs.
        let src = "from a import u\nfrom b import u\n\n\ndef f():\n    return u.X\n";
        assert_eq!(
            python_qualified_uses(src),
            vec![("b.u".to_string(), "X".to_string())]
        );
    }

    #[test]
    fn a_plain_import_and_a_from_import_competing_for_a_name_resolve_by_position() {
        let src = "import u\nfrom b import u\n\n\ndef f():\n    return u.X\n";
        assert_eq!(
            python_qualified_uses(src),
            vec![("b.u".to_string(), "X".to_string())]
        );
        let reversed = "from b import u\nimport u\n\n\ndef f():\n    return u.X\n";
        assert_eq!(
            python_qualified_uses(reversed),
            vec![("u".to_string(), "X".to_string())]
        );
    }

    #[test]
    fn several_modules_from_one_package_each_bind_their_own_path() {
        let src = "from pacote import um, dois as d\n\n\ndef f():\n    return um.A + d.B\n";
        let uses = python_qualified_uses(src);
        assert!(
            uses.contains(&("pacote.um".to_string(), "A".to_string())),
            "{uses:?}"
        );
        assert!(
            uses.contains(&("pacote.dois".to_string(), "B".to_string())),
            "{uses:?}"
        );
    }
}
