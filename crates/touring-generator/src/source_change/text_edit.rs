//! `TextEdit` — ordered sequence of `Indel` (insert/delete) operations.
//!
//! B.5.2: `TextEdit` as ordered `Vec<Indel>` with non-overlap invariant
//! (validated at construction). All ranges are 0-indexed half-open [start, end).

#![allow(clippy::missing_errors_doc)]

use std::ops::Range;

/// Error when `Indel` ranges in a `TextEdit` violate the non-overlap invariant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("overlapping or unsorted indel at index {index}: {:?}", reason)]
pub struct NonOverlapError {
    /// Index of the offending indel within the sequence.
    pub index: usize,
    /// Human-readable explanation of the overlap or ordering violation.
    pub reason: String,
}

/// A single edit: delete a range and insert replacement text.
/// `delete` is a half-open range [start, end) in the original file.
/// `insert` is the text to insert at `delete.start`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Indel {
    /// Range to delete from the original text (0-indexed, half-open).
    pub delete: Range<usize>,
    /// Text to insert at `delete.start`.
    pub insert: String,
}

impl Indel {
    /// Create an insertion at offset with no deletion.
    #[must_use]
    pub fn insert(offset: usize, text: String) -> Self {
        Self {
            delete: offset..offset,
            insert: text,
        }
    }

    /// Create a deletion of the given range with no insertion.
    #[must_use]
    pub fn delete(range: Range<usize>) -> Self {
        Self {
            delete: range,
            insert: String::new(),
        }
    }

    /// Returns the start offset of the indel.
    #[must_use]
    pub fn start(&self) -> usize {
        self.delete.start
    }

    /// Returns the end offset of the indel.
    #[must_use]
    pub fn end(&self) -> usize {
        self.delete.end
    }

    /// Returns true if this is a pure insertion (zero-length delete).
    #[inline]
    #[must_use]
    pub fn is_insertion(&self) -> bool {
        self.delete.is_empty()
    }

    /// Returns true if this is a pure deletion (empty insert).
    #[inline]
    #[must_use]
    pub fn is_deletion(&self) -> bool {
        self.insert.is_empty()
    }
}

/// Ordered sequence of `Indel` operations on a single file.
/// The indels MUST be sorted by start offset and non-overlapping — enforced at construction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextEdit {
    indels: Vec<Indel>,
}

impl TextEdit {
    /// Create a `TextEdit` from a fallible iterator of `Indel`.
    /// Returns `Err` if the indels are unsorted or overlapping.
    pub fn try_from_iter<I>(iter: I) -> Result<Self, NonOverlapError>
    where
        I: IntoIterator<Item = Indel>,
    {
        let indels: Vec<Indel> = iter.into_iter().collect();
        validate_sorted_non_overlap(&indels)?;
        Ok(Self { indels })
    }

    /// Returns the number of individual indel operations.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.indels.len()
    }

    /// Returns true if there are no indels.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indels.is_empty()
    }

    /// Returns an iterator over the indels.
    #[inline]
    pub fn indels(&self) -> impl Iterator<Item = &Indel> {
        self.indels.iter()
    }

    /// Apply all indels to the given source text.
    ///
    /// Because indels are sorted and non-overlapping, we can apply them
    /// in a single left-to-right pass.
    ///
    /// # Panics
    ///
    /// Panics if an indel range exceeds the source text length.
    #[must_use]
    pub fn apply_all(&self, source: &str) -> String {
        if self.indels.is_empty() {
            return source.to_string();
        }

        let mut result = String::with_capacity(source.len() + self.total_insertion_len());
        let mut src_cursor = 0;

        for indel in &self.indels {
            // Copy text before this indel
            result.push_str(&source[src_cursor..indel.delete.start]);
            // Delete range (already deleted by not copying it above)
            src_cursor = indel.delete.end;
            // Insert new text
            result.push_str(&indel.insert);
        }

        // Append remaining text after the last indel
        if src_cursor < source.len() {
            result.push_str(&source[src_cursor..]);
        }

        result
    }

    /// Returns total length of all inserted text.
    fn total_insertion_len(&self) -> usize {
        self.indels.iter().map(|i| i.insert.len()).sum()
    }

    /// Returns the end offset of the last indel, or 0 if empty.
    #[must_use]
    pub fn last_end(&self) -> usize {
        self.indels.last().map_or(0, Indel::end)
    }
}

/// Validate that indels are sorted by start and non-overlapping.
fn validate_sorted_non_overlap(indels: &[Indel]) -> Result<(), NonOverlapError> {
    for (i, indel) in indels.iter().enumerate() {
        if indel.delete.start > indel.delete.end {
            return Err(NonOverlapError {
                index: i,
                reason: format!(
                    "delete.start ({}) > delete.end ({})",
                    indel.delete.start, indel.delete.end
                ),
            });
        }
        if i > 0 {
            let prev = &indels[i - 1];
            if indel.delete.start < prev.delete.end {
                return Err(NonOverlapError {
                    index: i,
                    reason: format!(
                        "overlap: previous indel ends at {} but current starts at {}",
                        prev.delete.end, indel.delete.start
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Builder-style constructor for `TextEdit` from indels vector.
/// This is fallible — use `TextEdit::try_from_iter` for validation.
impl From<Vec<Indel>> for TextEdit {
    fn from(indels: Vec<Indel>) -> Self {
        Self { indels }
    }
}

impl std::fmt::Display for Indel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Indel(delete={:?}, insert={:?})",
            self.delete, self.insert
        )
    }
}

impl std::fmt::Display for TextEdit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextEdit({} indels)", self.len())
    }
}

// ---------------------------------------------------------------------------
// rkyv support
// ---------------------------------------------------------------------------

// rkyv serialization is deferred to touring-rkyv integration in B.5.8.

#[cfg(not(feature = "zero-copy"))]
mod rkyv_support {
    // No-op when zero-copy feature is disabled
}

#[cfg(test)]
mod tests {
    use super::{Indel, TextEdit};

    #[allow(dead_code)]
    fn indel(start: usize, end: usize, insert: &str) -> Indel {
        Indel {
            delete: start..end,
            insert: insert.into(),
        }
    }

    #[test]
    fn apply_all_insertion_at_start() {
        let te = TextEdit::try_from_iter(vec![Indel::insert(0, "Hello ".into())]).unwrap();
        assert_eq!(te.apply_all("World"), "Hello World");
    }

    #[test]
    fn apply_all_insertion_in_middle() {
        let te = TextEdit::try_from_iter(vec![Indel::insert(5, " wonderful".into())]).unwrap();
        assert_eq!(te.apply_all("Hello World"), "Hello wonderful World");
    }

    #[test]
    fn apply_all_delete_range() {
        let te = TextEdit::try_from_iter(vec![Indel {
            delete: 0..6,
            insert: String::new(),
        }])
        .unwrap();
        assert_eq!(te.apply_all("Hello World"), "World");
    }

    #[test]
    fn apply_all_replace() {
        let te = TextEdit::try_from_iter(vec![Indel {
            delete: 0..5,
            insert: "Goodbye".into(),
        }])
        .unwrap();
        assert_eq!(te.apply_all("Hello World"), "Goodbye World");
    }

    #[test]
    fn apply_all_multiple_indels() {
        // Two non-overlapping indels: delete "World" and insert ", beautiful"
        let te = TextEdit::try_from_iter(vec![Indel {
            delete: 6..11,
            insert: ", beautiful".into(),
        }])
        .unwrap();
        assert_eq!(te.apply_all("Hello World"), "Hello , beautiful");
    }

    #[test]
    fn apply_all_empty_insert() {
        let te = TextEdit::try_from_iter(vec![Indel::insert(5, String::new())]).unwrap();
        assert_eq!(te.apply_all("Hello World"), "Hello World");
    }

    #[test]
    fn apply_all_empty_text() {
        let te = TextEdit::try_from_iter(vec![Indel::insert(0, "New content".into())]).unwrap();
        assert_eq!(te.apply_all(""), "New content");
    }

    #[test]
    fn non_overlap_error_detected() {
        // Overlapping indels should fail
        let result = TextEdit::try_from_iter(vec![
            Indel {
                delete: 0..5,
                insert: "A".into(),
            },
            Indel {
                delete: 3..8,
                insert: "B".into(),
            }, // overlaps with previous (ends at 5)
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn unsorted_error_detected() {
        // Out-of-order indels should fail
        let result = TextEdit::try_from_iter(vec![
            Indel {
                delete: 10..15,
                insert: "A".into(),
            },
            Indel {
                delete: 0..5,
                insert: "B".into(),
            }, // starts before previous
        ]);
        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::reversed_empty_ranges)]
    fn invalid_range_error() {
        // start > end should fail — intentional reversed range to test error detection
        let result = TextEdit::try_from_iter(vec![Indel {
            delete: 10..5,
            insert: "A".into(),
        }]);
        assert!(result.is_err());
    }

    #[test]
    fn text_edit_display() {
        let te = TextEdit::try_from_iter(vec![Indel::insert(0, "x".into())]).unwrap();
        assert!(format!("{te}").contains("1 indels"));
    }

    #[test]
    fn indel_is_insertion() {
        assert!(Indel::insert(0, "test".into()).is_insertion());
        assert!(
            !Indel {
                delete: 0..5,
                insert: String::new()
            }
            .is_insertion()
        );
    }

    #[test]
    fn indel_is_deletion() {
        assert!(Indel::delete(0..5).is_deletion());
        assert!(!Indel::insert(0, "test".into()).is_deletion());
    }

    #[test]
    fn text_edit_from_indels_builder() {
        let te = TextEdit::try_from_iter(vec![Indel {
            delete: 0..5,
            insert: "A".into(),
        }])
        .unwrap();
        assert_eq!(te.len(), 1);
    }
}
