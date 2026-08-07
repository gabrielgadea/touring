use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, NodeMatch, Pattern};
use ast_grep_language::SupportLang;
use serde::{Deserialize, Serialize};

use super::error::{Error, Result};
use super::lang::Lang;

/// A single pattern hit with byte offsets, line/col positions, and captured
/// metavariables (`$VAR` and `$$$VAR`). Serializable so it flows cleanly
/// through the touring daemon IPC layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    /// The source text spanned by the match.
    pub text: String,
    /// Byte offset where the match begins.
    pub start_byte: usize,
    /// Byte offset just past the end of the match.
    pub end_byte: usize,
    /// 1-based line number where the match begins.
    pub start_line: usize,
    /// 1-based column where the match begins.
    pub start_col: usize,
    /// 1-based line number where the match ends.
    pub end_line: usize,
    /// 1-based column where the match ends.
    pub end_col: usize,
    /// Captured metavariables as `(name, value)` pairs (`$VAR` / `$$$VAR`).
    pub metavars: Vec<(String, String)>,
}

/// Search `source` for every occurrence of `pattern` using ast-grep's
/// structural matcher. Pattern syntax is the same as the `ast-grep` CLI:
/// `$VAR` matches one node, `$$$VAR` matches zero or more.
pub fn search(lang: Lang, source: &str, pattern: &str) -> Result<Vec<Match>> {
    // Guard: reject degenerate patterns that would cause a panic inside
    // ast-grep-core 0.36.0's match_tree at the debug_assert
    // "Ellipsis should be matched in parent level" (match_tree/mod.rs:82).
    // A bare `$$$` or `$$$NAME` as the entire pattern has no parent context,
    // so the ellipsis metavar is encountered at the root match level, which
    // the engine does not handle. Since `panic=abort` is the release profile,
    // this would abort the daemon process.
    // Heuristic: trim whitespace; if the result starts with `$$$` and
    // contains no other non-identifier/non-`$` characters, it is a bare
    // ellipsis pattern. Reject it with `Err(InvalidPattern)` rather than
    // letting it reach the matcher. (B-FUZZ-001)
    if is_degenerate_ellipsis_pattern(pattern) {
        return Err(Error::InvalidPattern {
            lang: lang.name(),
            reason: "bare ellipsis pattern ($$$VAR alone) has no parent context and cannot be matched; wrap it in a larger pattern".to_string(),
        });
    }
    let sg_lang = lang.as_ast_grep();
    let grep = AstGrep::new(source, sg_lang);
    // `Pattern::try_new` is fallible — `Pattern::new` unwraps internally and
    // panics on malformed patterns (empty, multi-node, degenerate AST).
    // `search` already returns `Result`, so surface the error instead.
    let pat = Pattern::try_new(pattern, sg_lang).map_err(|e| Error::InvalidPattern {
        lang: lang.name(),
        reason: format!("invalid pattern: {e}"),
    })?;
    let root = grep.root();

    let names = extract_metavar_names(pattern);
    let mut hits = Vec::new();
    for m in root.find_all(&pat) {
        hits.push(to_match(&m, &names));
    }
    Ok(hits)
}

fn to_match(m: &NodeMatch<'_, StrDoc<SupportLang>>, names: &[String]) -> Match {
    let range = m.range();
    let start = m.start_pos();
    let end = m.end_pos();

    let mut metavars = Vec::new();
    let env = m.get_env();
    for name in names {
        if let Some(captured) = env.get_match(name) {
            metavars.push((name.clone(), captured.text().to_string()));
        }
    }

    Match {
        text: m.text().to_string(),
        start_byte: range.start,
        end_byte: range.end,
        start_line: start.line(),
        start_col: start.column(m),
        end_line: end.line(),
        end_col: end.column(m),
        metavars,
    }
}

/// Detect a degenerate "bare ellipsis" pattern that would trigger the
/// `debug_assert!("Ellipsis should be matched in parent level")` panic inside
/// `ast-grep-core 0.36.0`'s `match_tree/mod.rs:82`.
///
/// A pattern is degenerate when, after trimming whitespace, it consists
/// **entirely** of one or more `$$$` tokens with optional trailing identifier
/// characters — e.g. `"$$$"`, `"$$$ARGS"`, `"  $$$  "`. These have no parent
/// AST node to anchor the ellipsis, so the engine asserts at the leaf level.
///
/// This guard is a pre-validation step that surfaces the error before the
/// pattern reaches the matcher (B-FUZZ-001). `catch_unwind` is not an option
/// because the release profile uses `panic=abort`.
pub(super) fn is_degenerate_ellipsis_pattern(pattern: &str) -> bool {
    let trimmed = pattern.trim();
    if !trimmed.starts_with("$$$") {
        return false;
    }
    // Everything after the leading `$$$` must be identifier chars or
    // additional `$` signs (for patterns like `$$$$X`). If any non-identifier,
    // non-`$` character is present the pattern has surrounding syntax and is
    // not a bare ellipsis.
    // Wave 5 (2026-05-23) — extended to accept ASCII control chars (NUL, \x01..)
    // after fuzz finding `finding:wave-5-fuzz-b-fuzz-001-extended:2026-05-23`
    // where input `"$$$\0"` bypassed the original whitespace-only `trim()`.
    // Real-world patterns never contain control chars, so classifying them as
    // degenerate is safe and closes the libfuzzer corner case.
    trimmed[3..]
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c.is_control())
}

/// Parse `$VAR` / `$$$VAR` tokens out of a pattern string. Minimal tokenizer
/// — enough to surface captures back to callers. Not a full lexer.
fn extract_metavar_names(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(&c) = bytes.get(i) {
        if c != b'$' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while matches!(bytes.get(j), Some(&b'$')) {
            j += 1;
        }
        let name_start = j;
        while let Some(&b) = bytes.get(j) {
            if b.is_ascii_alphanumeric() || b == b'_' {
                j += 1;
            } else {
                break;
            }
        }
        if j > name_start
            && let Some(name) = pattern.get(name_start..j)
        {
            let starts_ok = name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase() || c == '_');
            if starts_ok && !out.iter().any(|existing| existing == name) {
                out.push(name.to_string());
            }
        }
        i = j.max(i + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_metavars_basic() {
        assert_eq!(extract_metavar_names("console.log($X)"), vec!["X"]);
        assert_eq!(
            extract_metavar_names("fn $NAME($$$ARGS)"),
            vec!["NAME", "ARGS"]
        );
        assert!(extract_metavar_names("no vars here").is_empty());
    }

    #[test]
    fn extract_metavars_dedup() {
        assert_eq!(extract_metavar_names("$X + $X"), vec!["X"]);
    }

    #[test]
    fn search_rejects_malformed_pattern_without_panic() {
        // Regression (W11.6 fuzz): an empty pattern previously panicked inside
        // `ast_grep_core::Pattern::new`; it must now return `Err` cleanly.
        let result = search(Lang::Rust, "fn main() {}", "");
        assert!(matches!(result, Err(Error::InvalidPattern { .. })));
    }

    // B-FUZZ-001 regression suite — bare ellipsis patterns must be rejected
    // before reaching the matcher, where `debug_assert!("Ellipsis should be
    // matched in parent level")` would abort the process in debug builds and
    // abort the daemon (panic=abort) in release builds.

    #[test]
    fn search_rejects_bare_ellipsis_without_panic() {
        let result = search(Lang::Rust, "fn main() {}", "$$$");
        assert!(
            matches!(result, Err(Error::InvalidPattern { .. })),
            "bare $$$ must return Err, not panic"
        );
    }

    #[test]
    fn search_rejects_bare_named_ellipsis_without_panic() {
        let result = search(Lang::Rust, "fn main() {}", "$$$ARGS");
        assert!(
            matches!(result, Err(Error::InvalidPattern { .. })),
            "bare $$$ARGS must return Err, not panic"
        );
    }

    #[test]
    fn search_rejects_whitespace_padded_ellipsis_without_panic() {
        let result = search(Lang::Python, "def f(): pass", "  $$$  ");
        assert!(
            matches!(result, Err(Error::InvalidPattern { .. })),
            "whitespace-padded bare $$$ must return Err"
        );
    }

    #[test]
    fn search_allows_ellipsis_inside_pattern() {
        // An ellipsis that has surrounding syntax is valid — it is anchored
        // by a parent node. This must NOT be rejected by the guard.
        let result = search(
            Lang::Rust,
            "fn foo(a: i32, b: i32) {}",
            "fn $NAME($$$ARGS) {}",
        );
        assert!(
            result.is_ok(),
            "ellipsis inside fn pattern must succeed: {result:?}"
        );
    }

    #[test]
    fn is_degenerate_ellipsis_pattern_cases() {
        assert!(is_degenerate_ellipsis_pattern("$$$"));
        assert!(is_degenerate_ellipsis_pattern("$$$ARGS"));
        assert!(is_degenerate_ellipsis_pattern("  $$$  "));
        assert!(is_degenerate_ellipsis_pattern("  $$$ARGS  "));
        // These are NOT degenerate — they have surrounding syntax
        assert!(!is_degenerate_ellipsis_pattern("fn $F($$$ARGS) {}"));
        assert!(!is_degenerate_ellipsis_pattern("($$$)"));
        assert!(!is_degenerate_ellipsis_pattern(""));
        assert!(!is_degenerate_ellipsis_pattern("$X"));
        assert!(!is_degenerate_ellipsis_pattern("fn main() {}"));
    }

    /// Wave 5 (2026-05-23) — regression test for B-FUZZ-001 extension.
    /// Fuzz target `fuzz_polyglot_search_go` found that input `"$$$\0"`
    /// bypassed the guard (str::trim() trims whitespace only, not control
    /// chars). Memory key: `finding:wave-5-fuzz-b-fuzz-001-extended:2026-05-23`.
    /// Post-fix, the guard accepts ASCII control chars (incl. NUL) after the
    /// `$$$` prefix and classifies them as degenerate.
    #[test]
    fn is_degenerate_ellipsis_pattern_handles_control_char_suffix() {
        // Direct repro from fuzz crash artifact:
        //   crash-a1a60c95304c268b34e2c9badae4ab995ba7e7f9
        //   payload bytes: [54, 0, 0, 0, 36, 36, 36, 0]
        assert!(is_degenerate_ellipsis_pattern("$$$\0"));
        assert!(is_degenerate_ellipsis_pattern("$$$\u{0001}"));
        assert!(is_degenerate_ellipsis_pattern("$$$X\0"));
        assert!(is_degenerate_ellipsis_pattern("$$$\u{007f}")); // DEL
        // Edge: control char in MIDDLE of identifier portion still degenerate
        assert!(is_degenerate_ellipsis_pattern("$$$X\0Y"));
        // Non-degenerate cases must still be rejected even with control char
        assert!(!is_degenerate_ellipsis_pattern("fn $F\0() {}"));
    }

    /// Wave 5 (2026-05-23) — defense-in-depth: search() with fuzz crash input
    /// must NOT panic. Verifies the fix at the call site, not just the guard.
    #[test]
    fn search_rejects_b_fuzz_001_extended_payload_without_panic() {
        // Pattern is the bare ellipsis with NUL suffix; should be rejected by
        // the guard, not reach ast-grep-core where it would panic.
        let result = search(Lang::Go, "package main\nfunc main(){}", "$$$\0");
        assert!(result.is_err(), "guard must reject; got {:?}", result);
    }

    /// Wave 5 (mossy-crunching-owl, 2026-05-23) S-9 — **B-FUZZ-002 closure**.
    ///
    /// ast-grep-core 0.36 + tree-sitter-go 0.25 panicked in `node.rs:73`
    /// (`.expect("should parse")`) under ABI v15 grammar input — Go polyglot
    /// search produced production SIGABRT (BUG-FUZZ-002). After upgrading to
    /// ast-grep 0.42.3 + tree-sitter 0.26 (which bundles a v15 runtime that
    /// accepts the new Go grammar), this MUST return `Ok` for any structurally
    /// valid pattern, regardless of how unusual the source.
    ///
    /// This is the **permanent regression guard** against re-introducing the
    /// panic if a future upgrade reverts the ABI alignment.
    #[test]
    fn test_go_polyglot_search_post_abi_v15_returns_ok_on_arbitrary_input() {
        // Pre-fix (0.36 / 0.24): this exact call panicked under tree-sitter-go
        // ABI v15 grammar mismatch. Post-fix (0.42.3 / 0.26): must succeed.
        let result = search(Lang::Go, "package main\nfunc main() {}", "func $NAME() {}");
        assert!(
            result.is_ok(),
            "Go polyglot search must not panic/error on the canonical regression \
             payload after the Wave 5 ABI v15 alignment — B-FUZZ-002 regression: {:?}",
            result,
        );
        // And the pattern *should* actually match the source: `func main() {}`
        // is structurally `func $NAME() {}`. A zero-length match list would
        // imply a parser silently degrading on Go input — also a regression.
        let matches = result.unwrap();
        assert!(
            !matches.is_empty(),
            "Go pattern `func $NAME() {{}}` should match `func main() {{}}` after ABI v15 alignment",
        );
    }

    /// Wave 5 S-9 (extended) — **arbitrary input must not abort**. Belt-and-
    /// suspenders companion: even an unusual but structurally legal Go source
    /// must not trigger the historical panic.
    #[test]
    fn test_go_polyglot_search_handles_minimal_and_unusual_sources_without_panic() {
        // Minimal legal Go.
        let r1 = search(Lang::Go, "package p", "func $X() {}");
        // Unusual but legal: comments, blank lines, mixed whitespace.
        let r2 = search(
            Lang::Go,
            "// header\n\npackage p\n\n/* block */\n\nfunc f() {}\n",
            "func $X() {}",
        );
        assert!(r1.is_ok(), "minimal Go source must not panic: {:?}", r1);
        assert!(
            r2.is_ok(),
            "Go source with mixed comments must not panic: {:?}",
            r2
        );
    }
}
