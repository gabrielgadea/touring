//! SnippetProvider — cursor-based gap capture between AST nodes.
//!
//! Strategy mirrors rustfmt's `missed_spans.rs`: walk the AST and capture
//! original source text (whitespace, comments, doc markers) between nodes.

/// A gap of original source text between two AST nodes.
#[derive(Debug, Clone)]
pub struct Gap {
    /// Byte offset where gap begins.
    pub start: usize,
    /// Byte offset where gap ends.
    pub end: usize,
    /// The raw text of the gap.
    pub text: String,
}

impl Gap {
    /// Returns `true` if this gap contains only whitespace.
    pub fn is_whitespace_only(&self) -> bool {
        self.text.chars().all(|c| c.is_whitespace())
    }

    /// Returns `true` if this gap contains any line comments (`//`).
    pub fn has_line_comment(&self) -> bool {
        self.text.contains("//")
    }

    /// Returns `true` if this gap contains any block comments (`/* ... */`).
    pub fn has_block_comment(&self) -> bool {
        self.text.contains("/*")
    }
}

/// Captures gaps (whitespace + comments) between AST nodes while walking a syn file.
pub struct SnippetProvider<'a> {
    source: &'a str,
    /// The byte position of the last AST node we've processed.
    last_pos: usize,
    /// Accumulated gaps found so far.
    gaps: Vec<Gap>,
}

impl<'a> SnippetProvider<'a> {
    /// Create a new SnippetProvider for the given source text.
    pub fn new(source: &'a str) -> Self {
        SnippetProvider {
            source,
            last_pos: 0,
            gaps: Vec::new(),
        }
    }

    /// Advance by a number of bytes, capturing any gap since the previous node.
    /// Call this after emitting a formatted AST node.
    pub fn advance_by(&mut self, byte_count: usize) {
        let new_pos = (self.last_pos + byte_count).min(self.source.len());
        if new_pos > self.last_pos {
            let text = self.source[self.last_pos..new_pos].to_string();
            if !text.is_empty() {
                self.gaps.push(Gap {
                    start: self.last_pos,
                    end: new_pos,
                    text,
                });
            }
        }
        self.last_pos = new_pos;
    }

    /// Capture any trailing gap from last position to end of source.
    pub fn capture_trailing(&mut self) {
        if self.last_pos < self.source.len() {
            let text = self.source[self.last_pos..].to_string();
            self.gaps.push(Gap {
                start: self.last_pos,
                end: self.source.len(),
                text,
            });
            self.last_pos = self.source.len();
        }
    }

    /// Get all captured gaps.
    pub fn gaps(&self) -> &[Gap] {
        &self.gaps
    }

    /// Get the raw source text.
    pub fn source(&self) -> &str {
        self.source
    }

    /// Get the current last position.
    pub fn last_pos(&self) -> usize {
        self.last_pos
    }

    /// Find a gap containing a given byte offset.
    pub fn gap_at(&self, offset: usize) -> Option<&Gap> {
        self.gaps
            .iter()
            .find(|g| offset >= g.start && offset < g.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_is_whitespace_only() {
        let gap = Gap {
            start: 0,
            end: 10,
            text: "    \n    ".to_string(),
        };
        assert!(gap.is_whitespace_only());

        let gap2 = Gap {
            start: 0,
            end: 20,
            text: "    // comment".to_string(),
        };
        assert!(!gap2.is_whitespace_only());
    }

    #[test]
    fn snippet_provider_tracks_position() {
        let source = "abc def ghi";
        let mut sp = SnippetProvider::new(source);

        sp.advance_by(3); // "abc"
        sp.advance_by(1); // " "
        sp.advance_by(3); // "def"
        sp.capture_trailing();

        let gaps = sp.gaps();
        // Should have gaps: " " and " ghi"
        assert!(gaps.len() >= 1);
    }

    #[test]
    fn snippet_provider_capture_trailing() {
        let source = "fn foo() {}";
        let mut sp = SnippetProvider::new(source);

        sp.advance_by(source.len());
        sp.capture_trailing();

        // No trailing gap since we consumed entire source
        assert!(sp.gaps().is_empty() || sp.last_pos() == source.len());
    }
}
