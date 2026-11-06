//! Gate scenarios — event sequences in, verdicts out.
//!
//! One table drives three kinds of proof, all through the SAME entry point (`drive`, the
//! only implementation-specific function in this file):
//!
//! 1. **The contract** — every row states what each event must yield (`has` / `lacks` on the
//!    decoded text, and the verdict kind). The rows replace the old tests that reached into
//!    the gates' private ledgers, windows and helpers.
//! 2. **Equivalence** — the exact response of every step was recorded from the implementation
//!    BEFORE the gates moved out of `cli_suggester` (`tests/fixtures/gate_replay_corpus.json`);
//!    a refactor must reproduce it byte for byte. The justified-differences list is empty.
//! 3. **Coverage of the old suite** — each row names the old tests it subsumes (`covers`), and
//!    `tests/fixtures/old_gate_tests.tsv` lists every old gate test with where it went.
//!
//! The rows are data: a project root is a fresh temp dir (`{ROOT}` in a command, a path or an
//! expectation is that dir; `{OUT}` is a second dir OUTSIDE the project), and the session of
//! each step is prefixed with the scenario name, so no two scenarios share a ledger key.

use serde_json::{Value, json};
use std::path::Path;

// ── The DSL ──────────────────────────────────────────────────────────────────────────────

/// What the project declares in `.touring/touring.toml`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Scope {
    /// No declaration: the resolved mode is the default (`both`).
    Unset,
    Code,
}

/// The injected environment of a scenario. Every field is the value an operator would have set
/// in the DAEMON's environment — never the test process's.
#[derive(Clone, Default)]
pub(crate) struct Cfg {
    pub g2_rewrite_disabled: bool,
    pub g1_rewrite_disabled: bool,
    /// `TOURING_CODE_GATES_DISABLED=1`.
    pub kill_switch: bool,
    /// `TOURING_CODE_MODE` as the hook process sees it.
    pub env_mode: Option<&'static str>,
    /// `TOURING_CODE_ONLY`.
    pub env_alias: Option<&'static str>,
    /// Live precision of G1 injected as `(precision, volume)`; `None` = "no data".
    pub g1_precision: Option<(f64, u64)>,
    /// Build the scratch portfolio (one artifact that matches a `pytest` burst).
    pub portfolio_scratch: bool,
    /// Pin the R1 skeleton path the G7 route names (`None` = no skeleton on this machine).
    pub r1_skeleton: Option<&'static str>,
}

/// The kind of response a step must produce.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum K {
    /// No gate speaks: the call runs untouched.
    Pass,
    /// `additionalContext` only — never a decision.
    Advisory,
    /// `permissionDecision: deny`.
    Deny,
    /// `permissionDecision: allow` + `updatedInput`.
    Rewrite,
    /// Some response (any kind) — or, with `has`, one carrying that text.
    Speaks,
    /// The response is whatever it is — the step only moves the state forward.
    Any,
    /// Either no response, or one that does NOT carry `lacks[0]` — "this gate did not fire".
    Quiet,
    /// Anything except a rewrite (`updatedInput`).
    NotRewrite,
    /// Anything except a deny.
    NotDeny,
    /// `has` is required only when the response IS a rewrite.
    IfRewrite,
}

#[derive(Clone)]
pub(crate) struct W {
    pub k: K,
    pub has: Vec<String>,
    pub lacks: Vec<String>,
    /// For a deny: the gate id the CONSUMERS read from the reason (`tool_trio::deny_gate_of`).
    pub gate: Option<String>,
    /// Fields of the `updatedInput` (JSON pointer → value) a rewrite must carry.
    pub upd: Vec<(String, Value)>,
    /// The executable (non-comment) lines of the rewrite's `--code '…'` body.
    pub code: Option<Vec<String>>,
}

impl W {
    fn new(k: K, has: &[&str]) -> Self {
        Self {
            k,
            has: has.iter().map(|s| s.to_string()).collect(),
            lacks: Vec::new(),
            gate: None,
            upd: Vec::new(),
            code: None,
        }
    }
    /// Texts that must NOT appear in the response.
    pub fn without(mut self, lacks: &[&str]) -> Self {
        self.lacks.extend(lacks.iter().map(|s| s.to_string()));
        self
    }
    /// The gate id a deny must carry.
    pub fn gate(mut self, id: &str) -> Self {
        self.gate = Some(id.to_string());
        self
    }
    /// A field the `updatedInput` must carry.
    pub fn upd(mut self, pointer: &str, v: Value) -> Self {
        self.upd.push((pointer.to_string(), v));
        self
    }
    /// The executable lines of the rewritten program.
    pub fn code(mut self, lines: &[&str]) -> Self {
        self.code = Some(lines.iter().map(|s| s.to_string()).collect());
        self
    }
}

pub(crate) fn pass() -> W {
    W::new(K::Pass, &[])
}
pub(crate) fn any() -> W {
    W::new(K::Any, &[])
}
pub(crate) fn advisory(has: &[&str]) -> W {
    W::new(K::Advisory, has)
}
pub(crate) fn deny(has: &[&str]) -> W {
    W::new(K::Deny, has)
}
pub(crate) fn rewrite(has: &[&str]) -> W {
    W::new(K::Rewrite, has)
}
pub(crate) fn speaks(has: &[&str]) -> W {
    W::new(K::Speaks, has)
}
pub(crate) fn not_rewrite() -> W {
    W::new(K::NotRewrite, &[])
}
pub(crate) fn not_deny() -> W {
    W::new(K::NotDeny, &[])
}
pub(crate) fn if_rewrite(has: &[&str]) -> W {
    W::new(K::IfRewrite, has)
}
/// The gate identified by `marker` did not fire (the response is absent or lacks the marker).
pub(crate) fn quiet(marker: &str) -> W {
    let mut w = W::new(K::Quiet, &[]);
    w.lacks.push(marker.to_string());
    w
}

#[derive(Clone)]
pub(crate) enum Ev {
    Call {
        /// 0 = the project root, 1 = a second, unrelated project.
        proj: usize,
        sess: &'static str,
        tool: &'static str,
        input: Value,
        want: W,
    },
    /// Read `pointer` of the JSON file `path` (under the project) and compare it.
    Json {
        path: String,
        pointer: String,
        want: Option<u64>,
    },
    /// Write a file in the project (or `{OUT}`) and pin its mtime to `1_700_000_000 + tick` s.
    Touch {
        path: String,
        body: String,
        tick: u64,
    },
}

#[derive(Clone)]
pub(crate) struct Scn {
    pub name: &'static str,
    /// The old tests this row subsumes.
    pub covers: &'static [&'static str],
    pub scope: Scope,
    pub cfg: Cfg,
    pub files: Vec<(String, String)>,
    /// `(link path, target)` created before the first event.
    pub symlinks: Vec<(String, String)>,
    pub evs: Vec<Ev>,
}

pub(crate) fn scn(name: &'static str, covers: &'static [&'static str]) -> Scn {
    Scn {
        name,
        covers,
        scope: Scope::Unset,
        cfg: Cfg::default(),
        files: Vec::new(),
        symlinks: Vec::new(),
        evs: Vec::new(),
    }
}

impl Scn {
    pub fn code(mut self) -> Self {
        self.scope = Scope::Code;
        self
    }
    pub fn cfg(mut self, cfg: Cfg) -> Self {
        self.cfg = cfg;
        self
    }
    pub fn file(mut self, path: &str, body: &str) -> Self {
        self.files.push((path.into(), body.into()));
        self
    }
    pub fn symlink(mut self, link: &str, target: &str) -> Self {
        self.symlinks.push((link.into(), target.into()));
        self
    }
    pub fn call(self, sess: &'static str, tool: &'static str, input: Value, want: W) -> Self {
        self.call_on(0, sess, tool, input, want)
    }
    pub fn call_on(
        mut self,
        proj: usize,
        sess: &'static str,
        tool: &'static str,
        input: Value,
        want: W,
    ) -> Self {
        self.evs.push(Ev::Call {
            proj,
            sess,
            tool,
            input,
            want,
        });
        self
    }
    /// A Bash call on the second project.
    pub fn bash_on(self, proj: usize, sess: &'static str, cmd: &str, want: W) -> Self {
        self.call_on(proj, sess, "Bash", json!({ "command": cmd }), want)
    }
    pub fn json_at(mut self, path: &str, pointer: &str, want: Option<u64>) -> Self {
        self.evs.push(Ev::Json {
            path: path.into(),
            pointer: pointer.into(),
            want,
        });
        self
    }
    pub fn bash(self, sess: &'static str, cmd: &str, want: W) -> Self {
        self.call(sess, "Bash", json!({ "command": cmd }), want)
    }
    pub fn read(self, sess: &'static str, fp: &str, want: W) -> Self {
        self.call(sess, "Read", json!({ "file_path": fp }), want)
    }
    pub fn edit(self, sess: &'static str, fp: &str, want: W) -> Self {
        self.call(
            sess,
            "Edit",
            json!({ "file_path": fp, "new_string": "let x = 1;" }),
            want,
        )
    }
    pub fn touch(mut self, path: &str, body: &str, tick: u64) -> Self {
        self.evs.push(Ev::Touch {
            path: path.into(),
            body: body.into(),
            tick,
        });
        self
    }
    /// Repeat `f(i)` for `i` in `from..to`, appending whatever events it adds.
    pub fn each(self, from: usize, to: usize, f: impl Fn(Self, usize) -> Self) -> Self {
        (from..to).fold(self, f)
    }
}

// ── What a response is ───────────────────────────────────────────────────────────────────

/// A response decoded: the kind, and every string the model would read.
pub(crate) struct Obs {
    pub k: K,
    pub text: String,
    pub updated: Value,
}

pub(crate) fn observe(out: &Option<String>) -> Obs {
    let Some(raw) = out else {
        return Obs {
            k: K::Pass,
            text: String::new(),
            updated: Value::Null,
        };
    };
    let v: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let h = &v["hookSpecificOutput"];
    let k = match h["permissionDecision"].as_str() {
        Some("deny") => K::Deny,
        Some("allow") if h.get("updatedInput").is_some() => K::Rewrite,
        _ => K::Advisory,
    };
    let mut text = String::new();
    for part in [
        h["permissionDecisionReason"].as_str(),
        h["additionalContext"].as_str(),
        h["updatedInput"]["command"].as_str(),
    ]
    .into_iter()
    .flatten()
    {
        text.push_str(part);
        text.push('\n');
    }
    Obs {
        k,
        text,
        updated: h["updatedInput"].clone(),
    }
}

/// The executable lines of the `--code '…'` body of a rewritten command.
fn code_lines(command: &str) -> Vec<String> {
    let Some(i) = command.find("--code '") else {
        return Vec::new();
    };
    let body = &command[i + "--code '".len()..command.len() - 1];
    body.replace("'\\''", "'")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

fn check(scn: &str, i: usize, want: &W, out: &Option<String>) {
    let o = observe(out);
    let ctx = || format!("[{scn} step {i}] got {:?}: {}", o.k, o.text);
    match want.k {
        K::Any => return,
        K::Speaks => assert!(out.is_some(), "{} — expected a response", ctx()),
        K::Quiet => {}
        K::NotRewrite => assert_ne!(o.k, K::Rewrite, "{} — must not rewrite", ctx()),
        K::NotDeny => assert_ne!(o.k, K::Deny, "{} — must not deny", ctx()),
        K::IfRewrite => {
            if o.k != K::Rewrite {
                return;
            }
        }
        k => assert_eq!(o.k, k, "{} — expected {k:?}", ctx()),
    }
    for h in &want.has {
        assert!(o.text.contains(h.as_str()), "{} — missing `{h}`", ctx());
    }
    for l in &want.lacks {
        assert!(!o.text.contains(l.as_str()), "{} — must lack `{l}`", ctx());
    }
    for (pointer, v) in &want.upd {
        assert_eq!(
            o.updated.pointer(pointer),
            Some(v),
            "{} — updatedInput {pointer}",
            ctx()
        );
    }
    if let Some(lines) = &want.code {
        let got = code_lines(o.updated["command"].as_str().unwrap_or_default());
        assert_eq!(&got, lines, "{} — the program's executable lines", ctx());
    }
    if let (Some(id), Some(raw)) = (&want.gate, out) {
        assert_eq!(
            touring_hooks_shared::tool_trio::deny_gate_of(raw).as_deref(),
            Some(id.as_str()),
            "{} — the gate id consumers read",
            ctx()
        );
    }
}

// ── The runner ───────────────────────────────────────────────────────────────────────────

/// One step as recorded: the call (as a template) and the normalized response.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Step {
    pub tool: String,
    pub input: Value,
    pub out: Option<String>,
}

#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Entry {
    pub scn: String,
    pub steps: Vec<Step>,
}

/// The implementation under test: `(project root, session, tool, input, config, portfolio dir)`
/// → the hook response (`None` = the call is not touched).
pub(crate) type Drive<'a> =
    &'a mut dyn FnMut(&Path, &str, &str, &Value, &Cfg, &Path) -> Option<String>;

fn subst(s: &str, root: &Path, out: &Path) -> String {
    s.replace("{ROOT}", &root.display().to_string())
        .replace("{OUT}", &out.display().to_string())
}

fn subst_value(v: &Value, root: &Path, out: &Path) -> Value {
    serde_json::from_str(&subst(&v.to_string(), root, out)).expect("substituted json")
}

fn pin_mtime(path: &Path, tick: u64) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + tick);
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(t)
        .expect("set_modified");
}

/// The scratch portfolio: ONE artifact whose purpose matches a `pytest` burst.
fn build_portfolio(dir: &Path) {
    use touring_foundation::portfolio::store::{PortfolioIndex, save_to};
    use touring_foundation::portfolio::{CapabilityEntry, CapabilityKind, Evidence};
    let entry = CapabilityEntry {
        id: CapabilityEntry::make_id(CapabilityKind::Script, "~/x/rodar_suite.py"),
        display_path: "~/x/rodar_suite.py".into(),
        kind: CapabilityKind::Script,
        name: "rodar_suite".into(),
        purpose: "executar suite de testes pytest agregada com digest resumido".into(),
        language: "python".into(),
        entry_point: Some("python3 ~/x/rodar_suite.py --fast".into()),
        provenance: "teste s6".into(),
        keywords: vec!["pytest".into(), "suite".into(), "tests".into()],
        evidence: Evidence::default(),
        purpose_inherited: false,
    };
    let mut index = PortfolioIndex::empty();
    index.entries.push(entry);
    save_to(dir, &index).expect("save portfolio");
}

/// Run one scenario against `drive`, asserting every step's expectation, and return the
/// normalized transcript (what the corpus records).
pub(crate) fn run(scn: &Scn, drive: Drive) -> Entry {
    execute(scn, drive, true)
}

/// Play a scenario's events and return the transcript WITHOUT asserting the row's own
/// expectations — for properties that re-run the table under another configuration.
pub(crate) fn play(scn: &Scn, drive: Drive) -> Entry {
    execute(scn, drive, false)
}

fn execute(scn: &Scn, drive: Drive, checked: bool) -> Entry {
    let root_dir = tempfile::tempdir().expect("project root");
    let root2_dir = tempfile::tempdir().expect("second project root");
    let out_dir = tempfile::tempdir().expect("outside dir");
    let port_dir = tempfile::tempdir().expect("portfolio dir");
    let (root, root2, out) = (root_dir.path(), root2_dir.path(), out_dir.path());
    // The same scenario runs in several tests of one process, in parallel: a session shared by
    // two runs would share the session-keyed ledgers (G3, G5, M3, pending denies).
    static RUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let run_id = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if scn.cfg.portfolio_scratch {
        build_portfolio(port_dir.path());
    }
    if scn.scope == Scope::Code {
        std::fs::create_dir_all(root.join(".touring")).expect("mkdir .touring");
        std::fs::write(
            root.join(".touring/touring.toml"),
            "[code_mode]\nmode = \"code\"\n",
        )
        .expect("write toml");
    }
    for (path, body) in &scn.files {
        let p = std::path::PathBuf::from(subst(path, root, out));
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, subst(body, root, out)).expect("write file");
        pin_mtime(&p, 0);
    }
    for (link, target) in &scn.symlinks {
        let l = std::path::PathBuf::from(subst(link, root, out));
        std::fs::create_dir_all(l.parent().expect("parent")).expect("mkdir");
        std::os::unix::fs::symlink(subst(target, root, out), &l).expect("symlink");
    }
    let norm = |s: String| {
        s.replace(&root.display().to_string(), "{ROOT}")
            .replace(&root2.display().to_string(), "{ROOT2}")
            .replace(&out.display().to_string(), "{OUT}")
    };
    let mut steps = Vec::new();
    let mut i = 0;
    for ev in &scn.evs {
        match ev {
            Ev::Touch { path, body, tick } => {
                let p = std::path::PathBuf::from(subst(path, root, out));
                std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
                std::fs::write(&p, subst(body, root, out)).expect("touch write");
                pin_mtime(&p, *tick);
            }
            Ev::Json {
                path,
                pointer,
                want,
            } => {
                let p = root.join(subst(path, root, out));
                let v: Value = std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or(Value::Null);
                if checked {
                    assert_eq!(
                        v.pointer(pointer).and_then(Value::as_u64),
                        *want,
                        "[{} probe {path}{pointer}] {v}",
                        scn.name
                    );
                }
            }
            Ev::Call {
                proj,
                sess,
                tool,
                input,
                want,
            } => {
                let session = format!("{}#{run_id}::{sess}", scn.name);
                let this_root = if *proj == 0 { root } else { root2 };
                let real = subst_value(input, root, out);
                let resp = drive(this_root, &session, tool, &real, &scn.cfg, port_dir.path());
                let resp = resp.map(norm);
                let mut w = want.clone();
                w.has = w.has.iter().map(|s| subst(s, root, out)).collect();
                w.lacks = w.lacks.iter().map(|s| subst(s, root, out)).collect();
                // The expectation is stated with real paths; the transcript with `{ROOT}`.
                let resp_real = resp.as_ref().map(|r| subst(r, root, out));
                if checked {
                    check(scn.name, i, &w, &resp_real);
                }
                steps.push(Step {
                    tool: tool.to_string(),
                    input: input.clone(),
                    out: resp,
                });
                i += 1;
            }
        }
    }
    Entry {
        scn: scn.name.to_string(),
        steps,
    }
}
