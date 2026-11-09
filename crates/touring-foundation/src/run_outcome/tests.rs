//! Tests of the run journal: the writer is byte-for-byte the one it replaced, the two
//! reading views, the tail searches and the append discipline.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::journal::PIPE_BUF;
use super::recurrence::TAIL_BYTES;
use super::*;

fn run_log<'a>(
    run_id: &'a str,
    language: &'a str,
    stdout_bytes: usize,
    outcome: RunOutcome,
) -> RunLog<'a> {
    RunLog {
        run_id,
        language,
        stdout_bytes,
        outcome,
        duration_ms: 0,
        bytes_elided: 0,
        code_hash: "0000000000000000",
        code_simhash: 0,
        code_simhash_v2: 0,
        source_kind: None,
        source_file: None,
        harvest: None,
        brief: false,
        orchestrate: false,
        hint: None,
        tmp_bytes: 0,
    }
}

fn outcome(wire: i32, failure: Option<FailureKind>) -> RunOutcome {
    RunOutcome::new(ExitStatus::from_wire(wire), failure)
}

// ── the writer is byte-for-byte the one it replaces ─────────────────────

/// Five REAL journal lines (the latest schema, written by the writer this
/// module replaced), sanitized only in free-text values (`file` paths).
/// `RunLog::to_json` must reproduce every byte: existing journals on disk
/// must keep parsing the same, and new records must serialize identically to
/// what the old writer produced for the same outcome.
#[test]
fn to_json_reproduces_real_journal_lines_byte_for_byte() {
    // (golden line, the RunLog that produced it, ts, prior_same_hash)
    let mut cases: Vec<(&str, RunLog<'_>, u64, u32)> = Vec::new();

    let mut ok_hinted = run_log("run-1793294236685-459457", "python", 351, outcome(0, None));
    ok_hinted.duration_ms = 21;
    ok_hinted.code_hash = "c3c308319b7afaa7";
    ok_hinted.code_simhash = 0x0d9e_925d_7b64_d83f;
    ok_hinted.code_simhash_v2 = 0x0d9e_925d_7b64_d83f;
    ok_hinted.source_kind = Some("inline");
    ok_hinted.hint = Some(("snippet:auto:1abf59ca6914", 0.379));
    cases.push((
        r#"{"brief":false,"bytes_elided":0,"code_hash":"c3c308319b7afaa7","code_hash_stdout_bytes":351,"code_simhash":"0d9e925d7b64d83f","code_simhash_v2":"0d9e925d7b64d83f","duration_ms":21,"exit_code":0,"failure_kind":null,"file":null,"harvest":null,"hint":{"key":"snippet:auto:1abf59ca6914","sim":0.379},"language":"python","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793294236685-459457","source":"inline","tmp_bytes":0,"ts":1793294236}"#,
        ok_hinted,
        1_793_294_236,
        0,
    ));

    let mut timed_out = run_log(
        "run-1793221341926-392070",
        "bash",
        0,
        outcome(-2, Some(FailureKind::Timeout)),
    );
    timed_out.duration_ms = 60252;
    timed_out.code_hash = "fdd6f52f459db556";
    timed_out.code_simhash = 0xa9c6_a21b_9254_81d1;
    timed_out.code_simhash_v2 = 0xa9c6_a21b_9254_83c1;
    timed_out.source_kind = Some("inline");
    cases.push((
        r#"{"brief":false,"bytes_elided":0,"code_hash":"fdd6f52f459db556","code_hash_stdout_bytes":0,"code_simhash":"a9c6a21b925481d1","code_simhash_v2":"a9c6a21b925483c1","duration_ms":60252,"exit_code":-2,"failure_kind":"timeout","file":null,"harvest":null,"hint":null,"language":"bash","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793221341926-392070","source":"inline","tmp_bytes":0,"ts":1793221402}"#,
        timed_out,
        1_793_221_402,
        0,
    ));

    let mut exception = run_log(
        "run-1793200215179-2909345",
        "python",
        10,
        outcome(3, Some(FailureKind::Exception)),
    );
    exception.duration_ms = 35;
    exception.code_hash = "0a8b054edec7bbfb";
    exception.code_simhash = 0xa900_068e_274b_4d20;
    exception.code_simhash_v2 = 0xa900_068e_274b_4d20;
    cases.push((
        r#"{"brief":false,"bytes_elided":0,"code_hash":"0a8b054edec7bbfb","code_hash_stdout_bytes":10,"code_simhash":"a900068e274b4d20","code_simhash_v2":"a900068e274b4d20","duration_ms":35,"exit_code":3,"failure_kind":"exception","file":null,"harvest":null,"hint":null,"language":"python","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793200215179-2909345","source":null,"tmp_bytes":0,"ts":1793200215}"#,
        exception,
        1_793_200_215,
        0,
    ));

    let mut cut = run_log(
        "run-1793545710305-1273806",
        "python",
        1_006_488,
        outcome(1, Some(FailureKind::OutputLimit)),
    );
    cut.duration_ms = 11353;
    cut.bytes_elided = 998_265;
    cut.code_hash = "5d3a06ba80330eb3";
    cut.code_simhash = 0x910e_9b15_7a5e_9d2b;
    cut.code_simhash_v2 = 0x910c_9315_6b5c_9d29;
    cut.source_kind = Some("file");
    cut.source_file = Some("scripts/a21_feat.py");
    cases.push((
        r#"{"brief":false,"bytes_elided":998265,"code_hash":"5d3a06ba80330eb3","code_hash_stdout_bytes":1006488,"code_simhash":"910e9b157a5e9d2b","code_simhash_v2":"910c93156b5c9d29","duration_ms":11353,"exit_code":1,"failure_kind":"output-limit","file":"scripts/a21_feat.py","harvest":null,"hint":null,"language":"python","orchestrate":false,"prior_same_hash":0,"run_id":"run-1793545710305-1273806","source":"file","tmp_bytes":0,"ts":1793545721}"#,
        cut,
        1_793_545_721,
        0,
    ));

    let mut recurring = run_log("run-1793198511710-1032087", "bash", 41, outcome(0, None));
    recurring.duration_ms = 4;
    recurring.code_hash = "863158b0212508cf";
    recurring.code_simhash = 0xbb93_121c_fed3_c4c0;
    recurring.code_simhash_v2 = 0xbb93_121c_fed3_c4c0;
    cases.push((
        r#"{"brief":false,"bytes_elided":0,"code_hash":"863158b0212508cf","code_hash_stdout_bytes":41,"code_simhash":"bb93121cfed3c4c0","code_simhash_v2":"bb93121cfed3c4c0","duration_ms":4,"exit_code":0,"failure_kind":null,"file":null,"harvest":null,"hint":null,"language":"bash","orchestrate":false,"prior_same_hash":8,"run_id":"run-1793198511710-1032087","source":null,"tmp_bytes":0,"ts":1793198511}"#,
        recurring,
        1_793_198_511,
        8,
    ));

    for (golden, log, ts, prior) in cases {
        let json = log.to_json(ts, prior);
        assert_eq!(
            serde_json::to_string(&json).expect("serializes"),
            golden,
            "the record for {} drifted from what the old writer produced",
            log.run_id
        );
    }
}

#[test]
fn to_json_carries_the_flags_harvest_and_file_the_gates_read() {
    let mut log = run_log("run-9", "bash", 0, outcome(0, None));
    log.source_kind = Some("file");
    log.source_file = Some("scripts/x.sh");
    log.harvest = Some("slug");
    log.brief = true;
    log.orchestrate = true;
    log.tmp_bytes = 4096;
    let v = log.to_json(1, 0);
    assert_eq!(v["source"], "file");
    assert_eq!(v["file"], "scripts/x.sh");
    assert_eq!(v["harvest"], "slug");
    assert_eq!(v["brief"], true);
    assert_eq!(v["orchestrate"], true);
    assert_eq!(v["tmp_bytes"], 4096);
    assert_eq!(v["hint"], Value::Null);
}

// ── paths ────────────────────────────────────────────────────────────────

#[test]
fn the_journal_path_is_the_canonical_one() {
    assert_eq!(
        journal_path(Path::new("/home/test"))
            .to_str()
            .expect("utf-8 literal"),
        "/home/test/.claude/touring/run_journal.jsonl"
    );
    assert_eq!(
        Journal::under_home(Path::new("/h")).path(),
        Path::new("/h/.claude/touring/run_journal.jsonl")
    );
    assert_eq!(Journal::at("/x/y.jsonl").path(), Path::new("/x/y.jsonl"));
}

// ── the lenient view ─────────────────────────────────────────────────────

#[test]
fn classify_line_separates_blank_malformed_and_records() {
    assert!(matches!(classify_line(""), Line::Blank));
    assert!(matches!(classify_line("  \t "), Line::Blank));
    assert!(matches!(classify_line("NOT JSON"), Line::Malformed));
    assert!(matches!(classify_line("{\"ts\":17"), Line::Malformed));
    assert!(matches!(
        classify_line("{\"a\":1} trailing"),
        Line::Malformed
    ));
    assert!(matches!(classify_line("123"), Line::Record(_)));
    assert!(matches!(classify_line("  {\"ts\":1}  "), Line::Record(_)));
}

/// C3a (04/11/2026): the family says what a parseable line IS, beside the reader that parses it.
#[test]
fn family_tells_runs_from_refusals_and_non_objects() {
    use super::LineFamily;
    let fam = |line: &str| match classify_line(line) {
        Line::Record(r) => r.family(),
        other => panic!("{line:?} is not a record: {other:?}"),
    };
    assert_eq!(
        fam(r#"{"exit_code":0,"language":"python"}"#),
        LineFamily::Run
    );
    assert_eq!(
        fam(r#"{"exit_code":"0"}"#),
        LineFamily::Run,
        "an unusable exit is still a run"
    );
    assert_eq!(
        fam("{}"),
        LineFamily::Run,
        "an empty record was always counted as a run"
    );
    assert_eq!(
        fam(r#"{"repeat_blocked":true,"ts":1}"#),
        LineFamily::Refusal
    );
    assert_eq!(fam("123"), LineFamily::NotObject);
    assert_eq!(fam(r#""x""#), LineFamily::NotObject);
    assert_eq!(fam("[1,2]"), LineFamily::NotObject);
}

fn record(line: &str) -> RunRecord {
    match classify_line(line) {
        Line::Record(r) => r,
        other => panic!("not a record: {other:?}"),
    }
}

#[test]
fn a_record_reads_its_fields_totally() {
    let r = record(
        r#"{"ts":1793,"language":"bash","exit_code":-2,"failure_kind":"timeout","duration_ms":5}"#,
    );
    assert_eq!(r.language(), Some("bash"));
    assert_eq!(r.ts(), Some(1793));
    assert_eq!(r.exit(), ExitStatus::TimedOut);
    assert_eq!(r.failure_kind(), Some(FailureKind::Timeout));
    assert_eq!(
        r.outcome(),
        RunOutcome::new(ExitStatus::TimedOut, Some(FailureKind::Timeout))
    );
}

/// A line that is valid JSON but not a run record: every accessor answers
/// "absent", and an absent exit is NOT a zero exit.
#[test]
fn a_foreign_json_line_reads_as_a_failed_run_with_nothing_known() {
    for line in [
        "123",
        "\"x\"",
        "[1,2]",
        "null",
        r#"{"success":false,"repeat_blocked":{"ts":9}}"#,
    ] {
        let r = record(line);
        assert_eq!(r.language(), None, "{line}");
        assert_eq!(r.ts(), None, "{line}");
        assert_eq!(r.exit(), ExitStatus::NoExitCode, "{line}");
        assert!(!r.outcome().exited_zero(), "{line}");
        assert_eq!(r.failure_kind(), None, "{line}");
    }
}

/// Exit codes the old reader's `as_i64` could not use: a float, a string,
/// beyond `i32`, null. All read as no exit (non-zero, never ok).
#[test]
fn an_unusable_exit_code_reads_as_no_exit_not_as_zero() {
    for line in [
        r#"{"exit_code":0.0}"#,
        r#"{"exit_code":"0"}"#,
        r#"{"exit_code":null}"#,
        r#"{"exit_code":3000000000}"#,
        r#"{"exit_code":-3000000000}"#,
    ] {
        assert_eq!(record(line).exit(), ExitStatus::NoExitCode, "{line}");
    }
    assert_eq!(record(r#"{"exit_code":0}"#).exit(), ExitStatus::Code(0));
    assert_eq!(record(r#"{"exit_code":141}"#).exit(), ExitStatus::Code(141));
}

#[test]
fn failure_kind_goes_through_the_canonical_taxonomy() {
    assert_eq!(
        record(r#"{"failure_kind":"kind-que-nao-existe"}"#).failure_kind(),
        Some(FailureKind::Other)
    );
    assert_eq!(record(r#"{"failure_kind":null}"#).failure_kind(), None);
    assert_eq!(record(r#"{"failure_kind":7}"#).failure_kind(), None);
    assert_eq!(record("{}").failure_kind(), None);
    assert_eq!(
        record(r#"{"failure_kind":"proc-exit"}"#).failure_kind(),
        Some(FailureKind::ProcExit)
    );
}

#[test]
fn count_runs_counts_json_lines_and_exit_zero_and_skips_garbage() {
    let journal = concat!(
        "{\"exit_code\":0,\"language\":\"python\"}\n",
        "lixo-nao-json\n",
        "\n",
        "{\"exit_code\":1,\"language\":\"bash\"}\n",
        "{\"exit_code\":0}\n",
        "{\"success\":false}\n",
        "7\n",
        "{\"exit_code\":0,\"failure_kind\":\"output-limit\"}\n",
    );
    // 6 JSON lines; exit 0 on 3 (the output-limit one counts: PROCESS-level).
    assert_eq!(count_runs(journal), (6, 3));
    assert_eq!(count_runs(""), (0, 0));
}

// ── the strict view ──────────────────────────────────────────────────────

fn lines_iter(s: &str) -> impl Iterator<Item = (usize, std::io::Result<String>)> + '_ {
    s.lines().enumerate().map(|(i, l)| (i, Ok(l.to_string())))
}

#[test]
fn scan_entries_is_strict_counts_malformed_and_skips_blank() {
    let s = concat!(
        r#"{"ts":1,"language":"bash","exit_code":0,"duration_ms":5}"#,
        "\n",
        "\n",
        "NOT JSON\n",
        // valid JSON, but not a full entry (no duration_ms): STRICT view rejects it
        r#"{"ts":2,"language":"bash","exit_code":0}"#,
        "\n",
        r#"{"success":false,"repeat_blocked":{"ts":3}}"#,
        "\n",
        r#"{"ts":4,"language":"python","exit_code":1,"failure_kind":"exception","duration_ms":9}"#,
    );
    let scan = scan_entries(lines_iter(s)).expect("no io error");
    assert_eq!(scan.entries.len(), 2);
    assert_eq!(scan.malformed_lines, 3);
    assert_eq!(scan.malformed_first_line, Some(2));
    let (line, _) = scan.first_error.expect("first error kept");
    assert_eq!(line, 2);
}

#[test]
fn scan_entries_propagates_an_io_error() {
    let bad = [(0usize, Err(std::io::Error::other("boom")))];
    assert!(scan_entries(bad).is_err());
}

#[test]
fn an_entry_outcome_decodes_wire_and_label() {
    let e: JournalEntry = serde_json::from_str(
        r#"{"ts":1,"language":"bash","exit_code":-2,"failure_kind":"timeout","duration_ms":3}"#,
    )
    .expect("parses");
    assert_eq!(
        e.outcome(),
        RunOutcome::new(ExitStatus::TimedOut, Some(FailureKind::Timeout))
    );
    let e: JournalEntry = serde_json::from_str(
        r#"{"ts":1,"language":"bash","exit_code":0,"failure_kind":"zzz","duration_ms":3}"#,
    )
    .expect("parses");
    assert_eq!(e.outcome().failure, Some(FailureKind::Other));
    assert!(e.outcome().exited_zero() && !e.outcome().is_success());
}

// ── the tail searches ────────────────────────────────────────────────────

fn tmp_journal() -> (tempfile::TempDir, Journal) {
    let dir = tempfile::tempdir().expect("tempdir");
    let j = Journal::at(dir.path().join("run_journal.jsonl"));
    (dir, j)
}

#[test]
fn count_prior_same_hash_reads_the_tail_and_fails_open() {
    let (_dir, j) = tmp_journal();
    assert_eq!(
        j.count_prior_same_hash("aaaaaaaaaaaaaaaa"),
        0,
        "no file: fail-open"
    );
    std::fs::write(
        j.path(),
        concat!(
            "{\"code_hash\":\"aaaaaaaaaaaaaaaa\",\"exit_code\":0}\n",
            "{\"code_hash\":\"bbbbbbbbbbbbbbbb\",\"exit_code\":0}\n",
            "{\"code_hash\":\"aaaaaaaaaaaaaaaa\",\"exit_code\":1}\n",
        ),
    )
    .expect("write");
    assert_eq!(j.count_prior_same_hash("aaaaaaaaaaaaaaaa"), 2);
    assert_eq!(j.count_prior_same_hash("bbbbbbbbbbbbbbbb"), 1);
    assert_eq!(j.count_prior_same_hash("cccccccccccccccc"), 0);
}

#[test]
fn count_prior_same_hash_sees_only_the_last_512_kib() {
    let (_dir, j) = tmp_journal();
    let old = "{\"code_hash\":\"aaaaaaaaaaaaaaaa\"}\n";
    let filler = "{\"x\":\"".to_string() + &"y".repeat(1000) + "\"}\n";
    let mut body = String::from(old);
    while body.len() < (TAIL_BYTES as usize) + 4096 {
        body.push_str(&filler);
    }
    body.push_str("{\"code_hash\":\"aaaaaaaaaaaaaaaa\"}\n");
    std::fs::write(j.path(), body).expect("write");
    assert_eq!(
        j.count_prior_same_hash("aaaaaaaaaaaaaaaa"),
        1,
        "the run older than the tail window faded out by design"
    );
}

#[test]
fn find_prior_similar_respects_tau_orders_by_distance_and_reads_metadata() {
    let (_dir, j) = tmp_journal();
    let base: u64 = 0xF0F0_F0F0_F0F0_F0F0;
    let near = base ^ 0b11; // 2 bits
    let nearer = base ^ 0b1; // 1 bit
    let far = base ^ 0xFFFF; // 16 bits
    let line = |id: &str, h: u64, harvest: &str| {
        format!("{{\"run_id\":\"{id}\",\"code_simhash_v2\":\"{h:016x}\",\"harvest\":{harvest}}}\n")
    };
    let body = [
        line("far", far, "null"),
        line("near", near, "\"slug-a\""),
        line("nearer", nearer, "null"),
        "{\"run_id\":\"old\",\"code_simhash\":\"f0f0f0f0f0f0f0f0\"}\n".to_string(),
    ]
    .concat();
    std::fs::write(j.path(), body).expect("write");
    let s = j.find_prior_similar(SimhashVersion::V2, base, 9);
    let ids: Vec<_> = s
        .priors
        .iter()
        .map(|p| (p.run_id.as_str(), p.dist))
        .collect();
    assert_eq!(ids, [("nearer", 1), ("near", 2)]);
    assert_eq!(s.priors[1].harvest.as_deref(), Some("slug-a"));
    assert_eq!(s.priors[0].harvest, None);
    assert_eq!(s.malformed_lines, 0);
    // v1 and v2 never compare across versions: the v1 search sees only the v1 line.
    let v1 = j.find_prior_similar(SimhashVersion::V1, base, 9);
    assert_eq!(v1.priors.len(), 1);
    assert_eq!(v1.priors[0].run_id, "old");
    // τ is inclusive
    assert_eq!(
        j.find_prior_similar(SimhashVersion::V2, base, 1)
            .priors
            .len(),
        1
    );
    assert_eq!(
        j.find_prior_similar(SimhashVersion::V2, base, 0)
            .priors
            .len(),
        0
    );
}

#[test]
fn find_prior_similar_counts_corrupt_tail_lines_and_fails_open_without_a_file() {
    let (_dir, j) = tmp_journal();
    assert!(
        j.find_prior_similar(SimhashVersion::V2, 0, 9)
            .priors
            .is_empty()
    );
    std::fs::write(
        j.path(),
        concat!(
            "{\"run_id\":\"a\",\"code_simhash_v2\":\"0000000000000000\"}\n",
            "{\"run_id\":\"b\",\"code_simhash_v2\":\"00000000\n",
            "{\"run_id\":\"c\",\"code_simhash_v2\":\"zzzz\"}\n",
        ),
    )
    .expect("write");
    let s = j.find_prior_similar(SimhashVersion::V2, 0, 9);
    assert_eq!(s.priors.len(), 1, "only `a` parses to a hash");
    assert_eq!(
        s.malformed_lines, 1,
        "the truncated line is COUNTED, not skipped silently"
    );
}

// ── the writer ───────────────────────────────────────────────────────────

#[test]
fn record_run_appends_one_line_and_returns_the_prior_count() {
    let (dir, _) = tmp_journal();
    let j = Journal::under_home(dir.path()); // dir `.claude/touring` does not exist yet
    let mut log = run_log("run-1", "bash", 3, outcome(0, None));
    log.code_hash = "1111111111111111";
    assert_eq!(j.record_run(&log), 0, "first run: nothing before it");
    log.run_id = "run-2";
    assert_eq!(
        j.record_run(&log),
        1,
        "second run sees the first, not itself"
    );
    let text = j.read_to_string().expect("journal exists");
    let ids: Vec<_> = text
        .lines()
        .map(|l| {
            let r: Value = serde_json::from_str(l).expect("each line is one record");
            (
                r["run_id"].as_str().map(str::to_string),
                r["prior_same_hash"].as_u64(),
            )
        })
        .collect();
    assert_eq!(
        ids,
        [
            (Some("run-1".into()), Some(0)),
            (Some("run-2".into()), Some(1))
        ]
    );
}

#[test]
fn append_json_fails_open_when_the_directory_is_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let j = Journal::at(dir.path().join("no-such-dir").join("j.jsonl"));
    assert!(!j.append_json(&json!({"a": 1})));
}

/// Concurrent writers, small AND above `PIPE_BUF`: every line must still be
/// one whole record — the splice the single-write discipline exists to prevent.
#[test]
fn concurrent_appends_never_splice_a_record() {
    let (_dir, j) = tmp_journal();
    let big = "z".repeat(PIPE_BUF * 3);
    std::thread::scope(|s| {
        for t in 0..8u32 {
            let j = &j;
            let big = &big;
            s.spawn(move || {
                for i in 0..40u32 {
                    let rec = if i % 4 == 0 {
                        json!({"t": t, "i": i, "pad": big})
                    } else {
                        json!({"t": t, "i": i})
                    };
                    assert!(j.append_json(&rec));
                }
            });
        }
    });
    let text = j.read_to_string().expect("journal");
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let v: Value = serde_json::from_str(line).expect("no spliced/garbled line");
        seen.insert((v["t"].as_u64().expect("t"), v["i"].as_u64().expect("i")));
    }
    assert_eq!(seen.len(), 8 * 40, "every record landed exactly once");
}

/// T-1 (31/10/2026) — the journal CANNOT be corrupted by concurrency.
///
/// The defect: the record was written in TWO calls (the body and the `\n`),
/// and two processes spliced between them — 400 invalid lines in
/// 47,102 in the real journal, 99 of them with 2+ `run_id` (ids 1 ms apart).
///
/// The test re-executes the test binary itself in N PROCESSES (the child mode
/// is the SAME test, with the environment variable that makes it only
/// write — which is what prevents the recursion). Each process appends M
/// records; the final journal must have ZERO invalid lines and N×M
/// records.
#[test]
fn journal_append_is_atomic_across_processes() {
    const FILHOS: usize = 12;
    const POR_FILHO: usize = 250;

    // Child mode: only writes and exits.
    if let Ok(dir) = std::env::var("TOURING_JOURNAL_RACE_DIR") {
        let journal = Journal::at(PathBuf::from(dir).join("run_journal.jsonl"));
        for i in 0..POR_FILHO {
            journal.append_json(&json!({
                "run_id": format!("run-corrida-{i}"),
                "language": "bash",
                "ts": i,
                "exit_code": 0,
                "duration_ms": 1,
                "failure_kind": Value::Null,
                "bytes_elided": 0,
                "code_hash_stdout_bytes": 0,
                "code_hash": "abc",
                "code_simhash": "0000000000000000",
                "code_simhash_v2": "0000000000000000",
                "prior": 0,
            }));
        }
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let exe = std::env::current_exe().expect("current_exe");
    let mut filhos = Vec::new();
    for _ in 0..FILHOS {
        let mut c = std::process::Command::new(&exe);
        c.env("TOURING_JOURNAL_RACE_DIR", dir.path());
        c.arg("journal_append_is_atomic_across_processes");
        c.arg("--nocapture");
        c.arg("--test-threads=1");
        filhos.push(c.spawn().expect("spawn do filho"));
    }
    for mut f in filhos {
        assert!(f.wait().expect("wait do filho").success(), "o filho falhou");
    }

    let conteudo = Journal::at(dir.path().join("run_journal.jsonl"))
        .read_to_string()
        .expect("lê o journal");
    let (mut invalidas, mut total) = (0u64, 0u64);
    for line in conteudo.lines() {
        match classify_line(line) {
            Line::Blank => {}
            Line::Malformed => {
                total += 1;
                invalidas += 1;
            }
            Line::Record(_) => total += 1,
        }
    }
    assert_eq!(invalidas, 0, "linhas inválidas no journal: {invalidas}");
    assert_eq!(
        total,
        (FILHOS * POR_FILHO) as u64,
        "registros: {total} (esperado {})",
        FILHOS * POR_FILHO
    );
}
