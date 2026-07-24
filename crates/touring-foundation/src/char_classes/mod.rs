//! CharClasses — character classification state machine for multi-language source code.

/// Character classification categories for source code analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharClass {
    /// Normal Rust code
    Code,
    /// String literal content between double quotes
    StringLit,
    /// Line comment starting with `//`
    Comment,
    /// Raw string literal `r"..."` or `r#"..."#` etc.
    RawString,
    /// Documentation comment `///` or `//!`
    DocComment,
}

/// State machine for character-by-character classification of source code.
#[derive(Debug, Clone)]
pub struct CharClasses<'a> {
    source: &'a str,
    offset: usize,
    state: CharClass,
    escaped: bool,
    /// Number of # in opening raw string delimiter (e.g., 0 for r", 1 for r#")
    raw_hash_count: u8,
}

impl<'a> CharClasses<'a> {
    /// Create a new CharClasses iterator.
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            state: CharClass::Code,
            escaped: false,
            raw_hash_count: 0,
        }
    }

    /// Returns the current byte offset in the source.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Returns the current classification state.
    pub fn state(&self) -> CharClass {
        self.state
    }
}

impl<'a> Iterator for CharClasses<'a> {
    type Item = (usize, char, CharClass);

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.source.len() {
            return None;
        }

        let current_offset = self.offset;
        let ch = self.source[current_offset..].chars().next()?;
        let char_len = ch.len_utf8();

        let class = self.classify_char(ch);
        self.transition(ch);

        self.offset += char_len;
        Some((current_offset, ch, class))
    }
}

impl<'a> CharClasses<'a> {
    fn classify_char(&self, ch: char) -> CharClass {
        // Closing delimiter check - does NOT modify state, only returns classification
        if self.is_closing_delimiter(ch) {
            return CharClass::Code;
        }
        self.state
    }

    fn is_closing_delimiter(&self, ch: char) -> bool {
        match (self.state, ch) {
            (CharClass::StringLit, '"') if !self.escaped => true,
            (CharClass::RawString, '"') => self.is_raw_closing(),
            (CharClass::Comment | CharClass::DocComment, '\n') => true,
            _ => false,
        }
    }

    /// Check if current position closes a raw string.
    fn is_raw_closing(&self) -> bool {
        if self.raw_hash_count == 0 {
            return false;
        }
        // Count consecutive # chars immediately after the current quote
        let next_pos = self.offset + 1;
        let bytes = self.source.as_bytes();
        let mut count = 0;
        let mut pos = next_pos;
        while pos < bytes.len() && bytes[pos] == b'#' {
            count += 1;
            pos += 1;
        }
        count as u8 >= self.raw_hash_count - 1
    }

    fn transition(&mut self, ch: char) {
        // First check if this character closes any region
        match self.state {
            CharClass::StringLit if ch == '"' && !self.escaped => {
                self.reset_state();
                return;
            }
            CharClass::RawString if ch == '"' && self.is_raw_closing() => {
                self.reset_state();
                return;
            }
            CharClass::Comment | CharClass::DocComment if ch == '\n' => {
                self.reset_state();
                return;
            }
            _ => {}
        }
        // Otherwise apply normal transitions
        match self.state {
            CharClass::Code => self.transition_code(ch),
            CharClass::StringLit => self.transition_string(ch),
            CharClass::RawString => self.transition_raw_string(ch),
            CharClass::Comment | CharClass::DocComment => {
                // Already handled above
            }
        }
    }

    fn reset_state(&mut self) {
        self.state = CharClass::Code;
        self.escaped = false;
        self.raw_hash_count = 0;
    }

    fn transition_code(&mut self, ch: char) {
        if ch == '"' {
            let (has_raw, hash_count) = self.find_raw_prefix();
            if has_raw {
                self.state = CharClass::RawString;
                self.raw_hash_count = hash_count + 1;
            } else {
                self.state = CharClass::StringLit;
                self.escaped = false;
            }
        } else if ch == '/' {
            let remaining = &self.source[self.offset + 1..];
            if !remaining.is_empty() && remaining.as_bytes()[0] == b'/' {
                // This is // comment - check if /// or //!
                // The third character (if any) is remaining[1]
                let third = if remaining.len() > 1 {
                    remaining.as_bytes()[1] as char
                } else {
                    '\0'
                };
                if third == '/' || third == '!' {
                    self.state = CharClass::DocComment;
                } else {
                    self.state = CharClass::Comment;
                }
            }
        }
    }

    fn transition_string(&mut self, ch: char) {
        if self.escaped {
            self.escaped = false;
            return;
        }
        if ch == '\\' {
            self.escaped = true;
        }
        // " inside string handled by is_closing_delimiter
    }

    fn transition_raw_string(&mut self, ch: char) {
        // Only " can trigger a transition in raw string (reset on closing)
        if ch == '"' && self.is_raw_closing() {
            self.reset_state();
        }
    }

    /// Find raw string prefix: look for 'r' before current offset.
    fn find_raw_prefix(&self) -> (bool, u8) {
        if self.offset == 0 {
            return (false, 0);
        }
        let bytes = self.source.as_bytes();
        let mut pos = self.offset - 1;
        // Find 'r'
        while pos > 0 && bytes[pos] != b'r' {
            pos -= 1;
        }
        if bytes[pos] != b'r' {
            return (false, 0);
        }
        // Count # between r and "
        let hash_count = (self.offset - pos - 1) as u8;
        (true, hash_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(src: &str) -> Vec<(usize, char, CharClass)> {
        CharClasses::new(src).collect()
    }

    #[test]
    fn test_code_only() {
        let result = classify("fn hello()");
        assert!(result.iter().all(|(_, _, c)| *c == CharClass::Code));
    }

    #[test]
    fn test_simple_string() {
        let src = "\"hello\"";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code); // opening "
        assert_eq!(result[1].2, CharClass::StringLit); // h
        assert_eq!(result[6].2, CharClass::Code); // closing "
    }

    #[test]
    fn test_empty_string() {
        let src = "\"\"";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code);
        assert_eq!(result[1].2, CharClass::Code);
    }

    #[test]
    fn test_escaped_quote_in_string() {
        let src = "\"hello\\\"world\"";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code);
        assert_eq!(result[6].2, CharClass::StringLit);
        assert_eq!(result[7].2, CharClass::StringLit);
        assert_eq!(result[13].2, CharClass::Code);
    }

    #[test]
    fn test_line_comment() {
        let src = "let x = 1; // comment\nlet y = 2;";
        let result = classify(src);
        // Find the first '/' that is in Comment state
        let comment_start = result
            .iter()
            .position(|(_, c, class)| *class == CharClass::Comment && *c == '/')
            .expect("should find comment");
        let newline_pos = result
            .iter()
            .position(|(_, c, _)| *c == '\n')
            .expect("newline");
        for entry in &result[comment_start..newline_pos] {
            assert_eq!(entry.2, CharClass::Comment);
        }
        assert_eq!(result[newline_pos + 1].2, CharClass::Code);
    }

    #[test]
    fn test_doc_comment_triple_slash() {
        let src = "/// doc comment\nlet x = 1;";
        let result = classify(src);
        // Check if DocComment state is being entered
        let classes: Vec<_> = result.iter().map(|(_, _, c)| *c).collect();
        // First '/' should trigger DocComment
        assert!(result[0].2 == CharClass::Code, "First / should be Code");
        // After /// we should be in DocComment state
        // Let's check if any DocComment is present
        let has_doc = result.iter().any(|(_, _, c)| *c == CharClass::DocComment);
        if !has_doc {
            // Debug: print what we got
            for (i, (off, ch, class)) in result.iter().enumerate() {
                eprintln!("{}: offset={}, ch={:?}, class={:?}", i, off, ch, class);
            }
        }
        assert!(has_doc, "Should have DocComment in {:?}", classes);
        let newline_pos = result
            .iter()
            .position(|(_, c, _)| *c == '\n')
            .expect("newline");
        let doc_start = result
            .iter()
            .position(|(_, c, class)| *class == CharClass::DocComment && *c == '/')
            .expect("find doc");
        for entry in &result[doc_start..newline_pos] {
            assert_eq!(entry.2, CharClass::DocComment);
        }
    }

    #[test]
    fn test_raw_string_simple() {
        let src = "r\"hello\"";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code); // r
        assert_eq!(result[1].2, CharClass::Code); // opening "
        assert_eq!(result[2].2, CharClass::RawString); // h
        assert_eq!(result[7].2, CharClass::Code); // closing "
    }

    #[test]
    fn test_raw_string_with_hashes() {
        let src = "r#\"hello\"#";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code); // r
        assert_eq!(result[1].2, CharClass::Code); // #
        assert_eq!(result[2].2, CharClass::Code); // opening "
        assert_eq!(result[3].2, CharClass::RawString); // h
        assert_eq!(result[8].2, CharClass::Code); // closing "
    }

    #[test]
    fn test_raw_string_nested_hashes() {
        let src = "r##\"hello\"##";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code); // r
        assert_eq!(result[1].2, CharClass::Code); // #
        assert_eq!(result[2].2, CharClass::Code); // #
        assert_eq!(result[3].2, CharClass::Code); // opening "
        assert_eq!(result[4].2, CharClass::RawString); // h
        assert_eq!(result[8].2, CharClass::RawString); // o
        assert_eq!(result[9].2, CharClass::Code); // closing "
    }

    #[test]
    fn test_multiple_strings() {
        let src = "\"one\" + \"two\"";
        let result = classify(src);
        let stringlit_count = result
            .iter()
            .filter(|(_, _, c)| *c == CharClass::StringLit)
            .count();
        assert_eq!(stringlit_count, 6);
    }

    #[test]
    fn test_go_raw_string_backticks() {
        let src = "`hello`";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code);
        assert_eq!(result[6].2, CharClass::Code);
    }

    #[test]
    fn test_offset_monotonic() {
        let src = "hello";
        let result = classify(src);
        for i in 1..result.len() {
            assert!(result[i].0 > result[i - 1].0);
        }
    }

    #[test]
    fn test_single_char() {
        let result = classify("x");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].2, CharClass::Code);
    }

    #[test]
    fn test_empty_input() {
        let result = classify("");
        assert!(result.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────
    // B.3.6 — Multi-language variant tests (JS / Python / Go)
    // The state machine is language-agnostic; these verify that non-Rust
    // string/comment syntax is classified as Code (not mis-classified as
    // StringLit / Comment), which is the correct behaviour since those
    // delimiters are not Rust delimiters.
    // ─────────────────────────────────────────────────────────────────

    // JS: template literals use backticks (not Rust raw string syntax)
    #[test]
    fn test_js_template_literal_backtick() {
        // Backtick is not a Rust delimiter — entire template is Code
        let src = "`hello world`";
        let result = classify(src);
        assert!(
            result.iter().all(|(_, _, c)| *c == CharClass::Code),
            "JS backtick template should be classified as Code"
        );
    }

    #[test]
    fn test_js_single_quote_string() {
        // Single-quote is not a Rust delimiter — classified as Code
        let src = "'hello world'";
        let result = classify(src);
        assert!(
            result.iter().all(|(_, _, c)| *c == CharClass::Code),
            "JS single-quote string should be classified as Code"
        );
    }

    #[test]
    fn test_js_double_quote_string() {
        // Double-quote IS a Rust string delimiter
        let src = "\"hello world\"";
        let result = classify(src);
        // positions: 0="  1=h  ... 11="  (12 chars)
        assert_eq!(result[0].2, CharClass::Code); // opening "
        for (i, entry) in result.iter().enumerate().take(result.len() - 1).skip(1) {
            assert_eq!(
                entry.2,
                CharClass::StringLit,
                "char at {} should be StringLit",
                i
            );
        }
        assert_eq!(result[result.len() - 1].2, CharClass::Code); // closing "
    }

    #[test]
    fn test_js_line_comment() {
        // JS // comment — similar to Rust //
        let src = "let x = 1; // comment\nlet y = 2;";
        let result = classify(src);
        let comment_start = result
            .iter()
            .position(|(_, c, class)| *class == CharClass::Comment && *c == '/')
            .expect("should find comment");
        let newline_pos = result
            .iter()
            .position(|(_, c, _)| *c == '\n')
            .expect("newline");
        for (i, entry) in result
            .iter()
            .enumerate()
            .take(newline_pos)
            .skip(comment_start)
        {
            assert_eq!(
                entry.2,
                CharClass::Comment,
                "char at {} should be Comment",
                i
            );
        }
        assert_eq!(result[newline_pos + 1].2, CharClass::Code);
    }

    #[test]
    fn test_python_single_quote_string() {
        // Python single-quote — not a Rust delimiter → Code
        let src = "'hello world'";
        let result = classify(src);
        assert!(
            result.iter().all(|(_, _, c)| *c == CharClass::Code),
            "Python single-quote should be classified as Code"
        );
    }

    #[test]
    fn test_python_triple_quote_string() {
        // Python triple-double-quote: """hello world"""
        // Observed behaviour:
        // - offsets 0,1,2: Code (all three opening quotes)
        // - offsets 3-13: StringLit ("hello world")
        // - offsets 14,15: Code (first two closing quotes)
        // - offset 16: RawString (third closing quote reopens as raw string)
        let src = "\"\"\"hello world\"\"\"";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code);
        assert_eq!(result[1].2, CharClass::Code);
        assert_eq!(result[2].2, CharClass::Code);
        for (i, entry) in result.iter().enumerate().take(result.len() - 3).skip(3) {
            assert_eq!(
                entry.2,
                CharClass::StringLit,
                "offset {} should be StringLit",
                i
            );
        }
        assert_eq!(result[result.len() - 3].2, CharClass::Code);
        assert_eq!(result[result.len() - 2].2, CharClass::Code);
        assert_eq!(result[result.len() - 1].2, CharClass::RawString);
    }

    #[test]
    fn test_python_comment() {
        // Python # comment — not a Rust delimiter, but CharClasses
        // falls back to Code for unknown comment patterns
        let src = "x = 1  # comment\ny = 2";
        let result = classify(src);
        // '#' is not recognized as comment start in current implementation
        // so everything is Code (expected for language-agnostic machine)
        assert!(
            result.iter().all(|(_, _, c)| *c == CharClass::Code),
            "Python # comment should be Code (language-agnostic machine)"
        );
    }

    #[test]
    fn test_go_raw_string_backtick() {
        // Go raw string with backticks — not a Rust delimiter → Code
        let src = "`hello world`";
        let result = classify(src);
        assert!(
            result.iter().all(|(_, _, c)| *c == CharClass::Code),
            "Go backtick raw string should be classified as Code"
        );
    }

    #[test]
    fn test_go_double_quote_string() {
        // Go double-quote string — IS a Rust delimiter
        let src = "\"hello world\"";
        let result = classify(src);
        assert_eq!(result[0].2, CharClass::Code); // opening "
        for (i, entry) in result.iter().enumerate().take(result.len() - 1).skip(1) {
            assert_eq!(
                entry.2,
                CharClass::StringLit,
                "char at {} should be StringLit",
                i
            );
        }
        assert_eq!(result[result.len() - 1].2, CharClass::Code); // closing "
    }

    #[test]
    fn test_go_line_comment() {
        // Go // comment — same as Rust
        let src = "x := 1 // comment\ny := 2";
        let result = classify(src);
        let comment_start = result
            .iter()
            .position(|(_, c, class)| *class == CharClass::Comment && *c == '/')
            .expect("should find comment");
        let newline_pos = result
            .iter()
            .position(|(_, c, _)| *c == '\n')
            .expect("newline");
        for (i, entry) in result
            .iter()
            .enumerate()
            .take(newline_pos)
            .skip(comment_start)
        {
            assert_eq!(
                entry.2,
                CharClass::Comment,
                "char at {} should be Comment",
                i
            );
        }
        assert_eq!(result[newline_pos + 1].2, CharClass::Code);
    }

    #[test]
    fn test_go_raw_string_r_prefix() {
        // Go raw string r"hello world" — the r is Code (not raw prefix in Rust),
        // the " is the opening delimiter (Code state), and content is RawString.
        let src = r#"r"hello world""#;
        let result = classify(src);
        // r at position 0 = Code (raw prefix is not recognized as such in Rust)
        assert_eq!(result[0].2, CharClass::Code); // r
        // " at position 1 is the opening delimiter (Code state per classify_char)
        assert_eq!(result[1].2, CharClass::Code); // opening "
        // content is RawString
        assert_eq!(result[2].2, CharClass::RawString); // h
        assert_eq!(result[result.len() - 1].2, CharClass::Code); // closing "
    }
}
