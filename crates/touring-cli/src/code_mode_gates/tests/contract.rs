//! Contracts that keep the proofs honest:
//!
//! * the windows `spec` declares are the ones the ledgers enforce (a window edited in one place
//!   and not the other changes the gate without changing its documentation, or the reverse);
//! * every test of the old suite is accounted for — a row of the table, a unit test, an arm
//!   test, a precedence row, or a suggester test that stayed — so no asserted scenario is lost;
//! * the committed replay fixtures carry no user path (the repository is public).

use super::rows::scenarios;
use crate::code_mode_gates::GateState;
use crate::code_mode_gates::spec::*;
use std::collections::BTreeSet;
use std::time::Duration;

fn secs(n: u64) -> Option<Duration> {
    Some(Duration::from_secs(n))
}

/// `(ledger, capacity, window)` as the store builds it, against the constants `spec` names.
#[test]
fn the_declared_windows_are_the_ones_the_ledgers_enforce() {
    let st = GateState::new();
    macro_rules! pin {
        ($field:ident, $cap:expr, $ttl:expr) => {{
            let p = st.$field.policy();
            assert_eq!(
                p.max_capacity(),
                Some($cap),
                concat!(stringify!($field), ": capacity")
            );
            assert_eq!(
                p.time_to_live(),
                $ttl,
                concat!(stringify!($field), ": window")
            );
        }};
    }
    pin!(g6_seen, CACHE_MAX_CAPACITY, secs(SUGGESTION_TTL_SECS));
    pin!(mutation_epoch, CONFORMAL_TAU_MAX_PROJECTS, None);
    pin!(
        pending_g9,
        SESSION_LEDGER_CAPACITY,
        secs(PENDING_DENY_TTL_SECS)
    );
    pin!(
        pending_g2,
        SESSION_LEDGER_CAPACITY,
        secs(PENDING_DENY_TTL_SECS)
    );
    pin!(
        burst_ledger,
        CACHE_MAX_CAPACITY,
        secs(CODE_MODE_WINDOW_SECS)
    );
    pin!(
        pending_g1,
        SESSION_LEDGER_CAPACITY,
        secs(PENDING_DENY_TTL_SECS)
    );
    pin!(
        g3_streak,
        SESSION_LEDGER_CAPACITY,
        secs(SESSION_STATE_TTL_SECS)
    );
    pin!(
        g3_read_files,
        CACHE_MAX_CAPACITY,
        secs(SESSION_STATE_TTL_SECS)
    );
    pin!(g7_seen, CACHE_MAX_CAPACITY, secs(SESSION_STATE_TTL_SECS));
    pin!(g7_stamps, CACHE_MAX_CAPACITY, secs(SESSION_STATE_TTL_SECS));
    pin!(
        m3_distinct_reads,
        SESSION_LEDGER_CAPACITY,
        secs(SESSION_STATE_TTL_SECS)
    );
    pin!(
        g4_last_locate,
        SESSION_LEDGER_CAPACITY,
        secs(G4_LOCATE_TTL_SECS)
    );
    pin!(
        g5_edit_streak,
        SESSION_LEDGER_CAPACITY,
        secs(SESSION_STATE_TTL_SECS)
    );
    pin!(
        written_scripts_ledger,
        CACHE_MAX_CAPACITY,
        secs(WRITE_RUN_WINDOW_SECS)
    );
    pin!(
        write_run_pair_ledger,
        CACHE_MAX_CAPACITY,
        secs(WRITE_RUN_WINDOW_SECS)
    );
    pin!(
        exec_burst_ledger,
        CACHE_MAX_CAPACITY,
        secs(EXEC_BURST_WINDOW_SECS)
    );
    pin!(
        inspect_burst_ledger,
        CACHE_MAX_CAPACITY,
        secs(INSPECT_BURST_WINDOW_SECS)
    );
    pin!(
        pending_g10,
        SESSION_LEDGER_CAPACITY,
        secs(PENDING_DENY_TTL_SECS)
    );
    pin!(bypass_ledger, BYPASS_LEDGER_CAPACITY, secs(G11_WINDOW_SECS));
}

/// The numbers CLAUDE.md §10 documents, spelled once more where a change cannot slip by.
#[test]
fn the_documented_windows_and_thresholds_are_these() {
    assert_eq!(INSPECT_BURST_WINDOW_SECS, 300, "inspection burst window");
    assert_eq!(INSPECT_BURST_ADVISE_AT, 2, "inspection burst: the 2nd");
    assert_eq!(G1_DENY_AT, 3, "G1: the 3rd");
    assert_eq!(EXEC_BURST_WINDOW_SECS, 600, "G10 window");
    assert_eq!(EXEC_BURST_DENY_AT, 5, "G10: the 5th");
    assert_eq!(WRITE_RUN_WINDOW_SECS, 600, "write→run window");
    assert_eq!(WRITE_RUN_DENY_AT, 2, "write→run: the 2nd");
    assert_eq!(G11_WINDOW_SECS, 600, "G11 window");
    assert_eq!(
        G11_MAX_STRICT_RUN, 1,
        "G11: a second adjacent bypass is the deny"
    );
    assert_eq!(
        G11_MAX_SINCE_ROUTE, 2,
        "G11: a third bypass without the route is the deny"
    );
    assert_eq!(M3_DELEGATION_AT, 10, "M3: the 10th distinct file");
    assert_eq!(SUGGESTION_TTL_SECS, 300, "G6 window");
    assert_eq!(CODE_MODE_WINDOW_SECS, 180, "G1 burst window");
    assert_eq!(
        PENDING_DENY_TTL_SECS, 60,
        "a deny waits 60 s for its follow-through"
    );
    assert_eq!(
        SESSION_STATE_TTL_SECS, 3600,
        "G3 / G5 / G7 / M3 session memory"
    );
    assert_eq!(G4_LOCATE_TTL_SECS, 120, "G4 locate window");
    assert_eq!(
        G1_SIMILARITY_AT, 0.5,
        "G1: similarity that fires on the 2nd"
    );
    assert_eq!(G1_DEMOTE_FLOOR, 0.70, "G1 demotion precision floor");
    assert_eq!(
        G1_DEMOTE_MIN_EVENTS, 100,
        "G1 demotion needs this many events"
    );
}

struct OldTest<'a> {
    name: &'a str,
    home: &'a str,
    detail: &'a str,
}

fn old_tests(tsv: &str) -> Vec<OldTest<'_>> {
    tsv.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            OldTest {
                name: c[0],
                home: c[3],
                detail: c.get(4).copied().unwrap_or(""),
            }
        })
        .collect()
}

fn defines(src: &str, name: &str) -> bool {
    src.contains(&format!("fn {name}(")) || src.contains(&format!("fn {name}<"))
}

const SOURCES: [(&str, &str); 7] = [
    ("tests/unit.rs", include_str!("unit.rs")),
    ("tests/arm_unit.rs", include_str!("arm_unit.rs")),
    ("tests/replay.rs", include_str!("replay.rs")),
    ("tests/modes.rs", include_str!("modes.rs")),
    (
        "cli_suggester_tests.rs",
        include_str!("../../cli_suggester_tests.rs"),
    ),
    ("cli_suggester.rs", include_str!("../../cli_suggester.rs")),
    ("g10_route.rs", include_str!("../g10_route.rs")),
];

/// The old test is a `covers` entry of every row the map names, and every named row exists.
fn covered_by_rows(t: &OldTest<'_>, rows: &[super::scenarios::Scn]) -> bool {
    !t.detail.is_empty()
        && t.detail.split(',').all(|scn| {
            rows.iter()
                .any(|r| r.name == scn && r.covers.contains(&t.name))
        })
}

/// The function the map names is defined in the file it names (`unit`, `arm`, `suggester`, `g10`).
fn defined_in_home(t: &OldTest<'_>) -> bool {
    let wanted = |file: &str| match t.home {
        "unit" => file == "tests/unit.rs",
        "arm" => file == "tests/arm_unit.rs",
        "g10" => file == "g10_route.rs",
        _ => file.starts_with("cli_suggester"),
    };
    SOURCES
        .iter()
        .any(|(file, src)| wanted(file) && defines(src, t.name))
}

/// `file::function` in the detail column (`mode`, `replay`).
fn defined_where_detail_says(t: &OldTest<'_>) -> bool {
    let (file, func) = t.detail.split_once("::").unwrap_or((t.detail, t.name));
    SOURCES
        .iter()
        .any(|(f, src)| *f == file && defines(src, func))
}

fn accounted_for(t: &OldTest<'_>, rows: &[super::scenarios::Scn]) -> bool {
    match t.home {
        "row" => covered_by_rows(t, rows),
        "unit" | "arm" | "suggester" | "g10" => defined_in_home(t),
        "mode" | "replay" => defined_where_detail_says(t),
        _ => false,
    }
}

/// Every test of the old suite maps to something that still asserts it.
#[test]
fn every_old_gate_test_is_accounted_for() {
    const TSV: &str = include_str!("../../../tests/fixtures/old_gate_tests.tsv");
    let rows = scenarios();
    let tests = old_tests(TSV);
    assert_eq!(
        tests.len(),
        317,
        "the old suite had 317 test functions (311 in the suggester's three files, 6 in g10_route)"
    );
    let lost: Vec<String> = tests
        .iter()
        .filter(|t| !accounted_for(t, &rows))
        .map(|t| format!("{} -> {} {}", t.name, t.home, t.detail))
        .collect();
    assert!(
        lost.is_empty(),
        "old tests no longer asserted anywhere: {lost:#?}"
    );
}

/// A row that claims to cover an old test must claim one the map knows.
#[test]
fn every_covered_name_in_the_table_is_an_old_test() {
    const TSV: &str = include_str!("../../../tests/fixtures/old_gate_tests.tsv");
    let known: BTreeSet<&str> = old_tests(TSV).iter().map(|t| t.name).collect();
    let mut stray = Vec::new();
    for r in scenarios() {
        for c in r.covers {
            if !known.contains(c) {
                stray.push(format!("{} covers {c}", r.name));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "rows covering unknown old tests: {stray:#?}"
    );
}

/// The repository is public: the committed fixtures hold tool names, timings, verdicts and
/// gate ids over synthetic roots — never a user's home directory.
#[test]
fn the_replay_fixtures_carry_no_user_path() {
    for (what, text) in [
        (
            "gate_replay_corpus.json",
            include_str!("../../../tests/fixtures/gate_replay_corpus.json"),
        ),
        (
            "old_gate_tests.tsv",
            include_str!("../../../tests/fixtures/old_gate_tests.tsv"),
        ),
    ] {
        for (at, _) in text.match_indices("/home/") {
            assert!(
                text[at..].starts_with("/home/u/"),
                "{what}: a /home/ path that is not the synthetic /home/u/ (byte {at})"
            );
        }
        assert!(!text.contains("/Users/"), "{what}: a macOS home path");
    }
}
