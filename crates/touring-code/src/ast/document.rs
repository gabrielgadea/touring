//! RopeDocument — maintains a Rope text buffer with O(log N) edits.
//! Provides byte-char conversion for tree-sitter integration.
//!
//! Tree-sitter operates on byte offsets; Ropey internally uses char indices.
//! This module bridges the two, enabling incremental reparsing after edits
//! without copying the entire source string.

use ropey::Rope;
use tracing::warn;

/// A text document backed by a Rope for efficient editing.
///
/// Supports O(log N) insertions and deletions, O(1) line count.
/// Produces [`tree_sitter::InputEdit`] values that can be fed directly
/// to `Tree::edit` for incremental reparsing.
#[derive(Debug)]
pub struct RopeDocument {
    rope: Rope,
    path: String,
}

impl RopeDocument {
    /// Create a new document from string content.
    pub fn new(path: &str, content: &str) -> Self {
        Self {
            rope: Rope::from_str(content),
            path: path.to_string(),
        }
    }

    /// Create from file on disk.
    ///
    /// # Errors
    /// Returns `std::io::Error` if the file cannot be read.
    pub fn from_file(path: &str) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::new(path, &content))
    }

    /// Get the document path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get total byte length.
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Get total char count.
    #[must_use]
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Get line count. O(1).
    #[must_use]
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Check if document is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rope.len_bytes() == 0
    }

    /// Convert byte offset to char offset. O(log N).
    ///
    /// # Panics
    /// Panics if `byte_idx` is out of bounds or not on a char boundary.
    #[must_use]
    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.rope.byte_to_char(byte_idx)
    }

    /// Convert char offset to byte offset. O(log N).
    ///
    /// # Panics
    /// Panics if `char_idx` is out of bounds.
    #[must_use]
    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.rope.char_to_byte(char_idx)
    }

    /// Convert byte offset to `(row, col_bytes)` for tree-sitter `Point`.
    ///
    /// Row and column are zero-based. Column is in bytes (not chars),
    /// matching tree-sitter's expectation.
    #[must_use]
    pub fn byte_to_point(&self, byte_idx: usize) -> (usize, usize) {
        let char_idx = self.rope.byte_to_char(byte_idx);
        let line = self.rope.char_to_line(char_idx);
        let line_start_char = self.rope.line_to_char(line);
        let line_start_byte = self.rope.char_to_byte(line_start_char);
        (line, byte_idx - line_start_byte)
    }

    /// Safe variant of [`byte_to_point`](Self::byte_to_point) that returns `None`
    /// for out-of-bounds byte indices instead of panicking.
    ///
    /// Use this in external-facing code where the byte offset may be untrusted.
    /// For internal, pre-validated offsets prefer [`byte_to_point`](Self::byte_to_point).
    #[must_use]
    pub fn byte_to_point_safe(&self, byte_idx: usize) -> Option<(usize, usize)> {
        let char_idx = self.rope.try_byte_to_char(byte_idx).ok()?;
        let line = self.rope.try_char_to_line(char_idx).ok()?;
        let line_start_char = self.rope.try_line_to_char(line).ok()?;
        let line_start_byte = self.rope.try_char_to_byte(line_start_char).ok()?;
        Some((line, byte_idx - line_start_byte))
    }

    /// Apply an edit: replace byte range `[start_byte..old_end_byte)` with `new_text`.
    ///
    /// Returns the [`tree_sitter::InputEdit`] describing the change, suitable
    /// for passing to `Tree::edit` before incremental reparsing.
    ///
    /// # Sprint 4.6 cross-audit hardening (2026-05-24, REGRA #0 potencializar)
    ///
    /// External byte offsets may be stale relative to current rope state —
    /// for instance, an offset computed against a previous revision of the
    /// document and applied after the underlying buffer shrank (cross-actor
    /// coordination, deferred application of queued edits). Previously,
    /// `byte_to_char(offset)` panicked on OOB and killed the
    /// `touring-project-actor` thread, terminating the daemon.
    ///
    /// We now clamp `start_byte` and `old_end_byte` to `rope.len_bytes()`
    /// before any rope operation, emitting a `tracing::warn` when clamping
    /// fires so the upstream coordination bug remains diagnosable. The
    /// clamp preserves the InputEdit contract: a no-op edit at end-of-rope
    /// is well-defined and tree-sitter accepts it.
    pub fn edit(
        &mut self,
        start_byte: usize,
        old_end_byte: usize,
        new_text: &str,
    ) -> tree_sitter::InputEdit {
        // Defensive clamp: external offsets may be stale (cross-actor race,
        // queued-after-mutation). Without this, byte_to_char panics and
        // kills the touring-project-actor → daemon dies (Sprint 4.6 bug).
        let len_bytes = self.rope.len_bytes();
        let safe_start = start_byte.min(len_bytes);
        // old_end must be >= start AND <= len_bytes
        let safe_old_end = old_end_byte.min(len_bytes).max(safe_start);
        if start_byte > len_bytes || old_end_byte > len_bytes {
            warn!(
                target: "touring_code::ast::document",
                rope_len_bytes = len_bytes,
                stale_start_byte = start_byte,
                stale_old_end_byte = old_end_byte,
                clamped_start = safe_start,
                clamped_old_end = safe_old_end,
                "Document::edit received OOB byte offset; clamped to rope length to prevent panic. \
                 Upstream coordination bug likely — investigate stale offset provenance."
            );
        }
        let start_byte = safe_start;
        let old_end_byte = safe_old_end;

        // Capture positions BEFORE mutation (clamped — guaranteed in-bounds)
        let start_char = self.rope.byte_to_char(start_byte);
        let old_end_char = self.rope.byte_to_char(old_end_byte);
        let (start_row, start_col) = self.byte_to_point(start_byte);
        let (old_end_row, old_end_col) = self.byte_to_point(old_end_byte);

        // Remove old content
        if old_end_char > start_char {
            self.rope.remove(start_char..old_end_char);
        }

        // Insert new content
        if !new_text.is_empty() {
            self.rope.insert(start_char, new_text);
        }

        // Compute new end position AFTER mutation
        let new_end_byte = start_byte + new_text.len();
        let clamped = new_end_byte.min(self.rope.len_bytes());
        let (new_end_row, new_end_col) = self.byte_to_point(clamped);

        tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: tree_sitter::Point::new(start_row, start_col),
            old_end_position: tree_sitter::Point::new(old_end_row, old_end_col),
            new_end_position: tree_sitter::Point::new(new_end_row, new_end_col),
        }
    }

    /// Get the full content as a `String`.
    #[must_use]
    pub fn content(&self) -> String {
        self.rope.to_string()
    }

    /// Get a slice of the content by byte range `[start..end)`.
    ///
    /// Sprint 4.6 hardening (2026-05-24): OOB byte indices are clamped to
    /// `rope.len_bytes()` instead of panicking, matching `Document::edit`
    /// defensive policy. Non-char-boundary indices still panic (that is a
    /// genuine programmer error, not stale-offset coordination).
    #[must_use]
    pub fn slice_bytes(&self, start: usize, end: usize) -> String {
        let len_bytes = self.rope.len_bytes();
        let safe_start = start.min(len_bytes);
        let safe_end = end.min(len_bytes).max(safe_start);
        if start > len_bytes || end > len_bytes {
            warn!(
                target: "touring_code::ast::document",
                rope_len_bytes = len_bytes,
                stale_start = start,
                stale_end = end,
                clamped_start = safe_start,
                clamped_end = safe_end,
                "Document::slice_bytes received OOB byte offset; clamped to rope length."
            );
        }
        let start_char = self.rope.byte_to_char(safe_start);
        let end_char = self.rope.byte_to_char(safe_end);
        self.rope.slice(start_char..end_char).to_string()
    }

    /// Read a contiguous byte chunk starting at `byte_offset`.
    ///
    /// Returns a byte slice from the underlying rope chunk, suitable
    /// for use as a tree-sitter parse callback via [`Self::chunk_at_byte`].
    /// Returns an empty slice if `byte_offset >= len_bytes()`.
    #[must_use]
    pub fn chunk_at_byte(&self, byte_offset: usize) -> &[u8] {
        if byte_offset >= self.rope.len_bytes() {
            return &[];
        }
        let char_idx = self.rope.byte_to_char(byte_offset);
        let (chunk_str, chunk_start_char, _, _) = self.rope.chunk_at_char(char_idx);
        let chunk_start_byte = self.rope.char_to_byte(chunk_start_char);
        let offset_in_chunk = byte_offset - chunk_start_byte;
        chunk_str
            .as_bytes()
            .get(offset_in_chunk..)
            .unwrap_or_default()
    }
}

impl std::fmt::Display for RopeDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RopeDocument({}, {} bytes)", self.path, self.len_bytes())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs are asserted non-empty before indexing
    use super::*;

    // ── Construction ────────────────────────────────────────────────────

    #[test]
    fn test_new_from_content() {
        let doc = RopeDocument::new("test.py", "hello world\n");
        assert_eq!(doc.len_bytes(), 12);
        assert_eq!(doc.len_chars(), 12);
        assert_eq!(doc.len_lines(), 2); // "hello world\n" = line 0 + trailing empty
        assert_eq!(doc.path(), "test.py");
        assert!(!doc.is_empty());
    }

    #[test]
    fn test_empty_document() {
        let doc = RopeDocument::new("empty.py", "");
        assert!(doc.is_empty());
        assert_eq!(doc.len_bytes(), 0);
        assert_eq!(doc.len_chars(), 0);
        assert_eq!(doc.len_lines(), 1); // ropey: empty string = 1 line
    }

    #[test]
    fn test_from_file_nonexistent() {
        let result = RopeDocument::from_file("/nonexistent/path.py");
        assert!(result.is_err());
    }

    // ── Byte/Char conversion (ASCII) ────────────────────────────────────

    #[test]
    fn test_byte_to_char_ascii() {
        let doc = RopeDocument::new("t.py", "hello world");
        assert_eq!(doc.byte_to_char(0), 0);
        assert_eq!(doc.byte_to_char(5), 5);
        assert_eq!(doc.byte_to_char(11), 11);
    }

    #[test]
    fn test_char_to_byte_ascii() {
        let doc = RopeDocument::new("t.py", "hello world");
        assert_eq!(doc.char_to_byte(0), 0);
        assert_eq!(doc.char_to_byte(5), 5);
    }

    // ── Byte/Char conversion (Unicode) ──────────────────────────────────

    #[test]
    fn test_byte_char_unicode() {
        // "análise" has multi-byte chars: á = 2 bytes UTF-8
        let doc = RopeDocument::new("t.py", "análise técnica");
        // Verify byte index of 't' in "técnica"
        let content = doc.content();
        let byte_of_t = content.find('t').expect("'t' must exist");
        let char_of_t = doc.byte_to_char(byte_of_t);
        // Multi-byte chars mean char_of_t <= byte_of_t
        assert!(
            char_of_t <= byte_of_t,
            "char index ({char_of_t}) should be <= byte index ({byte_of_t}) with multi-byte chars"
        );
        // Round-trip: char_to_byte(byte_to_char(b)) == b
        assert_eq!(doc.char_to_byte(char_of_t), byte_of_t);
    }

    // ── byte_to_point ───────────────────────────────────────────────────

    #[test]
    fn test_byte_to_point_multiline() {
        let doc = RopeDocument::new("t.py", "line1\nline2\nline3\n");
        assert_eq!(doc.byte_to_point(0), (0, 0)); // start of line 0
        assert_eq!(doc.byte_to_point(6), (1, 0)); // start of line 1
        assert_eq!(doc.byte_to_point(12), (2, 0)); // start of line 2
        assert_eq!(doc.byte_to_point(8), (1, 2)); // 2 bytes into line 1
    }

    #[test]
    fn test_byte_to_point_single_line() {
        let doc = RopeDocument::new("t.py", "abcdef");
        assert_eq!(doc.byte_to_point(0), (0, 0));
        assert_eq!(doc.byte_to_point(3), (0, 3));
    }

    // ── Edit: replacement ───────────────────────────────────────────────

    #[test]
    fn test_edit_same_length_replacement() {
        let mut doc = RopeDocument::new("t.py", "def foo():\n    pass\n");
        // Replace "foo" (bytes 4..7) with "bar"
        let edit = doc.edit(4, 7, "bar");
        assert_eq!(edit.start_byte, 4);
        assert_eq!(edit.old_end_byte, 7);
        assert_eq!(edit.new_end_byte, 7); // same length
        assert_eq!(doc.content(), "def bar():\n    pass\n");
    }

    #[test]
    fn test_edit_longer_replacement() {
        let mut doc = RopeDocument::new("t.py", "def foo():\n    pass\n");
        // Replace "foo" (bytes 4..7) with "longer_name"
        let edit = doc.edit(4, 7, "longer_name");
        assert_eq!(edit.start_byte, 4);
        assert_eq!(edit.old_end_byte, 7);
        assert_eq!(edit.new_end_byte, 4 + 11); // "longer_name".len() == 11
        assert_eq!(doc.content(), "def longer_name():\n    pass\n");
    }

    // ── Edit: insertion ─────────────────────────────────────────────────

    #[test]
    fn test_edit_insertion() {
        let mut doc = RopeDocument::new("t.py", "def foo():\n    pass\n");
        // Insert "name, " at byte 8 (between parentheses: just before ')')
        let edit = doc.edit(8, 8, "name, ");
        assert_eq!(edit.start_byte, 8);
        assert_eq!(edit.old_end_byte, 8);
        assert_eq!(edit.new_end_byte, 14);
        assert_eq!(doc.content(), "def foo(name, ):\n    pass\n");
    }

    // ── Edit: deletion ──────────────────────────────────────────────────

    #[test]
    fn test_edit_deletion() {
        let mut doc = RopeDocument::new("t.py", "def foo(x):\n    pass\n");
        // Delete "x" at bytes 8..9
        let edit = doc.edit(8, 9, "");
        assert_eq!(edit.start_byte, 8);
        assert_eq!(edit.old_end_byte, 9);
        assert_eq!(edit.new_end_byte, 8);
        assert_eq!(doc.content(), "def foo():\n    pass\n");
    }

    // ── Edit: positions ─────────────────────────────────────────────────

    #[test]
    fn test_edit_start_position() {
        let mut doc = RopeDocument::new("t.py", "def foo():\n    pass\n");
        let edit = doc.edit(4, 7, "bar");
        // "foo" starts at line 0, col 4
        assert_eq!(edit.start_position, tree_sitter::Point::new(0, 4));
    }

    #[test]
    fn test_edit_multiline_position() {
        let mut doc = RopeDocument::new("t.py", "line1\nline2\nline3\n");
        // Replace "line2" (bytes 6..11) with "replaced"
        let edit = doc.edit(6, 11, "replaced");
        assert_eq!(edit.start_position, tree_sitter::Point::new(1, 0));
        assert_eq!(edit.old_end_position, tree_sitter::Point::new(1, 5));
        assert_eq!(doc.content(), "line1\nreplaced\nline3\n");
    }

    // ── Edit: content integrity ─────────────────────────────────────────

    #[test]
    fn test_edit_preserves_content_integrity() {
        let mut doc = RopeDocument::new("t.py", "abcdefghij");
        doc.edit(3, 6, "XYZ"); // replace "def" with "XYZ"
        assert_eq!(doc.content(), "abcXYZghij");
        assert_eq!(doc.len_bytes(), 10);
    }

    #[test]
    fn test_sequential_edits() {
        let mut doc = RopeDocument::new("t.py", "hello world");
        doc.edit(0, 5, "goodbye"); // "hello" -> "goodbye"
        assert_eq!(doc.content(), "goodbye world");
        doc.edit(8, 13, "earth"); // "world" -> "earth"
        assert_eq!(doc.content(), "goodbye earth");
    }

    // ── slice_bytes ─────────────────────────────────────────────────────

    #[test]
    fn test_slice_bytes_ascii() {
        let doc = RopeDocument::new("t.py", "hello world");
        assert_eq!(doc.slice_bytes(0, 5), "hello");
        assert_eq!(doc.slice_bytes(6, 11), "world");
    }

    #[test]
    fn test_slice_bytes_unicode() {
        let doc = RopeDocument::new("t.py", "olá mundo");
        // "olá" = 4 bytes (á = 2 bytes), so bytes 0..4 = "olá"
        assert_eq!(doc.slice_bytes(0, 4), "olá");
    }

    // ── chunk_at_byte ────────────────────────────────────────────────────

    #[test]
    fn test_chunk_at_byte_start() {
        let doc = RopeDocument::new("t.py", "hello world");
        let result = doc.chunk_at_byte(0);
        assert!(!result.is_empty());
        assert_eq!(result[0], b'h');
    }

    #[test]
    fn test_chunk_at_byte_mid() {
        let doc = RopeDocument::new("t.py", "hello world");
        let result = doc.chunk_at_byte(6);
        assert!(!result.is_empty());
        assert_eq!(result[0], b'w');
    }

    #[test]
    fn test_chunk_at_byte_past_end() {
        let doc = RopeDocument::new("t.py", "hi");
        let result = doc.chunk_at_byte(100);
        assert!(result.is_empty());
    }

    #[test]
    fn test_chunk_at_byte_exact_end() {
        let doc = RopeDocument::new("t.py", "hi");
        let result = doc.chunk_at_byte(2); // len_bytes() == 2
        assert!(result.is_empty());
    }

    // ── byte_to_point_safe ─────────────────────────────────────────────

    #[test]
    fn test_byte_to_point_safe_out_of_bounds() {
        let doc = RopeDocument::new("t.py", "hello");
        assert!(doc.byte_to_point_safe(1000).is_none());
        assert!(doc.byte_to_point_safe(6).is_none()); // len_bytes() == 5
    }

    #[test]
    fn test_byte_to_point_safe_valid() {
        let doc = RopeDocument::new("t.py", "hello");
        assert_eq!(doc.byte_to_point_safe(0), Some((0, 0)));
        assert_eq!(doc.byte_to_point_safe(3), Some((0, 3)));
        assert_eq!(doc.byte_to_point_safe(5), Some((0, 5))); // end-of-string is valid
    }

    #[test]
    fn test_byte_to_point_safe_multiline() {
        let doc = RopeDocument::new("t.py", "line1\nline2\nline3\n");
        assert_eq!(doc.byte_to_point_safe(0), Some((0, 0)));
        assert_eq!(doc.byte_to_point_safe(6), Some((1, 0)));
        assert_eq!(doc.byte_to_point_safe(12), Some((2, 0)));
        assert_eq!(doc.byte_to_point_safe(8), Some((1, 2)));
        // end of content
        assert_eq!(doc.byte_to_point_safe(18), Some((3, 0)));
        // past end
        assert!(doc.byte_to_point_safe(19).is_none());
    }

    #[test]
    fn test_byte_to_point_safe_empty_document() {
        let doc = RopeDocument::new("t.py", "");
        assert_eq!(doc.byte_to_point_safe(0), Some((0, 0)));
        assert!(doc.byte_to_point_safe(1).is_none());
    }

    #[test]
    fn test_byte_to_point_safe_unicode() {
        // "olá" = 4 bytes (á = 2 bytes UTF-8)
        let doc = RopeDocument::new("t.py", "olá");
        assert_eq!(doc.byte_to_point_safe(0), Some((0, 0)));
        assert_eq!(doc.byte_to_point_safe(4), Some((0, 4))); // end of "olá"
        assert!(doc.byte_to_point_safe(5).is_none());
    }

    #[test]
    fn test_byte_to_point_safe_agrees_with_byte_to_point() {
        let doc = RopeDocument::new("t.py", "line1\nline2\nline3\n");
        for byte_idx in 0..=doc.len_bytes() {
            let safe = doc.byte_to_point_safe(byte_idx);
            let direct = doc.byte_to_point(byte_idx);
            assert_eq!(safe, Some(direct), "Mismatch at byte_idx={byte_idx}");
        }
    }

    // ── Display ─────────────────────────────────────────────────────────

    #[test]
    fn test_display() {
        let doc = RopeDocument::new("test.py", "hello");
        let s = format!("{doc}");
        assert!(s.contains("test.py"));
        assert!(s.contains("5 bytes"));
    }

    // ── Sprint 4.6 cross-audit hardening (REGRA #0) ─────────────────────
    //
    // Original bug: thread `touring-project-actor` died with
    //   "byte index 84877, Rope/RopeSlice byte length 84279"
    // because Document::edit() forwarded a stale external byte_idx to
    // rope.byte_to_char which panics on OOB. The fix clamps the inputs
    // and emits tracing::warn. These tests prove the panic is gone.

    #[test]
    fn sprint_4_6_edit_clamps_oob_start_byte_no_panic() {
        let mut doc = RopeDocument::new("t.py", "hello");
        // start_byte 1000 vs rope len 5 — stale offset scenario
        let edit = doc.edit(1000, 1000, "world");
        // Clamp pushes both to len_bytes=5 → equivalent to insertion at end
        assert_eq!(edit.start_byte, 5);
        assert_eq!(edit.old_end_byte, 5);
        assert_eq!(doc.content(), "helloworld");
    }

    #[test]
    fn sprint_4_6_edit_clamps_oob_old_end_byte_no_panic() {
        let mut doc = RopeDocument::new("t.py", "hello");
        // start in-bounds, old_end OOB — stale-after-shrink scenario
        let edit = doc.edit(2, 9999, "Y");
        // old_end clamped to 5, start preserved at 2
        assert_eq!(edit.start_byte, 2);
        assert_eq!(edit.old_end_byte, 5);
        // Resulting text: "he" + "Y" = "heY"
        assert_eq!(doc.content(), "heY");
    }

    #[test]
    fn sprint_4_6_edit_clamps_inverted_oob_no_panic() {
        // both OOB AND inverted (old_end < start when clamped to same value)
        let mut doc = RopeDocument::new("t.py", "abc");
        let edit = doc.edit(100, 50, "Z");
        // safe_old_end.max(safe_start) keeps order invariant
        assert_eq!(edit.start_byte, 3);
        assert_eq!(edit.old_end_byte, 3);
        assert_eq!(doc.content(), "abcZ");
    }

    #[test]
    fn sprint_4_6_slice_bytes_clamps_oob_no_panic() {
        let doc = RopeDocument::new("t.py", "hello");
        // start in-bounds, end OOB
        assert_eq!(doc.slice_bytes(2, 9999), "llo");
        // Both OOB
        assert_eq!(doc.slice_bytes(100, 200), "");
        // Inverted OOB
        assert_eq!(doc.slice_bytes(9999, 50), "");
    }

    #[test]
    fn sprint_4_6_edit_inbounds_path_unchanged() {
        // Regression: clamp logic must NOT change behavior for in-bounds inputs.
        let mut doc = RopeDocument::new("t.py", "hello world");
        let edit = doc.edit(6, 11, "Rust");
        assert_eq!(edit.start_byte, 6);
        assert_eq!(edit.old_end_byte, 11);
        assert_eq!(doc.content(), "hello Rust");
    }
}
