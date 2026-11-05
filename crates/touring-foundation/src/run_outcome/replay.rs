//! Replay: the module's reader against a REAL journal.
//!
//! `tests/fixtures/run_journal_sample.jsonl` is a sample of the real
//! `~/.claude/touring/run_journal.jsonl` kept to numeric / enum fields only (no
//! program text, no stdout/stderr, no paths — this repository is public), plus
//! the oddities the real file carries and the synthetic ones the old readers
//! disagreed on: the repeat-gate's `repeat_blocked` events, non-object lines
//! (`123`, `"x"`), truncated and garbage lines, blank lines, a float exit, an
//! unknown label, an exit beyond `i32`, and `exit 0` with a failure class.
//!
//! The `golden.*` files next to it were computed by the code this module
//! REPLACED (the KPI ruler, the M1 counter and the strict aggregate), run on the
//! same sample, before any of it was touched. So each assertion below reads
//! "the new reader says what the old readers said".

use std::collections::BTreeMap;

use serde_json::Value;

use super::{Line, classify_line, count_runs, scan_entries};

const SAMPLE: &str = include_str!("../../tests/fixtures/run_journal_sample.jsonl");

fn golden(text: &str) -> Value {
    serde_json::from_str(text).expect("golden parses")
}

#[test]
fn the_sample_is_the_shape_the_goldens_describe() {
    // A guard on the fixture itself: a truncated checkout would make every
    // assertion below vacuous.
    assert!(
        SAMPLE.lines().count() > 300,
        "the sample lost its body: {} lines",
        SAMPLE.lines().count()
    );
    assert!(SAMPLE.lines().any(|l| l.contains("repeat_blocked")));
    assert!(SAMPLE.lines().any(|l| l.trim().is_empty()));
}

/// The M1 counter: `(runs_total, runs_ok)` is what the old `adherence_counts` returned.
#[test]
fn count_runs_equals_the_old_m1_counter_on_the_sample() {
    let g = golden(include_str!(
        "../../tests/fixtures/run_journal_sample.golden.m1.json"
    ));
    let (total, ok) = count_runs(SAMPLE);
    assert_eq!(total, g["runs_total"].as_u64().expect("runs_total"));
    assert_eq!(ok, g["runs_ok"].as_u64().expect("runs_ok"));
}

/// The lenient view reproduces every number of the old KPI ruler: the
/// denominator, the ok count, the per-language and per-class histograms, and
/// the corrupt-line count (which the ruler has always taken blank lines into).
#[test]
fn the_lenient_view_reproduces_the_old_kpi_ruler_on_the_sample() {
    let g = golden(include_str!(
        "../../tests/fixtures/run_journal_sample.golden.kpi.json"
    ));
    let a = &g["adherence"];

    let (mut total, mut ok, mut corrupt) = (0u64, 0u64, 0u64);
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    let mut by_lang: BTreeMap<String, u64> = BTreeMap::new();
    for line in SAMPLE.lines() {
        match classify_line(line) {
            Line::Blank | Line::Malformed => corrupt += 1,
            Line::Record(run) => {
                total += 1;
                let outcome = run.outcome();
                ok += u64::from(outcome.exited_zero());
                if let Some(kind) = outcome.failure {
                    *by_kind.entry(kind.as_str().to_string()).or_default() += 1;
                }
                *by_lang
                    .entry(run.language().unwrap_or("?").to_string())
                    .or_default() += 1;
            }
        }
    }
    assert_eq!(total, a["runs_total"].as_u64().expect("runs_total"));
    assert_eq!(ok, a["runs_ok"].as_u64().expect("runs_ok"));
    assert_eq!(
        corrupt,
        a["malformed_lines"].as_u64().expect("malformed_lines")
    );
    assert_eq!(
        serde_json::to_value(&by_kind).expect("ser"),
        a["by_failure_kind"]
    );
    assert_eq!(
        serde_json::to_value(&by_lang).expect("ser"),
        a["by_language"]
    );
}

/// The strict view reproduces the old aggregate's entry and corruption counts,
/// and its per-language / per-label tallies.
#[test]
fn the_strict_view_reproduces_the_old_aggregate_on_the_sample() {
    let g = golden(include_str!(
        "../../tests/fixtures/run_journal_sample.golden.aggregate.json"
    ));
    let scan = scan_entries(
        SAMPLE
            .lines()
            .enumerate()
            .map(|(i, l)| (i, Ok(l.to_string()))),
    )
    .expect("no io error on an in-memory sample");
    assert_eq!(
        scan.entries.len() as u64,
        g["total_entries"].as_u64().expect("total")
    );
    assert_eq!(
        scan.malformed_lines,
        g["malformed_lines"].as_u64().expect("malformed")
    );
    assert_eq!(
        scan.malformed_first_line,
        g["malformed_first_line"].as_u64(),
        "the first corrupt line is where the old reader pointed"
    );

    let mut langs: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    let mut labels: BTreeMap<&str, u64> = BTreeMap::new();
    for e in &scan.entries {
        let slot = langs.entry(e.language.as_str()).or_default();
        slot.0 += 1;
        slot.1 += u64::from(e.outcome().has_failure());
        if let Some(label) = e.failure_kind.as_deref() {
            *labels.entry(label).or_default() += 1;
        }
    }
    for (lang, (count, failures)) in &langs {
        assert_eq!(g["by_language"][*lang]["count"], *count, "{lang}");
        assert_eq!(
            g["by_language"][*lang]["failure_count"], *failures,
            "{lang}"
        );
    }
    assert_eq!(
        g["by_language"].as_object().expect("by_language").len(),
        langs.len()
    );
    assert_eq!(
        serde_json::to_value(&labels).expect("ser"),
        g["failure_counts"]
    );
}

/// The one decision the readers disagree on, stated on the sample: the repeat-gate
/// events are runs to the lenient ruler and corruption to the strict reader.
#[test]
fn a_refusal_event_is_a_run_to_the_ruler_and_corruption_to_the_aggregate() {
    let events = SAMPLE
        .lines()
        .filter(|l| l.contains("repeat_blocked"))
        .count() as u64;
    assert!(events > 0);
    let lenient_runs = SAMPLE
        .lines()
        .filter(|l| matches!(classify_line(l), Line::Record(_)))
        .count() as u64;
    let strict = scan_entries(
        SAMPLE
            .lines()
            .enumerate()
            .map(|(i, l)| (i, Ok(l.to_string()))),
    )
    .expect("no io error");
    assert!(
        lenient_runs > strict.entries.len() as u64,
        "the lenient view counts the events the strict one rejects"
    );
}
