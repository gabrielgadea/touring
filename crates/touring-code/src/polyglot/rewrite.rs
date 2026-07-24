use ast_grep_core::{AstGrep, Pattern};

use super::error::{Error, Result};
use super::lang::Lang;
use super::search::is_degenerate_ellipsis_pattern;

/// Apply `replacement` to every occurrence of `pattern` in `source`.
/// `replacement` may reference metavars bound by `pattern` — e.g. searching
/// `console.log($X)` and rewriting to `logger.info($X)` preserves the arg.
///
/// Iterates `AstGrep::replace` until no further match is found. The tree is
/// re-parsed between each edit, so position drift from prior substitutions
/// never corrupts later matches.
///
/// Returns the transformed source. If no matches are found, returns the
/// input verbatim.
pub fn rewrite(lang: Lang, source: &str, pattern: &str, replacement: &str) -> Result<String> {
    // Guard: reject bare ellipsis patterns before they reach the matcher and
    // trigger the debug_assert in match_tree/mod.rs:82 (B-FUZZ-001).
    // See `search::is_degenerate_ellipsis_pattern` for the heuristic.
    if is_degenerate_ellipsis_pattern(pattern) {
        return Err(Error::InvalidPattern {
            lang: lang.name(),
            reason: "bare ellipsis pattern ($$$VAR alone) has no parent context and cannot be matched; wrap it in a larger pattern".to_string(),
        });
    }
    let sg_lang = lang.as_ast_grep();
    let mut grep = AstGrep::new(source, sg_lang);
    // `Pattern::try_new` is fallible — `Pattern::new` unwraps internally and
    // panics on malformed patterns (empty, multi-node, degenerate AST).
    // `rewrite` already returns `Result`, so surface the error instead.
    let pat = Pattern::try_new(pattern, sg_lang).map_err(|e| Error::InvalidPattern {
        lang: lang.name(),
        reason: format!("invalid pattern: {e}"),
    })?;

    // Bound the loop to prevent runaway rewrite cycles when the replacement
    // itself matches the pattern (identity rewrite or fixpoint miss).
    let mut remaining = 10_000usize;
    loop {
        let replaced =
            grep.replace(pat.clone(), replacement)
                .map_err(|e| Error::InvalidPattern {
                    lang: lang.name(),
                    reason: format!("replace failed: {e:?}"),
                })?;
        if !replaced {
            break;
        }
        remaining = remaining.saturating_sub(1);
        if remaining == 0 {
            return Err(Error::InvalidPattern {
                lang: lang.name(),
                reason: "rewrite exceeded 10000 iterations — likely a self-matching pattern"
                    .to_string(),
            });
        }
    }
    Ok(grep.generate())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_rejects_malformed_pattern_without_panic() {
        // Regression (W11.6 fuzz): an empty pattern previously panicked inside
        // `ast_grep_core::Pattern::new`; it must now return `Err` cleanly.
        let result = rewrite(Lang::Rust, "fn main() {}", "", "fn other() {}");
        assert!(matches!(result, Err(Error::InvalidPattern { .. })));
    }

    // B-FUZZ-001 regression suite — bare ellipsis patterns must be rejected
    // before reaching the matcher (same guard as in search.rs).

    #[test]
    fn rewrite_rejects_bare_ellipsis_without_panic() {
        let result = rewrite(Lang::Rust, "fn main() {}", "$$$", "something");
        assert!(
            matches!(result, Err(Error::InvalidPattern { .. })),
            "bare $$$ must return Err, not panic"
        );
    }

    #[test]
    fn rewrite_rejects_bare_named_ellipsis_without_panic() {
        let result = rewrite(Lang::Rust, "fn main() {}", "$$$ARGS", "$$$ARGS");
        assert!(
            matches!(result, Err(Error::InvalidPattern { .. })),
            "bare $$$ARGS must return Err, not panic"
        );
    }

    #[test]
    fn rewrite_allows_ellipsis_inside_pattern() {
        // A valid pattern with ellipsis anchored inside surrounding syntax.
        let source = "foo(1, 2, 3)";
        let result = rewrite(Lang::JavaScript, source, "foo($$$ARGS)", "bar($$$ARGS)");
        assert!(
            result.is_ok(),
            "ellipsis inside call pattern must succeed: {result:?}"
        );
    }
}
