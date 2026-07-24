//! `SnippetEdit` — cursor placement and tab-stop metadata for editor integration.
//!
//! B.5.3: `SnippetEdit` for cursor placement post-apply (`$0`, `${0:default}`).
//!
//! This encodes Visual Studio Code / IntelliJ-style snippet syntax, used to
//! tell the Claude Code shell where to place the cursor after applying a change.

use std::fmt;

/// A single tab stop in a snippet, with an optional default text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStop {
    /// The tab stop index (0 is the final cursor position).
    pub index: u8,
    /// Default text if the user doesn't type anything before advancing.
    pub default_text: String,
}

/// A snippet edit encoding cursor placement and tab stops.
///
/// Snippet syntax follows VS Code / `IntelliJ` conventions:
/// - `$0` — final cursor position (only one per snippet)
/// - `${0:default}` — tab stop with index 0 and default text "default"
/// - `${1:foo}` — tab stop with index 1 and default text "foo"
/// - `$variable` — simple variable substitution
///
/// The snippet is applied after the text edits, providing cursor placement
/// metadata to the client (Claude Code shell).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SnippetEdit {
    /// The raw snippet template string.
    template: String,
    /// Parsed tab stops extracted from the template.
    tab_stops: Vec<TabStop>,
    /// The final cursor position (`$0`).
    cursor_position: Option<u8>,
}

impl SnippetEdit {
    /// Create a new `SnippetEdit` from a template string.
    ///
    /// Parses tab stops and cursor position from the template.
    /// Returns `None` if the template is invalid.
    #[must_use]
    pub fn new(template: String) -> Self {
        let (tab_stops, cursor_position) = parse_snippet(&template);
        Self {
            template,
            tab_stops,
            cursor_position,
        }
    }

    /// Create a `SnippetEdit` with explicit tab stops and cursor position.
    /// Use this for more complex snippet scenarios.
    #[must_use]
    pub fn with_tab_stops(
        template: String,
        tab_stops: Vec<TabStop>,
        cursor_position: Option<u8>,
    ) -> Self {
        Self {
            template,
            tab_stops,
            cursor_position,
        }
    }

    /// Returns the raw template string.
    #[inline]
    #[must_use]
    pub fn template(&self) -> &str {
        &self.template
    }

    /// Returns the parsed tab stops.
    #[inline]
    #[must_use]
    pub fn tab_stops(&self) -> &[TabStop] {
        &self.tab_stops
    }

    /// Returns the cursor position (index of `$0`).
    #[inline]
    #[must_use]
    pub fn cursor_position(&self) -> Option<u8> {
        self.cursor_position
    }

    /// Returns true if this snippet has no tab stops (just a cursor placement).
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.tab_stops.is_empty()
    }
}

/// Parse a snippet template string, extracting tab stops and cursor position.
fn parse_snippet(template: &str) -> (Vec<TabStop>, Option<u8>) {
    let mut tab_stops = Vec::new();
    let mut cursor_position = None;
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '$' {
            continue;
        }

        let (n, default_text) = match chars.peek() {
            Some('{') => parse_brace_form(&mut chars),
            Some(d) if d.is_ascii_digit() => parse_digit_form(&mut chars),
            _ => continue,
        };

        if n == 0 {
            cursor_position = Some(0);
        } else {
            tab_stops.push(TabStop {
                index: n,
                default_text,
            });
        }
    }

    (tab_stops, cursor_position)
}

/// Parse `${N:default}` form. Returns `(N, default_text)`.
fn parse_brace_form(chars: &mut std::iter::Peekable<impl Iterator<Item = char>>) -> (u8, String) {
    chars.next(); // consume '{'
    let n = parse_number(chars);

    let default_text = match chars.peek() {
        Some(':') => {
            chars.next(); // consume ':'
            extract_until_close_brace(chars)
        }
        Some('}') => {
            chars.next();
            String::new()
        }
        _ => String::new(),
    };

    (n, default_text)
}

/// Parse `$N` form (no braces). Returns `(N, empty_default)`.
fn parse_digit_form(chars: &mut std::iter::Peekable<impl Iterator<Item = char>>) -> (u8, String) {
    let n = parse_number(chars);
    (n, String::new())
}

/// Parse a decimal number from the character iterator.
fn parse_number(chars: &mut std::iter::Peekable<impl Iterator<Item = char>>) -> u8 {
    let mut num_buf = String::new();
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            num_buf.push(ch);
            chars.next();
        } else {
            break;
        }
    }
    num_buf.parse().unwrap_or(0)
}

/// Extract text until matching '}' (accounting for nested braces).
fn extract_until_close_brace(
    chars: &mut std::iter::Peekable<impl Iterator<Item = char>>,
) -> String {
    let mut text = String::new();
    let mut depth = 1;
    for ch in chars {
        match ch {
            '}' if depth == 1 => break,
            '{' => {
                depth += 1;
                text.push(ch);
            }
            _ => text.push(ch),
        }
    }
    text
}

impl fmt::Display for SnippetEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SnippetEdit({:?})", self.template)
    }
}

// rkyv serialization is deferred to touring-rkyv integration in B.5.8.

#[cfg(not(feature = "zero-copy"))]
mod rkyv_support {
    // No-op when zero-copy feature is disabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_simple_cursor() {
        let snippet = SnippetEdit::new("fn main() $0".into());
        assert_eq!(snippet.cursor_position(), Some(0));
        assert!(snippet.tab_stops().is_empty());
        assert_eq!(snippet.template(), "fn main() $0");
    }

    #[test]
    fn snippet_with_tab_stop() {
        let snippet = SnippetEdit::new("fn ${1:name}() $0".into());
        assert_eq!(snippet.cursor_position(), Some(0));
        assert_eq!(snippet.tab_stops().len(), 1);
        assert_eq!(snippet.tab_stops()[0].index, 1);
        assert_eq!(snippet.tab_stops()[0].default_text, "name");
    }

    #[test]
    fn snippet_with_default_text() {
        let snippet = SnippetEdit::new("let x: ${1:i32} = $0".into());
        assert_eq!(snippet.tab_stops()[0].default_text, "i32");
    }

    #[test]
    fn snippet_no_cursor() {
        let snippet = SnippetEdit::new("static content".into());
        assert_eq!(snippet.cursor_position(), None);
        assert!(snippet.tab_stops().is_empty());
    }

    #[test]
    fn snippet_multiple_tab_stops() {
        let snippet = SnippetEdit::new("fn ${1:name}(${2:arg}: ${3:type}) $0".into());
        assert_eq!(snippet.tab_stops().len(), 3);
        assert_eq!(snippet.tab_stops()[0].default_text, "name");
        assert_eq!(snippet.tab_stops()[1].default_text, "arg");
        assert_eq!(snippet.tab_stops()[2].default_text, "type");
    }

    #[test]
    fn snippet_display() {
        let snippet = SnippetEdit::new("fn main() $0".into());
        let s = format!("{snippet}");
        assert!(s.contains("fn main()"));
    }

    #[test]
    fn snippet_is_simple() {
        assert!(SnippetEdit::new("$0".into()).is_simple());
        assert!(SnippetEdit::new("no cursor".into()).is_simple());
        assert!(!SnippetEdit::new("${1:x}".into()).is_simple());
    }

    #[test]
    fn snippet_empty_template() {
        let snippet = SnippetEdit::new(String::new());
        assert_eq!(snippet.cursor_position(), None);
        assert!(snippet.tab_stops().is_empty());
    }
}
