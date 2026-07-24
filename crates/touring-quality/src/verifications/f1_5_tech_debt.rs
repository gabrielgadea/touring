//! F1.5 — Technical Debt verifier (D05).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_tech_debt`] — a faithful D05 / SQALE-style
//! signal the prior substring stub could not give. Three precision levers the
//! stub lacked: (1) **word-boundary markers** (`DEBUG` no longer matches `BUG`,
//! `AUTODOC`/`TODO_LIMIT` no longer match `TODO` — the stub's `raw.matches("BUG")`
//! counted `DEBUG`); (2) **comment-scoped markers** (a `// TODO` counts; a
//! `let todo = …` identifier or a `"TODO in a string"` does not); (3) **code debt
//! vs managed debt** — `todo!()`/`unimplemented!()` (panicking incomplete code)
//! and `#[allow(dead_code/unused)]` (suppression debt) counted in production
//! only, while a `TODO(#123)` carrying an issue reference is *tracked* debt,
//! weighted far lighter than a bare invisible `TODO`.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior raw marker
//! substring density (counts `DEBUG` as `BUG`, markers in code/strings), labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`).

use crate::verifications::Verification;
use crate::{DimId, DimScore, DimStatus};
use anyhow::Result;
use std::path::Path;

/// F1.5 verifier — Technical Debt.
#[allow(non_camel_case_types)]
pub struct F1_5_TechDebt;

impl Verification for F1_5_TechDebt {
    fn id(&self) -> DimId {
        DimId::F1_5
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        // Detector-own-source: the tech-debt engine, the antipattern engine, and
        // this verifier embed the marker/needle vocabulary (`TODO`/`FIXME`, the
        // `todo!(` / `allow(dead_code` needles) as *detection logic* and prose
        // documentation; scoring them is a false positive (SAST-standard:
        // gitleaks/Semgrep allowlist their own rules + tests). The trade-off
        // (intentional, documented) is that a genuine TODO inside those detector
        // dirs is not flagged by F1.5 — they are heavily reviewed anyway.
        if is_detector_own_source(target) {
            let value = 1.0;
            return Ok(DimScore {
                value,
                status: DimStatus::from_score(value),
                evidence: "F1.5: detector-own-source (embeds debt-marker vocabulary as \
                           detection logic) — allowlisted"
                    .to_string(),
                suggestions: vec![],
                latency_ms: 0,
            });
        }
        let (value, evidence) = analyze_tech_debt_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

/// The tech-debt detectors and this verifier embed marker/needle literals as
/// detection logic and documentation, so scoring their own source is a false
/// positive. Mirrors `f2_6_config::is_detector_own_source` (test/bench dirs +
/// the quality engine + verifier dirs). Not feature-gated — pure path logic.
fn is_detector_own_source(target: &Path) -> bool {
    let canonical = target.canonicalize();
    let p = canonical
        .as_deref()
        .map(|c| c.to_string_lossy())
        .unwrap_or_else(|_| target.to_string_lossy());
    const NON_PRODUCTION_DIRS: [&str; 4] = ["/tests/", "/test/", "/benches/", "/bench/"];
    if NON_PRODUCTION_DIRS.iter().any(|s| p.contains(s)) {
        return true;
    }
    // The debt detectors (`tech_debt.rs`, `antipatterns.rs`) and the F1.5
    // verifier embed the marker/needle vocabulary; allowlist the engine +
    // verifier source dirs (same set as the security verifiers).
    const DETECTOR_SOURCES: [&str; 3] = [
        "touring-quality/src",
        "touring-quality/tests",
        "touring-analysis/src/quality",
    ];
    DETECTOR_SOURCES.iter().any(|s| p.contains(s))
}

// ── Real engine: word-bounded, comment-scoped, production-aware debt ──────────
#[cfg(feature = "workspace-integration")]
fn analyze_tech_debt_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::analyze_tech_debt;

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_tech_debt(&raw, lang);

    let untracked = r.comment_markers.saturating_sub(r.tracked_markers);
    let value = score_tech_debt(
        r.code_debt,
        r.suppressions,
        untracked,
        r.tracked_markers,
        r.total_lines,
    );

    let evidence = format!(
        "F1.5: code_debt(todo!/unimplemented!)={} suppressions(allow dead/unused)={} \
         markers={}({} tracked) ({lang}) — score={value:.3} \
         (touring-analysis analyze_tech_debt, word-bounded + comment-scoped)",
        r.code_debt, r.suppressions, r.comment_markers, r.tracked_markers
    );
    Ok((value, evidence))
}

/// SQALE-style debt-density score. Weights reflect remediation severity:
/// `todo!()`/`unimplemented!()` (panicking incomplete production code) heaviest,
/// then `allow(dead_code/unused)` suppression, then a bare untracked marker
/// (invisible debt), and finally a *tracked* `TODO(#123)` (managed debt — lightest).
/// Density is per source line; calibrated so a clean file scores 1.0 and a file
/// peppered with debt falls below 0.5.
#[cfg(feature = "workspace-integration")]
fn score_tech_debt(
    code_debt: usize,
    suppressions: usize,
    untracked_markers: usize,
    tracked_markers: usize,
    total_lines: usize,
) -> f32 {
    let loc = total_lines.max(1) as f32;
    let weighted = code_debt as f32 * 1.0
        + suppressions as f32 * 0.6
        + untracked_markers as f32 * 0.35
        + tracked_markers as f32 * 0.1;
    let density = weighted / loc;
    // Multiplier 25: ~2 `todo!()` per 100 lines reaches the 0.5 Fail line; a
    // handful of tracked markers in a normal file stays a high Pass. Empirically
    // calibrated against real workspace files (clean → 1.0).
    (1.0 - density * 25.0).clamp(0.0, 1.0)
}

// ── Standalone fallback: raw marker substring density (no boundary/scope) ─────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_tech_debt_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;

    let markers = ["TODO", "FIXME", "HACK", "XXX", "BUG"];
    let total_loc = raw.lines().count().max(1);
    let mut count = 0;
    for m in &markers {
        count += raw.matches(m).count();
    }
    let density = count as f32 / total_loc as f32;
    let value = (1.0 - (density * 100.0).min(1.0)).clamp(0.0, 1.0);
    let evidence = format!(
        "Technical Debt: {count} raw marker matches (substring; build \
         --features workspace-integration for word-bounded comment-scoped debt) — score={value:.3}"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_rs(content: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".rs")
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
    fn test_tech_debt_returns_valid_score() {
        let f = write_temp_rs("fn example() {}\n");
        let s = F1_5_TechDebt.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_tech_debt_empty_file() {
        let f = write_temp("");
        let s = F1_5_TechDebt.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Clean code scores high.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_clean_code_scores_high() {
        let f = write_temp_rs("fn add(a: i32, b: i32) -> i32 { a + b }\n");
        let s = F1_5_TechDebt.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "clean code should score high, got {}",
            s.value
        );
    }

    /// **End-to-end FP fix**: a file full of `DEBUG`/`debugger` identifiers (the
    /// stub's `matches("BUG")` false positive) must NOT be penalised.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_debug_identifiers_not_penalised() {
        let code = "const DEBUG: bool = true;\nfn debugger() {}\nfn debug_log() {}\n\
                    let debugging = DEBUG;\nfn xxx_unrelated() {}\n";
        let f = write_temp_rs(code);
        let s = F1_5_TechDebt.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "DEBUG/debugger/xxx_ are not debt markers, got {}",
            s.value
        );
    }

    /// Real debt (code-debt macros) lowers the score below clean.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_code_debt_lowers_score() {
        let clean = write_temp_rs("fn a() -> i32 { 1 }\nfn b() -> i32 { 2 }\n");
        let debt = write_temp_rs("fn a() { todo!() }\nfn b() { unimplemented!() }\n");
        let sc = F1_5_TechDebt.check(clean.path()).expect("check");
        let sd = F1_5_TechDebt.check(debt.path()).expect("check");
        assert!(
            sd.value < sc.value,
            "code-debt ({}) must score below clean ({})",
            sd.value,
            sc.value
        );
    }

    /// Tracked debt (`TODO(#123)`) weighs lighter than the same count of bare
    /// markers — tested on the score function directly (deterministic; tiny
    /// file fixtures would saturate the density to 0 for both and hide the
    /// difference).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_tracked_debt_lighter_than_untracked() {
        // 3 markers in a 200-line file: untracked weighs 0.35 each, tracked 0.1.
        let untracked = score_tech_debt(0, 0, 3, 0, 200);
        let tracked = score_tech_debt(0, 0, 0, 3, 200);
        assert!(
            tracked > untracked,
            "tracked ({tracked}) should score above untracked ({untracked})"
        );
        assert!(untracked < 1.0, "untracked markers must lower the score");
        assert!(
            (score_tech_debt(0, 0, 0, 0, 200) - 1.0).abs() < 1e-6,
            "no debt → 1.0"
        );
    }

    #[test]
    fn test_detector_own_source_allowlisted() {
        // The debt engine + verifier dirs embed the marker vocabulary.
        assert!(is_detector_own_source(std::path::Path::new(
            "/x/touring-analysis/src/quality/tech_debt.rs"
        )));
        assert!(is_detector_own_source(std::path::Path::new(
            "/x/touring-quality/src/verifications/f1_5_tech_debt.rs"
        )));
        assert!(is_detector_own_source(std::path::Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        // Ordinary production code is scored, not allowlisted.
        assert!(!is_detector_own_source(std::path::Path::new(
            "/x/crates/foo/src/lib.rs"
        )));
    }
}
