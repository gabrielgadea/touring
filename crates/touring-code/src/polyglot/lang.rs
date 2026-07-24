use ast_grep_language::SupportLang;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;

use super::error::{Error, Result};

/// Touring-facing language enum. Thin wrapper over `SupportLang` so the public
/// API stays stable if ast-grep's enum shape changes upstream.
///
/// **Wave 5 (mossy-crunching-owl, 2026-05-23) S-14 status — `Lang::Md` NOT
/// WIRED**: the Wave 5 plan called for adding a `Markdown` variant routed to
/// `SupportLang::Markdown`. Verification (ast-grep-language 0.42.3
/// `SupportLang::all_langs()`) confirmed the upstream enum exposes 27
/// variants but **none is Markdown** — even though `tree-sitter-md 0.5.3`
/// (ABI 15) is now in the workspace via direct dependency. Wiring `Lang::Md`
/// would require either (a) forking ast-grep-language, or (b) introducing a
/// non-ast-grep parse path that violates the "thin wrapper" architecture
/// stated above. Deferred to a future wave when upstream exposes Markdown,
/// tracked at `lesson:wave-5-mossy-crunching-owl-S9-S13-S14-closure:2026-05-23`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lang {
    /// JavaScript source.
    JavaScript,
    /// TypeScript source (non-JSX).
    TypeScript,
    /// TypeScript with JSX (`.tsx`).
    Tsx,
    /// Python source.
    Python,
    /// Go source.
    Go,
    /// Rust source.
    Rust,
    /// Java source.
    Java,
    /// C source.
    C,
    /// C++ source.
    Cpp,
    /// C# source.
    CSharp,
    /// Ruby source.
    Ruby,
    /// PHP source.
    Php,
    /// Kotlin source.
    Kotlin,
    /// Swift source.
    Swift,
    /// Scala source.
    Scala,
    /// Lua source.
    Lua,
    /// Shell / Bash source.
    Bash,
    /// HTML markup.
    Html,
    /// CSS stylesheets.
    Css,
    /// JSON data.
    Json,
    /// YAML data.
    Yaml,
    /// P1.3 CEG — Elixir grammar is bundled by ast-grep-language 0.36 via
    /// tree-sitter-elixir. Added to support the CEG forbidden-call scanner.
    Elixir,
}

impl Lang {
    /// Map this `Lang` to the corresponding ast-grep `SupportLang`.
    pub fn as_ast_grep(self) -> SupportLang {
        match self {
            Lang::JavaScript => SupportLang::JavaScript,
            Lang::TypeScript => SupportLang::TypeScript,
            Lang::Tsx => SupportLang::Tsx,
            Lang::Python => SupportLang::Python,
            Lang::Go => SupportLang::Go,
            Lang::Rust => SupportLang::Rust,
            Lang::Java => SupportLang::Java,
            Lang::C => SupportLang::C,
            Lang::Cpp => SupportLang::Cpp,
            Lang::CSharp => SupportLang::CSharp,
            Lang::Ruby => SupportLang::Ruby,
            Lang::Php => SupportLang::Php,
            Lang::Kotlin => SupportLang::Kotlin,
            Lang::Swift => SupportLang::Swift,
            Lang::Scala => SupportLang::Scala,
            Lang::Lua => SupportLang::Lua,
            Lang::Bash => SupportLang::Bash,
            Lang::Html => SupportLang::Html,
            Lang::Css => SupportLang::Css,
            Lang::Json => SupportLang::Json,
            Lang::Yaml => SupportLang::Yaml,
            Lang::Elixir => SupportLang::Elixir,
        }
    }

    /// Return the canonical lowercase name of this language (e.g. `"rust"`).
    pub fn name(self) -> &'static str {
        match self {
            Lang::JavaScript => "javascript",
            Lang::TypeScript => "typescript",
            Lang::Tsx => "tsx",
            Lang::Python => "python",
            Lang::Go => "go",
            Lang::Rust => "rust",
            Lang::Java => "java",
            Lang::C => "c",
            Lang::Cpp => "cpp",
            Lang::CSharp => "csharp",
            Lang::Ruby => "ruby",
            Lang::Php => "php",
            Lang::Kotlin => "kotlin",
            Lang::Swift => "swift",
            Lang::Scala => "scala",
            Lang::Lua => "lua",
            Lang::Bash => "bash",
            Lang::Html => "html",
            Lang::Css => "css",
            Lang::Json => "json",
            Lang::Yaml => "yaml",
            Lang::Elixir => "elixir",
        }
    }
}

impl FromStr for Lang {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let lang = match s.to_ascii_lowercase().as_str() {
            "js" | "javascript" => Lang::JavaScript,
            "ts" | "typescript" => Lang::TypeScript,
            "tsx" => Lang::Tsx,
            "py" | "python" => Lang::Python,
            "go" => Lang::Go,
            "rs" | "rust" => Lang::Rust,
            "java" => Lang::Java,
            "c" => Lang::C,
            "cpp" | "c++" | "cc" | "cxx" => Lang::Cpp,
            "cs" | "csharp" | "c#" => Lang::CSharp,
            "rb" | "ruby" => Lang::Ruby,
            "php" => Lang::Php,
            "kt" | "kotlin" => Lang::Kotlin,
            "swift" => Lang::Swift,
            "scala" => Lang::Scala,
            "lua" => Lang::Lua,
            "bash" | "sh" => Lang::Bash,
            "html" | "htm" => Lang::Html,
            "css" => Lang::Css,
            "json" => Lang::Json,
            "yaml" | "yml" => Lang::Yaml,
            "elixir" | "ex" | "exs" => Lang::Elixir,
            other => return Err(Error::UnknownLang(other.to_string())),
        };
        Ok(lang)
    }
}

/// Auto-detect a language from a file path's extension. Returns `None` if the
/// extension is missing or not mapped. Callers should fall back to explicit
/// `--lang` when `None`.
pub fn detect_lang(path: impl AsRef<Path>) -> Option<Lang> {
    let ext = path.as_ref().extension().and_then(|e| e.to_str())?;
    Lang::from_str(ext).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_from_extension() {
        assert_eq!(detect_lang("foo.py"), Some(Lang::Python));
        assert_eq!(detect_lang("foo.ts"), Some(Lang::TypeScript));
        assert_eq!(detect_lang("foo.tsx"), Some(Lang::Tsx));
        assert_eq!(detect_lang("foo.go"), Some(Lang::Go));
        assert_eq!(detect_lang("dir/sub/bar.js"), Some(Lang::JavaScript));
        assert_eq!(detect_lang("no_ext"), None);
        assert_eq!(detect_lang("foo.unknown"), None);
    }

    #[test]
    fn from_str_aliases() {
        assert_eq!(Lang::from_str("js").unwrap(), Lang::JavaScript);
        assert_eq!(Lang::from_str("JAVASCRIPT").unwrap(), Lang::JavaScript);
        assert_eq!(Lang::from_str("py").unwrap(), Lang::Python);
        assert!(Lang::from_str("cobol").is_err());
    }
}
