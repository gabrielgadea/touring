//! Python Syntax Validation — native Rust replacement for qa_python_syntax.py.
//!
//! Uses tree-sitter Python parser (via touring-ast's `validate_syntax`) for
//! fast syntax checking without executing the file. Equivalent to Python's
//! `py_compile.compile()`.
//!
//! Performance: <1ms per file (vs ~15ms Python py_compile).

use std::path::Path;

/// Result of a syntax validation check.
#[derive(Debug, Clone)]
pub struct SyntaxResult {
    /// Whether the syntax is valid.
    pub valid: bool,
    /// Error description if invalid, empty if valid.
    pub error: String,
}

/// Validate Python source code syntax using tree-sitter.
///
/// Parses the source and checks for ERROR or MISSING nodes in the AST.
/// This is equivalent to Python's `py_compile.compile()` but runs in <1ms.
///
/// # Arguments
/// * `source` - Python source code to validate
///
/// # Returns
/// `SyntaxResult` with validity status and error details.
pub fn validate_python_syntax(source: &str) -> SyntaxResult {
    match touring_code::ast::validate_syntax(source, touring_code::ast::Lang::Python) {
        Ok(true) => SyntaxResult {
            valid: true,
            error: String::new(),
        },
        Ok(false) => SyntaxResult {
            valid: false,
            error: "Syntax errors detected".to_string(),
        },
        Err(e) => SyntaxResult {
            valid: false,
            error: e.to_string(),
        },
    }
}

/// Validate a Python file's syntax by reading it from disk.
///
/// # Arguments
/// * `path` - Path to the Python file
///
/// # Returns
/// `SyntaxResult` with validity status and error details.
pub fn validate_python_file(path: &Path) -> SyntaxResult {
    if !path.exists() {
        return SyntaxResult {
            valid: false,
            error: format!("File not found: {}", path.display()),
        };
    }

    match std::fs::read_to_string(path) {
        Ok(source) => validate_python_syntax(&source),
        Err(e) => SyntaxResult {
            valid: false,
            error: format!("Failed to read file: {}", e),
        },
    }
}

/// Compose JSON output matching the Python hook's contract.
pub fn compose_json(file_path: &str, result: &SyntaxResult) -> serde_json::Value {
    let mut json = serde_json::json!({
        "hook": "qa_python_syntax",
        "file_path": file_path,
        "syntax_valid": result.valid,
    });

    if !result.valid
        && let Some(obj) = json.as_object_mut()
    {
        obj.insert(
            "error".to_string(),
            serde_json::Value::String(result.error.clone()),
        );
    }

    json
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::len_zero)]
    use super::*;

    #[test]
    fn test_validate_syntax_valid() {
        let result = validate_python_syntax("def foo(): pass");
        assert!(result.valid, "valid Python should pass: {:?}", result);
        assert!(result.error.is_empty());
    }

    #[test]
    fn test_validate_syntax_valid_complex() {
        let source = r#"
import json
from pathlib import Path

class Config:
    """Configuration manager."""

    def __init__(self, path: str) -> None:
        self.path = Path(path)
        self.data: dict = {}

    def load(self) -> dict:
        with open(self.path) as f:
            self.data = json.load(f)
        return self.data

    @staticmethod
    def default() -> "Config":
        return Config("/etc/default.json")

if __name__ == "__main__":
    cfg = Config("test.json")
    print(cfg.load())
"#;
        let result = validate_python_syntax(source);
        assert!(
            result.valid,
            "complex valid Python should pass: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_syntax_invalid_def() {
        let result = validate_python_syntax("def foo(:");
        assert!(!result.valid, "invalid syntax should fail");
        assert!(!result.error.is_empty());
    }

    #[test]
    fn test_validate_syntax_invalid_unmatched_paren() {
        let result = validate_python_syntax("print((1 + 2)");
        assert!(!result.valid, "unmatched paren should fail");
    }

    #[test]
    fn test_validate_syntax_empty_string() {
        let result = validate_python_syntax("");
        assert!(result.valid, "empty source is valid Python");
    }

    #[test]
    fn test_validate_syntax_comment_only() {
        let result = validate_python_syntax("# just a comment\n");
        assert!(result.valid, "comment-only is valid Python");
    }

    #[test]
    fn test_validate_syntax_invalid_class() {
        let result = validate_python_syntax("class :");
        assert!(!result.valid, "class without name should fail");
    }

    #[test]
    fn test_validate_file_nonexistent() {
        let result = validate_python_file(Path::new("/nonexistent/path.py"));
        assert!(!result.valid);
        assert!(result.error.contains("File not found"));
    }

    #[test]
    fn test_compose_json_valid() {
        let result = SyntaxResult {
            valid: true,
            error: String::new(),
        };
        let json = compose_json("/path/to/test.py", &result);
        assert_eq!(json["hook"], "qa_python_syntax");
        assert_eq!(json["syntax_valid"], true);
        assert!(json.get("error").is_none());
    }

    #[test]
    fn test_compose_json_invalid() {
        let result = SyntaxResult {
            valid: false,
            error: "1:5: syntax error near '('".to_string(),
        };
        let json = compose_json("/path/to/test.py", &result);
        assert_eq!(json["hook"], "qa_python_syntax");
        assert_eq!(json["syntax_valid"], false);
        assert_eq!(json["error"], "1:5: syntax error near '('");
    }

    #[test]
    fn test_validate_syntax_with_decorator() {
        let source = r#"
@staticmethod
def hello():
    return "world"
"#;
        let result = validate_python_syntax(source);
        assert!(
            result.valid,
            "decorator syntax should be valid: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_syntax_async_function() {
        let source = "async def fetch(url: str) -> str:\n    return await get(url)\n";
        let result = validate_python_syntax(source);
        assert!(result.valid, "async function should be valid: {:?}", result);
    }

    #[test]
    fn test_validate_syntax_match_statement() {
        let source = r#"
match command:
    case "quit":
        exit()
    case "hello":
        print("hi")
"#;
        let result = validate_python_syntax(source);
        assert!(
            result.valid,
            "match statement should be valid: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_syntax_fstring() {
        let source = r#"
name = "World"
msg = f"Hello, {name}!"
print(msg)
"#;
        let result = validate_python_syntax(source);
        assert!(result.valid, "f-string should be valid: {:?}", result);
    }

    #[test]
    fn test_validate_syntax_walrus() {
        let source = r#"
if (n := len(data)) > 10:
    print(f"List is too long ({n} elements)")
"#;
        let result = validate_python_syntax(source);
        assert!(
            result.valid,
            "walrus operator should be valid: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_file_with_temp() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("valid.py");
        std::fs::write(&file_path, "def hello():\n    return 42\n").unwrap();

        let result = validate_python_file(&file_path);
        assert!(result.valid, "temp file should be valid: {:?}", result);
    }

    #[test]
    fn test_validate_file_invalid_temp() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("invalid.py");
        std::fs::write(&file_path, "def foo(:\n").unwrap();

        let result = validate_python_file(&file_path);
        assert!(!result.valid, "invalid temp file should fail");
    }
}
