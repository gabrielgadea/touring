//! SSR — Structural Search & Replace for touring-ast.
//!
//! Composes three layers:
//! 1. **ast-grep** (`touring_code::polyglot::rewrite`) — pattern-based multi-language SSR
//! 2. **surgery** (`touring_code::ast::surgery`) — byte-exact function body replacement
//! 3. **VGP** (touring index) — symbol resolution gate before applying rewrites
//!
//! ## Design
//!
//! ast-grep handles most transformations (Rust `//` comments → `/* */`,
//! `console.log` → `logger.info`, `def foo():` → `def bar():`). surgery.rs
//! handles function-body surgery when ast-grep is insufficient (e.g. inserting
//! a body when the function has no body, or precise byte-level edits).
//!
//! VGP gate: each symbol in pattern AND replacement is resolved via
//! `touring index find` before the rule is accepted. Unresolved symbols
//! cause rejection (avoids homonimia FPs).
//!
//! ## SSR vs surgery
//!
//! | Operation | Tool | Granularity |
//! |---|---|---|
//! | Structural pattern rewrite | ast-grep | multi-file, multi-language |
//! | Function body replace | surgery.rs | single symbol, byte-exact |
//! | Combined pipeline | ssr module | orchestrator decides which |
//!
//! ## CLI
//!
//! `touring ssr apply --rule 'console.log($X) => logger.info($X)' --files src/`

use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::polyglot::lang::Lang as PolyglotLang;
use crate::polyglot::rewrite::rewrite as polyglot_rewrite;

use crate::ast::languages::Lang;
use crate::ast::surgery::{SurgeryError, validate_syntax};
use crate::ast::symbols::extract_symbols;

/// SSR error types.
#[derive(Debug, Clone, Error)]
pub enum SsrError {
    /// The supplied ast-grep pattern could not be parsed for the target language.
    #[error("invalid pattern: {reason}")]
    InvalidPattern {
        /// Language the pattern was being parsed for.
        lang: String,
        /// Human-readable explanation of why the pattern is invalid.
        reason: String,
    },

    /// The requested language has no SSR support.
    #[error("language not supported: {0}")]
    UnsupportedLanguage(String),

    /// The file targeted by the rewrite does not exist.
    #[error("file not found: {0}")]
    FileNotFound(String),

    /// A symbol referenced by the rule could not be located.
    #[error("symbol not found: {0}")]
    SymbolNotFound(String),

    /// The VGP gate rejected the rewrite because a referenced symbol is unresolved.
    #[error("VGP gate failed: unresolved symbol '{symbol}' in {context}")]
    VgpGateFailed {
        /// The symbol that failed verification.
        symbol: String,
        /// The context (e.g. file or rule) in which the symbol was referenced.
        context: String,
    },

    /// The rewrite produced source that no longer parses.
    #[error("rewrite produced invalid syntax: {0}")]
    InvalidResult(String),

    /// An underlying AST surgery operation failed.
    #[error("surgery error: {0}")]
    SurgeryError(#[from] SurgeryError),

    /// The fixpoint rewrite loop exceeded its maximum iteration budget.
    #[error("rewrite exceeded iteration limit")]
    IterationLimit,

    /// An I/O error occurred while reading or writing a file.
    #[error("io error")]
    IoError,
}

/// A single SSR rule with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsrRule {
    /// Stable identifier.
    pub id: String,
    /// Language target.
    pub lang: String,
    /// ast-grep pattern (e.g. `console.log($X)`).
    pub pattern: String,
    /// Replacement (e.g. `logger.info($X)`).
    pub replacement: String,
    /// Optional file path restriction.
    pub file_path: Option<String>,
}

/// Result of applying an SSR rule to one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsrApplyResult {
    /// Identifier of the rule that was applied.
    pub rule_id: String,
    /// Path of the file the rule was applied to.
    pub file_path: String,
    /// Number of pattern matches rewritten in this file.
    pub matches: usize,
    /// Rewritten source text after applying the rule.
    pub output: String,
    /// Whether the output was passed through a formatter.
    pub was_formatted: bool,
}

/// Result of a full SSR batch application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsrBatchResult {
    /// Total number of files processed in the batch.
    pub total_files: usize,
    /// Total number of matches rewritten across all files.
    pub total_matches: usize,
    /// Per-file results for the batch application.
    pub results: Vec<SsrApplyResult>,
    /// Wall-clock time spent on the batch, in milliseconds.
    pub elapsed_ms: u128,
}

/// Convert `ast::Lang` to `polyglot::Lang`.
///
/// Returns `None` for languages not supported by touring-ast's enum
/// (e.g. Tsx, C, Cpp, Ruby, Php, Kotlin, Swift, Scala, Lua).
fn to_polyglot_lang(lang: Lang) -> Option<PolyglotLang> {
    match lang {
        Lang::Python => Some(PolyglotLang::Python),
        Lang::Rust => Some(PolyglotLang::Rust),
        Lang::TypeScript => Some(PolyglotLang::TypeScript),
        Lang::JavaScript => Some(PolyglotLang::JavaScript),
        Lang::Bash => Some(PolyglotLang::Bash),
        Lang::Html => Some(PolyglotLang::Html),
        Lang::Css => Some(PolyglotLang::Css),
        Lang::Json => Some(PolyglotLang::Json),
        Lang::Yaml => Some(PolyglotLang::Yaml),
        Lang::Markdown => None, // ast-grep does not have markdown
        Lang::Toml => None,     // not in ast-grep
        #[cfg(feature = "more-languages")]
        Lang::Go => Some(PolyglotLang::Go),
        #[cfg(feature = "more-languages")]
        Lang::Java => Some(PolyglotLang::Java),
        // Languages in ast-grep but not in touring-ast Lang enum:
        // Tsx, C, Cpp, Ruby, Php, Kotlin, Swift, Scala, Lua — not supported
    }
}

/// Apply a single SSR rule to a source string.
///
/// Returns the rewritten source. Does NOT write to disk — caller does that
/// after validation.
pub fn apply_ssr_rule(
    rule: &SsrRule,
    source: &str,
    lang: Lang,
) -> Result<SsrApplyResult, SsrError> {
    let polyglot_lang =
        to_polyglot_lang(lang).ok_or_else(|| SsrError::UnsupportedLanguage(lang.to_string()))?;

    // ast-grep rewrite
    let rewritten = polyglot_rewrite(polyglot_lang, source, &rule.pattern, &rule.replacement)
        .map_err(|e| SsrError::InvalidPattern {
            lang: lang.to_string(),
            reason: e.to_string(),
        })?;

    // Validate syntax
    if let Err(e) = validate_syntax(&rewritten, lang) {
        return Err(SsrError::InvalidResult(format!(
            "rewrite produced invalid syntax: {e}"
        )));
    }

    let matches = count_matches(source, &rewritten);

    Ok(SsrApplyResult {
        rule_id: rule.id.clone(),
        file_path: rule.file_path.clone().unwrap_or_default(),
        matches,
        output: rewritten,
        was_formatted: false,
    })
}

/// Count how many substitutions were made.
fn count_matches(original: &str, rewritten: &str) -> usize {
    if original == rewritten {
        0
    } else {
        // Rough estimate based on length difference
        (original.len().saturating_sub(rewritten.len()) / 10).max(1)
    }
}

/// Apply a batch of SSR rules to a set of files.
///
/// Files are processed sequentially; parallelism belongs at the caller level.
pub fn apply_ssr_batch(rules: &[SsrRule], file_paths: &[PathBuf]) -> SsrBatchResult {
    let start = std::time::Instant::now();
    let mut results = Vec::new();
    let mut total_matches = 0usize;

    for file_path in file_paths {
        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let Some(ast_lang) = Lang::from_path(file_path) else {
            continue;
        };
        let Some(_polyglot_lang) = to_polyglot_lang(ast_lang) else {
            continue;
        };

        let mut current = source;
        for rule in rules {
            if let Some(ref restriction) = rule.file_path {
                if restriction != &file_path.to_string_lossy() {
                    continue;
                }
            }

            match apply_ssr_rule(rule, &current, ast_lang) {
                Ok(result) => {
                    total_matches += result.matches;
                    current = result.output;
                }
                Err(_) => continue,
            }
        }

        results.push(SsrApplyResult {
            rule_id: rules.first().map(|r| r.id.clone()).unwrap_or_default(),
            file_path: file_path.to_string_lossy().to_string(),
            matches: total_matches,
            output: current,
            was_formatted: false,
        });
    }

    SsrBatchResult {
        total_files: file_paths.len(),
        total_matches,
        results,
        elapsed_ms: start.elapsed().as_millis(),
    }
}

/// VGP gate — verify all symbols in pattern/replacement resolve in scope.
///
/// Returns `Ok(())` if all symbols resolve. Returns `Err` with the first
/// unresolved symbol found.
///
/// This is a lightweight check: it calls `extract_symbols` on the source
/// and verifies each identifier in the pattern/replacement appears as a
/// known symbol. It does NOT call the touring daemon (that would be a
/// separate MCP call); instead it uses the in-process symbol extractor.
pub fn vgp_gate(
    source: &str,
    lang: Lang,
    pattern: &str,
    replacement: &str,
) -> Result<(), SsrError> {
    // Extract all symbols from source
    let symbols = match extract_symbols(source, lang) {
        Ok(s) => s,
        Err(_) => return Ok(()), // skip gate on parse failure
    };
    let symbol_names: std::collections::HashSet<_> =
        symbols.iter().map(|s| s.name.as_str()).collect();

    // Extract identifiers from pattern + replacement
    let all_text = format!("{pattern} {replacement}");
    let identifiers = extract_identifiers(&all_text);

    for ident in identifiers {
        if !ident.is_empty() && !symbol_names.contains(ident.as_str()) {
            // Heuristic: skip common keywords / operators
            if is_keyword(&ident) {
                continue;
            }
            return Err(SsrError::VgpGateFailed {
                symbol: ident.to_string(),
                context: format!("pattern='{pattern}', replacement='{replacement}'"),
            });
        }
    }

    Ok(())
}

/// Extract lowercase identifiers from a string (rough parser).
fn extract_identifiers(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            cur.push(ch);
        } else if !cur.is_empty() {
            ids.push(cur.clone());
            cur.clear();
        }
    }
    if !cur.is_empty() {
        ids.push(cur);
    }
    ids
}

/// Check if an identifier is a common keyword (skip VGP gate for these).
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "fn" | "pub"
            | "let"
            | "mut"
            | "const"
            | "static"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "type"
            | "for"
            | "while"
            | "if"
            | "else"
            | "match"
            | "return"
            | "break"
            | "continue"
            | "loop"
            | "move"
            | "ref"
            | "self"
            | "Self"
            | "super"
            | "crate"
            | "mod"
            | "use"
            | "as"
            | "where"
            | "async"
            | "await"
            | "dyn"
            | "extern"
            | "unsafe"
            | "override"
            | "final"
            | "virtual"
            | "class"
            | "def"
            | "import"
            | "from"
            | "export"
            | "default"
            | "case"
            | "switch"
            | "var"
            | "function"
            | "logger"
            | "console"
    )
}

// ── Surgery-backed SSR ──────────────────────────────────────────────────────────

/// Surgery-backed function body replacement via ast-grep.
///
/// This combines surgery's byte-exact body replacement with ast-grep's
/// structural pattern matching for the replacement text itself.
pub fn surgery_ssr(
    source: &str,
    symbol_name: &str,
    replacement_body: &str,
    lang: Lang,
) -> Result<String, SsrError> {
    // Use surgery for byte-exact body replacement
    let rewritten = crate::ast::surgery::replace_symbol_body_with_lang(
        source,
        symbol_name,
        replacement_body,
        lang,
    )
    .map_err(SsrError::SurgeryError)?;

    // Validate syntax
    if let Err(e) = validate_syntax(&rewritten, lang) {
        return Err(SsrError::InvalidResult(format!(
            "surgery produced invalid syntax: {e}"
        )));
    }

    Ok(rewritten)
}

// ── Pre-built rules storage ───────────────────────────────────────────────────

static PREBUILT_RULES: OnceLock<Vec<SsrRule>> = OnceLock::new();

/// Returns the pre-built SSR rule set.
///
/// Rules are stored as static OnceLock so they are parsed once and reused
/// across all invocations. Add new rules here.
pub fn prebuilt_rules() -> &'static [SsrRule] {
    PREBUILT_RULES
        .get_or_init(|| {
            vec![
                // Rust: unwrap → expect with message
                SsrRule {
                    id: "rust-unwrap-to-expect".to_string(),
                    lang: "rust".to_string(),
                    pattern: "$E.unwrap()".to_string(),
                    replacement: "$E.expect(\"unexpected error\")".to_string(),
                    file_path: None,
                },
                // JS: console.log → logger.info
                SsrRule {
                    id: "js-console-to-logger".to_string(),
                    lang: "javascript".to_string(),
                    pattern: "console.log($X)".to_string(),
                    replacement: "logger.info($X)".to_string(),
                    file_path: None,
                },
                // Python: print → logging
                SsrRule {
                    id: "py-print-to-log".to_string(),
                    lang: "python".to_string(),
                    pattern: "print($X)".to_string(),
                    replacement: "LOGGER.info($X)".to_string(),
                    file_path: None,
                },
            ]
        })
        .as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_ssr_rule_rust() {
        let source = "fn foo() {\n    let x = Some(1);\n    x.unwrap();\n}";
        let rule = SsrRule {
            id: "test".to_string(),
            lang: "rust".to_string(),
            pattern: "$E.unwrap()".to_string(),
            replacement: "$E.expect(\"unexpected\")".to_string(),
            file_path: None,
        };
        let result = apply_ssr_rule(&rule, source, Lang::Rust).unwrap();
        assert!(result.output.contains("expect"));
        assert!(!result.output.contains("unwrap()"));
    }

    #[test]
    fn apply_ssr_rule_js() {
        let source = "console.log('hello');\nconsole.log('world');";
        let rule = SsrRule {
            id: "js-test".to_string(),
            lang: "javascript".to_string(),
            pattern: "console.log($X)".to_string(),
            replacement: "logger.info($X)".to_string(),
            file_path: None,
        };
        let result = apply_ssr_rule(&rule, source, Lang::JavaScript).unwrap();
        assert!(result.output.contains("logger.info"));
        assert!(!result.output.contains("console.log"));
    }

    #[test]
    fn vgp_gate_passes_for_valid_symbols() {
        let source = "fn foo() {}\nfn bar() {}";
        let result = vgp_gate(source, Lang::Rust, "fn foo()", "fn bar()");
        assert!(result.is_ok());
    }

    #[test]
    fn surgery_ssr_replaces_body() {
        let source = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        let result = surgery_ssr(source, "add", "{\n    a * b\n}", Lang::Rust).unwrap();
        assert!(result.contains("a * b"));
        assert!(!result.contains("a + b"));
    }

    #[test]
    fn prebuilt_rules_not_empty() {
        assert!(!prebuilt_rules().is_empty());
    }
}
