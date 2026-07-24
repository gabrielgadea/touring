//! Analyze error handling coverage in source code.
//!
//! Measures the ratio of functions returning Result/Option and
//! the density of `?` operator usage.

use memchr::memmem;

/// Error handling coverage analysis result.
#[derive(Debug, Clone)]
pub struct ErrorCoverage {
    /// Ratio of function signatures containing `Result<` or `Option<` (0.0–1.0).
    pub result_ratio: f64,
    /// Count of `?` operator usages.
    pub question_mark_count: usize,
    /// `?` per function (approximate density).
    pub question_mark_density: f64,
    /// Number of function-like signatures found.
    pub function_count: usize,
    /// Number of functions returning Result or Option.
    pub result_functions: usize,
}

/// Analyze error handling patterns in source code.
pub fn analyze_error_coverage(source: &str, language: &str) -> ErrorCoverage {
    let (fn_keyword, return_markers) = match language {
        "rust" => ("fn ", vec!["-> Result<", "-> Option<"]),
        "python" => ("def ", vec!["-> Optional[", "-> Result["]),
        "typescript" | "ts" => ("function ", vec!["Promise<", "Result<"]),
        "go" => ("func ", vec![") error", ") (error)", "(error)"]),
        "java" => ("void ", vec!["throws ", "Optional<"]),
        _ => ("fn ", vec!["Result<", "Option<"]),
    };

    let bytes = source.as_bytes();

    // Count function-like signatures
    let function_count = memmem::find_iter(bytes, fn_keyword.as_bytes()).count();

    // Count functions returning Result/Option
    let mut result_functions = 0;
    for marker in &return_markers {
        result_functions += memmem::find_iter(bytes, marker.as_bytes()).count();
    }
    // Cap at function_count to avoid ratio > 1.0
    result_functions = result_functions.min(function_count);

    // Count ? operator (Rust-specific, but useful as a signal)
    let question_mark_count = if language == "rust" {
        memmem::find_iter(bytes, b"?;").count()
            + memmem::find_iter(bytes, b"?)").count()
            + memmem::find_iter(bytes, b"?,").count()
    } else {
        0
    };

    let result_ratio = if function_count > 0 {
        result_functions as f64 / function_count as f64
    } else {
        0.0
    };

    let question_mark_density = if function_count > 0 {
        question_mark_count as f64 / function_count as f64
    } else {
        0.0
    };

    ErrorCoverage {
        result_ratio,
        question_mark_count,
        question_mark_density,
        function_count,
        result_functions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_all_result() {
        let source =
            "fn foo() -> Result<(), Error> { Ok(()) }\nfn bar() -> Result<i32, E> { Ok(1) }";
        let cov = analyze_error_coverage(source, "rust");
        assert_eq!(cov.function_count, 2);
        assert_eq!(cov.result_functions, 2);
        assert!((cov.result_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rust_no_result() {
        let source = "fn foo() -> i32 { 42 }\nfn bar() -> String { String::new() }";
        let cov = analyze_error_coverage(source, "rust");
        assert_eq!(cov.function_count, 2);
        assert_eq!(cov.result_functions, 0);
        assert_eq!(cov.result_ratio, 0.0);
    }

    #[test]
    fn test_rust_question_mark() {
        let source = "fn foo() -> Result<(), E> { let x = bar()?; let y = baz()?; Ok(()) }";
        let cov = analyze_error_coverage(source, "rust");
        assert_eq!(cov.question_mark_count, 2);
    }

    #[test]
    fn test_python_optional() {
        let source =
            "def foo() -> Optional[int]:\n    return None\ndef bar() -> str:\n    return ''";
        let cov = analyze_error_coverage(source, "python");
        assert_eq!(cov.function_count, 2);
        assert_eq!(cov.result_functions, 1);
    }

    #[test]
    fn test_empty_source() {
        let cov = analyze_error_coverage("", "rust");
        assert_eq!(cov.function_count, 0);
        assert_eq!(cov.result_ratio, 0.0);
    }

    #[test]
    fn test_go_error_return() {
        let source = "func Open(name string) (error) {\n}\nfunc Close() {\n}";
        let cov = analyze_error_coverage(source, "go");
        assert_eq!(cov.function_count, 2);
        assert!(cov.result_functions >= 1);
    }
}
