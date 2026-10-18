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
    // (1) Non-production harness — test/bench directories and test files (not a sink).
    if crate::verifications::is_non_production_harness(&p) {
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

/// One located match of the F2.1 CWE/OWASP registry.
#[derive(Debug, Clone, PartialEq)]
pub struct OwaspMatch {
    /// The pattern's name (`CMDi`, `SQLi`, ...).
    pub pattern: String,
    /// The CWE number (78 for command injection).
    pub cwe_id: u32,
    /// Severity in `[0, 10]`.
    pub severity: f32,
    /// Byte span of the match in the scanned text.
    pub span: (usize, usize),
    /// 1-based line of the match.
    pub line: usize,
}

/// The F2.1 engine's verdict on one text: **the** CWE/OWASP registry, read by the
/// gate (`touring-quality check --gate F2.1`) and by `touring scan`. Before this type
/// `touring scan` ran a second, hand-kept detector (`touring_code::cwe_scan`, four
/// Rust-hook-latency patterns) and answered "0 CWE(s), p0_block=false" for a
/// `subprocess.run("ls " + user, shell=True)` that the gate scored 0.070.
#[derive(Debug, Clone)]
pub struct OwaspScan {
    /// Language the registry ran as (`python`, `rust`, ...); `unknown` when it did not run.
    pub lang: String,
    /// Every match, in input order.
    pub matches: Vec<OwaspMatch>,
    /// The F2.1 score: `1 - sum(severity)/10`, clamped to `[0, 1]`.
    pub score: f32,
    /// Why the registry did NOT run on this text (allowlisted harness, detector own
    /// source, attack-fixture pragma, engine not built) -- `None` when it ran.
    pub not_scanned: Option<String>,
}

impl OwaspScan {
    fn not_scanned(lang: &str, reason: impl Into<String>) -> Self {
        Self {
            lang: lang.to_string(),
            matches: Vec::new(),
            score: 1.0,
            not_scanned: Some(reason.into()),
        }
    }

    /// Whether the gate BLOCKS on this verdict (F2.1 is a P0 dimension).
    #[must_use]
    pub fn blocks(&self) -> bool {
        crate::DimStatus::from_score(self.score) == crate::DimStatus::Fail
    }

    /// The tier one match of `severity` would put the dimension in on its own:
    /// `P0` (fails it), `P1` (warns) or `P2` -- the gate's own thresholds.
    #[must_use]
    pub fn tier_of(severity: f32) -> &'static str {
        match crate::DimStatus::from_score((1.0 - severity / 10.0).clamp(0.0, 1.0)) {
            crate::DimStatus::Fail => "P0",
            crate::DimStatus::Warn => "P1",
            _ => "P2",
        }
    }
}

/// Run the F2.1 registry over `raw` (the text of `target`), honouring the same
/// allowlists as the gate: non-production harness, detector own source, and the
/// attack-fixture pragma (FILE targets only).
#[must_use]
pub fn owasp_scan(raw: &str, target: &Path) -> OwaspScan {
    if is_detector_own_source(target) {
        return OwaspScan::not_scanned(
            "unknown",
            "detector own source or non-production harness (tests/, benches/): its OWASP markers are detection data, allowlisted",
        );
    }
    if target.is_file() && carries_attack_fixture_pragma(raw) {
        return OwaspScan::not_scanned(
            "unknown",
            format!(
                "file carries `{ALLOW_ATTACK_FIXTURE_PRAGMA}` (attack-payload fixture, explicitly allowlisted)"
            ),
        );
    }
    owasp_matches(raw, target)
}

/// 1-based line of byte `offset` in `raw`.
#[cfg(feature = "workspace-integration")]
fn line_of(raw: &str, offset: usize) -> usize {
    let end = offset.min(raw.len());
    raw.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// The registry itself, no allowlist: language from the extension / shebang, then
/// `SecurityAnalyzer`.
#[cfg(feature = "workspace-integration")]
fn owasp_matches(raw: &str, target: &Path) -> OwaspScan {
    use touring_analysis::quality::SecurityAnalyzer;
    let lang = crate::verifications::lang_for_source(target, raw);
    let report = SecurityAnalyzer::new().analyze(raw, lang);
    let score = if report.vuln_matches.is_empty() {
        1.0
    } else {
        let sum: f32 = report.vuln_matches.iter().map(|v| v.severity).sum();
        (1.0 - sum / 10.0).clamp(0.0, 1.0)
    };
    OwaspScan {
        lang: report.lang.clone(),
        matches: report
            .vuln_matches
            .iter()
            .map(|v| OwaspMatch {
                pattern: v.pattern_name.clone(),
                cwe_id: v.cwe_id,
                severity: v.severity,
                span: v.span,
                line: line_of(raw, v.span.0),
            })
            .collect(),
        score,
        not_scanned: None,
    }
}

/// Without `workspace-integration` there is no registry to run: say so.
#[cfg(not(feature = "workspace-integration"))]
fn owasp_matches(_raw: &str, _target: &Path) -> OwaspScan {
    OwaspScan::not_scanned(
        "unknown",
        "the CWE/OWASP engine is not built into this binary (`workspace-integration` is off)",
    )
}

/// Real engine: delegate to the curated CWE/OWASP registry. F2.1 isolates the
/// *vulnerability* signal (the OWASP/CWE patterns) from generic antipatterns
/// (which belong to F4.1), mirroring `SecurityReport`'s own vuln-score formula.
#[cfg(feature = "workspace-integration")]
fn analyze_owasp(raw: &str, target: &Path) -> (f32, String) {
    let scan = owasp_matches(raw, target);
    let value = scan.score;
    // Name the first match (pattern, line, source text). A bare count forces the
    // author to bisect the file to find out what tripped a BLOCK gate — which is
    // exactly what happened three times on 2026-08-02, all false positives.
    // Vulnerability matches are code patterns, not credentials, so quoting the
    // line is safe here (contrast F2.4, which must redact).
    let evidence = match scan.matches.first() {
        None => format!(
            "OWASP/CWE (SecurityAnalyzer, {} patterns): 0 vulnerability match(es), score={value:.3}",
            scan.lang
        ),
        Some(first) => {
            let (line, excerpt) = crate::verifications::locate_span(raw, first.span, false);
            format!(
                "OWASP/CWE (SecurityAnalyzer, {} patterns): {} vulnerability match(es), \
                 score={value:.3}; first: {} (CWE-{}) at line {line} — `{excerpt}`",
                scan.lang,
                scan.matches.len(),
                first.pattern,
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
        assert!(carries_attack_fixture_pragma(&format!(
            "#!/usr/bin/env python3\n# {pragma}\nx = 1\n"
        )));
        assert!(carries_attack_fixture_pragma(&format!(
            "// {pragma} — benchmark payloads\n"
        )));
        assert!(carries_attack_fixture_pragma(&format!(
            "<!-- {pragma} -->\n"
        )));
        assert!(
            !carries_attack_fixture_pragma(&format!(
                "SKIP = \"{pragma}\"\nq = f\"SELECT {{x}}\"\n"
            )),
            "a string literal is not a declaration"
        );
        assert!(
            !carries_attack_fixture_pragma(&format!(
                "{}# {pragma}\n",
                "x = 1\n".repeat(PRAGMA_HEADER_LINES)
            )),
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
            (
                "touring-quality-score",
                "#!/usr/bin/env bash\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    -o|--output|--output=*)\n      take_lock ;;\n  esac\ndone\n",
            ),
            // scripts/install.sh: `&& curl` is the script, not a payload.
            (
                "install.sh",
                "if curl -fSL -o \"${tmp}/a.sig\" \"${url}.sig\" \\\n   && curl -fSL -o \"${tmp}/a.pem\" \"${url}.pem\"; then\n  log ok\nfi\n",
            ),
            // client/omarchy/bin/cc_build.py: module docstring.
            (
                "cc_build.py",
                "#!/usr/bin/env python3\n\"\"\"Builder.\n\nNo external requests: only 127.0.0.1 URLs and inline <script> (test-enforced).\n\"\"\"\nimport html as _html\n_JS = \"document.body.dataset.ready = 1;\"\n\ndef page(name):\n    return f\"\"\"<!DOCTYPE html>\n<html><body><h1>{_html.escape(name)}</h1>\n<script>\n{_JS}\n</script>\n</body></html>\"\"\"\n",
            ),
            // holon-wasm-components/*/src/lib.rs: the WIT path a macro reads at build time.
            (
                "lib.rs",
                "wit_bindgen::generate!({\n    path: \"../../crates/touring-wasm/wit/holon-core.wit\",\n    world: \"holon-component\",\n});\n",
            ),
            // crates/touring-cli/src/cli_suggester.rs: a flag placeholder in usage text.
            (
                "suggest.rs",
                "const HINT: &str = \"rode `touring run --lang bash --file <script>` de uma vez\";\n",
            ),
            // scripts/_archive/generate_w0_premium_artifacts.py: a symlink in a tree listing.
            (
                "layout.py",
                "LAYOUT = \"\"\"\n├── bin/\n│   ├── touring -> ../../../~/.touring/toolchains/1.0.0/bin/touring\n\"\"\"\n",
            ),
        ];
        for (name, body) in clean {
            let (value, evidence) = score(name, body);
            assert_eq!(value, 1.0, "{name}: {evidence}");
        }
        let sinks = [
            (
                "deploy",
                "#!/bin/sh\nsh -c \"$1\"\npython3 -c 'import os; os.system(f\"ping {h}\")'\n",
            ),
            (
                "handler.py",
                "\"\"\"Doc.\"\"\"\ncur.execute(\n    \"\"\"SELECT a FROM t UNION SELECT password FROM users\"\"\"\n)\n",
            ),
            (
                "page.rs",
                "// the template used to say <script>\nfn page() -> String { format!(\"<script>{}</script>\", user_input()) }\n",
            ),
            (
                "read.rs",
                "fn read(p: &str) -> String { std::fs::read_to_string(format!(\"../../{p}\")).unwrap_or_default() }\n",
            ),
        ];
        for (name, body) in sinks {
            let (value, evidence) = score(name, body);
            assert!(value < 0.5, "{name} must still fail: {evidence}");
        }
    }

    /// F2.1 false positive (09/10/2026, touring-4f), measured through `measure`
    /// on real files — the shape the P0 Write gate scores: the words
    /// `javascript:` in a Python log message are prose (score 1.0), while the
    /// same token as a URL still fails.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn javascript_in_prose_passes_but_a_javascript_url_still_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let score = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, body).expect("write");
            F2_1_Owasp.measure(&path).expect("measure")
        };
        let (value, evidence) = score(
            "probe.py",
            "def f(log):\n    log(\"rota javascript: handlers desativados\")\n",
        );
        assert_eq!(value, 1.0, "prose must pass: {evidence}");
        for (name, body) in [
            (
                "href.py",
                "html = '<a href=\"javascript:alert(1)\">x</a>'\n",
            ),
            ("loc.js", "var x = 1;\nlocation = 'javascript:' + x;\n"),
            ("page.html", "<a href=\"javascript:alert(1)\">x</a>\n"),
        ] {
            let (value, evidence) = score(name, body);
            assert!(value < 0.5, "{name} must still fail: {evidence}");
        }
    }

    /// Follow-ups to the prose fix (09/10/2026), through `measure` on real files: a
    /// scheme BLOCKLIST passes, while the scheme in any letter case as a URL fails.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn a_scheme_blocklist_passes_and_a_cased_javascript_url_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let score = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, body).expect("write");
            F2_1_Owasp.measure(&path).expect("measure")
        };
        let (value, evidence) = score(
            "sanitizer.py",
            "def is_unsafe(url):\n    if url.lower().startswith((\"javascript:\", \"data:\")):\n        return True\n    return url != \"javascript:\"\n",
        );
        assert_eq!(value, 1.0, "a scheme blocklist must pass: {evidence}");
        let (value, evidence) = score(
            "guard.js",
            "function unsafe(u) {\n  if (u.startsWith('javascript:') || u === 'javascript:') { return true; }\n  return false;\n}\n",
        );
        assert_eq!(value, 1.0, "a JS scheme blocklist must pass: {evidence}");
        for (name, body) in [
            (
                "cased.py",
                "html = '<a href=\"JavaScript:alert(1)\">x</a>'\n",
            ),
            ("assign.js", "if (ok) el.href = \"javascript:\" + x;\n"),
            ("cased.js", "location = 'JAVASCRIPT:' + x;\n"),
        ] {
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
        assert_eq!(
            lang_for_source(Path::new("tool"), "#!/usr/bin/env bash\n"),
            "shell"
        );
        assert_eq!(
            lang_for_source(Path::new("tool"), "#!/bin/sh -e\n"),
            "shell"
        );
        assert_eq!(
            lang_for_source(Path::new("tool"), "#!/usr/bin/env -S python3 -u\n"),
            "python"
        );
        assert_eq!(
            lang_for_source(Path::new("tool"), "#!/usr/bin/node\n"),
            "javascript"
        );
        assert_eq!(lang_for_source(Path::new("tool"), "no shebang\n"), "rust");
        assert_eq!(
            lang_for_source(Path::new("a.py"), "#!/bin/bash\n"),
            "python",
            "the extension wins"
        );
    }

    // ---- 17/10/2026: `owasp_scan` is the registry the gate reads ---------------------

    const CMDI_PY: &str =
        "import subprocess, sys\nuser = sys.argv[1]\nsubprocess.run(\"ls \" + user, shell=True)\n";

    fn write_py(content: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".py")
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn owasp_scan_is_the_registry_the_gate_reads() {
        let f = write_py(CMDI_PY);
        let scan = owasp_scan(CMDI_PY, f.path());
        assert!(scan.not_scanned.is_none(), "{scan:?}");
        assert_eq!(scan.lang, "python");
        assert!(
            scan.matches.iter().any(|m| m.cwe_id == 78 && m.line == 3),
            "{scan:?}"
        );
        assert!(scan.blocks(), "{scan:?}");
        let gate = F2_1_Owasp.check(f.path()).expect("gate");
        assert!(
            (gate.value - scan.score).abs() < 1e-6,
            "the gate scored {} and the scan {}",
            gate.value,
            scan.score
        );
        assert_eq!(gate.status == crate::DimStatus::Fail, scan.blocks());
    }

    #[test]
    fn owasp_scan_of_clean_source_is_clean_and_does_not_block() {
        let src = "def add(a, b):\n    return a + b\n";
        let f = write_py(src);
        let scan = owasp_scan(src, f.path());
        assert!(
            scan.not_scanned.is_none() && scan.matches.is_empty(),
            "{scan:?}"
        );
        assert_eq!(scan.score, 1.0);
        assert!(!scan.blocks());
    }

    #[test]
    fn owasp_scan_says_when_it_did_not_run() {
        let dir = tempfile::tempdir().expect("tempdir");
        let harness = dir.path().join("tests/payloads.py");
        std::fs::create_dir_all(harness.parent().expect("parent")).expect("mkdir");
        std::fs::write(&harness, CMDI_PY).expect("write");
        let scan = owasp_scan(CMDI_PY, &harness);
        assert!(
            scan.not_scanned
                .as_deref()
                .is_some_and(|why| why.contains("harness")),
            "{scan:?}"
        );
        assert!(scan.matches.is_empty() && !scan.blocks());

        let fixture = format!("# {ALLOW_ATTACK_FIXTURE_PRAGMA}\n{CMDI_PY}");
        let f = write_py(&fixture);
        let scan = owasp_scan(&fixture, f.path());
        assert!(
            scan.not_scanned
                .as_deref()
                .is_some_and(|why| why.contains("allow-attack-fixture")),
            "{scan:?}"
        );
        // The pragma is honoured for a FILE target only, like the gate.
        let as_dir = owasp_scan(&fixture, dir.path());
        assert!(
            as_dir.not_scanned.is_none(),
            "a directory never opts out: {as_dir:?}"
        );
    }

    #[test]
    fn tier_of_uses_the_gates_own_thresholds() {
        assert_eq!(
            OwaspScan::tier_of(9.3),
            "P0",
            "score 0.07 fails the dimension"
        );
        assert_eq!(OwaspScan::tier_of(5.1), "P0", "score 0.49 fails");
        assert_eq!(OwaspScan::tier_of(3.0), "P1", "score 0.70 only warns");
        assert_eq!(OwaspScan::tier_of(1.0), "P2", "score 0.90 passes");
    }

    #[test]
    fn blocks_is_the_fail_line_of_the_dimension() {
        let at = |score: f32| OwaspScan {
            lang: "python".into(),
            matches: Vec::new(),
            score,
            not_scanned: None,
        };
        assert!(at(0.49).blocks());
        assert!(!at(0.5).blocks(), "0.5 is a warning, not a failure");
        assert!(!at(1.0).blocks());
    }
}
