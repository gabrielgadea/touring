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

    /// The Claude Code project slug of a path: `/` becomes `-`, leading `-` kept
    /// (`/home/u/projects/x` → `-home-u-projects-x`). It names the memory
    /// directory `~/.claude/projects/<slug>/memory` — the one place the auto-memory
    /// of a project lives (98 files for touring, 606 for analise, 145 for `~`,
    /// measured 13/09/2026), and until this reader nothing indexed it.
    #[must_use]
    pub fn claude_project_slug(path: &std::path::Path) -> String {
        path.to_string_lossy().replace('/', "-")
    }

    /// The companion roots of `project_root` — directories OUTSIDE the project
    /// whose files the index carries under the stable key
    /// `@companion/<name>/<relative path>`: the `[index.companion_roots]` table of
    /// `.touring/touring.toml` (`name = "path"`; `~` expands to `$HOME`, `{slug}`
    /// to the project's Claude Code slug) layered over the defaults — `rules`,
    /// `commands`, `agents`, `skills` under `~/.claude`, `memory` (this
    /// project's auto-memory) and `memory-home` (the memory of `~` as a project)
    /// — unless `[index] companion_defaults = false`. Names are the key alphabet
    /// (`[A-Za-z0-9_-]`); an invalid name or a root missing on disk is dropped,
    /// and a malformed file yields the defaults (fail-open, like the daemon
    /// opt-in above). Sorted by name so every walk sees the same order.
    ///
    /// A root without a `.touring/` directory has NO companions: that directory
    /// is where the index is configured, and a scratch directory handed to a
    /// rebuild (every test fixture, `touring index rebuild --dir /tmp/x`) must
    /// not pull the whole of `~/.claude/skills` into its store by default.
    #[must_use]
    pub fn companion_roots_for(project_root: &std::path::Path) -> Vec<CompanionRoot> {
        if !project_root.join(".touring").is_dir() {
            return Vec::new();
        }
        let home = std::env::var("HOME").ok().map(PathBuf::from);
        let slug = Self::claude_project_slug(project_root);
        let expand = |raw: &str| -> Option<PathBuf> {
            let with_slug = raw.replace("{slug}", &slug);
            if let Some(rest) = with_slug.strip_prefix("~/") {
                return home.as_ref().map(|h| h.join(rest));
            }
            if with_slug == "~" {
                return home.clone();
            }
            Some(PathBuf::from(with_slug))
        };
        let mut roots: std::collections::BTreeMap<String, PathBuf> =
            std::collections::BTreeMap::new();
        let toml_path = project_root.join(".touring").join("touring.toml");
        let value = std::fs::read_to_string(&toml_path)
            .ok()
            .and_then(|t| t.parse::<toml::Value>().ok());
        let index = value.as_ref().and_then(|v| v.get("index"));
        let defaults_on = index
            .and_then(|i| i.get("companion_defaults"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(true);
        if defaults_on && let Some(h) = home.as_ref() {
            for name in ["rules", "commands", "agents", "skills"] {
                roots.insert(name.to_string(), h.join(".claude").join(name));
            }
            roots.insert(
                "memory".to_string(),
                h.join(".claude")
                    .join("projects")
                    .join(&slug)
                    .join("memory"),
            );
            let home_slug = Self::claude_project_slug(h);
            if home_slug != slug {
                roots.insert(
                    "memory-home".to_string(),
                    h.join(".claude")
                        .join("projects")
                        .join(home_slug)
                        .join("memory"),
                );
            }
        }
        if let Some(table) = index
            .and_then(|i| i.get("companion_roots"))
            .and_then(toml::Value::as_table)
        {
            for (name, raw) in table {
                let Some(raw) = raw.as_str() else { continue };
                if let Some(path) = expand(raw) {
                    roots.insert(name.clone(), path);
                }
            }
        }
        roots
            .into_iter()
            .filter(|(name, path)| {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    && path.is_dir()
            })
            .map(|(name, path)| CompanionRoot { name, path })
            .collect()
    }

    /// Directories of `project_root` the index must not walk, declared by the
    /// project: `[index] exclude_dirs = ["client", "vendor/generated"]` in
    /// `.touring/touring.toml`. Each entry is a path RELATIVE to the root (never a
    /// bare name matched anywhere — a `client/` of a web app is code); leading
    /// `./` and trailing `/` are ignored, and absolute paths, `..` components and
    /// empty entries are dropped. No `.touring/` or no table → nothing excluded.
    /// Sorted and de-duplicated so every walk sees the same list.
    #[must_use]
    pub fn index_excluded_dirs_for(project_root: &std::path::Path) -> Vec<String> {
        if !project_root.join(".touring").is_dir() {
            return Vec::new();
        }
        let Some(value) =
            std::fs::read_to_string(project_root.join(".touring").join("touring.toml"))
                .ok()
                .and_then(|t| t.parse::<toml::Value>().ok())
        else {
            return Vec::new();
        };
        let mut dirs: Vec<String> = value
            .get("index")
            .and_then(|i| i.get("exclude_dirs"))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .map(|raw| {
                raw.trim()
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .to_string()
            })
            .filter(|d| {
                !d.is_empty()
                    && !std::path::Path::new(d).is_absolute()
                    && d.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
            })
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
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
    ///
    /// `.git` may be a FILE: a linked worktree (and a submodule) carries a
    /// `gitdir:` pointer instead of the directory. Accepting only the directory
    /// resolved a worktree to its parent project, and the WorktreeCreate rebuild
    /// indexed the worktree under the parent's databases (cross-audit
    /// 14/09/2026, B6).
    fn has_project_marker(dir: &std::path::Path) -> bool {
        if dir.join(".touring").is_dir() || dir.join(".git").exists() {
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

    /// The project root a canonical knowledge/symbols DB path belongs to —
    /// the inverse of [`Self::knowledge_db_canonical`].
    ///
    /// It lives beside its mirror image on purpose: whoever changes the layout
    /// `<root>/.claude/touring/<name>.db` has to change both, and having them
    /// in two crates is how they drift.
    ///
    /// Used to canonicalize wiring paths against the root of the DATABASE
    /// rather than of the process. Until 2026-08-19 that root came from
    /// `TOURING_WORKSPACE_ROOT`, which every per-project daemon inherits from
    /// the session that spawned it — so one project's paths were normalized
    /// against another's root, and a DELETE keyed to the same marker was aimed
    /// at the wrong project's rows.
    ///
    /// Both directory names are CHECKED rather than assumed: three components
    /// up from an arbitrary path is meaningless, and for a shallow path it is
    /// `/`, which would make every absolute path "relative". A relative input
    /// resolves against the current directory, because that is the shape the
    /// daemon actually opens (`.claude/touring/knowledge.db` with its cwd
    /// pinned to the project). `$HOME` is refused: `~/.claude/touring/*.db` is
    /// the GLOBAL store, whose rows span projects and have no single root.
    ///
    /// Returns the root WITH a trailing separator, ready for `strip_prefix`,
    /// or `None` when no root can be derived.
    #[must_use]
    pub fn project_root_for_db(db_path: &std::path::Path) -> Option<String> {
        let resolved;
        let db_path = if db_path.is_absolute() {
            db_path
        } else {
            resolved = std::env::current_dir().ok()?.join(db_path);
            resolved.as_path()
        };
        let touring_dir = db_path.parent()?;
        if touring_dir.file_name()? != "touring" {
            return None;
        }
        let claude_dir = touring_dir.parent()?;
        if claude_dir.file_name()? != ".claude" {
            return None;
        }
        let root = claude_dir.parent()?;
        if root.as_os_str().is_empty() {
            return None;
        }
        if std::env::var_os("HOME").is_some_and(|home| root == std::path::Path::new(&home)) {
            return None;
        }
        let mut root = root.to_string_lossy().into_owned();
        if !root.ends_with('/') {
            root.push('/');
        }
        Some(root)
    }

    /// Whether non-Rust source participates in the wiring graph **for one
    /// project root**.
    ///
    /// The flag was a process-global `OnceLock` over `TOURING_POLYGLOT_WIRING`,
    /// which forced an all-or-nothing answer: a Python codebase either got
    /// Rust-only wiring, or every project served by the machine flipped at
    /// once. But whether Python counts as wiring is a property of the PROJECT,
    /// exactly as its canonical root is — so it is resolved from the project,
    /// per database.
    ///
    /// Order: `TOURING_POLYGLOT_WIRING` (the historical escape hatch — an
    /// explicit env override still wins, which is what keeps the polyglot PoC
    /// tests and CI deterministic) → `polyglot_wiring` in the Project layer
    /// (`<root>/.touring/touring.toml`) → User layer → `false`.
    ///
    /// `None` root (in-memory DB, non-canonical layout) resolves to the env
    /// alone, then `false`. Reading up to three TOML files is fine because
    /// callers resolve this ONCE per database open and keep the `bool` — the
    /// write gate runs per symbol and must stay a field read.
    #[must_use]
    pub fn polyglot_wiring_for_root(project_root: Option<&std::path::Path>) -> bool {
        if let Ok(raw) = std::env::var("TOURING_POLYGLOT_WIRING") {
            return raw == "1" || raw.eq_ignore_ascii_case("true");
        }
        let Some(root) = project_root else {
            return false;
        };
        let system = Some(PathBuf::from("/etc/touring/config.toml"));
        let user = std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".touring").join("config.toml"));
        let project = Some(root.join(".touring").join("touring.toml"));
        // Layer reads fail OPEN (missing/malformed contributes nothing), so a
        // broken toml can never turn wiring on by accident.
        TouringConfig::detect_layered_from(system, user, project)
            .map(|c| c.polyglot_wiring)
            .unwrap_or(false)
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
    fn a_linked_worktree_is_its_own_project_not_its_parent() {
        let tmp = tempfile::tempdir().expect("tmp");
        let home = tmp.path();
        mkdirs(home, "proj/.git");
        let worktree = mkdirs(home, "proj/.worktrees/feature");
        std::fs::write(
            worktree.join(".git"),
            "gitdir: ../../.git/worktrees/feature\n",
        )
        .expect(".git file");
        let inside = mkdirs(home, "proj/.worktrees/feature/src");
        let got = TouringConfig::normalize_project_root_inner(&inside, Some(home));
        assert_eq!(got, worktree, "the `.git` FILE marks the worktree root");
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

#[cfg(test)]
mod project_root_for_db_tests {
    use super::TouringConfig;
    use std::path::Path;

    #[test]
    fn inverts_knowledge_db_canonical() {
        // The property that matters: build a path from a root, get the root back.
        let root = Path::new("/home/u/projects/app");
        let db = TouringConfig::knowledge_db_canonical(root);
        assert_eq!(
            TouringConfig::project_root_for_db(&db),
            Some("/home/u/projects/app/".to_string())
        );
    }

    #[test]
    fn resolves_a_relative_db_path_against_the_current_directory() {
        // The daemon opens `.claude/touring/knowledge.db` with its cwd pinned to
        // the project root — the only shape production uses, and the one the
        // first cut of this rule rejected (three components up from a relative
        // path is the empty path), so the migration silently did nothing.
        let cwd = std::env::current_dir().expect("cwd");
        assert_eq!(
            TouringConfig::project_root_for_db(Path::new(".claude/touring/knowledge.db")),
            Some(format!("{}/", cwd.display()))
        );
    }

    #[test]
    fn refuses_the_global_store_under_home() {
        let home = std::env::var("HOME").expect("HOME");
        let global = format!("{home}/.claude/touring/knowledge.db");
        assert_eq!(TouringConfig::project_root_for_db(Path::new(&global)), None);
    }

    #[test]
    fn refuses_layouts_that_are_not_dot_claude_touring() {
        assert_eq!(
            TouringConfig::project_root_for_db(Path::new("/tmp/scratch.db")),
            None
        );
        assert_eq!(
            TouringConfig::project_root_for_db(Path::new("/a/b/c/knowledge.db")),
            None
        );
        assert_eq!(
            TouringConfig::project_root_for_db(Path::new(
                "/home/u/app/config/touring/knowledge.db"
            )),
            None,
            "the parent of `touring/` must be `.claude/`"
        );
    }
}

#[cfg(test)]
mod polyglot_wiring_opt_in_tests {
    use super::TouringConfig;

    /// Writes `<root>/.touring/touring.toml` with the given body.
    fn project_with(body: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join(".touring");
        std::fs::create_dir_all(&dir).expect("mkdir .touring");
        std::fs::write(dir.join("touring.toml"), body).expect("write toml");
        tmp
    }

    // These tests read the process env, so they assert only what holds under
    // BOTH states of `TOURING_POLYGLOT_WIRING` — mutating it would be a
    // process-global side effect on a parallel test runner. When the override
    // is set, it wins by design and the project layer is not consulted; that
    // branch is asserted directly below.
    fn env_override() -> Option<bool> {
        std::env::var("TOURING_POLYGLOT_WIRING")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }

    #[test]
    fn a_project_can_say_yes_on_its_own() {
        let tmp = project_with("polyglot_wiring = true\n");
        let expected = env_override().unwrap_or(true);
        assert_eq!(
            TouringConfig::polyglot_wiring_for_root(Some(tmp.path())),
            expected,
            "the project layer is what turns polyglot wiring on for ONE project"
        );
    }

    #[test]
    fn a_project_that_says_nothing_stays_rust_only() {
        let tmp = project_with("cache_size = 10000\n");
        let expected = env_override().unwrap_or(false);
        assert_eq!(
            TouringConfig::polyglot_wiring_for_root(Some(tmp.path())),
            expected,
            "silence means Rust-only — the 258-false-positive default"
        );
    }

    #[test]
    fn no_root_resolves_to_the_env_alone() {
        assert_eq!(
            TouringConfig::polyglot_wiring_for_root(None),
            env_override().unwrap_or(false)
        );
    }

    #[test]
    fn a_malformed_project_toml_never_turns_it_on() {
        // Layer reads fail OPEN. Failing open must mean OFF here: a broken
        // config that silently enabled polyglot wiring would change what the
        // whole graph answers, which is the opposite of a safe default.
        let tmp = project_with("polyglot_wiring = tru\n[[[");
        let expected = env_override().unwrap_or(false);
        assert_eq!(
            TouringConfig::polyglot_wiring_for_root(Some(tmp.path())),
            expected
        );
    }

    #[test]
    fn two_projects_disagree_without_touching_each_other() {
        // The whole point of the per-project opt-in: one yes, one no, same
        // process, same instant.
        let yes = project_with("polyglot_wiring = true\n");
        let no = project_with("polyglot_wiring = false\n");
        let a = TouringConfig::polyglot_wiring_for_root(Some(yes.path()));
        let b = TouringConfig::polyglot_wiring_for_root(Some(no.path()));
        match env_override() {
            Some(forced) => {
                assert_eq!(
                    (a, b),
                    (forced, forced),
                    "an explicit env override applies to both"
                );
            }
            None => assert!(a && !b, "each project answers for itself: got ({a}, {b})"),
        }
    }
}

#[cfg(test)]
mod companion_roots_tests {
    use super::TouringConfig;
    use std::path::{Path, PathBuf};

    /// A project root with `<root>/.touring/touring.toml` holding `body`.
    fn project_with(body: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join(".touring");
        std::fs::create_dir_all(&dir).expect("mkdir .touring");
        std::fs::write(dir.join("touring.toml"), body).expect("write toml");
        tmp
    }

    fn names(roots: &[super::CompanionRoot]) -> Vec<&str> {
        roots.iter().map(|r| r.name.as_str()).collect()
    }

    fn path_of<'a>(roots: &'a [super::CompanionRoot], name: &str) -> Option<&'a Path> {
        roots
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.path.as_path())
    }

    // These tests never mutate `HOME` (process-global on a parallel runner):
    // the defaults are asserted only through properties that hold whatever
    // the home directory contains, and the explicit table is asserted exactly.

    #[test]
    fn an_explicit_table_with_defaults_off_is_the_whole_answer_sorted_by_name() {
        let tmp = project_with("[index]\ncompanion_defaults = false\n");
        let notes = tmp.path().join("notes");
        let zed = tmp.path().join("zed");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::create_dir_all(&zed).unwrap();
        std::fs::write(
            tmp.path().join(".touring/touring.toml"),
            format!(
                "[index]\ncompanion_defaults = false\n[index.companion_roots]\nzed = \"{}\"\nnotes = \"{}\"\n",
                zed.display(),
                notes.display()
            ),
        )
        .unwrap();
        let roots = TouringConfig::companion_roots_for(tmp.path());
        assert_eq!(
            names(&roots),
            vec!["notes", "zed"],
            "sorted by name, nothing else"
        );
        assert_eq!(path_of(&roots, "notes"), Some(notes.as_path()));
        assert_eq!(path_of(&roots, "zed"), Some(zed.as_path()));
    }

    #[test]
    fn a_missing_directory_and_an_invalid_name_are_dropped() {
        let tmp = project_with("");
        let here = tmp.path().join("here");
        std::fs::create_dir_all(&here).unwrap();
        std::fs::write(
            tmp.path().join(".touring/touring.toml"),
            format!(
                "[index]\ncompanion_defaults = false\n[index.companion_roots]\nhere = \"{}\"\ngone = \"{}\"\n\"bad name\" = \"{}\"\n\"a/b\" = \"{}\"\n",
                here.display(),
                tmp.path().join("does-not-exist").display(),
                here.display(),
                here.display()
            ),
        )
        .unwrap();
        let roots = TouringConfig::companion_roots_for(tmp.path());
        assert_eq!(
            names(&roots),
            vec!["here"],
            "a root missing on disk and a name outside the key alphabet never reach the walker"
        );
    }

    #[test]
    fn slug_and_tilde_expand_in_explicit_paths() {
        let tmp = project_with("");
        let slug = TouringConfig::claude_project_slug(tmp.path());
        assert!(
            !slug.contains('/'),
            "the slug is the path with every `/` turned into `-`: {slug}"
        );
        let by_slug = tmp.path().join("store").join(&slug).join("memory");
        std::fs::create_dir_all(&by_slug).unwrap();
        std::fs::write(
            tmp.path().join(".touring/touring.toml"),
            format!(
                "[index]\ncompanion_defaults = false\n[index.companion_roots]\nmem = \"{}/store/{{slug}}/memory\"\n",
                tmp.path().display()
            ),
        )
        .unwrap();
        let roots = TouringConfig::companion_roots_for(tmp.path());
        assert_eq!(
            path_of(&roots, "mem"),
            Some(by_slug.as_path()),
            "`{{slug}}` expands to this project's slug"
        );

        // `~` expands to HOME; the root is kept exactly when that directory exists.
        std::fs::write(
            tmp.path().join(".touring/touring.toml"),
            "[index]\ncompanion_defaults = false\n[index.companion_roots]\nhome = \"~\"\n",
        )
        .unwrap();
        let roots = TouringConfig::companion_roots_for(tmp.path());
        match std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .filter(|h| h.is_dir())
        {
            Some(home) => assert_eq!(path_of(&roots, "home"), Some(home.as_path())),
            None => assert!(roots.is_empty(), "no HOME, no `~` root"),
        }
    }

    #[test]
    fn the_defaults_are_named_roots_under_home_that_exist_on_disk() {
        let tmp = project_with("");
        let roots = TouringConfig::companion_roots_for(tmp.path());
        let home = std::env::var("HOME").ok().map(PathBuf::from);
        for root in &roots {
            assert!(
                root.path.is_dir(),
                "{}: every companion root exists on disk",
                root.path.display()
            );
            assert!(
                [
                    "rules",
                    "commands",
                    "agents",
                    "skills",
                    "memory",
                    "memory-home"
                ]
                .contains(&root.name.as_str()),
                "unexpected default name {}",
                root.name
            );
            let home = home.as_ref().expect("a default root implies HOME was set");
            assert!(
                root.path.starts_with(home.join(".claude")),
                "{} lives under ~/.claude",
                root.path.display()
            );
        }
        if let Some(memory) = path_of(&roots, "memory") {
            let slug = TouringConfig::claude_project_slug(tmp.path());
            assert!(
                memory.ends_with(Path::new("projects").join(slug).join("memory")),
                "the project memory is keyed by its slug: {}",
                memory.display()
            );
        }
        // Sorted by name, so every walk sees the same order.
        let mut sorted = names(&roots);
        sorted.sort_unstable();
        assert_eq!(names(&roots), sorted);
    }

    #[test]
    fn an_explicit_entry_overrides_a_default_of_the_same_name() {
        let tmp = project_with("");
        let mine = tmp.path().join("my-rules");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(
            tmp.path().join(".touring/touring.toml"),
            format!("[index.companion_roots]\nrules = \"{}\"\n", mine.display()),
        )
        .unwrap();
        let roots = TouringConfig::companion_roots_for(tmp.path());
        assert_eq!(
            path_of(&roots, "rules"),
            Some(mine.as_path()),
            "the project's own `rules` wins over `~/.claude/rules`"
        );
    }

    #[test]
    fn a_root_without_touring_dir_has_no_companions_at_all() {
        // No `.touring/` → no index configuration → nothing beyond the root
        // itself, whatever HOME holds. This is what keeps every scratch root
        // (test fixtures, `rebuild --dir /tmp/x`) from walking `~/.claude`.
        let bare = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(bare.path().join(".claude")).unwrap();
        assert!(TouringConfig::companion_roots_for(bare.path()).is_empty());
    }

    #[test]
    fn excluded_dirs_are_root_relative_normalized_and_sanitized() {
        let tmp = project_with(
            "[index]\nexclude_dirs = [\"./client/\", \"vendor/generated\", \"client\", \"/etc\", \"../up\", \"a/../b\", \"\"]\n",
        );
        assert_eq!(
            TouringConfig::index_excluded_dirs_for(tmp.path()),
            vec!["client".to_string(), "vendor/generated".to_string()]
        );
        let bare = tempfile::tempdir().expect("tempdir");
        assert!(
            TouringConfig::index_excluded_dirs_for(bare.path()).is_empty(),
            "no .touring, no exclusions"
        );
        let broken = project_with("[index]\nexclude_dirs = [[[");
        assert!(
            TouringConfig::index_excluded_dirs_for(broken.path()).is_empty(),
            "malformed file excludes nothing"
        );
    }

    #[test]
    fn a_malformed_file_yields_the_defaults_and_no_explicit_root() {
        let broken = project_with("[index]\ncompanion_defaults = fals\n[[[");
        let clean = project_with("");
        let a = TouringConfig::companion_roots_for(broken.path());
        let b = TouringConfig::companion_roots_for(clean.path());
        assert_eq!(
            names(&a),
            names(&b),
            "fail-open means the defaults, exactly as an empty file would give them"
        );
    }
}
