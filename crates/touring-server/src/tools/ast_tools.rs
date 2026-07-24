//! AST Tools - Symbol extraction and code overview
//!
//! Public library API for parsing source files into symbol vectors with
//! pluggable output formatting. The same primitives back the MCP tools
//! `touring_ast_overview` / `touring_ast_find` exposed by `tools_core.rs`
//! AND any future consumer that needs structured AST extraction outside the
//! MCP dispatch (CLI commands, integration tests, agent flows).
//!
//! ## Restored 2026-05-14 (W2.4 ULTRATHINK reversal)
//!
//! Previous wave inlined `touring_ast_overview` / `touring_ast_find` directly
//! into the calling MCP methods, eliminating these as "dead code". REGRA #0
//! ("sempre potencializar — orphan pub symbol → wire") inverts that decision:
//! the symbols are restored, **aperfeiçoados** (FromStr/Display/Default/From
//! conversions), and **wired** to ≥2 consumers each.

use std::str::FromStr;

use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};

use touring_code::ast::languages::Lang;

use crate::output::toon::{
    calculate_savings, serialize_brief, serialize_compact, serialize_symbols, serialize_with_header,
};
use touring_code::ast::symbols::{Symbol, extract_symbols};

// ─────────────────────────────────────────────────────────────────────────
// AST Overview API — symbol extraction with pluggable output formatting
// ─────────────────────────────────────────────────────────────────────────

/// AST Overview Tool — extract symbols from a source file and format them.
///
/// The tool itself is stateless (unit struct); it exists as a named API
/// surface so callers express intent (`AstOverviewTool::new().run(...)`)
/// rather than glue together loose function calls. Useful when chaining
/// multiple transformations.
///
/// # Example
///
/// ```ignore
/// use crate::tools::ast_tools::{AstOverviewTool, OutputFormat};
/// let tool = AstOverviewTool::new();
/// let symbols = tool.extract_from_content(code, Lang::Rust)?;
/// let toon = tool.format_symbols(&symbols, OutputFormat::Toon, Some("a.rs"));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct AstOverviewTool;

impl AstOverviewTool {
    /// Create a new AST overview tool. Stateless — clone-free.
    pub fn new() -> Self {
        Self
    }

    /// Extract symbols from file content for the given language.
    pub fn extract_from_content(
        &self,
        content: &str,
        lang: Lang,
    ) -> touring_foundation::Result<Vec<Symbol>> {
        extract_symbols(content, lang)
            .map_err(|e| touring_foundation::TouringError::Parse(e.to_string()))
    }

    /// Format a symbol vector using the selected output format.
    ///
    /// `file_path` is only used by [`OutputFormat::Toon`] (for the file
    /// header). Other formats ignore it.
    pub fn format_symbols(
        &self,
        symbols: &[Symbol],
        format: OutputFormat,
        file_path: Option<&str>,
    ) -> String {
        match format {
            OutputFormat::Toon => match file_path {
                Some(path) => serialize_with_header(symbols, path),
                None => serialize_symbols(symbols),
            },
            OutputFormat::Compact => serialize_compact(symbols),
            OutputFormat::Brief => symbols
                .iter()
                .map(serialize_brief)
                .collect::<Vec<_>>()
                .join("\n"),
            OutputFormat::Json => serde_json::to_string_pretty(symbols)
                .unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e)),
        }
    }
}

impl Default for AstOverviewTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Output format options for symbol rendering.
///
/// `Toon` is the canonical wire format (40-60% smaller than JSON for typical
/// symbol vectors). `Json` exists for callers that need machine-readable
/// output without TOON parsing. `Compact` and `Brief` are budget-aware
/// human-readable alternatives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Standard TOON format (default) — 40-60% token reduction vs JSON.
    #[default]
    Toon,
    /// Compact TOON with kind grouping for dense displays.
    Compact,
    /// Brief one-line-per-symbol — best for `grep`-style flows.
    Brief,
    /// Pretty JSON — for machine consumers without a TOON parser.
    Json,
}

impl OutputFormat {
    /// Parse from any case-insensitive variant of the discriminant.
    /// Falls back to [`OutputFormat::Toon`] on unknown / empty input —
    /// matches the implicit behavior the MCP tool exposed before this API
    /// was promoted.
    pub fn parse_lenient(s: &str) -> Self {
        Self::from_str(s).unwrap_or_default()
    }
}

impl FromStr for OutputFormat {
    type Err = touring_foundation::TouringError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "toon" | "" => Ok(Self::Toon),
            "compact" => Ok(Self::Compact),
            "brief" => Ok(Self::Brief),
            "json" => Ok(Self::Json),
            other => Err(touring_foundation::TouringError::AstValidation(format!(
                "Unknown output format '{}'. Valid: toon, compact, brief, json",
                other
            ))),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Toon => "toon",
            Self::Compact => "compact",
            Self::Brief => "brief",
            Self::Json => "json",
        };
        f.write_str(s)
    }
}

/// Input arguments for [`touring_ast_overview`] tool.
///
/// Either `content` or `file_path` must be provided. When both are present
/// `content` wins (the file is not re-read). Use [`AstOverviewArgs::default`]
/// for builder-style construction.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AstOverviewArgs {
    /// File content to analyze (required if `file_path` not provided).
    pub content: Option<String>,
    /// File path for language detection + TOON header (optional if `content` provided).
    pub file_path: Option<String>,
    /// Language override (e.g., "python", "rust", "typescript", "javascript").
    pub language: Option<String>,
    /// Output format: "toon" (default), "compact", "brief", "json".
    pub format: Option<String>,
    /// Include token savings analysis appended to the output.
    pub show_savings: Option<bool>,
}

impl AstOverviewArgs {
    /// Builder helper — sets `content`. Other fields default.
    pub fn with_content(content: impl Into<String>) -> Self {
        Self {
            content: Some(content.into()),
            ..Self::default()
        }
    }

    /// Builder helper — sets `file_path`. Other fields default.
    pub fn with_file_path(file_path: impl Into<String>) -> Self {
        Self {
            file_path: Some(file_path.into()),
            ..Self::default()
        }
    }

    /// Resolve to a concrete (content, lang) pair, performing disk I/O
    /// + path-based language detection as necessary.
    ///
    /// Centralizes the validation flow that was previously open-coded across
    /// every caller of this struct. Returns a [`touring_foundation::Result`]
    /// so consumers can map to their own error type via `?`.
    pub fn resolve_content_and_lang(
        self,
    ) -> touring_foundation::Result<(String, Lang, Option<String>)> {
        let content = if let Some(c) = self.content {
            c
        } else if let Some(ref path) = self.file_path {
            std::fs::read_to_string(path).map_err(|e| {
                touring_foundation::TouringError::AstValidation(format!(
                    "Failed to read file '{}': {}",
                    path, e
                ))
            })?
        } else {
            return Err(touring_foundation::TouringError::AstValidation(
                "Either 'content' or 'file_path' must be provided".to_string(),
            ));
        };

        let lang = if let Some(lang_str) = self.language {
            match lang_str.to_lowercase().as_str() {
                "python" | "py" => Lang::Python,
                "rust" | "rs" => Lang::Rust,
                "typescript" | "ts" | "tsx" => Lang::TypeScript,
                "javascript" | "js" | "jsx" => Lang::JavaScript,
                _ => {
                    return Err(touring_foundation::TouringError::AstValidation(format!(
                        "Unsupported language: {}. Use: python, rust, typescript, javascript",
                        lang_str
                    )));
                }
            }
        } else if let Some(ref path) = self.file_path {
            Lang::from_path(std::path::Path::new(path)).ok_or_else(|| {
                touring_foundation::TouringError::AstValidation(
                    "Cannot detect language from file path. Provide 'language' explicitly."
                        .to_string(),
                )
            })?
        } else {
            return Err(touring_foundation::TouringError::AstValidation(
                "Cannot detect language. Provide 'language' or 'file_path'.".to_string(),
            ));
        };

        Ok((content, lang, self.file_path))
    }
}

/// Execute the `touring_ast_overview` tool.
///
/// Extract symbols (functions, classes, structs) from source code for LLM
/// analysis. Returns TOON format (40-60% token reduction vs JSON) with
/// symbol names, kinds, line numbers, signatures, and visibility. Supports
/// Python, Rust, TypeScript, JavaScript.
///
/// Consumers:
/// * `server::tools_core::ast_overview` — MCP tool dispatch
/// * `server::tools_analysis::suggest` (code_pattern action)
/// * `server::tools_analysis::refactor` (analyze action)
/// * `tools::ast_tools::tests` — unit tests
///
/// # Example
/// ```ignore
/// let args = AstOverviewArgs::with_content("def foo(): pass");
/// let result = touring_ast_overview(args)?;
/// ```
pub fn touring_ast_overview(
    args: AstOverviewArgs,
) -> Result<CallToolResult, touring_foundation::TouringError> {
    let tool = AstOverviewTool::new();
    let show_savings = args.show_savings.unwrap_or(false);
    let format = OutputFormat::parse_lenient(args.format.as_deref().unwrap_or(""));

    let (content, lang, file_path) = args.resolve_content_and_lang()?;
    let symbols = tool.extract_from_content(&content, lang)?;
    let mut output = tool.format_symbols(&symbols, format, file_path.as_deref());

    if show_savings {
        let savings = calculate_savings(&symbols);
        output.push('\n');
        output.push_str("# Token Savings:\n");
        output.push_str(&savings.to_string());
        output.push('\n');
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

/// Get symbol at specific line
///
/// Returns the symbol that contains the given line number
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct SymbolAtLineArgs {
    /// File content to analyze
    pub content: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Language (e.g., "python", "rust")
    pub language: String,
}

/// Find the symbol (function, class, etc.) at a specific line number.
/// Returns symbol name, kind, and boundaries.
pub fn touring_symbol_at_line(
    args: SymbolAtLineArgs,
) -> Result<CallToolResult, touring_foundation::TouringError> {
    let lang = match args.language.to_lowercase().as_str() {
        "python" | "py" => Lang::Python,
        "rust" | "rs" => Lang::Rust,
        "typescript" | "ts" => Lang::TypeScript,
        "javascript" | "js" => Lang::JavaScript,
        _ => {
            return Err(touring_foundation::TouringError::AstValidation(format!(
                "Unsupported language: {}",
                args.language
            )));
        }
    };

    let symbols = extract_symbols(&args.content, lang)
        .map_err(|e| touring_foundation::TouringError::Parse(e.to_string()))?;

    // Find symbol containing the line
    let found = symbols
        .iter()
        .find(|s| s.line <= args.line && s.end_line >= args.line);

    let output = if let Some(sym) = found {
        format!(
            "Symbol: {}\nKind: {}\nLines: {}-{}\nVisibility: {}\nSignature: {}",
            sym.name,
            sym.kind,
            sym.line,
            sym.end_line,
            if sym.is_public { "public" } else { "private" },
            sym.signature
        )
    } else {
        format!("No symbol found at line {}", args.line)
    };

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

/// Compare two versions of code
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct DiffSymbolsArgs {
    /// Original code content
    pub original: String,
    /// Modified code content
    pub modified: String,
    /// Language (e.g., "python", "rust")
    pub language: String,
}

/// Compare symbols between two code versions. Shows added, removed, and modified symbols.
pub fn touring_diff_symbols(
    args: DiffSymbolsArgs,
) -> Result<CallToolResult, touring_foundation::TouringError> {
    let lang = match args.language.to_lowercase().as_str() {
        "python" | "py" => Lang::Python,
        "rust" | "rs" => Lang::Rust,
        "typescript" | "ts" => Lang::TypeScript,
        "javascript" | "js" => Lang::JavaScript,
        _ => {
            return Err(touring_foundation::TouringError::AstValidation(format!(
                "Unsupported language: {}",
                args.language
            )));
        }
    };

    let original_symbols = extract_symbols(&args.original, lang)
        .map_err(|e| touring_foundation::TouringError::Parse(e.to_string()))?;
    let modified_symbols = extract_symbols(&args.modified, lang)
        .map_err(|e| touring_foundation::TouringError::Parse(e.to_string()))?;

    // Create lookup by name
    let orig_by_name: std::collections::HashMap<_, _> =
        original_symbols.iter().map(|s| (&s.name, s)).collect();
    let mod_by_name: std::collections::HashMap<_, _> =
        modified_symbols.iter().map(|s| (&s.name, s)).collect();

    // Find changes
    let added: Vec<_> = modified_symbols
        .iter()
        .filter(|s| !orig_by_name.contains_key(&s.name))
        .collect();
    let removed: Vec<_> = original_symbols
        .iter()
        .filter(|s| !mod_by_name.contains_key(&s.name))
        .collect();

    let mut output = String::new();

    if !added.is_empty() {
        output.push_str("# Added:\n");
        for sym in &added {
            output.push_str(&format!(
                "+ {} ({} @ line {})\n",
                sym.name, sym.kind, sym.line
            ));
        }
        output.push('\n');
    }

    if !removed.is_empty() {
        output.push_str("# Removed:\n");
        for sym in &removed {
            output.push_str(&format!(
                "- {} ({} @ line {})\n",
                sym.name, sym.kind, sym.line
            ));
        }
        output.push('\n');
    }

    if added.is_empty() && removed.is_empty() {
        output.push_str("No symbol changes detected.\n");
    }

    output.push_str(&format!(
        "\nSummary: {} added, {} removed, {} unchanged",
        added.len(),
        removed.len(),
        original_symbols.len() - removed.len()
    ));

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

/// File content with path
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct FileContent {
    /// File path (relative to project root)
    pub path: String,
    /// File content
    pub content: String,
    /// Language: "python", "rust", "typescript", "javascript"
    pub language: String,
}

impl FileContent {
    /// Builder-style constructor (avoids verbose struct-literal initialization
    /// at call sites that already have the three strings on hand).
    pub fn new(
        path: impl Into<String>,
        content: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            language: language.into(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// AST Find API — locate symbol definitions/references across files
// ─────────────────────────────────────────────────────────────────────────

/// Serde default used by [`AstFindArgs::definitions_only`].
///
/// Public so the same default can be used by sibling structs in other tool
/// modules without copy-pasting. Stable contract: always returns `true`.
pub fn default_true() -> bool {
    true
}

/// Input arguments for [`touring_ast_find`] tool.
///
/// Searches for symbol definitions and references across multiple files.
/// `files` can be empty — in that case [`touring_ast_find`] falls back to
/// the persisted SymbolStore.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct AstFindArgs {
    /// Symbol name to search for.
    pub symbol_name: String,
    /// List of files to search (each with content and path).
    pub files: Vec<FileContent>,
    /// Include only definitions (`true`) or also references (`false`).
    #[serde(default = "default_true")]
    pub definitions_only: bool,
}

impl Default for AstFindArgs {
    fn default() -> Self {
        Self {
            symbol_name: String::new(),
            files: Vec::new(),
            definitions_only: true,
        }
    }
}

impl AstFindArgs {
    /// Builder helper — sets `symbol_name`. Other fields default.
    pub fn for_symbol(symbol_name: impl Into<String>) -> Self {
        Self {
            symbol_name: symbol_name.into(),
            ..Self::default()
        }
    }

    /// Override the default `definitions_only = true` with chainable syntax.
    pub fn include_references(mut self) -> Self {
        self.definitions_only = false;
        self
    }

    /// Append a [`FileContent`] to the search corpus.
    pub fn with_file(mut self, file: FileContent) -> Self {
        self.files.push(file);
        self
    }
}

/// Execute the `touring_ast_find` tool.
///
/// Find symbol definitions and references across project files. Returns
/// locations with file paths, line numbers, and context. Falls back to the
/// persisted SymbolStore when `args.files` is empty.
///
/// Consumers:
/// * `server::tools_core::ast_find` — MCP tool dispatch (provided-files path)
/// * `tools::ast_tools::tests` — unit tests
///
/// # Example
/// ```ignore
/// let args = AstFindArgs::for_symbol("MyStruct")
///     .with_file(FileContent::new("a.rs", "struct MyStruct;", "rust"));
/// let result = touring_ast_find(args)?;
/// ```
pub fn touring_ast_find(
    args: AstFindArgs,
) -> Result<CallToolResult, touring_foundation::TouringError> {
    use touring_code::ast::graph::{SymbolIndex, SymbolLocation};

    let mut index = SymbolIndex::new();
    let mut from_cache = false;

    if args.files.is_empty() {
        // Fallback to persisted SymbolStore when no files provided.
        let project_root = std::env::var("CLAUDE_PROJECT_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
        let db_path =
            touring_foundation::config::TouringConfig::symbols_db_canonical(&project_root);
        if db_path.exists() {
            if let Ok(store) = touring_code::ast::store::SymbolStore::new(&db_path) {
                let loaded = store.load_into_index(&mut index).unwrap_or(0);
                if loaded > 0 {
                    from_cache = true;
                }
            }
        }
    } else {
        for file in &args.files {
            let lang = match file.language.to_lowercase().as_str() {
                "python" | "py" => Lang::Python,
                "rust" | "rs" => Lang::Rust,
                "typescript" | "ts" | "tsx" => Lang::TypeScript,
                "javascript" | "js" | "jsx" => Lang::JavaScript,
                _ => continue,
            };

            if let Err(e) = index.index_file(&file.path, &file.content, lang) {
                eprintln!("Warning: failed to index {}: {}", file.path, e);
            }
        }
    }

    let locations: Vec<&SymbolLocation> = index.find_symbol(&args.symbol_name);

    let filtered: Vec<&SymbolLocation> = if args.definitions_only {
        locations
            .into_iter()
            .filter(|loc| loc.is_definition)
            .collect()
    } else {
        locations
    };

    let mut output = format!("# Symbol: {}\n\n", args.symbol_name);

    if filtered.is_empty() {
        output.push_str("No definitions found.\n");
    } else {
        output.push_str(&format!("Found {} location(s):\n\n", filtered.len()));
        for loc in &filtered {
            output.push_str(&format!(
                "## {}:{}:{}\n",
                loc.file_path, loc.line, loc.column
            ));
            output.push_str(&format!("- **File**: {}\n", loc.file_path));
            output.push_str(&format!("- **Line**: {}\n", loc.line));
            output.push_str(&format!(
                "- **Type**: {}\n",
                if loc.is_definition {
                    "definition"
                } else {
                    "reference"
                }
            ));
            output.push('\n');
        }
    }

    let stats = index.stats();
    let source = if from_cache {
        "persisted DB"
    } else {
        "provided files"
    };
    output.push_str(&format!(
        "\n---\nIndexed {} files, {} unique symbols, {} total locations (source: {source})\n",
        stats.total_files, stats.total_symbols, stats.total_locations
    ));

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

/// Blast Radius Tool - Calculate impact of changing a symbol
///
/// Uses BFS to find all files transitively affected by a change.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct BlastRadiusArgs {
    /// Starting file path
    pub file_path: String,
    /// Optional: specific symbol name (if empty, analyzes whole file)
    pub symbol_name: Option<String>,
    /// Project files to analyze
    pub files: Vec<FileContent>,
    /// Maximum search depth (default: unlimited)
    pub max_depth: Option<usize>,
}

/// Execute touring_blast_radius tool
///
/// Calculate the blast radius of changing a symbol or file.
/// Returns all affected files with dependency distance.
pub fn touring_blast_radius(
    args: BlastRadiusArgs,
) -> Result<CallToolResult, touring_foundation::TouringError> {
    use touring_code::ast::graph::SymbolIndex;
    use touring_code::ast::languages::Lang;

    let mut index = SymbolIndex::new();

    // Index all provided files
    for file in &args.files {
        let lang = match file.language.to_lowercase().as_str() {
            "python" | "py" => Lang::Python,
            "rust" | "rs" => Lang::Rust,
            "typescript" | "ts" => Lang::TypeScript,
            "javascript" | "js" => Lang::JavaScript,
            _ => continue,
        };

        if let Err(e) = index.index_file(&file.path, &file.content, lang) {
            eprintln!("Warning: failed to index {}: {}", file.path, e);
        }
    }

    // Calculate blast radius
    let radius = index.blast_radius(&args.file_path);

    // Format output
    let mut output = String::new();

    if let Some(ref symbol) = args.symbol_name {
        output.push_str(&format!(
            "# Blast Radius: {} in {}\n\n",
            symbol, args.file_path
        ));
    } else {
        output.push_str(&format!("# Blast Radius: {}\n\n", args.file_path));
    }

    output.push_str(&format!("**Files affected**: {}\n", radius.file_count));
    output.push_str(&format!(
        "**Max distance**: {} hops\n\n",
        radius.max_distance
    ));

    if !radius.affected_files.is_empty() {
        output.push_str("## Affected Files\n\n");
        for file in &radius.affected_files {
            output.push_str(&format!("- `{}`\n", file));
        }
    }

    if !radius.affected_symbols.is_empty() {
        output.push_str(&format!(
            "\n## Affected Symbols ({} total)\n\n",
            radius.affected_symbols.len()
        ));

        // Group by file
        let mut by_file: std::collections::HashMap<&str, Vec<&str>> =
            std::collections::HashMap::new();
        for (file, sym) in &radius.affected_symbols {
            by_file.entry(file).or_default().push(sym);
        }

        for (file, symbols) in by_file.iter().take(10) {
            output.push_str(&format!("### {}\n", file));
            for sym in symbols.iter().take(5) {
                output.push_str(&format!("- {}\n", sym));
            }
            if symbols.len() > 5 {
                output.push_str(&format!("- ... and {} more\n", symbols.len() - 5));
            }
            output.push('\n');
        }
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

// W2.4 ULTRATHINK reversal 2026-05-14 — tests RESTORED with expanded coverage.
// REGRA #0 ("sempre potencializar") inverted the previous "delete and inline"
// decision: helpers are kept as public API and tests now cover both the
// original behavior AND the new builder/FromStr/Display aperfeiçoamentos.

#[cfg(test)]
mod tests {
    use super::*;

    // ── OutputFormat (new FromStr/Display surface) ───────────────────────

    #[test]
    fn test_output_format_default_is_toon() {
        assert_eq!(OutputFormat::default(), OutputFormat::Toon);
    }

    #[test]
    fn test_output_format_from_str_all_variants() {
        for (input, expected) in [
            ("toon", OutputFormat::Toon),
            ("TOON", OutputFormat::Toon), // case-insensitive
            ("", OutputFormat::Toon),     // empty → default
            ("compact", OutputFormat::Compact),
            ("brief", OutputFormat::Brief),
            ("json", OutputFormat::Json),
            ("  json  ", OutputFormat::Json), // trim whitespace
        ] {
            let parsed: OutputFormat = input.parse().unwrap();
            assert_eq!(parsed, expected, "parse '{}'", input);
        }
    }

    #[test]
    fn test_output_format_from_str_rejects_unknown() {
        let err = "yaml".parse::<OutputFormat>().unwrap_err();
        assert!(format!("{err}").contains("Unknown output format"));
    }

    #[test]
    fn test_output_format_parse_lenient_falls_back_to_default() {
        assert_eq!(OutputFormat::parse_lenient(""), OutputFormat::Toon);
        assert_eq!(OutputFormat::parse_lenient("garbage"), OutputFormat::Toon);
        assert_eq!(
            OutputFormat::parse_lenient("compact"),
            OutputFormat::Compact
        );
    }

    #[test]
    fn test_output_format_display_roundtrip() {
        for v in [
            OutputFormat::Toon,
            OutputFormat::Compact,
            OutputFormat::Brief,
            OutputFormat::Json,
        ] {
            let displayed = format!("{v}");
            let parsed: OutputFormat = displayed.parse().unwrap();
            assert_eq!(v, parsed, "roundtrip '{displayed}'");
        }
    }

    #[test]
    fn test_output_format_serde_lowercase() {
        let json = serde_json::to_string(&OutputFormat::Toon).unwrap();
        assert_eq!(json, "\"toon\"");
        let parsed: OutputFormat = serde_json::from_str("\"json\"").unwrap();
        assert_eq!(parsed, OutputFormat::Json);
    }

    // ── AstOverviewArgs (new builder + Default + resolve_*) ──────────────

    #[test]
    fn test_ast_overview_args_default_is_all_none() {
        let args = AstOverviewArgs::default();
        assert!(args.content.is_none());
        assert!(args.file_path.is_none());
        assert!(args.language.is_none());
        assert!(args.format.is_none());
        assert!(args.show_savings.is_none());
    }

    #[test]
    fn test_ast_overview_args_with_content_builder() {
        let args = AstOverviewArgs::with_content("def x(): pass");
        assert_eq!(args.content.as_deref(), Some("def x(): pass"));
        assert!(args.file_path.is_none());
    }

    #[test]
    fn test_ast_overview_args_with_file_path_builder() {
        let args = AstOverviewArgs::with_file_path("/tmp/x.py");
        assert_eq!(args.file_path.as_deref(), Some("/tmp/x.py"));
        assert!(args.content.is_none());
    }

    #[test]
    fn test_ast_overview_args_parsing() {
        let json = r#"{
            "content": "def foo(): pass",
            "language": "python",
            "format": "toon"
        }"#;
        let args: AstOverviewArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.content.as_deref(), Some("def foo(): pass"));
        assert_eq!(args.language.as_deref(), Some("python"));
        assert_eq!(args.format.as_deref(), Some("toon"));
    }

    #[test]
    fn test_resolve_content_and_lang_explicit_language() {
        let args = AstOverviewArgs {
            content: Some("def x(): pass".to_string()),
            language: Some("python".to_string()),
            ..Default::default()
        };
        let (content, lang, file_path) = args.resolve_content_and_lang().unwrap();
        assert_eq!(content, "def x(): pass");
        assert!(matches!(lang, Lang::Python));
        assert!(file_path.is_none());
    }

    #[test]
    fn test_resolve_content_and_lang_infers_from_path() {
        let args = AstOverviewArgs {
            content: Some("x = 1".to_string()),
            file_path: Some("/tmp/x.py".to_string()),
            ..Default::default()
        };
        let (_, lang, file_path) = args.resolve_content_and_lang().unwrap();
        assert!(matches!(lang, Lang::Python));
        assert_eq!(file_path.as_deref(), Some("/tmp/x.py"));
    }

    #[test]
    fn test_resolve_content_and_lang_unknown_language_err() {
        let args = AstOverviewArgs {
            content: Some("...".to_string()),
            language: Some("cobol".to_string()),
            ..Default::default()
        };
        let err = args.resolve_content_and_lang().unwrap_err();
        assert!(format!("{err}").contains("Unsupported language"));
    }

    #[test]
    fn test_resolve_content_and_lang_missing_content_err() {
        let args = AstOverviewArgs {
            language: Some("python".to_string()),
            ..Default::default()
        };
        let err = args.resolve_content_and_lang().unwrap_err();
        assert!(format!("{err}").contains("Either 'content' or 'file_path'"));
    }

    // ── touring_ast_overview (end-to-end) ────────────────────────────────

    #[test]
    fn test_touring_ast_overview_with_content_returns_toon() {
        let args = AstOverviewArgs {
            content: Some("def foo(): pass".to_string()),
            language: Some("python".to_string()),
            format: Some("toon".to_string()),
            ..Default::default()
        };
        let result = touring_ast_overview(args);
        assert!(result.is_ok(), "should succeed for valid python content");
    }

    #[test]
    fn test_touring_ast_overview_file_path_reads_disk() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "def foo(): pass").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let args = AstOverviewArgs {
            file_path: Some(path),
            language: Some("python".to_string()),
            ..Default::default()
        };
        let result = touring_ast_overview(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_touring_ast_overview_neither_content_nor_path_errs() {
        let args = AstOverviewArgs {
            language: Some("python".to_string()),
            ..Default::default()
        };
        assert!(touring_ast_overview(args).is_err());
    }

    // ── AstFindArgs (new builder + Default) ──────────────────────────────

    #[test]
    fn test_ast_find_args_default() {
        let args = AstFindArgs::default();
        assert_eq!(args.symbol_name, "");
        assert!(args.files.is_empty());
        assert!(args.definitions_only); // default is true
    }

    #[test]
    fn test_ast_find_args_for_symbol_builder() {
        let args = AstFindArgs::for_symbol("MyStruct");
        assert_eq!(args.symbol_name, "MyStruct");
        assert!(args.definitions_only);
    }

    #[test]
    fn test_ast_find_args_include_references_chain() {
        let args = AstFindArgs::for_symbol("x").include_references();
        assert!(!args.definitions_only);
    }

    #[test]
    fn test_ast_find_args_with_file_chain() {
        let args = AstFindArgs::for_symbol("MyStruct")
            .with_file(FileContent::new("a.rs", "struct MyStruct;", "rust"))
            .with_file(FileContent::new("b.rs", "fn foo() {}", "rust"));
        assert_eq!(args.files.len(), 2);
        assert_eq!(args.files[0].path, "a.rs");
    }

    #[test]
    fn test_default_true_returns_true() {
        assert!(default_true());
    }

    // ── FileContent (new builder) ────────────────────────────────────────

    #[test]
    fn test_file_content_builder() {
        let fc = FileContent::new("x.rs", "fn x() {}", "rust");
        assert_eq!(fc.path, "x.rs");
        assert_eq!(fc.content, "fn x() {}");
        assert_eq!(fc.language, "rust");
    }

    // ── AstOverviewTool (existing API stays alive via tests) ─────────────

    #[test]
    fn test_ast_overview_tool_default() {
        let _tool = AstOverviewTool; // unit struct → trivial
    }

    #[test]
    fn test_ast_overview_tool_format_brief_no_path_needed() {
        let tool = AstOverviewTool::new();
        let symbols: Vec<Symbol> = vec![];
        let out = tool.format_symbols(&symbols, OutputFormat::Brief, None);
        assert_eq!(out, ""); // empty symbols → empty brief
    }
}
