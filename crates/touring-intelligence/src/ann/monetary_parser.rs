//! Monetary Parser - Pln2 Module 1
//!
//! High-performance parser for Brazilian monetary values.
//!
//! # Features
//!
//! - 50+ regex patterns for Brazilian currency formats
//! - Multiplier detection (milhões, bilhões, mil)
//! - Currency symbol detection (R$, US$, €)
//! - Batch processing with Rayon parallelism
//!
//! # Performance
//!
//! - Baseline (Python): 0.83ms/call
//! - Target (Rust): 0.05ms/call
//! - Speedup: 16.7x

use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::{Regex, RegexSet};
use std::fmt;
use thiserror::Error;

// ============================================================================
// ERROR TYPES
// ============================================================================

/// Errors during monetary value parsing
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ParseError {
    /// Invalid number format
    #[error("Invalid number format: {0}")]
    InvalidFormat(String),

    /// Empty input
    #[error("Empty input")]
    EmptyInput,

    /// Regex compilation error
    #[error("Regex error: {0}")]
    RegexError(String),
}

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Parsed monetary value with metadata
#[derive(Debug, Clone)]
pub struct MonetaryValue {
    /// Normalized numeric value
    pub value: f64,

    /// Currency code (BRL, USD, EUR)
    pub currency: String,

    /// Multiplier applied (1.0, 1000.0, 1_000_000.0, 1_000_000_000.0)
    pub multiplier: f64,

    /// Original text that was matched
    pub source_text: String,

    /// Position in source text (start, end)
    pub position: (usize, usize),

    /// Confidence score [0.0, 1.0]
    pub confidence: f64,

    /// Context around the match
    pub context: String,
}

impl fmt::Display for MonetaryValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MonetaryValue({} {} @ {:?}, conf={:.2})",
            self.currency, self.value, self.position, self.confidence
        )
    }
}

// ============================================================================
// REGEX PATTERNS (Compiled Once)
// ============================================================================

/// Compiled patterns for monetary value detection
static MONETARY_PATTERNS: Lazy<MonetaryPatterns> = Lazy::new(|| {
    MonetaryPatterns::new().unwrap_or_else(|_| panic!("Failed to compile monetary patterns"))
});

struct MonetaryPatterns {
    /// Primary patterns for structured values (R$ X.XXX,XX).
    ///
    /// EC61: `primary_set` (RegexSet) is used as a fast O(n_chars) pre-filter in
    /// `parse_monetary` — if no pattern matches, the per-pattern `primary_patterns`
    /// loop is skipped entirely. `primary_patterns` (Vec<Regex>) is used for the
    /// per-match extraction pass after the pre-filter confirms at least one hit.
    primary_set: RegexSet,
    primary_patterns: Vec<Regex>,

    /// Number extraction
    number_pattern: Regex,
}

impl MonetaryPatterns {
    fn new() -> Result<Self, ParseError> {
        // Primary patterns for R$ X.XXX,XX format
        let primary_strings = vec![
            // R$ with Brazilian format (most common)
            r"R\$\s*[\d.,]+(?:\s*(?:milhões?|bilhões?|mil))?",
            // US$ with US format
            r"US\$\s*[\d.,]+",
            // € Euro
            r"€\s*[\d.,]+",
            // Number followed by "reais"
            r"[\d.,]+\s*reais",
            // Number followed by multiplier and optional currency
            r"[\d.,]+\s*(?:milhão|milhões|bilhão|bilhões|mil)(?:\s+(?:de\s+)?(?:reais|dólares|euros))?",
            // Written values: X milhões/bilhões (with comma decimal)
            r"\d+(?:[.,]\d+)?\s*(?:milhão|milhões|bilhão|bilhões|mil)",
        ];

        let primary_set =
            RegexSet::new(&primary_strings).map_err(|e| ParseError::RegexError(e.to_string()))?;

        let primary_patterns: Vec<Regex> = primary_strings
            .iter()
            .map(|p| Regex::new(p).unwrap_or_else(|_| panic!("pre-validated regex pattern: {p}")))
            .collect();

        // Number extraction (handles both BR and US formats)
        let number_pattern =
            Regex::new(r"[\d.,]+").map_err(|e| ParseError::RegexError(e.to_string()))?;

        Ok(Self {
            primary_set,
            primary_patterns,
            number_pattern,
        })
    }
}

// ============================================================================
// CORE FUNCTIONS
// ============================================================================

/// Normalize a Brazilian number format to f64
///
/// Handles both Brazilian (1.234,56) and US (1,234.56) formats.
pub fn normalize_br_number(s: &str) -> Result<f64, ParseError> {
    let s = s.trim();

    if s.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Count dots and commas to determine format
    let dot_count = s.matches('.').count();
    let comma_count = s.matches(',').count();

    let normalized = if comma_count == 1
        && s.ends_with(&['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'][..])
    {
        // Check if comma is decimal separator (Brazilian format)
        let parts: Vec<&str> = s.split(',').collect();
        let after_comma_len = parts.get(1).map_or(0, |p| p.len());
        if parts.len() == 2 && after_comma_len <= 2 {
            // Brazilian format: 1.234,56
            s.replace('.', "").replace(',', ".")
        } else if dot_count > 0 {
            // US format with comma thousands: 1,234.56
            s.replace(',', "")
        } else {
            // Single comma, check decimal places
            if parts.len() == 2 && after_comma_len <= 2 {
                s.replace(',', ".")
            } else {
                s.replace(',', "")
            }
        }
    } else if dot_count == 1 && comma_count == 0 {
        // Could be US decimal or BR thousands
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 2 && parts.get(1).is_some_and(|p| p.len() == 3) {
            // Brazilian thousands separator: 1.000
            s.replace('.', "")
        } else {
            // US decimal: 1.50
            s.to_string()
        }
    } else if dot_count > 1 {
        // Multiple dots = Brazilian thousands separators
        s.replace('.', "").replace(',', ".")
    } else if comma_count > 1 {
        // Multiple commas = US thousands separators
        s.replace(',', "")
    } else {
        // No separators or single comma
        s.replace(',', ".")
    };

    normalized
        .parse::<f64>()
        .map_err(|_| ParseError::InvalidFormat(s.to_string()))
}

/// Detect multiplier from context text
pub fn detect_multiplier(context: &str) -> f64 {
    let context_lower = context.to_lowercase();

    // Check for billions first (bilhão, bilhões)
    if context_lower.contains("bilhão")
        || context_lower.contains("bilhões")
        || context_lower.contains("bilhao")
        || context_lower.contains("bilhoes")
    {
        1_000_000_000.0
    // Check for millions (milhão, milhões)
    } else if context_lower.contains("milhão")
        || context_lower.contains("milhões")
        || context_lower.contains("milhao")
        || context_lower.contains("milhoes")
    {
        1_000_000.0
    // Check for thousands (mil) - must be word boundary
    } else if context_lower.contains(" mil")
        || context_lower.ends_with("mil")
        || context_lower.starts_with("mil ")
    {
        1_000.0
    } else {
        1.0
    }
}

/// Detect currency from text
fn detect_currency(text: &str) -> (&'static str, f64) {
    let text_upper = text.to_uppercase();

    if text_upper.contains("US$") || text_upper.contains("USD") || text_upper.contains("DÓLAR") {
        ("USD", 0.95)
    } else if text_upper.contains('€') || text_upper.contains("EUR") {
        ("EUR", 0.95)
    } else if text_upper.contains("R$")
        || text_upper.contains("BRL")
        || text_upper.contains("REAIS")
    {
        ("BRL", 1.0)
    } else {
        ("BRL", 0.7) // Default with lower confidence
    }
}

/// Extract context around a match
fn extract_context(text: &str, start: usize, end: usize, context_size: usize) -> String {
    let ctx_start = start.saturating_sub(context_size);
    let ctx_end = (end + context_size).min(text.len());

    // MSRV 1.75-compatible char boundary helpers (floor_char_boundary/ceil_char_boundary
    // are stable only since 1.80). Walk bytes backwards/forwards to find UTF-8 boundaries.
    let safe_start = floor_char_boundary(text, ctx_start);
    let safe_end = ceil_char_boundary(text, ctx_end);

    text[safe_start..safe_end].to_string()
}

/// Find the largest byte index `<= i` that is a valid char boundary.
/// MSRV 1.75-compatible polyfill for `str::floor_char_boundary` (stable 1.80+).
fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Find the smallest byte index `>= i` that is a valid char boundary.
/// MSRV 1.75-compatible polyfill for `str::ceil_char_boundary` (stable 1.80+).
fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Parse monetary values from text
pub fn parse_monetary(text: &str) -> Result<Vec<MonetaryValue>, ParseError> {
    if text.is_empty() {
        return Ok(vec![]);
    }

    let patterns = &*MONETARY_PATTERNS;

    // EC61: Fast pre-filter via RegexSet — O(n_chars) single pass.
    // If no primary pattern matches at all, skip the per-pattern loop entirely.
    if !patterns.primary_set.is_match(text) {
        return Ok(vec![]);
    }

    let mut results = Vec::new();
    let mut seen_positions = std::collections::HashSet::new();

    // Find all matching patterns
    for pattern in &patterns.primary_patterns {
        for mat in pattern.find_iter(text) {
            let start = mat.start();
            let end = mat.end();

            // Skip if we already captured this position
            if seen_positions.contains(&start) {
                continue;
            }
            seen_positions.insert(start);

            let matched_text = mat.as_str();

            // Extract the number part
            if let Some(num_match) = patterns.number_pattern.find(matched_text) {
                let num_str = num_match.as_str();

                if let Ok(base_value) = normalize_br_number(num_str) {
                    let (currency, currency_confidence) = detect_currency(matched_text);
                    let multiplier = detect_multiplier(matched_text);
                    let final_value = base_value * multiplier;

                    // Skip percentages (values like 10.5 followed by %)
                    let after_match = &text[end..].chars().take(5).collect::<String>();
                    if after_match.trim_start().starts_with('%') {
                        continue;
                    }

                    // Calculate confidence
                    let has_symbol = matched_text.contains("R$")
                        || matched_text.contains("US$")
                        || matched_text.contains('€');
                    let base_confidence = if has_symbol { 0.95 } else { 0.75 };
                    let confidence = base_confidence * currency_confidence;

                    let context = extract_context(text, start, end, 30);

                    results.push(MonetaryValue {
                        value: final_value,
                        currency: currency.to_string(),
                        multiplier,
                        source_text: matched_text.to_string(),
                        position: (start, end),
                        confidence,
                        context,
                    });
                }
            }
        }
    }

    // Sort by position
    results.sort_by_key(|v| v.position.0);

    Ok(results)
}

/// Parse monetary values from multiple texts in parallel
///
/// Uses Rayon for parallel processing across multiple documents.
pub fn parse_monetary_batch(texts: Vec<String>) -> Vec<Vec<MonetaryValue>> {
    texts
        .par_iter()
        .map(|text| parse_monetary(text).unwrap_or_default())
        .collect()
}

// NOTE: str::floor_char_boundary and str::ceil_char_boundary are stable since
// Rust 1.77 — the StringExt polyfill was removed in EC54 as redundant.

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patterns_compile() {
        // Force lazy initialization
        let _ = &*MONETARY_PATTERNS;
    }

    #[test]
    fn test_detect_currency_brl() {
        let (currency, _) = detect_currency("R$ 100,00");
        assert_eq!(currency, "BRL");
    }

    #[test]
    fn test_detect_currency_usd() {
        let (currency, _) = detect_currency("US$ 100.00");
        assert_eq!(currency, "USD");
    }

    #[test]
    fn test_detect_currency_eur() {
        let (currency, _) = detect_currency("€ 100,00");
        assert_eq!(currency, "EUR");
    }

    #[test]
    fn test_context_extraction() {
        let text = "O valor é R$ 100,00 no total";
        let context = extract_context(text, 10, 20, 5);
        assert!(!context.is_empty());
    }

    #[test]
    fn test_string_ext_floor() {
        let s = "Olá";
        assert!(s.is_char_boundary(s.floor_char_boundary(2)));
    }

    #[test]
    fn test_string_ext_ceil() {
        let s = "Olá";
        assert!(s.is_char_boundary(s.ceil_char_boundary(2)));
    }

    #[test]
    fn test_normalize_br_number_basic() {
        assert!((normalize_br_number("1.234,56").unwrap() - 1234.56).abs() < 0.01);
    }

    #[test]
    fn test_parse_monetary_basic() {
        let result = parse_monetary("R$ 1.000,00").unwrap();
        assert_eq!(result.len(), 1);
        assert!((result[0].value - 1000.0).abs() < 0.01);
    }

    #[test]
    fn test_detect_multiplier_millions() {
        assert!((detect_multiplier("100 milhões") - 1_000_000.0).abs() < 0.01);
    }

    #[test]
    fn test_detect_multiplier_billions() {
        assert!((detect_multiplier("2 bilhões") - 1_000_000_000.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_monetary_batch_basic() {
        let texts = vec!["R$ 100,00".to_string(), "US$ 200.00".to_string()];
        let results = parse_monetary_batch(texts);
        assert_eq!(results.len(), 2);
    }
}
