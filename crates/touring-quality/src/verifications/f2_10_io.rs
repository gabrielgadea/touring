//! F2.10 — I/O Bottlenecks verifier (D23).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_io`] — a detector of four I/O smells:
//! **blocking I/O in async context** (`async fn` + `std::fs::`/`std::net::`/
//! `TcpStream::connect`/`UdpSocket::`/`reqwest::blocking` in the same file —
//! "blocking the thread … will starve the executor" per tokio docs),
//! **`block_on(` inside a runtime** (panics: "Cannot start a runtime from
//! within a runtime"), **I/O inside a loop body** (a `std::fs::`/`std::net::`/
//! `TcpStream::`/`reqwest::blocking` call inside a `for`/`while` body — the
//! N+1 I/O counterpart to F2.7's db N+1), and **unbuffered read loops**
//! (`read_exact(` inside a loop body with no `BufReader` in the same body).
//!
//! **Disjoint** from F2.7 db-perf (which keys on `db.execute`/`db.query` in
//! loop, db-specific — F2.10 keys on the file/network I/O family, none of
//! which the db engine inspects), F2.8 memory (which keys on
//! `unbounded_channel(`/`Box::leak(`/.clone in loop, not I/O), and F2.9
//! caching (which keys on the cache builder chain + cache-named get/insert).
//! The `loop_bodies` shared helper is reused (DRY with F2.7/F2.8) so the
//! closure-brace-in-iterator-expr and `impl … for` regressions stay handled
//! in one place.
//!
//! This replaces a stub that scored `async_io / (sync_io + async_io)` ratio —
//! an anti-metric that was blind to *where* the sync I/O appears (an async
//! file with a single `std::fs::read` near the top of `main` is a critical
//! finding regardless of how much `tokio::fs` it has elsewhere). Comments
//! and `#[cfg(test)]`/test regions are excluded via `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `std::fs::`/`std::net::`/`block_on(` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F2.10 verifier — I/O Bottlenecks.
#[allow(non_camel_case_types)]
pub struct F2_10_Io;

impl Verification for F2_10_Io {
    fn id(&self) -> DimId {
        DimId::F2_10
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_io_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: I/O-bottleneck anti-pattern detection ───────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_io_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_io, score_io};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.10: detector own source (I/O-bottleneck needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_io(&raw, lang);
    let value = score_io(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F2.10: {} I/O-bottleneck anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_io: blocking-in-async / block_on-in-runtime / io-in-loop / unbuffered-read-loop){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the I/O needle vocabulary (`std::fs::`,
/// `std::net::`, `block_on(`, `BufReader`, …) as detection data, so scoring
/// their own source is a self-match false positive. Mirrors
/// `f2_8_memory::is_detector_own_source` (test/bench dirs + the quality
/// engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: blocking-I/O density heuristic ──────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_io_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("std::fs::").count()
        + raw.matches("std::net::").count()
        + raw.matches("block_on(").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{smells} std-blocking-I/O/block_on smell(s) over {} lines (heuristic; build --features \
         workspace-integration for full I/O-bottleneck analysis)",
        lines as usize
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    fn write_temp_ext(content: &str, suffix: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }
    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }
    #[test]
    fn test_io_returns_valid_score() {
        let f = write_temp_ext(
            "async fn f() { let _ = tokio::fs::read(\"a\").await; }\n",
            ".rs",
        );
        let s = F2_10_Io.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_io_empty_file() {
        let f = write_temp("");
        let s = F2_10_Io.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Async fn + blocking I/O → low score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_blocking_in_async_scores_lower() {
        let bad = write_temp_ext(
            "async fn load() {\n    let s = std::fs::read_to_string(\"a.txt\").unwrap();\n}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "async fn load() {\n    let s = tokio::fs::read_to_string(\"a.txt\").await.unwrap();\n}\n",
            ".rs",
        );
        let sb = F2_10_Io.check(bad.path()).expect("check");
        let sg = F2_10_Io.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "blocking-in-async ({}) < tokio-fs ({})",
            sb.value,
            sg.value
        );
    }
    /// block_on inside async fn panics → flagged (one finding is enough).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_block_on_in_async_flagged() {
        let bad = write_temp_ext(
            "async fn go() {\n    let r = tokio::runtime::Runtime::new().unwrap().block_on(work());\n}\n",
            ".rs",
        );
        let good = write_temp_ext("async fn go() { let r = work().await; }\n", ".rs");
        let sb = F2_10_Io.check(bad.path()).expect("check");
        let sg = F2_10_Io.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "block_on-in-async ({}) < clean ({})",
            sb.value,
            sg.value
        );
    }
    /// I/O in loop body → flagged.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_io_in_loop_flagged() {
        // Dirty: `std::fs::read_to_string` inside a `for` body (N+1 I/O).
        let bad = write_temp_ext(
            "fn load_all(paths: &[&str]) {\n    for p in paths { let s = std::fs::read_to_string(p).unwrap(); }\n}\n",
            ".rs",
        );
        // Clean: same fs call but OUTSIDE any loop body.
        let good = write_temp_ext(
            "fn load_one() { let s = std::fs::read_to_string(\"a\").unwrap(); }\n",
            ".rs",
        );
        let sb = F2_10_Io.check(bad.path()).expect("check");
        let sg = F2_10_Io.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "io-in-loop ({}) < single-call ({})",
            sb.value,
            sg.value
        );
    }
    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/io.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-storage/src/file.rs"
        )));
    }
}
