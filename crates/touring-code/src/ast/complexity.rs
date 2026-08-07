//! Cyclomatic complexity computation via tree-sitter AST walk.
//!
//! Computes McCabe cyclomatic complexity for individual symbols by counting
//! branch nodes in their AST subtree. Formula: `CC = 1 + Σ(decision_points)`.
//!
//! Supports Python, Rust, TypeScript, and JavaScript.

use tree_sitter::Node;

use crate::ast::error::AstResult;
use crate::ast::languages::Lang;

/// Branch node kinds per language.
///
/// These are AST node types that represent decision points (control flow branches).
/// Each one adds +1 to cyclomatic complexity.
///
/// NOTE: `binary_expression` is NOT listed here — it is handled separately in
/// `count_branches()` via `is_logical_binary()`, which checks the operator child
/// to only count `&&`/`||` (Rust/TS/JS) or `and`/`or` (Python uses `boolean_operator`
/// which IS listed here since it's a distinct node type in tree-sitter-python).
fn is_branch_node(kind: &str, lang: Lang) -> bool {
    match lang {
        Lang::Python => matches!(
            kind,
            "if_statement"
                | "elif_clause"
                | "for_statement"
                | "while_statement"
                | "try_statement"
                | "except_clause"
                | "with_statement"
                | "assert_statement"
                | "boolean_operator"  // Python-specific: `and`, `or` are distinct node types
                | "conditional_expression"  // ternary: x if cond else y
                | "list_comprehension"
                | "set_comprehension"
                | "dictionary_comprehension"
                | "generator_expression"
        ),
        Lang::Rust => matches!(
            kind,
            "if_expression"
                | "else_clause"
                | "for_expression"
                | "while_expression"
                | "loop_expression"
                | "match_expression"
                | "match_arm"
                | "try_expression"  // ? operator
                | "closure_expression"
        ),
        Lang::TypeScript | Lang::JavaScript => matches!(
            kind,
            "if_statement"
                | "else_clause"
                | "for_statement"
                | "for_in_statement"
                | "while_statement"
                | "do_statement"
                | "switch_case"
                | "catch_clause"
                | "ternary_expression"
                | "optional_chain_expression" // ?.
        ),
        Lang::Bash => matches!(
            kind,
            "if_statement"
                | "elif_clause"
                | "for_statement"
                | "while_statement"
                | "case_statement"
                | "case_item"
        ),
        // Data/markup languages have no control flow
        _ => false,
    }
}

/// Check if a binary expression node uses a logical operator (&&, ||, and, or).
fn is_logical_binary(source: &str, node: Node, lang: Lang) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }
    // Get the operator child
    if let Some(op) = node.child_by_field_name("operator")
        && let Ok(text) = op.utf8_text(source.as_bytes())
    {
        return match lang {
            Lang::Python => matches!(text, "and" | "or"),
            Lang::Rust | Lang::TypeScript | Lang::JavaScript | Lang::Bash => {
                matches!(text, "&&" | "||")
            }
            // Data/markup languages have no binary expressions
            _ => false,
        };
    }
    false
}

/// Walk a node's subtree and count decision points.
fn count_branches(source: &str, node: Node, lang: Lang) -> u16 {
    let mut count: u16 = 0;

    let mut cursor = node.walk();
    let mut reached_end = false;

    while !reached_end {
        let current = cursor.node();
        let kind = current.kind();

        // Special handling for binary expressions (only logical operators count)
        if kind == "binary_expression" {
            if is_logical_binary(source, current, lang) {
                count = count.saturating_add(1);
            }
        } else if is_branch_node(kind, lang) {
            count = count.saturating_add(1);
        }

        // DFS traversal
        if cursor.goto_first_child() {
            continue;
        }
        if cursor.goto_next_sibling() {
            continue;
        }

        // Backtrack
        loop {
            if !cursor.goto_parent() {
                reached_end = true;
                break;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }

    count
}

/// Compute cyclomatic complexity for a tree-sitter node (typically a function/method body).
///
/// Returns `1 + branch_count` where branch_count is the number of decision points.
pub fn compute_for_node(source: &str, node: Node, lang: Lang) -> u16 {
    1u16.saturating_add(count_branches(source, node, lang))
}

/// Compute cyclomatic complexity for a symbol given its byte range in source.
///
/// Parses the source with tree-sitter, finds the deepest node covering the range,
/// and computes complexity for that subtree.
pub fn compute_complexity(
    source: &str,
    lang: Lang,
    start_byte: usize,
    end_byte: usize,
) -> AstResult<u16> {
    let pool = crate::ast::parser::ParserPool::new();
    let tree = pool.parse(source, lang)?;

    let root = tree.root_node();
    let node = find_deepest_node(root, start_byte, end_byte);

    Ok(compute_for_node(source, node, lang))
}

/// Compute complexity for all functions/methods in a source file.
///
/// Returns a vec of `(symbol_name, complexity)` pairs.
pub fn compute_complexity_for_source(source: &str, lang: Lang) -> AstResult<Vec<(String, u16)>> {
    let pool = crate::ast::parser::ParserPool::new();
    let tree = pool.parse(source, lang)?;
    let symbols = crate::ast::symbols::extract_symbols_with_pool(source, lang, &pool)?;

    let root = tree.root_node();

    let mut results = Vec::with_capacity(symbols.len());
    for sym in &symbols {
        if sym.kind.is_callable() {
            let node = find_deepest_node(root, sym.start_byte, sym.end_byte);
            let cc = compute_for_node(source, node, lang);
            results.push((sym.name.clone(), cc));
        }
    }

    Ok(results)
}

/// Enrich a mutable vec of Symbols with computed complexity.
///
/// This is the preferred API for hooks — no need to handle `tree_sitter::Node` directly.
/// Only callable symbols (functions, methods) get complexity populated.
pub fn enrich_symbols_with_complexity(
    symbols: &mut [crate::ast::symbols::Symbol],
    source: &str,
    lang: Lang,
) -> AstResult<()> {
    let pool = crate::ast::parser::ParserPool::new();
    let tree = pool.parse(source, lang)?;
    enrich_symbols_with_complexity_from_tree(symbols, source, &tree, lang);
    Ok(())
}

/// Enrich symbols with complexity using a pre-parsed tree (zero allocation).
///
/// P2.3: When the caller already has a `Tree` (e.g., from `IncrementalParser`),
/// this avoids re-parsing the source entirely.
pub fn enrich_symbols_with_complexity_from_tree(
    symbols: &mut [crate::ast::symbols::Symbol],
    source: &str,
    tree: &tree_sitter::Tree,
    lang: Lang,
) {
    let root = tree.root_node();
    for sym in symbols.iter_mut() {
        if sym.kind.is_callable() {
            let node = find_deepest_node(root, sym.start_byte, sym.end_byte);
            let cc = compute_for_node(source, node, lang);
            sym.complexity = Some(cc);
        }
    }
}

/// Compute complexity for all functions/methods using a pre-parsed tree.
///
/// P2.3: Avoids re-parsing when the caller already has the tree.
pub fn compute_complexity_for_source_from_tree(
    source: &str,
    lang: Lang,
    tree: &tree_sitter::Tree,
) -> AstResult<Vec<(String, u16)>> {
    let pool = crate::ast::parser::ParserPool::new();
    let symbols = crate::ast::symbols::extract_symbols_with_pool(source, lang, &pool)?;
    let root = tree.root_node();

    let mut results = Vec::with_capacity(symbols.len());
    for sym in &symbols {
        if sym.kind.is_callable() {
            let node = find_deepest_node(root, sym.start_byte, sym.end_byte);
            let cc = compute_for_node(source, node, lang);
            results.push((sym.name.clone(), cc));
        }
    }

    Ok(results)
}

/// Compute cyclomatic complexity for a symbol using a pre-parsed tree.
///
/// P2.3: Avoids re-parsing when the caller already has the tree.
pub fn compute_complexity_from_tree(
    source: &str,
    tree: &tree_sitter::Tree,
    lang: Lang,
    start_byte: usize,
    end_byte: usize,
) -> u16 {
    let root = tree.root_node();
    let node = find_deepest_node(root, start_byte, end_byte);
    compute_for_node(source, node, lang)
}

/// Find the deepest node that covers the given byte range.
fn find_deepest_node(node: Node, start_byte: usize, end_byte: usize) -> Node {
    let mut best = node;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.start_byte() <= start_byte && child.end_byte() >= end_byte {
            best = find_deepest_node(child, start_byte, end_byte);
        }
    }

    best
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test result vecs are asserted non-empty before indexing
    use super::*;
    use crate::ast::symbols::extract_symbols;

    #[test]
    fn test_simple_function_cc1() {
        let source = "def simple():\n    return 42\n";
        let result = compute_complexity_for_source(source, Lang::Python).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "simple");
        assert_eq!(result[0].1, 1, "Simple function should have CC=1");
    }

    #[test]
    fn test_if_else_cc2() {
        let source = r#"
def check(x):
    if x > 0:
        return "positive"
    return "non-positive"
"#;
        let result = compute_complexity_for_source(source, Lang::Python).unwrap();
        assert_eq!(result[0].0, "check");
        assert!(
            result[0].1 >= 2,
            "if adds +1: CC should be >= 2, got {}",
            result[0].1
        );
    }

    #[test]
    fn test_multiple_branches_python() {
        let source = r#"
def complex(x, y):
    if x > 0:
        for i in range(x):
            if y > i:
                return True
            elif y == i:
                continue
    while x > 0:
        x -= 1
    return False
"#;
        let result = compute_complexity_for_source(source, Lang::Python).unwrap();
        assert_eq!(result[0].0, "complex");
        // if + for + if + elif + while = 5 branches → CC = 6
        assert!(result[0].1 >= 5, "CC should be >= 5, got {}", result[0].1);
    }

    #[test]
    fn test_logical_operators_python() {
        let source = r#"
def logic(a, b, c):
    if a and b or c:
        return True
    return False
"#;
        let result = compute_complexity_for_source(source, Lang::Python).unwrap();
        // if + and + or = 3 branches → CC = 4
        assert!(
            result[0].1 >= 3,
            "and/or should add CC, got {}",
            result[0].1
        );
    }

    #[test]
    fn test_rust_match_complexity() {
        let source = r#"
fn handle(x: i32) -> &'static str {
    match x {
        0 => "zero",
        1 => "one",
        _ => "other",
    }
}
"#;
        let result = compute_complexity_for_source(source, Lang::Rust).unwrap();
        assert_eq!(result[0].0, "handle");
        // match + 3 arms = 4 → CC = 5
        assert!(
            result[0].1 >= 4,
            "match + arms should add CC, got {}",
            result[0].1
        );
    }

    #[test]
    fn test_rust_combined() {
        let source = r#"
fn process(data: &[i32]) -> i32 {
    let mut sum = 0;
    for item in data {
        if *item > 0 && *item < 100 {
            sum += item;
        } else if *item >= 100 {
            sum += 100;
        }
    }
    sum
}
"#;
        let result = compute_complexity_for_source(source, Lang::Rust).unwrap();
        assert_eq!(result[0].0, "process");
        // for + if + && + else_clause = 4 → CC >= 5
        assert!(result[0].1 >= 4, "CC should be >= 4, got {}", result[0].1);
    }

    #[test]
    fn test_typescript_ternary() {
        let source = r#"
function choose(x: number): string {
    return x > 0 ? "pos" : "neg";
}
"#;
        let result = compute_complexity_for_source(source, Lang::TypeScript).unwrap();
        assert_eq!(result[0].0, "choose");
        // ternary = 1 → CC = 2
        assert!(
            result[0].1 >= 2,
            "ternary should add CC, got {}",
            result[0].1
        );
    }

    #[test]
    fn test_javascript_switch() {
        let source = r#"
function grade(score) {
    switch(true) {
        case score >= 90: return "A";
        case score >= 80: return "B";
        case score >= 70: return "C";
        default: return "F";
    }
}
"#;
        let result = compute_complexity_for_source(source, Lang::JavaScript).unwrap();
        assert_eq!(result[0].0, "grade");
        // 4 switch_case = 4 → CC = 5
        assert!(
            result[0].1 >= 4,
            "switch cases should add CC, got {}",
            result[0].1
        );
    }

    #[test]
    fn test_class_method_not_counted() {
        // Classes themselves shouldn't have complexity computed
        let source = r#"
class Foo:
    def method(self):
        if True:
            return 1
        return 0
"#;
        let result = compute_complexity_for_source(source, Lang::Python).unwrap();
        // Only callable symbols get complexity
        assert!(
            result.iter().all(|(name, _)| name != "Foo"),
            "Class should not have complexity computed"
        );
    }

    #[test]
    fn test_compute_complexity_direct() {
        let source = "def f():\n    if True:\n        pass\n";
        let cc = compute_complexity(source, Lang::Python, 0, source.len()).unwrap();
        assert!(cc >= 2, "Direct compute should work, got CC={}", cc);
    }

    #[test]
    fn test_enriched_symbols_with_complexity() {
        let source = r#"
def simple():
    return 1

def complex(x):
    if x > 0:
        for i in range(x):
            pass
    return 0
"#;
        let symbols = extract_symbols(source, Lang::Python).unwrap();
        assert_eq!(symbols.len(), 2);

        // Manually compute and verify complexity enrichment
        let pool = crate::ast::parser::ParserPool::new();
        let tree = pool.parse(source, Lang::Python).unwrap();
        let root = tree.root_node();

        for sym in &symbols {
            let node = find_deepest_node(root, sym.start_byte, sym.end_byte);
            let cc = compute_for_node(source, node, Lang::Python);
            if sym.name == "simple" {
                assert_eq!(cc, 1, "simple() should have CC=1");
            } else if sym.name == "complex" {
                assert!(cc >= 3, "complex() should have CC>=3, got {}", cc);
            }
        }
    }

    // ── P2.3: Tree-reuse API tests ──────────────────────────────────

    #[test]
    fn test_compute_complexity_from_tree() {
        let source = "def f():\n    if True:\n        pass\n";
        let pool = crate::ast::parser::ParserPool::new();
        let tree = pool.parse(source, Lang::Python).unwrap();
        let cc = super::compute_complexity_from_tree(source, &tree, Lang::Python, 0, source.len());
        assert!(cc >= 2, "from_tree should work, got CC={}", cc);
    }

    #[test]
    fn test_compute_complexity_for_source_from_tree() {
        let source = r#"
def simple():
    return 42

def branchy(x):
    if x > 0:
        for i in range(x):
            pass
    return 0
"#;
        let pool = crate::ast::parser::ParserPool::new();
        let tree = pool.parse(source, Lang::Python).unwrap();
        let result =
            super::compute_complexity_for_source_from_tree(source, Lang::Python, &tree).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "simple");
        assert_eq!(result[0].1, 1);
        assert!(
            result[1].1 >= 3,
            "branchy CC should be >= 3, got {}",
            result[1].1
        );
    }

    #[test]
    fn test_enrich_symbols_with_complexity_from_tree() {
        let source = "def foo():\n    if True:\n        pass\n";
        let pool = crate::ast::parser::ParserPool::new();
        let tree = pool.parse(source, Lang::Python).unwrap();
        let mut symbols = extract_symbols(source, Lang::Python).unwrap();

        super::enrich_symbols_with_complexity_from_tree(&mut symbols, source, &tree, Lang::Python);

        assert!(symbols[0].complexity.is_some());
        assert!(symbols[0].complexity.unwrap() >= 2);
    }

    #[test]
    fn test_from_tree_matches_reparse() {
        // Verify that tree-reuse produces identical results to re-parsing.
        let source = r#"
fn handle(x: i32) -> &'static str {
    match x {
        0 => "zero",
        1 => "one",
        _ => "other",
    }
}
"#;
        let reparse_result = compute_complexity_for_source(source, Lang::Rust).unwrap();
        let pool = crate::ast::parser::ParserPool::new();
        let tree = pool.parse(source, Lang::Rust).unwrap();
        let tree_result =
            super::compute_complexity_for_source_from_tree(source, Lang::Rust, &tree).unwrap();

        assert_eq!(
            reparse_result, tree_result,
            "Tree-reuse must match re-parse"
        );
    }
}
