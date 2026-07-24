//! Diff-based [`tree_sitter::InputEdit`] synthesis.
//!
//! Sentrux Master Plan Wave 3 P6 (2026-05-09). The
//! `crate::parser::IncrementalParser` already supports incremental
//! re-parsing — but the public API
//! `crate::parser::IncrementalParser::parse_incremental` requires the
//! caller to provide a fully-formed [`tree_sitter::InputEdit`] describing
//! exactly which byte range changed. That is unergonomic when all the
//! caller has is `(old_source, new_source)` (e.g. "I just received a
//! file-watcher event").
//!
//! This module closes that gap with two pure helpers:
//!
//! * `compute_input_edit_from_diff` — given `(old, new)`, walks the
//!   strings to find the longest common prefix and longest common
//!   suffix, then returns the minimal [`tree_sitter::InputEdit`]
//!   describing the change (or `None` when `old == new`).
//! * [`IncrementalParser::parse_incremental_auto`] (extension impl) —
//!   convenience wrapper that takes `(path, new_source, old_source)`,
//!   synthesises the edit, and dispatches to `parse_incremental`.
//!
//! Both helpers are pure: they do **not** read from the filesystem,
//! and they keep the existing
//! `crate::parser::IncrementalParser` API intact.

use tree_sitter::{InputEdit, Point};

use crate::ast::error::AstResult;
use crate::ast::languages::Lang;
use crate::ast::parser::IncrementalParser;

/// Synthesise the minimal [`InputEdit`] describing the transition from
/// `old` to `new`.
///
/// Returns `None` when the strings are byte-identical (no edit). The
/// algorithm is intentionally simple — it scans bytes from both ends to
/// find the longest common prefix and suffix, then constructs the edit
/// from the residual middle. This is correct for any single contiguous
/// change (insertion, deletion, replacement) and produces a
/// conservative-but-valid edit even for split changes (multiple disjoint
/// insertions): the resulting `InputEdit` will cover the entire range
/// from the first divergence to the last divergence, which tree-sitter
/// handles correctly (it will re-parse that whole region and reuse the
/// rest).
///
/// # Edit fields produced
///
/// * `start_byte` — first byte that differs.
/// * `old_end_byte` — last byte of the old string that differs (+1).
/// * `new_end_byte` — last byte of the new string that differs (+1).
/// * `start_position` — `Point { row, column }` of `start_byte` in
///   `old`.
/// * `old_end_position` — `Point` of `old_end_byte` in `old`.
/// * `new_end_position` — `Point` of `new_end_byte` in `new`.
///
/// # Example
///
/// ```ignore
/// use touring_code::ast::parser_diff::compute_input_edit_from_diff;
/// let old = "fn main() { let x = 1; }";
/// let new = "fn main() { let x = 42; }";
/// let edit = compute_input_edit_from_diff(old, new).unwrap();
/// assert!(edit.new_end_byte - edit.start_byte >= 2); // covers "42"
/// ```
#[must_use]
pub fn compute_input_edit_from_diff(old: &str, new: &str) -> Option<InputEdit> {
    if old == new {
        return None;
    }
    let prefix = common_prefix_len_bytes(old.as_bytes(), new.as_bytes());
    let max_suffix = old.len().min(new.len()).saturating_sub(prefix);
    let suffix = common_suffix_len_bytes(
        &old.as_bytes()[prefix..],
        &new.as_bytes()[prefix..],
        max_suffix,
    );

    let start_byte = prefix;
    let old_end_byte = old.len().saturating_sub(suffix);
    let new_end_byte = new.len().saturating_sub(suffix);

    Some(InputEdit {
        start_byte,
        old_end_byte,
        new_end_byte,
        start_position: byte_to_point(old, start_byte),
        old_end_position: byte_to_point(old, old_end_byte),
        new_end_position: byte_to_point(new, new_end_byte),
    })
}

fn common_prefix_len_bytes(a: &[u8], b: &[u8]) -> usize {
    let limit = a.len().min(b.len());
    let mut i = 0;
    while i < limit && a[i] == b[i] {
        i += 1;
    }
    i
}

fn common_suffix_len_bytes(a: &[u8], b: &[u8], cap: usize) -> usize {
    let limit = a.len().min(b.len()).min(cap);
    let mut i = 0;
    while i < limit && a[a.len() - 1 - i] == b[b.len() - 1 - i] {
        i += 1;
    }
    i
}

/// Map a byte offset into a `(row, column)` point inside `source`.
///
/// `column` is the byte offset from the most recent `\n` (zero-indexed),
/// matching tree-sitter's [`Point`] semantics.
#[must_use]
pub fn byte_to_point(source: &str, byte_offset: usize) -> Point {
    let bytes = source.as_bytes();
    let cap = byte_offset.min(bytes.len());
    let mut row = 0_usize;
    let mut last_newline: usize = 0;
    let mut seen_newline = false;
    for (i, b) in bytes.iter().enumerate().take(cap) {
        if *b == b'\n' {
            row += 1;
            last_newline = i + 1;
            seen_newline = true;
        }
    }
    let column = if seen_newline {
        cap.saturating_sub(last_newline)
    } else {
        cap
    };
    Point { row, column }
}

impl IncrementalParser {
    /// Convenience wrapper around
    /// [`Self::parse_incremental`] that synthesises the
    /// [`InputEdit`] from `(old_source, new_source)` instead of
    /// requiring the caller to pre-compute it.
    ///
    /// 1. If `old_source == new_source`, falls back to a full parse and
    ///    returns an empty `changed_ranges` (zero-cost no-op detection).
    /// 2. Otherwise, calls [`compute_input_edit_from_diff`] to produce
    ///    the minimal `InputEdit`, then dispatches to
    ///    [`Self::parse_incremental`] which applies the edit and reuses
    ///    the cached old tree.
    ///
    /// Returns `(new_tree, changed_ranges)`. When there is no cached
    /// tree for `path`, the inner `parse_incremental` falls back to a
    /// full parse with empty `changed_ranges` (consistent with the
    /// existing API).
    ///
    /// # Errors
    ///
    /// Returns an `crate::error::AstError` if language detection
    /// fails or tree-sitter rejects the parse.
    pub fn parse_incremental_auto(
        &mut self,
        path: &str,
        new_source: &str,
        old_source: &str,
    ) -> AstResult<(tree_sitter::Tree, Vec<tree_sitter::Range>)> {
        match compute_input_edit_from_diff(old_source, new_source) {
            None => {
                // No content change — re-cache the existing source under
                // the same path so subsequent diffs continue to work.
                let lang = Lang::from_path(std::path::Path::new(path)).ok_or_else(|| {
                    crate::ast::error::AstError::UnknownLanguage(path.to_string())
                })?;
                let tree = self.parse_and_cache(path, new_source, lang)?;
                Ok((tree, Vec::new()))
            }
            Some(edit) => self.parse_incremental(path, new_source, &edit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_yield_no_edit() {
        assert!(compute_input_edit_from_diff("hello", "hello").is_none());
        assert!(compute_input_edit_from_diff("", "").is_none());
    }

    #[test]
    fn pure_append_locates_end() {
        let old = "fn a() {}";
        let new = "fn a() {}\nfn b() {}";
        let edit = compute_input_edit_from_diff(old, new).expect("diff");
        assert_eq!(edit.start_byte, old.len());
        assert_eq!(edit.old_end_byte, old.len());
        assert_eq!(edit.new_end_byte, new.len());
    }

    #[test]
    fn pure_prepend_locates_start() {
        // Pure prepend requires NO shared prefix between old and new
        // — otherwise the algorithm correctly collapses the prefix.
        let old = "BBB";
        let new = "AAABBB";
        let edit = compute_input_edit_from_diff(old, new).expect("diff");
        assert_eq!(edit.start_byte, 0);
        assert_eq!(edit.old_end_byte, 0);
        assert_eq!(edit.new_end_byte, new.len() - old.len());
    }

    #[test]
    fn shared_prefix_collapses_to_minimal_edit_region() {
        // When old and new share a non-empty prefix, the algorithm
        // correctly locates the change at the first divergence —
        // this is the design intent (minimal edit), not a bug.
        let old = "fn b() {}";
        let new = "fn a() {}\n\nfn b() {}";
        let edit = compute_input_edit_from_diff(old, new).expect("diff");
        assert_eq!(edit.start_byte, 3, "first divergence after `fn `");
        // Old has nothing after `fn `; the suffix `b() {}` is shared
        // (6 bytes), so old_end_byte = old.len() - 6 = 3 (zero-width
        // hole in the old text where the new content gets inserted).
        assert_eq!(edit.old_end_byte, 3);
        assert_eq!(edit.new_end_byte, new.len() - 6);
    }

    #[test]
    fn middle_replacement() {
        let old = "let x = 1;";
        let new = "let x = 42;";
        let edit = compute_input_edit_from_diff(old, new).expect("diff");
        assert_eq!(edit.start_byte, 8); // before "1"
        assert_eq!(edit.old_end_byte, 9); // after "1"
        assert_eq!(edit.new_end_byte, 10); // after "42"
    }

    #[test]
    fn full_replacement() {
        let old = "abc";
        let new = "xyz";
        let edit = compute_input_edit_from_diff(old, new).expect("diff");
        assert_eq!(edit.start_byte, 0);
        assert_eq!(edit.old_end_byte, 3);
        assert_eq!(edit.new_end_byte, 3);
    }

    #[test]
    fn multi_line_edit_computes_row_column() {
        let old = "line1\nline2\nline3";
        let new = "line1\nline22\nline3";
        let edit = compute_input_edit_from_diff(old, new).expect("diff");
        // start_byte points at the first divergence — within "line2" → "line22".
        assert_eq!(edit.start_position.row, 1);
        assert!(edit.start_position.column >= 4);
    }

    #[test]
    fn byte_to_point_zero_offset_is_origin() {
        let p = byte_to_point("anything", 0);
        assert_eq!(p.row, 0);
        assert_eq!(p.column, 0);
    }

    #[test]
    fn byte_to_point_after_newline_increments_row() {
        let src = "abc\ndef";
        let p = byte_to_point(src, 5); // 'e' is at row=1, column=1
        assert_eq!(p.row, 1);
        assert_eq!(p.column, 1);
    }

    #[test]
    fn byte_to_point_clamps_to_source_len() {
        let src = "abc";
        let p = byte_to_point(src, 1000);
        assert_eq!(p.row, 0);
        assert_eq!(p.column, 3);
    }

    #[test]
    fn empty_old_yields_full_insert() {
        let edit = compute_input_edit_from_diff("", "fn x() {}").expect("diff");
        assert_eq!(edit.start_byte, 0);
        assert_eq!(edit.old_end_byte, 0);
        assert_eq!(edit.new_end_byte, 9);
    }

    #[test]
    fn empty_new_yields_full_delete() {
        let edit = compute_input_edit_from_diff("fn x() {}", "").expect("diff");
        assert_eq!(edit.start_byte, 0);
        assert_eq!(edit.old_end_byte, 9);
        assert_eq!(edit.new_end_byte, 0);
    }

    #[test]
    fn parse_incremental_auto_handles_first_call_without_cache() {
        let mut inc = IncrementalParser::new();
        // No cached tree — auto should fall back to full parse.
        let result = inc.parse_incremental_auto(
            "test.rs",
            "fn main() { let x = 1; }",
            "", // no previous source
        );
        assert!(
            result.is_ok(),
            "first call should succeed via full-parse fallback"
        );
        let (_tree, changes) = result.unwrap();
        // Without an old cached tree, parse_incremental returns empty changes.
        assert!(changes.is_empty());
    }

    #[test]
    fn parse_incremental_auto_detects_no_change() {
        let mut inc = IncrementalParser::new();
        let source = "fn x() {}";
        inc.parse_and_cache("test.rs", source, Lang::Rust)
            .expect("cache");
        let result = inc.parse_incremental_auto("test.rs", source, source);
        assert!(result.is_ok());
        let (_tree, changes) = result.unwrap();
        assert!(
            changes.is_empty(),
            "identical sources should report no changes"
        );
    }

    #[test]
    fn parse_incremental_auto_uses_cached_tree() {
        let mut inc = IncrementalParser::new();
        let v1 = "fn x() { let a = 1; }";
        let v2 = "fn x() { let a = 42; }";
        inc.parse_and_cache("test.rs", v1, Lang::Rust)
            .expect("cache");
        let (tree, _changes) = inc
            .parse_incremental_auto("test.rs", v2, v1)
            .expect("incremental auto");
        // Sanity: parsed tree's source reflects v2.
        assert_eq!(
            &v2[tree.root_node().byte_range()],
            v2,
            "tree should span the new source"
        );
    }
}
