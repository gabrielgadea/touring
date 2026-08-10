//! F2.8 — Memory Management verifier (D21).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_memory_mgmt`] — a detector of four
//! memory smells: **unbounded growth** (`unbounded_channel(`/`unbounded(` /
//! Python `maxsize=None`), **leaks** (`Box::leak`/`mem::forget`/`.leak()`),
//! **refcount cycles** (a `parent`/`prev`/`owner` back-reference held as a strong
//! `Rc`/`Arc` — or C++ `shared_ptr` — instead of `Weak`), and **hot-path
//! allocation** (`.to_vec()`/`.to_owned()` inside a loop). It is disjoint from
//! F1.11 design-patterns (which keys on `Rc<RefCell<`) by keying on the
//! back-reference *name* + unbounded/leak/alloc. F2.8 is heaviest on Rust/C++
//! (manual refcounting + explicit leaks); GC languages have a small detectable
//! manual-memory surface, so the engine covers the language-specific signals it
//! can (Python's unbounded `lru_cache`).
//!
//! This replaces a stub that scored `Box::`/`Vec::` density. Comments and
//! `#[cfg(test)]`/test regions are excluded via `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `unbounded_channel(`/`Box::leak(` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F2.8 verifier — Memory Management.
#[allow(non_camel_case_types)]
pub struct F2_8_Memory;

impl Verification for F2_8_Memory {
    fn id(&self) -> DimId {
        DimId::F2_8
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_memory_dim(target)
    }
}

// ── Real engine: memory-management anti-pattern detection ─────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_memory_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_memory_mgmt, score_memory_mgmt};

    // The engine (`memory.rs`) embeds every needle (`b"unbounded_channel("`,
    // `b"Box::leak("`, `b"mem::forget("`, the `BACKREF_WORDS` table, …) as data,
    // so scanning its own source is a self-match false positive.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.8: detector own source (memory-mgmt needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_memory_mgmt(&raw, lang);

    let value = score_memory_mgmt(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F2.8: {} memory-management anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_memory_mgmt: unbounded growth / leak / refcount cycle / hot-path clone){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the memory-mgmt needle vocabulary
/// (`unbounded_channel(`, `Box::leak(`, `mem::forget(`, the `BACKREF_WORDS`
/// table, …) as detection data, so scoring their own source is a self-match
/// false positive. Mirrors `f1_5_tech_debt::is_detector_own_source` (test/bench
/// dirs + the quality engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: unbounded/leak density heuristic ─────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_memory_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("unbounded_channel(").count()
        + raw.matches("unbounded(").count()
        + raw.matches("Box::leak(").count()
        + raw.matches("mem::forget(").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{smells} unbounded/leak smell(s) over {} lines (heuristic; build --features \
         workspace-integration for full memory-management analysis)",
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
    fn test_memory_returns_valid_score() {
        let f = write_temp_ext("let (tx, rx) = mpsc::channel(64);\n", ".rs");
        let s = F2_8_Memory.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_memory_empty_file() {
        let f = write_temp("");
        let s = F2_8_Memory.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Bounded, leak-free file → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_bounded_clean_high() {
        let f = write_temp_ext(
            "fn pipe() {\n    let (tx, rx) = mpsc::channel(128);\n    let parent: Weak<Node> = w;\n}\n",
            ".rs",
        );
        let s = F2_8_Memory.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "bounded + Weak should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: an unbounded channel scores below a bounded one —
    /// the stub (`Box::`/`Vec::` density) was blind to it.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_unbounded_scores_lower() {
        let bad = write_temp_ext(
            "fn pipe() {\n    let (tx, rx) = mpsc::unbounded_channel();\n}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "fn pipe() {\n    let (tx, rx) = mpsc::channel(128);\n}\n",
            ".rs",
        );
        let sb = F2_8_Memory.check(bad.path()).expect("check");
        let sg = F2_8_Memory.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "unbounded ({}) < bounded ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: a Python unbounded `lru_cache` is flagged; a bounded one is not.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_python_unbounded_cache_polyglot() {
        let bad = write_temp_ext("@lru_cache(maxsize=None)\ndef f(x):\n    return x\n", ".py");
        let good = write_temp_ext("@lru_cache(maxsize=256)\ndef f(x):\n    return x\n", ".py");
        let sb = F2_8_Memory.check(bad.path()).expect("check");
        let sg = F2_8_Memory.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "maxsize=None ({}) must score below maxsize=256 ({})",
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
            "/x/touring-analysis/src/quality/memory.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-storage/src/cache.rs"
        )));
    }
}
