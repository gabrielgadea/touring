//! Same-file references: which of a file's own public symbols it uses itself.
//!
//! B4 (15/09/2026): Python had no consumer edge of any kind for a use inside the
//! declaring file — the three inferred extractors are Rust-only — so a constant
//! read by its own module and a constant nobody reads were the same row in the
//! wiring graph: an orphan. This pass records the self-use, and
//! `internal_only_symbols` is what then tells the two apart.
//!
//! The edge it produces is `ast_inferred`: a bare-name match inside one file,
//! never a resolved import.

use std::collections::BTreeSet;

use crate::ast::languages::Lang;
use crate::ast::parser::parse_bounded;
use crate::ast::symbols::Symbol;

/// Public symbols of `symbols` that `source` references outside their own
/// definition span.
///
/// The name node of the definition itself lives inside that span, so a symbol
/// that is merely declared is never reported; a module-level constant read from
/// a function below it is.
#[must_use]
pub fn python_self_referenced_names(source: &str, symbols: &[Symbol]) -> BTreeSet<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::symbols::extract_symbols;

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
