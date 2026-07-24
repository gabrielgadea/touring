//! Canonical language detection from file path extension.
//!
//! Replaces 5 duplicated implementations across pre_edit, pre_edit_prevention,
//! pre_write, post_write, and post_read.

/// Detect programming language from a file extension.
///
/// Returns `Some("language_name")` for known extensions, `None` for unknown.
/// This is the single canonical implementation — all hooks should call this.
pub fn detect_language(file_path: &str) -> Option<&'static str> {
    let ext = file_path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rust"),
        "py" | "pyi" | "pyw" => Some("python"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        "java" => Some("java"),
        "rb" => Some("ruby"),
        "sh" | "bash" | "zsh" => Some("shell"),
        "sql" => Some("sql"),
        "md" | "markdown" => Some("markdown"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "json" => Some("json"),
        "html" => Some("html"),
        "css" => Some("css"),
        _ => None,
    }
}

/// Whether the detected language is a "code" language (has functions, classes,
/// imports — as opposed to data/markup like JSON, YAML, TOML, Markdown, HTML, CSS).
///
/// Used by the wiring system to skip registration of data-file keys as pub symbols,
/// which would otherwise generate massive false-positive orphan counts.
pub fn is_code_language(language: &str) -> bool {
    matches!(
        language,
        "rust"
            | "python"
            | "typescript"
            | "javascript"
            | "go"
            | "c"
            | "cpp"
            | "java"
            | "ruby"
            | "shell"
            | "sql"
    )
}

/// Whether the file at `path` is a code file (by extension).
///
/// Convenience wrapper: `detect_language(path).is_some_and(is_code_language)`.
pub fn is_code_file_ext(path: &str) -> bool {
    detect_language(path).is_some_and(is_code_language)
}

/// Convenience: returns `"unknown"` instead of `None` for callers that need a
/// non-optional string (e.g. `speculate_v2`, antipattern checks).
pub fn detect_language_or_unknown(file_path: &str) -> &'static str {
    detect_language(file_path).unwrap_or("unknown")
}

/// Convenience: returns an owned `String` for callers that need `String`
/// (e.g. `post_read::detect_language` returned `String`).
///
/// Falls back to the raw extension (or `"unknown"`) so that every file path
/// produces a non-empty language tag for knowledge DB storage.
pub fn detect_language_owned(file_path: &str) -> String {
    if let Some(lang) = detect_language(file_path) {
        return lang.to_string();
    }
    // Fallback: use the raw extension (mirrors old post_read behaviour).
    std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_languages() {
        assert_eq!(detect_language("src/main.rs"), Some("rust"));
        assert_eq!(detect_language("app.py"), Some("python"));
        assert_eq!(detect_language("stubs.pyi"), Some("python"));
        assert_eq!(detect_language("win.pyw"), Some("python"));
        assert_eq!(detect_language("app.ts"), Some("typescript"));
        assert_eq!(detect_language("app.tsx"), Some("typescript"));
        assert_eq!(detect_language("index.js"), Some("javascript"));
        assert_eq!(detect_language("index.jsx"), Some("javascript"));
        assert_eq!(detect_language("index.mjs"), Some("javascript"));
        assert_eq!(detect_language("index.cjs"), Some("javascript"));
        assert_eq!(detect_language("main.go"), Some("go"));
        assert_eq!(detect_language("main.c"), Some("c"));
        assert_eq!(detect_language("header.h"), Some("c"));
        assert_eq!(detect_language("main.cpp"), Some("cpp"));
        assert_eq!(detect_language("main.cc"), Some("cpp"));
        assert_eq!(detect_language("main.cxx"), Some("cpp"));
        assert_eq!(detect_language("header.hpp"), Some("cpp"));
        assert_eq!(detect_language("Main.java"), Some("java"));
        assert_eq!(detect_language("app.rb"), Some("ruby"));
        assert_eq!(detect_language("run.sh"), Some("shell"));
        assert_eq!(detect_language("run.bash"), Some("shell"));
        assert_eq!(detect_language("run.zsh"), Some("shell"));
        assert_eq!(detect_language("query.sql"), Some("sql"));
        assert_eq!(detect_language("README.md"), Some("markdown"));
        assert_eq!(detect_language("doc.markdown"), Some("markdown"));
        assert_eq!(detect_language("Cargo.toml"), Some("toml"));
        assert_eq!(detect_language("config.yaml"), Some("yaml"));
        assert_eq!(detect_language("config.yml"), Some("yaml"));
        assert_eq!(detect_language("package.json"), Some("json"));
        assert_eq!(detect_language("index.html"), Some("html"));
        assert_eq!(detect_language("style.css"), Some("css"));
    }

    #[test]
    fn test_unknown_extensions() {
        assert_eq!(detect_language("Makefile"), None);
        assert_eq!(detect_language("noext"), None);
        assert_eq!(detect_language(""), None);
    }

    #[test]
    fn test_detect_language_or_unknown() {
        assert_eq!(detect_language_or_unknown("main.rs"), "rust");
        assert_eq!(detect_language_or_unknown("Makefile"), "unknown");
        assert_eq!(detect_language_or_unknown(""), "unknown");
    }

    #[test]
    fn test_detect_language_owned() {
        assert_eq!(detect_language_owned("main.rs"), "rust");
        assert_eq!(detect_language_owned("main.xyz"), "xyz");
        assert_eq!(detect_language_owned("noext"), "unknown");
    }

    #[test]
    fn test_is_code_language_accepts_code() {
        for lang in [
            "rust",
            "python",
            "typescript",
            "javascript",
            "go",
            "c",
            "cpp",
            "java",
            "ruby",
            "shell",
            "sql",
        ] {
            assert!(
                is_code_language(lang),
                "expected {lang} to be a code language"
            );
        }
    }

    #[test]
    fn test_is_code_language_rejects_data_and_markup() {
        for lang in ["json", "toml", "yaml", "markdown", "html", "css", "unknown"] {
            assert!(
                !is_code_language(lang),
                "expected {lang} NOT to be a code language"
            );
        }
    }

    #[test]
    fn test_is_code_file_ext() {
        // Code files
        assert!(is_code_file_ext("src/main.rs"));
        assert!(is_code_file_ext("app.py"));
        assert!(is_code_file_ext("index.ts"));
        assert!(is_code_file_ext("main.go"));
        assert!(is_code_file_ext("Main.java"));
        // Non-code files
        assert!(!is_code_file_ext("Cargo.toml"));
        assert!(!is_code_file_ext("package.json"));
        assert!(!is_code_file_ext("config.yaml"));
        assert!(!is_code_file_ext("README.md"));
        assert!(!is_code_file_ext("index.html"));
        assert!(!is_code_file_ext("style.css"));
        // Unknown
        assert!(!is_code_file_ext("Makefile"));
        assert!(!is_code_file_ext(""));
    }
}
