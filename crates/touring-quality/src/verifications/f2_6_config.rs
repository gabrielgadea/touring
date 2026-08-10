//! F2.6 — Configuration Security verifier (OWASP A05:2021).
//!
//! Precision program (2026-06-22, hybrid architecture, mirrors F2.1/F2.5): under
//! the `workspace-integration` feature this delegates to the real in-workspace
//! engine [`touring_analysis::quality::ConfigSecurityAnalyzer`] — a curated,
//! region-aware OWASP A05 misconfiguration catalog (disabled TLS verification
//! CWE-295, permissive CORS CWE-942, active debug in production CWE-489, insecure
//! cookie flags CWE-614, unsafe CSP CWE-693, world-writable modes CWE-732).
//!
//! The previous W3/W4 stub scored a file by the ratio of `debug!`/`println!`/
//! `env::` occurrences — a false-positive machine (any logging lowered the score)
//! and a total false-negative for actual misconfiguration (it never looked at
//! TLS/CORS/debug settings at all). Without the feature a clearly-labelled
//! substring fallback remains so the crate stays standalone-buildable.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F2.6 verifier — Configuration Security.
#[allow(non_camel_case_types)]
pub struct F2_6_Config;

/// Files that are not a production misconfiguration surface (same principle as
/// F2.1): a non-production test/bench corpus may build an insecure client to
/// exercise error paths, and the security-detector's own source embeds the
/// misconfiguration tokens as detection logic. Scoring either is a false
/// positive (SAST-standard: gitleaks/Semgrep allowlist their own rules + tests).
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
    // Security-detector own source: the ConfigSecurityAnalyzer rule catalog and
    // this verifier's fallback embed misconfig literals (danger_accept_*, CORS *).
    const DETECTOR_SOURCES: [&str; 3] = [
        "touring-quality/src",
        "touring-quality/tests",
        "touring-analysis/src/quality",
    ];
    DETECTOR_SOURCES.iter().any(|s| p.contains(s))
}

/// Byte budget for the *source* half of the directory scan. Mirrors the shared
/// `DIR_SCAN_BYTE_CAP` in [`crate::verifications`] (re-declared: that const is
/// module-private), keeping F2.6's production-only blob self-contained.
const F2_6_SCAN_BYTE_CAP: usize = 2 * 1024 * 1024;

/// Separate budget for the config artifacts, so a large source tree can never
/// spend the artifacts' share.
///
/// Until 2026-08-07 both halves drew on `F2_6_SCAN_BYTE_CAP` from ONE running
/// total, and the source half ran first. On this workspace — 31.6 MB of source
/// against a 2 MB budget — the source loop exhausted it inside the first ~6% of
/// files and returned, so the artifact loop never executed: **94 config files,
/// `.github/workflows/ci.yml` and `dependabot.yml` among them, were never read
/// by the dimension whose entire subject is insecure configuration.** A BLOCK
/// gate that structurally cannot see its own evidence reports PASS for the same
/// reason an unplugged smoke detector stays quiet.
///
/// Config artifacts are small and are this dimension's PRIMARY evidence, so they
/// are now read first and from their own budget; source smells are the secondary
/// signal and take what is left of theirs.
const F2_6_ARTIFACT_BYTE_CAP: usize = 8 * 1024 * 1024;

/// Build the production configuration surface under `target`, EXCLUDING
/// detector-own source and non-production corpora ([`is_detector_own_source`]).
///
/// The shared [`crate::verifications::read_target_source`] concatenates *every*
/// source file, which for a DIRECTORY scope includes this crate's own verifier
/// catalog — whose fallback arrays and tests embed misconfiguration literals
/// (`danger_accept_invalid_certs(true)`, `.allow_any_origin(`, …) as DETECTION
/// LOGIC. Scanning those is a self-match false positive: exactly the case the
/// per-file `is_detector_own_source` guard prevents for a *file* target, but
/// which the directory path bypassed (the guard saw only the directory path, not
/// the individual files folded into the blob). This re-applies the exclusion per
/// enumerated file — for both the source blob and the resolved config artifacts —
/// closing the FP for `ScopeNative` (directory) invocations. Dogfooding
/// 2026-07-02: F2.6 ranked `touring-quality` itself `Unranked` by matching its
/// own rule fixtures.
fn read_production_config_surface(target: &Path) -> Result<String> {
    use crate::verifications::{ArtifactClass, enumerate_source_files, resolve_artifacts};
    if !target.is_dir() {
        // A file target reaches here only when it is NOT detector-own (the guard
        // in `check` returns early otherwise): read it verbatim.
        return crate::verifications::read_target_source(target);
    }
    let mut out = String::new();
    // (1) real config artifacts (yaml/env/ini/…) — test fixtures excluded.
    //     FIRST and on their own budget: they are the dimension's primary
    //     evidence, and running them second let a big source tree starve them
    //     out entirely (see `F2_6_ARTIFACT_BYTE_CAP`).
    for p in resolve_artifacts(target, ArtifactClass::Config) {
        if is_detector_own_source(&p) {
            continue;
        }
        if let Ok(s) = std::fs::read_to_string(&p) {
            out.push('\n');
            out.push_str(&s);
            if out.len() >= F2_6_ARTIFACT_BYTE_CAP {
                return Ok(out);
            }
        }
    }
    // (2) code-embedded misconfig smells — detector-own source excluded. The
    //     cap applies to the SOURCE bytes alone, so the artifacts already read
    //     neither consume this budget nor are consumed by it.
    let artifact_bytes = out.len();
    for p in enumerate_source_files(target) {
        if is_detector_own_source(&p) {
            continue;
        }
        if let Ok(s) = std::fs::read_to_string(&p) {
            out.push_str(&s);
            out.push('\n');
            if out.len() - artifact_bytes >= F2_6_SCAN_BYTE_CAP {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

/// Real engine: delegate to the curated OWASP A05 misconfiguration catalog.
#[cfg(feature = "workspace-integration")]
fn analyze_config(raw: &str, target: &Path) -> (f32, String) {
    use touring_analysis::quality::ConfigSecurityAnalyzer;
    let lang = crate::verifications::lang_from_ext(target);
    let report = ConfigSecurityAnalyzer::new().analyze(raw, lang);
    let evidence = if report.misconfigs.is_empty() {
        "Config Security (OWASP A05, ConfigSecurityAnalyzer): 0 misconfigurations, score=1.000"
            .to_string()
    } else {
        let mut names: Vec<String> = report
            .misconfigs
            .iter()
            .map(|m| format!("{} (CWE-{})", m.pattern_name, m.cwe_id))
            .collect();
        names.sort();
        names.dedup();
        let shown = names[..names.len().min(4)].join(", ");
        format!(
            "Config Security (OWASP A05): {} misconfiguration(s): {}{} — score={:.3}",
            report.misconfigs.len(),
            shown,
            if names.len() > 4 {
                format!(" (+{} more)", names.len() - 4)
            } else {
                String::new()
            },
            report.score
        )
    };
    (report.score, evidence)
}

/// Fallback (no `workspace-integration`): the few highest-signal misconfig tokens.
/// Explicitly labelled — it cannot region-suppress comments/tests like the engine.
#[cfg(not(feature = "workspace-integration"))]
fn analyze_config(raw: &str, _target: &Path) -> (f32, String) {
    const CRITICAL: [&str; 4] = [
        "danger_accept_invalid_certs(true)",
        "rejectunauthorized: false",
        "insecureskipverify: true",
        ".allow_any_origin(",
    ];
    let lower = raw.to_ascii_lowercase();
    let hit = CRITICAL.iter().any(|p| lower.contains(p));
    let value = if hit { 0.0 } else { 1.0 };
    let evidence = format!(
        "Config Security (substring fallback — build --features workspace-integration for the OWASP A05 ConfigSecurityAnalyzer): score={value:.3}"
    );
    (value, evidence)
}

impl Verification for F2_6_Config {
    fn id(&self) -> DimId {
        DimId::F2_6
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        if is_detector_own_source(target) {
            return Ok((
                1.0,
                "Config Security: detector own source / non-production corpus — allowlisted (score=1.000)"
                    .to_string(),
            ));
        }
        // F2.6 is HYBRID: config-security smells live BOTH in code (`verify=False`,
        // `debug=True`, CORS `*` in a handler) AND in real config files
        // (yaml/env/ini/conf/toml). The scan blob is built by
        // `read_production_config_surface`, which — unlike the shared
        // `read_target_source` — EXCLUDES detector-own source and non-production
        // corpora per file. Those files embed the misconfiguration literals as
        // DETECTION LOGIC; concatenating them made F2.6 self-match and rank this
        // very crate `Unranked` (dogfooding 2026-07-02). The per-file exclusion
        // mirrors the `is_detector_own_source` guard above, which only covers a
        // single-FILE target.
        let raw = read_production_config_surface(target)?;
        let (value, evidence) = analyze_config(&raw, target);
        Ok((value, evidence))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    /// A source tree bigger than the source budget must NOT cost the dimension
    /// its config artifacts.
    ///
    /// Before 2026-08-07 both halves shared one running total and source ran
    /// first, so on this workspace (31.6 MB of source against a 2 MB budget)
    /// the artifact loop was never reached and all 94 config files — CI
    /// workflows included — went unread. The assertion is deliberately about
    /// the artifact's CONTENT: a budget that merely "ran" proves nothing.
    #[test]
    fn oversized_source_tree_never_starves_the_config_artifacts() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Comfortably past F2_6_SCAN_BYTE_CAP so the source half must stop early.
        let filler = "pub fn f() {}\n".repeat(40_000); // ~560 KB each
        for i in 0..8 {
            std::fs::write(dir.path().join(format!("big{i}.rs")), &filler).expect("write source");
        }
        std::fs::write(
            dir.path().join("service.yml"),
            "server:\n  tls_verify: false\n",
        )
        .expect("write config");

        let surface = read_production_config_surface(dir.path()).expect("surface");
        assert!(
            surface.contains("tls_verify: false"),
            "config artifact must be present regardless of source-tree size \
             (surface = {} bytes)",
            surface.len()
        );
    }

    #[test]
    fn detector_own_source_is_allowlisted() {
        assert!(is_detector_own_source(std::path::Path::new(
            "/repo/touring-analysis/src/quality/config_security.rs"
        )));
        assert!(is_detector_own_source(std::path::Path::new(
            "/repo/some-crate/tests/integration.rs"
        )));
        assert!(!is_detector_own_source(std::path::Path::new(
            "/repo/some-crate/src/server.rs"
        )));
    }

    #[test]
    fn test_config_returns_valid_score() {
        let f = write_temp("fn example() {}\n");
        let s = F2_6_Config.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_config_empty_file() {
        let f = write_temp("");
        let s = F2_6_Config.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    // Real-engine behaviour (default `workspace-integration`).
    #[cfg(feature = "workspace-integration")]
    mod real_engine {
        use super::*;
        // The tests assert on the DimStatus that `check` derives; the verifier
        // itself only returns (score, evidence) now.
        use crate::DimStatus;

        #[test]
        fn tls_disabled_blocks() {
            let f = write_temp("fn c() { let _ = b.danger_accept_invalid_certs(true).build(); }\n");
            let s = F2_6_Config.check(f.path()).expect("check");
            assert_eq!(s.status, DimStatus::Fail, "TLS-off must BLOCK: {}", s.value);
            assert!(s.evidence.contains("CWE-295"));
        }

        #[test]
        fn clean_server_passes() {
            let f = write_temp("fn handler() -> Result<(), ()> { Ok(()) }\n");
            let s = F2_6_Config.check(f.path()).expect("check");
            assert!((s.value - 1.0).abs() < 1e-6, "clean file: {}", s.value);
        }

        #[test]
        fn logging_is_not_a_misconfig() {
            // The old stub penalised this; the real engine must not.
            let f = write_temp(
                "fn f() { println!(\"hi\"); debug!(\"x\"); let _ = std::env::var(\"H\"); }\n",
            );
            let s = F2_6_Config.check(f.path()).expect("check");
            assert!(
                (s.value - 1.0).abs() < 1e-6,
                "logging must not be flagged: {}",
                s.value
            );
        }

        #[test]
        fn dir_scope_excludes_detector_own_and_test_corpora() {
            // Regression (dogfooding 2026-07-02): scoring a DIRECTORY must not
            // self-match misconfig literals embedded as DETECTION LOGIC in a
            // detector-own catalog or a test corpus. Before the fix the dir blob
            // concatenated those files → 0.0 (BLOCK) → the crate ranked Unranked.
            let dir = tempfile::tempdir().expect("tempdir");
            // (a) detector-own catalog: path carries the `touring-quality/src`
            //     marker that `is_detector_own_source` allowlists.
            let det = dir.path().join("touring-quality/src");
            std::fs::create_dir_all(&det).expect("mkdir det");
            std::fs::write(
                det.join("f2_6_config.rs"),
                "const CATALOG: [&str; 2] = [\".allow_any_origin(\", \"danger_accept_invalid_certs(true)\"];\n",
            )
            .expect("write det");
            // (b) test corpus: path carries `/tests/`.
            let tst = dir.path().join("tests");
            std::fs::create_dir_all(&tst).expect("mkdir tst");
            std::fs::write(
                tst.join("fixture.rs"),
                "fn f() { let _ = b.danger_accept_invalid_certs(true).build(); }\n",
            )
            .expect("write tst");
            // (c) a clean production file.
            std::fs::write(dir.path().join("lib.rs"), "pub fn ok() {}\n").expect("write lib");

            let s = F2_6_Config.check(dir.path()).expect("check");
            assert_eq!(
                s.status,
                DimStatus::Pass,
                "detector/test literals must not BLOCK a dir scope: value={} evidence={}",
                s.value,
                s.evidence
            );
        }

        #[test]
        fn dir_scope_still_flags_real_production_misconfig() {
            // No false negative: a REAL misconfig in a production source file
            // under the directory must still BLOCK (CWE-295, TLS off).
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::write(
                dir.path().join("server.rs"),
                "fn build() { let _ = client.danger_accept_invalid_certs(true).build(); }\n",
            )
            .expect("write server");
            let s = F2_6_Config.check(dir.path()).expect("check");
            assert_eq!(
                s.status,
                DimStatus::Fail,
                "real production misconfig must BLOCK: value={}",
                s.value
            );
            assert!(s.evidence.contains("CWE-295"), "evidence: {}", s.evidence);
        }
    }
}
