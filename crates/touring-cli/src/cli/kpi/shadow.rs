//! r3 shadow KPI (S-10, plan `2026-10-02-jev-harness-r3`) — the live reading of the
//! shadow journal.
//!
//! The Python evaluator writes ONE `shadow_decision` line per (event × arm) to
//! `~/.claude/touring/logs/r3_shadow.jsonl` (closed schema:
//! `scripts/r3_event_schema.py`). This family reads that file over the same 7-day window
//! as the `decision` family and answers, per `use_case` and per arm: how many events and
//! decisions, how often the arm agrees with the live baseline B0 (`baseline_agrees`), how
//! many blocks it WOULD drop (`decision.would_drop`) and the arm latency p50/p95.
//!
//! The rule that runs through the `decision` family holds here too: **absence of data is
//! `unmeasured`, never `0`**. A `0` agreement would read as "the arm always disagrees with
//! B0" and a `0` count as "the shadow ran and saw nothing" — both are the SAME bytes as "the
//! evaluator does not run yet", which is exactly the state of a new KPI in its first hours.
//!
//! Journals are separate, by construction: this module reads `r3_shadow.jsonl` and nothing
//! else, never writes, and never touches the `decision` family's journals
//! (`intent_journal`, `recall_journal`, `jev_journal`) — `intent_decision_kpi_is_unaffected`
//! holds it to that.

use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Same 7-day window as the `decision` family.
const WINDOW_MS: u64 = 7 * 24 * 3_600_000;

/// The evaluator's single output file; the emitter's `r3_events.jsonl` sits next to it.
const SHADOW_JOURNAL_FILE: &str = "r3_shadow.jsonl";

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// `r3_shadow.jsonl` beside the events file — ONE definition of the logs directory (the
/// emitter's, including its `TOURING_R3_EVENTS_FILE` override), so the writer and the
/// reader of the shadow pair can never disagree about where it is.
fn journal_path() -> PathBuf {
    touring_hook_runtime::r3_shadow::events_path().with_file_name(SHADOW_JOURNAL_FILE)
}

/// What one slice of decision lines (a use case, or one arm of it) adds up to.
#[derive(Default)]
struct Tally {
    decisions: u64,
    agree: u64,
    disagree: u64,
    would_drop: u64,
    latencies: Vec<u64>,
}

impl Tally {
    fn add(&mut self, record: &Value) {
        self.decisions += 1;
        // `null` = B0 had no verdict to compare with: neither agreement nor disagreement.
        match record.get("baseline_agrees").and_then(Value::as_bool) {
            Some(true) => self.agree += 1,
            Some(false) => self.disagree += 1,
            None => {}
        }
        if record
            .pointer("/decision/would_drop")
            .and_then(Value::as_bool)
            == Some(true)
        {
            self.would_drop += 1;
        }
        if let Some(ms) = record.get("latency_ms").and_then(Value::as_u64) {
            self.latencies.push(ms);
        }
    }

    fn report(&self) -> Value {
        let compared = self.agree + self.disagree;
        let mut latencies = self.latencies.clone();
        latencies.sort_unstable();
        json!({
            "n_decisions": self.decisions,
            // No comparable line ⇒ null, never 0: "never agrees" is a measurement.
            "agreement": if compared == 0 {
                Value::Null
            } else {
                json!(round4(self.agree as f64 / compared as f64))
            },
            "agreement_n": compared,
            "would_drop": self.would_drop,
            "latency_ms": match (nearest_rank(&latencies, 0.50), nearest_rank(&latencies, 0.95)) {
                (Some(p50), Some(p95)) => json!({"p50": p50, "p95": p95, "n": latencies.len()}),
                _ => Value::Null,
            },
        })
    }
}

/// Everything one `use_case` accumulates.
#[derive(Default)]
struct UseCase {
    total: Tally,
    arms: BTreeMap<String, Tally>,
    /// The DISTINCT events evaluated: one event yields a line per arm.
    events: HashSet<(String, u64, String)>,
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// Nearest-rank percentile over an ascending slice: exact, integer, no interpolation (a
/// latency of 31 ms is a latency somebody measured; 31,4 never was).
fn nearest_rank(sorted: &[u64], q: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (q * sorted.len() as f64).ceil() as usize;
    sorted.get(rank.clamp(1, sorted.len()) - 1).copied()
}

/// `(use_case, arm, ts_ms)` of a line that IS a shadow decision; `None` for anything else.
fn admit(record: &Value) -> Option<(&str, &str, u64)> {
    if record.get("kind")?.as_str()? != "shadow_decision" {
        return None;
    }
    Some((
        record.get("use_case")?.as_str()?,
        record.get("arm")?.as_str()?,
        record.get("ts_ms")?.as_u64()?,
    ))
}

/// The event a decision refers to (`event_ref`), when it carries a well-formed one.
fn event_key(record: &Value) -> Option<(String, u64, String)> {
    let r = record.get("event_ref")?;
    Some((
        r.get("kind")?.as_str()?.to_string(),
        r.get("ts_ms")?.as_u64()?,
        r.get("session_hash")?.as_str()?.to_string(),
    ))
}

fn unmeasured(reason: &str, path: &Path, unreadable: u64, foreign: u64) -> Value {
    json!({
        "status": "unmeasured",
        "reason": reason,
        "journal": path.display().to_string(),
        "unreadable_lines": unreadable,
        "foreign_lines": foreign,
    })
}

/// The KPI over `path`, counting lines with `ts_ms >= cutoff_ms`.
fn shadow_at(path: &Path, cutoff_ms: u64) -> Value {
    let Ok(text) = std::fs::read_to_string(path) else {
        return unmeasured("no_journal", path, 0, 0);
    };
    let (mut unreadable, mut foreign) = (0u64, 0u64);
    let mut cases: BTreeMap<String, UseCase> = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        // A truncated line (a crash mid-write) must not hide the rest of the series.
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            unreadable += 1;
            continue;
        };
        let Some((use_case, arm, ts_ms)) = admit(&record) else {
            foreign += 1;
            continue;
        };
        if ts_ms < cutoff_ms {
            continue;
        }
        let case = cases.entry(use_case.to_string()).or_default();
        case.total.add(&record);
        case.arms.entry(arm.to_string()).or_default().add(&record);
        if let Some(key) = event_key(&record) {
            case.events.insert(key);
        }
    }
    if cases.is_empty() {
        return unmeasured("no_lines_in_window", path, unreadable, foreign);
    }
    let use_cases: serde_json::Map<String, Value> = cases
        .iter()
        .map(|(name, case)| {
            let mut out = case.total.report();
            out["n_events"] = json!(case.events.len());
            out["arms"] = Value::Object(
                case.arms
                    .iter()
                    .map(|(arm, tally)| (arm.clone(), tally.report()))
                    .collect(),
            );
            (name.clone(), out)
        })
        .collect();
    json!({
        "status": "measured",
        "window_days": WINDOW_MS / (24 * 3_600_000),
        "journal": path.display().to_string(),
        "unreadable_lines": unreadable,
        "foreign_lines": foreign,
        "use_cases": use_cases,
        // Lines the EMITTER refused or failed to write, since this process started — only
        // the emitting process (the daemon) can answer, and it resets on restart.
        "emitter_dropped_since_start": touring_hook_runtime::r3_shadow::dropped_event_count(),
    })
}

/// The `shadow` family of `touring kpi -j`.
pub(crate) fn shadow_family() -> Value {
    shadow_at(&journal_path(), now_ms().saturating_sub(WINDOW_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_790_000_000_000;

    /// One `shadow_decision` line as the evaluator writes it (contract §2).
    fn decision(
        use_case: &str,
        arm: &str,
        ts: u64,
        agrees: Value,
        would_drop: Option<bool>,
        latency: u64,
        event_ts: u64,
    ) -> Value {
        let mut d = json!({"followed": true, "p": 0.9});
        if let Some(w) = would_drop {
            d["would_drop"] = json!(w);
        }
        json!({
            "v": 1, "kind": "shadow_decision", "use_case": use_case, "ts_ms": ts, "arm": arm,
            "event_ref": {"kind": "a2_offer_closed", "ts_ms": event_ts, "session_hash": "9f86d081884c"},
            "decision": d, "baseline_agrees": agrees, "latency_ms": latency, "model": "laya-multilingual",
        })
    }

    fn write_journal(dir: &Path, lines: &[Value]) -> PathBuf {
        let path = dir.join(SHADOW_JOURNAL_FILE);
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&path, body).expect("write journal");
        path
    }

    #[test]
    fn empty_journal_is_unmeasured_not_zero() {
        let dir = tempfile::tempdir().expect("tempdir");
        // No file at all.
        let absent = shadow_at(&dir.path().join(SHADOW_JOURNAL_FILE), 0);
        assert_eq!(absent["status"], "unmeasured");
        assert_eq!(absent["reason"], "no_journal");
        // An empty file is a journal that exists and holds nothing.
        let path = write_journal(dir.path(), &[]);
        let empty = shadow_at(&path, 0);
        assert_eq!(empty["status"], "unmeasured");
        assert_eq!(empty["reason"], "no_lines_in_window");
        // The shape the plan demands: `unmeasured`, never a table of zeros.
        assert!(empty.get("use_cases").is_none());
        assert!(empty.get("emitter_dropped_since_start").is_none());
    }

    #[test]
    fn agreement_is_computed_per_use_case() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lines = vec![
            // a2_outcome / B-L: 3 agree, 1 disagree ⇒ 0.75; one null (no verdict) is not compared.
            decision("a2_outcome", "B-L", T0 + 1, json!(true), None, 10, T0),
            decision("a2_outcome", "B-L", T0 + 2, json!(true), None, 20, T0 + 1),
            decision("a2_outcome", "B-L", T0 + 3, json!(true), None, 30, T0 + 2),
            decision("a2_outcome", "B-L", T0 + 4, json!(false), None, 40, T0 + 3),
            decision("a2_outcome", "B-L", T0 + 5, Value::Null, None, 50, T0 + 4),
            // a2_outcome / B-J: 1 agree, 1 disagree ⇒ 0.5.
            decision("a2_outcome", "B-J", T0 + 6, json!(true), None, 100, T0),
            decision("a2_outcome", "B-J", T0 + 7, json!(false), None, 200, T0 + 1),
            // a1_relevance / B-D: no baseline to compare ⇒ agreement null, 2 would-drop of 3.
            decision(
                "a1_relevance",
                "B-D",
                T0 + 8,
                Value::Null,
                Some(true),
                1,
                T0 + 5,
            ),
            decision(
                "a1_relevance",
                "B-D",
                T0 + 9,
                Value::Null,
                Some(true),
                1,
                T0 + 6,
            ),
            decision(
                "a1_relevance",
                "B-D",
                T0 + 10,
                Value::Null,
                Some(false),
                1,
                T0 + 7,
            ),
        ];
        let path = write_journal(dir.path(), &lines);
        let out = shadow_at(&path, T0);
        assert_eq!(out["status"], "measured");

        let a2 = &out["use_cases"]["a2_outcome"];
        assert_eq!(a2["n_decisions"], 7);
        assert_eq!(a2["n_events"], 5, "events 0..4: B-J re-reads two of B-L's");
        assert_eq!(a2["agreement_n"], 6, "the null is not a comparison");
        assert_eq!(a2["agreement"], 0.6667, "4 of 6 across both arms");
        assert_eq!(a2["arms"]["B-L"]["agreement"], 0.75);
        assert_eq!(a2["arms"]["B-J"]["agreement"], 0.5);
        assert_eq!(a2["would_drop"], 0);

        let a1 = &out["use_cases"]["a1_relevance"];
        assert_eq!(a1["n_decisions"], 3);
        assert_eq!(a1["would_drop"], 2);
        assert!(
            a1["agreement"].is_null(),
            "nothing compared ⇒ null, never 0"
        );
        assert_eq!(a1["agreement_n"], 0);
        assert!(out["use_cases"].get("other").is_none());
    }

    #[test]
    fn latency_percentiles_are_nearest_rank() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lines: Vec<Value> = [40u64, 10, 30, 20]
            .iter()
            .enumerate()
            .map(|(i, ms)| {
                decision(
                    "a2_outcome",
                    "B-L",
                    T0 + i as u64,
                    json!(true),
                    None,
                    *ms,
                    T0 + i as u64,
                )
            })
            .collect();
        let out = shadow_at(&write_journal(dir.path(), &lines), T0);
        let lat = &out["use_cases"]["a2_outcome"]["latency_ms"];
        assert_eq!(lat["p50"], 20, "rank ceil(0.5·4) = 2");
        assert_eq!(lat["p95"], 40, "rank ceil(0.95·4) = 4");
        assert_eq!(lat["n"], 4);
        assert_eq!(nearest_rank(&[], 0.5), None);
        assert_eq!(nearest_rank(&[7], 0.95), Some(7));
    }

    #[test]
    fn window_keeps_only_recent_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lines = vec![
            decision("a2_outcome", "B-L", T0 - 1, json!(false), None, 1, T0),
            decision("a2_outcome", "B-L", T0, json!(true), None, 1, T0),
        ];
        let path = write_journal(dir.path(), &lines);
        let out = shadow_at(&path, T0);
        assert_eq!(
            out["use_cases"]["a2_outcome"]["n_decisions"], 1,
            "ts == cutoff is IN, older is out"
        );
        assert_eq!(out["use_cases"]["a2_outcome"]["agreement"], 1.0);
        // Everything older than the window ⇒ unmeasured, not an empty table.
        let old_only = shadow_at(&path, T0 + 1);
        assert_eq!(old_only["status"], "unmeasured");
        assert_eq!(old_only["reason"], "no_lines_in_window");
    }

    #[test]
    fn unreadable_and_foreign_lines_are_counted_never_hidden() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(SHADOW_JOURNAL_FILE);
        let good = decision("a2_outcome", "B-L", T0 + 1, json!(true), None, 5, T0);
        let foreign = json!({"v": 1, "kind": "a2_offer_closed", "ts_ms": T0 + 2});
        let no_ts = json!({"kind": "shadow_decision", "use_case": "a2_outcome", "arm": "B-L"});
        std::fs::write(&path, format!("{good}\n{{not json\n{foreign}\n{no_ts}\n\n"))
            .expect("write");
        let out = shadow_at(&path, T0);
        assert_eq!(
            out["status"], "measured",
            "bad lines never abort the reading"
        );
        assert_eq!(out["unreadable_lines"], 1);
        assert_eq!(
            out["foreign_lines"], 2,
            "an event line and an undatable decision"
        );
        assert_eq!(out["use_cases"]["a2_outcome"]["n_decisions"], 1);
        // Only garbage ⇒ unmeasured, and the garbage is still reported.
        let junk = dir.path().join("junk.jsonl");
        std::fs::write(&junk, "{oops\n").expect("write");
        let only_bad = shadow_at(&junk, 0);
        assert_eq!(only_bad["status"], "unmeasured");
        assert_eq!(only_bad["unreadable_lines"], 1);
    }

    #[test]
    fn intent_decision_kpi_is_unaffected() {
        let dir = tempfile::tempdir().expect("tempdir");
        // The `decision` family's journals sit in the SAME directory, with lines that look
        // exactly like the kind of thing the shadow reader must never count.
        let siblings = [
            "intent_journal.jsonl",
            "recall_journal.jsonl",
            "jev_journal.jsonl",
        ];
        let bodies: Vec<String> = siblings
            .iter()
            .map(|name| {
                let body = format!(
                    "{}\n",
                    json!({"ts_ms": T0 + 1, "intent": "code", "source": "keyword", "kind": "shadow_decision",
                           "use_case": "a2_outcome", "arm": "B-L", "latency_ms": 9})
                );
                std::fs::write(dir.path().join(name), &body).expect("write sibling");
                body
            })
            .collect();
        // No r3_shadow.jsonl ⇒ unmeasured, whatever the siblings hold.
        let none = shadow_at(&dir.path().join(SHADOW_JOURNAL_FILE), 0);
        assert_eq!(none["status"], "unmeasured");
        assert_eq!(none["reason"], "no_journal");
        // With the shadow journal present only ITS lines count…
        let path = write_journal(
            dir.path(),
            &[decision(
                "a2_outcome",
                "B-L",
                T0 + 1,
                json!(true),
                None,
                5,
                T0,
            )],
        );
        let out = shadow_at(&path, T0);
        assert_eq!(out["use_cases"]["a2_outcome"]["n_decisions"], 1);
        // …and the sibling journals are byte-identical afterwards: this reader never writes.
        for (name, body) in siblings.iter().zip(&bodies) {
            let now = std::fs::read_to_string(dir.path().join(name)).expect("read sibling");
            assert_eq!(&now, body, "{name} must be untouched");
        }
        assert!(
            !siblings.contains(&SHADOW_JOURNAL_FILE),
            "the shadow never shares a journal"
        );
    }

    /// S5 ("KPI visível") is one line inside `cli_kpi`, which needs a live runtime to run —
    /// so the registration is guarded at the source: the family reaches `touring kpi -j`
    /// under its own key, and the `decision` family is still there beside it.
    #[test]
    fn shadow_family_is_registered_in_cli_kpi_beside_decision() {
        let kpi_source = include_str!("../kpi.rs");
        assert!(kpi_source.contains(r#"out["shadow"] = shadow::shadow_family();"#));
        assert!(kpi_source.contains(r#"out["decision"] = decision::decision_family();"#));
        assert!(!kpi_source.contains(r#"out["decision"] = shadow::"#));
    }

    #[test]
    fn journal_sits_beside_the_events_file() {
        let events = touring_hook_runtime::r3_shadow::events_path();
        assert_eq!(journal_path().parent(), events.parent());
        assert_eq!(
            journal_path().file_name().and_then(|n| n.to_str()),
            Some(SHADOW_JOURNAL_FILE)
        );
    }
}
