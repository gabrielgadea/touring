//! E2E tests for SecurityAnalyzer — VulnerabilityPattern x antipatterns bridge.
//!
//! Tests the integration between touring-offensive VulnerabilityPattern (10 CWE patterns)
//! and touring-analysis antipattern detection (SIMD memchr scanning).

use std::sync::Arc;
use touring_analysis::quality::{SecurityAnalyzer, antipatterns};
use touring_offensive::vuln::{VulnMatch, VulnerabilityPattern};

#[test]
fn test_security_analyzer_rust_antipatterns() {
    let source = r#"
fn example() {
    let x = something.unwrap();
    todo!();
    panic!("oops");
}
"#;

    let analyzer = SecurityAnalyzer::new();
    let report = analyzer.analyze(source, "rust");

    // Should detect .unwrap() antipattern
    assert!(
        !report.antipattern_hits.is_empty(),
        "Expected antipattern hits for .unwrap() and panic!"
    );

    // Should detect panic!() antipattern
    let has_panic = report
        .antipattern_hits
        .iter()
        .any(|(msg, _)| msg.contains("panic!"));
    assert!(has_panic, "Expected panic!() antipattern detection");
}

#[test]
fn test_security_analyzer_rust_vulnerability_patterns() {
    // SQL injection vulnerable code with actual SQLi payload matching SqlInjectionPattern regex
    // SqlInjectionPattern detects: ('\s*OR\s*'|;\s*--|UNION\s+SELECT)
    let source = r#"
fn query_user(input: &str) -> String {
    format!("SELECT * FROM users WHERE name = '{}' OR '1'='1'", input)
}
"#;

    let analyzer = SecurityAnalyzer::new();
    let report = analyzer.analyze(source, "rust");

    // SqlInjectionPattern should detect the vulnerability
    let has_sqli = report
        .vuln_matches
        .iter()
        .any(|v| v.pattern_name.contains("SQL") || v.cwe_id == 89);
    assert!(
        has_sqli,
        "Expected SQL injection vulnerability detection, got: {:?}",
        report.vuln_matches
    );
}

#[test]
fn test_security_analyzer_python_antipatterns() {
    let source = r#"
def foo():
    try:
        something()
    except:
        pass
"#;

    let analyzer = SecurityAnalyzer::new();
    let report = analyzer.analyze(source, "python");

    // Should detect bare except: antipattern
    let has_bare_except = report
        .antipattern_hits
        .iter()
        .any(|(msg, _)| msg.contains("bare"));
    assert!(
        has_bare_except,
        "Expected bare except: antipattern detection, got: {:?}",
        report.antipattern_hits
    );
}

#[test]
fn test_security_analyzer_empty_source() {
    let source = "";
    let analyzer = SecurityAnalyzer::new();
    let report = analyzer.analyze(source, "rust");

    assert!(report.antipattern_hits.is_empty());
    assert!(report.vuln_matches.is_empty());
    assert_eq!(report.combined_score, 1.0); // Perfect score for empty
}

#[test]
fn test_security_analyzer_combined_score() {
    // Clean code should have high combined score
    let source = "fn clean() { println!(\"hello\"); }";
    let analyzer = SecurityAnalyzer::new();
    let report = analyzer.analyze(source, "rust");

    assert_eq!(report.lang, "rust");
    assert!(
        report.combined_score >= 0.0 && report.combined_score <= 1.0,
        "combined_score should be in [0, 1]"
    );
}

#[test]
fn test_security_analyzer_with_custom_patterns() {
    #[derive(Debug)]
    struct DummyPattern;
    impl VulnerabilityPattern for DummyPattern {
        fn detect(&self, input: &str) -> Option<VulnMatch> {
            if input.contains("DUMMY_VULN") {
                Some(VulnMatch::new("Dummy".into(), (0, 10), 5.0, 999))
            } else {
                None
            }
        }
        fn name(&self) -> &str {
            "Dummy"
        }
        fn severity(&self) -> f32 {
            5.0
        }
        fn cwe_id(&self) -> u32 {
            999
        }
    }

    let analyzer = SecurityAnalyzer::with_patterns(vec![Arc::new(DummyPattern)]);
    let report = analyzer.analyze("DUMMY_VULN in code", "rust");

    assert!(
        !report.vuln_matches.is_empty(),
        "Expected vulnerability match for DUMMY_VULN"
    );
    let first = report.vuln_matches.first().expect("one vuln match");
    assert_eq!(first.cwe_id, 999);
}

#[test]
fn test_security_analyzer_score_with_vulnerabilities() {
    // Code with both antipatterns and vulnerabilities should score low
    let source = r#"
fn example() {
    let x = something.unwrap();
    todo!();
    let cmd = format!("ls {}", user_input);
}
"#;

    let analyzer = SecurityAnalyzer::new();
    let report = analyzer.analyze(source, "rust");

    // Antipatterns detected
    assert!(
        !report.antipattern_hits.is_empty(),
        "Should detect rust antipatterns"
    );

    // Score should be low (penalized for antipatterns and vulnerabilities)
    assert!(
        report.combined_score < 1.0,
        "Score should be < 1.0 when issues are present"
    );
}

#[test]
fn test_security_analyzer_language_correct() {
    let analyzer = SecurityAnalyzer::new();

    let report_rust = analyzer.analyze("fn test() {}", "rust");
    let report_python = analyzer.analyze("def test(): pass", "python");

    assert_eq!(report_rust.lang, "rust");
    assert_eq!(report_python.lang, "python");
}

#[test]
fn test_security_analyzer_default_constructor() {
    // Verify default constructor works and uses all registered patterns
    let analyzer = SecurityAnalyzer::default();
    let report = analyzer.analyze("", "rust");

    // Empty source should have perfect antipattern score but vuln score depends on registry
    assert_eq!(report.lang, "rust");
    assert!(report.antipattern_hits.is_empty());
}

#[test]
fn test_security_analyzer_pattern_registry_integration() {
    // Verify SecurityAnalyzer::new() (which uses PatternRegistry::all()) detects issues
    let analyzer = SecurityAnalyzer::new();

    // SQL injection test with actual SQLi payload
    // SqlInjectionPattern detects: ('\s*OR\s*'|;\s*--|UNION\s+SELECT)
    let sqli = r#"format!("SELECT * FROM users WHERE id = '{}' OR '1'='1'", user_id)"#;
    let report = analyzer.analyze(sqli, "rust");

    let has_sql = report
        .vuln_matches
        .iter()
        .any(|v| v.pattern_name.to_lowercase().contains("sql"));
    assert!(
        has_sql,
        "PatternRegistry should include SQL injection detector"
    );
}

#[test]
fn test_security_analyzer_vuln_matches_structure() {
    #[derive(Debug)]
    struct MinimalPattern;
    impl VulnerabilityPattern for MinimalPattern {
        fn detect(&self, input: &str) -> Option<VulnMatch> {
            input
                .find("INJECTION_POINT")
                .map(|pos| VulnMatch::new("TestPattern".into(), (pos, pos + 14), 7.5, 123))
        }
        fn name(&self) -> &str {
            "TestPattern"
        }
        fn severity(&self) -> f32 {
            7.5
        }
        fn cwe_id(&self) -> u32 {
            123
        }
    }

    let analyzer = SecurityAnalyzer::with_patterns(vec![Arc::new(MinimalPattern)]);
    let report = analyzer.analyze("some INJECTION_POINT here", "rust");

    assert_eq!(report.vuln_matches.len(), 1);
    let vuln = report.vuln_matches.first().expect("one match");
    assert_eq!(vuln.pattern_name, "TestPattern");
    assert_eq!(vuln.severity, 7.5);
    assert_eq!(vuln.cwe_id, 123);
}

#[test]
fn test_security_analyzer_score_bounds() {
    let analyzer = SecurityAnalyzer::new();

    // Empty = perfect score
    let empty_report = analyzer.analyze("", "rust");
    assert_eq!(empty_report.combined_score, 1.0);

    // Known bad rust = score < 1.0
    let bad_report = analyzer.analyze("something.unwrap()", "rust");
    assert!(
        bad_report.combined_score < 1.0,
        "Code with unwrap() should score < 1.0"
    );
}

#[test]
fn test_security_analyzer_antipattern_hit_format() {
    // Verify antipattern hit format matches (message, line) tuple structure
    let source = "fn line_one() {}\nfn line_two() { todo!(); }";
    let hits = antipatterns::detect_antipatterns(source, "rust");

    let todo_hit = hits.iter().find(|(msg, _line)| msg.contains("todo!"));
    assert!(todo_hit.is_some(), "Should find todo! hit");
    assert_eq!(todo_hit.unwrap().1, 2, "todo!() is on line 2");
}

/// Cross-audit R2 (14/09/2026): a payload quoted in a comment no longer hides a
/// real sink of the same class further down — the precision pass drops the
/// comment match and keeps the sink.
#[test]
fn a_commented_payload_does_not_hide_the_real_sink_below_it() {
    let source = "// the old template said <script>\nfn page(u: &str) -> String { format!(\"<script>{u}</script>\") }\n";
    let report = SecurityAnalyzer::new().analyze(source, "rust");
    let xss: Vec<&VulnMatch> = report
        .vuln_matches
        .iter()
        .filter(|m| m.pattern_name == "XSS")
        .collect();
    assert_eq!(
        xss.len(),
        1,
        "one finding per pattern: {:?}",
        report.vuln_matches
    );
    assert!(
        xss[0].span.0 > source.find('\n').expect("two lines"),
        "the sink, not the comment"
    );
    let twice = "let a = \"<script>x()</script>\";\nlet b = \"<script>y()</script>\";\n";
    let report = SecurityAnalyzer::new().analyze(twice, "rust");
    assert_eq!(
        report
            .vuln_matches
            .iter()
            .filter(|m| m.pattern_name == "XSS")
            .count(),
        1,
        "two sinks of one pattern are one finding, as the score has always read them"
    );
}

/// In a shell script `&& curl` is the script; the dynamic-exec arm still counts.
#[test]
fn shell_operators_are_not_command_injection_in_a_shell_script() {
    let script = "#!/bin/sh\ncurl -o a \"$u.sig\" && curl -o b \"$u.pem\"; rm -f tmp.$$\n";
    let shell = SecurityAnalyzer::new().analyze(script, "shell");
    assert!(
        shell.vuln_matches.iter().all(|m| m.pattern_name != "CMDi"),
        "{:?}",
        shell.vuln_matches
    );
    let smuggled =
        SecurityAnalyzer::new().analyze("cmd = user + \" && curl http://evil \"\n", "python");
    assert!(
        smuggled
            .vuln_matches
            .iter()
            .any(|m| m.pattern_name == "CMDi")
    );
    let exec = SecurityAnalyzer::new().analyze(
        "python3 -c 'import os; os.system(f\"ping {h}\")'\n",
        "shell",
    );
    assert!(exec.vuln_matches.iter().any(|m| m.pattern_name == "CMDi"));
}

/// A Python docstring documents; a triple-quoted argument is code.
#[test]
fn python_docstrings_do_not_reach_a_sink_but_triple_quoted_arguments_do() {
    let doc = "\"\"\"Tool.\n\nRenders inline <script> blocks.\n\"\"\"\nimport json\n";
    assert!(
        SecurityAnalyzer::new()
            .analyze(doc, "python")
            .vuln_matches
            .is_empty()
    );
    let arg = "cur.execute(\n    \"\"\"SELECT a FROM t UNION SELECT b FROM u\"\"\"\n)\n";
    let report = SecurityAnalyzer::new().analyze(arg, "python");
    assert!(report.vuln_matches.iter().any(|m| m.pattern_name == "SQLi"));
}

fn traversal(source: &str, lang: &str) -> bool {
    SecurityAnalyzer::new()
        .analyze(source, lang)
        .vuln_matches
        .iter()
        .any(|m| m.pattern_name == "PathTraversal")
}

/// Canvas D (15/09/2026): five analise validators failed F2.1 on the idiom that
/// finds a repository root from the script's own directory.
#[test]
fn a_climb_from_the_scripts_own_directory_is_not_traversal_but_one_from_input_is() {
    for anchored in [
        "#!/bin/bash\ncd \"$(dirname \"$0\")/../../../..\" || exit 2\n",
        "#!/bin/bash\nRAIZ=\"$(cd \"$(dirname \"$0\")/../../../..\" && pwd)\"\n",
        "#!/bin/bash\nSCRIPT_DIR=\"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)\"\nPROJECT_ROOT=\"$(cd \"$SCRIPT_DIR/../../..\" && pwd)\"\n",
        "#!/bin/sh\nreadonly HERE=$(dirname \"$0\")\n. \"${HERE}/../../lib.sh\"\n",
    ] {
        assert!(!traversal(anchored, "shell"), "{anchored}");
    }
    for from_input in [
        "#!/bin/bash\ncat \"$1/../../etc/passwd\"\n",
        "#!/bin/bash\ncd \"$(dirname \"$INPUT\")/../../..\"\n",
        "#!/bin/bash\ncd \"$TARGET_DIR/../../..\"\n",
        "#!/bin/bash\nSCRIPT_DIR=\"$1\"\ncd \"$SCRIPT_DIR/../../..\"\n",
    ] {
        assert!(traversal(from_input, "shell"), "{from_input}");
    }
    // The exemption is shell syntax: the same text in Python still counts.
    assert!(traversal(
        "path = f\"$(dirname \\\"$0\\\")/../../..\"\n",
        "python"
    ));
}

/// 27/10/2026: `crates/touring-server/build.rs` failed F2.1 (0.2, P0) on
/// `println!("cargo:rerun-if-changed=../../.git/HEAD");` — a directive CARGO reads,
/// not a path the program opens. The detector, not the build script, was wrong.
#[test]
fn test_cargo_rerun_directive_is_not_path_traversal() {
    for directive in [
        // The two lines of `crates/touring-server/build.rs`, verbatim.
        "fn main() {\n    println!(\"cargo:rerun-if-changed=../../.git/HEAD\");\n}\n",
        "fn main() {\n    println!(\"cargo:rerun-if-changed=../../.git/refs/heads/main\");\n}\n",
        // The `cargo::` form and another directive key.
        "fn main() {\n    println!(\"cargo::rerun-if-changed=../../.git/HEAD\");\n}\n",
        "fn main() {\n    println!(\"cargo:rustc-link-search=native=../../vendor/lib\");\n}\n",
        // `print!`, and a call that rustfmt split so the literal starts on its own line.
        "fn main() {\n    print!(\"cargo:rerun-if-changed=../../a\\n\");\n}\n",
        "fn main() {\n    println!(\n        \"cargo:rerun-if-changed=../../some/long/path\"\n    );\n}\n",
        // An interpolated tail: the climb itself is still a constant.
        "fn main() {\n    println!(\"cargo:rerun-if-changed=../../{}\", name);\n}\n",
    ] {
        assert!(!traversal(directive, "rust"), "{directive}");
    }
}

/// The positive control for the cargo-directive exemption: it is the directive
/// form and nothing wider, and dropping it must never hide a real sink.
#[test]
fn test_cargo_directive_exemption_is_narrow_and_real_traversal_still_counts() {
    for sink in [
        // A request-driven read: the canonical CWE-22 shape.
        "fn read(user_input: &str) -> String {\n    std::fs::read_to_string(format!(\"../../{}\", user_input)).unwrap_or_default()\n}\n",
        // A printed path is not a cargo directive.
        "fn main() {\n    println!(\"see ../../etc/passwd\");\n}\n",
        // cargo reads stdout only; stderr is not a directive channel.
        "fn main() {\n    eprintln!(\"cargo:rerun-if-changed=../../.git/HEAD\");\n}\n",
        // A `cargo:`-looking string that is opened as a path is not printed to cargo.
        "fn main() {\n    let p = \"cargo:rerun-if-changed=../../.git/HEAD\";\n    let _ = std::fs::read(p);\n}\n",
        // No `<key>=`: not the directive grammar.
        "fn main() {\n    println!(\"cargo:../../x\");\n}\n",
        // Directive lines first, a real sink further down: the sink survives.
        "fn main() {\n    println!(\"cargo:rerun-if-changed=../../.git/HEAD\");\n    let _ = std::fs::read(\"../../etc/passwd\");\n}\n",
    ] {
        assert!(traversal(sink, "rust"), "{sink}");
    }
}

/// 25/09/2026 (touring-36, the analise-50 judge): an HTML event handler whose
/// VALUE is a constant is inline JS, not CWE-79 — no untrusted data reaches
/// it (`onclick="rodar('preset')"` in a Python-generated template failed P0
/// on a file of 05/09 nobody touched). The code judge flags the sink only
/// when the value carries interpolation or a variable. The shared
/// `XssPattern` is untouched on purpose: the concolic/payload path measures
/// INPUT, where the handler IS the attack (its own test suite proves it).
#[test]
fn a_constant_handler_is_inline_js_but_a_tainted_one_is_a_sink() {
    // Single-line string assignment — NOT a docstring, so the docstring
    // region pass cannot be what spares it: the taint filter is measured.
    let constant = "html = \"<button onclick=\\\"rodar('preset')\\\">layout preset</button>\"\n";
    let report = SecurityAnalyzer::new().analyze(constant, "python");
    assert!(
        !report.vuln_matches.iter().any(|m| m.pattern_name == "XSS"),
        "a constant handler is inline JS, not a sink: {:?}",
        report.vuln_matches
    );
    // The controls that must keep failing (touring-36's list): f-string with
    // a variable, .format(user_input), and a `+ var +` concatenation.
    for tainted in [
        "html = f\"<button onclick=\\\"rodar('{nome}')\\\">x</button>\"\n",
        "html = \"<button onclick=\\\"rodar('{}')\\\">x</button>\".format(nome)\n",
        "html = \"<button onclick=\\\"rodar('\" + nome + \"')\\\">x</button>\"\n",
    ] {
        let report = SecurityAnalyzer::new().analyze(tainted, "python");
        assert!(
            report.vuln_matches.iter().any(|m| m.pattern_name == "XSS"),
            "a tainted handler is a sink: {tainted}"
        );
    }
}

fn xss_findings(source: &str, lang: &str) -> usize {
    SecurityAnalyzer::new()
        .analyze(source, lang)
        .vuln_matches
        .iter()
        .filter(|m| m.pattern_name == "XSS")
        .count()
}

/// F2.1 false positive (09/10/2026, touring-4f): a Python `Write` whose only
/// "XSS" was the words `javascript:` in a log message was blocked as CWE-79.
/// `javascript:` is a URL scheme: it is a sink where a URL value begins (an
/// attribute, a quoted URL, a call argument), never as a word in prose. The
/// controls — the same token as a URL — must keep failing, in every language
/// the Write gate scores.
#[test]
fn the_word_javascript_in_a_log_message_is_not_xss_but_a_javascript_url_is() {
    for (source, lang) in [
        (
            "def f(log):\n    log(\"rota javascript: handlers desativados\")\n",
            "python",
        ),
        ("print(f\"bloqueado: javascript: em {url}\")\n", "python"),
        (
            "console.log(\"rota javascript: handlers desativados\");\n",
            "javascript",
        ),
    ] {
        assert_eq!(
            xss_findings(source, lang),
            0,
            "prose is not a sink: {source}"
        );
    }
    for (source, lang) in [
        ("html = '<a href=\"javascript:alert(1)\">x</a>'\n", "python"),
        ("html = f'<a href=\"javascript:{x}\">go</a>'\n", "python"),
        ("redirect(\"javascript:alert(1)\")\n", "python"),
        ("var x = 1;\nlocation = 'javascript:' + x;\n", "javascript"),
        ("window.open(\"javascript:alert(1)\");\n", "javascript"),
        ("<a href=\"javascript:alert(1)\">x</a>\n", "html"),
    ] {
        assert_eq!(
            xss_findings(source, lang),
            1,
            "a javascript: URL is a sink: {source}"
        );
    }
}

/// The scheme is case-insensitive in a browser: `JavaScript:` is the same sink.
#[test]
fn a_javascript_url_is_a_sink_in_any_letter_case() {
    for (source, lang) in [
        ("html = '<a href=\"JavaScript:alert(1)\">x</a>'\n", "python"),
        ("redirect(\" \tJAVASCRIPT:alert(1)\")\n", "python"),
        ("location = 'jAvAsCrIpT:' + x;\n", "javascript"),
        ("window.open(\"Javascript:alert(1)\");\n", "javascript"),
        ("<a href=\"JaVaScRiPt:alert(1)\">x</a>\n", "html"),
    ] {
        assert_eq!(
            xss_findings(source, lang),
            1,
            "cased URL is a sink: {source}"
        );
    }
    for (source, lang) in [
        ("log(\"rota JavaScript: handlers desativados\")\n", "python"),
        ("doc = 'TypeScript / JavaScript: exports'\n", "python"),
    ] {
        assert_eq!(
            xss_findings(source, lang),
            0,
            "cased prose is not: {source}"
        );
    }
}

/// Defensive code names the scheme to refuse it; assigning it as a URL is the sink.
#[test]
fn a_scheme_blocklist_is_not_a_sink_but_a_url_assignment_is() {
    for (source, lang) in [
        (
            "def bad(url):\n    return url.lower().startswith((\"javascript:\", \"data:\"))\n",
            "python",
        ),
        (
            "def bad(s):\n    return s != \"javascript:\" and s not in (\"data:\", \"javascript:\")\n",
            "python",
        ),
        (
            "if (u.startsWith('javascript:')) { return false; }\n",
            "javascript",
        ),
        (
            "if (scheme === 'javascript:') { return false; }\n",
            "javascript",
        ),
        (
            "for (let i=0;i<n;i++) { if (u.indexOf('javascript:') === 0) bad(); }\n",
            "javascript",
        ),
    ] {
        assert_eq!(
            xss_findings(source, lang),
            0,
            "a blocklist is not a sink: {source}"
        );
    }
    for (source, lang) in [
        ("if ok:\n    el.href = \"javascript:\" + x\n", "python"),
        ("x == 1 and go(\"javascript:alert(1)\")\n", "python"),
        (
            "redirect(url if url.startswith(\"/\") else \"javascript:void(0)\")\n",
            "python",
        ),
        ("if (ok) el.href = \"javascript:\" + x;\n", "javascript"),
    ] {
        assert_eq!(
            xss_findings(source, lang),
            1,
            "a URL assignment is a sink: {source}"
        );
    }
}
