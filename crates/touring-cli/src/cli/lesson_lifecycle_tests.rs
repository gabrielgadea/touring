//! Tests of the `lesson_lifecycle` shadow ruler (B1, 01/11/2026).
//!
//! The pair extractor is a PORT of `touring-server`'s `extract_error_resolution_pairs`, which
//! `touring-cli` cannot call (the server sits above it in the dependency graph). The parity
//! tests below are the original's own unit tests, case by case, so the two stay one
//! algorithm; the Python reference `medicao/f4a_lessons.py` is compared on the real corpus by
//! the probe, not here.

use super::*;
use crate::cli::transcript_scan::{KPI_CAPS, fixtures::*};
use rusqlite::Connection;
use serde_json::json;

/// A marker planted in the error text of every fixture that must not leak.
const CANARY: &str = "CANARIO-NAO-VAZAR";
/// A three-byte character: a byte cut at the wrong place would split it.
const WIDE: &str = "€";

fn lessons(lines: &[String]) -> Vec<RawPair> {
    let mut t = LessonTracker::default();
    t.observe_lines(lines.iter().map(String::as_str));
    t.finish().pairs
}

/// A failed call of `tool` with `err`, then a successful one.
fn fail_then_ok(tool: &str, n: &str, err: &str) -> Vec<String> {
    vec![
        assistant_call(&format!("f{n}"), tool, json!({"command": "x"})),
        call_result(&format!("f{n}"), true, json!(err)),
        assistant_call(&format!("o{n}"), tool, json!({"command": "y"})),
        call_result(&format!("o{n}"), false, json!("ok")),
    ]
}

fn session_of(id: &str, tool: &str, errs: &[&str]) -> (String, SessionLessons) {
    let mut lines = Vec::new();
    for (i, e) in errs.iter().enumerate() {
        lines.extend(fail_then_ok(tool, &i.to_string(), e));
    }
    (id.to_string(), SessionLessons { pairs: lessons(&lines) })
}

fn no_db() -> Result<StoredLessons, String> {
    Err("no memory.db".to_string())
}

// ── normalisation ───────────────────────────────────────────────────────────────────────────

/// The four substitutions, in the reference's order: paths first (their digits go with them),
/// then long hex, then numbers, then whitespace — and the ends are trimmed.
#[test]
fn normalize_key_replaces_paths_hex_numbers_and_collapses_whitespace() {
    assert_eq!(
        normalize_key("error: cannot open /home/gabrielgadea/projects/x.rs at line 42"),
        "error: cannot open <path> at line <n>"
    );
    assert_eq!(normalize_key("commit deadbeefcafe0123 failed"), "commit <hex> failed");
    assert_eq!(normalize_key("addr 0xdeadbeef and 12345"), "addr <hex> and <n>");
    assert_eq!(normalize_key("/tmp/run-12345/out.log"), "<path>", "digits inside a path go with it");
    assert_eq!(normalize_key("a  \n\t b"), "a b");
    assert_eq!(normalize_key("  x  "), "x");
    assert_eq!(normalize_key("1234567890"), "<hex>", "ten digits are hex before they are a number");
}

/// The key is cut AFTER normalising, from the text already cut at 500 chars — so two errors
/// that differ only in numbers, paths or past the 500th char are the same fact.
#[test]
fn lesson_key_is_cut_after_the_normalisation_and_after_the_500_char_cut() {
    let a = format!("failed at /a/b.rs:12 {}", "x".repeat(700));
    let b = format!("failed at /c/d/e.rs:99 {}", "x".repeat(700));
    assert_eq!(lesson_key(&a), lesson_key(&b));
    let head = "same ".repeat(100);
    assert_eq!(
        lesson_key(&format!("{head}tail-one")),
        lesson_key(&format!("{head}tail-two")),
        "differences past the 200th normalised char are not part of the key"
    );
    assert_ne!(lesson_key("cannot find crate foo"), lesson_key("cannot find crate bar"));
    assert!(lesson_key(&WIDE.repeat(600)).chars().count() <= KEY_PREFIX_CHARS);
}

// ── the miner's state machine (ported unit tests, one for one) ───────────────────────────────

/// `test_sm_basic_pair`: failed Bash then successful Bash is exactly one pair.
#[test]
fn a_failure_resolved_by_the_same_tool_is_one_pair() {
    let p = lessons(&fail_then_ok("Bash", "1", "error: command not found"));
    assert_eq!(p.len(), 1);
    assert_eq!((p[0].tool.as_str(), p[0].error_text.as_str()), ("Bash", "error: command not found"));
}

/// `test_sm_no_resolution_yields_empty`.
#[test]
fn a_failure_with_no_later_success_yields_no_pair() {
    let p = lessons(&[
        assistant_call("f", "Bash", json!({})),
        call_result("f", true, json!("error")),
    ]);
    assert!(p.is_empty());
}

/// `test_sm_different_tool_not_a_resolution`: the resolution must be the same tool.
#[test]
fn a_success_of_another_tool_is_not_a_resolution() {
    let p = lessons(&[
        assistant_call("f", "Bash", json!({})),
        call_result("f", true, json!("error")),
        assistant_call("e", "Edit", json!({})),
        call_result("e", false, json!("ok")),
    ]);
    assert!(p.is_empty());
}

/// `test_sm_resolution_beyond_window`: a chain of failures longer than the window is dropped;
/// one within it emits ONE pair from the FIRST failure.
#[test]
fn a_chain_longer_than_the_window_is_dropped_and_a_short_one_emits_its_first_error() {
    let chain = |failures: usize| {
        let mut lines = Vec::new();
        for i in 0..failures {
            lines.push(assistant_call(&format!("f{i}"), "Bash", json!({})));
            lines.push(call_result(&format!("f{i}"), true, json!(format!("error {i}"))));
        }
        lines.push(assistant_call("ok", "Bash", json!({})));
        lines.push(call_result("ok", false, json!("ok")));
        lessons(&lines)
    };
    assert!(chain(RESOLUTION_SCAN_WINDOW + 1).is_empty());
    let within = chain(RESOLUTION_SCAN_WINDOW);
    assert_eq!(within.len(), 1);
    assert_eq!(within[0].error_text, "error 0", "the broadest context: the first failure");
}

/// `test_sm_error_text_truncated` and `test_sm_utf8_truncation_boundary`: 500 CHARS, never a
/// byte cut.
#[test]
fn the_error_text_is_cut_at_500_chars_on_a_char_boundary() {
    for unit in ["x", WIDE] {
        let p = lessons(&fail_then_ok("Bash", "1", &unit.repeat(ERROR_TEXT_MAX + 7)));
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].error_text.chars().count(), ERROR_TEXT_MAX);
    }
}

/// `test_sm_dangling_tool_use_id` and `test_sm_empty_input`.
#[test]
fn dangling_results_and_empty_input_yield_nothing() {
    assert!(lessons(&[call_result("ghost", true, json!("error from nowhere"))]).is_empty());
    assert!(lessons(&[]).is_empty());
}

/// A call whose result never arrives breaks the chain (the reference resets on it).
#[test]
fn a_call_without_a_result_resets_the_chain() {
    let p = lessons(&[
        assistant_call("f1", "Bash", json!({})),
        call_result("f1", true, json!("error")),
        assistant_call("lost", "Bash", json!({})),
        assistant_call("ok", "Bash", json!({})),
        call_result("ok", false, json!("ok")),
    ]);
    assert!(p.is_empty());
}

/// A tool result that is a list of text blocks is the texts JOINED with nothing (the miner's
/// `flatten_tool_result_content`); a list with no text block falls back to its JSON.
#[test]
fn a_block_list_result_is_flattened_as_the_miner_flattens_it() {
    let blocks = json!([{"type": "text", "text": "cannot "}, {"type": "text", "text": "find"}]);
    let mut lines = vec![
        assistant_call("f", "Bash", json!({})),
        call_result("f", true, blocks),
        assistant_call("o", "Bash", json!({})),
        call_result("o", false, json!("ok")),
    ];
    assert_eq!(lessons(&lines)[0].error_text, "cannot find");
    lines[1] = call_result("f", true, json!([{"type": "image"}]));
    assert_eq!(lessons(&lines)[0].error_text, r#"[{"type":"image"}]"#);
}

/// The first result of an id wins (`entry().or_insert_with`).
#[test]
fn the_first_result_of_an_id_wins() {
    let p = lessons(&[
        assistant_call("f", "Bash", json!({})),
        call_result("f", true, json!("first")),
        call_result("f", true, json!("second")),
        assistant_call("o", "Bash", json!({})),
        call_result("o", false, json!("ok")),
    ]);
    assert_eq!(p[0].error_text, "first");
}

// ── lifecycle over sessions ──────────────────────────────────────────────────────────────────

/// Sessions are counted per NORMALISED key: K1 (digits and paths differ) in three sessions is
/// one active fact; K2 stays single-session; K3 has two; the same key under another tool is
/// another key; and a key repeated inside one session counts once.
#[test]
fn distinct_sessions_are_counted_per_normalised_key() {
    let sessions = [
        session_of("s1", "Bash", &["error 11 in /a/x.rs", "only here", "error 11 in /a/x.rs"]),
        session_of("s2", "Bash", &["error 22 in /b/y.rs"]),
        session_of("s3", "Bash", &["error 33 in /c/z.rs", "shared by two"]),
        session_of("s4", "Bash", &["shared by two"]),
        session_of("s5", "Edit", &["error 44 in /d/w.rs"]),
    ];
    let r = lesson_report(&sessions, &no_db());
    assert_eq!(r["distinct_keys"], 4, "{r}");
    assert_eq!(r["pairs_total"], 8);
    assert_eq!(r["keys_by_distinct_sessions"], json!({"1": 2, "2": 1, "3": 1, "4": 0, "5+": 0}));
    assert_eq!(r["lifecycle"]["k_promote"], 3);
    assert_eq!(r["lifecycle"]["active"], 1);
    assert_eq!(r["lifecycle"]["candidate"], 3);
    assert_eq!(r["lifecycle"]["single_session"], 2);
}

#[test]
fn five_or_more_sessions_share_the_last_bucket() {
    let sessions: Vec<_> = (0..6)
        .map(|i| session_of(&format!("s{i}"), "Bash", &["boom"]))
        .collect();
    let r = lesson_report(&sessions, &no_db());
    assert_eq!(r["keys_by_distinct_sessions"]["5+"], 1);
    assert_eq!(r["lifecycle"]["active"], 1);
}

/// With no sessions the share is null, not a division by zero shown as 0.
#[test]
fn an_empty_corpus_has_no_share() {
    let r = lesson_report(&[], &no_db());
    assert_eq!(r["distinct_keys"], 0);
    assert!(r["lifecycle"]["share_active"].is_null());
}

// ── the stock already in memory.db ───────────────────────────────────────────────────────────

fn make_db(dir: &std::path::Path, rows: &[(String, serde_json::Value)]) -> std::path::PathBuf {
    let path = dir.join("memory.db");
    let con = Connection::open(&path).expect("create");
    con.execute(
        "CREATE TABLE memory_entries (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )
    .expect("table");
    for (key, value) in rows {
        con.execute(
            "INSERT INTO memory_entries (key, value) VALUES (?1, ?2)",
            [key.as_str(), value.to_string().as_str()],
        )
        .expect("insert");
    }
    path
}

fn lesson_row(tool: &str, error: &str) -> (String, serde_json::Value) {
    let class = tool.to_lowercase();
    (
        format!("outcome:{class}:transcript-{}:failure", raw_hash8(tool, error)),
        json!({"tool": tool, "error": error, "resolution_input": "x", "session_id": "s", "timestamp": "t"}),
    )
}

/// The overlap with the lessons already stored is measured two ways: EXACTLY, by the hash the
/// miner put in the row key, and by the normalised key of the stored error text, which also
/// shows how many stored lessons are really one fact.
#[test]
fn the_stored_lessons_overlap_the_corpus_by_hash_and_by_normalised_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = make_db(
        dir.path(),
        &[
            lesson_row("Bash", "error 11 in /a/x.rs"),
            lesson_row("Bash", "error 22 in /b/y.rs"), // another hash, the same fact
            lesson_row("Bash", "something else entirely"),
            ("outcome:bash:not-mined:failure".to_string(), json!({"tool": "Bash", "error": "ignored"})),
            ("unrelated:key".to_string(), json!({})),
        ],
    );
    let stored = read_stored_lessons(&path).expect("readable");
    assert_eq!(stored.lessons, 3, "only the transcript-mined rows are lessons");

    let sessions = [
        session_of("s1", "Bash", &["error 11 in /a/x.rs"]),
        session_of("s2", "Bash", &["error 33 in /c/z.rs"]),
        session_of("s3", "Bash", &["error 44 in /d/w.rs"]),
    ];
    let r = lesson_report(&sessions, &Ok(stored));
    let db = &r["memory_db"];
    assert_eq!(db["available"], true);
    assert_eq!(db["lessons"], 3);
    assert_eq!(db["normalized_keys"], 2, "two stored lessons are one fact: {db}");
    assert_eq!(db["raw_overlap"]["corpus_raw_keys"], 3);
    assert_eq!(db["raw_overlap"]["in_db"], 1, "only the first corpus text was stored verbatim");
    assert_eq!(db["key_overlap"]["corpus_keys"], 1, "the three corpus errors are one fact: {r}");
    assert_eq!(db["key_overlap"]["in_db"], 1);
    assert_eq!(db["key_overlap"]["active_total"], 1);
    assert_eq!(db["key_overlap"]["active_in_db"], 1, "the 3-session fact is already stored");
}

/// Reading the stock must never touch the file: same bytes, no sibling files.
#[test]
fn reading_the_stock_does_not_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = make_db(dir.path(), &[lesson_row("Bash", "boom")]);
    let before = std::fs::read(&path).expect("read");
    let listing = |d: &std::path::Path| {
        let mut names: Vec<_> = std::fs::read_dir(d)
            .expect("ls")
            .flatten()
            .map(|e| e.file_name())
            .collect();
        names.sort();
        names
    };
    let files = listing(dir.path());
    assert_eq!(read_stored_lessons(&path).expect("readable").lessons, 1);
    assert_eq!(std::fs::read(&path).expect("read"), before);
    assert_eq!(listing(dir.path()), files, "no journal, no wal, no shm");
}

#[test]
fn a_missing_or_foreign_database_is_reported_not_counted_as_zero() {
    assert!(read_stored_lessons(std::path::Path::new("/nonexistent/memory.db")).is_err());
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("memory.db");
    Connection::open(&path)
        .expect("create")
        .execute("CREATE TABLE other (x TEXT)", [])
        .expect("table");
    let err = read_stored_lessons(&path).err().expect("no memory_entries table");
    assert!(err.contains("memory_entries"), "{err}");
    let r = lesson_report(&[], &Err(err));
    assert_eq!(r["memory_db"]["available"], false);
    assert!(r["memory_db"]["reason"].is_string());
    assert!(r["memory_db"]["lessons"].is_null(), "no count is not a 0 count");
}

/// Counts and numbers only: the error text of a lesson never reaches the payload.
#[test]
fn the_payload_never_contains_transcript_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = make_db(dir.path(), &[lesson_row("Bash", &format!("{CANARY} stored"))]);
    let sessions = [session_of("s1", "Bash", &[&format!("{CANARY} corpus")])];
    let r = lesson_report(&sessions, &read_stored_lessons(&path));
    assert!(!r.to_string().contains(CANARY), "{r}");
}

/// The IO shell: the transcripts of the project, newest first, and the project's own memory.db
/// (absent here, which is said, not zeroed).
#[test]
fn the_shell_walks_the_transcripts_and_reports_the_missing_database() {
    let root = std::path::Path::new("/home/x/proj");
    let home = tempfile::tempdir().expect("tempdir");
    let dir = crate::cli::context_budget::transcript_dir_for(home.path(), root);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for id in ["s1", "s2"] {
        let body = fail_then_ok("Bash", "1", "boom 7").join("\n");
        std::fs::write(dir.join(format!("{id}.jsonl")), body).expect("write");
    }
    let r = lesson_lifecycle_in(home.path(), root, &KPI_CAPS);
    assert_eq!(r["available"], true);
    assert_eq!(r["sessions_scanned"], 2);
    assert_eq!(r["distinct_keys"], 1);
    assert_eq!(r["keys_by_distinct_sessions"]["2"], 1);
    assert_eq!(r["memory_db"]["available"], false);
}

#[test]
fn an_absent_transcript_directory_is_displayed_not_zero() {
    let home = tempfile::tempdir().expect("tempdir");
    let r = lesson_lifecycle_in(home.path(), std::path::Path::new("/home/x/none"), &KPI_CAPS);
    assert_eq!(r["available"], false);
    assert!(r["reason"].as_str().is_some_and(|m| m.contains("no transcript directory")));
}
