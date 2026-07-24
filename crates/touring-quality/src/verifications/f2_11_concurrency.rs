//! F2.11 — Concurrency verifier (D24).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_concurrency`] — a language-aware
//! detector of the canonical concurrency smells. Six detectors total:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `lock-across-await` | `std::sync::Mutex::lock(` + `.await` in same async fn body | Rust |
//! | `sync-locks-in-async` | `async fn` + `std::sync::Mutex::lock(` (file-level) | Rust |
//! | `arc-mutex-no-channel` | `Arc<Mutex<…>>` shared state with no `tokio::sync::mpsc`/`oneshot` | Rust |
//! | `mutex-where-atomic` | `Mutex<u64/i64/usize>` for a counter (use `AtomicU64`) | Rust |
//! | `go-goroutine-mutex` | `go func()` + `sync.Mutex` in same file | Go |
//! | `py-async-threading-lock` | `async def` + `threading.Lock` (blocks the event loop) | Python |
//!
//! It is **disjoint** from F2.8 memory (which keys on `unbounded_channel(`
//! / leak / `.clone` — concurrency keys on lock semantics and channel-vs-
//! state-shape), F2.10 I/O (which keys on `std::fs::` in `async fn` +
//! `block_on(` — concurrency keys on `std::sync::Mutex::lock(` in `async fn`),
//! and F2.9 caching. `lock-across-await` requires the await to be in the
//! *same brace-scope* as the lock — neither F2.8 nor F2.10 inspects that
//! relationship.
//!
//! This replaces a stub that scored `(Mutex + RwLock + .lock())` density
//! against `Atomic` density (anti-metric: more locks = lower score regardless
//! of context — a tokio actor full of `Arc<Mutex<…>>` and a tight atomic-
//! counter file would both score the same).
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `Mutex::lock(` / `sync.Mutex` / `go func(` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F2.11 verifier — Concurrency.
#[allow(non_camel_case_types)]
pub struct F2_11_Concurrency;

impl Verification for F2_11_Concurrency {
    fn id(&self) -> DimId {
        DimId::F2_11
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_concurrency_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: concurrency anti-pattern detection ─────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_concurrency_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_concurrency, score_concurrency};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.11: detector own source (concurrency needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_concurrency(&raw, lang);
    let value = score_concurrency(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F2.11: {} concurrency anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_concurrency: lock-across-await / sync-locks-in-async / arc-mutex-no-channel / mutex-where-atomic / go-goroutine-mutex / py-async-threading-lock){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the concurrency needle vocabulary
/// (`async fn`, `std::sync::Mutex::lock(`, `go func(`, `threading.Lock(`, …)
/// as detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source` (test/bench dirs +
/// the quality engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: concurrency-density heuristic ───────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_concurrency_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("std::sync::Mutex::lock(").count()
        + raw.matches("go func(").count()
        + raw.matches("threading.Lock(").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{smells} std-sync-lock / go-func / threading.Lock smell(s) over {} lines (heuristic; build \
         --features workspace-integration for full concurrency analysis)",
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
    fn test_concurrency_returns_valid_score() {
        let f = write_temp_ext(
            "fn f() { let m = std::sync::Mutex::new(0u64); let _ = m.lock().unwrap(); }\n",
            ".rs",
        );
        let s = F2_11_Concurrency.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_concurrency_empty_file() {
        let f = write_temp("");
        let s = F2_11_Concurrency.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Lock-across-await is the canonical deadlock-in-async smell.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_lock_across_await_scores_lower() {
        let bad = write_temp_ext(
            "async fn go() {\n    let _g = std::sync::Mutex::new(0).lock().unwrap();\n    do_work().await;\n}\n",
            ".rs",
        );
        let good = write_temp_ext("async fn go() { let r = work().await; use_r(r); }\n", ".rs");
        let sb = F2_11_Concurrency.check(bad.path()).expect("check");
        let sg = F2_11_Concurrency.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "lock-across-await ({}) < clean ({})",
            sb.value,
            sg.value
        );
    }
    /// Mutex<u64> for a counter is a lock-free opportunity.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_mutex_u64_flagged() {
        let bad = write_temp_ext(
            "let c: std::sync::Mutex<u64> = std::sync::Mutex::new(0);\n",
            ".rs",
        );
        let good = write_temp_ext(
            "use std::sync::atomic::{AtomicU64, Ordering};\nlet c = AtomicU64::new(0);\n",
            ".rs",
        );
        let sb = F2_11_Concurrency.check(bad.path()).expect("check");
        let sg = F2_11_Concurrency.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "Mutex<u64> ({}) < AtomicU64 ({})",
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
            "/x/touring-analysis/src/quality/concurrency.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-storage/src/sync.rs"
        )));
    }
}
