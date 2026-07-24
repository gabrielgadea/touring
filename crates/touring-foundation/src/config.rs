//! Configuration for the Touring runtime.
//!
//! Loads from `.claude/touring/config.toml` if present, otherwise uses defaults.
//! All paths are relative to the project root.
//!
//! **W12.4 layered loader** (2026-05-23): `detect_layered()` reads in precedence
//! order Hardcoded < `/etc/touring/config.toml` (System) < `~/.touring/config.toml`
//! (User) < `.touring/touring.toml` (Project, walk-up). Last-write-wins per key
//! via `toml::Value` recursive merge.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Detection/loading (`detect`, `detect_tiered`, layered loader) and path
// resolution (`*_canonical`, daemon socket, `ensure_dirs`) are split into
// cohesive submodules; the methods stay on `TouringConfig`, so every public
// path is unchanged (2026-07-02 cohesion split of the 1136-LOC config.rs).
mod detect;
mod paths;

/// Global configuration for the Touring runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TouringConfig {
    /// Root directory of the project being analyzed.
    pub project_root: PathBuf,

    /// SQLite database path for symbols.
    pub symbols_db_path: PathBuf,

    /// RLM memory database path.
    pub rlm_db_path: PathBuf,

    /// Semantic recall database path.
    pub semantic_db_path: PathBuf,

    /// touring_knowledge.db path (always local, never global).
    /// None until detect_tiered() is called.
    #[serde(skip)]
    pub touring_knowledge_path: Option<PathBuf>,

    /// ANN memory recall database path (always local — `.claude/data/ann_memory.db`).
    /// None until HookRuntime::init_ann_memory() is called.
    #[serde(skip)]
    pub ann_memory_path: Option<PathBuf>,

    /// knowledge.db — symbols AST + file knowledge + wiring (ALWAYS LOCAL).
    /// Consolidates: symbols.db + touring_knowledge.db + wiring tables.
    #[serde(default = "default_knowledge_db_path")]
    pub knowledge_db_path: PathBuf,

    /// memory.db — RLM episodic + semantic recall + ANN embeddings (local-first, global fallback).
    /// Consolidates: rlm_memory.db + touring_rlm.db + semantic_recall.db + ann_memory.db.
    #[serde(default = "default_memory_db_path")]
    pub memory_db_path: PathBuf,

    /// graph.db — GoT sessions + RL pipeline + hook events (ALWAYS LOCAL).
    /// Consolidates: got_snapshots.db + touring_pipeline.db (learning tables) + hook_events.
    #[serde(default = "default_graph_db_path")]
    pub graph_db_path: PathBuf,

    /// LRU cache size (in entries).
    pub cache_size: usize,

    /// File watcher debounce interval (ms).
    pub watcher_debounce_ms: u64,

    /// Maximum file size to parse (bytes).
    pub max_file_size: usize,

    /// Enable debug logging.
    pub debug: bool,

    /// GPU embedding service URL (default: `http://localhost:8200`).
    #[serde(default = "default_gpu_url")]
    pub gpu_service_url: String,

    /// Embedding dimension (must match GPU service model, default: 384).
    #[serde(default = "default_embedding_dim")]
    pub embedding_dim: usize,

    /// Auto-generate embeddings on `store()` (default: true).
    #[serde(default = "default_auto_embed")]
    pub auto_embed: bool,

    /// Enable JSONL file watcher for live ingestion (default: true).
    #[serde(default = "default_jsonl_watch_enabled")]
    pub jsonl_watch_enabled: bool,

    /// JSONL watcher poll interval in seconds (default: 30).
    #[serde(default = "default_jsonl_poll_interval")]
    pub jsonl_poll_interval_s: u64,

    /// Enable evolution engine periodic analysis (default: true).
    #[serde(default = "default_evolution_enabled")]
    pub evolution_enabled: bool,

    /// Evolution analysis interval in seconds (default: 300 = 5 min).
    #[serde(default = "default_evolution_interval")]
    pub evolution_interval_s: u64,

    /// W-F0.2 (Productization Fase 0) — per-project toolchain pin from the
    /// Project layer (`[toolchain]` in `.touring/touring.toml`, rustup-style).
    /// `None` when the project does not pin a toolchain (today's behaviour:
    /// the default/global toolchain). Consumed by `touring update` /
    /// `.touring/bin` resolution (Fases 2-3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<ToolchainPin>,
}

/// Per-project toolchain pin, mirroring `rust-toolchain.toml`'s `[toolchain]`
/// table: `channel` names an installed toolchain directory under
/// `~/.touring/toolchains/<channel>/`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainPin {
    /// Toolchain channel (a version like `"30.3.0"` or a named channel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

fn default_gpu_url() -> String {
    std::env::var("GPU_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8200".to_string())
}

fn default_embedding_dim() -> usize {
    384
}

fn default_auto_embed() -> bool {
    true
}

fn default_jsonl_watch_enabled() -> bool {
    true
}

fn default_jsonl_poll_interval() -> u64 {
    30
}

fn default_evolution_enabled() -> bool {
    true
}

fn default_evolution_interval() -> u64 {
    300
}

fn default_knowledge_db_path() -> PathBuf {
    PathBuf::from(".claude")
        .join("touring")
        .join("knowledge.db")
}

fn default_memory_db_path() -> PathBuf {
    PathBuf::from(".claude").join("touring").join("memory.db")
}

fn default_graph_db_path() -> PathBuf {
    PathBuf::from(".claude").join("touring").join("graph.db")
}

impl Default for TouringConfig {
    fn default() -> Self {
        let claude_dir = PathBuf::from(".claude");
        Self {
            project_root: PathBuf::from("."),
            symbols_db_path: claude_dir.join("touring").join("symbols.db"),
            rlm_db_path: claude_dir.join("data").join("rlm_memory.db"),
            semantic_db_path: claude_dir.join("data").join("semantic_recall.db"),
            touring_knowledge_path: None,
            ann_memory_path: None,
            knowledge_db_path: claude_dir.join("touring").join("knowledge.db"),
            memory_db_path: claude_dir.join("touring").join("memory.db"),
            graph_db_path: claude_dir.join("touring").join("graph.db"),
            cache_size: 10000,
            watcher_debounce_ms: 100,
            max_file_size: 5 * 1024 * 1024, // 5MB
            debug: false,
            gpu_service_url: default_gpu_url(),
            embedding_dim: default_embedding_dim(),
            auto_embed: default_auto_embed(),
            jsonl_watch_enabled: default_jsonl_watch_enabled(),
            jsonl_poll_interval_s: default_jsonl_poll_interval(),
            evolution_enabled: default_evolution_enabled(),
            evolution_interval_s: default_evolution_interval(),
            toolchain: None,
        }
    }
}

/// Directories that must NEVER be treated as a project root. Resolving a
/// relative `.claude/touring/*.db` default against them materializes ghost
/// databases (e.g. `/tmp/.claude/touring/symbols.db` with 0 symbols) that
/// the tiered resolver then prefers forever. Root cause of the 2026-06-12
/// "index counters report 0" incident: the daemon spawns detached with
/// CWD=/tmp and `CLAUDE_PROJECT_DIR` unset.
const NON_PROJECT_ROOTS: &[&str] = &["/", "/tmp", "/var/tmp"];

/// Pure project-root selection — extracted from env reads so the priority
/// chain is unit-testable without process-global env mutation.
///
/// Priority:
/// 1. `claude_dir` (`CLAUDE_PROJECT_DIR`) if it is an existing directory.
/// 2. `touring_root` (`TOURING_PROJECT_ROOT`) if it is an existing
///    directory — the daemon spawn contract already exports this.
/// 3. `cwd`, but only when it plausibly IS a project: not in
///    [`NON_PROJECT_ROOTS`] and carrying a project marker (`.claude/`,
///    `Cargo.toml`, `pyproject.toml`, `package.json` or `.git`).
/// 4. `home` — anchors at the canonical global `~/.claude/touring/*`.
fn pick_project_root(
    claude_dir: Option<PathBuf>,
    touring_root: Option<PathBuf>,
    cwd: PathBuf,
    home: Option<PathBuf>,
) -> PathBuf {
    for candidate in [claude_dir, touring_root].into_iter().flatten() {
        if candidate.is_dir() {
            return candidate;
        }
        tracing::warn!(
            path = %candidate.display(),
            "project root env var points to a missing directory — ignored"
        );
    }
    let cwd_blacklisted = NON_PROJECT_ROOTS
        .iter()
        .any(|p| cwd == std::path::Path::new(p));
    let cwd_is_project = !cwd_blacklisted
        && [
            "Cargo.toml",
            "pyproject.toml",
            "package.json",
            ".git",
            ".claude",
        ]
        .iter()
        .any(|m| cwd.join(m).exists());
    if cwd_is_project {
        return cwd;
    }
    match home {
        Some(h) => {
            tracing::warn!(
                cwd = %cwd.display(),
                home = %h.display(),
                "CWD is not a project directory — anchoring databases at $HOME"
            );
            h
        }
        None => cwd,
    }
}

/// Resolve the effective project root from the environment using the
/// 4-step chain documented on `pick_project_root`.
#[must_use]
pub fn resolve_project_root() -> PathBuf {
    pick_project_root(
        std::env::var("CLAUDE_PROJECT_DIR").ok().map(PathBuf::from),
        std::env::var("TOURING_PROJECT_ROOT")
            .ok()
            .map(PathBuf::from),
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        std::env::var("HOME").ok().map(PathBuf::from),
    )
}

#[cfg(test)]
mod tests {
    // --- pick_project_root: 2026-06-12 "index counters report 0" fix ---

    #[test]
    fn project_root_prefers_claude_project_dir() {
        let tmp = std::env::temp_dir().join("tpr_claude_dir_test");
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let picked = super::pick_project_root(
            Some(tmp.clone()),
            Some(std::path::PathBuf::from("/nonexistent-touring-root")),
            std::path::PathBuf::from("/tmp"),
            Some(std::path::PathBuf::from("/home/x")),
        );
        assert_eq!(picked, tmp);
    }

    #[test]
    fn project_root_falls_back_to_touring_project_root() {
        let tmp = std::env::temp_dir().join("tpr_touring_root_test");
        std::fs::create_dir_all(&tmp).expect("mkdir");
        let picked = super::pick_project_root(
            None,
            Some(tmp.clone()),
            std::path::PathBuf::from("/tmp"),
            Some(std::path::PathBuf::from("/home/x")),
        );
        assert_eq!(picked, tmp);
    }

    #[test]
    fn project_root_never_uses_tmp_cwd() {
        // /tmp may even contain a ghost .claude/ from the original incident —
        // the NON_PROJECT_ROOTS blacklist must win over the marker heuristic.
        let picked = super::pick_project_root(
            None,
            None,
            std::path::PathBuf::from("/tmp"),
            Some(std::path::PathBuf::from("/home/x")),
        );
        assert_eq!(picked, std::path::PathBuf::from("/home/x"));
    }

    #[test]
    fn project_root_accepts_project_like_cwd() {
        let cwd = std::env::temp_dir().join("tpr_project_like_cwd");
        std::fs::create_dir_all(cwd.join(".claude")).expect("mkdir");
        let picked = super::pick_project_root(
            None,
            None,
            cwd.clone(),
            Some(std::path::PathBuf::from("/home/x")),
        );
        assert_eq!(picked, cwd);
    }

    #[test]
    fn project_root_non_project_cwd_falls_back_to_home() {
        let cwd = std::env::temp_dir().join("tpr_bare_cwd");
        let _ = std::fs::remove_dir_all(&cwd);
        std::fs::create_dir_all(&cwd).expect("mkdir");
        let picked =
            super::pick_project_root(None, None, cwd, Some(std::path::PathBuf::from("/home/x")));
        assert_eq!(picked, std::path::PathBuf::from("/home/x"));
    }

    use super::*;

    #[test]
    fn test_default_config_values() {
        let config = TouringConfig::default();
        assert_eq!(config.project_root, PathBuf::from("."));
        assert_eq!(config.cache_size, 10000);
        assert_eq!(config.watcher_debounce_ms, 100);
        assert_eq!(config.max_file_size, 5 * 1024 * 1024);
        assert!(!config.debug);
        assert_eq!(config.embedding_dim, 384);
        assert!(config.auto_embed);
        assert!(config.jsonl_watch_enabled);
        assert_eq!(config.jsonl_poll_interval_s, 30);
        assert!(config.evolution_enabled);
        assert_eq!(config.evolution_interval_s, 300);
        assert!(config.touring_knowledge_path.is_none());
    }

    #[test]
    fn test_default_db_paths_relative() {
        let config = TouringConfig::default();
        assert!(config.symbols_db_path.ends_with("symbols.db"));
        assert!(config.rlm_db_path.ends_with("rlm_memory.db"));
        assert!(config.semantic_db_path.ends_with("semantic_recall.db"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = TouringConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: TouringConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(config.cache_size, deserialized.cache_size);
        assert_eq!(config.embedding_dim, deserialized.embedding_dim);
        assert_eq!(config.max_file_size, deserialized.max_file_size);
        // touring_knowledge_path is #[serde(skip)] — should be None after deserialization
        assert!(deserialized.touring_knowledge_path.is_none());
    }

    #[test]
    fn test_toml_deserialization_partial() {
        // Partial TOML should use defaults for missing fields
        let toml_str = r#"
            cache_size = 5000
            debug = true
            project_root = "/tmp/test"
            symbols_db_path = "/tmp/symbols.db"
            rlm_db_path = "/tmp/rlm.db"
            semantic_db_path = "/tmp/semantic.db"
            watcher_debounce_ms = 200
            max_file_size = 1024
        "#;
        let config: TouringConfig = toml::from_str(toml_str).expect("parse toml");
        assert_eq!(config.cache_size, 5000);
        assert!(config.debug);
        // Defaults for fields not specified in TOML
        assert_eq!(config.embedding_dim, 384);
        assert!(config.auto_embed);
    }

    #[test]
    fn test_detect_uses_env_overrides() {
        // Save and restore env vars
        let old_db = std::env::var("TOURING_DB_PATH").ok();
        let old_mem = std::env::var("TOURING_MEMORY_PATH").ok();

        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_DB_PATH", "/tmp/test_symbols.db") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_MEMORY_PATH", "/tmp/test_memory") };

        let config = TouringConfig::detect();
        assert_eq!(
            config.symbols_db_path,
            PathBuf::from("/tmp/test_symbols.db")
        );
        assert_eq!(
            config.rlm_db_path,
            PathBuf::from("/tmp/test_memory/rlm_memory.db")
        );
        assert_eq!(
            config.semantic_db_path,
            PathBuf::from("/tmp/test_memory/semantic_recall.db")
        );

        // Restore
        match old_db {
            // TODO: Audit that the environment access only happens in single-threaded code.
            Some(v) => unsafe { std::env::set_var("TOURING_DB_PATH", v) },
            // TODO: Audit that the environment access only happens in single-threaded code.
            None => unsafe { std::env::remove_var("TOURING_DB_PATH") },
        }
        match old_mem {
            // TODO: Audit that the environment access only happens in single-threaded code.
            Some(v) => unsafe { std::env::set_var("TOURING_MEMORY_PATH", v) },
            // TODO: Audit that the environment access only happens in single-threaded code.
            None => unsafe { std::env::remove_var("TOURING_MEMORY_PATH") },
        }
    }

    #[test]
    fn test_detect_tiered_knowledge_always_local() {
        let config = TouringConfig::detect_tiered();
        // touring_knowledge_path must always be set after detect_tiered
        assert!(config.touring_knowledge_path.is_some());
        let kp = config.touring_knowledge_path.unwrap();
        assert!(kp.ends_with("touring_knowledge.db"));
        // Must contain "data" in path (project-local .claude/data/)
        assert!(kp.to_string_lossy().contains("data"));
    }

    #[test]
    fn test_ensure_dirs_creates_parents() {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let config = TouringConfig {
            symbols_db_path: tmp.path().join("a/b/symbols.db"),
            rlm_db_path: tmp.path().join("c/d/rlm.db"),
            semantic_db_path: tmp.path().join("e/f/semantic.db"),
            touring_knowledge_path: Some(tmp.path().join("g/h/knowledge.db")),
            ..TouringConfig::default()
        };
        config.ensure_dirs().expect("ensure_dirs");
        assert!(tmp.path().join("a/b").is_dir());
        assert!(tmp.path().join("c/d").is_dir());
        assert!(tmp.path().join("e/f").is_dir());
        assert!(tmp.path().join("g/h").is_dir());
    }

    #[test]
    fn test_gpu_url_default() {
        // When GPU_SERVICE_URL is not set, should use localhost:8200
        let old = std::env::var("GPU_SERVICE_URL").ok();
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("GPU_SERVICE_URL") };
        let url = default_gpu_url();
        assert_eq!(url, "http://localhost:8200");
        if let Some(v) = old {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::set_var("GPU_SERVICE_URL", v) };
        }
    }

    #[test]
    fn test_symbols_db_canonical_path() {
        let root = PathBuf::from("/home/user/project");
        let canonical = TouringConfig::symbols_db_canonical(&root);
        assert_eq!(
            canonical,
            PathBuf::from("/home/user/project/.claude/touring/symbols.db")
        );
    }

    #[test]
    fn test_symbols_db_canonical_matches_default() {
        // The canonical helper must produce the same relative path as the default config
        let config = TouringConfig::default();
        let canonical = TouringConfig::symbols_db_canonical(&config.project_root);
        assert_eq!(canonical, config.symbols_db_path);
    }

    // ── W12.4 — Layered config loader tests ────────────────────────────────

    #[test]
    fn test_layered_precedence_hardcoded_only() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        // No system/user/project files exist → hardcoded defaults win
        let config = TouringConfig::detect_layered_from(
            None,
            Some(tmp.path().join("nonexistent_user.toml")),
            Some(tmp.path().join("nonexistent_project.toml")),
        )
        .expect("detect_layered_from");
        assert_eq!(config.cache_size, 10_000); // default
        assert_eq!(config.embedding_dim, 384); // default
    }

    #[test]
    fn test_layered_precedence_user_overrides_hardcoded() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let user_path = tmp.path().join("user.toml");
        std::fs::write(&user_path, "cache_size = 50000\nembedding_dim = 768\n")
            .expect("write user toml");
        let config = TouringConfig::detect_layered_from(None, Some(user_path), None)
            .expect("detect_layered_from");
        assert_eq!(config.cache_size, 50_000);
        assert_eq!(config.embedding_dim, 768);
    }

    #[test]
    fn test_layered_precedence_project_overrides_user() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let user_path = tmp.path().join("user.toml");
        let project_path = tmp.path().join("project.toml");
        std::fs::write(&user_path, "cache_size = 50000\nembedding_dim = 768\n").unwrap();
        std::fs::write(&project_path, "cache_size = 99999\n").unwrap();
        let config = TouringConfig::detect_layered_from(None, Some(user_path), Some(project_path))
            .expect("detect_layered_from");
        assert_eq!(config.cache_size, 99_999); // project wins
        assert_eq!(config.embedding_dim, 768); // user value preserved (project didn't override)
    }

    #[test]
    fn test_layered_reads_project_toolchain_pin() {
        // W-F0.2 (Productization Fase 0): the `[toolchain] channel` pin written
        // by `touring init-project` must survive the layered merge, and its
        // absence must stay `None` (today's unpinned behaviour).
        let tmp = tempfile::tempdir().expect("tmpdir");
        let project_path = tmp.path().join("project.toml");
        std::fs::write(
            &project_path,
            "cache_size = 123\n\n[toolchain]\nchannel = \"30.3.0\"\n",
        )
        .unwrap();
        let config = TouringConfig::detect_layered_from(None, None, Some(project_path))
            .expect("detect_layered_from");
        assert_eq!(config.cache_size, 123);
        assert_eq!(
            config.toolchain.and_then(|t| t.channel).as_deref(),
            Some("30.3.0")
        );

        // No [toolchain] table anywhere → pin stays None.
        let bare =
            TouringConfig::detect_layered_from(None, None, None).expect("detect_layered_from bare");
        assert!(bare.toolchain.is_none());
    }

    #[test]
    fn test_layered_precedence_system_user_project_chain() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let sys = tmp.path().join("sys.toml");
        let user = tmp.path().join("user.toml");
        let project = tmp.path().join("project.toml");
        std::fs::write(
            &sys,
            "cache_size = 1000\nembedding_dim = 128\nmax_file_size = 1024\n",
        )
        .unwrap();
        std::fs::write(&user, "cache_size = 5000\nembedding_dim = 384\n").unwrap();
        std::fs::write(&project, "cache_size = 9999\n").unwrap();
        let config = TouringConfig::detect_layered_from(Some(sys), Some(user), Some(project))
            .expect("detect_layered_from");
        assert_eq!(config.cache_size, 9_999); // project
        assert_eq!(config.embedding_dim, 384); // user
        assert_eq!(config.max_file_size, 1024); // system
    }

    // ── W12.5 — Daemon socket path resolver tests ─────────────────────────

    #[test]
    fn test_socket_resolver_env_override_wins() {
        // Use _inner directly to avoid env-var race conditions with parallel
        // tests. The wrapper resolve_daemon_socket_path_from reads the env
        // var; the _inner takes it as explicit arg.
        let p = TouringConfig::resolve_daemon_socket_path_inner(None, Some("/explicit/path.sock"));
        assert_eq!(p, PathBuf::from("/explicit/path.sock"));
    }

    #[test]
    fn test_socket_resolver_walk_up_finds_project_sock() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let proj = tmp.path().join("proj");
        let sock_dir = proj.join(".touring");
        let nested = proj.join("sub/sub2");
        std::fs::create_dir_all(&sock_dir).unwrap();
        std::fs::create_dir_all(&nested).unwrap();
        let sock = sock_dir.join("daemon.sock");
        std::fs::write(&sock, b"placeholder").unwrap(); // not a real socket, just must exist

        let p = TouringConfig::resolve_daemon_socket_path_inner(Some(nested), None);
        assert_eq!(p, sock);
    }

    #[test]
    fn test_socket_resolver_falls_back_to_global_tmp() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        // No .touring/daemon.sock anywhere in tree
        let p =
            TouringConfig::resolve_daemon_socket_path_inner(Some(tmp.path().to_path_buf()), None);
        let s = p.to_string_lossy();
        assert!(
            s.starts_with("/tmp/touring-daemon-") && s.ends_with(".sock"),
            "expected /tmp/touring-daemon-<uid>.sock, got {}",
            s
        );
    }

    #[test]
    fn test_socket_resolver_no_start_dir_falls_to_global() {
        let p = TouringConfig::resolve_daemon_socket_path_inner(None, None);
        let s = p.to_string_lossy();
        assert!(s.starts_with("/tmp/touring-daemon-"));
    }

    #[test]
    fn test_socket_resolver_empty_env_override_treated_as_unset() {
        // Empty string in env var falls through to walk-up / global fallback
        let p = TouringConfig::resolve_daemon_socket_path_inner(None, Some(""));
        let s = p.to_string_lossy();
        assert!(s.starts_with("/tmp/touring-daemon-"));
    }

    #[test]
    fn test_layered_malformed_file_falls_through_to_lower_layer() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let user = tmp.path().join("user.toml");
        let project = tmp.path().join("project.toml");
        std::fs::write(&user, "cache_size = 7777\n").unwrap();
        // Malformed TOML in project → fall through to user/hardcoded merge
        std::fs::write(&project, "cache_size = NOT_A_NUMBER\n[unclosed").unwrap();
        let config = TouringConfig::detect_layered_from(None, Some(user), Some(project))
            .expect("detect_layered_from succeeds despite malformed project");
        assert_eq!(config.cache_size, 7_777); // user value, project dropped
    }
}
