//! Tests of the KPI adherence ruler (`adherence_from_lines`): the real-journal replay
//! against goldens computed by the pre-consolidation code, the process-level success,
//! the corrupt-line accounting, and the ignored replay probe.

use super::adherence_from_lines;

/// The ruler over a sanitized sample of the REAL journal (plus the oddities
/// the real file carries: refusal events, a non-object line, truncated and
/// blank lines, an unknown label, an exit beyond `i32`) is exactly what the
/// ruler printed BEFORE the line parser moved to `touring_foundation::run_outcome`
/// — the golden was computed by the old code, on this very file.
#[test]
fn the_real_journal_sample_reads_exactly_as_the_old_ruler_did() {
    let sample =
        include_str!("../../../../../touring-foundation/tests/fixtures/run_journal_sample.jsonl");
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../touring-foundation/tests/fixtures/run_journal_sample.golden.kpi.json"
    ))
    .expect("golden parses");
    // C3a (04/11/2026, aprofundamento #3, approved by Gabriel): the golden is the OLD ruler's and
    // stays on disk as computed; the new ruler is the golden minus exactly what C3a moved — the 6
    // refusal events and 2 non-object lines leave the denominator (509 → 501), the 8 rows they
    // put under language "?" go with them, one "retry" that was a refusal stops counting as wasted
    // (84 → 83), and the two families are exposed as counters. `runs_ok` does not move.
    let mut expected = golden["adherence"].clone();
    assert_eq!(expected["runs_total"], 509, "the golden changed under this test");
    expected["runs_total"] = 501.into();
    expected["success_rate"] = serde_json::json!(193.0 / 501.0);
    expected["wasted_attempts_retry_pairs"] = 83.into();
    expected["by_language"]
        .as_object_mut()
        .expect("by_language")
        .remove("?");
    expected["refused_events"] = 6.into();
    expected["non_object_lines"] = 2.into();
    let live = adherence_from_lines(sample.lines());
    expected["note"] = live["note"].clone();
    assert_eq!(live, expected);

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../touring-foundation/tests/fixtures/run_journal_sample.jsonl");
    let agg = touring_code::journal::read_journal(&path).expect("the fixture reads");
    assert_eq!(
        super::reuse_from_aggregate(
            &agg,
            touring_intelligence::rl::memory::snippet_stats::LadderTotals::default()
        ),
        golden["reuse"]
    );
}

/// The ruler's success is PROCESS-level: exit 0 with a failure class still
/// counts as ok (a cut output exited clean), and a line with no `exit_code`
/// is a failed run — never an ok one. A refusal event is not a run at all (C3a).
#[test]
fn success_is_exit_zero_and_a_missing_exit_is_not_ok() {
    let linhas = [
        r#"{"ts":1,"language":"bash","exit_code":0,"failure_kind":"output-limit"}"#,
        r#"{"ts":2,"language":"bash","exit_code":0}"#,
        r#"{"ts":3,"language":"bash"}"#,
        r#"{"ts":4,"language":"bash","exit_code":-2,"failure_kind":"timeout"}"#,
        r#"{"success":false,"repeat_blocked":{"ts":9}}"#,
    ];
    let v = adherence_from_lines(linhas.iter().copied());
    // C3a (04/11/2026): the refusal event is no run — out of the denominator, counted apart.
    assert_eq!(v["runs_total"], 4);
    assert_eq!(v["runs_ok"], 2);
    assert_eq!(v["refused_events"], 1);
    assert_eq!(v["by_failure_kind"]["output-limit"], 1);
    assert_eq!(v["by_failure_kind"]["timeout"], 1);
    assert!(v["by_language"].get("?").is_none(), "a refusal is not a run of any language");
}

/// A blank line is counted with the corrupt ones — the ruler's historical
/// reading of untrimmed `lines()`, preserved so the denominator does not move.
#[test]
fn a_blank_line_is_counted_as_malformed_by_this_ruler() {
    let v = adherence_from_lines(["", r#"{"ts":1,"language":"bash","exit_code":0}"#].into_iter());
    assert_eq!(v["runs_total"], 1);
    assert_eq!(v["malformed_lines"], 1);
}

#[test]
fn malformed_journal_lines_are_counted_not_skipped() {
    let linhas = [
        r#"{"ts":1,"language":"bash","exit_code":0}"#,
        "NOT JSON",
        r#"{"ts":2,"language":"bash","exit_code":0}"#,
        "TAMBEM NAO",
    ];
    let v = adherence_from_lines(linhas.iter().copied());
    assert_eq!(
        v["runs_total"], 2,
        "só as linhas válidas entram no denominador"
    );
    assert_eq!(v["malformed_lines"], 2, "as linhas ruins são CONTADAS");
}

// Replay probe (2026-11-02) — runs the KPI over a journal COPY so the same
// numbers can be compared before/after the run-outcome consolidation.
// `--ignored`; reads `TOURING_REPLAY_JOURNAL`, writes `$TOURING_REPLAY_OUT/kpi.json`.
use touring_intelligence::rl::memory::snippet_stats::LadderTotals;

#[test]
#[ignore = "replay probe: needs TOURING_REPLAY_JOURNAL and TOURING_REPLAY_OUT"]
fn replay_kpi_adherence_and_reuse_over_a_journal() {
    let (Some(j), Some(out)) = (
        std::env::var_os("TOURING_REPLAY_JOURNAL"),
        std::env::var_os("TOURING_REPLAY_OUT"),
    ) else {
        return;
    };
    let text = std::fs::read_to_string(&j).expect("journal readable");
    let adherence = super::adherence_from_lines(text.lines());
    let reuse = match touring_code::journal::read_journal(std::path::Path::new(&j)) {
        Ok(agg) => super::reuse_from_aggregate(&agg, LadderTotals::default()),
        Err(e) => serde_json::json!({"error": e.to_string()}),
    };
    let body = serde_json::to_string_pretty(&serde_json::json!({
        "adherence": adherence,
        "reuse": reuse,
    }))
    .expect("serialize");
    std::fs::write(std::path::Path::new(&out).join("kpi.json"), body).expect("write");
}
