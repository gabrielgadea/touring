//! Test coverage from a `cargo-llvm-cov` LCOV artifact.
//!
//! True line coverage requires running the suite under instrumentation
//! (`cargo llvm-cov`) — it cannot be computed by inspecting a source buffer.
//! This module *consumes* the LCOV artifact that `cargo llvm-cov` emits, giving
//! REAL per-file line coverage (`hit / found`) when the artifact is present.
//! When no artifact exists (or it has no record for the target), callers fall
//! back to a presence proxy — this module only reports what the instrumentation
//! actually measured, never a guess.
//!
//! Backs the F3.1 dimension. Hermetic: pure file parse, no process spawn.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Per-file line coverage parsed from one LCOV `SF:` record.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FileCoverage {
    /// `LF:` — instrumented (found) lines.
    pub lines_found: usize,
    /// `LH:` — covered (hit) lines.
    pub lines_hit: usize,
}

impl FileCoverage {
    /// Coverage ratio in `[0,1]`, or `None` when no lines were instrumented.
    #[must_use]
    pub fn ratio(&self) -> Option<f32> {
        if self.lines_found == 0 {
            None
        } else {
            Some(self.lines_hit as f32 / self.lines_found as f32)
        }
    }
}

/// Result of looking up coverage for a target file.
#[derive(Debug, Clone, Default)]
pub struct CoverageReport {
    /// Path of the LCOV artifact consumed, if one was found.
    pub artifact_path: Option<PathBuf>,
    /// REAL line-coverage ratio for the target file, if the artifact had a
    /// matching record. `None` → caller must fall back to a presence proxy.
    pub ratio: Option<f32>,
    /// Instrumented lines for the target file (0 when no record matched).
    pub lines_found: usize,
    /// Covered lines for the target file (0 when no record matched).
    pub lines_hit: usize,
}

/// Canonical locations where `cargo llvm-cov --lcov` artifacts land, relative
/// to a crate/workspace root (checked at each ancestor while walking up).
const LCOV_CANDIDATES: &[&str] = &[
    "lcov.info",
    "target/llvm-cov/lcov.info",
    "coverage/lcov.info",
    "target/nextest/lcov.info",
];

/// Bound on the directory walk-up (workspace roots are shallow).
const MAX_WALK_UP: usize = 12;

/// Coverage analyzer that consumes a `cargo-llvm-cov` LCOV artifact.
pub struct CoverageArtifact;

impl CoverageArtifact {
    /// Construct the analyzer (stateless).
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Look up real per-file coverage for `target` from the nearest LCOV
    /// artifact. `report.ratio` is `Some` only when an artifact was found AND
    /// it contains a record matching the target file.
    #[must_use]
    pub fn analyze(&self, target: &Path) -> CoverageReport {
        let Some(artifact) = find_lcov(target) else {
            return CoverageReport::default();
        };
        let Ok(content) = std::fs::read_to_string(&artifact) else {
            return CoverageReport {
                artifact_path: Some(artifact),
                ..Default::default()
            };
        };
        let table = parse_lcov(&content);
        let cov = match_target(&table, target);
        CoverageReport {
            artifact_path: Some(artifact),
            ratio: cov.and_then(|c| c.ratio()),
            lines_found: cov.map(|c| c.lines_found).unwrap_or(0),
            lines_hit: cov.map(|c| c.lines_hit).unwrap_or(0),
        }
    }
}

impl Default for CoverageArtifact {
    fn default() -> Self {
        Self::new()
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Walk up from `target` looking for an LCOV artifact in the canonical spots.
fn find_lcov(target: &Path) -> Option<PathBuf> {
    let mut dir: &Path = if target.is_dir() {
        target
    } else {
        target.parent()?
    };
    for _ in 0..MAX_WALK_UP {
        for cand in LCOV_CANDIDATES {
            let p = dir.join(cand);
            if p.is_file() {
                return Some(p);
            }
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    None
}

/// Parse an LCOV buffer into a `SF-path → FileCoverage` table.
///
/// Prefers the `LF:`/`LH:` summary lines; falls back to counting `DA:` records
/// (a `DA:<line>,<count>` with `count > 0` is a hit) when the summary is absent.
fn parse_lcov(content: &str) -> HashMap<String, FileCoverage> {
    let mut table: HashMap<String, FileCoverage> = HashMap::new();
    let mut cur_file: Option<String> = None;
    let mut da_found = 0usize;
    let mut da_hit = 0usize;
    let mut lf: Option<usize> = None;
    let mut lh: Option<usize> = None;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("SF:") {
            commit(
                &mut table,
                cur_file.take(),
                da_found,
                da_hit,
                lf.take(),
                lh.take(),
            );
            da_found = 0;
            da_hit = 0;
            cur_file = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("DA:") {
            if let Some((_, count)) = rest.split_once(',') {
                da_found += 1;
                let c = count.split(',').next().unwrap_or("0").trim();
                if c.parse::<u64>().map(|n| n > 0).unwrap_or(false) {
                    da_hit += 1;
                }
            }
        } else if let Some(rest) = line.strip_prefix("LF:") {
            lf = rest.trim().parse::<usize>().ok();
        } else if let Some(rest) = line.strip_prefix("LH:") {
            lh = rest.trim().parse::<usize>().ok();
        } else if line.starts_with("end_of_record") {
            commit(
                &mut table,
                cur_file.take(),
                da_found,
                da_hit,
                lf.take(),
                lh.take(),
            );
            da_found = 0;
            da_hit = 0;
        }
    }
    // Trailing record without an explicit `end_of_record`.
    commit(
        &mut table,
        cur_file.take(),
        da_found,
        da_hit,
        lf.take(),
        lh.take(),
    );
    table
}

/// Fold one accumulated `SF:` block into the table. `LF:`/`LH:` win over the
/// `DA:` tallies when present.
fn commit(
    table: &mut HashMap<String, FileCoverage>,
    file: Option<String>,
    da_found: usize,
    da_hit: usize,
    lf: Option<usize>,
    lh: Option<usize>,
) {
    if let Some(f) = file {
        let found = lf.unwrap_or(da_found);
        let hit = lh.unwrap_or(da_hit);
        let entry = table.entry(f).or_default();
        entry.lines_found += found;
        entry.lines_hit += hit;
    }
}

/// Match the target file to an LCOV `SF:` record. Tries an exact canonical
/// path match first, then a last-two-components suffix match (`parent/file`)
/// so that `mod.rs` records are disambiguated by their parent directory.
fn match_target(table: &HashMap<String, FileCoverage>, target: &Path) -> Option<FileCoverage> {
    let canon_target = target.canonicalize().ok();
    let tail = last_two(target);
    let mut suffix_hit: Option<FileCoverage> = None;
    for (sf, cov) in table {
        if let (Some(ct), Ok(cs)) = (&canon_target, Path::new(sf).canonicalize())
            && *ct == cs
        {
            return Some(*cov);
        }
        if let Some(t) = &tail
            && sf.replace('\\', "/").ends_with(t)
        {
            suffix_hit = Some(*cov);
        }
    }
    suffix_hit
}

/// The last two path components as a `parent/file` string (or just `file`).
fn last_two(p: &Path) -> Option<String> {
    let file = p.file_name()?.to_string_lossy();
    match p.parent().and_then(Path::file_name) {
        Some(parent) => Some(format!("{}/{}", parent.to_string_lossy(), file)),
        None => Some(file.into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn ratio_is_none_when_no_lines_instrumented() {
        let c = FileCoverage {
            lines_found: 0,
            lines_hit: 0,
        };
        assert_eq!(c.ratio(), None);
    }

    #[test]
    fn ratio_computes_hit_over_found() {
        let c = FileCoverage {
            lines_found: 10,
            lines_hit: 7,
        };
        assert!((c.ratio().expect("ratio") - 0.7).abs() < 1e-6);
    }

    #[test]
    fn parse_lcov_prefers_lf_lh_summary() {
        let lcov = "TN:\nSF:/proj/src/foo.rs\nDA:1,1\nDA:2,0\nLF:10\nLH:8\nend_of_record\n";
        let table = parse_lcov(lcov);
        let cov = table.get("/proj/src/foo.rs").expect("record");
        assert_eq!(cov.lines_found, 10, "LF wins over DA count");
        assert_eq!(cov.lines_hit, 8, "LH wins over DA hit count");
    }

    #[test]
    fn parse_lcov_falls_back_to_da_count() {
        // No LF/LH → count DA records (count>0 = hit).
        let lcov = "SF:/proj/src/bar.rs\nDA:1,3\nDA:2,0\nDA:3,1\nend_of_record\n";
        let table = parse_lcov(lcov);
        let cov = table.get("/proj/src/bar.rs").expect("record");
        assert_eq!(cov.lines_found, 3, "3 DA records");
        assert_eq!(cov.lines_hit, 2, "two with count>0");
    }

    #[test]
    fn parse_lcov_handles_multiple_records() {
        let lcov = "SF:a.rs\nLF:4\nLH:4\nend_of_record\nSF:b.rs\nLF:10\nLH:0\nend_of_record\n";
        let table = parse_lcov(lcov);
        assert_eq!(table.len(), 2);
        assert_eq!(table.get("a.rs").expect("a").ratio(), Some(1.0));
        assert_eq!(table.get("b.rs").expect("b").ratio(), Some(0.0));
    }

    #[test]
    fn match_target_by_parent_and_file_suffix() {
        let mut table = HashMap::new();
        table.insert(
            "/abs/crate/src/quality/mod.rs".to_string(),
            FileCoverage {
                lines_found: 20,
                lines_hit: 15,
            },
        );
        // A different mod.rs must NOT match (parent dir differs).
        table.insert(
            "/abs/crate/src/wiring/mod.rs".to_string(),
            FileCoverage {
                lines_found: 100,
                lines_hit: 0,
            },
        );
        let got = match_target(&table, Path::new("src/quality/mod.rs")).expect("match");
        assert_eq!(got.lines_hit, 15, "matched the quality/mod.rs record");
    }

    #[test]
    fn analyze_returns_default_when_no_artifact() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let target = dir.path().join("src").join("x.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
        std::fs::write(&target, "fn x() {}\n").expect("write src");
        let report = CoverageArtifact::new().analyze(&target);
        assert!(report.artifact_path.is_none(), "no lcov present");
        assert!(report.ratio.is_none());
    }

    #[test]
    fn analyze_consumes_real_artifact_for_target_file() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let src_dir = dir.path().join("src").join("quality");
        std::fs::create_dir_all(&src_dir).expect("mkdir");
        let target = src_dir.join("foo.rs");
        std::fs::write(&target, "pub fn foo() {}\n").expect("write src");

        // cargo-llvm-cov drops lcov.info at the (walked-up) root.
        let mut lcov = std::fs::File::create(dir.path().join("lcov.info")).expect("lcov");
        let sf = target.display();
        write!(
            lcov,
            "TN:\nSF:{sf}\nDA:1,1\nDA:2,1\nDA:3,0\nDA:4,1\nLF:4\nLH:3\nend_of_record\n"
        )
        .expect("write lcov");

        let report = CoverageArtifact::new().analyze(&target);
        assert!(report.artifact_path.is_some(), "artifact found");
        assert_eq!(report.lines_found, 4);
        assert_eq!(report.lines_hit, 3);
        assert!(
            (report.ratio.expect("ratio") - 0.75).abs() < 1e-6,
            "real coverage 3/4, got {:?}",
            report.ratio
        );
    }
}
