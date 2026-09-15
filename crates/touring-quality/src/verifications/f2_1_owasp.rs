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
    let lang = crate::verifications::lang_for_source(target, raw);
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
        let raw = crate::verifications::read_security_source(target)?;
        // File-level opt-out for fixtures that embed attack payloads ON PURPOSE —
        // a benchmark scenario asserting that `UNION SELECT …` is blocked must
        // hold the payload (cross-audit 14/09/2026: `docs/agentic-bench/run_bench.py`
        // zeroed F2.1). Same convention as `touring-quality:allow-secrets`:
        // narrow, auditable, grep-able — and honoured only for a FILE target, so
        // one fixture in a directory never allowlists the directory's other files,
        // and only as a header comment (`carries_attack_fixture_pragma`).
        if target.is_file() && carries_attack_fixture_pragma(&raw) {
            return Ok((
                1.0,
                format!(
                    "OWASP Top 10: file carries `{ALLOW_ATTACK_FIXTURE_PRAGMA}` \
                     (attack-payload fixture, explicitly allowlisted) — score=1.000"
                ),
            ));
        }
        let (value, evidence) = analyze_owasp(&raw, target);
        Ok((value, evidence))
    }
}

/// File-level opt-out marker for fixtures that embed OWASP attack payloads on
/// purpose (benchmarks and detector corpora).
pub(crate) const ALLOW_ATTACK_FIXTURE_PRAGMA: &str = "touring-quality:allow-attack-fixture";

/// How many leading lines may carry the pragma.
const PRAGMA_HEADER_LINES: usize = 5;

/// Whether the file declares itself an attack fixture: the pragma in a COMMENT
/// line among the first [`PRAGMA_HEADER_LINES`].
///
/// A bare substring match let any production file silence this P0 gate with the
/// string in a literal, a docstring or a log message, anywhere in the file
/// (cross-audit 14/09/2026, R2-4). A declaration sits at the top, in a comment.
fn carries_attack_fixture_pragma(raw: &str) -> bool {
    const COMMENT_PREFIXES: &[&str] = &["#", "//", "--", "/*", "*", "<!--", ";"];
    raw.lines().take(PRAGMA_HEADER_LINES).any(|line| {
        let line = line.trim_start();
        COMMENT_PREFIXES.iter().any(|p| line.starts_with(p))
            && line.contains(ALLOW_ATTACK_FIXTURE_PRAGMA)
    })
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

    /// Cross-audit 14/09/2026: an attack-payload fixture opts out with
    /// `touring-quality:allow-attack-fixture` — as a FILE; the pragma never
    /// allowlists the directory it sits in.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn the_attack_fixture_pragma_is_a_file_opt_out_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let payload = "let q = \"' OR '1'='1\"; let u = \"UNION SELECT password FROM users\";\n";
        std::fs::write(
            dir.path().join("bench.py"),
            format!("# touring-quality:allow-attack-fixture\n{payload}"),
        )
        .expect("fixture");
        std::fs::write(dir.path().join("handler.py"), payload).expect("real sink");
        let (fixture, _) = F2_1_Owasp
            .measure(&dir.path().join("bench.py"))
            .expect("fixture");
        assert_eq!(fixture, 1.0, "the fixture opts out");
        let (real, _) = F2_1_Owasp
            .measure(&dir.path().join("handler.py"))
            .expect("real");
        assert!(real < 0.5, "the same payload without the pragma fails");
        let (whole, evidence) = F2_1_Owasp.measure(dir.path()).expect("dir");
        assert!(
            whole < 0.5,
            "the directory is not allowlisted by one fixture: {evidence}"
        );
    }

    /// Cross-audit 14/09/2026 (R2-4): the pragma is a header comment. In a
    /// string literal, or in a comment far below the top, it allowlists nothing.
    #[test]
    fn only_a_header_comment_declares_an_attack_fixture() {
        let pragma = ALLOW_ATTACK_FIXTURE_PRAGMA;
        assert!(carries_attack_fixture_pragma(&format!("#!/usr/bin/env python3\n# {pragma}\nx = 1\n")));
        assert!(carries_attack_fixture_pragma(&format!("// {pragma} — benchmark payloads\n")));
        assert!(carries_attack_fixture_pragma(&format!("<!-- {pragma} -->\n")));
        assert!(
            !carries_attack_fixture_pragma(&format!("SKIP = \"{pragma}\"\nq = f\"SELECT {{x}}\"\n")),
            "a string literal is not a declaration"
        );
        assert!(
            !carries_attack_fixture_pragma(&format!("{}# {pragma}\n", "x = 1\n".repeat(PRAGMA_HEADER_LINES))),
            "a comment below the header is not a declaration"
        );
    }

    /// Cross-audit R2 (14/09/2026): the shapes of the seven workspace files that
    /// failed this P0 gate without a vulnerability, measured through `measure`
    /// on real files, next to the sink of the same class that must still fail.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn r2_workspace_false_positives_pass_and_real_sinks_still_fail() {
        let dir = tempfile::tempdir().expect("tempdir");
        let score = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, body).expect("write");
            F2_1_Owasp.measure(&path).expect("measure")
        };
        let clean = [
            // scripts/touring-quality-score: extensionless bash, `case` glob.
            ("touring-quality-score", "#!/usr/bin/env bash\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    -o|--output|--output=*)\n      take_lock ;;\n  esac\ndone\n"),
            // scripts/install.sh: `&& curl` is the script, not a payload.
            ("install.sh", "if curl -fSL -o \"${tmp}/a.sig\" \"${url}.sig\" \\\n   && curl -fSL -o \"${tmp}/a.pem\" \"${url}.pem\"; then\n  log ok\nfi\n"),
            // client/omarchy/bin/cc_build.py: module docstring.
            ("cc_build.py", "#!/usr/bin/env python3\n\"\"\"Builder.\n\nNo external requests: only 127.0.0.1 URLs and inline <script> (test-enforced).\n\"\"\"\nimport html as _html\n_JS = \"document.body.dataset.ready = 1;\"\n\ndef page(name):\n    return f\"\"\"<!DOCTYPE html>\n<html><body><h1>{_html.escape(name)}</h1>\n<script>\n{_JS}\n</script>\n</body></html>\"\"\"\n"),
            // holon-wasm-components/*/src/lib.rs: the WIT path a macro reads at build time.
            ("lib.rs", "wit_bindgen::generate!({\n    path: \"../../crates/touring-wasm/wit/holon-core.wit\",\n    world: \"holon-component\",\n});\n"),
            // crates/touring-cli/src/cli_suggester.rs: a flag placeholder in usage text.
            ("suggest.rs", "const HINT: &str = \"rode `touring run --lang bash --file <script>` de uma vez\";\n"),
            // scripts/_archive/generate_w0_premium_artifacts.py: a symlink in a tree listing.
            ("layout.py", "LAYOUT = \"\"\"\n├── bin/\n│   ├── touring -> ../../../~/.touring/toolchains/1.0.0/bin/touring\n\"\"\"\n"),
        ];
        for (name, body) in clean {
            let (value, evidence) = score(name, body);
            assert_eq!(value, 1.0, "{name}: {evidence}");
        }
        let sinks = [
            ("deploy", "#!/bin/sh\nsh -c \"$1\"\npython3 -c 'import os; os.system(f\"ping {h}\")'\n"),
            ("handler.py", "\"\"\"Doc.\"\"\"\ncur.execute(\n    \"\"\"SELECT a FROM t UNION SELECT password FROM users\"\"\"\n)\n"),
            ("page.rs", "// the template used to say <script>\nfn page() -> String { format!(\"<script>{}</script>\", user_input()) }\n"),
            ("read.rs", "fn read(p: &str) -> String { std::fs::read_to_string(format!(\"../../{p}\")).unwrap_or_default() }\n"),
        ];
        for (name, body) in sinks {
            let (value, evidence) = score(name, body);
            assert!(value < 0.5, "{name} must still fail: {evidence}");
        }
    }

    #[cfg(feature = "workspace-integration")]
    #[test]
    fn the_security_scan_reads_shell_and_shebangs() {
        use crate::verifications::lang_for_source;
        use std::path::Path;
        assert_eq!(lang_for_source(Path::new("install.sh"), ""), "shell");
        assert_eq!(lang_for_source(Path::new("run.bash"), ""), "shell");
        assert_eq!(lang_for_source(Path::new("tool"), "#!/usr/bin/env bash\n"), "shell");
        assert_eq!(lang_for_source(Path::new("tool"), "#!/bin/sh -e\n"), "shell");
        assert_eq!(lang_for_source(Path::new("tool"), "#!/usr/bin/env -S python3 -u\n"), "python");
        assert_eq!(lang_for_source(Path::new("tool"), "#!/usr/bin/node\n"), "javascript");
        assert_eq!(lang_for_source(Path::new("tool"), "no shebang\n"), "rust");
        assert_eq!(lang_for_source(Path::new("a.py"), "#!/bin/bash\n"), "python", "the extension wins");
    }
}
