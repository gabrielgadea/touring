//! Width/indent budget for template rendering.
//!
//! Inspired by `rustfmt`'s `Shape` (`rewrite.rs`). Propagates a width budget
//! through the typestate pipeline so that generators can detect overflow
//! and fall back to multiline strategies before committing artifacts.
//!
//! When rendered content exceeds `max_width`, the typestate returns `None` so
//! the caller can retry with a multiline strategy.

/// Width budget for template rendering.
///
/// `RenderShape` tracks the remaining characters available on the current
/// line (`max_width`), current indentation level (`indent`), and character
/// offset from line start (`offset`). The `budget(used)` method creates a
/// child shape with updated remaining width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderShape {
    /// Maximum line width in characters.
    pub max_width: u16,
    /// Indentation level in spaces.
    pub indent: u16,
    /// Character offset from start of line.
    pub offset: u16,
}

impl RenderShape {
    /// Create a new shape with explicit values.
    #[inline]
    #[must_use]
    pub fn new(max_width: u16, indent: u16, offset: u16) -> Self {
        Self {
            max_width,
            indent,
            offset,
        }
    }

    /// Create a shape with default `max_width` (100) and zero indent/offset.
    #[inline]
    #[must_use]
    pub fn default_width() -> Self {
        Self {
            max_width: 100,
            indent: 0,
            offset: 0,
        }
    }

    /// Remaining characters on the current line.
    #[inline]
    #[must_use]
    pub fn remaining(&self) -> u16 {
        self.max_width.saturating_sub(self.offset)
    }

    /// Check if a string of `len` characters would fit on the current line.
    #[inline]
    #[must_use]
    pub fn fits(&self, len: usize) -> bool {
        self.remaining() as usize >= len
    }

    /// Create a child shape representing space already consumed on the line.
    /// Used for nested rendering (e.g., function arguments, struct fields).
    #[inline]
    #[must_use]
    pub fn budget(&self, used: u16) -> Self {
        Self {
            max_width: self.max_width,
            indent: self.indent,
            offset: self.offset.saturating_add(used),
        }
    }

    /// Create a child shape with increased indent (e.g., block body).
    #[inline]
    #[must_use]
    pub fn indent_by(&self, extra: u16) -> Self {
        Self {
            max_width: self.max_width,
            indent: self.indent.saturating_add(extra),
            offset: 0, // reset offset for new line
        }
    }

    /// Create a shape for a new line at the current indent level.
    #[inline]
    #[must_use]
    pub fn new_line(&self) -> Self {
        Self {
            max_width: self.max_width,
            indent: self.indent,
            offset: 0,
        }
    }

    /// Total indent in spaces (indent × 4 spaces).
    #[inline]
    #[must_use]
    pub fn indent_spaces(&self) -> u16 {
        self.indent.saturating_mul(4)
    }

    /// Check if rendering content would overflow the per-line width budget.
    ///
    /// BUG FIX (Sprint 8 P2): the previous implementation compared
    /// `content.len()` (TOTAL bytes across all lines) with `max_width`
    /// (single line width budget). For multiline templates like
    /// `python_script.tera` (14 lines, max line 34 chars, total 207 bytes),
    /// the old check fired a false-positive overflow because 207 > 100,
    /// even though no individual line exceeded the budget.
    ///
    /// Correct semantics: shape gate is line-oriented. Iterate lines,
    /// measure each, compare with `offset + line_len > max_width`. Empty
    /// content cannot overflow (vacuously false).
    #[inline]
    #[must_use]
    pub fn would_overflow(&self, content: &str) -> bool {
        content.lines().any(|line| {
            let line_len = u16::try_from(line.len()).unwrap_or(u16::MAX);
            self.offset.saturating_add(line_len) > self.max_width
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shape_is_100_width() {
        let s = RenderShape::default_width();
        assert_eq!(s.max_width, 100);
        assert_eq!(s.indent, 0);
        assert_eq!(s.offset, 0);
    }

    #[test]
    fn remaining_calculation() {
        let s = RenderShape {
            max_width: 100,
            indent: 2,
            offset: 40,
        };
        assert_eq!(s.remaining(), 60);
    }

    #[test]
    fn fits_check() {
        let s = RenderShape {
            max_width: 100,
            indent: 0,
            offset: 80,
        };
        assert!(s.fits(20));
        assert!(!s.fits(25));
    }

    #[test]
    fn budget_creates_child() {
        let parent = RenderShape {
            max_width: 100,
            indent: 0,
            offset: 0,
        };
        let child = parent.budget(4);
        assert_eq!(child.offset, 4);
        assert_eq!(child.max_width, 100);
    }

    #[test]
    fn indent_by_increases_indent() {
        let s = RenderShape {
            max_width: 100,
            indent: 1,
            offset: 0,
        };
        let nested = s.indent_by(1);
        assert_eq!(nested.indent, 2);
        assert_eq!(nested.offset, 0); // reset on new line
    }

    #[test]
    fn new_line_resets_offset() {
        let s = RenderShape {
            max_width: 100,
            indent: 1,
            offset: 50,
        };
        let nl = s.new_line();
        assert_eq!(nl.offset, 0);
        assert_eq!(nl.indent, 1);
    }

    #[test]
    fn would_overflow() {
        let s = RenderShape {
            max_width: 80,
            indent: 0,
            offset: 75,
        };
        assert!(!s.would_overflow("foo"));
        assert!(s.would_overflow("foobarbazqux")); // 12 chars, would exceed 80
    }

    #[test]
    fn saturating_arithmetic_under_overflow() {
        let s = RenderShape {
            max_width: 100,
            indent: 0,
            offset: 150,
        };
        // offset > max_width — remaining should be 0
        assert_eq!(s.remaining(), 0);
        assert!(!s.fits(1));
    }
}
