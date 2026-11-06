//! Contract: every test of the OLD lifecycle suite is still covered by something that exists.
//!
//! `old_dispatch_tests.tsv` freezes the 1.247 tests the suite had on 02/11/2026, before the
//! handler-surface rewrite (`surface_tests.rs`), one line per (old test, new home):
//!
//! | disposition | `new_home` | what must still be true |
//! |---|---|---|
//! | `kept` | the file that still holds the test | a `#[test] fn <name>` exists in that file |
//! | `checks` | comma-separated check ids | every id exists in `lifecycle_surface_checks.jsonl` |
//! | `leaf` | the file whose `mod tests` holds it | a `#[test] fn <name>` exists there, and the note says why no `handle_*` path reaches the behavior |
//! | `removed` | `covered_by:<check id>` | the note gives the reason, and the check that covers it exists |
//!
//! Deleting a test, renaming it, or dropping a check an old test was folded into turns one of these
//! red, so coverage cannot erode silently; `OLD_TOTAL` stops the table itself from shrinking.

use super::surface_tests::{checks, s};
use std::collections::BTreeSet;

const MAPPING: &str = include_str!("../../tests/fixtures/old_dispatch_tests.tsv");

/// Distinct tests in the old suite (`lifecycle/tests.rs` 1.208 + `lifecycle/e2e_tests.rs` 39).
const OLD_TOTAL: usize = 1247;

/// The source files a mapping line may point at.
fn source(file: &str) -> &'static str {
    match file {
        "lifecycle/tests.rs" => include_str!("tests.rs"),
        "lifecycle/e2e_tests.rs" => include_str!("e2e_tests.rs"),
        "lifecycle/task_get.rs" => include_str!("task_get.rs"),
        "lifecycle/task_list.rs" => include_str!("task_list.rs"),
        "lifecycle/task_update.rs" => include_str!("task_update.rs"),
        "lifecycle/plan_mode/enter.rs" => include_str!("plan_mode/enter.rs"),
        "lifecycle/plan_mode/exit.rs" => include_str!("plan_mode/exit.rs"),
        other => panic!("mapping points at a file this contract does not read: {other}"),
    }
}

/// True when `fn <name>(` is declared with `#[test]` among the attribute lines right above it.
fn declares_test(file: &str, name: &str) -> bool {
    let text = source(file);
    let needle = format!("fn {name}(");
    text.match_indices(&needle).any(|(at, _)| {
        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        let prefix = text[line_start..at].trim();
        if !matches!(prefix, "" | "pub" | "pub(crate)" | "async") {
            return false;
        }
        text[..line_start]
            .lines()
            .rev()
            .take_while(|l| l.trim().starts_with("#["))
            .any(|l| l.trim() == "#[test]")
    })
}

struct Line {
    test: &'static str,
    file: &'static str,
    disposition: &'static str,
    home: &'static str,
    note: &'static str,
}

fn lines() -> Vec<Line> {
    MAPPING
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&'static str> = l.split('\t').collect();
            assert_eq!(c.len(), 6, "malformed mapping line: {l}");
            Line {
                test: c[0],
                file: c[1],
                disposition: c[3],
                home: c[4],
                note: c[5],
            }
        })
        .collect()
}

/// What is wrong with one mapping line (empty when its home still exists).
fn problems(l: &Line, check_ids: &BTreeSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    match l.disposition {
        "kept" => {
            if !declares_test(l.home, l.test) {
                out.push(format!("{}: no #[test] fn left in {}", l.test, l.home));
            }
        }
        "leaf" => {
            if !declares_test(l.home, l.test) {
                out.push(format!(
                    "{}: leaf unit test missing from {}",
                    l.test, l.home
                ));
            }
            if l.note.len() < 20 {
                out.push(format!(
                    "{}: a leaf unit test must say why no handle_* path reaches it",
                    l.test
                ));
            }
        }
        "checks" => {
            out.extend(
                l.home
                    .split(',')
                    .filter(|id| !check_ids.contains(id))
                    .map(|id| format!("{}: check {id} no longer exists", l.test)),
            );
        }
        "removed" => {
            let covered = l.home.strip_prefix("covered_by:").unwrap_or("");
            if !check_ids.contains(covered) {
                out.push(format!(
                    "{}: covered_by check {covered:?} no longer exists",
                    l.test
                ));
            }
            if l.note.len() < 20 {
                out.push(format!("{}: a removed test must carry its reason", l.test));
            }
        }
        other => out.push(format!("{}: unknown disposition {other:?}", l.test)),
    }
    out
}

#[test]
fn the_mapping_still_lists_every_old_test() {
    let distinct: BTreeSet<(&str, &str)> = lines().iter().map(|l| (l.file, l.test)).collect();
    assert_eq!(
        distinct.len(),
        OLD_TOTAL,
        "old_dispatch_tests.tsv must keep accounting for all {OLD_TOTAL} old tests"
    );
}

#[test]
fn every_old_test_still_has_a_home_that_exists() {
    let check_ids: BTreeSet<&str> = checks().iter().map(|c| s(c, "id")).collect();
    let lost: Vec<String> = lines()
        .iter()
        .flat_map(|l| problems(l, &check_ids))
        .collect();
    assert!(
        lost.is_empty(),
        "old tests that lost their home:\n{}",
        lost.join("\n")
    );
}

#[test]
fn the_contract_notices_a_lost_home() {
    // Mutation-style self-check: a mapping line whose home is gone must be reported.
    let ids: BTreeSet<&str> = ["present_check"].into_iter().collect();
    let gone = Line {
        test: "t",
        file: "lifecycle/tests.rs",
        disposition: "checks",
        home: "absent_check",
        note: "",
    };
    assert_eq!(problems(&gone, &ids).len(), 1);
    let kept = Line {
        test: "no_such_test_anywhere",
        file: "lifecycle/tests.rs",
        disposition: "kept",
        home: "lifecycle/tests.rs",
        note: "",
    };
    assert_eq!(problems(&kept, &ids).len(), 1);
    let ok = Line {
        test: "t",
        file: "lifecycle/tests.rs",
        disposition: "checks",
        home: "present_check",
        note: "",
    };
    assert!(problems(&ok, &ids).is_empty());
}
