//! Output Formatting - TOON and JSON serialization

pub mod json;
pub mod toon;

use touring_code::ast::symbols::Symbol;

/// Format symbols as TOON (Token-Optimized Object Notation)
///
/// Returns TOON format: `name|kind|line|end_line|vis|signature` per line
pub fn format_toon(symbols: &[Symbol]) -> String {
    toon::serialize_symbols(symbols)
}

/// Format symbols as JSON
///
/// Returns pretty-printed JSON array of symbols
pub fn format_json(symbols: &[Symbol]) -> String {
    json::format_json(symbols)
}
