//! TOON (Token-Optimized Object Notation) serialization
//!
//! Compact format for LLM consumption - 40-60% token reduction vs JSON.
//! Uses positional fields with single-character delimiters instead of
//! verbose JSON with repeated keys.

use touring_code::ast::symbols::Symbol;

/// Serialize symbols to TOON format
///
/// Format per symbol: `name|kind|line|end_line|pub/priv|signature`
/// Symbols separated by newlines
///
/// Example:
/// ```text
/// // 2 symbols
/// my_func|function|10|25|pub|def my_func(x: int) -> str:
/// MyClass|class|30|50|pub|class MyClass:
/// ```
pub(crate) fn serialize_symbols(symbols: &[Symbol]) -> String {
    let mut output = String::new();

    for sym in symbols {
        // Format: name|kind|line|end_line|visibility|signature
        let visibility = if sym.is_public { "pub" } else { "priv" };
        output.push_str(&format!(
            "{}|{}|{}|{}|{}|{}\n",
            escape_pipe(&sym.name),
            sym.kind,
            sym.line,
            sym.end_line,
            visibility,
            escape_pipe(&sym.signature)
        ));
    }

    output
}

/// Serialize symbols with header metadata
///
/// Format:
/// ```text
/// # count:N
/// # format:TOONv1
/// name|kind|line|end_line|vis|sig
/// ```
pub(crate) fn serialize_with_header(symbols: &[Symbol], file_path: &str) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!("# file:{}\n", escape_pipe(file_path)));
    output.push_str(&format!("# count:{}\n", symbols.len()));
    output.push_str("# format:TOONv1\n");
    output.push_str("# schema:name|kind|line|end_line|vis|signature\n");
    output.push('\n');

    // Symbols
    output.push_str(&serialize_symbols(symbols));

    output
}

/// Compact serialization for large files - groups by kind
///
/// Format:
/// ```text
/// # functions:3
/// fn|foo|10|20|pub|def foo():
/// fn|bar|25|30|priv|def bar(x):
/// # classes:1
/// cl|MyClass|50|100|pub|class MyClass:
/// ```
pub(crate) fn serialize_compact(symbols: &[Symbol]) -> String {
    let mut output = String::new();

    // Group by kind (SymbolKind.as_str() for HashMap key)
    let mut by_kind: std::collections::HashMap<&str, Vec<&Symbol>> =
        std::collections::HashMap::new();

    for sym in symbols {
        by_kind.entry(sym.kind.as_str()).or_default().push(sym);
    }

    // Output by kind with short codes
    for (kind, syms) in by_kind {
        let kind_code = kind_to_code(kind);
        output.push_str(&format!("# {}:{}\n", kind, syms.len()));

        for sym in syms {
            let visibility = if sym.is_public { "+" } else { "-" };
            output.push_str(&format!(
                "{}|{}|{}|{}|{}|{}\n",
                kind_code,
                escape_pipe(&sym.name),
                sym.line,
                sym.end_line,
                visibility,
                escape_pipe(&sym.signature)
            ));
        }

        output.push('\n');
    }

    output
}

/// Serialize single symbol to minimal format
///
/// Format: `kind:name@line (vis)`
/// Example: `fn:my_func@10 (pub)`
pub(crate) fn serialize_brief(sym: &Symbol) -> String {
    let vis = if sym.is_public { "pub" } else { "priv" };
    format!("{}:{}@{} ({})", sym.kind, sym.name, sym.line, vis)
}

/// Split a TOON line respecting escaped pipes
///
/// "foo\\|bar|baz" -> ["foo|bar", "baz"]
fn split_respecting_escapes(line: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = line.as_bytes();
    let mut i = 0;

    // SAFETY: All indexing below is bounded by `i < bytes.len()` loop guard.
    // `bytes[i]` and `bytes[i-1]` (when i>0) are always in-bounds.
    // Slicing `line[start..i]` is safe because start <= i <= line.len()
    // and the string is ASCII-safe (pipe/backslash are single-byte).
    #[allow(clippy::indexing_slicing)]
    while i < bytes.len() {
        if bytes[i] == b'|' {
            // Check if this pipe is escaped (preceded by backslash)
            if i > 0 && bytes[i - 1] == b'\\' {
                // This is an escaped pipe, skip it
                i += 1;
            } else {
                // This is a field separator
                parts.push(&line[start..i]);
                start = i + 1;
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    // Add the last part
    if start <= line.len() {
        parts.push(&line[start..]);
    }

    parts
}

/// Deserialize TOON format back to symbols (basic parsing)
///
/// Note: This is lossy - byte positions are not restored
pub fn deserialize_symbols(toon: &str) -> Vec<PartialSymbol> {
    let mut symbols = Vec::new();

    for line in toon.lines() {
        // Skip headers and empty lines
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        let parts = split_respecting_escapes(line);
        // SAFETY: All indexing is guarded by `parts.len() >= 6`, so parts[0..5] are in-bounds.
        #[allow(clippy::indexing_slicing)]
        if parts.len() >= 6 {
            symbols.push(PartialSymbol {
                name: unescape_pipe(parts[0]).to_string(),
                kind: parts[1].to_string(),
                line: parts[2].parse().unwrap_or(0),
                end_line: parts[3].parse().unwrap_or(0),
                is_public: parts[4] == "pub" || parts[4] == "+",
                signature: unescape_pipe(parts[5]).to_string(),
            });
        }
    }

    symbols
}

/// Partial symbol for deserialization (without byte positions)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialSymbol {
    /// Symbol name.
    pub name: String,
    /// Symbol kind (e.g. `fn`, `struct`, `enum`).
    pub kind: String,
    /// First line of the symbol's definition.
    pub line: usize,
    /// Last line of the symbol's definition.
    pub end_line: usize,
    /// Whether the symbol is publicly visible.
    pub is_public: bool,
    /// Reconstructed signature text of the symbol.
    pub signature: String,
}

impl PartialSymbol {
    /// Convert to full Symbol (byte positions set to 0)
    pub fn to_symbol(&self) -> Symbol {
        Symbol::new(
            &self.name,
            self.kind.as_str(),
            self.line,
            self.end_line,
            0, // column
            0, // start_byte
            0, // end_byte
            &self.signature,
            self.is_public,
        )
    }
}

/// Escape pipe characters in data
fn escape_pipe(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Unescape pipe characters
fn unescape_pipe(s: &str) -> String {
    s.replace("\\|", "|")
}

/// Convert kind to short code
fn kind_to_code(kind: &str) -> &str {
    match kind {
        "function" => "fn",
        "class" => "cl",
        "struct" => "st",
        "enum" => "en",
        "trait" => "tr",
        "impl" => "im",
        "interface" => "if",
        "method" => "mt",
        "other" => "ot",
        _ => "xx",
    }
}

/// Calculate token savings vs JSON
pub(crate) fn calculate_savings(symbols: &[Symbol]) -> TokenSavings {
    let toon = serialize_symbols(symbols);

    // Approximate JSON size
    let json_approx: String = symbols
        .iter()
        .map(|s| {
            format!(
                "{{\"name\":\"{}\",\"kind\":\"{}\",\"line\":{},\"end_line\":{},\"start_byte\":{},\"end_byte\":{},\"signature\":\"{}\",\"is_public\":{}}}",
                s.name.replace('"', "\\\""),
                s.kind,
                s.line,
                s.end_line,
                s.start_byte,
                s.end_byte,
                s.signature.replace('"', "\\\""),
                s.is_public
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let toon_tokens = estimate_tokens(&toon);
    let json_tokens = estimate_tokens(&json_approx);

    TokenSavings {
        toon_size: toon.len(),
        json_size: json_approx.len(),
        toon_tokens,
        json_tokens,
        savings_percent: if json_tokens > 0 {
            ((json_tokens - toon_tokens) as f64 / json_tokens as f64) * 100.0
        } else {
            0.0
        },
    }
}

/// Token savings statistics
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TokenSavings {
    pub toon_size: usize,
    pub json_size: usize,
    pub toon_tokens: usize,
    pub json_tokens: usize,
    pub savings_percent: f64,
}

impl std::fmt::Display for TokenSavings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TOON: {} chars ({} tokens) vs JSON: {} chars ({} tokens) - {:.1}% reduction",
            self.toon_size,
            self.toon_tokens,
            self.json_size,
            self.json_tokens,
            self.savings_percent
        )
    }
}

/// Estimate token count (approximation: ~4 chars per token for code)
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4 + text.split_whitespace().count() / 2
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_serialize_deserialize() {
        let symbols = vec![
            create_test_symbol("foo", "function", 10),
            create_test_symbol("MyClass", "class", 25),
        ];

        let toon = serialize_symbols(&symbols);
        let deserialized = deserialize_symbols(&toon);

        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].name, "foo");
        assert_eq!(deserialized[0].kind, "function");
        assert_eq!(deserialized[1].name, "MyClass");
        assert_eq!(deserialized[1].kind, "class");
    }

    #[test]
    fn test_serialize_with_header() {
        let symbols = vec![create_test_symbol("test", "function", 1)];
        let toon = serialize_with_header(&symbols, "test.py");

        assert!(toon.contains("# file:test.py"));
        assert!(toon.contains("# count:1"));
        assert!(toon.contains("# format:TOONv1"));
    }

    #[test]
    fn test_pipe_escaping() {
        let mut sym = create_test_symbol("foo|bar", "function", 1);
        sym.signature = "def foo|bar():".to_string();

        let toon = serialize_symbols(&[sym]);
        assert!(toon.contains("foo\\|bar"));

        let deserialized = deserialize_symbols(&toon);
        assert_eq!(deserialized[0].name, "foo|bar");
    }

    #[test]
    fn test_brief_format() {
        let sym = create_test_symbol("my_func", "function", 10);
        let brief = serialize_brief(&sym);
        assert_eq!(brief, "function:my_func@10 (pub)");
    }

    #[test]
    fn test_kind_codes() {
        assert_eq!(kind_to_code("function"), "fn");
        assert_eq!(kind_to_code("class"), "cl");
        assert_eq!(kind_to_code("struct"), "st");
        assert_eq!(kind_to_code("enum"), "en");
        assert_eq!(kind_to_code("trait"), "tr");
        assert_eq!(kind_to_code("impl"), "im");
        assert_eq!(kind_to_code("interface"), "if");
        assert_eq!(kind_to_code("method"), "mt");
        assert_eq!(kind_to_code("other"), "ot");
        assert_eq!(kind_to_code("unknown"), "xx");
    }

    #[test]
    fn test_savings_calculation() {
        let symbols = vec![
            create_test_symbol("foo", "function", 10),
            create_test_symbol("bar", "function", 20),
        ];

        let savings = calculate_savings(&symbols);
        assert!(savings.savings_percent > 20.0); // Should be significant savings
        assert!(savings.toon_tokens < savings.json_tokens);
    }

    #[test]
    fn test_empty_symbols() {
        let toon = serialize_symbols(&[]);
        assert!(toon.is_empty());

        let deserialized = deserialize_symbols(&toon);
        assert!(deserialized.is_empty());
    }

    #[test]
    fn test_compact_format() {
        let symbols = vec![
            create_test_symbol("foo", "function", 10),
            create_test_symbol("bar", "function", 20),
            create_test_symbol("MyClass", "class", 30),
        ];

        let compact = serialize_compact(&symbols);
        assert!(compact.contains("# function:2"));
        assert!(compact.contains("# class:1"));
        assert!(compact.contains("fn|foo"));
        assert!(compact.contains("cl|MyClass"));
    }
}
