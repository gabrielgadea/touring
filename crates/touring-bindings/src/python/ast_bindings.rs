//! PyO3 bindings for touring-ast — exposes AST analysis to Python.
//!
//! Provides: symbol extraction, complexity analysis, syntax validation,
//! import extraction, and supported language listing.

use std::collections::HashMap;
use std::path::Path;

use pyo3::prelude::*;

use touring_code::ast::{
    Lang, Symbol, compute_complexity_for_source, enrich_symbols_with_complexity, extract_symbols,
    extract_symbols_from_file, validate_syntax,
};

use crate::python::exceptions::AstParseError;

// ═══════════════════════════════════════════════════════════════════════════
// PyAstSymbol — immutable wrapper around touring_code::ast::Symbol
// ═══════════════════════════════════════════════════════════════════════════

/// A Python-exposed symbol extracted from source code via tree-sitter.
#[pyclass(frozen, get_all, name = "AstSymbol", skip_from_py_object)]
#[derive(Debug, Clone)]
pub struct PyAstSymbol {
    /// Symbol name (e.g., "my_function").
    pub name: String,
    /// Symbol kind as string (e.g., "function", "class", "method").
    pub kind: String,
    /// Start line (1-indexed).
    pub line: usize,
    /// End line (1-indexed).
    pub end_line: usize,
    /// Start column (0-indexed, byte offset within line).
    pub column: usize,
    /// Signature without body.
    pub signature: String,
    /// Whether the symbol is public.
    pub is_public: bool,
    /// Name of the parent container (e.g., "MyClass" for a method).
    pub parent_name: Option<String>,
    /// First line of the docstring/doc comment.
    pub docstring: Option<String>,
    /// Whether the function/method is async.
    pub is_async: bool,
    /// Visibility level as string (e.g., "public", "private").
    pub visibility: Option<String>,
    /// Cyclomatic complexity (populated when requested).
    pub complexity: Option<u16>,
    /// Decorator names.
    pub decorators: Vec<String>,
    /// Start byte offset in source (for surgery operations).
    pub start_byte: usize,
    /// End byte offset in source (for surgery operations).
    pub end_byte: usize,
}

impl From<&Symbol> for PyAstSymbol {
    fn from(s: &Symbol) -> Self {
        Self {
            name: s.name.clone(),
            kind: s.kind.as_str().to_string(),
            line: s.line,
            end_line: s.end_line,
            column: s.column,
            signature: s.signature.clone(),
            is_public: s.is_public,
            parent_name: s.parent_name.clone(),
            docstring: s.docstring.clone(),
            is_async: s.is_async,
            visibility: s.visibility.map(|v| v.as_str().to_string()),
            complexity: s.complexity,
            decorators: s.decorators.clone(),
            start_byte: s.start_byte,
            end_byte: s.end_byte,
        }
    }
}

#[pymethods]
impl PyAstSymbol {
    fn __repr__(&self) -> String {
        format!(
            "AstSymbol(name='{}', kind='{}', lines={}-{})",
            self.name, self.kind, self.line, self.end_line
        )
    }

    /// Serialize to JSON string.
    fn to_dict(&self) -> PyResult<String> {
        let json = serde_json::json!({
            "name": self.name,
            "kind": self.kind,
            "line": self.line,
            "end_line": self.end_line,
            "column": self.column,
            "signature": self.signature,
            "is_public": self.is_public,
            "parent_name": self.parent_name,
            "docstring": self.docstring,
            "is_async": self.is_async,
            "visibility": self.visibility,
            "complexity": self.complexity,
            "decorators": self.decorators,
        });
        serde_json::to_string(&json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Free functions
// ═══════════════════════════════════════════════════════════════════════════

/// Extract symbols from source code string.
///
/// Args:
///     source: The source code to parse.
///     file_path: Path used for language detection (e.g., "main.py").
///     include_complexity: Whether to compute cyclomatic complexity (default: True).
///
/// Returns:
///     List of AstSymbol objects.
#[pyfunction]
#[pyo3(signature = (source, file_path, include_complexity=true))]
pub fn py_extract_symbols(
    py: Python<'_>,
    source: String,
    file_path: String,
    include_complexity: bool,
) -> PyResult<Vec<PyAstSymbol>> {
    let lang = Lang::from_path(Path::new(&file_path))
        .ok_or_else(|| AstParseError::new_err(format!("Unsupported language for: {file_path}")))?;

    let result = py.detach(|| {
        let mut symbols = extract_symbols(&source, lang).map_err(|e| e.to_string())?;
        if include_complexity {
            let _ = enrich_symbols_with_complexity(&mut symbols, &source, lang);
        }
        Ok::<Vec<Symbol>, String>(symbols)
    });

    match result {
        Ok(symbols) => Ok(symbols.iter().map(PyAstSymbol::from).collect()),
        Err(e) => Err(AstParseError::new_err(e)),
    }
}

/// Extract symbols from a file on disk.
///
/// Args:
///     file_path: Absolute path to the source file.
///
/// Returns:
///     List of AstSymbol objects.
#[pyfunction]
pub fn py_extract_symbols_from_file(
    py: Python<'_>,
    file_path: String,
) -> PyResult<Vec<PyAstSymbol>> {
    let path = Path::new(&file_path);
    let result = py.detach(|| extract_symbols_from_file(path).map_err(|e| e.to_string()));

    match result {
        Ok(symbols) => Ok(symbols.iter().map(PyAstSymbol::from).collect()),
        Err(e) => Err(AstParseError::new_err(e)),
    }
}

/// Compute cyclomatic complexity for all functions in source.
///
/// Args:
///     source: The source code to analyze.
///     file_path: Path used for language detection.
///
/// Returns:
///     Dict of {function_name: complexity}.
#[pyfunction]
pub fn py_compute_complexity(
    py: Python<'_>,
    source: String,
    file_path: String,
) -> PyResult<HashMap<String, u16>> {
    let lang = Lang::from_path(Path::new(&file_path))
        .ok_or_else(|| AstParseError::new_err(format!("Unsupported language for: {file_path}")))?;

    py.detach(|| compute_complexity_for_source(&source, lang).map_err(|e| e.to_string()))
        .map(|v| v.into_iter().collect())
        .map_err(AstParseError::new_err)
}

/// Validate syntax of source code.
///
/// Args:
///     source: The source code to validate.
///     file_path: Path used for language detection.
///
/// Returns:
///     True if valid, False if there are parse errors.
#[pyfunction]
pub fn py_validate_syntax(source: String, file_path: String) -> PyResult<bool> {
    let lang = Lang::from_path(Path::new(&file_path))
        .ok_or_else(|| AstParseError::new_err(format!("Unsupported language for: {file_path}")))?;

    match validate_syntax(&source, lang) {
        Ok(valid) => Ok(valid),
        // Syntax errors are not exceptions — they mean "invalid syntax"
        Err(_) => Ok(false),
    }
}

/// Extract import information from source code.
///
/// Args:
///     source: The source code to analyze.
///     file_path: Path used for language detection.
///
/// Returns:
///     List of (module_path, [imported_symbols]) tuples.
#[pyfunction]
pub fn py_extract_imports(
    source: String,
    file_path: String,
) -> PyResult<Vec<(String, Vec<String>)>> {
    let lang = Lang::from_path(Path::new(&file_path))
        .ok_or_else(|| AstParseError::new_err(format!("Unsupported language for: {file_path}")))?;

    let imports = touring_code::ast::graph::extract_imports(&source, lang);
    Ok(imports
        .into_iter()
        .map(|i| (i.module_path, i.symbols))
        .collect())
}

/// Get list of supported languages.
///
/// Returns:
///     List of language name strings.
#[pyfunction]
pub fn py_supported_languages() -> Vec<String> {
    vec![
        "python".into(),
        "rust".into(),
        "typescript".into(),
        "javascript".into(),
        "bash".into(),
        "html".into(),
        "css".into(),
        "markdown".into(),
        "json".into(),
        "toml".into(),
        "yaml".into(),
    ]
}

// ═══════════════════════════════════════════════════════════════════════════
// Module registration
// ═══════════════════════════════════════════════════════════════════════════

/// Register all AST bindings in the parent module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAstSymbol>()?;
    m.add_function(wrap_pyfunction!(py_extract_symbols, m)?)?;
    m.add_function(wrap_pyfunction!(py_extract_symbols_from_file, m)?)?;
    m.add_function(wrap_pyfunction!(py_compute_complexity, m)?)?;
    m.add_function(wrap_pyfunction!(py_validate_syntax, m)?)?;
    m.add_function(wrap_pyfunction!(py_extract_imports, m)?)?;
    m.add_function(wrap_pyfunction!(py_supported_languages, m)?)?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_symbols_python() {
        let source = "def hello():\n    pass\n";
        let symbols = extract_symbols(source, Lang::Python).unwrap();
        assert!(!symbols.is_empty());
        let py_sym = PyAstSymbol::from(&symbols[0]);
        assert_eq!(py_sym.name, "hello");
        assert_eq!(py_sym.kind, "function");
    }

    #[test]
    fn test_extract_symbols_with_complexity() {
        let source = "def foo(x):\n    if x > 0:\n        return x\n    return 0\n";
        let mut symbols = extract_symbols(source, Lang::Python).unwrap();
        let _ = enrich_symbols_with_complexity(&mut symbols, source, Lang::Python);
        let py_sym = PyAstSymbol::from(&symbols[0]);
        assert!(py_sym.complexity.is_some());
        assert!(py_sym.complexity.unwrap() >= 2, "if branch adds CC");
    }

    #[test]
    fn test_compute_complexity_for_source() {
        let source = "def simple():\n    return 1\n";
        let results = compute_complexity_for_source(source, Lang::Python).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "simple");
        assert_eq!(results[0].1, 1);
    }

    #[test]
    fn test_validate_syntax_valid_python() {
        let result = validate_syntax("def f(): pass\n", Lang::Python);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_validate_syntax_invalid_python() {
        // Missing colon — tree-sitter still parses but flags errors
        let result = validate_syntax("def f()\n    pass\n", Lang::Python);
        // Either Ok(false) or Err — both mean invalid syntax
        if let Ok(valid) = result {
            assert!(!valid);
        }
        // Err is also acceptable: SyntaxError signals invalid syntax.
    }

    #[test]
    fn test_extract_imports_python() {
        let source = "from os.path import join, exists\nimport sys\n";
        let imports = touring_code::ast::graph::extract_imports(source, Lang::Python);
        assert!(!imports.is_empty());
        // Check at least one import was found
        let has_os_path = imports.iter().any(|i| i.module_path.contains("os"));
        let has_sys = imports.iter().any(|i| i.module_path.contains("sys"));
        assert!(has_os_path || has_sys, "Should find at least one import");
    }

    #[test]
    fn test_supported_languages_count() {
        let langs = py_supported_languages();
        assert_eq!(langs.len(), 11, "Should support 11 languages");
        assert!(langs.contains(&"python".to_string()));
        assert!(langs.contains(&"rust".to_string()));
    }

    #[test]
    fn test_py_ast_symbol_repr() {
        let sym = PyAstSymbol {
            name: "test_fn".into(),
            kind: "function".into(),
            line: 1,
            end_line: 3,
            column: 0,
            signature: "def test_fn():".into(),
            is_public: true,
            parent_name: None,
            docstring: None,
            is_async: false,
            visibility: Some("public".into()),
            complexity: Some(1),
            decorators: vec![],
            start_byte: 0,
            end_byte: 0,
        };
        let repr = sym.__repr__();
        assert!(repr.contains("test_fn"));
        assert!(repr.contains("function"));
    }

    #[test]
    fn test_py_ast_symbol_to_dict() {
        let sym = PyAstSymbol {
            name: "my_class".into(),
            kind: "class".into(),
            line: 10,
            end_line: 20,
            column: 0,
            signature: "class MyClass:".into(),
            is_public: true,
            parent_name: None,
            docstring: Some("A docstring".into()),
            is_async: false,
            visibility: Some("public".into()),
            complexity: None,
            decorators: vec!["dataclass".into()],
            start_byte: 0,
            end_byte: 0,
        };
        let json = sym.to_dict().unwrap();
        assert!(json.contains("\"name\":\"my_class\""));
        assert!(json.contains("\"kind\":\"class\""));
        assert!(json.contains("dataclass"));
    }

    #[test]
    fn test_lang_from_path_unsupported() {
        let result = Lang::from_path(Path::new("file.xyz"));
        assert!(result.is_none());
    }
}
