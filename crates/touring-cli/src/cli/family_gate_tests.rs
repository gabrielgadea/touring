//! Tests of the `family_gate` shadow ruler (B2, 01/11/2026). The rule and the calibration are
//! exercised with explicit numbers, never with the calibrated constants, so recalibrating cannot
//! silently change what a test proves.

use super::*;
use crate::cli::context_budget::{ContextBudgetAggregate, aggregate_session_lines};
use crate::cli::transcript_scan::{KPI_CAPS, fixtures::*};
use serde_json::json;

/// A touring-suggest block asking for `touring index …` (target: the two words after `MUST`).
const SUGGEST: &str = "[TOURING SUGGEST]\n  MUST touring index find foo -j\n";
/// An evidence-guard block asking for `cargo check …`.
const GUARD: &str = "EVIDENCE GUARD\n  MUST cargo check -p x\n";
/// A gate's deny remedy: its MUST rides a call that was blocked.
const DENY: &str = "[G6 redundant]\n  MUST touring run --lang bash\n";
/// A marker planted in every transcript text the fixtures carry.
const CANARY: &str = "CANARIO-NAO-VAZAR";

fn bash(i: usize, cmd: &str) -> String {
    assistant_call(&format!("b{i}"), "Bash", json!({"command": cmd}))
}

fn session(lines: &[String]) -> SessionFamilies {
    let mut t = FamilyTracker::default();
    t.observe_lines(lines.iter().map(String::as_str));
    t.finish()
}

fn stats(emitted: u64, denied: u64, followed: u64) -> FamilyFollow {
    FamilyFollow {
        blocks: emitted,
        bytes: emitted * 100,
        emitted,
        denied,
        followed,
    }
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

/// A session of `n` MUST blocks of `block`, `followed` of which the next Bash call ran.
fn run_of(block: &str, n: usize, followed: usize, date: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for i in 0..n {
        let line = hook(block);
        lines.push(if i == 0 { stamped(&line, date) } else { line });
        let cmd = if i < followed { "touring index find x" } else { "ls" };
        lines.push(bash(i, cmd));
    }
    lines
}

fn rule(n_min: u64, tau: f64, tau_off: f64) -> Rule {
    Rule { n_min, tau, tau_off }
}

/// Each MUST is credited to the family of the block that EMITTED it — the question the global
/// ratio cannot answer. The follow is armed by the last MUST before a Bash call.
#[test]
fn each_must_is_attributed_to_the_family_of_the_block_that_emitted_it() {
    let s = session(&[
        hook(SUGGEST),
        bash(1, "touring index find x"),
        hook(GUARD),
        bash(2, "ls"),
        hook(DENY),
        bash(3, "touring run --lang bash"),
    ]);
    let f = &s.families;
    assert_eq!((f["touring-suggest"].emitted, f["touring-suggest"].followed), (1, 1));
    assert_eq!((f["evidence-guard"].emitted, f["evidence-guard"].followed), (1, 0));
    let deny = &f["code-mode-deny"];
    assert_eq!((deny.emitted, deny.denied, deny.followed), (1, 1, 0), "a deny never arms");
    assert_eq!(deny.sample(), 0);
}

/// One reading of "carries a MUST", one rule of "followed": over a transcript with a superseded
/// MUST, an unfollowed one, a deny and a trailing one, the per-family counts add up to the
/// global ones `context_budget` publishes.
#[test]
fn the_families_add_up_to_the_global_counts_of_context_budget() {
    let lines = [
        hook(SUGGEST),
        hook(SUGGEST), // supersedes the one above: emitted, never followed
        bash(1, "touring index find x"),
        hook(GUARD),
        bash(2, "ls"),
        hook(DENY),
        bash(3, "ls"),
        hook(SUGGEST), // pending at the end of the transcript
    ];
    let s = session(&lines);
    let mut agg = ContextBudgetAggregate::default();
    let mut seen = std::collections::BTreeMap::new();
    aggregate_session_lines(lines.iter().map(String::as_str), &mut agg, &mut seen, None);
    let sum = |f: fn(&FamilyFollow) -> u64| s.families.values().map(f).sum::<u64>();
    assert_eq!(sum(|f| f.emitted), agg.must_emitted);
    assert_eq!(sum(|f| f.denied), agg.must_denied);
    assert_eq!(sum(|f| f.followed), agg.must_followed);
    assert_eq!(agg.must_emitted, 5);
    assert_eq!(agg.must_followed, 1);
}

#[test]
fn bytes_blocks_and_tokens_follow_the_declared_conversion() {
    let s = session(&[hook(SUGGEST), hook(SUGGEST)]);
    let f = &s.families["touring-suggest"];
    assert_eq!(f.blocks, 2);
    assert_eq!(f.bytes, 2 * SUGGEST.len() as u64);
    let p = family_gate_report(&[SessionFamilies {
        date: Some("2026-11-02".into()),
        families: s.families.clone(),
    }]);
    let fam = &p["windows"]["application"]["families"]["touring-suggest"];
    assert!(num(&fam["injected_tokens_est"]) - f.bytes as f64 * 0.25 < 1e-9);
    assert_eq!(fam["injected_bytes"], f.bytes);
}

/// The first MUST-bearing record that carries a `timestamp` dates the session.
#[test]
fn a_session_is_dated_by_its_first_timestamp() {
    let s = session(&[
        user_text("no stamp here"),
        stamped(&hook(SUGGEST), "2026-10-15T12:00:00.000Z"),
        stamped(&hook(SUGGEST), "2026-11-03T08:00:00.000Z"),
    ]);
    assert_eq!(s.date.as_deref(), Some("2026-10-15"));
    assert_eq!(session(&[hook(SUGGEST)]).date, None, "no timestamp is not a date");
}

/// The declared rule, on explicit thresholds: n below the minimum, a family with nothing to
/// follow and a family that only rode denials are NOT MEASURED — absent signal is not zero.
#[test]
fn recommend_applies_the_declared_rule_and_never_scores_an_absent_signal() {
    let r = rule(20, 0.4, 0.2);
    let level = |s: &FamilyFollow| recommend(s, &r).level.map(Level::as_str);
    assert_eq!(level(&stats(100, 0, 50)), Some("full"));
    assert_eq!(level(&stats(100, 0, 40)), Some("full"), "exactly tau stays");
    assert_eq!(level(&stats(100, 0, 39)), Some("brief"));
    assert_eq!(level(&stats(100, 0, 20)), Some("brief"), "exactly tau_off stays brief");
    assert_eq!(level(&stats(100, 0, 19)), Some("off"));
    assert_eq!(level(&stats(100, 0, 0)), Some("off"));
    // denied MUSTs leave the denominator, as in the global ratio
    assert_eq!(recommend(&stats(120, 20, 50), &r).n, 100);

    let thin = recommend(&stats(19, 0, 0), &r);
    assert_eq!(thin.level, None);
    assert!(thin.reason.as_deref().is_some_and(|m| m.contains("n=19")), "{thin:?}");
    assert_eq!(thin.follow_ratio, None, "no ratio is published below the minimum n");

    let nothing = recommend(&FamilyFollow { blocks: 40, bytes: 4000, ..FamilyFollow::default() }, &r);
    assert_eq!(nothing.level, None);
    assert!(nothing.reason.as_deref().is_some_and(|m| m.contains("no MUST")), "{nothing:?}");

    let denied = recommend(&stats(50, 50, 0), &r);
    assert_eq!(denied.level, None);
    assert!(denied.reason.as_deref().is_some_and(|m| m.contains("denied")), "{denied:?}");
}

/// The thresholds are declared constants; whatever their calibrated values, they must be
/// coherent with the global ruler's own floor of evidence.
#[test]
fn the_declared_constants_are_coherent() {
    assert!(RULE.tau_off < RULE.tau && RULE.tau < 1.0, "{RULE:?}");
    assert!(RULE.n_min >= touring_foundation::cila::ADHERENCE_MIN_SAMPLE);
    assert!(PRECISION_MIN > 0.5 && PRECISION_MIN <= 1.0);
    assert_eq!(TAU_GRID.len() * N_GRID.len(), 55);
}

/// Windows are split by the date the session started: before the application start it
/// calibrates, from it on it tests. A session with no date is counted apart, in neither.
#[test]
fn sessions_split_into_calibration_and_application_windows_by_date() {
    let fam = |d: Option<&str>| SessionFamilies {
        date: d.map(str::to_string),
        families: [("touring-suggest".to_string(), stats(30, 0, 15))].into(),
    };
    let p = family_gate_report(&[
        fam(Some("2026-10-02")),
        fam(Some("2026-10-31")),
        fam(Some(APPLICATION_START)),
        fam(None),
    ]);
    assert_eq!(p["windows"]["calibration"]["sessions"], 2);
    assert_eq!(p["windows"]["application"]["sessions"], 1);
    assert_eq!(p["undated_sessions"], 1);
    assert_eq!(p["windows"]["calibration"]["before"], APPLICATION_START);
    assert_eq!(p["windows"]["application"]["from"], APPLICATION_START);
    assert_eq!(p["windows"]["calibration"]["families"]["touring-suggest"]["must_emitted"], 60);
}

/// The payload per family carries n, the measured ratio with its interval, the tokens and the
/// recommendation — in both windows, side by side.
#[test]
fn the_payload_shows_the_recommendation_per_family_in_both_windows() {
    let mut lines = run_of(SUGGEST, 200, 150, "2026-10-10T10:00:00Z");
    lines.extend(run_of(GUARD, 200, 0, "2026-10-10T10:00:01Z"));
    let oct = session(&lines);
    let mut lines = run_of(SUGGEST, 200, 150, "2026-11-02T10:00:00Z");
    lines.extend(run_of(GUARD, 200, 0, "2026-11-02T10:00:01Z"));
    let nov = session(&lines);
    let p = family_gate_report(&[oct, nov]);
    for window in ["calibration", "application"] {
        let fams = &p["windows"][window]["families"];
        let s = &fams["touring-suggest"];
        assert_eq!(s["status"], "measured", "{window}");
        assert_eq!(s["n"], 200);
        assert_eq!(s["followed"], 150);
        assert!(num(&s["follow_ratio"]) - 0.75 < 1e-9);
        assert_eq!(s["recommended_level"], "full");
        let ci = s["follow_ratio_ci95"].as_array().expect("interval");
        assert!(num(&ci[0]) < 0.75 && 0.75 < num(&ci[1]), "{ci:?}");
        assert_eq!(fams["evidence-guard"]["recommended_level"], "off", "0 of 200 followed");
    }
    assert!(p["rule"]["levels"] == json!(["full", "brief", "off"]), "{}", p["rule"]);
    assert_eq!(p["rule"]["n_min"], RULE.n_min);
    assert_eq!(p["mode"], "shadow");
}

/// A family with nothing attributable is displayed as not measured WITH the reason, and its
/// recommendation is null — not `full`, not `off`.
#[test]
fn a_family_that_cannot_be_measured_says_why() {
    let p = family_gate_report(&[session(&[
        stamped(&hook("[PORTFOLIO · x]\n  a fact with no directive"), "2026-11-02T00:00:00Z"),
        stamped(&hook(DENY), "2026-11-02T00:00:01Z"),
        bash(1, "ls"),
    ])]);
    let fams = &p["windows"]["application"]["families"];
    let portfolio = &fams["plugin-hook"];
    assert_eq!(portfolio["status"], "not_measured");
    assert!(portfolio["recommended_level"].is_null());
    assert!(portfolio["reason"].as_str().is_some_and(|r| r.contains("no MUST")), "{portfolio}");
    assert!(fams["code-mode-deny"]["reason"].as_str().is_some_and(|r| r.contains("denied")));
    assert!(portfolio["follow_ratio"].is_null(), "no signal is not a 0 ratio");
}

/// Temporal validation: a flag is kept only if it persists OUT of sample. B is below tau in the
/// calibration window but above it in the application window, so no grid point that flags B is
/// eligible; A persists, so the choice flags A alone, with the most evidence (largest n) and
/// the least intrusive threshold (smallest tau) among the ties.
#[test]
fn calibrate_keeps_only_the_flags_that_persist_out_of_sample() {
    let cal = [
        ("a".to_string(), FamilyFollow { bytes: 1_000, ..stats(200, 0, 20) }),
        ("b".to_string(), FamilyFollow { bytes: 5_000, ..stats(200, 0, 90) }),
    ]
    .into();
    let app = [
        ("a".to_string(), stats(150, 0, 15)),
        ("b".to_string(), stats(150, 0, 90)),
    ]
    .into();
    let c = calibrate(&cal, &app);
    assert_eq!(c.rows.len(), 55);
    let chosen = c.chosen.expect("a choice exists");
    assert_eq!(chosen.n_min, 100);
    assert!((chosen.tau - 0.15).abs() < 1e-9, "{chosen:?}");
    assert_eq!(chosen.flagged, 1);
    assert_eq!(chosen.covered_bytes, 1_000);
    assert_eq!((chosen.confirmed, chosen.reversed, chosen.unobserved), (1, 0, 0));
    assert!(
        c.rows
            .iter()
            .filter(|r| r.tau >= 0.5 && r.n_min <= 100)
            .all(|r| !r.eligible && r.reversed == 1),
        "every point that flags B and can observe it is refuted by the application window"
    );
    assert!(
        c.rows
            .iter()
            .filter(|r| r.n_min == 200)
            .all(|r| !r.eligible && r.unobserved == r.flagged),
        "the application window has only 150 followable MUSTs: n_min 200 observes nothing"
    );
}

/// No persistence, no choice: when every flag reverses (or none can be observed) the
/// procedure says so instead of picking a threshold.
#[test]
fn calibrate_declines_to_choose_without_out_of_sample_evidence() {
    let cal = [("a".to_string(), stats(200, 0, 20))].into();
    let reversed = [("a".to_string(), stats(150, 0, 135))].into();
    assert!(calibrate(&cal, &reversed).chosen.is_none());
    let silent = std::collections::BTreeMap::new();
    let c = calibrate(&cal, &silent);
    assert!(c.chosen.is_none());
    assert!(c.rows.iter().any(|r| r.unobserved > 0 && r.precision.is_none()));
}

/// Counts, family names and numbers — never a byte of the transcript.
#[test]
fn the_payload_never_contains_transcript_text() {
    let secret = format!("[TOURING SUGGEST]\n  MUST touring index {CANARY}\n");
    let p = family_gate_report(&[session(&[
        stamped(&hook(&secret), "2026-11-02T00:00:00Z"),
        bash(1, &format!("touring index {CANARY}")),
    ])]);
    assert!(!p.to_string().contains(CANARY), "{p}");
}

/// The IO shell: two sessions on either side of the application start, newest first, with the
/// scan's own limits shown.
#[test]
fn the_shell_reads_both_windows_from_the_transcripts() {
    let root = std::path::Path::new("/home/x/proj");
    let home = tempfile::tempdir().expect("tempdir");
    let dir = crate::cli::context_budget::transcript_dir_for(home.path(), root);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for (id, date) in [("s-oct", "2026-10-20T10:00:00Z"), ("s-nov", "2026-11-02T10:00:00Z")] {
        let body = run_of(SUGGEST, 4, 2, date).join("\n");
        std::fs::write(dir.join(format!("{id}.jsonl")), body).expect("write");
    }
    let p = family_gate_in(home.path(), root, &KPI_CAPS);
    assert_eq!(p["available"], true);
    assert_eq!(p["sessions_scanned"], 2);
    assert_eq!(p["windows"]["calibration"]["sessions"], 1);
    assert_eq!(p["windows"]["application"]["sessions"], 1);
    assert_eq!(p["windows"]["application"]["families"]["touring-suggest"]["must_emitted"], 4);
}

#[test]
fn an_absent_transcript_directory_is_displayed_not_zero() {
    let home = tempfile::tempdir().expect("tempdir");
    let p = family_gate_in(home.path(), std::path::Path::new("/home/x/none"), &KPI_CAPS);
    assert_eq!(p["available"], false);
    assert!(p["reason"].as_str().is_some_and(|r| r.contains("no transcript directory")));
}
