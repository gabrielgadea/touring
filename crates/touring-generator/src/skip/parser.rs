//! `touring_generator::skip::parser` — Comment walker for skip regions.
//!
#![allow(
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]
//!
//! Parses `// touring:skip-region` … `// touring:skip-end` markers (line and block
//! comments) and `#[touring::skip]` Rust attributes to build a `SkipContext`.

use crate::skip::codes::Q_310_REGION_FROZEN;
use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Span in byte offsets within a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteSpan {
    /// Inclusive start offset in bytes from the beginning of the file.
    pub start: u64,
    /// Exclusive end offset in bytes (half-open `[start, end)`).
    pub end: u64,
}

impl ByteSpan {
    fn overlaps(self, other: ByteSpan) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// A single skip region — file + byte range.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkipRegion {
    /// Path of the source file the region belongs to.
    pub file_path: String,
    /// Byte range covered by the skip region.
    pub span: ByteSpan,
    /// Comment/attribute style that declared the region.
    pub style: SkipStyle,
}

/// How a skip region was declared in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkipStyle {
    /// Declared via line comment markers (`// touring:skip-region` / `// touring:skip-end`).
    LineComment,
    /// Declared via a block comment (`/* touring:skip-region */`).
    BlockComment,
    /// Declared via a Rust attribute (`#[touring::skip]`).
    RustAttribute,
}

impl SkipRegion {
    /// Create a new skip region.
    #[must_use]
    pub fn new(file_path: String, start: u64, end: u64, style: SkipStyle) -> Self {
        Self {
            file_path,
            span: ByteSpan { start, end },
            style,
        }
    }
}

/// Set of skip regions.
#[derive(Debug, Clone, Default)]
pub struct SkipContext {
    regions: Arc<RwLock<HashSet<SkipRegion>>>,
}

impl SkipContext {
    /// Create a new empty skip context.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regions: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Parse a source file and extract skip regions.
    ///
    /// Supports:
    /// - Line comments: `// touring:skip-region` / `// touring:skip-end` (region includes both markers)
    /// - Block comments: `/* touring:skip-region */` on its own line
    /// - Rust attributes: `#[touring::skip]`
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn parse_file(&self, file_path: &Path, source: &str) -> Result<usize> {
        let mut count = 0;
        let mut line_cursor = 1u64; // start at 1 so first line has start=1 (no empty line at 0)
        let mut in_region = false;
        let mut region_start: Option<u64> = None;

        for line in source.lines() {
            let line_start = line_cursor;
            let line_end = line_cursor + line.len() as u64 + 1; // +1 for newline

            let trimmed = line.trim();

            // Rust attribute: #[touring::skip] or #[touring(skip)] — NOT a `#` comment region marker
            if trimmed.starts_with('#')
                && (trimmed.contains("touring::skip") || trimmed.contains("touring(skip)"))
            {
                let region = SkipRegion::new(
                    file_path.to_string_lossy().to_string(),
                    line_start,
                    line_end,
                    SkipStyle::RustAttribute,
                );
                self.regions
                    .write()
                    .expect("RwLock poisoned")
                    .insert(region);
                count += 1;
            }
            // For all other `#` lines (Python `# touring:skip-*` markers), do NOT continue.
            // Fall through to the line comment region block below.

            // Line comment (Rust `//` or Python `#`): touring:skip-region → start multi-line region
            // Block comment: `/* touring:skip-region */` on its own line
            let is_line_comment = trimmed.starts_with("//") || trimmed.starts_with('#');
            let is_start_marker = is_line_comment
                && trimmed.contains("touring:skip-region")
                && !trimmed.contains("touring:skip-end");
            let is_end_marker =
                is_line_comment && trimmed.contains("touring:skip-end") && in_region;

            if is_start_marker {
                region_start = Some(line_end); // region starts AFTER this line
                in_region = true;
            } else if is_end_marker {
                // Region covers from AFTER start-marker line to BEFORE this line
                if let Some(start) = region_start.take() {
                    let region = SkipRegion::new(
                        file_path.to_string_lossy().to_string(),
                        start,
                        line_start, // end BEFORE this end-marker line
                        SkipStyle::LineComment,
                    );
                    self.regions
                        .write()
                        .expect("RwLock poisoned")
                        .insert(region);
                    count += 1;
                }
                in_region = false;
            }

            // Block comment: /* touring:skip-region */ anywhere on the line
            if let Some(cmt_start) = line.find("/*") {
                let cmt_trimmed = line[cmt_start..].trim_start();
                if cmt_trimmed.starts_with("/*") && cmt_trimmed.contains("touring:skip-region") {
                    if let Some(cmt_end) = line[cmt_start..].find("*/") {
                        let comment_start = line_cursor + cmt_start as u64;
                        let comment_end = comment_start + cmt_end as u64 + 2;
                        let region = SkipRegion::new(
                            file_path.to_string_lossy().to_string(),
                            comment_start,
                            comment_end,
                            SkipStyle::BlockComment,
                        );
                        self.regions
                            .write()
                            .expect("RwLock poisoned")
                            .insert(region);
                        count += 1;
                    }
                }
            }

            line_cursor = line_end;
        }

        // Handle unclosed region: extends to end of file
        if in_region {
            if let Some(start) = region_start.take() {
                let region = SkipRegion::new(
                    file_path.to_string_lossy().to_string(),
                    start,
                    line_cursor, // end at current cursor = EOF
                    SkipStyle::LineComment,
                );
                self.regions
                    .write()
                    .expect("RwLock poisoned")
                    .insert(region);
                count += 1;
            }
        }

        Ok(count)
    }

    /// Parse a directory recursively.
    pub fn parse_dir(&self, dir: &Path) -> Result<usize> {
        let mut total = 0;
        self.walk_dir(dir, &mut total)?;
        Ok(total)
    }

    fn walk_dir(&self, dir: &Path, total: &mut usize) -> Result<()> {
        if dir.is_file() {
            let source = std::fs::read_to_string(dir)
                .map_err(|e| anyhow!("read error {}: {e}", dir.display()))?;
            // dir IS the file path here (caller checked is_file on original path)
            *total += self.parse_file(dir, &source)?;
            return Ok(());
        }
        let entries =
            std::fs::read_dir(dir).map_err(|e| anyhow!("read_dir {}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| anyhow!("entry error: {e}"))?;
            let path = entry.path();
            if path.is_dir() {
                self.walk_dir(&path, total)?;
            } else {
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                *total += self
                    .parse_file(&path, &source)
                    .map_err(|e| anyhow!("{e}"))?;
            }
        }
        Ok(())
    }

    /// Returns all skip regions for a given file path.
    #[must_use]
    pub fn regions_for(&self, file_path: &str) -> Vec<SkipRegion> {
        self.regions
            .read()
            .expect("RwLock poisoned")
            .iter()
            .filter(|r| r.file_path == file_path)
            .cloned()
            .collect()
    }

    /// Check if a proposed edit (`file_path` + `byte_range`) overlaps any skip region.
    /// Returns `Ok(())` if clean, `Err(SkipViolation)` if frozen.
    pub fn check_edit(&self, file_path: &str, start: u64, end: u64) -> Result<(), SkipViolation> {
        let edit_span = ByteSpan { start, end };
        let regions = self.regions.read().expect("RwLock poisoned");

        for region in regions.iter() {
            if region.file_path == file_path && region.span.overlaps(edit_span) {
                return Err(SkipViolation {
                    code: Q_310_REGION_FROZEN,
                    file_path: file_path.to_string(),
                    region_span: region.span,
                    edit_span,
                    style: region.style,
                });
            }
        }

        Ok(())
    }

    /// Total number of regions loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.regions.read().expect("RwLock poisoned").len()
    }

    /// True if no regions are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.read().expect("RwLock poisoned").is_empty()
    }

    /// Clear all loaded regions.
    pub fn clear(&self) {
        self.regions.write().expect("RwLock poisoned").clear();
    }
}

/// Diagnostic payload for a skip-region violation.
#[derive(Debug, Clone)]
pub struct SkipViolation {
    /// Diagnostic code identifying the frozen-region violation.
    pub code: &'static str,
    /// Path of the file where the violation occurred.
    pub file_path: String,
    /// Byte range of the frozen skip region that was hit.
    pub region_span: ByteSpan,
    /// Byte range of the proposed edit that overlapped the region.
    pub edit_span: ByteSpan,
    /// Comment/attribute style that declared the frozen region.
    pub style: SkipStyle,
}

impl std::fmt::Display for SkipViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: edit {{ {}..{} }} overlaps frozen skip-region {{ {}..{} }} in '{}' ({:?})",
            self.code,
            self.edit_span.start,
            self.edit_span.end,
            self.region_span.start,
            self.region_span.end,
            self.file_path,
            self.style
        )
    }
}

impl std::error::Error for SkipViolation {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_comment_region() {
        let ctx = SkipContext::new();
        let source = "fn foo() {}\n// touring:skip-region\nfn blocked() {}\n// touring:skip-end\nfn bar() {}";
        ctx.parse_file(Path::new("test.rs"), source)
            .expect("parse_file failed");
        let regions = ctx.regions_for("test.rs");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].style, SkipStyle::LineComment);
        // foo at ~0-12, region starts at ~14 (after \n of foo line), edit 0-10 should not overlap
        assert!(
            ctx.check_edit("test.rs", 0, 10).is_ok(),
            "foo should not overlap region"
        );
        // blocked falls in region
        assert!(
            ctx.check_edit("test.rs", 26, 40).is_err(),
            "blocked should be in frozen region"
        );
    }

    #[test]
    fn block_comment_single_line() {
        let ctx = SkipContext::new();
        let source = "fn foo() {} /* touring:skip-region */ fn bar() {}";
        ctx.parse_file(Path::new("test.rs"), source)
            .expect("parse_file failed");
        assert_eq!(ctx.len(), 1);
        let regions = ctx.regions_for("test.rs");
        assert_eq!(regions[0].style, SkipStyle::BlockComment);
    }

    #[test]
    fn rust_attribute() {
        let ctx = SkipContext::new();
        let source = "fn foo() {}\n#[touring::skip]\nfn blocked() {}\nfn bar() {}";
        ctx.parse_file(Path::new("test.rs"), source)
            .expect("parse_file failed");
        let regions = ctx.regions_for("test.rs");
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].style, SkipStyle::RustAttribute);
    }

    #[test]
    fn no_overlap_empty() {
        let ctx = SkipContext::new();
        ctx.parse_file(
            Path::new("test.rs"),
            "// touring:skip-region\nfn foo() {}\n// touring:skip-end",
        )
        .expect("parse_file failed");
        assert!(ctx.check_edit("test.rs", 30, 50).is_err());
        assert!(ctx.check_edit("test.rs", 0, 10).is_ok());
    }

    #[test]
    fn clear_and_reparse() {
        let ctx = SkipContext::new();
        ctx.parse_file(
            Path::new("test.rs"),
            "// touring:skip-region\nfn foo() {}\n// touring:skip-end",
        )
        .expect("parse_file failed");
        assert_eq!(ctx.len(), 1);
        ctx.clear();
        assert!(ctx.is_empty());
    }
}
