//! Unified error type for all touring-ast operations.
//!
//! ## SpanTrace Support (tracing-error integration)
//!
//! Any `AstError` can be upgraded to a [`TracedAstError`] via [`AstError::traced`]
//! or [`AstResultExt::traced`]. The traced variant captures the active
//! `tracing` span stack at the call site — when the daemon logs the error,
//! the span trace reveals which file, symbol, and phase of analysis were
//! active when the failure occurred.
//!
//! Install [`tracing_error::ErrorLayer`] in your tracing subscriber to
//! render `SpanTrace` in logs:
//!
//! ```ignore
//! use tracing_subscriber::prelude::*;
//! tracing_subscriber::registry()
//!     .with(tracing_error::ErrorLayer::default())
//!     .with(tracing_subscriber::fmt::layer())
//!     .init();
//! ```

use miette::Diagnostic;
use tracing_error::SpanTrace;

/// Unified error type for all AST operations.
///
/// Every public function in `touring-ast` returns `Result<T, AstError>`.
/// The enum is `#[non_exhaustive]` so new variants can be added without
/// breaking downstream crates.
///
/// Implements [`miette::Diagnostic`] so the daemon can render a compact,
/// CILA-budget-friendly error with `code`, `help`, and (where applicable)
/// a `url` pointing to the Touring error catalogue.
#[derive(Debug, thiserror::Error, Diagnostic)]
#[non_exhaustive]
pub enum AstError {
    /// Language could not be detected from file extension.
    #[error("unknown language for path: {0}")]
    #[diagnostic(
        code(touring_ast::unknown_language),
        help(
            "supported: rust, python, typescript, javascript, go, java, bash, toml, yaml, json, html, css, markdown"
        )
    )]
    UnknownLanguage(String),

    /// tree-sitter failed to parse the source.
    #[error("parse failed: {0}")]
    #[diagnostic(
        code(touring_ast::parse_failed),
        help(
            "the source could not be parsed by tree-sitter; check for unbalanced delimiters or grammar mismatch"
        )
    )]
    ParseFailed(String),

    /// The tree-sitter grammar for a language could not be loaded — almost
    /// always an ABI version mismatch between the compiled grammar crate and
    /// the `tree-sitter` runtime (`Parser::set_language` rejected it).
    ///
    /// This is an **infrastructure limitation, not a defect in the source**
    /// being analyzed. Callers MUST treat it as "structure could not be
    /// checked", never as "the code is broken" — conflating the two produces
    /// false-positive syntax errors on every edit of an affected language.
    #[error("grammar unavailable: {0}")]
    #[diagnostic(
        code(touring_ast::grammar_unavailable),
        severity(Warning),
        help(
            "the grammar crate's tree-sitter ABI does not match the runtime; this is a tooling limitation — the analyzed source is not at fault. Align the grammar crate version's ABI with the `tree-sitter` runtime in Cargo.toml"
        )
    )]
    GrammarUnavailable(String),

    /// tree-sitter query compilation failed.
    #[error("query compilation failed: {0}")]
    #[diagnostic(
        code(touring_ast::query_error),
        help("verify the tree-sitter query S-expression against the grammar node set")
    )]
    QueryError(String),

    /// Symbol not found during surgery or lookup.
    #[error("symbol not found: {0}")]
    #[diagnostic(
        code(touring_ast::symbol_not_found),
        help("run `touring index find <symbol>` to confirm the symbol exists and is indexed")
    )]
    SymbolNotFound(String),

    /// Byte range is out of bounds for the source buffer.
    #[error("invalid range: {0}")]
    #[diagnostic(
        code(touring_ast::invalid_range),
        help("byte offsets must fall within the source buffer; rebuild indices after edits")
    )]
    InvalidRange(String),

    /// Post-edit syntax validation failed.
    #[error("syntax error after edit: {0}")]
    #[diagnostic(
        code(touring_ast::syntax_error),
        help("the edit produced invalid source; re-run `touring pre-edit` and narrow the scope")
    )]
    SyntaxError(String),

    /// File I/O error.
    #[error("I/O error: {0}")]
    #[diagnostic(code(touring_ast::io))]
    Io(#[from] std::io::Error),

    /// SQLite persistence error.
    #[error("SQLite error: {0}")]
    #[diagnostic(
        code(touring_ast::sqlite),
        help(
            "check `touring doctor -j` for `project_db` status; a corrupt symbols.db needs `touring index rebuild`"
        )
    )]
    Sqlite(#[from] rusqlite::Error),

    /// Internal mutex was poisoned.
    #[error("lock poisoned: {0}")]
    #[diagnostic(
        code(touring_ast::lock_poisoned),
        severity(Warning),
        help("a panic while holding the lock poisoned it; inspect the upstream panic cause")
    )]
    LockPoisoned(String),

    /// Language is not supported for the requested operation.
    #[error("unsupported language: {0}")]
    #[diagnostic(
        code(touring_ast::unsupported_language),
        help(
            "extend `Lang::detect` in crates/touring-ast/src/languages.rs to register the grammar"
        )
    )]
    UnsupportedLanguage(String),
}

/// Convenience alias used across the crate.
pub type AstResult<T> = Result<T, AstError>;

// ── SpanTrace enrichment (tracing-error) ─────────────────────────────

/// An [`AstError`] enriched with a [`SpanTrace`] captured at the point the
/// error crossed the traced boundary.
///
/// Use [`AstError::traced`] or [`AstResultExt::traced`] to upgrade an error.
/// When printed with `{}`, shows the underlying error; with `{:#}` / Debug
/// or through a tracing subscriber with [`tracing_error::ErrorLayer`],
/// shows the full span trace.
///
/// This is **additive**: existing `AstError` callers continue to work
/// unchanged. `TracedAstError` is for logging / debug paths where knowing
/// "which span was active when this failed" matters.
///
/// Wave 5: `From<AstError>` is implemented manually below (derive_more::From
/// requires single-field structs; TracedAstError has two fields because the
/// captured SpanTrace must live alongside the source error).
#[derive(Debug, thiserror::Error)]
#[error("{source}")]
pub struct TracedAstError {
    /// The underlying error.
    #[source]
    pub source: AstError,
    /// Snapshot of the active tracing span stack at capture time.
    pub span_trace: SpanTrace,
}

/// `From<AstError>` — allows `?` to auto-promote untraced errors by
/// capturing the current span stack at conversion time.
impl From<AstError> for TracedAstError {
    fn from(source: AstError) -> Self {
        Self::new(source)
    }
}

/// Delegate [`miette::Diagnostic`] to the inner [`AstError`] — the
/// traced wrapper adds a `SpanTrace` but preserves the source's
/// `code`, `help`, `url`, and `severity` verbatim for CILA output.
impl Diagnostic for TracedAstError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.source.code()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.source.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.source.url()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.source.severity()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        Some(&self.source)
    }
}

impl TracedAstError {
    /// Create a new traced error, capturing the current span stack.
    #[must_use]
    pub fn new(source: AstError) -> Self {
        Self {
            source,
            span_trace: SpanTrace::capture(),
        }
    }
}

impl AstError {
    /// Capture the current tracing span stack into a [`TracedAstError`].
    ///
    /// Call this at error boundaries where logs are emitted so the span
    /// context travels with the error:
    ///
    /// ```ignore
    /// let tree = parse(&src).map_err(|e| e.traced())?;
    /// ```
    #[must_use]
    pub fn traced(self) -> TracedAstError {
        TracedAstError::new(self)
    }
}

/// Extension trait for `Result<T, AstError>` to capture a SpanTrace on
/// the error path with a single `.traced()?` call.
///
/// Adding `use touring_code::ast::AstResultExt;` in scope makes `.traced()` a
/// method on any `Result<T, AstError>`.
pub trait AstResultExt<T> {
    /// Upgrade the error to `TracedAstError` capturing the current span stack.
    fn traced(self) -> Result<T, TracedAstError>;
}

impl<T> AstResultExt<T> for Result<T, AstError> {
    fn traced(self) -> Result<T, TracedAstError> {
        self.map_err(TracedAstError::new)
    }
}

// ── Backward compatibility ──────────────────────────────────────────────

impl From<AstError> for String {
    fn from(e: AstError) -> String {
        e.to_string()
    }
}

impl From<String> for AstError {
    fn from(s: String) -> Self {
        AstError::ParseFailed(s)
    }
}

// ── From SurgeryError (legacy) ──────────────────────────────────────────

impl From<crate::ast::surgery::SurgeryError> for AstError {
    fn from(e: crate::ast::surgery::SurgeryError) -> Self {
        match e {
            crate::ast::surgery::SurgeryError::SymbolNotFound(s) => AstError::SymbolNotFound(s),
            crate::ast::surgery::SurgeryError::ParseError(s) => AstError::ParseFailed(s),
            crate::ast::surgery::SurgeryError::InvalidLanguage(s) => AstError::UnknownLanguage(s),
            crate::ast::surgery::SurgeryError::InvalidRange(s) => AstError::InvalidRange(s),
            crate::ast::surgery::SurgeryError::SyntaxError(s) => AstError::SyntaxError(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Tier A (2026-04-19): named import of the pretty_assertions
    // shadow-macros from the crate-wide test prelude. Named (not glob)
    // so Rust's macro resolver picks these over the std prelude without
    // raising E0659 (ambiguous macro import).
    use crate::ast::test_util::{assert_eq, assert_ne};

    #[test]
    fn traced_preserves_display_message() {
        let err = AstError::ParseFailed("boom".to_string());
        let traced = err.traced();
        assert_eq!(traced.to_string(), "parse failed: boom");
    }

    #[test]
    fn traced_wraps_distinct_variants_without_collision() {
        let parse = AstError::ParseFailed("a".to_string()).traced();
        let io = AstError::ParseFailed("b".to_string()).traced();
        // pretty_assertions::assert_ne! produces a structural diff when
        // the two sides accidentally become equal after refactors.
        assert_ne!(parse.to_string(), io.to_string());
    }

    #[test]
    fn traced_result_ext_wraps_err() {
        let result: Result<(), AstError> = Err(AstError::SymbolNotFound("foo".to_string()));
        let traced: Result<(), TracedAstError> = result.traced();
        assert!(traced.is_err());
        assert_eq!(
            traced.expect_err("traced must be Err").source.to_string(),
            "symbol not found: foo"
        );
    }

    #[test]
    fn traced_result_ext_passes_ok_through() {
        let result: Result<i32, AstError> = Ok(42);
        let traced: Result<i32, TracedAstError> = result.traced();
        assert_eq!(traced.expect("ok value must pass through"), 42);
    }

    #[test]
    fn traced_new_captures_source() {
        let err = AstError::SyntaxError("bad brace".to_string());
        let traced = TracedAstError::new(err);
        assert!(matches!(traced.source, AstError::SyntaxError(ref s) if s == "bad brace"));
    }

    #[test]
    fn traced_source_chain_reaches_original() {
        use std::error::Error;
        let err = AstError::QueryError("bad query".to_string());
        let traced = err.traced();
        // `source()` chain: TracedAstError -> AstError
        let src = traced.source().expect("traced must have a source");
        assert!(src.to_string().contains("bad query"));
    }

    // ── Diagnostic (miette) integration ──────────────────────────────────

    #[test]
    fn ast_error_diagnostic_emits_stable_code() {
        let err = AstError::UnknownLanguage("foo.xyz".to_string());
        let code = err
            .code()
            .expect("UnknownLanguage must advertise a diagnostic code")
            .to_string();
        assert_eq!(code, "touring_ast::unknown_language");
    }

    #[test]
    fn ast_error_diagnostic_emits_help_for_actionable_variants() {
        // Every actionable variant (all except LockPoisoned-ish advisory)
        // must carry a non-empty `help` so the LLM can resolve without
        // a round trip. Spot-check the four most frequent ones.
        for err in [
            AstError::UnknownLanguage("x".into()),
            AstError::ParseFailed("y".into()),
            AstError::SymbolNotFound("z".into()),
            AstError::SyntaxError("w".into()),
        ] {
            let help = err.help().map(|d| d.to_string()).unwrap_or_default();
            assert!(!help.is_empty(), "help for {err:?} must be non-empty");
        }
    }

    #[test]
    fn lock_poisoned_has_warning_severity() {
        let err = AstError::LockPoisoned("inner panicked".into());
        assert_eq!(err.severity(), Some(miette::Severity::Warning));
    }

    #[test]
    fn grammar_unavailable_is_warning_with_stable_code() {
        let err = AstError::GrammarUnavailable(
            "markdown grammar rejected by the tree-sitter runtime (ABI mismatch): \
             Incompatible language version 15. Expected 13-14."
                .to_string(),
        );
        // An ABI mismatch is a tooling limitation, not a source defect — it
        // must carry Warning severity so loggers and the CILA renderer never
        // escalate it to an Error the way `ParseFailed` would.
        assert_eq!(err.severity(), Some(miette::Severity::Warning));
        let code = err
            .code()
            .expect("GrammarUnavailable must advertise a diagnostic code")
            .to_string();
        assert_eq!(code, "touring_ast::grammar_unavailable");
        // Display surfaces both the language and the underlying reason.
        let msg = err.to_string();
        assert!(
            msg.contains("markdown"),
            "Display must name the language: {msg}"
        );
        assert!(
            msg.contains("grammar unavailable"),
            "Display must state the failure mode: {msg}"
        );
    }

    #[test]
    fn traced_delegates_diagnostic_code_to_source() {
        let err = AstError::SymbolNotFound("Foo".into());
        let expected = err.code().expect("source must have code").to_string();
        let traced = err.traced();
        let delegated = traced
            .code()
            .expect("traced must delegate code to source")
            .to_string();
        assert_eq!(delegated, expected);
    }

    #[test]
    fn traced_exposes_diagnostic_source_chain() {
        let err = AstError::InvalidRange("offset=99".into());
        let traced = err.traced();
        assert!(
            traced.diagnostic_source().is_some(),
            "traced must expose an inner Diagnostic source"
        );
    }

    #[test]
    fn report_output_contains_code_and_help() {
        // Verifies the CILA-budget-friendly path: a miette::Report
        // rendered with the compact (non-fancy) handler contains both
        // the diagnostic code and the help text — the two pieces an
        // LLM consumer needs to self-correct without a stack trace.
        let err = AstError::SymbolNotFound("FooWidget".into());
        let report: miette::Report = miette::Report::new(err);
        let rendered = format!("{report:?}");
        assert!(
            rendered.contains("touring_ast::symbol_not_found"),
            "report must surface the diagnostic code, got:\n{rendered}"
        );
        assert!(
            rendered.contains("touring index find"),
            "report must surface the help text, got:\n{rendered}"
        );
    }
}
