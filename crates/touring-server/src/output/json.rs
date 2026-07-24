//! JSON output formatting for symbols

use serde_json;

use touring_code::ast::symbols::Symbol;

/// Format symbols as JSON string
///
/// Returns a pretty-printed JSON array of symbols.
pub fn format_json(symbols: &[Symbol]) -> String {
    match serde_json::to_string_pretty(symbols) {
        Ok(json) => json,
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    }
}

/// Format symbols as compact JSON (single line)
pub fn format_json_compact(symbols: &[Symbol]) -> String {
    match serde_json::to_string(symbols) {
        Ok(json) => json,
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_code::ast::symbols::Symbol;

    fn create_test_symbol(name: &str, kind: &str, line: usize) -> Symbol {
        Symbol::new(
            name,
            kind,
            line,
            line + 10,
            0,   // column
            0,   // start_byte
            100, // end_byte
            format!("{} {}(...)", kind, name),
            true,
        )
    }

    #[test]
    fn test_format_json_basic() {
        let symbols = vec![
            create_test_symbol("foo", "function", 10),
            create_test_symbol("MyClass", "class", 25),
        ];

        let json = format_json(&symbols);
        assert!(json.contains("\"name\": \"foo\""));
        assert!(json.contains("\"name\": \"MyClass\""));
        assert!(json.contains("\"kind\": \"function\""));
        assert!(json.contains("\"kind\": \"class\""));
    }

    #[test]
    fn test_format_json_empty() {
        let json = format_json(&[]);
        assert_eq!(json, "[]");
    }

    #[test]
    fn test_format_json_compact() {
        let symbols = vec![create_test_symbol("test", "function", 1)];
        let json = format_json_compact(&symbols);
        // Compact JSON should not have newlines
        assert!(!json.contains('\n'));
        assert!(json.contains("\"name\":\"test\""));
    }
}
