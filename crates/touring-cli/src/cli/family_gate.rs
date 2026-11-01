//! B2 (01/11/2026) — the follow ruler per injection family, in SHADOW.
//!
//! `touring kpi` already publishes ONE `injection_follow_ratio` for the whole injection channel
//! (0,598 against a floor of 0,60). Nothing used it to decide anything: a family nobody follows
//! keeps being injected. The ADR-002 answer is a per-family gate; this module is its measurement
//! and its recommendation, and it CHANGES NOTHING — no policy is written, no level is set. The
//! shadow is the point: the recommendation is published next to the measurement so the A/B that
//! follows the AB34 window can be judged against what the rule said before the data.
//!
//! # What is measured
//!
//! The rule is the global one, attributed. A block that carries a `MUST <target>` arms a check;
//! the next `Bash` call either ran the target (the whole two-word target, else its head) or did
//! not. [`context_budget::must_request`](crate::cli::context_budget) and `must_followed_by` are
//! the ONE reading of both, shared with the global ratio, so the per-family counts add up to the
//! global ones (asserted by `the_families_add_up_to_the_global_counts_of_context_budget`). The
//! credit goes to the family of the block that emitted the MUST that armed the check. A MUST that
//! rode a gate's deny never arms (coercion, not persuasion) and leaves the denominator.
//!
//! Following is OBSERVED behaviour, never occurrence: TokenEconomizer's own FIRE_GATE counts a
//! fact as fired whenever it was exercised (`distiller.py:292`, `envfacts.py:508`), a rate of 1,0
//! by construction. Here a family with no `MUST` has nothing to follow and says so
//! (`not_measured`), as does a family whose every MUST was a denied one, as does a family below
//! the minimum n. Absent signal is not zero, and is never scored as a failure to follow.
//!
//! # The rule and its calibration
//!
//! `full` while the follow ratio stays at or above [`TAU`]; `brief` from [`TAU_OFF`] up to it;
//! `off` below it — only with at least [`N_MIN`] followable MUSTs. Both are declared constants
//! calibrated by TEMPORAL validation: [`calibrate`] reads the thresholds off the sessions that
//! started before [`APPLICATION_START`], and the recommendation is then reported on the sessions
//! from that date on, separately, in the same payload. A threshold is chosen only if the families
//! it flags in the calibration window STAY flagged out of sample (precision at least
//! [`PRECISION_MIN`]); among the eligible grid points the one that covers the most injected bytes
//! wins, with the most evidence (largest n) and the least intrusive threshold (smallest tau)
//! breaking ties. `calibration.constants_match_chosen` tells whether the constants below are what
//! the procedure derives from the data the payload was computed over.
//!
//! The levels are the ADR's (`full`, `brief`, `off`). The live policy's middle level is
//! `no-advisory`; mapping the two is a decision of the policy owner, not of this ruler.
//!
//! The payload carries counts, family names and numbers — never a byte of the transcript.

use crate::cli::context_budget::{
    attributed_family, hook_injections, must_followed_by, must_request, transcript_dir_for,
};
use crate::cli::context_cost::{TOKENS_PER_BYTE, merge, round_to, scan_meta};
use crate::cli::transcript_scan::{SHADOW_CAPS, ScanCaps, scan_dir, tool_calls};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Sessions that started on or after this date are the APPLICATION window (where the rule is
/// tested); those before it are the CALIBRATION window (where the thresholds are chosen).
pub const APPLICATION_START: &str = "2026-11-01";

/// Smallest share of a calibration flag that must persist out of sample for a grid point to be
/// eligible: of the flagged families observable in the application window (n at least `n_min`),
/// at least this fraction must still be below `tau` there.
pub const PRECISION_MIN: f64 = 0.8;

/// The `n_min` values the calibration tries — none below the global ruler's own minimum sample.
pub const N_GRID: [u64; 5] = [20, 30, 50, 100, 200];

/// The `tau` values the calibration tries.
pub const TAU_GRID: [f64; 11] = [
    0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50, 0.55, 0.60,
];

/// Minimum followable MUSTs (emitted and not denied) before a family's ratio is published and
/// judged. CALIBRATED on the October sessions (163, before [`APPLICATION_START`]) by
/// [`calibrate`] — see the module docs and `calibration` in the payload. 50 is the largest `n_min`
/// that still judges every family the October window flags (the smallest of them has n=68).
pub const N_MIN: u64 = 50;

/// A family followed less than this goes below `full`. CALIBRATED on the same run: every `tau`
/// between 0,52 and 0,82 flags the same three October families (follow ratios 0,10 / 0,49 /
/// 0,52), and the criterion takes the smallest. Evidence is thin by construction: of the three
/// flags only one family had n >= [`N_MIN`] in the 52 November sessions (1 confirmed, 0 reversed,
/// 2 unobserved), so the precision of 1,0 rests on a single family — re-run `calibrate` as the
/// application window grows.
pub const TAU: f64 = 0.55;

/// A family followed less than this goes to `off`: half of [`TAU`] by declaration.
pub const TAU_OFF: f64 = TAU / 2.0;

/// The ADR-002 level a family is recommended to run at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Inject as today.
    Full,
    /// Inject a reduced form.
    Brief,
    /// Do not inject.
    Off,
}

impl Level {
    /// The level's name in the payload.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Brief => "brief",
            Self::Off => "off",
        }
    }
}

/// The thresholds of the rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    /// Minimum followable MUSTs before a family is judged.
    pub n_min: u64,
    /// Below this follow ratio the family leaves `full`.
    pub tau: f64,
    /// Below this follow ratio the family goes `off`.
    pub tau_off: f64,
}

/// The declared rule.
pub const RULE: Rule = Rule {
    n_min: N_MIN,
    tau: TAU,
    tau_off: TAU_OFF,
};

impl Rule {
    fn level_for(&self, ratio: f64) -> Level {
        if ratio >= self.tau {
            Level::Full
        } else if ratio >= self.tau_off {
            Level::Brief
        } else {
            Level::Off
        }
    }
}

/// What one family injected and how it was followed, over some set of sessions.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FamilyFollow {
    /// Blocks the family injected.
    pub blocks: u64,
    /// Bytes it injected.
    pub bytes: u64,
    /// Blocks that carried a `MUST` (denied ones included, as in `must_emitted`).
    pub emitted: u64,
    /// Of `emitted`, the ones that rode a denied call.
    pub denied: u64,
    /// Of the followable ones, those the next Bash call ran.
    pub followed: u64,
}

impl FamilyFollow {
    /// The MUSTs a follow can be judged on: emitted and not denied (the global ratio's own
    /// denominator).
    #[must_use]
    pub fn sample(&self) -> u64 {
        self.emitted.saturating_sub(self.denied)
    }

    fn absorb(&mut self, o: &Self) {
        self.blocks += o.blocks;
        self.bytes += o.bytes;
        self.emitted += o.emitted;
        self.denied += o.denied;
        self.followed += o.followed;
    }
}

/// The judgement of one family under a rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// Followable MUSTs.
    pub n: u64,
    /// `followed / n`, published only when the family is measured.
    pub follow_ratio: Option<f64>,
    /// The recommended level, `None` when the family is not measured.
    pub level: Option<Level>,
    /// Why the family is not measured.
    pub reason: Option<String>,
}

fn not_measured(n: u64, reason: String) -> Verdict {
    Verdict {
        n,
        follow_ratio: None,
        level: None,
        reason: Some(reason),
    }
}

/// Apply `rule` to one family. A family that cannot be judged is `not_measured` with the reason,
/// never scored: no MUST to follow, every MUST denied, or n below the minimum.
#[must_use]
pub fn recommend(f: &FamilyFollow, rule: &Rule) -> Verdict {
    let n = f.sample();
    if f.emitted == 0 {
        return not_measured(
            0,
            "the family emits no MUST: nothing to follow, so no follow signal".to_string(),
        );
    }
    if n == 0 {
        return not_measured(
            0,
            format!(
                "every MUST of this family rode a denied call ({} denied): coercion, not persuasion — no follow signal",
                f.denied
            ),
        );
    }
    if n < rule.n_min {
        return not_measured(n, format!("n={n} < n_min={}", rule.n_min));
    }
    let ratio = f.followed as f64 / n as f64;
    Verdict {
        n,
        follow_ratio: Some(ratio),
        level: Some(rule.level_for(ratio)),
        reason: None,
    }
}

/// One session after the walk: the day it started and what each family did in it.
#[derive(Debug, Clone, Default)]
pub struct SessionFamilies {
    /// `YYYY-MM-DD` of the first record that carries a timestamp; `None` when none does.
    pub date: Option<String>,
    /// Per family.
    pub families: BTreeMap<String, FamilyFollow>,
}

/// `YYYY-MM-DD` when `ts` starts with one.
fn date_prefix(ts: &str) -> Option<&str> {
    let d = ts.get(..10)?;
    let shaped = d.bytes().enumerate().all(|(i, b)| {
        if i == 4 || i == 7 {
            b == b'-'
        } else {
            b.is_ascii_digit()
        }
    });
    shaped.then_some(d)
}

/// Walks one session's records, crediting every MUST to the family that emitted it.
#[derive(Debug, Default)]
pub struct FamilyTracker {
    families: BTreeMap<String, FamilyFollow>,
    /// The check the last non-denied MUST armed: `(target, family)`.
    pending: Option<(String, String)>,
    date: Option<String>,
}

impl FamilyTracker {
    /// Fold one transcript record in.
    pub fn observe(&mut self, rec: &Value) {
        if self.date.is_none()
            && let Some(d) = rec
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(date_prefix)
        {
            self.date = Some(d.to_string());
        }
        match rec.get("type").and_then(Value::as_str) {
            Some("attachment") => self.observe_injections(rec),
            Some("assistant") => self.observe_calls(rec),
            _ => {}
        }
    }

    /// [`observe`](Self::observe) over raw JSONL lines; a line that is not JSON is skipped.
    #[cfg(test)]
    pub fn observe_lines<'a>(&mut self, lines: impl Iterator<Item = &'a str>) {
        for line in lines {
            if let Ok(rec) = serde_json::from_str::<Value>(line) {
                self.observe(&rec);
            }
        }
    }

    fn observe_injections(&mut self, rec: &Value) {
        for inj in hook_injections(rec) {
            let family = attributed_family(&inj.text, inj.emitter.as_deref());
            let stats = self.families.entry(family.to_string()).or_default();
            stats.blocks += 1;
            stats.bytes += inj.text.len() as u64;
            let Some(req) = must_request(&inj.text, family) else {
                continue;
            };
            stats.emitted += 1;
            if req.denied {
                stats.denied += 1;
            } else {
                self.pending = Some((req.target, family.to_string()));
            }
        }
    }

    fn observe_calls(&mut self, rec: &Value) {
        for call in tool_calls(rec).filter(|c| c.name == "Bash") {
            let Some((target, family)) = self.pending.take() else {
                continue;
            };
            let cmd = call
                .input
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if must_followed_by(&target, cmd) {
                self.families.entry(family).or_default().followed += 1;
            }
        }
    }

    /// The session's result.
    #[must_use]
    pub fn finish(self) -> SessionFamilies {
        SessionFamilies {
            date: self.date,
            families: self.families,
        }
    }
}

/// Sum the sessions selected by `keep` (by date) into one window.
fn window<F: Fn(&str) -> bool>(
    sessions: &[SessionFamilies],
    keep: F,
) -> (u64, BTreeMap<String, FamilyFollow>) {
    let mut merged: BTreeMap<String, FamilyFollow> = BTreeMap::new();
    let mut n = 0;
    for s in sessions {
        if !s.date.as_deref().is_some_and(&keep) {
            continue;
        }
        n += 1;
        for (family, f) in &s.families {
            merged.entry(family.clone()).or_default().absorb(f);
        }
    }
    (n, merged)
}

/// Wilson 95% interval of `k` followed out of `n`.
fn wilson95(k: u64, n: u64) -> Option<(f64, f64)> {
    if n == 0 {
        return None;
    }
    let z = 1.96_f64;
    let n = n as f64;
    let p = k as f64 / n;
    let denom = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denom;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denom;
    Some(((centre - half).max(0.0), (centre + half).min(1.0)))
}

fn family_json(f: &FamilyFollow, rule: &Rule) -> Value {
    let v = recommend(f, rule);
    let measured = v.level.is_some();
    json!({
        "blocks": f.blocks,
        "injected_bytes": f.bytes,
        "injected_tokens_est": f.bytes as f64 * TOKENS_PER_BYTE,
        "must_emitted": f.emitted,
        "must_denied": f.denied,
        "n": v.n,
        "followed": f.followed,
        "follow_ratio": v.follow_ratio.map(|r| round_to(r, 6)),
        "follow_ratio_ci95": if measured {
            wilson95(f.followed, v.n).map(|(lo, hi)| json!([round_to(lo, 6), round_to(hi, 6)]))
        } else {
            None
        },
        "status": if measured { "measured" } else { "not_measured" },
        "recommended_level": v.level.map(Level::as_str),
        "reason": v.reason,
    })
}

fn window_json(
    sessions: u64,
    families: &BTreeMap<String, FamilyFollow>,
    bound: (&str, &str),
) -> Value {
    let by_family: serde_json::Map<String, Value> = families
        .iter()
        .map(|(name, f)| (name.clone(), family_json(f, &RULE)))
        .collect();
    json!({
        bound.0: bound.1,
        "sessions": sessions,
        "families": by_family,
    })
}

/// One point of the calibration grid, judged out of sample.
#[derive(Debug, Clone, PartialEq)]
pub struct GridRow {
    /// Minimum followable MUSTs.
    pub n_min: u64,
    /// The `tau` of this point.
    pub tau: f64,
    /// Calibration-window families it would flag (judged, ratio below `tau`).
    pub flagged: u64,
    /// Bytes those families injected in the calibration window.
    pub covered_bytes: u64,
    /// Flagged families still below `tau` in the application window.
    pub confirmed: u64,
    /// Flagged families at or above `tau` in the application window.
    pub reversed: u64,
    /// Flagged families the application window cannot judge (n below `n_min`).
    pub unobserved: u64,
    /// `confirmed / (confirmed + reversed)`; `None` when nothing could be observed.
    pub precision: Option<f64>,
    /// `precision >= PRECISION_MIN`.
    pub eligible: bool,
}

/// The whole grid and the point chosen (if any).
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// One row per `(n_min, tau)`.
    pub rows: Vec<GridRow>,
    /// The eligible row covering the most bytes, ties to the largest n then the smallest tau.
    pub chosen: Option<GridRow>,
}

/// Ratio of `f` when it can be judged at `n_min`.
fn judged_ratio(f: &FamilyFollow, n_min: u64) -> Option<f64> {
    let n = f.sample();
    (n > 0 && n >= n_min).then(|| f.followed as f64 / n as f64)
}

fn grid_row(
    cal: &BTreeMap<String, FamilyFollow>,
    app: &BTreeMap<String, FamilyFollow>,
    n_min: u64,
    tau: f64,
) -> GridRow {
    let mut row = GridRow {
        n_min,
        tau,
        flagged: 0,
        covered_bytes: 0,
        confirmed: 0,
        reversed: 0,
        unobserved: 0,
        precision: None,
        eligible: false,
    };
    for (family, f) in cal {
        if !judged_ratio(f, n_min).is_some_and(|r| r < tau) {
            continue;
        }
        row.flagged += 1;
        row.covered_bytes += f.bytes;
        match app.get(family).and_then(|a| judged_ratio(a, n_min)) {
            Some(r) if r < tau => row.confirmed += 1,
            Some(_) => row.reversed += 1,
            None => row.unobserved += 1,
        }
    }
    let observed = row.confirmed + row.reversed;
    row.precision = (observed > 0).then(|| row.confirmed as f64 / observed as f64);
    row.eligible = row.precision.is_some_and(|p| p >= PRECISION_MIN);
    row
}

/// Calibrate on `cal`, validate on `app`: every `(n_min, tau)` of the grid, and the choice.
#[must_use]
pub fn calibrate(
    cal: &BTreeMap<String, FamilyFollow>,
    app: &BTreeMap<String, FamilyFollow>,
) -> Calibration {
    let rows: Vec<GridRow> = N_GRID
        .iter()
        .flat_map(|&n| TAU_GRID.iter().map(move |&t| (n, t)))
        .map(|(n, t)| grid_row(cal, app, n, t))
        .collect();
    let chosen = rows
        .iter()
        .filter(|r| r.eligible)
        .max_by(|a, b| {
            a.covered_bytes
                .cmp(&b.covered_bytes)
                .then(a.n_min.cmp(&b.n_min))
                .then(b.tau.total_cmp(&a.tau))
        })
        .cloned();
    Calibration { rows, chosen }
}

fn row_json(r: &GridRow) -> Value {
    json!({
        "n_min": r.n_min, "tau": r.tau, "flagged": r.flagged, "covered_bytes": r.covered_bytes,
        "confirmed": r.confirmed, "reversed": r.reversed, "unobserved": r.unobserved,
        "precision": r.precision.map(|p| round_to(p, 4)), "eligible": r.eligible,
    })
}

fn calibration_json(cal: &BTreeMap<String, FamilyFollow>, app: &BTreeMap<String, FamilyFollow>) -> Value {
    let c = calibrate(cal, app);
    let matches = c
        .chosen
        .as_ref()
        .map(|r| r.n_min == RULE.n_min && (r.tau - RULE.tau).abs() < 1e-9);
    json!({
        "grid": {"n_min": N_GRID, "tau": TAU_GRID},
        "rows": c.rows.len(),
        "eligible_rows": c.rows.iter().filter(|r| r.eligible).count(),
        "chosen": c.chosen.as_ref().map(row_json),
        "constants_match_chosen": matches,
        "criterion": "among the grid points whose calibration-window flags persist out of sample (precision >= precision_min), the one covering the most injected bytes; ties to the largest n_min, then the smallest tau",
    })
}

fn rule_json() -> Value {
    json!({
        "n_min": RULE.n_min,
        "tau": RULE.tau,
        "tau_off": RULE.tau_off,
        "precision_min": PRECISION_MIN,
        "levels": ["full", "brief", "off"],
        "method": "full while follow_ratio >= tau; brief from tau_off up to tau; off below; only with n (followable MUSTs) >= n_min; otherwise not_measured with the reason",
    })
}

/// The `family_gate` payload over `sessions` (one entry per transcript).
#[must_use]
pub fn family_gate_report(sessions: &[SessionFamilies]) -> Value {
    let (cal_n, cal) = window(sessions, |d| d < APPLICATION_START);
    let (app_n, app) = window(sessions, |d| d >= APPLICATION_START);
    json!({
        "mode": "shadow",
        "rule": rule_json(),
        "windows": {
            "calibration": window_json(cal_n, &cal, ("before", APPLICATION_START)),
            "application": window_json(app_n, &app, ("from", APPLICATION_START)),
        },
        "undated_sessions": sessions.iter().filter(|s| s.date.is_none()).count(),
        "calibration": calibration_json(&cal, &app),
        "method": "per family: the MUST blocks it emitted, and how many the next Bash call ran (the global injection_follow_ratio rule, attributed); tokens are bytes x tokens_per_byte (an estimate). Nothing is written to any policy.",
    })
}

/// The `family_gate` section for `project_root`, under the shadow caps.
#[must_use]
pub fn family_gate(project_root: &Path) -> Value {
    let Some(home) = std::env::var_os("HOME") else {
        return json!({"available": false, "reason": "HOME unset"});
    };
    family_gate_in(Path::new(&home), project_root, &SHADOW_CAPS)
}

/// [`family_gate`] against an explicit `home` and `caps` — the testable body.
#[must_use]
pub fn family_gate_in(home: &Path, project_root: &Path, caps: &ScanCaps) -> Value {
    let dir = transcript_dir_for(home, project_root);
    let scan = match scan_dir::<FamilyTracker>(&dir, caps, &mut |t, rec| t.observe(rec)) {
        Ok(scan) => scan,
        Err(reason) => return json!({"available": false, "reason": reason}),
    };
    let malformed: u64 = scan.sessions.iter().map(|s| s.malformed_lines).sum();
    let meta = scan_meta(
        scan.sessions.len(),
        scan.bytes_scanned,
        scan.capped,
        scan.skipped_by_cap,
        malformed,
    );
    let sessions: Vec<SessionFamilies> = scan
        .sessions
        .into_iter()
        .map(|s| s.state.finish())
        .collect();
    let mut out = family_gate_report(&sessions);
    merge(&mut out, meta);
    out
}

#[cfg(test)]
#[path = "family_gate_tests.rs"]
mod tests;
