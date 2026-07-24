//! F4.1 — Language Idioms verifier (D40).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_idioms`] — a polyglot detector of
//! non-idiomatic constructs across **7 languages**, approximating a
//! high-confidence subset of each language's lint oracle: clippy (Rust —
//! `len_zero`, `bool_comparison`, `ptr_arg`, …), ruff (Python — E711/E712/E721/
//! E731/E722), ESLint (TS/JS — `eqeqeq` char-aware, `no-var`, `no-explicit-any`,
//! …), go vet (`interface{}`→`any`), clang-tidy (C++), and Java legacy APIs.
//! Comments and `#[cfg(test)]`/test regions are excluded via `code_regions`.
//!
//! This replaces a stub that counted `let ` + `match ` occurrences and returned
//! `1.0` above five — a metric with no relationship to idiomaticity.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior `let`/`match`
//! count heuristic, labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`). A per-file scanner cannot
//! replace a type-aware linter, so it is honest about catching a subset.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F4.1 verifier — Language Idioms.
#[allow(non_camel_case_types)]
pub struct F4_1_Idioms;

impl Verification for F4_1_Idioms {
    fn id(&self) -> DimId {
        DimId::F4_1
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_idioms_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: polyglot idiom detection ─────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_idioms_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_idioms, score_idioms};

    // The idiom engine (`idioms.rs`) embeds every needle (`b".len() == 0"`,
    // `b"== None"`, …) as data, so scanning its own source is a self-match FP.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F4.1: detector own source (idiom needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_idioms(&raw, lang);

    let value = score_idioms(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F4.1: {} non-idiomatic construct(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_idioms: clippy/ruff/ESLint/go-vet/clang-tidy idioms){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The idiom engine and this verifier embed the needle vocabulary
/// (`b".len() == 0"`, `b"== None"`, …) as detection data, so scoring their own
/// source is a self-match false positive. Mirrors
/// `f1_5_tech_debt::is_detector_own_source` (test/bench dirs + the quality
/// engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: let/match count heuristic (no idiom awareness) ────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_idioms_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let modern = raw.matches("let ").count() + raw.matches("match ").count();
    let value = if modern > 5 { 1.0 } else { 0.7 };
    let evidence = format!(
        "{modern} let/match constructs (heuristic; build --features workspace-integration \
         for polyglot idiom analysis)"
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
    fn test_idioms_returns_valid_score() {
        let f = write_temp_ext("fn f(v: &[u32]) -> bool { v.is_empty() }\n", ".rs");
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value), "out of range: {}", s.value);
    }

    #[test]
    fn test_idioms_empty_file() {
        let f = write_temp("");
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Idiomatic rust → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_idiomatic_rust_high() {
        let f = write_temp_ext(
            "fn f(v: &[u32]) -> bool {\n    if v.is_empty() {\n        return true;\n    }\n    v.first().is_some()\n}\n",
            ".rs",
        );
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "idiomatic rust should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: non-idiomatic rust (`.len()==0`, `&Vec`, `==true`)
    /// scores below idiomatic — the stub (counting `let`/`match`) could not tell.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_non_idiomatic_rust_lower() {
        let bad = write_temp_ext(
            "fn f(v: &Vec<u32>) -> bool {\n    if v.len() == 0 {\n        return false;\n    }\n    v.is_empty() == true\n}\n",
            ".rs",
        );
        let good = write_temp_ext("fn f(v: &[u32]) -> bool { v.is_empty() }\n", ".rs");
        let sb = F4_1_Idioms.check(bad.path()).expect("check");
        let sg = F4_1_Idioms.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "non-idiomatic ({}) < idiomatic ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: Python `== None`/`== True` are flagged via the `.py` extension.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_python_idioms_flagged() {
        let f = write_temp_ext(
            "def f(x):\n    if x == None:\n        return\n    if x == True:\n        pass\n",
            ".py",
        );
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!(
            s.value < 1.0,
            "python E711/E712 should lower the score, got {}",
            s.value
        );
    }

    /// Polyglot: TS `eqeqeq` flags loose `==` but not strict `===`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_ts_eqeqeq_polyglot() {
        let loose = write_temp_ext(
            "function f(a: number, b: number) { return a == b; }\n",
            ".ts",
        );
        let strict = write_temp_ext(
            "function f(a: number, b: number) { return a === b; }\n",
            ".ts",
        );
        let sl = F4_1_Idioms.check(loose.path()).expect("check");
        let ss = F4_1_Idioms.check(strict.path()).expect("check");
        assert!(
            sl.value < ss.value,
            "loose == ({}) must score below strict === ({})",
            sl.value,
            ss.value
        );
        assert!(
            (ss.value - 1.0).abs() < 1e-6,
            "strict TS is idiomatic, got {}",
            ss.value
        );
    }

    /// The idiom engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/idioms.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
