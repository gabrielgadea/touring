//! Code-aware tokenizer for Tantivy that splits identifiers by
//! camelCase, snake_case, dots, double-colons, and path separators.
//!
//! Designed for DISCOVER precision: splitting code identifiers into
//! constituent words enables partial-match search on symbol names.
//!
//! # Examples
//! - `computeFinancialModel` -> `["compute", "financial", "model"]`
//! - `file_knowledge_db` -> `["file", "knowledge", "db"]`
//! - `touring::ast::parser` -> `["touring", "ast", "parser"]`
//! - `src/main.rs` -> `["src", "main", "rs"]`

/// Split a code identifier into constituent tokens.
///
/// Handles: camelCase, PascalCase, SCREAMING_SNAKE, snake_case,
/// dots, double-colons (`::`) , forward slashes, and backslashes.
///
/// All tokens are lowercased. Empty inputs produce an empty vec.
pub fn tokenize_code(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();

    // SAFETY: All accesses use indices bounded by `len` (chars.len()).
    // `i-1` is safe because the guard `!current.is_empty()` implies i > 0.
    // `i+1` is guarded by `i + 1 < len`.
    #[allow(clippy::indexing_slicing)]
    for i in 0..len {
        let c = chars[i];

        // Separators: _ . : / \  — flush current token and skip
        if c == '_' || c == '.' || c == '/' || c == '\\' || c == ':' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current).to_lowercase());
            }
            continue;
        }

        // camelCase / PascalCase boundary detection
        if c.is_uppercase() && !current.is_empty() {
            let prev_lower = chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit();
            let acronym_end = i + 1 < len
                && chars[i + 1].is_lowercase()
                && current.len() > 1
                && current.chars().all(|ch| ch.is_uppercase());

            if prev_lower || acronym_end {
                tokens.push(std::mem::take(&mut current).to_lowercase());
            }
        }

        if c.is_alphanumeric() {
            current.push(c);
        } else {
            // Any other non-alphanumeric, non-separator char: flush
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current).to_lowercase());
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current.to_lowercase());
    }

    tokens.retain(|t| !t.is_empty());
    tokens
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_case_split() {
        assert_eq!(
            tokenize_code("computeFinancialModel"),
            vec!["compute", "financial", "model"]
        );
    }

    #[test]
    fn test_pascal_case_split() {
        assert_eq!(
            tokenize_code("RewardBreakdown"),
            vec!["reward", "breakdown"]
        );
    }

    #[test]
    fn test_snake_case_split() {
        assert_eq!(
            tokenize_code("file_knowledge_db"),
            vec!["file", "knowledge", "db"]
        );
    }

    #[test]
    fn test_path_split() {
        assert_eq!(tokenize_code("src/main.rs"), vec!["src", "main", "rs"]);
    }

    #[test]
    fn test_namespace_split_double_colon() {
        assert_eq!(
            tokenize_code("touring::ast::parser"),
            vec!["touring", "ast", "parser"]
        );
    }

    #[test]
    fn test_mixed_case_complex() {
        assert_eq!(
            tokenize_code("getHTTPSConnection_v2"),
            vec!["get", "https", "connection", "v2"]
        );
    }

    #[test]
    fn test_single_word_unchanged() {
        assert_eq!(tokenize_code("parser"), vec!["parser"]);
    }

    #[test]
    fn test_unicode_preserved() {
        let result = tokenize_code("análise_técnica");
        assert_eq!(result, vec!["análise", "técnica"]);
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(tokenize_code(""), Vec::<String>::new());
    }

    #[test]
    fn test_all_separators() {
        assert_eq!(tokenize_code("a_b.c::d/e"), vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn test_numbers_in_identifier() {
        assert_eq!(
            tokenize_code("compute2ndOrder"),
            vec!["compute2nd", "order"]
        );
    }

    #[test]
    fn test_consecutive_uppercase_acronym() {
        // "HTMLParser" -> "html", "parser"
        assert_eq!(tokenize_code("HTMLParser"), vec!["html", "parser"]);
    }

    #[test]
    fn test_all_uppercase() {
        assert_eq!(tokenize_code("CONSTANT"), vec!["constant"]);
    }

    #[test]
    fn test_backslash_path() {
        assert_eq!(
            tokenize_code("src\\models\\user.rs"),
            vec!["src", "models", "user", "rs"]
        );
    }

    #[test]
    fn test_screaming_snake() {
        assert_eq!(
            tokenize_code("MAX_RETRY_COUNT"),
            vec!["max", "retry", "count"]
        );
    }

    #[test]
    fn test_trailing_separator() {
        assert_eq!(tokenize_code("foo_"), vec!["foo"]);
    }

    #[test]
    fn test_leading_separator() {
        assert_eq!(tokenize_code("_private"), vec!["private"]);
    }

    #[test]
    fn test_multiple_consecutive_separators() {
        assert_eq!(tokenize_code("a__b...c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_digit_transition() {
        // digit followed by uppercase triggers split
        assert_eq!(tokenize_code("phase2Build"), vec!["phase2", "build"]);
    }

    #[test]
    fn test_real_world_rust_path() {
        assert_eq!(
            tokenize_code("crates/touring-nlp/src/search_index/mod.rs"),
            vec![
                "crates", "touring", "nlp", "src", "search", "index", "mod", "rs"
            ]
        );
    }

    #[test]
    fn test_real_world_python_symbol() {
        assert_eq!(
            tokenize_code("ProcessAccumulator.compute_financial_model"),
            vec!["process", "accumulator", "compute", "financial", "model"]
        );
    }
}
