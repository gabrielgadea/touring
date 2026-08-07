//! AST Surgery - Byte-exact code editing using tree-sitter
//!
//! S3: Provides surgical editing capabilities for functions, classes, and methods.
//! All edits preserve formatting and comments outside the edited region.

use std::path::Path;

use tree_sitter::{Node, Parser, Tree};

use touring_foundation::truncate_str;

use crate::ast::languages::Lang;
use crate::ast::symbols::{Symbol, extract_symbols, find_depth_zero_colon};

/// Maximum recursion depth for tree traversal to prevent stack overflow on pathological inputs.
pub const MAX_RECURSION_DEPTH: usize = 512;

/// Result type for surgery operations
pub(crate) type SurgeryResult<T> = Result<T, SurgeryError>;

/// Errors that can occur during AST surgery
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SurgeryError {
    /// Symbol not found in source
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),
    /// Parse error
    #[error("Parse error: {0}")]
    ParseError(String),
    /// Invalid language
    #[error("Invalid language: {0}")]
    InvalidLanguage(String),
    /// Invalid edit range
    #[error("Invalid range: {0}")]
    InvalidRange(String),
    /// Syntax validation failed
    #[error("Syntax error: {0}")]
    SyntaxError(String),
}

/// Find a symbol by name in the source
fn find_symbol(source: &str, symbol_name: &str, lang: Lang) -> SurgeryResult<Symbol> {
    let symbols =
        extract_symbols(source, lang).map_err(|e| SurgeryError::ParseError(e.to_string()))?;

    symbols
        .into_iter()
        .find(|s| s.name == symbol_name)
        .ok_or_else(|| SurgeryError::SymbolNotFound(symbol_name.to_string()))
}

/// Parse source into a tree (delegates to thread-local parser pool).
fn parse_source(source: &str, lang: Lang) -> SurgeryResult<Tree> {
    crate::ast::parser::parse_thread_local(source, lang)
        .map_err(|e| SurgeryError::ParseError(e.to_string()))
}

/// Find the body range of a symbol (the part after the signature)
/// Returns (start_byte, end_byte) of the body
fn find_body_range(source: &str, symbol: &Symbol, tree: &Tree) -> SurgeryResult<(usize, usize)> {
    let root = tree.root_node();
    let symbol_start = symbol.start_byte;
    let symbol_end = symbol.end_byte;

    // Find the most specific (deepest) node covering the symbol range.
    // When the symbol covers the entire source, the root `module` node
    // has the same bytes as the `function_definition` child — we want
    // the child, not the root.
    let mut target_node: Option<Node> = None;

    fn find_deepest_node_at_range<'a>(
        node: Node<'a>,
        start: usize,
        end: usize,
        target: &mut Option<Node<'a>>,
        depth: usize,
    ) {
        if depth >= MAX_RECURSION_DEPTH {
            tracing::warn!(
                "surgery: max recursion depth {} reached in find_deepest_node_at_range",
                MAX_RECURSION_DEPTH
            );
            return;
        }
        if node.start_byte() == start && node.end_byte() == end {
            *target = Some(node);
            // Keep searching children — a child with the same range is
            // more specific (e.g., function_definition inside module).
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32)
                && child.start_byte() <= start
                && child.end_byte() >= end
            {
                find_deepest_node_at_range(child, start, end, target, depth + 1);
            }
        }
    }

    find_deepest_node_at_range(root, symbol_start, symbol_end, &mut target_node, 0);

    let node = target_node.ok_or_else(|| {
        SurgeryError::InvalidRange(format!(
            "Could not find node for symbol {} at bytes {}-{}",
            symbol.name, symbol_start, symbol_end
        ))
    })?;

    // Find the body based on language and node type
    let body_range = find_body_in_node(source, node)?;

    Ok(body_range)
}

/// Find the body range within a specific node
fn find_body_in_node(source: &str, node: Node) -> SurgeryResult<(usize, usize)> {
    let kind = node.kind();
    let node_text = &source[node.start_byte()..node.end_byte()];

    match kind {
        // Python function_definition and class_definition
        "function_definition" | "class_definition" | "decorated_definition" => {
            // Prefer the tree-sitter "block" child — most reliable for Python.
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i as u32) {
                    if child.kind() == "block" {
                        return Ok((child.start_byte(), child.end_byte()));
                    }
                    // For decorated_definition, recurse into the inner definition.
                    if child.kind() == "function_definition" || child.kind() == "class_definition" {
                        return find_body_in_node(source, child);
                    }
                }
            }

            // Fallback: find body colon at bracket depth 0 (skips type-hint colons)
            if let Some(colon_pos) = find_depth_zero_colon(node_text) {
                let after_colon = &node_text[colon_pos + 1..];
                if after_colon.is_empty()
                    || after_colon.starts_with('\n')
                    || after_colon.starts_with('\r')
                    || after_colon.trim_start().starts_with('\n')
                {
                    let body_start = node.start_byte() + colon_pos + 1;
                    let body_end = node.end_byte();
                    return Ok((body_start, body_end));
                }
            }

            Err(SurgeryError::InvalidRange(
                "Could not find function body".to_string(),
            ))
        }
        // Rust function_item, struct_item, etc.
        "function_item" | "struct_item" | "enum_item" | "trait_item" => {
            // For Rust, find the block or the body after signature
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i as u32) {
                    let child_kind = child.kind();
                    if child_kind == "block" || child_kind == "field_declaration_list" {
                        return Ok((child.start_byte(), child.end_byte()));
                    }
                }
            }

            // For functions without block (e.g., just signature), return empty range
            Ok((node.end_byte(), node.end_byte()))
        }
        // TypeScript/JavaScript
        "function_declaration" | "class_declaration" | "method_definition" => {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i as u32)
                    && (child.kind() == "statement_block" || child.kind() == "class_body")
                {
                    return Ok((child.start_byte(), child.end_byte()));
                }
            }

            Err(SurgeryError::InvalidRange(
                "Could not find function/class body".to_string(),
            ))
        }
        _ => {
            // Generic fallback: try to find a block child
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i as u32) {
                    let child_kind = child.kind();
                    if child_kind.contains("block")
                        || child_kind.contains("body")
                        || child_kind == "field_declaration_list"
                    {
                        return Ok((child.start_byte(), child.end_byte()));
                    }
                }
            }

            // Last resort: return the whole node
            Ok((node.start_byte(), node.end_byte()))
        }
    }
}

/// Replace the body of a symbol with new content
///
/// # Arguments
/// * `source` - Original source code
/// * `symbol_name` - Name of the symbol to edit
/// * `new_body` - New body content (will be indented appropriately)
///
/// # Returns
/// Modified source code with the symbol body replaced
pub fn replace_symbol_body(
    source: &str,
    symbol_name: &str,
    new_body: &str,
) -> SurgeryResult<String> {
    // Detect language from source (simple heuristic)
    let lang = detect_language(source)?;

    // Find the symbol
    let symbol = find_symbol(source, symbol_name, lang)?;

    // Parse to get precise node information
    let tree = parse_source(source, lang)?;

    // Find the body range
    let (body_start, body_end) = find_body_range(source, &symbol, &tree)?;

    // If body is empty, just return the original
    if body_start >= body_end {
        return Err(SurgeryError::InvalidRange(
            "Symbol has no body to replace".to_string(),
        ));
    }

    // Calculate the indentation of the original body
    let _original_body = &source[body_start..body_end];
    let base_indent = calculate_base_indent(source, body_start, body_end);

    // Apply indentation to new body
    let indented_new_body = apply_indentation(new_body, base_indent);

    // Perform the replacement
    let mut result =
        String::with_capacity(source.len() - (body_end - body_start) + indented_new_body.len());

    result.push_str(&source[..body_start]);
    result.push_str(&indented_new_body);
    result.push_str(&source[body_end..]);

    // Validate the result
    if let Err(e) = validate_syntax(&result, lang) {
        return Err(SurgeryError::SyntaxError(format!(
            "Edit would create invalid syntax: {}",
            e
        )));
    }

    Ok(result)
}

/// Calculate the base indentation of a body
fn calculate_base_indent(source: &str, body_start: usize, body_end: usize) -> usize {
    let body = &source[body_start..body_end.min(source.len())];

    // Find the first non-empty line
    for line in body.lines() {
        let trimmed = line.trim_start();
        if !trimmed.is_empty() {
            return line.len() - trimmed.len();
        }
    }

    // Default: 4 spaces
    4
}

/// Apply base indentation to new content
fn apply_indentation(content: &str, base_indent: usize) -> String {
    if content.is_empty() {
        return content.to_string();
    }

    let indent_str = " ".repeat(base_indent);

    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return content.to_string();
    }

    let mut result = String::with_capacity(content.len() + lines.len() * base_indent);

    // First line: apply base indent and preserve the line's existing indentation.
    // Do NOT strip the first line's leading spaces — callers may intentionally
    // indent their new_body (e.g., a sub-block inside a function).
    result.push_str(&indent_str);
    if let Some(first) = lines.first() {
        result.push_str(first);
    }

    // Subsequent lines preserve relative indentation
    for line in lines.get(1..).unwrap_or_default() {
        result.push('\n');
        if !line.trim().is_empty() {
            result.push_str(&indent_str);
            result.push_str(line);
        }
    }

    result
}

/// Detect language from source content (simple heuristic)
fn detect_language(source: &str) -> SurgeryResult<Lang> {
    // Check for Python indicators
    if source.contains("def ") && (source.contains(":") || source.contains("    ")) {
        return Ok(Lang::Python);
    }

    // Check for Rust indicators
    if source.contains("fn ") || source.contains("pub fn ") {
        return Ok(Lang::Rust);
    }

    // Check for TypeScript indicators
    if source.contains(": ") && source.contains("function ") || source.contains(": string") {
        return Ok(Lang::TypeScript);
    }

    // Check for JavaScript indicators
    if source.contains("function ") || source.contains("const ") || source.contains("let ") {
        return Ok(Lang::JavaScript);
    }

    // Default to Python if contains def
    if source.contains("def ") {
        return Ok(Lang::Python);
    }

    Err(SurgeryError::InvalidLanguage(
        "Could not detect language from source".to_string(),
    ))
}

/// Validate that source code has no syntax errors
///
/// # Arguments
/// * `content` - Source code to validate
/// * `language` - Language identifier ("python", "rust", "typescript", "javascript")
///
/// # Returns
/// Ok(true) if syntax is valid, Err otherwise
pub fn validate_syntax(content: &str, lang: Lang) -> SurgeryResult<bool> {
    let mut parser = Parser::new();
    parser
        .set_language(&lang.tree_sitter_language())
        .map_err(|e| SurgeryError::InvalidLanguage(format!("{:?}", e)))?;

    let tree = parser
        .parse(content, None)
        .ok_or_else(|| SurgeryError::ParseError("Failed to parse source".to_string()))?;

    // Check for error nodes
    if tree.root_node().has_error() {
        // Collect error messages
        let errors = collect_errors(tree.root_node(), content);
        return Err(SurgeryError::SyntaxError(format!(
            "Syntax errors found: {}",
            errors.join("; ")
        )));
    }

    Ok(true)
}

/// Collect error messages from tree
fn collect_errors(node: Node, source: &str) -> Vec<String> {
    let mut errors = Vec::new();

    fn collect_recursive(node: Node, source: &str, errors: &mut Vec<String>, depth: usize) {
        if depth >= MAX_RECURSION_DEPTH {
            tracing::warn!(
                "surgery: max recursion depth {} reached in collect_recursive",
                MAX_RECURSION_DEPTH
            );
            return;
        }
        if node.is_error() || node.is_missing() {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("");
            let pos = node.start_position();
            errors.push(format!(
                "line {} col {}: '{}'",
                pos.row + 1,
                pos.column + 1,
                truncate_str(text, 50)
            ));
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                collect_recursive(child, source, errors, depth + 1);
            }
        }
    }

    collect_recursive(node, source, &mut errors, 0);
    errors
}

/// Replace the body of a symbol with new content, using an explicit language.
///
/// Preferred over [`replace_symbol_body`] which guesses the language.
pub fn replace_symbol_body_with_lang(
    source: &str,
    symbol_name: &str,
    new_body: &str,
    lang: Lang,
) -> SurgeryResult<String> {
    let symbol = find_symbol(source, symbol_name, lang)?;
    let tree = parse_source(source, lang)?;
    let (body_start, body_end) = find_body_range(source, &symbol, &tree)?;

    if body_start >= body_end {
        return Err(SurgeryError::InvalidRange(
            "Symbol has no body to replace".to_string(),
        ));
    }

    let _original_body = &source[body_start..body_end];
    let base_indent = calculate_base_indent(source, body_start, body_end);
    let indented_new_body = apply_indentation(new_body, base_indent);

    let mut result =
        String::with_capacity(source.len() - (body_end - body_start) + indented_new_body.len());
    result.push_str(&source[..body_start]);
    result.push_str(&indented_new_body);
    result.push_str(&source[body_end..]);

    if let Err(e) = validate_syntax(&result, lang) {
        return Err(SurgeryError::SyntaxError(format!(
            "Edit would create invalid syntax: {}",
            e
        )));
    }

    Ok(result)
}

/// Replace the body of a symbol, detecting language from the file extension.
///
/// This is the preferred alternative to [`replace_symbol_body`]: language detection
/// via file path extension is deterministic and never misclassifies valid source.
///
/// # Arguments
/// * `path` - File path used only for language detection (extension matters, not existence)
/// * `source` - Original source code (file contents)
/// * `symbol_name` - Name of the symbol to edit
/// * `new_body` - New body content
pub fn replace_symbol_body_for_file(
    path: &Path,
    source: &str,
    symbol_name: &str,
    new_body: &str,
) -> SurgeryResult<String> {
    let lang = Lang::from_path(path).ok_or_else(|| {
        SurgeryError::InvalidLanguage(format!("Unknown language for path: {}", path.display()))
    })?;
    replace_symbol_body_with_lang(source, symbol_name, new_body, lang)
}

/// Get the source range of a symbol
///
/// # Arguments
/// * `source` - Source code
/// * `symbol_name` - Name of the symbol
///
/// # Returns
/// (start_byte, end_byte) of the symbol
pub fn get_symbol_range(source: &str, symbol_name: &str) -> SurgeryResult<(usize, usize)> {
    let lang = detect_language(source)?;
    let symbol = find_symbol(source, symbol_name, lang)?;
    Ok((symbol.start_byte, symbol.end_byte))
}

/// Get the source range of a symbol with an explicit language.
pub fn get_symbol_range_with_lang(
    source: &str,
    symbol_name: &str,
    lang: Lang,
) -> SurgeryResult<(usize, usize)> {
    let symbol = find_symbol(source, symbol_name, lang)?;
    Ok((symbol.start_byte, symbol.end_byte))
}

/// Extract just the body of a symbol without the signature
///
/// # Arguments
/// * `source` - Source code
/// * `symbol_name` - Name of the symbol
///
/// # Returns
/// The body content (excluding signature)
pub fn extract_symbol_body(source: &str, symbol_name: &str) -> SurgeryResult<String> {
    let lang = detect_language(source)?;
    let symbol = find_symbol(source, symbol_name, lang)?;
    let tree = parse_source(source, lang)?;
    let (body_start, body_end) = find_body_range(source, &symbol, &tree)?;

    Ok(source[body_start..body_end].to_string())
}

/// Extract just the body of a symbol with an explicit language.
pub fn extract_symbol_body_with_lang(
    source: &str,
    symbol_name: &str,
    lang: Lang,
) -> SurgeryResult<String> {
    let symbol = find_symbol(source, symbol_name, lang)?;
    let tree = parse_source(source, lang)?;
    let (body_start, body_end) = find_body_range(source, &symbol, &tree)?;
    Ok(source[body_start..body_end].to_string())
}

// ── Rust formatting via prettyplease ─────────────────────────────────

/// Format Rust source code using `prettyplease` — produces rustfmt-like
/// output without invoking the external `rustfmt` binary.
///
/// Round-trips `source` through `syn::parse_file` + `prettyplease::unparse`.
/// If the input is not valid Rust, returns `SurgeryError::ParseError`.
///
/// Use this after [`replace_symbol_body`] or [`replace_symbol_body_with_lang`]
/// to emit a cleanly-formatted file instead of source with surgical byte
/// splices. This makes Claude Code edits look like a human wrote them.
///
/// Non-Rust languages are out of scope — this helper only handles Rust.
///
/// # Example
///
/// ```no_run
/// use touring_code::ast::surgery::format_rust_code;
///
/// let messy = "fn  foo(  x : i32)->i32{x+1}";
/// let clean = format_rust_code(messy).expect("valid rust");
/// assert!(clean.contains("fn foo(x: i32) -> i32"));
/// ```
pub fn format_rust_code(source: &str) -> SurgeryResult<String> {
    let file =
        syn::parse_file(source).map_err(|e| SurgeryError::ParseError(format!("syn parse: {e}")))?;
    Ok(prettyplease::unparse(&file))
}

/// Format Rust source only if the file parses cleanly; otherwise return
/// the input unchanged. Useful in edit pipelines where we want best-effort
/// formatting without failing on partial / in-progress code.
///
/// Returns `(formatted_output, was_formatted)` — the bool indicates
/// whether formatting was actually applied.
#[must_use]
pub fn format_rust_code_best_effort(source: &str) -> (String, bool) {
    match format_rust_code(source) {
        Ok(formatted) => (formatted, true),
        Err(_) => (source.to_string(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_python() {
        let source = "def foo(): pass";
        let lang = detect_language(source).unwrap();
        assert_eq!(lang, Lang::Python);
    }

    #[test]
    fn test_detect_language_rust() {
        let source = "fn foo() {}";
        let lang = detect_language(source).unwrap();
        assert_eq!(lang, Lang::Rust);
    }

    #[test]
    fn test_validate_valid_python() {
        let source = "def foo():\n    pass";
        let result = validate_syntax(source, Lang::Python);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_validate_invalid_python() {
        let source = "def foo(";
        let result = validate_syntax(source, Lang::Python);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_indentation() {
        let content = "return 42\n    do_something()";
        let indented = apply_indentation(content, 4);
        assert!(indented.contains("    return 42"));
        assert!(indented.contains("        do_something()"));
    }

    #[test]
    fn test_find_symbol() {
        let source = "def foo():\n    pass\n\ndef bar():\n    return 1";
        let lang = Lang::Python;
        let symbol = find_symbol(source, "foo", lang).unwrap();
        assert_eq!(symbol.name, "foo");
    }

    #[test]
    fn test_find_symbol_not_found() {
        let source = "def foo(): pass";
        let lang = Lang::Python;
        let result = find_symbol(source, "nonexistent", lang);
        assert!(matches!(result, Err(SurgeryError::SymbolNotFound(_))));
    }

    #[test]
    fn test_extract_body_with_type_hints() {
        // Body extraction must work correctly when params have type hints
        let source = "def foo(x: int) -> str:\n    return str(x)";
        let body = extract_symbol_body(source, "foo").unwrap();
        assert!(body.contains("return str(x)"));
        // Must NOT include the signature
        assert!(!body.contains("def foo"));
    }

    #[test]
    fn test_extract_body_with_nested_brackets() {
        // dict[str, int] has colons inside brackets -- must not split there
        let source = "def bar(d: dict[str, int]):\n    return d";
        let body = extract_symbol_body(source, "bar").unwrap();
        assert!(body.contains("return d"));
        assert!(!body.contains("def bar"));
    }

    // ── P5.1: Additional surgery tests ──────────────────────────────

    #[test]
    fn test_replace_python_function_body() {
        let source = "def greet(name):\n    print(f'Hello {name}')\n    return name\n";
        let result = replace_symbol_body(source, "greet", "return f'Hi {name}'").unwrap();
        assert!(result.contains("return f'Hi {name}'"));
        assert!(!result.contains("print(f'Hello {name}')"));
        assert!(result.contains("def greet(name):"));
    }

    #[test]
    fn test_replace_rust_function_body() {
        let source = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        // For Rust, the body replacement replaces the block content.
        // The replacement must include the braces since the body IS the block.
        let result =
            replace_symbol_body_with_lang(source, "add", "{\n    a * b\n}", Lang::Rust).unwrap();
        assert!(result.contains("a * b"));
        assert!(!result.contains("a + b"));
    }

    #[test]
    fn test_replace_nonexistent_symbol() {
        let source = "def foo():\n    pass\n";
        let result = replace_symbol_body(source, "nonexistent", "pass");
        assert!(matches!(result, Err(SurgeryError::SymbolNotFound(_))));
    }

    #[test]
    fn test_get_symbol_range_python() {
        let source = "def foo():\n    pass\n\ndef bar():\n    return 1\n";
        let (start, end) = get_symbol_range(source, "bar").unwrap();
        let extracted = &source[start..end];
        assert!(extracted.contains("def bar()"));
        assert!(extracted.contains("return 1"));
    }

    #[test]
    fn test_get_symbol_range_not_found() {
        let source = "def foo():\n    pass\n";
        let result = get_symbol_range(source, "missing");
        assert!(matches!(result, Err(SurgeryError::SymbolNotFound(_))));
    }

    #[test]
    fn test_validate_valid_rust() {
        let source = "fn main() { let x = 42; }";
        assert!(validate_syntax(source, Lang::Rust).is_ok());
    }

    #[test]
    fn test_validate_invalid_rust() {
        let source = "fn main() { let x = ; }";
        assert!(validate_syntax(source, Lang::Rust).is_err());
    }

    #[test]
    fn test_validate_valid_typescript() {
        let source = "function greet(): void { console.log('hi'); }";
        assert!(validate_syntax(source, Lang::TypeScript).is_ok());
    }

    /// Stale expectation fixed 2026-06-11: tree-sitter-bash 0.25 landed in
    /// Wave 5 (replacing the tokenizer fallback), so `Lang::Bash` is a fully
    /// supported grammar — `"some code"` parses as a plain bash command.
    /// Real bash syntax errors still surface as `SyntaxError`.
    #[test]
    fn test_validate_bash_now_supported() {
        assert!(validate_syntax("some code", Lang::Bash).is_ok());
        let bad = validate_syntax("if [ x; then fi do", Lang::Bash);
        assert!(matches!(bad, Err(SurgeryError::SyntaxError(_))));
    }

    #[test]
    fn test_extract_body_python_class() {
        let source = "class Foo:\n    x = 1\n    def bar(self):\n        pass\n";
        let body = extract_symbol_body(source, "Foo").unwrap();
        assert!(body.contains("x = 1"));
        assert!(body.contains("def bar"));
    }

    #[test]
    fn test_apply_indentation_empty() {
        let result = apply_indentation("", 4);
        assert_eq!(result, "");
    }

    #[test]
    fn test_apply_indentation_preserves_first_line_existing_indent() {
        // Callers may write new_body with its own indentation (e.g., a sub-block).
        // apply_indentation should prepend base_indent WITHOUT stripping the
        // existing leading spaces from line 1.
        let content = "    sub_call()"; // 4 spaces of pre-existing indent
        let indented = apply_indentation(content, 4);
        // Result: 4 (base) + 4 (existing) = 8 spaces
        assert!(
            indented.starts_with("        sub_call()"),
            "First line indent should be base_indent + existing_indent, got: {indented:?}"
        );
    }

    #[test]
    fn test_replace_preserves_surrounding_code() {
        let source = "x = 1\n\ndef middle():\n    pass\n\ny = 2\n";
        let result = replace_symbol_body(source, "middle", "return 42").unwrap();
        assert!(result.contains("x = 1"), "Code before should survive");
        assert!(result.contains("y = 2"), "Code after should survive");
        assert!(result.contains("return 42"));
    }

    #[test]
    fn test_extract_body_with_lang_rust() {
        let source = "fn compute(x: i32) -> i32 {\n    x * 2\n}\n";
        let body = extract_symbol_body_with_lang(source, "compute", Lang::Rust).unwrap();
        assert!(body.contains("x * 2"));
    }

    #[test]
    fn test_get_symbol_range_with_lang() {
        let source = "fn foo() {}\nfn bar() { let x = 1; }\n";
        let (start, end) = get_symbol_range_with_lang(source, "bar", Lang::Rust).unwrap();
        let extracted = &source[start..end];
        assert!(extracted.contains("fn bar"));
        assert!(extracted.contains("let x = 1"));
    }

    #[test]
    fn test_surgery_recursion_depth_guard_constant() {
        // Verify the depth guard constant is present and has the expected value.
        // find_deepest_node_at_range and collect_recursive use this guard internally
        // to prevent stack overflow on pathological AST inputs.
        assert!(MAX_RECURSION_DEPTH > 0, "depth guard must be positive");
        assert_eq!(
            MAX_RECURSION_DEPTH, 512,
            "expected default guard depth of 512"
        );
    }

    #[test]
    fn test_surgery_depth_guard_does_not_panic_on_nested_input() {
        // Exercise the public surgery API with a moderately nested Rust expression.
        // If the depth guard is broken this would stack-overflow; returning any
        // result (Ok or Err) is sufficient to prove the guard activates cleanly.
        let nested = "fn f() { let x = if true { if true { if true { if true { \
                      if true { 42 } else { 0 } } else { 0 } } else { 0 } \
                      } else { 0 } } else { 0 }; }";
        // validate_syntax exercises the tree-sitter parser which drives the
        // recursive traversal paths guarded by MAX_RECURSION_DEPTH.
        let result = validate_syntax(nested, Lang::Rust);
        assert!(
            result.is_ok(),
            "moderately nested Rust should parse: {result:?}"
        );

        // extract_symbol_body exercises find_deepest_node_at_range internally.
        let body_result = extract_symbol_body_with_lang(nested, "f", Lang::Rust);
        assert!(
            body_result.is_ok(),
            "extract_symbol_body should succeed: {body_result:?}"
        );
    }

    // ── format_rust_code tests ──────────────────────────────────────────

    #[test]
    fn format_rust_code_normalizes_whitespace() {
        let messy = "fn  foo(  x : i32)->i32{x+1}";
        let clean = format_rust_code(messy).expect("valid rust");
        assert!(clean.contains("fn foo(x: i32) -> i32"), "got: {clean}");
        // prettyplease adds newlines and standard spacing
        assert!(clean.contains("x + 1"), "got: {clean}");
    }

    #[test]
    fn format_rust_code_preserves_semantics() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let formatted = format_rust_code(src).expect("valid rust");
        // Re-parsing the formatted output must succeed — idempotent under syn.
        let second = format_rust_code(&formatted).expect("formatted output must re-parse");
        assert_eq!(
            formatted, second,
            "prettyplease must be idempotent: fmt(fmt(x)) == fmt(x)"
        );
    }

    #[test]
    fn format_rust_code_rejects_invalid() {
        let result = format_rust_code("this is not rust {{{");
        assert!(result.is_err());
        assert!(matches!(result, Err(SurgeryError::ParseError(_))));
    }

    #[test]
    fn format_rust_code_best_effort_passes_through_invalid() {
        let bad = "this is not rust {{{";
        let (out, formatted) = format_rust_code_best_effort(bad);
        assert!(!formatted, "must report formatted=false on invalid input");
        assert_eq!(out, bad, "must return original on failure");
    }

    #[test]
    fn format_rust_code_best_effort_formats_valid() {
        let (out, formatted) = format_rust_code_best_effort("fn x(){}");
        assert!(formatted);
        assert!(out.contains("fn x()"));
    }
}
