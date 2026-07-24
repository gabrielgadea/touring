//! AST-enriched feature extraction for LinUCB bandit.
//!
//! Extends the base 19-dim proxy vector with 16 AST-derived features,
//! producing a 35-dim enriched feature vector suitable for [`super::ast_enriched::AstEnrichedBandit`].
//!
//! All AST features are normalized to `[0.0, 1.0]`. On any error (file not found,
//! unsupported language, parse failure), the module returns `[0.0; 16]` — graceful
//! degradation that lets the bandit fall back to base features only.

/// Total dimension of AST-enriched feature vector (base 19 + AST 16).
pub const FEATURE_DIM_AST: usize = 35;

/// Number of AST-derived features appended to the base vector.
pub const AST_FEATURE_COUNT: usize = 16;

// ── Constants used only when ast-features is enabled ─────────────────────

/// Normalization ceiling for symbol count (files with more symbols are clamped to 1.0).
#[cfg(feature = "ast-features")]
const MAX_SYMBOL_COUNT: f64 = 100.0;

/// Normalization ceiling for cyclomatic complexity.
#[cfg(feature = "ast-features")]
const MAX_COMPLEXITY: f64 = 50.0;

/// Normalization ceiling for nesting depth.
#[cfg(feature = "ast-features")]
const MAX_NESTING: f64 = 10.0;

/// Normalization ceiling for import/decorator count.
#[cfg(feature = "ast-features")]
const MAX_COUNT: f64 = 50.0;

/// Extract 16 AST-derived features from a source file.
///
/// Feature layout (indices relative to the 16-element array):
///
/// | Idx | Feature              | Normalization        |
/// |-----|----------------------|----------------------|
/// |  0  | symbol_density       | count / 100          |
/// |  1  | avg_complexity       | mean / 50            |
/// |  2  | max_complexity       | max / 50             |
/// |  3  | has_async            | 0.0 or 1.0           |
/// |  4  | is_test_file         | 0.0 or 1.0           |
/// |  5  | public_ratio         | public / total        |
/// |  6  | max_nesting_depth    | max_lines / 10       |
/// |  7  | doc_coverage         | documented / total    |
/// |  8  | error_handler_count  | (reserved) 0.0       |
/// |  9  | import_count_norm    | decorators / 50      |
/// | 10  | callable_ratio       | callables / total     |
/// | 11  | type_def_ratio       | types / total         |
/// | 12  | container_ratio      | containers / total    |
/// | 13  | avg_symbol_size      | avg lines / 100       |
/// | 14  | (padding)            | 0.0                  |
/// | 15  | (padding)            | 0.0                  |
///
/// Returns `[0.0; 16]` if the file cannot be parsed or has no symbols.
#[cfg(feature = "ast-features")]
pub fn extract_ast_features(file_path: &str) -> [f64; AST_FEATURE_COUNT] {
    extract_ast_features_inner(file_path).unwrap_or([0.0; AST_FEATURE_COUNT])
}

/// Non-cfg version that always returns zeros (used when `ast-features` is disabled).
#[cfg(not(feature = "ast-features"))]
pub fn extract_ast_features(_file_path: &str) -> [f64; AST_FEATURE_COUNT] {
    [0.0; AST_FEATURE_COUNT]
}

/// Inner extraction — returns `None` on any failure for easy `unwrap_or` in the public API.
#[cfg(feature = "ast-features")]
fn extract_ast_features_inner(file_path: &str) -> Option<[f64; AST_FEATURE_COUNT]> {
    use std::path::Path;
    let path = Path::new(file_path);
    let symbols = touring_code::ast::extract_symbols_from_file(path).ok()?;

    if symbols.is_empty() {
        return None;
    }

    let total = symbols.len() as f64;
    let mut features = [0.0f64; AST_FEATURE_COUNT];

    // [0] symbol_density — how many symbols per file (normalized)
    features[0] = (total / MAX_SYMBOL_COUNT).min(1.0);

    // [1] avg_complexity — mean cyclomatic complexity across symbols that have it
    let complexities: Vec<f64> = symbols
        .iter()
        .filter_map(|s| s.complexity.map(f64::from))
        .collect();
    if !complexities.is_empty() {
        let sum: f64 = complexities.iter().sum();
        features[1] = ((sum / complexities.len() as f64) / MAX_COMPLEXITY).min(1.0);
    }

    // [2] max_complexity
    if let Some(&max_c) = complexities
        .iter()
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        features[2] = (max_c / MAX_COMPLEXITY).min(1.0);
    }

    // [3] has_async — 1.0 if any symbol is async
    features[3] = if symbols.iter().any(|s| s.is_async) {
        1.0
    } else {
        0.0
    };

    // [4] is_test_file — heuristic: filename contains "test" or symbols named "test_*"
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
    let has_test_name = filename.contains("test");
    let has_test_symbols = symbols.iter().any(|s| s.name.starts_with("test_"));
    features[4] = if has_test_name || has_test_symbols {
        1.0
    } else {
        0.0
    };

    // [5] public_ratio — fraction of symbols that are public
    let public_count = symbols.iter().filter(|s| s.is_public).count() as f64;
    features[5] = public_count / total;

    // [6] max_nesting_depth proxy — max (end_line - line) across symbols, normalized
    let max_span = symbols
        .iter()
        .map(|s| s.end_line.saturating_sub(s.line))
        .max()
        .unwrap_or(0) as f64;
    features[6] = (max_span / MAX_NESTING).min(1.0);

    // [7] doc_coverage — fraction of symbols with docstrings
    let doc_count = symbols.iter().filter(|s| s.docstring.is_some()).count() as f64;
    features[7] = doc_count / total;

    // [8] error_handler_count — reserved (would need deeper AST analysis)
    features[8] = 0.0;

    // [9] decorator/import count proxy — total decorators across all symbols, normalized
    let decorator_total: usize = symbols.iter().map(|s| s.decorators.len()).sum();
    features[9] = (decorator_total as f64 / MAX_COUNT).min(1.0);

    // [10] callable_ratio — fraction of symbols that are callable
    let callable_count = symbols.iter().filter(|s| s.kind.is_callable()).count() as f64;
    features[10] = callable_count / total;

    // [11] type_def_ratio — fraction that are type definitions
    let type_count = symbols
        .iter()
        .filter(|s| s.kind.is_type_definition())
        .count() as f64;
    features[11] = type_count / total;

    // [12] container_ratio — fraction that are containers
    let container_count = symbols.iter().filter(|s| s.kind.is_container()).count() as f64;
    features[12] = container_count / total;

    // [13] avg_symbol_size — average line span per symbol, normalized
    let total_span: usize = symbols
        .iter()
        .map(|s| s.end_line.saturating_sub(s.line).max(1))
        .sum();
    features[13] = ((total_span as f64 / total) / MAX_SYMBOL_COUNT).min(1.0);

    // [14..15] padding — reserved for future use
    features[14] = 0.0;
    features[15] = 0.0;

    Some(features)
}

/// Combine base 19-dim features with 16 AST features into a 35-dim enriched vector.
///
/// The first 19 elements are copied from `base`, the remaining 16 are extracted
/// from the file at `file_path` using [`extract_ast_features`].
pub fn enrich_features(base: &[f64; 19], file_path: &str) -> [f64; FEATURE_DIM_AST] {
    let ast = extract_ast_features(file_path);
    let mut enriched = [0.0f64; FEATURE_DIM_AST];
    enriched[..19].copy_from_slice(base);
    enriched[19..].copy_from_slice(&ast);
    enriched
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::needless_range_loop)] // vecs have fixed known length
    use super::*;

    #[test]
    fn enrich_preserves_base_features() {
        let base = [1.0; 19];
        let enriched = enrich_features(&base, "/nonexistent/file.py");
        // Base features should be preserved
        for i in 0..19 {
            assert!(
                (enriched[i] - 1.0).abs() < f64::EPSILON,
                "base feature at index {i} was modified"
            );
        }
        // AST features should be zeros for nonexistent file
        for i in 19..FEATURE_DIM_AST {
            assert!(
                enriched[i].abs() < f64::EPSILON,
                "AST feature at index {i} should be 0.0 for missing file, got {}",
                enriched[i]
            );
        }
    }

    #[test]
    fn feature_dim_ast_is_correct() {
        assert_eq!(FEATURE_DIM_AST, 19 + AST_FEATURE_COUNT);
        assert_eq!(FEATURE_DIM_AST, 35);
    }

    #[test]
    fn extract_ast_features_graceful_on_missing_file() {
        let features = extract_ast_features("/this/does/not/exist.rs");
        assert_eq!(features, [0.0; AST_FEATURE_COUNT]);
    }

    #[test]
    fn extract_ast_features_graceful_on_unsupported_extension() {
        let features = extract_ast_features("/tmp/data.csv");
        assert_eq!(features, [0.0; AST_FEATURE_COUNT]);
    }
}
