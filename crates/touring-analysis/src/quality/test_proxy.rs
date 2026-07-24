//! Test coverage proxy estimation.
//!
//! Uses heuristics (test file count, #[test] density, cfg(test) presence)
//! as a proxy for actual test coverage.

use memchr::memmem;

/// Test coverage proxy result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TestProxy {
    /// Number of `#[test]` annotations found.
    pub test_count: usize,
    /// Number of `#[cfg(test)]` blocks found.
    pub cfg_test_count: usize,
    /// Whether this file is itself a test file.
    pub is_test_file: bool,
    /// Proxy score (0.0–1.0).
    pub score: f64,
}

/// Check if a file path indicates a test file.
pub fn is_test_file(path: &str) -> bool {
    let p = path.to_lowercase();
    p.contains("/tests/")
        || p.contains("/test/")
        || p.starts_with("tests/")
        || p.starts_with("test/")
        || p.contains("_test.")
        || p.contains("test_")
        || p.contains(".test.")
        || p.contains(".spec.")
}

/// Analyze test coverage proxy for a source file.
pub fn analyze_test_proxy(source: &str, file_path: &str) -> TestProxy {
    let bytes = source.as_bytes();

    let test_count = memmem::find_iter(bytes, b"#[test]").count()
        + memmem::find_iter(bytes, b"#[tokio::test]").count();

    let cfg_test_count = memmem::find_iter(bytes, b"#[cfg(test)]").count();

    let is_test = is_test_file(file_path);

    // Score: test files get 1.0, files with tests get 0.8, files with cfg(test) get 0.6
    let score = if is_test {
        1.0
    } else if test_count > 0 {
        0.8
    } else if cfg_test_count > 0 {
        0.6
    } else {
        0.0
    };

    TestProxy {
        test_count,
        cfg_test_count,
        is_test_file: is_test,
        score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_test_file_patterns() {
        assert!(is_test_file("tests/integration.rs"));
        assert!(is_test_file("src/foo_test.rs"));
        assert!(is_test_file("src/test_foo.rs"));
        assert!(is_test_file("src/foo.test.ts"));
        assert!(is_test_file("src/foo.spec.js"));
        assert!(!is_test_file("src/main.rs"));
        assert!(!is_test_file("src/lib.rs"));
    }

    #[test]
    fn test_file_with_tests() {
        let source = "#[test]\nfn test_foo() {}\n#[test]\nfn test_bar() {}";
        let proxy = analyze_test_proxy(source, "src/lib.rs");
        assert_eq!(proxy.test_count, 2);
        assert!(!proxy.is_test_file);
        assert!((proxy.score - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_file_without_tests() {
        let source = "fn main() {}";
        let proxy = analyze_test_proxy(source, "src/main.rs");
        assert_eq!(proxy.test_count, 0);
        assert_eq!(proxy.score, 0.0);
    }

    #[test]
    fn test_dedicated_test_file() {
        let source = "#[test]\nfn test_it() {}";
        let proxy = analyze_test_proxy(source, "tests/integration.rs");
        assert!(proxy.is_test_file);
        assert!((proxy.score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn helper() {} }";
        let proxy = analyze_test_proxy(source, "src/lib.rs");
        assert_eq!(proxy.cfg_test_count, 1);
        assert!((proxy.score - 0.6).abs() < f64::EPSILON);
    }
}
