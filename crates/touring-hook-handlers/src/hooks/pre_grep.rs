//! Pre-Grep Hook — symbol enrichment for `Grep` tool calls (D43, master plan W2).
//!
//! When Claude Code invokes the `Grep` tool with a symbol-like pattern (PascalCase,
//! snake_case, or camelCase identifier, length 3..=50), inject a context block with
//! locations from the symbol index. CC then frequently resolves the question without
//! running the Grep — massive token saving over time.
//!
//! Safety net (silence-by-default policy):
//! - Whitelist regex: only enrich strict identifiers (no spaces, regex meta, dots).
//! - Length guard: 3..=50 characters.
//! - Disable switch: `TOURING_DISABLE_PREGREP=1` → silent pass-through.
//! - 50 ms hard budget per lookup → silent pass-through on slow path.
//! - 0 results → silent pass-through (no false positive).
//! - Truncate to first 20 locations to keep injected context bounded.
//!
//! Target latency: <10 ms warm cache, <50 ms cold.

use crate::runtime::{HookResponse, HookRuntime};
use crate::shared::gate_metrics;
use std::time::{Duration, Instant};

/// Maximum pattern length we will treat as symbol-like.
const MAX_PATTERN_LEN: usize = 50;
/// Minimum pattern length we will treat as symbol-like.
const MIN_PATTERN_LEN: usize = 3;
/// Maximum number of locations to include in the enrichment block.
const MAX_LOCATIONS: usize = 20;
/// Hard budget for the symbol_store lookup (fail-open).
const LOOKUP_BUDGET: Duration = Duration::from_millis(50);
/// Environment variable that disables the entire enrichment path.
const ENV_DISABLE: &str = "TOURING_DISABLE_PREGREP";

/// Run the pre-grep hook (diverging — used by the `touring-hook pre-grep` CLI path).
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_grep"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Daemon-callable variant returning [`HookResponse`] instead of diverging.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    if std::env::var(ENV_DISABLE).is_ok() {
        return HookResponse::Allow;
    }

    let pattern = match extract_pattern(input) {
        Some(p) => p,
        None => return HookResponse::Allow,
    };

    if !is_symbol_like(&pattern) {
        return HookResponse::Allow;
    }

    let started = Instant::now();
    let locations = lookup_symbol(runtime, &pattern, started);
    if locations.is_empty() {
        gate_metrics::record_pre_grep_zero_results();
        return HookResponse::Allow;
    }
    gate_metrics::record_pre_grep_enrichment();

    HookResponse::Context {
        context: format_enrichment(&pattern, &locations),
        event_name: Some("PreToolUse".to_string()),
    }
}

/// Extract the `pattern` field from a Grep/Glob `tool_input` payload, trimmed.
///
/// Accepts both `{"tool_input": {"pattern": "..."}}` (the canonical Claude Code
/// hook envelope) and the bare `{"pattern": "..."}` form (used in unit tests and
/// when callers strip the envelope upstream).
fn extract_pattern(input: &serde_json::Value) -> Option<String> {
    let scope = input.get("tool_input").unwrap_or(input);
    let raw = scope.get("pattern").and_then(|v| v.as_str())?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Return `true` when `pattern` looks like a strict programming identifier
/// (PascalCase, snake_case, or camelCase) of an enrichable length.
///
/// Rules:
/// - Length must be `MIN_PATTERN_LEN..=MAX_PATTERN_LEN`.
/// - First character must be ASCII letter or `_`.
/// - All other characters must be ASCII alphanumeric or `_`.
/// - At least 3 characters must be ASCII letters (excludes `___`, `_1_2`, etc.).
///
/// Anything containing whitespace, regex meta-characters, dots, slashes, or
/// non-ASCII letters returns `false` and the caller skips enrichment.
pub(crate) fn is_symbol_like(pattern: &str) -> bool {
    let len = pattern.chars().count();
    if !(MIN_PATTERN_LEN..=MAX_PATTERN_LEN).contains(&len) {
        return false;
    }
    let mut chars = pattern.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    let alphabetic_count = pattern.chars().filter(|c| c.is_ascii_alphabetic()).count();
    alphabetic_count >= MIN_PATTERN_LEN
}

/// Look up locations of `pattern` via the runtime's symbol store.
///
/// Returns an empty vector on any of: missing store, lookup error, exceeded
/// budget. Truncates to the first `MAX_LOCATIONS` results.
fn lookup_symbol(runtime: &HookRuntime, pattern: &str, started: Instant) -> Vec<EnrichedLocation> {
    let store = match runtime.infra.symbol_store.as_ref() {
        Some(s) => s,
        None => return Vec::new(),
    };
    if started.elapsed() >= LOOKUP_BUDGET {
        return Vec::new();
    }
    store
        .find_symbol(pattern)
        .map(|locations| {
            locations
                .into_iter()
                .take(MAX_LOCATIONS)
                .map(|loc| EnrichedLocation {
                    file_path: loc.file_path,
                    line: loc.line,
                    column: loc.column,
                    is_definition: loc.is_definition,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Internal value-type — keeps `format_enrichment` decoupled from the AST crate.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EnrichedLocation {
    file_path: String,
    line: usize,
    column: usize,
    is_definition: bool,
}

/// Render the user-facing enrichment block.
fn format_enrichment(pattern: &str, locations: &[EnrichedLocation]) -> String {
    let mut out = String::with_capacity(256 + 80 * locations.len());
    out.push_str("=== Touring symbol enrichment ===\n");
    let count = locations.len();
    out.push_str(&format!(
        "Pattern '{}' resolved to {} symbol location{}:\n",
        pattern,
        count,
        if count == 1 { "" } else { "s" },
    ));
    for loc in locations {
        out.push_str(&format!(
            "  - {}:{}:{} → {}\n",
            loc.file_path,
            loc.line,
            loc.column,
            if loc.is_definition { "def" } else { "ref" },
        ));
    }
    out.push_str(
        "\nSuggestion: read the file:line directly instead of running Grep — saves a tool call.\n",
    );
    out.push_str("=== End enrichment ===\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_is_symbol_like() {
        assert!(is_symbol_like("HookRuntime"));
        assert!(is_symbol_like("MyClass"));
        assert!(is_symbol_like("ABC"));
    }

    #[test]
    fn snake_case_is_symbol_like() {
        assert!(is_symbol_like("cli_pre_grep"));
        assert!(is_symbol_like("foo_bar"));
        assert!(is_symbol_like("_private_fn"));
    }

    #[test]
    fn camel_case_is_symbol_like() {
        assert!(is_symbol_like("getUserId"));
        assert!(is_symbol_like("foo"));
    }

    #[test]
    fn free_text_is_not_symbol_like() {
        assert!(!is_symbol_like("the quick brown fox"));
        assert!(!is_symbol_like("hello world"));
    }

    #[test]
    fn too_short_is_not_symbol_like() {
        assert!(!is_symbol_like("x"));
        assert!(!is_symbol_like("ab"));
    }

    #[test]
    fn too_long_is_not_symbol_like() {
        let s = "a".repeat(60);
        assert!(!is_symbol_like(&s));
    }

    #[test]
    fn digit_start_is_not_symbol_like() {
        assert!(!is_symbol_like("123abc"));
    }

    #[test]
    fn regex_meta_is_not_symbol_like() {
        assert!(!is_symbol_like("foo.*bar"));
        assert!(!is_symbol_like("^hello"));
        assert!(!is_symbol_like("[A-Z]+"));
        assert!(!is_symbol_like("path/to/file"));
    }

    #[test]
    fn underscores_only_is_not_symbol_like() {
        // Fewer than MIN_PATTERN_LEN ASCII letters → not symbol-like.
        assert!(!is_symbol_like("___"));
        assert!(!is_symbol_like("a_1_2"));
    }

    #[test]
    fn extract_pattern_from_tool_input() {
        let input = serde_json::json!({
            "tool_input": {"pattern": "HookRuntime"}
        });
        assert_eq!(extract_pattern(&input).as_deref(), Some("HookRuntime"));
    }

    #[test]
    fn extract_pattern_from_bare_payload() {
        let input = serde_json::json!({"pattern": "HookRuntime"});
        assert_eq!(extract_pattern(&input).as_deref(), Some("HookRuntime"));
    }

    #[test]
    fn extract_pattern_missing_returns_none() {
        let input = serde_json::json!({"tool_input": {}});
        assert!(extract_pattern(&input).is_none());
    }

    #[test]
    fn extract_pattern_trims_whitespace() {
        let input = serde_json::json!({"pattern": "  HookRuntime  "});
        assert_eq!(extract_pattern(&input).as_deref(), Some("HookRuntime"));
    }

    #[test]
    fn format_enrichment_contains_pattern_and_locations() {
        let locs = vec![
            EnrichedLocation {
                file_path: "src/foo.rs".into(),
                line: 42,
                column: 10,
                is_definition: true,
            },
            EnrichedLocation {
                file_path: "src/bar.rs".into(),
                line: 7,
                column: 1,
                is_definition: false,
            },
        ];
        let out = format_enrichment("Foo", &locs);
        assert!(out.contains("Pattern 'Foo' resolved to 2 symbol locations"));
        assert!(out.contains("src/foo.rs:42:10 → def"));
        assert!(out.contains("src/bar.rs:7:1 → ref"));
        assert!(out.contains("=== End enrichment ==="));
    }

    #[test]
    fn format_enrichment_pluralization_singular() {
        let one = vec![EnrichedLocation {
            file_path: "a".into(),
            line: 1,
            column: 1,
            is_definition: true,
        }];
        let out = format_enrichment("X", &one);
        assert!(
            out.contains("1 symbol location:"),
            "expected singular pluralization, got: {out}"
        );
    }
}
