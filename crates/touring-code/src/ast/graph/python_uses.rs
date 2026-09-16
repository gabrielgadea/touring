//! Qualified Python use: `import modulo as gm` followed by `gm.Nome`.
//!
//! The import-driven consumer pass only reads `from x import Nome`: a bare
//! `import x` carries no symbol, so it wrote no consumer row, and a project that
//! reaches its symbols through the module object recorded nothing at all. In the
//! analise, `google_maps_coleta.MatrizDeTransito` read as an orphan while
//! `programacao.py` used it twice through `gm.` (16/09/2026).
//!
//! What this returns is `(module_path, symbol)` pairs — the caller resolves the
//! module to a file and decides what to record.

use std::collections::{BTreeSet, HashMap};

use crate::ast::languages::Lang;
use crate::ast::parser::parse_bounded;

/// `(module_path, symbol)` for every `alias.Symbol` whose `alias` this file
/// bound with an `import`.
///
/// `import a.b` (no alias) binds `a`, so the use reads `a.b.Symbol` — a nested
/// attribute this pass deliberately leaves alone: the module path would be a
/// guess, and a wrong producer key is worse than a missing edge.
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

    let mut aliases: HashMap<String, String> = HashMap::new();
    let mut attributes: Vec<(String, String)> = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "import_statement" => collect_aliases(node, bytes, &mut aliases),
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
                .map(|module| (module.clone(), symbol.clone()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The names an `import` statement binds: `import x` → `x`, `import x as gm` →
/// `gm`, both pointing at the module path the import names.
fn collect_aliases(node: tree_sitter::Node, bytes: &[u8], aliases: &mut HashMap<String, String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "dotted_name" => {
                if let Ok(module) = child.utf8_text(bytes)
                    && !module.contains('.')
                {
                    aliases.insert(module.to_string(), module.to_string());
                }
            }
            "aliased_import" => {
                let module = child.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok());
                let alias = child.child_by_field_name("alias").and_then(|n| n.utf8_text(bytes).ok());
                if let (Some(module), Some(alias)) = (module, alias) {
                    aliases.insert(alias.to_string(), module.to_string());
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
        assert!(uses.contains(&("google_maps_coleta".to_string(), "MatrizDeTransito".to_string())), "{uses:?}");
        assert!(uses.contains(&("google_maps_coleta".to_string(), "lugares_validos".to_string())), "{uses:?}");
        // A module imported without an alias binds its own name.
        assert!(uses.contains(&("json".to_string(), "dumps".to_string())), "{uses:?}");
    }

    #[test]
    fn an_attribute_of_something_that_was_never_imported_is_not_a_use() {
        let src = "import gm_like as gm\n\n\ndef f(obj):\n    return obj.MatrizDeTransito + gm.Real\n";
        let uses = python_qualified_uses(src);
        assert_eq!(uses, vec![("gm_like".to_string(), "Real".to_string())], "{uses:?}");
    }

    #[test]
    fn a_dotted_import_without_alias_is_left_alone() {
        // `import a.b` binds `a`; the use reads `a.b.Symbol`, whose module path
        // this pass refuses to guess.
        let src = "import pacote.modulo\n\n\ndef f():\n    return pacote.modulo.Coisa()\n";
        assert!(python_qualified_uses(src).is_empty());
    }
}
