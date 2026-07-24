//! Technical-debt signal (D05) — comment markers + code debt + dead-code suppressions.
//!
//! A faithful D05 / SQALE-style debt signal that the prior substring stub could
//! not give. Three precision levers the stub lacked:
//!
//! 1. **Word-boundary markers** — `TODO`/`FIXME`/`HACK`/`XXX`/`BUG` are matched
//!    only as whole words, so `DEBUG` never matches `BUG`, `AUTODOC`/`TODO_LIMIT`
//!    never match `TODO` (the stub's `raw.matches("BUG")` counted `DEBUG`).
//! 2. **Comment-scoped markers** — a marker is debt only when it lives in a
//!    comment (or test region), not in a code identifier or a production string.
//!    Reuses [`super::code_regions`] (the markers' home is the *inverse* of where
//!    `count_prod_hazards` looks): a `// TODO` counts, a `let todo = ...` or
//!    `"TODO in a string"` does not.
//! 3. **Code debt vs managed debt** — `todo!()`/`unimplemented!()` (incomplete
//!    production code that panics) and `#[allow(dead_code)]`/`#[allow(unused…)]`
//!    (suppression debt) are counted in *production* only (not comment mentions);
//!    a `TODO(#123)` marker carrying an issue reference is *tracked* debt and is
//!    weighted far lighter than a bare, invisible `TODO`.
//!
//! Grounded in the SonarQube/SQALE model (debt = remediation cost) and the D05
//! rule (`~/.claude/rules/quality/D05_tech-debt.md`): markers, `allow(dead_code)`,
//! `todo!()`/`unimplemented!()`. Zero non-std dependencies.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// Whole-word comment markers (matched case-sensitively as upper-case words).
const MARKERS: &[&[u8]] = &[b"TODO", b"FIXME", b"HACK", b"XXX", b"BUG"];
/// Code-debt macros: incomplete production code that panics at runtime.
const CODE_DEBT: &[&[u8]] = &[b"todo!(", b"unimplemented!("];
/// Lint suppressions that hide unused / dead code instead of removing it.
const SUPPRESSIONS: &[&[u8]] = &[b"allow(dead_code", b"allow(unused"];

/// Technical-debt analysis for a source buffer.
#[derive(Debug, Clone, Default)]
pub struct TechDebtReport {
    /// Whole-word `TODO`/`FIXME`/`HACK`/`XXX`/`BUG` markers found in comments
    /// (or `#[cfg(test)]` regions); excludes code identifiers and prod strings.
    pub comment_markers: usize,
    /// Subset of `comment_markers` carrying an issue reference, e.g. `TODO(#123)`
    /// or `FIXME(JIRA-7)` — *tracked* (managed) debt, weighted lighter.
    pub tracked_markers: usize,
    /// `todo!()` / `unimplemented!()` in production code (not comments/test).
    pub code_debt: usize,
    /// `#[allow(dead_code)]` / `#[allow(unused…)]` in production code.
    pub suppressions: usize,
    /// Total source lines (denominator for SQALE-style debt density).
    pub total_lines: usize,
}

impl TechDebtReport {
    /// Total raw debt items (markers + code debt + suppressions).
    #[must_use]
    pub fn total(&self) -> usize {
        self.comment_markers + self.code_debt + self.suppressions
    }
}

/// True when `[start, end)` is a whole word — both neighbours are non-identifier
/// bytes (not ASCII alphanumeric and not `_`). Out-of-range neighbours count as
/// boundaries. This is what makes `DEBUG` not match `BUG` and `TODO_LIMIT` not
/// match `TODO`.
fn is_word(bytes: &[u8], start: usize, end: usize) -> bool {
    let left_ok = match start.checked_sub(1).and_then(|i| bytes.get(i)) {
        Some(&b) => !b.is_ascii_alphanumeric() && b != b'_',
        None => true,
    };
    let right_ok = match bytes.get(end) {
        Some(&b) => !b.is_ascii_alphanumeric() && b != b'_',
        None => true,
    };
    left_ok && right_ok
}

/// First non-space byte at or after `idx` (skips ASCII spaces/tabs only, so a
/// marker on its own line is not joined to the next line's `(`).
fn next_nonspace(bytes: &[u8], idx: usize) -> Option<u8> {
    let mut i = idx;
    while let Some(&b) = bytes.get(i) {
        if b == b' ' || b == b'\t' {
            i += 1;
        } else {
            return Some(b);
        }
    }
    None
}

/// Analyze technical debt in `source` (`lang` selects the comment/string lexer
/// for `code_regions`: `"rust"`, `"python"`, `"typescript"`, `"go"`, …).
///
/// Markers are counted inside comment/test regions; code-debt macros and
/// lint suppressions are counted in production regions only.
#[must_use]
pub fn analyze_tech_debt(source: &str, lang: &str) -> TechDebtReport {
    let regions = non_executable_regions(source, lang);
    let bytes = source.as_bytes();

    // Markers: whole-word AND inside a comment (or test) region.
    let mut comment_markers = 0usize;
    let mut tracked_markers = 0usize;
    for marker in MARKERS {
        for off in memmem::find_iter(bytes, marker) {
            let end = off + marker.len();
            if !is_word(bytes, off, end) {
                continue;
            }
            if !offset_suppressed(off, &regions) {
                continue;
            }
            comment_markers += 1;
            // Tracked when the next non-space byte opens a reference: `TODO(#…)`.
            if next_nonspace(bytes, end) == Some(b'(') {
                tracked_markers += 1;
            }
        }
    }

    // Code debt + suppressions: production only (not comment/test mentions).
    let count_prod = |needle: &[u8]| -> usize {
        memmem::find_iter(bytes, needle)
            .filter(|&off| !offset_suppressed(off, &regions))
            .count()
    };
    let code_debt: usize = CODE_DEBT.iter().map(|p| count_prod(p)).sum();
    let suppressions: usize = SUPPRESSIONS.iter().map(|p| count_prod(p)).sum();

    TechDebtReport {
        comment_markers,
        tracked_markers,
        code_debt,
        suppressions,
        total_lines: source.lines().count().max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_source_has_no_debt() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.total(), 0);
    }

    #[test]
    fn debug_identifier_is_not_a_bug_marker() {
        // The stub's `matches("BUG")` counted this; word-boundary must not.
        let src = "const DEBUG_MODE: bool = true;\nfn debugger() {}\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.comment_markers, 0, "DEBUG/debugger are not BUG markers");
    }

    #[test]
    fn todo_in_identifier_or_string_is_not_a_marker() {
        let src = "let todo_list = vec![1];\nlet s = \"TODO in a string\";\n";
        let r = analyze_tech_debt(src, "rust");
        // `todo_list` is lowercase + `_`-joined; the prod string is not a comment.
        assert_eq!(r.comment_markers, 0);
    }

    #[test]
    fn comment_marker_is_counted() {
        let src = "fn f() {}\n// TODO: refactor this\nfn g() {}\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.comment_markers, 1);
        assert_eq!(r.tracked_markers, 0, "bare TODO is untracked");
    }

    #[test]
    fn tracked_marker_carries_a_reference() {
        let src = "// TODO(#123): finish\n// FIXME(JIRA-7): later\n// HACK: bare\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.comment_markers, 3);
        assert_eq!(
            r.tracked_markers, 2,
            "TODO(#123) and FIXME(JIRA-7) are tracked"
        );
    }

    #[test]
    fn code_debt_macros_counted_in_production() {
        let src = "fn f() { todo!() }\nfn g() { unimplemented!() }\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.code_debt, 2);
    }

    #[test]
    fn code_debt_in_comment_is_not_counted() {
        // A mention of `todo!()` in a comment is documentation, not debt.
        let src = "fn f() {}\n// replaced the old todo!() with a real impl\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.code_debt, 0);
    }

    #[test]
    fn suppressions_counted() {
        let src = "#[allow(dead_code)]\nfn unused() {}\n#[allow(unused_variables)]\nfn f() {}\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.suppressions, 2);
    }

    #[test]
    fn cfg_test_region_excludes_code_debt_but_keeps_markers() {
        // A `todo!()` inside a #[cfg(test)] is test scaffolding (not prod debt);
        // a TODO note there is still a tracked-able marker.
        let src =
            "fn prod() {}\n#[cfg(test)]\nmod tests {\n// TODO: add a case\nfn t() { todo!() }\n}\n";
        let r = analyze_tech_debt(src, "rust");
        assert_eq!(r.code_debt, 0, "test todo!() is not production debt");
        assert_eq!(r.comment_markers, 1, "the TODO note is still counted");
    }

    #[test]
    fn empty_source() {
        let r = analyze_tech_debt("", "rust");
        assert_eq!(r.total(), 0);
        assert_eq!(r.total_lines, 1);
    }
}
