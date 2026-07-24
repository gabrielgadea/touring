//! End-to-end integration test for the Sentrux rules engine.
//!
//! Wave 2 P3 (2026-05-09). Wires the full pipeline:
//!     build_workspace_from_path → compute_quality_signal
//!     parse_str (TOML) → evaluate → MetricViolation list
//!
//! Verifies the engine catches real budget breaches against a synthetic
//! on-disk source tree (no daemon required).

use std::fs;
use std::io::Write;
use std::path::Path;

use touring_analysis::quality::{build_workspace_from_path, compute_quality_signal};
use touring_analysis::rules::{Severity, count_by_severity, evaluate, parse_str};

const RULES_TOML: &str = r#"
version = "1.0"

[[rule]]
name = "no-cycles"
metric = "cycle_count"
op = "eq"
threshold = 0
severity = "deny"
message = "no dependency cycles allowed"

[[rule]]
name = "min-quality-signal"
metric = "signal_0_10000"
op = "ge"
threshold = 8000
severity = "warn"

[[rule]]
name = "no-god-files"
applies_to = "**/*.rs"
metric = "file_lines"
op = "lt"
threshold = 100
severity = "warn"

[[rule]]
name = "limit-cc"
applies_to = "**/*.rs"
metric = "function_cc"
op = "lt"
threshold = 10
severity = "deny"
"#;

fn write_file(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir parent");
    }
    let mut f = fs::File::create(&path).expect("create file");
    f.write_all(body.as_bytes()).expect("write body");
}

#[test]
fn end_to_end_pipeline_detects_breaches() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // a.rs has many lines (>100) and a CC-heavy function (>10).
    let mut bloat = String::from("fn complex_function(x: u32) -> u32 {\n    let mut acc = 0;\n");
    for _ in 0..30 {
        bloat.push_str("    if x > 0 { acc += 1 } else { acc += 2 };\n");
    }
    bloat.push_str("    acc\n}\n\n");
    for i in 0..120 {
        bloat.push_str(&format!("// padding line {i}\n"));
    }
    write_file(tmp.path(), "a.rs", &bloat);

    // Cycle: b -> c -> b
    write_file(
        tmp.path(),
        "b.rs",
        "use crate::c;\npub fn from_b() { c::from_c(); }\n",
    );
    write_file(
        tmp.path(),
        "c.rs",
        "use crate::b;\npub fn from_c() { b::from_b(); }\n",
    );

    let ws = build_workspace_from_path(tmp.path()).expect("walk");
    let signal = compute_quality_signal(&ws);
    let rules = parse_str(RULES_TOML).expect("parse rules");
    let violations = evaluate(&rules, &ws, &signal).expect("evaluate");

    let (deny, warn, info) = count_by_severity(&violations);
    println!(
        "[rules_engine_e2e] signal={}/10000 violations={} (deny={deny} warn={warn} info={info})",
        signal.signal_0_10000,
        violations.len()
    );
    for v in &violations {
        println!(
            "  [{:?}] {} :: {:?} loc={:?} actual={:.2} {} threshold={:.2}",
            v.severity,
            v.rule_name,
            v.metric,
            v.location,
            v.actual,
            v.op.symbol(),
            v.threshold
        );
    }

    // The synthetic workspace has cycles, so no-cycles must fire.
    assert!(
        violations.iter().any(|v| v.rule_name == "no-cycles"),
        "no-cycles rule should fire on cyclic synthetic workspace"
    );
    // a.rs is large → no-god-files must fire on it.
    assert!(
        violations
            .iter()
            .any(|v| v.rule_name == "no-god-files" && v.location.as_deref() == Some("a.rs")),
        "no-god-files should flag a.rs"
    );
    // complex_function in a.rs has CC well above 10 → limit-cc must fire.
    assert!(
        violations.iter().any(|v| v.rule_name == "limit-cc"
            && v.location
                .as_deref()
                .map(|l| l.contains("complex_function"))
                .unwrap_or(false)),
        "limit-cc should flag complex_function"
    );
    // At least one DENY-severity violation present.
    assert!(deny >= 1, "expected at least one deny-severity violation");
}

#[test]
fn empty_workspace_produces_zero_violations_for_workspace_metrics() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let ws = build_workspace_from_path(tmp.path()).expect("walk");
    let signal = compute_quality_signal(&ws);
    let rules = parse_str(
        r#"
version = "1.0"

[[rule]]
name = "no-cycles"
metric = "cycle_count"
op = "eq"
threshold = 0
severity = "deny"
"#,
    )
    .expect("parse rules");
    let violations = evaluate(&rules, &ws, &signal).expect("evaluate");
    assert!(violations.iter().all(|v| v.rule_name != "no-cycles"));
}

#[test]
fn deny_severity_separates_from_warn() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_file(tmp.path(), "a.rs", "use crate::b;\npub fn x() {}\n");
    write_file(tmp.path(), "b.rs", "use crate::a;\npub fn y() {}\n");
    let ws = build_workspace_from_path(tmp.path()).expect("walk");
    let signal = compute_quality_signal(&ws);
    let rules = parse_str(
        r#"
version = "1.0"

[[rule]]
name = "no-cycles-deny"
metric = "cycle_count"
op = "eq"
threshold = 0
severity = "deny"

[[rule]]
name = "min-signal-warn"
metric = "signal_0_10000"
op = "ge"
threshold = 12000
severity = "warn"
"#,
    )
    .expect("parse rules");
    let violations = evaluate(&rules, &ws, &signal).expect("evaluate");
    let (deny, warn, _info) = count_by_severity(&violations);
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.severity, Severity::Deny))
    );
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.severity, Severity::Warn))
    );
    assert!(
        deny >= 1 && warn >= 1,
        "expected at least one deny + one warn"
    );
}
