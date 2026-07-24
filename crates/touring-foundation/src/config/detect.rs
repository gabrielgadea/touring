//! Configuration detection & layered loading for [`TouringConfig`].
//!
//! Groups the discovery entry points (`detect`, `load`, `detect_tiered`) and the
//! W12.4 layered TOML loader (`detect_layered*`, `read_layer`) so the config data
//! model in the parent module stays focused on the struct + defaults. Split out of
//! `config.rs` (2026-07-02) along the detection/loading cohesion seam — the methods
//! stay on `TouringConfig`, so every public path is unchanged.

use super::*;
use std::path::Path;
use tracing_attributes::instrument;

/// Recursive merge of two `toml::Value`s. `overlay` keys take precedence over
/// `base` keys (last-write-wins). Tables are merged recursively; arrays and
/// scalars are replaced wholesale. Used by `TouringConfig::detect_layered_from`.
fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_t), toml::Value::Table(overlay_t)) => {
            for (k, v) in overlay_t {
                match base_t.get_mut(&k) {
                    Some(existing) => merge_toml(existing, v),
                    None => {
                        base_t.insert(k, v);
                    }
                }
            }
        }
        (slot, overlay) => {
            *slot = overlay;
        }
    }
}

impl TouringConfig {
    /// Auto-detect configuration from environment.
    ///
    /// Priority: env vars > config.toml > defaults.
    /// - `CLAUDE_PROJECT_DIR` / `TOURING_PROJECT_ROOT` / project-like CWD /
    ///   `$HOME`: project root (see [`resolve_project_root`])
    /// - `TOURING_DB_PATH`: symbols database path
    /// - `TOURING_MEMORY_PATH`: directory for rlm_memory.db + semantic_recall.db
    #[must_use]
    pub fn detect() -> Self {
        let project_root = resolve_project_root();

        let config_path = project_root.join(".claude/touring/config.toml");
        let mut config = if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => toml::from_str::<TouringConfig>(&content).unwrap_or_else(|e| {
                    tracing::warn!("Failed to parse config.toml: {e}. Using defaults.");
                    Self::default()
                }),
                Err(e) => {
                    tracing::warn!("Failed to read config.toml: {e}. Using defaults.");
                    Self::default()
                }
            }
        } else {
            Self::default()
        };

        config.project_root = project_root;

        // Anchor relative default DB paths at the resolved project root —
        // previously they resolved against the process CWD, materializing
        // ghost DBs wherever the process happened to start (e.g. /tmp).
        let root = config.project_root.clone();
        for path in [
            &mut config.symbols_db_path,
            &mut config.rlm_db_path,
            &mut config.semantic_db_path,
            &mut config.knowledge_db_path,
            &mut config.memory_db_path,
            &mut config.graph_db_path,
        ] {
            if path.is_relative() {
                *path = root.join(&*path);
            }
        }

        // Env var overrides (highest priority — matches MCP server config)
        if let Ok(db_path) = std::env::var("TOURING_DB_PATH") {
            config.symbols_db_path = PathBuf::from(db_path);
        }
        if let Ok(mem_path) = std::env::var("TOURING_MEMORY_PATH") {
            let mem_dir = PathBuf::from(mem_path);
            config.rlm_db_path = mem_dir.join("rlm_memory.db");
            config.semantic_db_path = mem_dir.join("semantic_recall.db");
        }

        config
    }

    /// Load config using tiered path resolution (local-first, global-fallback).
    /// This is the preferred entry point for cross-project usage.
    pub fn load() -> crate::Result<Self> {
        Ok(Self::detect_tiered())
    }

    /// W12.4 — Layered config loader (rustup-pattern adapted).
    ///
    /// Precedence (lowest to highest, last write wins per key):
    /// 1. Hardcoded defaults (`TouringConfig::default()`)
    /// 2. System: `/etc/touring/config.toml`
    /// 3. User: `~/.touring/config.toml`
    /// 4. Project: `.touring/touring.toml` (walk-up from CWD)
    ///
    /// Malformed TOML in any layer is logged at WARN and the layer is skipped.
    /// Missing files are silently ignored (a layer that doesn't exist contributes nothing).
    /// Env-var overrides (`TOURING_DB_PATH`, etc.) are applied by `detect()` and not
    /// re-applied here — call `detect_layered().with_env_overrides()` for full chain.
    pub fn detect_layered() -> crate::Result<Self> {
        let system = Some(PathBuf::from("/etc/touring/config.toml"));
        let user = std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".touring").join("config.toml"));
        let project = Self::find_project_toml_walk_up();
        Self::detect_layered_from(system, user, project)
    }

    /// Walk up from `CWD` looking for `.touring/touring.toml`. Stops at filesystem root.
    fn find_project_toml_walk_up() -> Option<PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let candidate = dir.join(".touring").join("touring.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Testable variant of `detect_layered` — takes explicit layer paths.
    /// Used by both production code (`detect_layered`) and unit tests.
    pub fn detect_layered_from(
        system: Option<PathBuf>,
        user: Option<PathBuf>,
        project: Option<PathBuf>,
    ) -> crate::Result<Self> {
        // Start from hardcoded defaults → serialize to toml::Value so we can merge.
        let default_value = toml::Value::try_from(Self::default())
            .map_err(|e| crate::TouringError::Config(format!("serialize defaults: {e}")))?;
        let mut merged: toml::Value = default_value;

        for layer in [system, user, project].into_iter().flatten() {
            match Self::read_layer(&layer) {
                Some(v) => merge_toml(&mut merged, v),
                None => continue, // missing or malformed — fall through
            }
        }

        toml::Value::try_into::<Self>(merged)
            .map_err(|e| crate::TouringError::Config(format!("deserialize merged config: {e}")))
    }

    /// Read a layer file, returning `Some(toml::Value)` on success, `None` on
    /// missing-file OR malformed-TOML (the latter is logged at WARN). Callers
    /// treat `None` as "this layer contributes nothing" — failing OPEN.
    fn read_layer(path: &Path) -> Option<toml::Value> {
        if !path.exists() {
            return None;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), err = %e, "layered_config: read failed");
                return None;
            }
        };
        match toml::from_str::<toml::Value>(&content) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(path = %path.display(), err = %e, "layered_config: parse failed");
                None
            }
        }
    }

    /// Tiered database path resolution: local-first, global-fallback.
    ///
    /// | DB | Local | Global Fallback |
    /// |----|-------|----------------|
    /// | `symbols_db_path` | `CWD/.claude/touring/symbols.db` | `~/.claude/rust/symbols.db` |
    /// | `rlm_db_path` | `CWD/.claude/data/rlm_memory.db` | `~/.claude/data/rlm_memory.db` |
    /// | `semantic_db_path` | `CWD/.claude/data/semantic_recall.db` | `~/.claude/data/semantic_recall.db` |
    /// | `touring_knowledge.db` | `CWD/.claude/data/touring_knowledge.db` | **N/A (always local)** |
    ///
    /// Rule P7 from G-TGM-001 v2.1: RL tables = GLOBAL; knowledge tables = LOCAL;
    /// `touring_knowledge.db` = always local.
    #[must_use]
    #[instrument(level = tracing::Level::DEBUG, skip_all, fields(project_root = %std::env::var("CLAUDE_PROJECT_DIR").unwrap_or_else(|_| ".".into())))]
    pub fn detect_tiered() -> Self {
        // 4-step chain: CLAUDE_PROJECT_DIR → TOURING_PROJECT_ROOT →
        // project-like CWD → $HOME (see [`resolve_project_root`]). Fixes
        // the 2026-06-12 incident where the detached daemon (CWD=/tmp,
        // CLAUDE_PROJECT_DIR unset) anchored every DB at /tmp/.claude/…
        // and reported symbol_count=0 from a ghost store.
        let project_root = resolve_project_root();

        tracing::debug!(root = %project_root.display(), "detect_tiered: resolving database paths");

        // Global roots (HOME-relative)
        let global_root = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".claude"))
            .unwrap_or_else(|_| PathBuf::from("/home/gabrielgadea/.claude"));

        let global_rust = global_root.join("rust");
        let global_data = global_root.join("data");

        // Local roots
        let local_touring = project_root.join(".claude").join("touring");
        let local_data = project_root.join(".claude").join("data");

        // Start from defaults then override with tiered paths
        let mut config = Self::default();

        // Tiered: symbols — local first, global fallback
        let symbols_local = local_touring.join("symbols.db");
        if symbols_local.exists() {
            config.symbols_db_path = symbols_local.clone();
            tracing::debug!(path = %symbols_local.display(), tier = "local", "symbols_db");
        } else {
            let fallback = global_rust.join("symbols.db");
            tracing::debug!(path = %fallback.display(), tier = "global_fallback", "symbols_db");
            config.symbols_db_path = fallback;
        }

        // Tiered: rlm_memory — local first, global fallback
        let rlm_local = local_data.join("rlm_memory.db");
        if rlm_local.exists() {
            config.rlm_db_path = rlm_local.clone();
            tracing::debug!(path = %rlm_local.display(), tier = "local", "rlm_db");
        } else {
            let fallback = global_data.join("rlm_memory.db");
            tracing::debug!(path = %fallback.display(), tier = "global_fallback", "rlm_db");
            config.rlm_db_path = fallback;
        }

        // Tiered: semantic_recall — local first, global fallback
        let semantic_local = local_data.join("semantic_recall.db");
        if semantic_local.exists() {
            config.semantic_db_path = semantic_local.clone();
            tracing::debug!(path = %semantic_local.display(), tier = "local", "semantic_db");
        } else {
            let fallback = global_data.join("semantic_recall.db");
            tracing::debug!(path = %fallback.display(), tier = "global_fallback", "semantic_db");
            config.semantic_db_path = fallback;
        }

        // touring_knowledge.db: ALWAYS local (never global) — no fallback
        let knowledge_path = local_data.join("touring_knowledge.db");
        config.touring_knowledge_path = Some(knowledge_path.clone());
        tracing::debug!(path = %knowledge_path.display(), tier = "local", "knowledge_db");

        config.project_root = project_root.clone();

        // Env var overrides still take highest priority
        if let Ok(db_path) = std::env::var("TOURING_DB_PATH") {
            tracing::debug!(path = %db_path, source = "TOURING_DB_PATH", "symbols_db override");
            config.symbols_db_path = PathBuf::from(db_path);
        }
        if let Ok(mem_path) = std::env::var("TOURING_MEMORY_PATH") {
            let mem_dir = PathBuf::from(mem_path);
            tracing::debug!(path = %mem_dir.display(), source = "TOURING_MEMORY_PATH", "memory_db override");
            config.rlm_db_path = mem_dir.join("rlm_memory.db");
            config.semantic_db_path = mem_dir.join("semantic_recall.db");
        }

        // Consolidated domain paths — always derive from project_root
        config.knowledge_db_path = project_root
            .join(".claude")
            .join("touring")
            .join("knowledge.db");
        config.graph_db_path = project_root
            .join(".claude")
            .join("touring")
            .join("graph.db");

        // memory.db: local-first, global fallback (RL data is global per rule P7)
        let memory_local = project_root
            .join(".claude")
            .join("touring")
            .join("memory.db");
        let home_dir = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| project_root.to_path_buf());
        let memory_global = home_dir.join(".claude").join("touring").join("memory.db");
        config.memory_db_path = if memory_local.exists() {
            memory_local
        } else {
            memory_global
        };

        // Env var overrides (highest priority)
        if let Ok(p) = std::env::var("TOURING_KNOWLEDGE_DB") {
            config.knowledge_db_path = PathBuf::from(p);
        }
        if let Ok(p) = std::env::var("TOURING_MEMORY_DB") {
            config.memory_db_path = PathBuf::from(p);
        }
        if let Ok(p) = std::env::var("TOURING_GRAPH_DB") {
            config.graph_db_path = PathBuf::from(p);
        }

        tracing::debug!(
            project = %config.project_root.display(),
            symbols = %config.symbols_db_path.display(),
            rlm = %config.rlm_db_path.display(),
            semantic = %config.semantic_db_path.display(),
            "detect_tiered: resolved"
        );

        config
    }
}
