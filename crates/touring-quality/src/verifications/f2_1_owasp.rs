//! F2.1 — OWASP Top 10 verifier.
//!
//! Precision program (2026-06-21, hybrid architecture): under the
//! `workspace-integration` feature this delegates to the real in-workspace
//! engine `touring_analysis::quality::SecurityAnalyzer`, which composes the
//! curated CWE/OWASP `PatternRegistry` (10 detectors: CmdInjection CWE-78,
//! SQLi CWE-89, XSS CWE-79, PathTraversal CWE-22, Deserialization CWE-502,
//! SSRF CWE-918, LDAP/XML injection, Buffer/Integer overflow). The previous
//! 7-substring list was both a false-positive machine (matched benign
//! `regex.exec(...)`) and a false-negative one (missed `execSync(\`…${x}…\`)`).
//! Without the feature, a clearly-labelled substring fallback remains so the
//! crate stays standalone-buildable.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F2.1 verifier — OWASP Top 10.
#[allow(non_camel_case_types)]
pub struct F2_1_Owasp;

/// Allowlist files that are not a production OWASP attack surface for F2.1.
///
/// Two principled exclusions, both SAST-standard (gitleaks/Semgrep allowlist
/// their own rules + fixtures; Semgrep's default `.semgrepignore` excludes
/// `test/`, `tests/`, `*_test.go`):
///
/// 1. **Non-production harness** — a payload literal in a test corpus or a
///    benchmark fixture is a detector *input*, not a deployed sink. F2.1 scores
///    *production* exposure, so `tests/`/`benches/` are excluded (e.g. the CEG's
///    `benches/ceg_baseline.rs` feeding `shell=True` to measure detection
///    latency). NB: F2.1-specific — `f2_4_secrets` must still scan tests, since
///    a hardcoded credential in a fixture is genuinely leakable.
/// 2. **Security-detector own source** — the verifiers, the `touring-offensive`
///    CWE `PatternRegistry` + concolic executor, the `touring-analysis`
///    `SecurityAnalyzer`, and the `touring-hooks-shared` ast-grep catalogs embed
///    OWASP/CWE markers as detection logic; scoring them is a false positive on
///    the engines' own source (the meta-finding from the 2026-06-20 review).
fn is_detector_own_source(target: &Path) -> bool {
    let canonical = target.canonicalize();
    let p = canonical
        .as_deref()
        .map(|c| c.to_string_lossy())
        .unwrap_or_else(|_| target.to_string_lossy());
    // (1) Non-production harness directories (test/bench code is not a sink).
    const NON_PRODUCTION_DIRS: [&str; 4] = ["/tests/", "/test/", "/benches/", "/bench/"];
    if NON_PRODUCTION_DIRS.iter().any(|s| p.contains(s)) {
        return true;
    }
    // (2) Security-detector own source.
    const DETECTOR_SOURCES: [&str; 8] = [
        "touring-quality/src",
        "touring-quality/tests",
        // The offensive-security engine: CWE PatternRegistry + concolic executor +
        // fuzzing — its source processes attack patterns as data by design.
        "touring-offensive/src",
        // The SecurityAnalyzer engine this verifier delegates to: its antipattern
        // detectors embed dangerous-call literals, and its tests are attack-corpora.
        "touring-analysis/src/quality",
        "touring-analysis/tests",
        // ast-grep pattern catalogs (string literals of dangerous calls).
        "touring-hooks-shared/src/forbidden_patterns",
        "touring-hooks-shared/src/risk_patterns",
        "touring-hooks-shared/src/antipatterns",
    ];
    DETECTOR_SOURCES.iter().any(|s| p.contains(s))
}

/// Real engine: delegate to the curated CWE/OWASP registry. F2.1 isolates the
/// *vulnerability* signal (the OWASP/CWE patterns) from generic antipatterns
/// (which belong to F4.1), mirroring `SecurityReport`'s own vuln-score formula.
#[cfg(feature = "workspace-integration")]
fn analyze_owasp(raw: &str, target: &Path) -> (f32, String) {
    use touring_analysis::quality::SecurityAnalyzer;
    let lang = crate::verifications::lang_from_ext(target);
    let report = SecurityAnalyzer::new().analyze(raw, lang);
    let value = if report.vuln_matches.is_empty() {
        1.0
    } else {
        let sum: f32 = report.vuln_matches.iter().map(|v| v.severity).sum();
        (1.0 - sum / 10.0).clamp(0.0, 1.0)
    };
    // Name the first match (pattern, line, source text). A bare count forces the
    // author to bisect the file to find out what tripped a BLOCK gate — which is
    // exactly what happened three times on 2026-08-02, all false positives.
    // Vulnerability matches are code patterns, not credentials, so quoting the
    // line is safe here (contrast F2.4, which must redact).
    let evidence = match report.vuln_matches.first() {
        None => format!(
            "OWASP/CWE (SecurityAnalyzer, {} patterns): 0 vulnerability match(es), score={value:.3}",
            report.lang
        ),
        Some(first) => {
            let (line, excerpt) =
                crate::verifications::locate_span(raw, (first.span.0, first.span.1), false);
            format!(
                "OWASP/CWE (SecurityAnalyzer, {} patterns): {} vulnerability match(es), \
                 score={value:.3}; first: {} (CWE-{}) at line {line} — `{excerpt}`",
                report.lang,
                report.vuln_matches.len(),
                first.pattern_name,
                first.cwe_id,
            )
        }
    };
    (value, evidence)
}

/// Fallback (no `workspace-integration`): coarse substring sinks. Explicitly
/// labelled as a fallback — it cannot separate `regex.exec()` from `os.system()`.
#[cfg(not(feature = "workspace-integration"))]
fn analyze_owasp(raw: &str, _target: &Path) -> (f32, String) {
    let dangerous = ["eval(", "system(", "popen(", "pickle.loads", "yaml.load"];
    let mut hits = 0;
    for p in &dangerous {
        hits += raw.matches(p).count();
    }
    let value = if hits == 0 { 1.0 } else { 0.0 };
    let evidence = format!(
        "OWASP Top 10 (substring fallback — build --features workspace-integration for the SecurityAnalyzer CWE/OWASP engine): score={value:.3}"
    );
    (value, evidence)
}

impl Verification for F2_1_Owasp {
    fn id(&self) -> DimId {
        DimId::F2_1
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        if is_detector_own_source(target) {
            return Ok((
                1.0,
                "OWASP Top 10: detector own source — markers are detection patterns, allowlisted (score=1.000)"
                    .to_string(),
            ));
        }
        let raw = crate::verifications::read_target_source(target)?;
        let (value, evidence) = analyze_owasp(&raw, target);
        Ok((value, evidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn detector_own_source_is_allowlisted() {
        assert!(is_detector_own_source(std::path::Path::new(
            "/repo/touring-quality/src/verifications/f2_1_owasp.rs"
        )));
        assert!(!is_detector_own_source(std::path::Path::new(
            "/some/other/project/src/main.rs"
        )));
        // Non-production harness (tests/benches) is excluded for F2.1.
        assert!(is_detector_own_source(std::path::Path::new(
            "/repo/touring-hooks/benches/ceg_baseline.rs"
        )));
        assert!(is_detector_own_source(std::path::Path::new(
            "/repo/some-crate/tests/integration.rs"
        )));
        // …but a plain production src file is still scanned.
        assert!(!is_detector_own_source(std::path::Path::new(
            "/repo/some-crate/src/handler.rs"
        )));
    }

    #[test]
    fn test_owasp_returns_valid_score() {
        let f = write_temp("fn example() {}\n");
        let s = F2_1_Owasp.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_owasp_empty_file() {
        let f = write_temp("");
        let s = F2_1_Owasp.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
}
