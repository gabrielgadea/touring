//! Closed world for the E2E tests that run `touring doctor -j`.
//!
//! **Why it exists.** The `doctor` reads the project state from the CLIENT's CWD:
//! `check_project_db`, `check_wiring_diagnostic` and `check_index_generation` open
//! `<cwd>/.claude/touring/{symbols,knowledge}.db`. Under `cargo test` the CWD is the package
//! directory (`crates/touring-hooks`), whose `.claude/touring/*.db` is runtime state — ignored by
//! git and outside the index since 18/10/2026 (decision D4). The tests passed on the machines
//! where that directory existed by accident (an old index, copied or left behind by another
//! run) and failed on a clean clone: `project_db`, `wiring_diagnostic` and `index_generation`
//! with `status: missing`. The proof that the product is wired and healthy cannot depend on what
//! the machine happened to keep.
//!
//! **What it does.** [`DoctorWorld`] builds a project in a tempdir, seeds the state the `doctor`
//! reads with the product's OWN constructors (`SymbolStore::new`, `FileKnowledgeDB::new` — the
//! same schema the daemon creates, never a hand-written SQLite that goes stale) and starts a
//! private daemon closed over that world ([`PrivateDaemon::start_isolated`]), so that the
//! client, the daemon, `HOME` and the CWD all point at the tempdir and nothing touches the
//! user's live daemon.
//!
//! **Circuit breaker.** `check_circuit_breaker` reads the file the breaker's own resolver
//! returns (`circuit_state_path`); the world points it inside the tempdir
//! (`TOURING_CIRCUIT_STATE_FILE`, via [`ProjectPin::circuit_state_file`]), so a breaker left
//! open on the machine does not decide the verdict — and [`DoctorWorld::trip_circuit_breaker`]
//! proves that the `doctor` reads that file and not the one in `/tmp`.
//!
//! Included by path (`wave24_hook_integration_e2e`) and by `mod common;` (the other suites):
//! in both cases `super::private_daemon` is the sibling holding the private daemon.

use super::private_daemon::{PrivateDaemon, ProjectPin};
use std::path::{Path, PathBuf};

/// How many `wiring_map` rows the seeding writes — and therefore what the `doctor` must count.
pub const SEEDED_WIRING_ROWS: u64 = 1;

/// Creates `<root>/.claude/touring/{symbols,knowledge}.db` with the product's schema and ONE
/// wiring row (a public symbol with no consumer).
///
/// **Why the row.** Both databases are also born inside the daemon when a hook request
/// instantiates the project runtime (`HookRuntime::new`: `FileKnowledgeDB::new` and
/// `SymbolStore::new`, `hook_runtime.rs`); the `doctor` probe alone does not instantiate it
/// (measured: without the seeding, `project_db` reports `missing`), but a world that started
/// exercising hooks would pass `project_db` with no seeding at all — "the file exists" does not
/// prove the seeding. The `doctor`'s count does: `wiring_diagnostic` reports `rows=1` only if it
/// read THIS seeded `knowledge.db`; a freshly created database reports `rows=0`, and the
/// repository's, thousands.
pub fn seed_project_state(root: &Path) {
    let dir = root.join(".claude").join("touring");
    std::fs::create_dir_all(&dir).expect("create <root>/.claude/touring");
    drop(
        touring_code::ast::store::SymbolStore::new(&dir.join("symbols.db"))
            .expect("create symbols.db with the product's constructor"),
    );
    let knowledge = touring_hooks::knowledge::FileKnowledgeDB::new(&dir.join("knowledge.db"))
        .expect("create knowledge.db with the product's constructor");
    knowledge
        .register_pub_symbol(
            "crates/seeded/src/lib.rs",
            "SeededSymbol",
            "struct",
            "public",
        )
        .expect("register the seeded wiring row");
    let orphans = knowledge.orphan_symbols().expect("query the seeded row");
    assert_eq!(
        orphans.len() as u64,
        SEEDED_WIRING_ROWS,
        "the seeded wiring row must pass the write gate: {orphans:?}"
    );
    drop(knowledge);
}

/// The result of a `touring doctor -j` inside the world.
pub struct DoctorRun {
    /// The client's exit code (`-1` if it ended by signal).
    pub code: i32,
    /// The JSON array of checks, in the order the `doctor` emits them.
    pub checks: Vec<serde_json::Value>,
}

/// A seeded project + a private daemon closed over it. `Drop` stops the daemon by the PID this
/// test created (REGRA #19) before the directory it serves disappears.
pub struct DoctorWorld {
    // `daemon` BEFORE `_dir`: fields drop in declaration order.
    daemon: PrivateDaemon,
    _dir: tempfile::TempDir,
    root: PathBuf,
    circuit_state: PathBuf,
}

impl DoctorWorld {
    /// `None` when the `touring`/`touring-daemon` binaries were not built or the daemon did not
    /// become ready: a SKIP announced on stderr by the caller, never a pass that looks like proof.
    #[must_use]
    pub fn start(tag: &str) -> Option<Self> {
        let dir = tempfile::Builder::new()
            .prefix("touring-e2edoctor-")
            .tempdir()
            .expect("create the hermetic temp dir");
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&root).expect("create the hermetic project root");
        std::fs::create_dir_all(&home).expect("create the hermetic HOME");
        // The state goes in BEFORE the daemon starts: it opens it at boot.
        seed_project_state(&root);
        let pin = ProjectPin {
            root: root.clone(),
            home,
        };
        let circuit_state = pin.circuit_state_file();
        let daemon = PrivateDaemon::start_isolated(tag, pin, Some(90))?;
        Some(Self {
            daemon,
            _dir: dir,
            root,
            circuit_state,
        })
    }

    /// Root of this world's project (the clients' CWD).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Opens this world's GLOBAL breaker (catastrophic, open until a very distant instant),
    /// writing the state file the `doctor` must read.
    pub fn trip_circuit_breaker(&self) {
        let state = serde_json::json!({
            "global": {"open_until_ts": 9_999_999_999_u64, "catastrophic_count": 3}
        });
        std::fs::write(&self.circuit_state, state.to_string())
            .expect("write this world's circuit state file");
    }

    /// Runs `touring doctor -j` pinned to this world. Fails LOUD if the client does not emit a
    /// JSON array: inside a world that is entirely ours that is a defect, not an "infrastructure
    /// gap".
    #[must_use]
    pub fn doctor(&self) -> DoctorRun {
        let mut cmd = self
            .daemon
            .client_command("touring")
            .expect("`touring` binary vanished after the daemon started");
        // This world's daemon embeds on CPU (`start_isolated`); the client declares the same
        // policy, otherwise `gpu_embeddings` reads "CPU with no CUDA failure" as `degraded` on a
        // machine with a GPU — a truth about the test's configuration, not about the product.
        let out = cmd
            .env("TOURING_EMBED_DEVICE", "cpu")
            .args(["doctor", "-j"])
            .output()
            .expect("spawn `touring doctor -j`");
        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!(
                "`doctor -j` emitted no JSON ({e}); exit {code}\nstdout: {stdout}\nstderr: {stderr}"
            )
        });
        let checks = json
            .as_array()
            .unwrap_or_else(|| panic!("`doctor -j` must return an array, got: {json}"))
            .clone();
        DoctorRun { code, checks }
    }

    /// Proves the `doctor` read THIS world's state and not another directory's. Two proofs, and
    /// the second is the one that carries the seeding:
    ///
    /// 1. `project_db` is `ok` and the path it reports lives under the tempdir root;
    /// 2. `wiring_diagnostic` counts exactly [`SEEDED_WIRING_ROWS`] row(s). The daemon creates the
    ///    databases on its own, so the file's existence (1) does not tell a seeded world from an
    ///    empty one; the count does (empty: `rows=0`; the repository's database: thousands).
    pub fn assert_state_was_read_from_this_world(&self, checks: &[serde_json::Value]) {
        let check = |name: &str| {
            checks
                .iter()
                .find(|c| c.get("name").and_then(serde_json::Value::as_str) == Some(name))
                .unwrap_or_else(|| panic!("`doctor -j` has no {name} check: {checks:?}"))
        };
        let field = |c: &serde_json::Value, key: &str| {
            c.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };

        let project_db = check("project_db");
        assert_eq!(
            field(project_db, "status"),
            "ok",
            "project_db must see the seeded symbols.db: {project_db}"
        );
        let detail = field(project_db, "detail");
        assert!(
            detail.starts_with(&self.root.to_string_lossy().into_owned()),
            "project_db read {detail:?}, not a database under this world's root {}",
            self.root.display()
        );

        let wiring = check("wiring_diagnostic");
        let wiring_detail = field(wiring, "detail");
        let rows = wiring_detail
            .split_whitespace()
            .find_map(|word| word.strip_prefix("rows="));
        assert_eq!(
            rows,
            Some(SEEDED_WIRING_ROWS.to_string().as_str()),
            "wiring_diagnostic must count exactly the seeded wiring row of this world \
             (an unseeded world reads rows=0, the repository's database thousands): {wiring}"
        );
    }
}
