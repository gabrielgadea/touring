//! `touring scan [vulnerabilities] <file>` — CWE vulnerability scan of a source file.
//!
//! Returns cwes array (id, severity, line) + p0_block flag.
//!
//! Complementação-hooks H6 (F1.5 + F4): handler PostToolUse(Write) needs this
//! subcommand to inject `touring.scan_vulnerabilities` JSON via additionalContext.
//!
//! F4 (2026-08-31): wires real CWE detection — reads the source from disk.
//! S8 (2026-09-02): the hook-layer detector is `touring_code::cwe_scan::detect_cwes`,
//! the SAME one the hook layer runs (the inline copy that lived here had drifted: it
//! never emitted CWE-22). This module only maps findings to the CLI JSON.
//!
//! 17/10/2026: `scan` and the F2.1 gate disagreed. `detect_cwes` is four
//! Rust-hook-latency patterns (CWE-798/89/22/394); the F2.1 gate runs the polyglot
//! CWE/OWASP registry (`SecurityAnalyzer`: command injection, SQLi, XSS, path
//! traversal, deserialization, SSRF, ...). On `subprocess.run("ls " + user,
//! shell=True)` the gate said 0.070 and `scan` said "0 CWE(s), p0_block=false" — a
//! mute zero from a tool that never looked. `scan` now runs BOTH: the hook-layer
//! patterns and the gate's own registry (`f2_1_owasp::owasp_scan`, one
//! implementation), maps the registry's matches through the gate's own thresholds, and
//! DECLARES what it did not scan (`engines`, `not_scanned`) instead of answering 0.

use super::common::{human_to_stderr, json_to_stdout, parse_global_flags};
use anyhow::Context;
use std::fs;
use std::path::Path;

#[derive(serde::Serialize)]
struct Cwe {
    id: String,
    severity: &'static str, // P0 | P1 | P2
    line: u32,
    description: String,
}

/// One detector `scan` consulted, and whether it actually ran on this file.
#[derive(serde::Serialize)]
struct Engine {
    name: &'static str,
    ran: bool,
    covers: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(serde::Serialize)]
struct ScanResult {
    file_path: String,
    lang: String,
    cwes: Vec<Cwe>,
    p0_block: bool,
    /// The detectors consulted — a `ran: false` entry says why a zero is not a clean bill.
    engines: Vec<Engine>,
    /// Security dimensions this command never looks at (see the gate for them).
    not_scanned: Vec<&'static str>,
}

/// What no `scan` run covers: the other security dimensions of the 50-dim harness.
const NOT_SCANNED: [&str; 5] = [
    "F2.2 input validation (touring-quality check --gate F2.2)",
    "F2.3 authentication/authorization (touring-quality check --gate F2.3)",
    "F2.4 hardcoded secrets beyond CWE-798 vendor-prefix literals (touring-quality check --gate F2.4)",
    "F2.5 dependency CVEs (touring-quality check --gate F2.5)",
    "F2.6 configuration security (touring-quality check --gate F2.6)",
];

/// Detect CWEs in source text with the hook-layer patterns.
///
/// S8 (2026-09-02): delegates to the ONE shared detector
/// (`touring_code::cwe_scan::detect_cwes`) — the inline copy that used to live
/// here had drifted from the hook layer's (it never emitted CWE-22). The CLI
/// only maps findings to its JSON shape.
fn detect_cwes_in_source(source: &str) -> Vec<Cwe> {
    touring_code::cwe_scan::detect_cwes(source)
        .into_iter()
        .map(|f| Cwe {
            id: f.id,
            severity: f.severity.as_str(),
            line: f.line,
            description: f.description,
        })
        .collect()
}

/// Scan `source` (the text of `path`) with the hook-layer patterns AND the F2.1
/// CWE/OWASP registry, and report which of them ran.
fn scan_source(source: &str, path: &Path) -> ScanResult {
    use touring_quality::verifications::f2_1_owasp::{OwaspScan, owasp_scan};

    let mut cwes = detect_cwes_in_source(source);
    let owasp = owasp_scan(source, path);
    for m in &owasp.matches {
        let id = format!("CWE-{}", m.cwe_id);
        let line = u32::try_from(m.line).unwrap_or(u32::MAX);
        let tier = OwaspScan::tier_of(m.severity);
        // The hook-layer detector may already have reported the same rule there: keep
        // ONE entry, at the more severe of the two tiers ("P0" < "P1" < "P2").
        if let Some(seen) = cwes.iter_mut().find(|c| c.id == id && c.line == line) {
            if tier < seen.severity {
                seen.severity = tier;
            }
            continue;
        }
        cwes.push(Cwe {
            id,
            severity: tier,
            line,
            description: format!(
                "{} (F2.1 CWE/OWASP registry, severity {:.1})",
                m.pattern, m.severity
            ),
        });
    }
    // The gate blocks on the registry's SUM of severities (several P1s can fail F2.1
    // together), not only on a single P0 match — use its own verdict.
    let p0_block = owasp.blocks() || cwes.iter().any(|c| c.severity == "P0");

    let engines = vec![
        Engine {
            name: "cwe_scan",
            ran: true,
            covers:
                "hook-layer patterns (touring_code::cwe_scan): CWE-798 vendor-prefix credentials, \
                     CWE-89 SQL via `format!`, CWE-22 `fs::read_to_string`, CWE-394 `.unwrap()` -- \
                     Rust syntax, so a file in another language gets only CWE-798 from this engine"
                    .to_string(),
            reason: None,
        },
        Engine {
            name: "owasp",
            ran: owasp.not_scanned.is_none(),
            covers: format!(
                "F2.1 CWE/OWASP registry (touring_analysis SecurityAnalyzer, lang={}): CWE-78 command \
                 injection, 89 SQLi, 79 XSS, 22 path traversal, 502 deserialization, 918 SSRF, \
                 LDAP/XML injection, buffer/integer overflow",
                owasp.lang
            ),
            reason: owasp.not_scanned.clone(),
        },
    ];

    ScanResult {
        file_path: path.display().to_string(),
        lang: owasp.lang,
        cwes,
        p0_block,
        engines,
        not_scanned: NOT_SCANNED.to_vec(),
    }
}

/// Read `file_path` and scan it.
fn scan_file(file_path: &str) -> anyhow::Result<ScanResult> {
    let source = fs::read_to_string(Path::new(file_path))
        .map_err(|e| anyhow::anyhow!("read {} failed: {}", file_path, e))?;
    Ok(scan_source(&source, Path::new(file_path)))
}

/// CLI entry point — `touring scan [vulnerabilities] <file>`.
///
/// Emits JSON `{ file_path, lang, cwes: [{ id, severity, line, description }], p0_block,
/// engines, not_scanned }` to stdout; human-readable status to stderr. Used by
/// complementação-hooks H6 handler.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (_flags, filtered) = parse_global_flags(args);

    // args[0] = binary path, args[1] = subcmd name "scan", args[2..] = positional.
    // The subcmd-name slot is "scan" itself; skip it to reach the file path.
    // We accept the file path as the first positional AFTER the subcmd token.
    let subcmd_skip = filtered
        .iter()
        .position(|a| a == "scan")
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut positional: Vec<&str> = filtered[subcmd_skip..].iter().map(String::as_str).collect();
    // The documented form is `touring scan vulnerabilities <file>`; that token is a
    // subcommand name, not the file (it used to be read as the path and failed with
    // "read vulnerabilities failed"). Skipped only when a path follows it.
    if positional.len() > 1 && positional[0] == "vulnerabilities" {
        positional.remove(0);
    }

    let file_path = positional
        .first()
        .context("file path required: touring scan [vulnerabilities] <file>")?
        .to_string();

    let result = scan_file(&file_path)?;

    json_to_stdout(&serde_json::to_string(&result)?);
    let not_run: Vec<String> = result
        .engines
        .iter()
        .filter(|e| !e.ran)
        .map(|e| {
            format!(
                "{} NOT run: {}",
                e.name,
                e.reason.as_deref().unwrap_or("unknown")
            )
        })
        .collect();
    human_to_stderr(&format!(
        "scan vulnerabilities for {}: {} CWE(s), p0_block={}{}",
        file_path,
        result.cwes.len(),
        result.p0_block,
        if not_run.is_empty() {
            String::new()
        } else {
            format!(" ({})", not_run.join("; "))
        }
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_file_arg() {
        // Use any existing file under target/ or repo root for testing.
        let args = vec!["/home/gabrielgadea/projects/touring/Cargo.toml".to_string()];
        let r = run(&args);
        assert!(r.is_ok());
    }

    #[test]
    fn missing_file_returns_error() {
        let args = vec![];
        let r = run(&args);
        assert!(r.is_err());
    }

    #[test]
    fn detect_cwes_inline_finds_hardcoded_key() {
        // Build fixture at runtime so the detector pattern doesn't trigger on the
        // test source itself.
        let needle = touring_code::cwe_scan::vendor_prefix_openai_like();
        let src = format!("const KEY: &str = \"{}PLACEHOLDER api\";\n", needle);
        let findings = detect_cwes_in_source(&src);
        assert!(
            findings
                .iter()
                .any(|c| c.id == "CWE-798" && c.severity == "P0")
        );
    }

    #[test]
    fn detect_cwes_inline_finds_sql_injection() {
        let src = "let q = format!(\"SELECT * FROM users WHERE id = {}\", id);\n";
        let findings = detect_cwes_in_source(src);
        assert!(findings.iter().any(|c| c.id == "CWE-89"));
    }

    #[test]
    fn detect_cwes_inline_finds_unwrap() {
        let unwrap_call = touring_code::cwe_scan::unwrap_call_pattern();
        let src = format!("fn main() {{\n    let x = foo(){unwrap_call};\n}}\n");
        let findings = detect_cwes_in_source(&src);
        assert!(findings.iter().any(|c| c.id == "CWE-394"));
    }

    #[test]
    fn detect_cwes_inline_clean_source_has_no_findings() {
        let src = "fn main() {\n    println!(\"hello\");\n}\n";
        let findings = detect_cwes_in_source(src);
        assert!(findings.is_empty());
    }

    /// S8 (2026-09-02): the CLI copy of the detector had drifted from the hook
    /// one — it never emitted CWE-22 (path traversal). One detector now serves
    /// both (`touring_code::cwe_scan::detect_cwes`); this is the drift guard.
    #[test]
    fn detect_cwes_inline_finds_path_traversal_like_the_hook_detector() {
        let src =
            "fn load(p: &str) -> String {\n    fs::read_to_string(p).unwrap_or_default()\n}\n";
        let ids: Vec<String> = detect_cwes_in_source(src)
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert!(ids.contains(&"CWE-22".to_string()), "{ids:?}");
    }

    // ---- 17/10/2026: scan and the F2.1 gate must agree -------------------------------

    use touring_quality::DimStatus;
    use touring_quality::verifications::Verification;
    use touring_quality::verifications::f2_1_owasp::F2_1_Owasp;

    /// The probe of the report: command injection through `shell=True`.
    const CMDI_PY: &str =
        "import subprocess, sys\nuser = sys.argv[1]\nsubprocess.run(\"ls \" + user, shell=True)\n";

    fn write(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, body).expect("write");
        path
    }

    fn gate_blocks(path: &Path) -> bool {
        F2_1_Owasp.check(path).expect("gate").status == DimStatus::Fail
    }

    #[test]
    fn command_injection_in_python_is_found_like_the_gate_finds_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(dir.path(), "probe_cmdi.py", CMDI_PY);
        assert!(
            gate_blocks(&path),
            "premise: the F2.1 gate blocks the probe"
        );
        let r = scan_file(path.to_str().expect("utf8")).expect("scan");
        assert!(
            r.cwes
                .iter()
                .any(|c| c.id == "CWE-78" && c.severity == "P0" && c.line == 3),
            "{:?}",
            r.cwes
                .iter()
                .map(|c| (&c.id, c.severity, c.line))
                .collect::<Vec<_>>()
        );
        assert!(r.p0_block, "the gate blocks, scan must too");
        assert_eq!(r.lang, "python");
    }

    #[test]
    fn scan_declares_what_it_ran_and_what_it_never_looks_at() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(dir.path(), "probe_cmdi.py", CMDI_PY);
        let r = scan_file(path.to_str().expect("utf8")).expect("scan");
        let owasp = r
            .engines
            .iter()
            .find(|e| e.name == "owasp")
            .expect("owasp engine");
        assert!(owasp.ran && owasp.reason.is_none());
        assert!(owasp.covers.contains("python"), "{}", owasp.covers);
        assert!(r.engines.iter().any(|e| e.name == "cwe_scan" && e.ran));
        assert!(
            r.not_scanned
                .iter()
                .any(|d| d.contains("F2.5 dependency CVEs")),
            "{:?}",
            r.not_scanned
        );
    }

    #[test]
    fn a_zero_from_an_engine_that_did_not_run_says_so() {
        // `tests/` is a non-production harness: the gate allowlists it, and scan
        // must not answer a bare "0 CWE(s)" as if it had looked.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(dir.path(), "tests/probe_cmdi.py", CMDI_PY);
        let r = scan_file(path.to_str().expect("utf8")).expect("scan");
        let owasp = r
            .engines
            .iter()
            .find(|e| e.name == "owasp")
            .expect("owasp engine");
        assert!(!owasp.ran, "{:?}", owasp.reason);
        assert!(
            owasp
                .reason
                .as_deref()
                .is_some_and(|why| why.contains("harness")),
            "{:?}",
            owasp.reason
        );
        assert!(!r.p0_block, "allowlisted harness: the gate passes it too");
        assert!(!gate_blocks(&path));
    }

    #[test]
    fn scan_and_the_gate_agree_on_a_small_corpus() {
        let dir = tempfile::tempdir().expect("tempdir");
        let corpus: [(&str, &str); 6] = [
            ("cmdi.py", CMDI_PY),
            ("clean.py", "def add(a, b):\n    return a + b\n"),
            (
                "xss.html",
                "<html><script>alert(document.cookie)</script></html>\n",
            ),
            (
                "sqli.py",
                "q = \"SELECT * FROM t WHERE name = 'x' OR '1'='1'\"\ncur.execute(q)\n",
            ),
            (
                "path.rs",
                "fn load(p: &str) -> String {\n    std::fs::read_to_string(p).unwrap_or_default()\n}\n",
            ),
            ("plain.rs", "fn main() {\n    println!(\"hello\");\n}\n"),
        ];
        let mut blocking = 0;
        for (name, body) in corpus {
            let path = write(dir.path(), name, body);
            let r = scan_file(path.to_str().expect("utf8")).expect("scan");
            let gate = gate_blocks(&path);
            let hook_layer_p0 = detect_cwes_in_source(body)
                .iter()
                .any(|c| c.severity == "P0");
            // The gate's verdict is contained in scan's, and equals it unless the
            // hook-layer patterns (credentials, ...) raise a P0 the gate never claimed.
            assert!(
                !gate || r.p0_block,
                "{name}: the gate blocks but scan does not"
            );
            if !hook_layer_p0 {
                assert_eq!(r.p0_block, gate, "{name}: scan and the gate disagree");
            }
            // Every CWE the registry puts in the gate's evidence is in scan's list.
            let registry = touring_quality::verifications::f2_1_owasp::owasp_scan(body, &path);
            for m in &registry.matches {
                assert!(
                    r.cwes.iter().any(|c| c.id == format!("CWE-{}", m.cwe_id)),
                    "{name}: scan lost CWE-{}",
                    m.cwe_id
                );
            }
            blocking += usize::from(gate);
        }
        assert!(
            blocking >= 1,
            "the corpus must contain at least one blocking sample"
        );
    }

    #[test]
    fn the_documented_subcommand_form_reads_the_file_not_the_word() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(dir.path(), "clean.py", "x = 1\n");
        let file = path.to_str().expect("utf8").to_string();
        let args = vec![
            "scan".to_string(),
            "vulnerabilities".to_string(),
            file.clone(),
        ];
        assert!(
            run(&args).is_ok(),
            "`scan vulnerabilities <file>` must scan <file>"
        );
        // The bare form keeps working.
        assert!(run(&["scan".to_string(), file]).is_ok());
    }

    #[test]
    fn a_hook_layer_finding_on_the_same_line_is_not_reported_twice() {
        // Both engines know SQL injection (CWE-89) here: the hook-layer pattern
        // (`format!` + `select`, tier P1) and the registry (`UNION SELECT`, tier P0).
        let dir = tempfile::tempdir().expect("tempdir");
        let body = "let q = format!(\"{} UNION SELECT password FROM users\", id);\n";
        let path = write(dir.path(), "q.rs", body);
        let hook = detect_cwes_in_source(body);
        assert!(
            hook.iter()
                .any(|c| c.id == "CWE-89" && c.line == 1 && c.severity == "P1"),
            "premise: the hook-layer detector reports CWE-89/P1 on line 1"
        );
        let registry = touring_quality::verifications::f2_1_owasp::owasp_scan(body, &path);
        assert!(
            registry
                .matches
                .iter()
                .any(|m| m.cwe_id == 89 && m.line == 1),
            "premise: the registry reports CWE-89 on line 1 too: {registry:?}"
        );
        let r = scan_file(path.to_str().expect("utf8")).expect("scan");
        let on_line_1: Vec<_> = r
            .cwes
            .iter()
            .filter(|c| c.id == "CWE-89" && c.line == 1)
            .collect();
        assert_eq!(
            on_line_1.len(),
            1,
            "one entry, not two: {:?}",
            r.cwes
                .iter()
                .map(|c| (&c.id, c.severity, c.line))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            on_line_1[0].severity, "P0",
            "the more severe tier wins (the gate blocks here)"
        );
        assert!(r.p0_block && gate_blocks(&path));
    }
}
