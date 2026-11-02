//! The gate scenario table, driven through the implementation under test.
//!
//! `drive` is the only implementation-specific function: when the gates moved out of
//! `cli_suggester` it was re-pointed, and every other line of this proof — the rows, the
//! recorded corpus, the expectations — stayed byte for byte. A refactor that changes any
//! response (a window, a threshold, the order of the chain, one character of a reason) fails
//! `the_recorded_corpus_is_reproduced_exactly`.

use super::gate_rows::scenarios;
use super::gate_scenarios::*;
use super::{CodeModeEnv, GateOverrides, code_mode_gates_with};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/gate_replay_corpus.json"
);

/// Differences from the recorded corpus that a human accepted, with the reason.
/// `(scenario, step, why)` — EMPTY: the refactor changes no response.
const JUSTIFIED_DIFFERENCES: &[(&str, usize, &str)] = &[];

/// The implementation under test.
fn drive(
    root: &Path,
    session: &str,
    tool: &str,
    input: &Value,
    cfg: &Cfg,
    portfolio: &Path,
) -> Option<String> {
    let ovr = GateOverrides {
        g2_rewrite_disabled: cfg.g2_rewrite_disabled,
        g1_rewrite_disabled: cfg.g1_rewrite_disabled,
        portfolio_dir: Some(portfolio.to_path_buf()),
        code_mode_env: CodeModeEnv {
            mode: cfg.env_mode.map(String::from),
            code_only_alias: cfg.env_alias.map(String::from),
            arm_armed: false,
        },
        code_gates_disabled: cfg.kill_switch,
        g1_precision: Some(cfg.g1_precision),
        r1_skeleton: Some(cfg.r1_skeleton.map(PathBuf::from)),
    };
    code_mode_gates_with(root, session, tool, input, &ovr)
}

// ── The composition of G7 × G3 × G9 (a generated state space) ────────────────────────────

const RETOMADA_REL: &str = "docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md";

#[derive(Clone, Copy, Debug)]
enum Op {
    Read,
    Edit,
    /// A write OUTSIDE the Edit tool: python, a linter, another session.
    Modify,
    /// `touring run` citing the file (resets G7).
    Run,
    /// A blind `sed -i` on the file: G9 denies and NAMES the Edit as the route.
    SedBlind,
}

const OPS: [Op; 5] = [Op::Read, Op::Edit, Op::Modify, Op::Run, Op::SedBlind];

/// FNV-1a over the normalized responses: the digest the corpus pins for the composition.
struct Fnv(u64, usize);

impl Fnv {
    fn push(&mut self, text: &str) {
        for b in text.bytes().chain([0xff]) {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.1 += 1;
    }
}

struct World<'a> {
    root: &'a Path,
    sess: String,
    fp: PathBuf,
    tick: u64,
    /// The harness' `readFileState` is up to date with the disk.
    fresh: bool,
    /// A successful `Read` already happened in this session (G3 knows the file).
    read: bool,
}

fn touch(fp: &Path, body: &str, tick: u64) {
    std::fs::write(fp, body).expect("write");
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + tick);
    std::fs::File::options()
        .write(true)
        .open(fp)
        .expect("open")
        .set_modified(t)
        .expect("set_modified");
}

fn is_deny(r: &Option<String>) -> bool {
    observe(r).k == K::Deny
}

struct Composition<'a> {
    drive: &'a mut dyn FnMut(&Path, &str, &str, &Value) -> Option<String>,
    digest: Fnv,
    sessions: u64,
    seds: u64,
}

impl Composition<'_> {
    fn call(&mut self, w: &World, tool: &str, input: &Value) -> Option<String> {
        let out = (self.drive)(w.root, &w.sess, tool, input);
        let root = w.root.display().to_string();
        self.digest
            .push(&out.clone().unwrap_or_default().replace(&root, "{ROOT}"));
        out
    }

    fn world<'a>(&mut self, root: &'a Path, fp: &Path) -> World<'a> {
        let n = self.sessions;
        self.sessions += 1;
        touch(fp, "base", 0);
        World {
            root,
            // NOT `g97-mundo-*`: the G3 streak is keyed by session alone, and the original
            // composition test (gates97_tests) explores the same space on the same names.
            sess: format!("replay-mundo-{n}"),
            fp: fp.to_path_buf(),
            tick: 0,
            fresh: false,
            read: false,
        }
    }

    fn read(&mut self, w: &mut World) -> Option<String> {
        let r = self.call(w, "Read", &json!({ "file_path": w.fp, "limit": 8 }));
        if !is_deny(&r) {
            w.fresh = true;
            w.read = true;
        }
        r
    }

    /// `.1` = the Edit SUCCEEDED (the hook let it through AND the harness accepts it).
    fn edit(&mut self, w: &mut World) -> (Option<String>, bool) {
        let r = self.call(
            w,
            "Edit",
            &json!({ "file_path": w.fp, "old_string": "a", "new_string": "b" }),
        );
        if w.read {
            assert!(
                !r.as_deref().is_some_and(|x| x.contains("[G3 edit-sem-read]")),
                "G3 must not speak of a file the session already read: {r:?}"
            );
        }
        let ok = !is_deny(&r) && w.fresh;
        if ok {
            w.tick += 1;
            touch(&w.fp, "editado", w.tick);
        }
        (r, ok)
    }

    fn apply(&mut self, w: &mut World, op: Op) {
        match op {
            Op::Read => {
                let _ = self.read(w);
            }
            Op::Edit => {
                let _ = self.edit(w);
            }
            Op::Modify => {
                w.tick += 1;
                touch(&w.fp, "externo", w.tick);
                w.fresh = false;
            }
            Op::Run => {
                let cmd = format!(
                    "touring run --lang python --args '[\"{}\"]' --file r1.py",
                    w.fp.display()
                );
                let _ = self.call(w, "Bash", &json!({ "command": cmd }));
            }
            Op::SedBlind => {
                let sed = |c: &mut Self, w: &World| {
                    let n = c.seds;
                    c.seds += 1;
                    c.call(
                        w,
                        "Bash",
                        &json!({ "command": format!("sed -i 's/a/b{n}/' {}", w.fp.display()) }),
                    )
                };
                let mut r = sed(self, w);
                // G5 (telemetry) speaks FIRST on the first Bash after a burst of ≥3 edits.
                if r.as_deref().is_some_and(|x| x.contains("[G5 telemetria]")) {
                    r = sed(self, w);
                }
                assert!(is_deny(&r), "a blind `sed -i` is denied by G9: {r:?}");
                let reason = observe(&r).text;
                assert!(
                    reason.contains(&format!("Edit tool em `{}`", w.fp.display())),
                    "the G9 deny names the Edit as the route: {reason}"
                );
            }
        }
    }

    fn after<'a>(&mut self, root: &'a Path, fp: &Path, prefix: &[Op]) -> World<'a> {
        let mut w = self.world(root, fp);
        for op in prefix {
            self.apply(&mut w, *op);
        }
        w
    }

    /// From this state there is an edit route no gate denies and the harness accepts: the
    /// direct `Edit`, or a released `Read` followed by the `Edit`.
    fn has_edit_route(&mut self, root: &Path, fp: &Path, prefix: &[Op]) -> bool {
        let mut direct = self.after(root, fp, prefix);
        if self.edit(&mut direct).1 {
            return true;
        }
        let mut via_read = self.after(root, fp, prefix);
        !is_deny(&self.read(&mut via_read)) && self.edit(&mut via_read).1
    }

    fn walk(
        &mut self,
        root: &Path,
        fp: &Path,
        prefix: &mut Vec<Op>,
        depth: usize,
        failures: &mut Vec<String>,
    ) {
        if !self.has_edit_route(root, fp, prefix) {
            failures.push(format!("{prefix:?}"));
        }
        if depth == 0 {
            return;
        }
        for op in OPS {
            prefix.push(op);
            self.walk(root, fp, prefix, depth - 1, failures);
            prefix.pop();
        }
    }
}

/// Explore every state reachable by up to 5 operations; return `(calls, digest)`.
fn explore_composition() -> (usize, u64) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let port = tempfile::tempdir().expect("portfolio");
    std::fs::create_dir_all(tmp.path().join(".touring")).expect("mkdir");
    std::fs::write(
        tmp.path().join(".touring/touring.toml"),
        "[code_mode]\nmode = \"code\"\n",
    )
    .expect("toml");
    let fp = tmp.path().join(RETOMADA_REL);
    std::fs::create_dir_all(fp.parent().expect("parent")).expect("mkdir");
    std::fs::write(&fp, "base").expect("write");
    let cfg = Cfg::default();
    let mut d = |root: &Path, sess: &str, tool: &str, input: &Value| {
        drive(root, sess, tool, input, &cfg, port.path())
    };
    let mut c = Composition {
        drive: &mut d,
        digest: Fnv(0xcbf2_9ce4_8422_2325, 0),
        sessions: 0,
        seds: 0,
    };
    let mut failures = Vec::new();
    c.walk(tmp.path(), &fp, &mut Vec::new(), 5, &mut failures);
    assert!(
        failures.is_empty(),
        "{} state(s) without an edit route — the circle; the smallest: {:?}",
        failures.len(),
        failures.iter().min_by_key(|f| f.len())
    );
    (c.digest.1, c.digest.0)
}

// ── The proofs ───────────────────────────────────────────────────────────────────────────

fn run_all() -> Vec<Entry> {
    scenarios()
        .iter()
        .map(|s| run(s, &mut |r, se, t, i, c, p| drive(r, se, t, i, c, p)))
        .collect()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Corpus {
    entries: Vec<Entry>,
    composition_calls: usize,
    composition_digest: String,
}

/// RECORD: `GATE_REPLAY_RECORD=<path> cargo test -p touring-cli record_the_gate_replay_corpus
/// -- --ignored`. Run against the implementation whose behavior is to be preserved.
#[test]
#[ignore = "writes the corpus; run on purpose, against the reference implementation"]
fn record_the_gate_replay_corpus() {
    let path = std::env::var("GATE_REPLAY_RECORD").expect("GATE_REPLAY_RECORD=<path>");
    let (calls, digest) = explore_composition();
    let corpus = Corpus {
        entries: run_all(),
        composition_calls: calls,
        composition_digest: format!("{digest:016x}"),
    };
    std::fs::write(&path, serde_json::to_string_pretty(&corpus).expect("json") + "\n")
        .expect("write corpus");
}

#[test]
fn the_scenario_table_holds_its_contract() {
    let rows = scenarios();
    let names: std::collections::BTreeSet<_> = rows.iter().map(|s| s.name).collect();
    assert_eq!(names.len(), rows.len(), "scenario names must be unique");
    // Every row runs; the failures are reported together, not one per run.
    let mut failed = Vec::new();
    for s in &rows {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(s, &mut |r, se, t, i, c, p| drive(r, se, t, i, c, p))
        }));
        if let Err(e) = r {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            failed.push(msg);
        }
    }
    assert!(failed.is_empty(), "{} row(s) failed:\n{}", failed.len(), failed.join("\n---\n"));
}

#[test]
fn the_recorded_corpus_is_reproduced_exactly() {
    let text = std::fs::read_to_string(CORPUS).expect("tests/fixtures/gate_replay_corpus.json");
    let want: Corpus = serde_json::from_str(&text).expect("corpus json");
    let got = run_all();
    let mut diffs = Vec::new();
    for e in &got {
        let Some(w) = want.entries.iter().find(|w| w.scn == e.scn) else {
            diffs.push(format!("scenario `{}` is not in the corpus", e.scn));
            continue;
        };
        assert_eq!(w.steps.len(), e.steps.len(), "`{}`: step count", e.scn);
        for (i, (a, b)) in w.steps.iter().zip(&e.steps).enumerate() {
            if a != b
                && !JUSTIFIED_DIFFERENCES
                    .iter()
                    .any(|(s, n, _)| *s == e.scn && *n == i)
            {
                diffs.push(format!(
                    "`{}` step {i} ({}):\n  recorded: {:?}\n  now:      {:?}",
                    e.scn, a.tool, a.out, b.out
                ));
            }
        }
    }
    for w in &want.entries {
        if !got.iter().any(|e| e.scn == w.scn) {
            diffs.push(format!("corpus scenario `{}` is no longer driven", w.scn));
        }
    }
    assert!(diffs.is_empty(), "{} difference(s):\n{}", diffs.len(), diffs.join("\n"));
}

/// ~30 000 gate calls, each a fresh session: on the process-global ledgers (capacity 4096) two
/// such explorations at once evict each other's live entries. Serial with the original test.
#[test]
#[serial_test::serial(gate_metrics, t3_env)]
fn the_composition_of_g7_g3_g9_always_leaves_an_edit_route() {
    let text = std::fs::read_to_string(CORPUS).expect("tests/fixtures/gate_replay_corpus.json");
    let want: Corpus = serde_json::from_str(&text).expect("corpus json");
    let (calls, digest) = explore_composition();
    assert_eq!(calls, want.composition_calls, "composition: number of gate calls");
    assert_eq!(
        format!("{digest:016x}"),
        want.composition_digest,
        "composition: digest of every response across the explored state space"
    );
}

// ── S-13: every denial names the next command ────────────────────────────────────────────

/// The reason of the gate ITSELF: the verdict merge appends other gates' advisories after the
/// separator, and a `touring run` in an advisory is not the next step of the denial.
fn own_reason(reason: &str) -> String {
    reason
        .split(touring_hook_runtime::verdict::ALSO_NON_BLOCKING)
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

const CALL_VERBS: [(&str, usize); 5] = [
    ("touring ", 3),
    ("Read ", 2),
    ("Edit ", 2),
    ("set -o pipefail", 3),
    ("TOURING_GATE_OK=1", 2),
];

fn opens_a_call(text: &str) -> bool {
    CALL_VERBS
        .iter()
        .any(|(verb, words)| text.starts_with(verb) && text.split_whitespace().count() >= *words)
}

fn names_next_command(reason: &str) -> bool {
    let ready_line = reason
        .lines()
        .any(|l| l.strip_prefix("  ").is_some_and(|r| opens_a_call(r.trim_start())));
    let in_backticks = reason.split('`').skip(1).step_by(2).any(opens_a_call);
    let bypass_then_command = reason.match_indices("TOURING_GATE_OK=1").any(|(i, _)| {
        reason[i + "TOURING_GATE_OK=1".len()..]
            .strip_prefix(' ')
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '_' | '~' | '$'))
    });
    reason.contains("touring run --") || ready_line || in_backticks || bypass_then_command
}

#[test]
fn every_gate_reason_names_the_next_command() {
    let mut reasons: Vec<(String, String)> = Vec::new();
    for s in scenarios() {
        let tagged: Vec<(usize, String)> = s
            .evs
            .iter()
            .filter_map(|e| match e {
                Ev::Call { want, .. } if want.k == K::Deny => want.gate.clone(),
                _ => None,
            })
            .enumerate()
            .collect();
        if tagged.is_empty() {
            continue;
        }
        let e = run(&s, &mut |r, se, t, i, c, p| drive(r, se, t, i, c, p));
        let denies = e
            .steps
            .iter()
            .zip(s.evs.iter().filter(|e| matches!(e, Ev::Call { .. })))
            .filter(|(_, ev)| matches!(ev, Ev::Call { want, .. } if want.k == K::Deny && want.gate.is_some()));
        for ((_, id), (step, _)) in tagged.iter().zip(denies) {
            let reason = observe(&step.out).text;
            reasons.push((id.clone(), own_reason(reason.trim_end())));
        }
    }
    for id in ["G1", "G2", "G3", "G6", "G7", "G9", "G10", "G11"] {
        assert!(
            reasons.iter().any(|(g, _)| g == id),
            "{id}: no row drives its deny — the structural test would have a hole"
        );
    }
    let lost_prefix: Vec<_> = reasons
        .iter()
        .filter(|(g, r)| !r.starts_with(&format!("[{g} ")))
        .collect();
    assert!(lost_prefix.is_empty(), "the `[G<n> name]` prefix is parsed by consumers: {lost_prefix:#?}");
    let no_command: Vec<_> = reasons.iter().filter(|(_, r)| !names_next_command(r)).collect();
    assert!(
        no_command.is_empty(),
        "a denial that does not name the next command teaches nothing ({} of {}): {no_command:#?}",
        no_command.len(),
        reasons.len()
    );
}

// ── Latency: the hook is on the hot path (~1 ms target) ──────────────────────────────────

/// Median / p99 (µs) of the gate decision over 1000 calls, for three workloads. Run with
/// `cargo test -p touring-cli gate_decision_latency -- --ignored --nocapture`, before and
/// after a change to the gates, under the SAME profile.
#[test]
#[ignore = "benchmark; prints, asserts only a sanity ceiling"]
#[serial_test::serial(gate_metrics, t3_env)]
fn gate_decision_latency() {
    let root = tempfile::tempdir().expect("root");
    let port = tempfile::tempdir().expect("portfolio");
    let cfg = Cfg::default();
    let workloads: [(&str, Box<dyn Fn(usize) -> (&'static str, Value)>); 3] = [
        (
            "bash-no-gate",
            Box::new(|i| ("Bash", json!({ "command": format!("cargo check -p crate{i}") }))),
        ),
        (
            "bash-inspection",
            Box::new(|i| ("Bash", json!({ "command": format!("rg -n 'pat{i}' src/f{i}.rs") }))),
        ),
        (
            "read-distinct",
            Box::new(|i| ("Read", json!({ "file_path": format!("src/lat_{i}.rs") }))),
        ),
    ];
    for (name, make) in &workloads {
        let session = format!("latency-{name}");
        let mut us: Vec<u128> = (0..1000)
            .map(|i| {
                let (tool, input) = make(i);
                let t = std::time::Instant::now();
                let _ = drive(root.path(), &session, tool, &input, &cfg, port.path());
                t.elapsed().as_nanos()
            })
            .collect();
        us.sort_unstable();
        eprintln!(
            "LATENCY {name}: median {} ns, p99 {} ns (1000 calls)",
            us[500], us[990]
        );
        assert!(us[500] < 5_000_000, "{name}: median over 5 ms is not a hot path");
    }
}
