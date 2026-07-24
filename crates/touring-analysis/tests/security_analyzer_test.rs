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
