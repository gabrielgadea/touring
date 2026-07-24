//! Language detection and configuration
//!
//! Supports 11 languages via tree-sitter grammars (always available):
//! - **Code**: Python, Rust, TypeScript, JavaScript, Bash
//! - **Markup**: HTML, CSS, Markdown
//! - **Data**: JSON, TOML, YAML
//!
//! With the `more-languages` feature enabled, additional languages are supported:
//! - **Code**: Go, Java

use std::path::Path;

/// Supported programming languages.
///
/// Wave 5: `strum::EnumIter` gives consumers `Lang::iter()` — a compile-time
/// enumeration of every supported language, used by touring-server /
/// touring-analysis / touring-python to avoid hand-maintained "all langs"
/// lists. `Display`, `FromStr`, and `as_str` remain manually implemented
/// below because they carry alias-handling logic (e.g. "py" → Python,
/// "ts" → TypeScript) that strum's lowercase-only derives do not cover.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, strum::EnumIter,
)]
pub enum Lang {
    // Code languages (always available)
    /// The Python programming language (`.py`, `.pyi`).
    Python,
    /// The Rust programming language (`.rs`).
    Rust,
    /// The TypeScript programming language (`.ts`, `.tsx`).
    TypeScript,
    /// The JavaScript programming language (`.js`, `.jsx`, `.mjs`, `.cjs`).
    JavaScript,
    /// Shell scripts (`.sh`, `.bash`, `.zsh`).
    Bash,
    // Markup languages
    /// HTML markup (`.html`, `.htm`).
    Html,
    /// CSS stylesheets (`.css`).
    Css,
    /// Markdown documents (`.md`).
    Markdown,
    // Data languages
    /// JSON data (`.json`).
    Json,
    /// TOML configuration (`.toml`).
    Toml,
    /// YAML data (`.yaml`, `.yml`).
    Yaml,
    // Additional languages (feature = "more-languages")
    /// The Go programming language (`.go`), gated behind the `more-languages` feature.
    #[cfg(feature = "more-languages")]
    Go,
    /// The Java programming language (`.java`), gated behind the `more-languages` feature.
    #[cfg(feature = "more-languages")]
    Java,
}

impl Lang {
    /// Detect language from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "py" | "pyi" => Some(Lang::Python),
            "rs" => Some(Lang::Rust),
            "ts" | "tsx" => Some(Lang::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Lang::JavaScript),
            "sh" | "bash" | "zsh" => Some(Lang::Bash),
            "html" | "htm" => Some(Lang::Html),
            "css" | "scss" => Some(Lang::Css),
            "md" | "mdx" => Some(Lang::Markdown),
            "json" | "jsonc" => Some(Lang::Json),
            "toml" => Some(Lang::Toml),
            "yaml" | "yml" => Some(Lang::Yaml),
            #[cfg(feature = "more-languages")]
            "go" => Some(Lang::Go),
            #[cfg(feature = "more-languages")]
            "java" => Some(Lang::Java),
            _ => None,
        }
    }

    /// Get the tree-sitter language object
    pub(crate) fn tree_sitter_language(&self) -> tree_sitter::Language {
        match self {
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::Bash => tree_sitter_bash::LANGUAGE.into(),
            Lang::Html => tree_sitter_html::LANGUAGE.into(),
            Lang::Css => tree_sitter_css::LANGUAGE.into(),
            Lang::Json => tree_sitter_json::LANGUAGE.into(),
            Lang::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            Lang::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Lang::Markdown => tree_sitter_md::LANGUAGE.into(),
            #[cfg(feature = "more-languages")]
            Lang::Go => tree_sitter_go::LANGUAGE.into(),
            #[cfg(feature = "more-languages")]
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
        }
    }

    /// Get the query file for this language (symbol extraction)
    pub(crate) fn query_file(&self) -> &'static str {
        match self {
            Lang::Python => include_str!("queries/python.scm"),
            Lang::Rust => include_str!("queries/rust.scm"),
            Lang::TypeScript => include_str!("queries/typescript.scm"),
            Lang::JavaScript => include_str!("queries/javascript.scm"),
            Lang::Bash => include_str!("queries/bash.scm"),
            Lang::Html => include_str!("queries/html.scm"),
            Lang::Css => include_str!("queries/css.scm"),
            Lang::Markdown => include_str!("queries/markdown.scm"),
            Lang::Json => include_str!("queries/json.scm"),
            Lang::Toml => include_str!("queries/toml.scm"),
            Lang::Yaml => include_str!("queries/yaml.scm"),
            #[cfg(feature = "more-languages")]
            Lang::Go => include_str!("queries/go.scm"),
            #[cfg(feature = "more-languages")]
            Lang::Java => include_str!("queries/java.scm"),
        }
    }

    /// Get the import query for this language (for dependency extraction).
    /// Returns empty string for languages without import semantics.
    pub(crate) fn import_query_file(&self) -> &'static str {
        match self {
            Lang::Python => include_str!("queries/python_imports.scm"),
            Lang::Rust => include_str!("queries/rust_imports.scm"),
            Lang::TypeScript | Lang::JavaScript => include_str!("queries/ts_imports.scm"),
            // No import semantics for these languages
            Lang::Bash
            | Lang::Html
            | Lang::Css
            | Lang::Markdown
            | Lang::Json
            | Lang::Toml
            | Lang::Yaml => "",
            #[cfg(feature = "more-languages")]
            Lang::Go => include_str!("queries/go_imports.scm"),
            #[cfg(feature = "more-languages")]
            Lang::Java => include_str!("queries/java_imports.scm"),
        }
    }

    /// Get the method-call query for this language (for dynamic-dispatch
    /// wiring beyond `use`-statement imports).
    ///
    /// Returns empty string for languages whose call-site wiring is already
    /// captured by `import_query_file` (Python's `from x import y` covers
    /// both type-level and function-level edges).
    ///
    /// Only Rust returns a non-empty query today — `.method()` and
    /// `Type::assoc_fn()` are syntactically invisible to `use`-statement
    /// scraping and accounted for ~43% of the orphan-count noise pre-F9.
    pub(crate) fn method_call_query_file(&self) -> &'static str {
        match self {
            Lang::Rust => include_str!("queries/rust_method_calls.scm"),
            _ => "",
        }
    }

    /// Get language name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Lang::Python => "python",
            Lang::Rust => "rust",
            Lang::TypeScript => "typescript",
            Lang::JavaScript => "javascript",
            Lang::Bash => "bash",
            Lang::Html => "html",
            Lang::Css => "css",
            Lang::Markdown => "markdown",
            Lang::Json => "json",
            Lang::Toml => "toml",
            Lang::Yaml => "yaml",
            #[cfg(feature = "more-languages")]
            Lang::Go => "go",
            #[cfg(feature = "more-languages")]
            Lang::Java => "java",
        }
    }

    /// Whether this language is a code language (has functions, classes, etc.)
    pub fn is_code(&self) -> bool {
        #[cfg(not(feature = "more-languages"))]
        return matches!(
            self,
            Lang::Python | Lang::Rust | Lang::TypeScript | Lang::JavaScript | Lang::Bash
        );
        #[cfg(feature = "more-languages")]
        matches!(
            self,
            Lang::Python
                | Lang::Rust
                | Lang::TypeScript
                | Lang::JavaScript
                | Lang::Bash
                | Lang::Go
                | Lang::Java
        )
    }

    /// Whether this language is a markup language
    pub fn is_markup(&self) -> bool {
        matches!(self, Lang::Html | Lang::Css | Lang::Markdown)
    }

    /// Whether this language is a data/config language
    pub fn is_data(&self) -> bool {
        matches!(self, Lang::Json | Lang::Toml | Lang::Yaml)
    }
}

impl std::str::FromStr for Lang {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Lang::Python),
            "rust" | "rs" => Ok(Lang::Rust),
            "typescript" | "ts" => Ok(Lang::TypeScript),
            "javascript" | "js" => Ok(Lang::JavaScript),
            "bash" | "sh" | "shell" | "zsh" => Ok(Lang::Bash),
            "html" | "htm" => Ok(Lang::Html),
            "css" | "scss" => Ok(Lang::Css),
            "markdown" | "md" => Ok(Lang::Markdown),
            "json" | "jsonc" => Ok(Lang::Json),
            "toml" => Ok(Lang::Toml),
            "yaml" | "yml" => Ok(Lang::Yaml),
            #[cfg(feature = "more-languages")]
            "go" => Ok(Lang::Go),
            #[cfg(feature = "more-languages")]
            "java" => Ok(Lang::Java),
            _ => Err(format!("Unknown language: {}", s)),
        }
    }
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_python() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("test.py")),
            Some(Lang::Python)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("test.pyi")),
            Some(Lang::Python)
        );
    }

    /// Wave 5: `Lang::iter()` courtesy of `strum::EnumIter`. Consumers
    /// (touring-server, touring-analysis, touring-python) can now iterate
    /// the supported languages without maintaining a parallel `all()` list.
    #[test]
    fn test_lang_iter_contains_every_variant() {
        use strum::IntoEnumIterator;
        let all: Vec<Lang> = Lang::iter().collect();
        // 11 base languages; +2 Go/Java when `more-languages` is enabled
        // (the default feature set turns this on).
        #[cfg(feature = "more-languages")]
        assert_eq!(all.len(), 13, "expected 13 langs with more-languages");
        #[cfg(not(feature = "more-languages"))]
        assert_eq!(all.len(), 11, "expected 11 langs without more-languages");
        // Invariant: iter yields unique variants.
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(
            unique.len(),
            all.len(),
            "Lang::iter must yield unique variants"
        );
        // Invariant: every variant round-trips through `as_str` ↔ `from_str`.
        for lang in all {
            let s = lang.as_str();
            let parsed: Lang = s
                .parse()
                .unwrap_or_else(|e| panic!("Lang::from_str({s:?}) failed: {e}"));
            assert_eq!(parsed, lang, "round-trip mismatch for {lang:?}");
        }
    }

    #[test]
    fn test_detect_rust() {
        assert_eq!(Lang::from_path(&PathBuf::from("test.rs")), Some(Lang::Rust));
    }

    #[test]
    fn test_detect_typescript() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("test.ts")),
            Some(Lang::TypeScript)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("test.tsx")),
            Some(Lang::TypeScript)
        );
    }

    #[test]
    fn test_detect_javascript() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("test.js")),
            Some(Lang::JavaScript)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("test.jsx")),
            Some(Lang::JavaScript)
        );
    }

    #[test]
    fn test_detect_bash() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("deploy.sh")),
            Some(Lang::Bash)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("setup.bash")),
            Some(Lang::Bash)
        );
    }

    #[test]
    fn test_detect_html() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("index.html")),
            Some(Lang::Html)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("page.htm")),
            Some(Lang::Html)
        );
    }

    #[test]
    fn test_detect_css() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("style.css")),
            Some(Lang::Css)
        );
    }

    #[test]
    fn test_detect_markdown() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("README.md")),
            Some(Lang::Markdown)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("docs.mdx")),
            Some(Lang::Markdown)
        );
    }

    #[test]
    fn test_detect_json() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("package.json")),
            Some(Lang::Json)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("tsconfig.jsonc")),
            Some(Lang::Json)
        );
    }

    #[test]
    fn test_detect_toml() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("Cargo.toml")),
            Some(Lang::Toml)
        );
    }

    #[test]
    fn test_detect_yaml() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("config.yaml")),
            Some(Lang::Yaml)
        );
        assert_eq!(
            Lang::from_path(&PathBuf::from("config.yml")),
            Some(Lang::Yaml)
        );
    }

    #[test]
    fn test_unknown_extension() {
        assert_eq!(Lang::from_path(&PathBuf::from("file.xyz")), None);
        assert_eq!(Lang::from_path(&PathBuf::from("noext")), None);
    }

    #[test]
    fn test_from_str() {
        assert_eq!("python".parse::<Lang>().unwrap(), Lang::Python);
        assert_eq!("rust".parse::<Lang>().unwrap(), Lang::Rust);
        assert_eq!("typescript".parse::<Lang>().unwrap(), Lang::TypeScript);
        assert_eq!("javascript".parse::<Lang>().unwrap(), Lang::JavaScript);
        assert_eq!("bash".parse::<Lang>().unwrap(), Lang::Bash);
        assert_eq!("html".parse::<Lang>().unwrap(), Lang::Html);
        assert_eq!("css".parse::<Lang>().unwrap(), Lang::Css);
        assert_eq!("markdown".parse::<Lang>().unwrap(), Lang::Markdown);
        assert_eq!("json".parse::<Lang>().unwrap(), Lang::Json);
        assert_eq!("toml".parse::<Lang>().unwrap(), Lang::Toml);
        assert_eq!("yaml".parse::<Lang>().unwrap(), Lang::Yaml);
    }

    #[test]
    fn test_language_categories() {
        assert!(Lang::Python.is_code());
        assert!(Lang::Bash.is_code());
        assert!(!Lang::Html.is_code());

        assert!(Lang::Html.is_markup());
        assert!(Lang::Css.is_markup());
        assert!(!Lang::Python.is_markup());

        assert!(Lang::Json.is_data());
        assert!(Lang::Toml.is_data());
        assert!(!Lang::Python.is_data());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Lang::Python), "python");
        assert_eq!(format!("{}", Lang::Toml), "toml");
    }

    #[cfg(feature = "more-languages")]
    #[test]
    fn test_detect_go() {
        assert_eq!(Lang::from_path(&PathBuf::from("main.go")), Some(Lang::Go));
    }

    #[cfg(feature = "more-languages")]
    #[test]
    fn test_detect_java() {
        assert_eq!(
            Lang::from_path(&PathBuf::from("Main.java")),
            Some(Lang::Java)
        );
    }

    #[cfg(feature = "more-languages")]
    #[test]
    fn test_from_str_more_languages() {
        assert_eq!("go".parse::<Lang>().unwrap(), Lang::Go);
        assert_eq!("java".parse::<Lang>().unwrap(), Lang::Java);
    }
}
