//! Path resolution for [`TouringConfig`] — daemon socket + canonical DB paths.
//!
//! Groups the read-only path resolvers (`resolve_daemon_socket_path*`, the
//! `*_canonical` helpers, `ensure_dirs`, `touring_knowledge_path`) so the config
//! data model in the parent module stays focused on the struct + defaults. Split
//! out of `config.rs` (2026-07-02) along the path-resolution cohesion seam — the
//! methods stay on `TouringConfig`, so every public path is unchanged.

use super::*;

impl TouringConfig {
    /// W12.5 partial — Per-project daemon socket path resolver.
    ///
    /// Resolution chain (first match wins):
    /// 1. `TOURING_DAEMON_SOCKET` env var (explicit override, for testing)
    /// 2. Per-project walk-up: looks for `<dir>/.touring/daemon.sock` from CWD
    ///    (or `$CLAUDE_PROJECT_DIR` if set), stopping at filesystem root
    /// 3. Global default: `/tmp/touring-daemon-<uid>.sock` (matches current
    ///    production daemon spawn convention — REGRA #2.5)
    ///
    /// This is a **read-only** path resolver — does NOT spawn a daemon, does
    /// NOT bind a socket. Foundation for W12.5 full daemon multi-instance.
    ///
    /// Returns the resolved path. Never returns None; the global fallback
    /// always produces a path (whether or not a daemon is listening there
    /// is a separate runtime check).
    pub fn resolve_daemon_socket_path() -> PathBuf {
        Self::resolve_daemon_socket_path_from(
            std::env::var("CLAUDE_PROJECT_DIR")
                .ok()
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok()),
        )
    }

    /// Testable variant — caller passes an explicit start directory (or `None`
    /// to skip the walk-up layer entirely). Production calls this with the
    /// CWD walk-up start and reads the env override implicitly. Tests pass
    /// `env_override` explicitly to avoid env-var races with parallel tests.
    ///
    /// Env layering (W12.5 unification, 2026-07-24): the canonical
    /// `TOURING_DAEMON_SOCKET` wins; the legacy `TOURING_DAEMON_SOCK` is kept
    /// for back-compat with older scripts/tests — previously only the
    /// `touring-hooks-core::ipc` copy honored it, which made the "unified"
    /// resolvers semantically divergent.
    pub fn resolve_daemon_socket_path_from(start_dir: Option<PathBuf>) -> PathBuf {
        let env_override = std::env::var("TOURING_DAEMON_SOCKET")
            .ok()
            .filter(|p| !p.is_empty())
            .or_else(|| {
                std::env::var("TOURING_DAEMON_SOCK")
                    .ok()
                    .filter(|p| !p.is_empty())
            });
        Self::resolve_daemon_socket_path_inner(start_dir, env_override.as_deref())
    }

    /// W12.5 — the per-socket daemon lock path (single source of truth).
    ///
    /// The singleton guard scopes to ONE socket so N per-project daemons
    /// coexist, while two daemons racing for the SAME socket still serialize
    /// (REGRA #19 idempotent resolution). The global socket keeps the legacy
    /// uid-only lock name so a live pre-W12.5 daemon and an upgraded binary
    /// agree on the same lock file across an upgrade; every other socket
    /// derives `/tmp/touring-daemon-<uid>-<fnv1a/8hex>.lock`.
    ///
    /// FNV-1a is inlined because it is stable across rustc versions and
    /// builds — `DefaultHasher` is NOT, and two binaries disagreeing on the
    /// lock name would let two daemons bind the same socket.
    #[must_use]
    pub fn daemon_lock_path_for(socket: &std::path::Path) -> PathBuf {
        // SAFETY: getuid() is a thread-safe, infallible POSIX call (no
        // arguments, no memory effects) — the unsafe only marks the FFI edge.
        let uid = unsafe { libc::getuid() };
        let global = PathBuf::from(format!("/tmp/touring-daemon-{uid}.sock"));
        if socket == global {
            return PathBuf::from(format!("/tmp/touring-daemon-{uid}.lock"));
        }
        PathBuf::from(format!(
            "/tmp/touring-daemon-{uid}-{}.lock",
            Self::socket_hash8(socket)
        ))
    }

    /// The calling process's real UID — the single libc FFI touchpoint for
    /// every uid-derived path in this module (lock, registry, global socket),
    /// public so consumers without a direct libc dependency (e.g.
    /// touring-dispatch) stop re-declaring their own getuid externs.
    ///
    /// SAFETY (encapsulated): `getuid(2)` is a thread-safe, infallible POSIX
    /// call with no arguments and no memory effects.
    #[must_use]
    pub fn current_uid() -> u32 {
        unsafe { libc::getuid() }
    }

    /// Stable 8-hex-char FNV-1a digest of a socket path — shared by the
    /// per-socket lock name and the daemon-registry entry name so both map
    /// 1:1 to the same daemon.
    #[must_use]
    pub fn socket_hash8(socket: &std::path::Path) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a offset basis
        for byte in socket.as_os_str().as_encoded_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
        }
        format!("{:08x}", hash & 0xffff_ffff)
    }

    /// W12.5 — directory where every bound daemon registers itself
    /// (`/tmp/touring-daemons-<uid>/<hash8>.json`, written on bind).
    ///
    /// The registry is best-effort observability for `daemon-ctl list-all`:
    /// a SIGKILLed daemon leaves a stale entry behind, so READERS validate
    /// liveness (socket connect + /proc comm) and prune what is dead — the
    /// writer never has to guarantee cleanup.
    #[must_use]
    pub fn daemon_registry_dir() -> PathBuf {
        // SAFETY: getuid() — infallible, thread-safe POSIX call (FFI edge only).
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/touring-daemons-{uid}"))
    }

    /// Registry entry path for one socket (see [`Self::daemon_registry_dir`]).
    #[must_use]
    pub fn daemon_registry_entry_for(socket: &std::path::Path) -> PathBuf {
        Self::daemon_registry_dir().join(format!("{}.json", Self::socket_hash8(socket)))
    }

    /// Pure-function core — no env reads, no syscalls except `libc::getuid()`
    /// at the global-fallback path. Fully race-free for unit tests.
    pub fn resolve_daemon_socket_path_inner(
        start_dir: Option<PathBuf>,
        env_override: Option<&str>,
    ) -> PathBuf {
        // Layer 1: explicit override (env var in production, explicit arg in tests)
        if let Some(p) = env_override
            && !p.is_empty()
        {
            return PathBuf::from(p);
        }

        // Layer 2: walk-up looking for a per-project daemon.
        //
        // Two ways a directory claims one (W12.5 1.5, 2026-07-24):
        //   a) `<dir>/.touring/daemon.sock` already EXISTS (a daemon bound it);
        //   b) `<dir>/.touring/touring.toml` opts in with
        //      `[daemon] per_project = true` — the socket path is returned even
        //      BEFORE any daemon bound it, so the very first client resolves
        //      per-project and its autostart spawns the daemon THERE (breaks
        //      the chicken-and-egg where opt-in only worked once a socket
        //      already existed).
        if let Some(mut dir) = start_dir {
            loop {
                let candidate = dir.join(".touring").join("daemon.sock");
                if candidate.exists() || Self::daemon_per_project_opt_in(&dir) {
                    return candidate;
                }
                if !dir.pop() {
                    break;
                }
            }
        }

        // Layer 3: global fallback /tmp/touring-daemon-<uid>.sock
        // (matches the convention used by `touring-hook --start-daemon` per
        // REGRA #2.5 — keeps backward compatibility with running daemon)
        let uid = unsafe {
            // SAFETY: getuid() is a thread-safe POSIX call that takes no
            // arguments, has no preconditions, and only returns the calling
            // process's real UID. No state mutation. Always safe.
            libc::getuid()
        };
        PathBuf::from(format!("/tmp/touring-daemon-{uid}.sock"))
    }

    /// W12.5 (1.5) — does `<dir>/.touring/touring.toml` opt in to a
    /// per-project daemon (`[daemon] per_project = true`)?
    ///
    /// Parsed as a generic `toml::Value` on purpose: the opt-in must be
    /// readable without dragging the full `TouringConfig` schema (and its
    /// defaults) into the hot socket-resolution path, and a malformed file
    /// must never panic — any parse failure reads as "no opt-in" (fail-open,
    /// the global daemon keeps working).
    #[must_use]
    pub fn daemon_per_project_opt_in(dir: &std::path::Path) -> bool {
        let toml_path = dir.join(".touring").join("touring.toml");
        let Ok(text) = std::fs::read_to_string(&toml_path) else {
            return false;
        };
        let Ok(value) = text.parse::<toml::Value>() else {
            return false;
        };
        value
            .get("daemon")
            .and_then(|d| d.get("per_project"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false)
    }

    /// touring_knowledge.db is always local to the project.
    /// None if the path has not been set (before detect_tiered is called).
    #[must_use]
    pub fn touring_knowledge_path(&self) -> Option<&PathBuf> {
        self.touring_knowledge_path.as_ref()
    }

    /// F0-pre (2026-07-20): normalize a raw client cwd into a REAL project root.
    ///
    /// Per-project state was historically keyed on the client's raw
    /// `current_dir()`, so every working directory (a skill's `scripts/` dir, a
    /// crate subdir) spawned its own stray `.claude/touring/` shard — the
    /// "29 stray DBs" class. This walk-up resolves any cwd to the nearest
    /// enclosing REAL project root:
    ///
    /// 1. Walk up from `cwd` (inclusive), returning the first directory holding
    ///    a project marker: `.touring/` (explicit init-project) · `.git/` ·
    ///    `Cargo.toml` containing `[workspace]`. `.claude/` is deliberately NOT
    ///    a marker — treating it as one is what created the strays.
    /// 2. The walk never crosses `$HOME`: reaching home without a marker (or
    ///    starting outside home and exhausting the path) falls back to `$HOME`,
    ///    whose canonical store is the global `~/.claude/touring/`.
    #[must_use]
    pub fn normalize_project_root(cwd: &std::path::Path) -> PathBuf {
        let home = std::env::var("HOME").ok().map(PathBuf::from);
        Self::normalize_project_root_inner(cwd, home.as_deref())
    }

    /// Pure walk core — `home` passed explicitly so tests avoid env races.
    #[must_use]
    pub fn normalize_project_root_inner(
        cwd: &std::path::Path,
        home: Option<&std::path::Path>,
    ) -> PathBuf {
        let fallback = || {
            home.map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| cwd.to_path_buf())
        };
        // An empty or relative cwd carries no project information — resolving
        // markers against it would silently anchor on the DAEMON's own cwd
        // (live incident 2026-07-20: touring-hook sent project_root="" and the
        // relative `.touring` check matched the daemon's ~/.claude/rust cwd,
        // stranding rows in the rust shard). No info → the global store.
        if !cwd.is_absolute() {
            return fallback();
        }
        // `$HOME/.claude` is the harness CONFIG directory, never a project — it
        // commonly carries its own `.git` (versioned dotfiles), which would
        // otherwise promote it to a root and mint the pathological
        // `~/.claude/.claude/touring/` shard (observed live 2026-07-20).
        // Projects INSIDE it (e.g. `~/.claude/rust` with `.touring/`) still win
        // because the walk reaches them before it reaches `~/.claude`.
        let harness_config: Option<PathBuf> = home.map(|h| h.join(".claude"));
        let mut dir = cwd.to_path_buf();
        loop {
            let is_harness_config = harness_config.as_deref().is_some_and(|c| dir == c);
            if !is_harness_config && Self::has_project_marker(&dir) {
                return dir;
            }
            if home.is_some_and(|h| dir == h) {
                return fallback();
            }
            if !dir.pop() {
                return fallback();
            }
        }
    }

    /// A directory is a project root iff it holds one of the REAL markers.
    fn has_project_marker(dir: &std::path::Path) -> bool {
        if dir.join(".touring").is_dir() || dir.join(".git").is_dir() {
            return true;
        }
        let cargo = dir.join("Cargo.toml");
        cargo.is_file()
            && std::fs::read_to_string(&cargo)
                .map(|text| text.contains("[workspace]"))
                .unwrap_or(false)
    }

    /// Canonical path for the per-project symbols DB.
    ///
    /// Shared by hooks, server, and Python indexer.
    /// Always resolves to `<project_root>/.claude/touring/symbols.db`.
    #[must_use]
    pub fn symbols_db_canonical(project_root: &std::path::Path) -> PathBuf {
        // When project_root is "." (relative default), avoid the "./.claude" prefix
        // that PathBuf::join produces — match the same format as Default::default().
        if project_root == std::path::Path::new(".") {
            PathBuf::from(".claude").join("touring").join("symbols.db")
        } else {
            project_root
                .join(".claude")
                .join("touring")
                .join("symbols.db")
        }
    }

    /// Canonical path for the per-project consolidated knowledge DB.
    ///
    /// Always resolves to `<project_root>/.claude/touring/knowledge.db`.
    /// Replaces the legacy `touring_knowledge.db` in `.claude/data/`.
    #[must_use]
    pub fn knowledge_db_canonical(project_root: &std::path::Path) -> PathBuf {
        if project_root == std::path::Path::new(".") {
            PathBuf::from(".claude")
                .join("touring")
                .join("knowledge.db")
        } else {
            project_root
                .join(".claude")
                .join("touring")
                .join("knowledge.db")
        }
    }

    /// Canonical path for the consolidated memory DB.
    ///
    /// Always resolves to `<project_root>/.claude/touring/memory.db`.
    /// Replaces legacy `rlm_memory.db`, `touring_rlm.db`, and `ann_memory.db`.
    #[must_use]
    pub fn memory_db_canonical(project_root: &std::path::Path) -> PathBuf {
        if project_root == std::path::Path::new(".") {
            PathBuf::from(".claude").join("touring").join("memory.db")
        } else {
            project_root
                .join(".claude")
                .join("touring")
                .join("memory.db")
        }
    }

    /// Canonical path for the per-project consolidated graph DB.
    ///
    /// Always resolves to `<project_root>/.claude/touring/graph.db`.
    /// Replaces legacy `touring_pipeline.db` and `got_snapshots.db`.
    #[must_use]
    pub fn graph_db_canonical(project_root: &std::path::Path) -> PathBuf {
        if project_root == std::path::Path::new(".") {
            PathBuf::from(".claude").join("touring").join("graph.db")
        } else {
            project_root
                .join(".claude")
                .join("touring")
                .join("graph.db")
        }
    }

    /// Canonical path for the durable action-outcome world model snapshot (ES4 P1).
    ///
    /// Always resolves to `<project_root>/.claude/touring/action_world_model.json`.
    /// Holds the JSON-safe snapshot of the process-global `LearnedOutcomeModel`
    /// (X4 PREDICT data source) so a restarted daemon warm-loads accumulated
    /// outcome history instead of predicting a flat `0.5` cold-start prior.
    #[must_use]
    pub fn world_model_canonical(project_root: &std::path::Path) -> PathBuf {
        if project_root == std::path::Path::new(".") {
            PathBuf::from(".claude")
                .join("touring")
                .join("action_world_model.json")
        } else {
            project_root
                .join(".claude")
                .join("touring")
                .join("action_world_model.json")
        }
    }

    /// Ensure database directories exist.
    pub fn ensure_dirs(&self) -> crate::Result<()> {
        for path in [
            &self.symbols_db_path,
            &self.rlm_db_path,
            &self.semantic_db_path,
        ] {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }
        if let Some(ref knowledge_path) = self.touring_knowledge_path
            && let Some(parent) = knowledge_path.parent()
        {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod w12_5_daemon_paths_tests {
    use super::*;

    fn global_socket() -> PathBuf {
        // SAFETY: getuid() — infallible, thread-safe POSIX call (FFI edge only).
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/touring-daemon-{uid}.sock"))
    }

    #[test]
    fn lock_for_global_socket_keeps_legacy_name() {
        // A live pre-W12.5 daemon and an upgraded binary must agree on the
        // same lock file, or an upgrade would let two daemons bind the socket.
        let lock = TouringConfig::daemon_lock_path_for(&global_socket());
        let uid = unsafe { libc::getuid() };
        assert_eq!(
            lock,
            PathBuf::from(format!("/tmp/touring-daemon-{uid}.lock"))
        );
    }

    #[test]
    fn lock_for_custom_socket_is_derived_stable_and_distinct() {
        let a = TouringConfig::daemon_lock_path_for(std::path::Path::new(
            "/proj/a/.touring/daemon.sock",
        ));
        let b = TouringConfig::daemon_lock_path_for(std::path::Path::new(
            "/proj/b/.touring/daemon.sock",
        ));
        assert_ne!(a, b, "distinct sockets must derive distinct locks");
        assert_ne!(a, TouringConfig::daemon_lock_path_for(&global_socket()));
        // Deterministic across calls (FNV-1a is build/version-stable).
        assert_eq!(
            a,
            TouringConfig::daemon_lock_path_for(std::path::Path::new(
                "/proj/a/.touring/daemon.sock"
            ))
        );
        assert!(a.to_string_lossy().ends_with(".lock"));
    }

    #[test]
    fn opt_in_toml_resolves_socket_before_it_exists() {
        // 1.5: the chicken-and-egg breaker — `[daemon] per_project = true`
        // resolves the per-project socket even though no daemon bound it yet.
        let tmp = tempfile::tempdir().expect("tmp");
        let proj = tmp.path().join("proj");
        std::fs::create_dir_all(proj.join(".touring")).expect("mkdir");
        std::fs::write(
            proj.join(".touring/touring.toml"),
            "[daemon]\nper_project = true\n",
        )
        .expect("write");
        let sub = proj.join("deep/sub");
        std::fs::create_dir_all(&sub).expect("mkdir");
        let got = TouringConfig::resolve_daemon_socket_path_inner(Some(sub), None);
        assert_eq!(got, proj.join(".touring/daemon.sock"));
    }

    #[test]
    fn without_opt_in_or_socket_falls_back_to_global() {
        let tmp = tempfile::tempdir().expect("tmp");
        let proj = tmp.path().join("proj");
        std::fs::create_dir_all(proj.join(".touring")).expect("mkdir");
        // touring.toml present but NOT opting in (default OFF — 1.5).
        std::fs::write(
            proj.join(".touring/touring.toml"),
            "[toolchain]\nchannel = \"30.3.0\"\n",
        )
        .expect("write");
        let got = TouringConfig::resolve_daemon_socket_path_inner(Some(proj.clone()), None);
        assert_eq!(
            got,
            global_socket(),
            "no opt-in must keep the global daemon"
        );
        // Malformed toml is fail-open (never a panic, never an opt-in).
        std::fs::write(proj.join(".touring/touring.toml"), "[[[not toml").expect("write");
        assert!(!TouringConfig::daemon_per_project_opt_in(&proj));
    }
}

#[cfg(test)]
mod normalize_project_root_tests {
    use super::*;
    use std::path::Path;

    fn mkdirs(root: &Path, rel: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    #[test]
    fn cwd_without_marker_falls_back_to_home() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        let deep = mkdirs(home, ".claude/skills/Touring/scripts");
        let got = TouringConfig::normalize_project_root_inner(&deep, Some(home));
        assert_eq!(got, home, "scripts dir has no marker → global (home)");
    }

    #[test]
    fn dot_claude_is_never_a_marker() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        let inside = mkdirs(home, ".claude/rust-no-marker/sub");
        mkdirs(home, ".claude/rust-no-marker/.claude");
        let got = TouringConfig::normalize_project_root_inner(&inside, Some(home));
        assert_eq!(
            got, home,
            ".claude/ presence must not promote a dir to project"
        );
    }

    #[test]
    fn versioned_home_dot_claude_is_not_a_project() {
        // Live incident 2026-07-20: ~/.claude carries .git (versioned dotfiles);
        // the walk promoted it to a project root and minted the pathological
        // ~/.claude/.claude/touring/ shard. The harness config dir must always
        // resolve to home (the global store).
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        mkdirs(home, ".claude/.git");
        let scripts = mkdirs(home, ".claude/skills/Touring/scripts");
        let got = TouringConfig::normalize_project_root_inner(&scripts, Some(home));
        assert_eq!(
            got, home,
            "~/.claude with .git must not become a project root"
        );
    }

    #[test]
    fn project_inside_dot_claude_with_own_marker_still_wins() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        mkdirs(home, ".claude/.git");
        let rust = mkdirs(home, ".claude/rust");
        mkdirs(home, ".claude/rust/.touring");
        let member = mkdirs(home, ".claude/rust/crates/foo");
        let got = TouringConfig::normalize_project_root_inner(&member, Some(home));
        assert_eq!(
            got, rust,
            "a real project inside ~/.claude keeps its own root"
        );
    }

    #[test]
    fn dot_touring_marks_a_project() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        let proj = mkdirs(home, "work/proj");
        mkdirs(home, "work/proj/.touring");
        let sub = mkdirs(home, "work/proj/deep/sub");
        let got = TouringConfig::normalize_project_root_inner(&sub, Some(home));
        assert_eq!(got, proj);
    }

    #[test]
    fn git_dir_marks_a_project() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        let proj = mkdirs(home, "projects/analise");
        mkdirs(home, "projects/analise/.git");
        let got = TouringConfig::normalize_project_root_inner(&proj, Some(home));
        assert_eq!(got, proj);
    }

    #[test]
    fn workspace_cargo_toml_marks_and_member_routes_to_workspace() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        let ws = mkdirs(home, "rustws");
        std::fs::write(ws.join("Cargo.toml"), "[workspace]\nmembers=[]\n").expect("write");
        let member = mkdirs(home, "rustws/crates/foo");
        std::fs::write(member.join("Cargo.toml"), "[package]\nname=\"foo\"\n").expect("write");
        let got = TouringConfig::normalize_project_root_inner(&member, Some(home));
        assert_eq!(got, ws, "member crate (non-workspace Cargo.toml) routes up");
    }

    #[test]
    fn home_itself_resolves_to_home() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        let got = TouringConfig::normalize_project_root_inner(home, Some(home));
        assert_eq!(got, home);
    }

    #[test]
    fn outside_home_without_marker_falls_back_to_home() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = mkdirs(tmp.path(), "home");
        let outside = mkdirs(tmp.path(), "elsewhere/deep");
        let got = TouringConfig::normalize_project_root_inner(&outside, Some(&home));
        assert_eq!(got, home);
    }

    #[test]
    fn no_home_degrades_to_cwd() {
        let tmp = tempfile::tempdir().expect("tmp");
        let cwd = mkdirs(tmp.path(), "nowhere");
        let got = TouringConfig::normalize_project_root_inner(&cwd, None);
        assert_eq!(got, cwd);
    }

    #[test]
    fn empty_or_relative_cwd_falls_back_to_home() {
        // Live incident 2026-07-20: project_root="" resolved relative markers
        // against the daemon's own cwd, stranding rows in a foreign shard.
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        let empty = TouringConfig::normalize_project_root_inner(Path::new(""), Some(home));
        assert_eq!(empty, home, "empty cwd must resolve to the global (home)");
        let relative =
            TouringConfig::normalize_project_root_inner(Path::new("some/rel/dir"), Some(home));
        assert_eq!(
            relative, home,
            "relative cwd must resolve to the global (home)"
        );
    }
}
