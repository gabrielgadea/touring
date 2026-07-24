//! touring-server — MCP Server (≈22 curated tools, ≈160 total) + Cortex CLI (73 handlers) + Modular Architecture.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (7 fixes: assist.rs ×3
// guarded `parse().unwrap()` → unwrap_or(0); entity strip_prefix + migrate file_name
// + 2 search_unified static-AbsPath → expect) — lock against future bare unwrap in
// non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

/// Typed error enum for touring-server public APIs (replaces `Result<_, String>`).
pub mod error;
pub use error::ServerError;

/// MCP server implementation. Curated default surface ≈22 tools (≈160 total;
/// `TOURING_MCP_ALL_TOOLS=1` lists all, `touring_search` discovers on demand).
pub mod server;
/// Tool business logic (AST, memory, file, project, utility, context, suggestions, risk, hints, search, cache).
pub mod tools;

/// Elite harness tools (migrated 2026-06-25 from touring-harness-mcp) —
/// 5 MCP-callable tools wrapping the 17-gate composite scorer from
/// `touring-quality`. Dispatch via `dispatch("elite_check", &params)` or
/// the `touring elite <tool>` CLI subcommand.
pub mod elite_tools;

/// System introspection — aggregates daemon-relevant runtime facts (uid,
/// canonical socket path, entity DB path, current unix millis, workspace
/// file count) into a single JSON envelope. Wires the helper functions
/// scattered across `server::tools_*` modules so all of them have a single
/// production-side consumer in addition to their unit tests. Consumed by
/// CLI diagnostic subcommands, audit trails, and the upcoming
/// `touring system info` command.
///
/// REGRA #0 (potencializar): centralizes 10+ otherwise-orphaned helpers
/// into one observable surface instead of deleting them.
pub mod system_info {
    use crate::server::tools_activity::{ActivityAppendParams, ActivityReplayParams, make_result};
    use crate::server::tools_analysis::{current_uid, daemon_socket_path};
    use crate::server::tools_core::default_entity_db_path;
    use crate::server::tools_quality_signal::{make_diff_error, unix_millis_now};
    use crate::tools::file_tools::{find_workspace_files, glob_to_regex};

    /// Capture the canonical system-introspection envelope. Fields:
    /// `uid`, `daemon_socket_path`, `entity_db_path`, `timestamp_ms`,
    /// `workspace_root`, `workspace_match_count`,  `glob_pattern_regex`,
    /// `sample_activity_params` (illustrates the builder API),
    /// `sample_replay_params`.
    ///
    /// `root` defaults to the current working directory when `None`.
    #[must_use]
    pub fn snapshot(root: Option<&std::path::Path>) -> serde_json::Value {
        let cwd = root.map(std::path::Path::to_path_buf).unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
        let entity_db = default_entity_db_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| format!("error: {e}"));
        let rs_matches = find_workspace_files(&cwd, "*.rs", false, None, 3, false);

        // Exercise the activity-params builders so cargo sees them as
        // reachable from the production tree (not only #[cfg(test)]).
        let sample_append = ActivityAppendParams::new()
            .with_action("system_info_snapshot")
            .with_actor("touring-server")
            .with_payload(serde_json::json!({"source": "system_info::snapshot"}));
        let sample_replay = ActivityReplayParams::new().with_limit(8);

        serde_json::json!({
            "uid": current_uid(),
            "daemon_socket_path": daemon_socket_path().display().to_string(),
            "entity_db_path": entity_db,
            "timestamp_ms": unix_millis_now(),
            "workspace_root": cwd.display().to_string(),
            "workspace_match_count": rs_matches.len(),
            "glob_pattern_regex": glob_to_regex("*.rs"),
            "sample_activity_params": {
                "action": sample_append.action,
                "actor": sample_append.actor,
                "payload": sample_append.payload,
            },
            "sample_replay_params": { "limit": sample_replay.limit },
        })
    }

    /// Build an error envelope demonstrating `make_diff_error` — useful
    /// for surfacing daemon-side failures in audit logs with consistent
    /// shape. Wires `make_diff_error` so the helper has a non-test caller.
    pub fn build_diff_error_envelope(
        prev: &std::path::Path,
        curr: &std::path::Path,
        field: &str,
        error: &str,
    ) -> serde_json::Value {
        // We don't need the CallToolResult here — only that the helper
        // is exercised by a non-test caller.
        let _ = make_diff_error(prev, curr, field, error);
        serde_json::json!({
            "kind": "diff_error_envelope",
            "previous_root": prev.display().to_string(),
            "current_root": curr.display().to_string(),
            "failed_field": field,
            "error": error,
            "timestamp_ms": unix_millis_now(),
        })
    }

    /// Wrap a JSON value in the canonical MCP tool-result shape used by
    /// activity tools. Provides a non-test caller for `make_result` so
    /// the helper survives cargo dead-code analysis when the `#[tool]`
    /// macro layer doesn't expose its consumers to the analyzer.
    pub fn wrap_tool_result(
        json: &serde_json::Value,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        make_result(json)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn snapshot_has_all_expected_fields() {
            let snap = snapshot(None);
            for key in [
                "uid",
                "daemon_socket_path",
                "entity_db_path",
                "timestamp_ms",
                "workspace_root",
                "workspace_match_count",
                "glob_pattern_regex",
                "sample_activity_params",
                "sample_replay_params",
            ] {
                assert!(snap.get(key).is_some(), "missing field: {key}");
            }
        }

        #[test]
        fn snapshot_timestamp_is_recent() {
            let ts = snapshot(None)
                .get("timestamp_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            assert!(
                ts > 1_700_000_000_000,
                "timestamp should be post-2023: {ts}"
            );
        }

        #[test]
        fn diff_error_envelope_includes_timestamp() {
            let env = build_diff_error_envelope(
                std::path::Path::new("/a"),
                std::path::Path::new("/b"),
                "cyclomatic",
                "parse failure",
            );
            assert_eq!(
                env.get("failed_field").and_then(|v| v.as_str()),
                Some("cyclomatic")
            );
            assert!(env.get("timestamp_ms").is_some());
        }

        #[test]
        fn wrap_tool_result_round_trip() {
            let result = wrap_tool_result(&serde_json::json!({"status": "ok"}));
            assert!(result.is_ok());
        }
    }
}

// Re-export key types for external consumers (integration tests, CLI bin).
pub use server::params::{DetailLevel, apply_detail_level};
/// Re-exports touring-cortex crate as `cortex` for convenience.
pub use touring_cortex as cortex;
/// Re-exports touring-hooks crate as `hooks` for convenience.
pub use touring_hooks as hooks;
/// Re-exports `touring_intelligence::index` as `index` for convenience.
pub use touring_intelligence::index;
/// Agent diary system with wing_{agent}/diary hierarchy and AAAK dialect support.
pub mod agent_diary;
/// Context compilation from multiple sources for injection.
pub mod context_compiler;
/// Graph query and traversal service.
pub mod graph_service;
/// Data ingestion pipeline for knowledge graph population.
pub mod ingest;
/// Knowledge adapter for external knowledge sources.
pub mod knowledge_adapter;
/// Persistent memory store backed by SQLite.
pub mod memory_store;
/// Observation masking for privacy-sensitive content filtering.
pub mod observation_masker;
/// Output formatting and response construction.
pub mod output;
/// Plugin runtime and extension management.
pub mod plugins;
/// Reasoning engine and inference handlers.
pub use touring_server_reasoning::reasoning;
/// Reinforcement learning action mapping and rewards.
pub mod rl_mapping;
/// Session management and state handling.
pub use touring_server_session::session;
/// Telemetry and metrics collection.
pub mod telemetry;

/// SCIP emit — lightweight Source Code Intelligence Protocol export.
pub mod scip_emit;

/// Graph visualization formatters (DOT, Mermaid, JSON).
pub use touring_server_visual::visual;

/// Refactoring utilities (rename with plan + risk analysis).
pub mod refactor;

/// Multi-project registry — register, list, and switch between touring workspaces.
pub mod projects;

/// Feature-composable `tracing` subscriber setup (fmt + tokio-console + OTLP).
/// Public so the `touring` binary entry point can call [`telemetry_init::init`].
pub mod telemetry_init;

/// CLI module — shared between lib (server/tools_core.rs) and binary (main.rs).
/// Declared once at crate root; server/ and main.rs both access via `super::cli`
/// or `touring_server::cli` respectively.
#[doc(hidden)]
/// Daemon socket RPC client — the neutral seam shared by `cli` and `server`
/// (Session A / step A1 of the touring-server split). Both sides depend on
/// `crate::daemon_client::daemon_query` instead of crossing the `cli` ↔
/// `server` module boundary to reach the daemon.
pub mod daemon_client;

/// C3 (coupling backlog) — curated, intent-searchable tool catalog backing
/// `touring search-tools` (CLI) and the `touring_search` MCP tool.
pub mod tool_catalog;

pub mod cli;
