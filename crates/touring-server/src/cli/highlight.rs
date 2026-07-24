//! Syntax highlighting for terminal CLI output.
//!
//! Wave 5 (2026-04-26) — provides ANSI-coloured rendering of source code
//! using the `syntect` crate (Sublime Text grammars + themes). Used by
//! `touring ast highlight <file>` and reusable from any CLI handler that
//! emits human-readable code to stdout.
//!
//! Wave B (2026-04-28) — CharClasses integration (B.3.4):
//! Lines that fall entirely inside string-literal or comment regions are
//! classified and optionally dimmed (different ANSI style) so non-code
//! regions visually recede without being stripped. Mixed lines (code +
//! string/comment on same line) receive normal syntax highlighting.
//!
//! # Design
//!
//! - Lazy global `SyntaxSet` + `ThemeSet` — first call decompresses the
//!   embedded grammar pack (~5–20 ms cold). Subsequent calls share the
//!   cached state via `once_cell::sync::Lazy` (Send + Sync).
//! - Strict NO-COLOR semantics: respects the
//!   [`NO_COLOR`](https://no-color.org/) env var and `IsTerminal::is_terminal`
//!   on stdout. JSON paths and CI runners always get plain text.
//! - Falls back to plain text for unknown languages (never panics).
//! - Uses the `regex-onig` syntect feature so the workspace's existing
//!   `onig 6.5.1` (transitive via `candle-transformers`) is reused —
//!   zero new C compilation.
//!
//! # Example
//!
//! ```no_run
//! use touring_server::cli::highlight::{highlight_to_ansi, is_terminal_color_enabled};
//! if is_terminal_color_enabled() {
//!     print!("{}", highlight_to_ansi("fn main() {}", "rs"));
//! }
//! ```

use std::io::IsTerminal;

use clap::Parser;
use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

use touring_foundation::char_classes::{CharClass, CharClasses};

/// ANSI reset sequence appended once at the end of highlighted output.
const ANSI_RESET: &str = "\x1b[0m";

/// ANSI style for dimmed/non-code regions (faint cyan).
const ANSI_DIM: &str = "\x1b[38;5;245m";

/// Default theme name — Solarized (dark) ships with `default-themes`
/// and offers good contrast against most terminal background colours.
const DEFAULT_THEME: &str = "Solarized (dark)";

/// A half-open byte range classified as non-code (string/comment/raw/doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringRegion {
    /// Inclusive byte offset where the non-code region begins.
    pub start: usize,
    /// Exclusive byte offset where the non-code region ends.
    pub end: usize,
}

/// Classify each line of `source` as Code or non-code based on CharClasses.
/// Returns a vector of `StringRegion` covering every string/comment/doc region.
fn string_like_regions(source: &str) -> Vec<StringRegion> {
    let chars = CharClasses::new(source);
    let mut regions = Vec::new();
    let mut current: Option<usize> = None;
    for (offset, _, class) in chars {
        match class {
            CharClass::StringLit
            | CharClass::RawString
            | CharClass::Comment
            | CharClass::DocComment => {
                if current.is_none() {
                    current = Some(offset);
                }
            }
            CharClass::Code => {
                if let Some(start) = current.take() {
                    regions.push(StringRegion { start, end: offset });
                }
            }
        }
    }
    regions
}

/// Classify a single line (by byte offset range) as Code or non-code.
/// Returns `true` if the line is entirely inside a string/comment region.
/// `line_start` is the byte offset of the line's first character;
/// `line_end` is the byte offset of the line's trailing `\n` (exclusive).
/// The line is non-code when it starts inside a region and the region
/// extends at least to the last content byte before `\n`.
fn line_is_non_code(line_start: usize, line_end: usize, regions: &[StringRegion]) -> bool {
    regions
        .iter()
        .any(|r| r.start <= line_start && r.end >= line_end.saturating_sub(1))
}

/// Lazily-initialised syntax registry covering ~300 Sublime Text grammars.
///
/// First access decompresses the embedded `default-syntaxes` dump
/// (≈5–20 ms cold). After that, lookups are O(1) hash hits.
static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

/// Lazily-initialised theme registry. Loads ~5 themes (Solarized,
/// base16 variants) bundled by `default-themes`.
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

/// Returns `true` when ANSI colour escapes should be emitted on stdout.
///
/// Honours both the [NO_COLOR](https://no-color.org/) standard and the
/// terminal-detection check from `IsTerminal::is_terminal`. JSON / pipe
/// / CI invocations always return `false`, so callers can branch cleanly:
///
/// ```ignore
/// let rendered = if is_terminal_color_enabled() {
///     highlight_to_ansi(&code, "rs")
/// } else {
///     code.to_string()
/// };
/// ```
pub fn is_terminal_color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// Detect a syntect-compatible language token from a file path's extension.
///
/// Returns the file extension (without the dot) if present, or `"txt"`
/// when no extension exists. The result is suitable for passing to
/// [`highlight_to_ansi`] as the `lang_hint` argument.
pub fn detect_lang_from_path(file_path: &str) -> &str {
    file_path.rsplit_once('.').map_or("txt", |(_, ext)| ext)
}

/// Resolve a syntect [`SyntaxReference`] from a language hint, with fallback.
///
/// The hint may be a token (e.g. `"rust"`, `"rs"`, `"python"`, `"py"`) or
/// a file extension. Unknown hints fall back to plain-text syntax — the
/// function never returns `None`.
fn resolve_syntax(lang_hint: &str) -> &'static SyntaxReference {
    SYNTAX_SET
        .find_syntax_by_token(lang_hint)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(lang_hint))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text())
}

/// Resolve the active theme — preferring [`DEFAULT_THEME`] but falling
/// back to whatever the registry can offer (themes are guaranteed by
/// `default-themes`, but defensive fallback keeps the function total).
fn resolve_theme() -> Option<&'static Theme> {
    THEME_SET
        .themes
        .get(DEFAULT_THEME)
        .or_else(|| THEME_SET.themes.values().next())
}

/// Render `code` as ANSI-coloured terminal output for the language `lang_hint`.
///
/// - Empty input returns an empty string immediately.
/// - Unknown languages fall back to plain-text rendering (still themed).
/// - Theme lookup failures return the input unchanged (never panics).
/// - Output always ends with an ANSI reset (`\x1b[0m`) to avoid bleeding
///   colour into subsequent terminal output.
/// - Lines entirely inside string/comment regions are dimmed (faint ANSI)
///   rather than fully syntax-highlighted — non-code content visually
///   recedes without being stripped (B.3.4).
///
/// This function does **not** check [`is_terminal_color_enabled`]; callers
/// should branch on that themselves so JSON paths can short-circuit.
pub fn highlight_to_ansi(code: &str, lang_hint: &str) -> String {
    if code.is_empty() {
        return String::new();
    }
    let syntax = resolve_syntax(lang_hint);
    let Some(theme) = resolve_theme() else {
        return code.to_string();
    };
    let regions = string_like_regions(code);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out = String::with_capacity(code.len() * 2);
    let mut line_byte_offset = 0usize;
    for line in LinesWithEndings::from(code) {
        let line_start = line_byte_offset;
        let line_end = line_byte_offset + line.len();
        let is_non_code = line_is_non_code(line_start, line_end, &regions);
        if is_non_code {
            // Non-code line: dim instead of full syntax highlighting.
            for ch in line.chars() {
                out.push_str(ANSI_DIM);
                out.push(ch);
            }
            out.push_str(ANSI_RESET);
        } else {
            match highlighter.highlight_line(line, &SYNTAX_SET) {
                Ok(ranges) => out.push_str(&as_24_bit_terminal_escaped(&ranges[..], false)),
                Err(_) => out.push_str(line),
            }
        }
        line_byte_offset = line_end;
    }
    out.push_str(ANSI_RESET);
    out
}

/// Render only lines `[start, end]` of `code` (1-indexed, inclusive) with
/// syntax highlighting. Out-of-range indices are clamped — never panics.
///
/// Useful for `touring ast highlight <file> --start 10 --end 30`.
pub fn highlight_range_to_ansi(code: &str, lang_hint: &str, start: usize, end: usize) -> String {
    if code.is_empty() {
        return String::new();
    }
    let total = code.lines().count();
    if total == 0 {
        return String::new();
    }
    let start_idx = start.saturating_sub(1).min(total.saturating_sub(1));
    let end_idx = end.saturating_sub(1).min(total.saturating_sub(1));
    if start_idx > end_idx {
        return String::new();
    }
    let mut buf = String::new();
    for (i, line) in code.lines().enumerate() {
        if i >= start_idx && i <= end_idx {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    highlight_to_ansi(&buf, lang_hint)
}

// ── CLI types (Wave P3-1.3 W6b clap-derive migration) ────────────────────────

/// `touring ast highlight <file> [--lang <name>] [--start <N>] [--end <N>]`
///
/// Reads the file from disk, applies syntax highlighting, and prints the
/// result to stdout. The `--lang` flag overrides extension-based detection.
/// `--start` / `--end` restrict output to a 1-indexed inclusive line range.
#[derive(Parser, Debug)]
#[command(
    name = "touring ast highlight",
    bin_name = "touring ast highlight",
    about = "Syntax-highlight a file for terminal output",
    disable_help_subcommand = true
)]
struct HighlightCli {
    /// Source file to highlight.
    file_path: String,
    /// Override language detection (e.g. rs, py, ts, txt).
    #[arg(long)]
    lang: Option<String>,
    /// First line to render (1-indexed, inclusive).
    #[arg(long)]
    start: Option<usize>,
    /// Last line to render (1-indexed, inclusive).
    #[arg(long)]
    end: Option<usize>,
}

/// Run the `touring ast highlight <file> [--lang <name>] [--start <N>] [--end <N>]` subcommand.
///
/// Reads the file from disk, applies syntax highlighting, and prints the
/// result to stdout. The `--lang` flag overrides extension-based detection.
/// `--start` / `--end` restrict output to a 1-indexed inclusive line range.
///
/// Errors with [`anyhow::Error`] when the file is missing or unreadable.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match HighlightCli::try_parse_from(args.iter().skip(1)) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    if cli.file_path.is_empty() {
        anyhow::bail!(
            "highlight requires a file path: touring ast highlight <file> [--lang <name>] [--start <N>] [--end <N>]"
        );
    }
    let file_path = &cli.file_path;
    let code = std::fs::read_to_string(file_path)
        .map_err(|e| anyhow::anyhow!("failed to read {file_path}: {e}"))?;
    let lang = cli
        .lang
        .unwrap_or_else(|| detect_lang_from_path(file_path).to_string());
    let start = cli.start;
    let end = cli.end;

    let color_on = is_terminal_color_enabled();
    let rendered = match (start, end, color_on) {
        (Some(s), Some(e), true) => highlight_range_to_ansi(&code, &lang, s, e),
        (Some(s), Some(e), false) => extract_lines_plain(&code, s, e),
        (_, _, true) => highlight_to_ansi(&code, &lang),
        (_, _, false) => code.clone(),
    };
    print!("{rendered}");
    Ok(())
}

/// Extract a 1-indexed inclusive line range from `code` without highlighting.
/// Used when colour output is disabled (`NO_COLOR`, non-TTY, JSON pipe, etc.).
fn extract_lines_plain(code: &str, start: usize, end: usize) -> String {
    let total = code.lines().count();
    if total == 0 {
        return String::new();
    }
    let start_idx = start.saturating_sub(1).min(total.saturating_sub(1));
    let end_idx = end.saturating_sub(1).min(total.saturating_sub(1));
    if start_idx > end_idx {
        return String::new();
    }
    let mut buf = String::new();
    for (i, line) in code.lines().enumerate() {
        if i >= start_idx && i <= end_idx {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    buf
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    HighlightCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_lang_from_path_handles_common_extensions() {
        assert_eq!(detect_lang_from_path("foo.rs"), "rs");
        assert_eq!(detect_lang_from_path("script.py"), "py");
        assert_eq!(detect_lang_from_path("app.tsx"), "tsx");
        assert_eq!(detect_lang_from_path("/tmp/style.css"), "css");
    }

    #[test]
    fn detect_lang_from_path_falls_back_for_extensionless() {
        assert_eq!(detect_lang_from_path("Makefile"), "txt");
        assert_eq!(detect_lang_from_path(""), "txt");
    }

    #[test]
    fn highlight_to_ansi_returns_empty_for_empty_input() {
        assert_eq!(highlight_to_ansi("", "rs"), "");
    }

    #[test]
    fn highlight_to_ansi_emits_ansi_escapes_for_known_lang() {
        let out = highlight_to_ansi("fn main() {}\n", "rs");
        assert!(
            out.contains("\x1b["),
            "expected ANSI escape sequence in output"
        );
        assert!(out.ends_with(ANSI_RESET), "expected reset at end");
    }

    #[test]
    fn highlight_to_ansi_unknown_lang_falls_back_to_plain_text_syntax() {
        // Unknown language resolves to plain-text syntax — output still
        // contains ANSI codes (the theme paints whitespace/identifiers)
        // but the function must NOT panic.
        let out = highlight_to_ansi("abc def\n", "no-such-language-xyz");
        assert!(!out.is_empty());
        assert!(out.ends_with(ANSI_RESET));
    }

    #[test]
    fn highlight_range_to_ansi_clamps_out_of_range() {
        let code = "line1\nline2\nline3\n";
        // start > total: clamped to last line
        let out = highlight_range_to_ansi(code, "txt", 99, 200);
        assert!(
            !out.is_empty(),
            "should not return empty for out-of-range start"
        );
        // start > end: returns empty
        assert_eq!(highlight_range_to_ansi(code, "txt", 5, 2), "");
    }

    #[test]
    fn highlight_range_to_ansi_preserves_only_selected_lines() {
        let code = "line1\nline2\nline3\nline4\n";
        let out = highlight_range_to_ansi(code, "txt", 2, 3);
        // Strip ANSI for content assertion (rough — keep only printable + newline)
        let stripped: String = strip_ansi(&out);
        assert!(stripped.contains("line2"));
        assert!(stripped.contains("line3"));
        assert!(!stripped.contains("line1"));
        assert!(!stripped.contains("line4"));
    }

    #[test]
    fn is_terminal_color_enabled_respects_no_color_env_var() {
        // Save current state
        let prev = std::env::var_os("NO_COLOR");
        // SAFETY: tests in the same process may race. This test asserts
        // a single deterministic outcome under the var being set.
        // SAFETY: setting/unsetting env vars is unsafe in multithreaded
        // tests as of Rust 1.78; we accept the risk because cargo tests
        // run serially per test binary by default and we restore state.
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }
        assert!(
            !is_terminal_color_enabled(),
            "NO_COLOR=1 must disable colour"
        );
        unsafe {
            match prev {
                Some(v) => std::env::set_var("NO_COLOR", v),
                None => std::env::remove_var("NO_COLOR"),
            }
        }
    }

    #[test]
    fn string_like_regions_code_only() {
        let src = "fn main() {}";
        let regions = string_like_regions(src);
        assert!(
            regions.is_empty(),
            "code-only source should have no string regions"
        );
    }

    #[test]
    fn string_like_regions_with_string() {
        let src = r#"let x = "hello";"#;
        let regions = string_like_regions(src);
        assert_eq!(regions.len(), 1, "one string literal region expected");
        assert!(
            src[regions[0].start..regions[0].end].contains("hello"),
            "region should cover the string content"
        );
    }

    #[test]
    fn string_like_regions_with_comment() {
        let src = "// this is a comment\nlet x = 1;";
        let regions = string_like_regions(src);
        assert_eq!(regions.len(), 1, "one comment region expected");
        assert!(
            src[regions[0].start..regions[0].end].contains("this is a comment"),
            "region should cover the comment text"
        );
    }

    #[test]
    fn line_is_non_code_true_for_comment_line() {
        // "// comment\n" spans bytes 0..10 (0..9 content + \n at 9)
        let regions = [StringRegion { start: 0, end: 9 }];
        assert!(
            line_is_non_code(0, 10, &regions),
            "comment line should be classified non-code"
        );
    }

    #[test]
    fn line_is_non_code_false_for_code_line() {
        let regions = [StringRegion { start: 0, end: 9 }];
        assert!(
            !line_is_non_code(12, 20, &regions),
            "code line outside region should not be classified non-code"
        );
    }

    #[test]
    fn line_is_non_code_false_for_mixed_line() {
        // A line where code comes before and after a comment — the comment
        // starts at byte 0, but the region ends before the final \n of this
        // specific line, so this line is NOT fully non-code.
        let src = "let x = 1; // comment\n";
        let regions = string_like_regions(src);
        // The comment region starts at byte 12 ("//"), ends before \n at 24
        // The first line "let x = 1; // comment\n" starts at 0, ends at 25
        // Region end (24) < line end content (24), so NOT fully enclosed
        let first_line_end = src.find('\n').expect("source must contain newline") + 1;
        assert!(
            !line_is_non_code(0, first_line_end, &regions),
            "mixed line (code before comment) should NOT be non-code"
        );
    }

    // ── clap-derive parse tests (Wave P3-1.3 W6b) ────────────────────────────

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn clap_highlight_parses_file_only() {
        // try_parse_from treats the first element as argv[0] (program name).
        // Keep "highlight" as argv[0] so "src/lib.rs" is the positional arg.
        let cli = HighlightCli::try_parse_from(s(&["highlight", "src/lib.rs"]))
            .expect("should parse file-only args");
        assert_eq!(cli.file_path, "src/lib.rs");
        assert!(cli.lang.is_none());
        assert!(cli.start.is_none());
        assert!(cli.end.is_none());
    }

    #[test]
    fn clap_highlight_parses_all_flags() {
        let args = s(&[
            "highlight",
            "foo.py",
            "--lang",
            "python",
            "--start",
            "5",
            "--end",
            "20",
        ]);
        let cli = HighlightCli::try_parse_from(args).expect("should parse all flags");
        assert_eq!(cli.file_path, "foo.py");
        assert_eq!(cli.lang.as_deref(), Some("python"));
        assert_eq!(cli.start, Some(5));
        assert_eq!(cli.end, Some(20));
    }

    #[test]
    fn clap_highlight_lang_equals_syntax() {
        // --lang=ts form; argv[0] = "highlight", positional = "app.js"
        let args = s(&["highlight", "app.js", "--lang=ts"]);
        let cli = HighlightCli::try_parse_from(args).expect("should parse --lang=ts");
        assert_eq!(cli.lang.as_deref(), Some("ts"));
    }

    /// Strip ANSI escape sequences for content-equality assertions in tests.
    fn strip_ansi(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until 'm' (SGR terminator) or end of string.
                for next in chars.by_ref() {
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }
}
