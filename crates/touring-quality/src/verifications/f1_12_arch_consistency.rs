//! F1.12 — Architectural Consistency verifier (D12).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_arch_consistency`] — a polyglot
//! detector of the canonical "mixed-pattern drift" smell within a file:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `mixed-error-styles` | `panic!` + `Result<T,_>` + `.unwrap()` + `anyhow`/`thiserror` (≥3 distinct styles in one file) | Rust |
//! | `mixed-logging-styles` | `println!`/`eprintln!` + `tracing::*!` or `log::*!` in the same file | Rust/JS/TS/Python |
//! | `mixed-config-sources` | `std::env::var(` + `config::`/`figment::`/`dotenv::` in the same file | Rust/Python |
//! | `hardcoded-role-string` | `if .*\.role == "admin"` / `== "user"` (string-literal role comparison) | all |
//! | `mixed-async-runtimes` | `tokio::` + `async-std::` + `smol::` (≥2 runtimes in same file) | Rust |
//!
//! Disjoint from F1.7 boundaries (F1.7 keys on `pub` vs `pub(crate)`
//! surface area; F1.12 keys on *internal* style consistency), F1.8 dep
//! cycles (F1.8 keys on workspace-level SCCs; F1.12 keys on intra-file
//! pattern mix), and F1.11 patterns (F1.11 keys on GoF / ownership
//! anti-patterns; F1.12 keys on cross-cutting-concern consistency).
//!
//! **Sources (context7, `/sverweij/dependency-cruiser`, High reputation,
//! bench 87.06)**: dependency-cruiser enforces layer / path policy via
//! the `forbidden` block (`name`, `from.path`, `to.path` regex). ArchUnit
//! enforces architectural rules in Java. In a single file, the analogous
//! "consistency" check is whether the file follows the workspace's
//! cross-cutting-concern convention — drift is in progress when a single
//! file mixes error/log/config patterns.
//!
//! **Standalone fallback (`--no-default-features`)**: a `mod ` density
//! heuristic (preserves the W3/W4 stub interface).
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F1.12 verifier — Architectural Consistency.
#[allow(non_camel_case_types)]
pub struct F1_12_ArchConsistency;

impl Verification for F1_12_ArchConsistency {
    fn id(&self) -> DimId {
        DimId::F1_12
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_arch_consistency_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: arch-consistency detector ──────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_arch_consistency_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_arch_consistency, score_arch_consistency};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F1.12: detector own source (arch_consistency needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_arch_consistency(&raw, lang);
    let value = score_arch_consistency(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F1.12: {} arch-consistency drift(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_arch_consistency: mixed-error-styles / \
         mixed-logging-styles / mixed-config-sources / hardcoded-role-string / \
         mixed-async-runtimes){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the arch-consistency needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: mod-density heuristic ─────────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_arch_consistency_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let mods = raw.matches("mod ").count();
    let value = if mods == 0 {
        1.0
    } else {
        1.0 - (mods as f32 / 20.0).min(1.0)
    };
    let evidence = format!(
        "{mods} `mod ` declarations over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full arch-consistency analysis)"
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
    fn test_arch_consistency_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F1_12_ArchConsistency.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_arch_consistency_empty_file() {
        let f = write_temp("");
        let s = F1_12_ArchConsistency.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Consistent single-style file scores higher than mixed-style file.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_consistent_file_scores_higher_than_mixed() {
        let mixed = write_temp_ext(
            "fn a() -> Result<(), String> { panic!(\"oops\"); }\n\
             fn b() { let x = foo().unwrap(); }\n\
             fn c() { println!(\"debug\"); tracing::info!(\"info\"); }\n\
             fn d() { std::env::var(\"X\").ok(); let c = config::Config::default(); }\n\
             fn e(u: &User) -> bool { u.role == \"admin\" }\n",
            ".rs",
        );
        let consistent = write_temp_ext(
            "use thiserror::Error;\nuse tracing::info;\n\
             fn handler() -> Result<(), AppError> { Ok(()) }\n",
            ".rs",
        );
        let sm = F1_12_ArchConsistency.check(mixed.path()).expect("check");
        let sc = F1_12_ArchConsistency
            .check(consistent.path())
            .expect("check");
        assert!(
            sc.value > sm.value,
            "consistent file ({}) must score above mixed-drift file ({})",
            sc.value,
            sm.value
        );
    }
    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/arch_consistency.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
