//! The cross-emitter invariant (`crates/touring-cli/.claude/CLAUDE.md` §Cross-emitter): the Python
//! emitter `scripts/gen_sdk.py` and `touring_code::sdk::signal_report_from_journal` produce the SAME
//! `SignalReport` for the same journal. Until 12/11/2026 the promise "schema drift fails the smoke
//! test" was false — `examples/sdk_smoke.rs` only writes a file, nothing compared — and the two had
//! drifted on the real journal (57.405 runs read by Rust, 57.539 by Python): the journal grew a
//! second family of line (`{"repeat_blocked":{…}}`) and non-object lines, which the strict Rust
//! reader does not count as runs and the Python emitter did. Rust is the reference.
//!
//! The comparison is over the parsed JSON, not the bytes (serde keeps struct order, Python sorts
//! keys), minus `generated_at_unix`, the only field that is a clock.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use touring_code::sdk::{SdkError, signal_report_from_journal};

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gen_sdk.py")
}

/// Run the Python emitter on `journal`. A missing interpreter is a FAILURE, never a silent skip:
/// a drift guard that does not run guards nothing.
fn python_emitter(journal: &Path, extra_args: &[&str]) -> Output {
    Command::new("python3")
        .arg(script())
        .arg("--journal")
        .arg(journal)
        .args(extra_args)
        .output()
        .expect("python3 must be on PATH: the cross-emitter test runs scripts/gen_sdk.py")
}

fn without_the_clock(mut report: Value) -> Value {
    report
        .as_object_mut()
        .expect("a report is an object")
        .remove("generated_at_unix");
    report
}

fn rust_report(journal: &Path) -> Value {
    without_the_clock(
        serde_json::to_value(signal_report_from_journal(journal).expect("Rust reads the journal"))
            .expect("serializes"),
    )
}

fn python_report(journal: &Path) -> Value {
    let out = python_emitter(journal, &[]);
    assert!(
        out.status.success(),
        "gen_sdk.py failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    without_the_clock(serde_json::from_slice(&out.stdout).expect("gen_sdk.py prints JSON"))
}

fn journal_with(dir: &Path, lines: &[&str]) -> PathBuf {
    let path = dir.join("run_journal.jsonl");
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, body).expect("write the journal");
    path
}

/// What the journal holds today: run lines of several vintages, the second family of line
/// (`repeat_blocked`), lines that are valid JSON but not objects, a torn line, and objects that
/// are JSON but not a run (a required field missing or of the wrong type).
const MIXED: &[&str] = &[
    // v1 line (no B2/N6 fields)
    r#"{"ts":1,"language":"python","exit_code":0,"failure_kind":null,"duration_ms":5,"code_hash_stdout_bytes":10,"bytes_elided":0}"#,
    // v2 line with every later field
    r#"{"ts":2,"language":"bash","exit_code":1,"failure_kind":"exception","duration_ms":9,"code_hash_stdout_bytes":0,"bytes_elided":3,"source":"inline","file":null,"harvest":null,"brief":false,"orchestrate":true,"tmp_bytes":0,"hint":{"key":"k","sim":0.5}}"#,
    // a failure label this build does not know, and a timeout
    r#"{"ts":3,"language":"python","exit_code":124,"failure_kind":"timeout","duration_ms":100}"#,
    r#"{"ts":4,"language":"js","exit_code":0,"failure_kind":"from-the-future","duration_ms":1}"#,
    "",
    // the second family of line
    r#"{"repeat_blocked":{"hash":"abc","count":3,"ts":5}}"#,
    r#"{"repeat_blocked":{"hash":"def","count":2,"ts":6}}"#,
    // valid JSON, not an object
    "123",
    "[1,2,3]",
    r#""text""#,
    "null",
    "true",
    // not JSON at all
    "not json at all",
    // objects that are not runs: a required field missing, or the wrong type
    r#"{"language":"bash","exit_code":0,"duration_ms":1}"#,
    r#"{"ts":"7","language":"bash","exit_code":0,"duration_ms":1}"#,
    r#"{"ts":8,"language":"bash","exit_code":0,"duration_ms":1.5}"#,
    r#"{"ts":9,"language":"bash","exit_code":0,"duration_ms":-1}"#,
    r#"{"ts":10,"language":null,"exit_code":0,"duration_ms":1}"#,
    r#"{"ts":11,"language":"bash","exit_code":true,"duration_ms":1}"#,
    r#"{"ts":12,"language":"bash","exit_code":0,"failure_kind":7,"duration_ms":1}"#,
    r#"{"ts":13,"language":"bash","exit_code":0,"duration_ms":1,"brief":"yes"}"#,
    // a run line again after all the noise
    r#"{"ts":14,"language":"bash","exit_code":0,"failure_kind":null,"duration_ms":35,"code_hash_stdout_bytes":0,"bytes_elided":0}"#,
];

#[test]
fn the_two_emitters_agree_on_a_journal_with_every_family_of_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = journal_with(dir.path(), MIXED);
    let rust = rust_report(&journal);
    // Guard the premise: the strict reader keeps exactly the five run lines of the fixture.
    assert_eq!(
        rust["total_runs"],
        serde_json::json!(5),
        "fixture drifted from the Rust reader: {rust}"
    );
    assert_eq!(python_report(&journal), rust);
}

#[test]
fn the_two_emitters_agree_on_a_journal_of_only_run_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = journal_with(dir.path(), &MIXED[..4]);
    assert_eq!(python_report(&journal), rust_report(&journal));
}

/// A journal in which NOTHING parses is an error for the Rust reader (`JournalMalformed`); the
/// Python emitter must refuse it too (exit 1), not print a report of zero runs.
#[test]
fn a_journal_where_nothing_parses_is_refused_by_both() {
    let dir = tempfile::tempdir().expect("tempdir");
    let journal = journal_with(dir.path(), &["not json", r#"{"repeat_blocked":{}}"#, "42"]);
    assert!(matches!(
        signal_report_from_journal(&journal),
        Err(SdkError::JournalMalformed { .. })
    ));
    let out = python_emitter(&journal, &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "gen_sdk.py must refuse it too; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// An absent journal is refused by both, and an empty one is an empty report for both.
#[test]
fn an_absent_journal_is_refused_and_an_empty_one_is_an_empty_report_by_both() {
    let dir = tempfile::tempdir().expect("tempdir");
    let absent = dir.path().join("absent.jsonl");
    assert!(matches!(
        signal_report_from_journal(&absent),
        Err(SdkError::JournalMissing(_))
    ));
    assert_eq!(python_emitter(&absent, &[]).status.code(), Some(1));

    let empty = journal_with(dir.path(), &[]);
    assert_eq!(python_report(&empty), rust_report(&empty));
}
