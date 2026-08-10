//! CLI subcommand handlers for touring-server.
//!
//! This module provides the CLI interface for the touring binary's telemetry
//! and monitoring subcommands (learning, wiring, cognitive, etc.).
//!
//! Each module exposes a `pub fn run(args: &[String]) -> anyhow::Result<()>` that
//! connects to the daemon socket, sends the appropriate request, and prints the response.

use std::io::IsTerminal;

// Daemon socket RPC client lives in `crate::daemon_client` (Session A / step A1
// of the touring-server split). Re-exported here so the 55 cli/* handlers keep
// using `super::daemon_query` and `common.rs` keeps using
// `super::DAEMON_READ_TIMEOUT_SECS` (the `--timeout` flag setter) unchanged.
pub use crate::daemon_client::{DAEMON_READ_TIMEOUT_SECS, daemon_query};
// `libc_getuid` was a private helper in this module; keep it crate-visible
// (not public API) so cli/* handlers (`daemon_ctl`, `doctor`, `entity`) that
// build their own per-user paths keep resolving `super::libc_getuid`.
pub(crate) use crate::daemon_client::libc_getuid;

pub mod activity;
pub mod assist;
pub mod ast;
pub mod audit; // R3 — touring audit CLI adapter over run_audit (code-mode, no MCP)
pub mod backup;
pub mod cascade;
pub mod change_contract;
pub mod classify;
pub mod clones;
pub mod cognitive;
pub mod command_table;
pub mod common;
pub mod completions;
pub mod component; // F3 — optional per-project components (W12.3)
pub mod conflict;
pub mod context;
pub mod daemon_ctl;
pub mod decompose;
pub mod definitions;
pub mod devrcfile;
pub mod diagnostics;
pub mod diary;
pub mod discover; // NEW-3 — High-ROI optimization candidate suggester
pub mod doctor;
pub mod e2e;
pub mod entity;
pub mod eval;
pub mod evolution;
pub mod exec;
pub mod file_knowledge;
pub mod filters; // NEW-4 — TOML user filter DSL CLI
pub mod find_code;
pub mod find_references;
pub mod fix;
pub mod flow;
pub mod flywheel;
pub mod gain; // NEW-3 — RTK parity analytics dashboard
pub mod gate_metrics;
pub mod generate;
pub mod gotcha;
pub mod governor;
pub mod granularity;
pub mod graph;
pub mod handlers_inferlet; // `touring inferlets install` subcommand
pub mod harness_metric;
pub mod health_delta;
pub mod highlight;
pub mod incremental;
pub mod index;
pub mod inferlets;
pub mod init;
pub mod init_project;
pub mod jobs;
pub mod kpi;
pub mod language;
pub mod learning;
pub mod license; // F5/G3 — tier model visibility (first touring-license consumer)
pub mod master; // R3 — master CLI commands (code-mode wrappers over Layer-3 scripts)
pub mod mcp_overhead;
pub mod mcts;
pub mod memory;
pub mod migrate;
pub mod migrate_from_global;
pub mod mutation_test;
pub mod neural;
pub mod overlay;
pub mod pii;
pub mod plugin;
pub mod profile;
pub mod project_toolchain; // F3 — channel ↔ lockfile ↔ .touring/bin state machine
pub mod projects;
pub mod quality_signal;
pub mod reason_tools; // C11/C12/C14 — orchestrator surfaces: budget-verify / plan-chain / consistency
pub mod rename;
pub mod repo_health;
pub mod repo_score;
pub mod resolve_def;
pub mod route; // C7 — RGAO task routing
pub mod run; // R1 — touring run: code-mode via CLI over the ctx_execute sandbox
pub mod saga;
pub mod search_tools; // C3 — intent-ranked tool discovery
pub mod portfolio; // prior-art discovery keyed by purpose
pub mod search_unified;
pub mod session;
pub mod shadow;
pub mod skip;
pub mod snapshot;
pub mod source_change;
pub mod ssr;
pub mod status;
pub mod suggest;
pub mod synergy;
pub mod tantivy;
pub mod tasksfile;
pub mod toolchain;
pub mod update; // F3 — per-project update propagation (W12.3)
pub mod viz;
pub mod wiring;
pub mod workflow; // W7 — shell completions + man page (clap_complete / clap_mangen)

/// Read JSON from stdin, returning `{}` if stdin is a terminal or times out.
///
/// Covers 3 stdin types: terminal (instant `{}`), pipe (read+parse), socket (2s timeout).
pub fn read_stdin_safe() -> serde_json::Value {
    use std::io::Read;

    if std::io::stdin().is_terminal() {
        return serde_json::json!({});
    }

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut input = String::new();
        let result = std::io::stdin().read_to_string(&mut input);
        let _ = tx.send((input, result));
    });

    match rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok((input, Ok(_))) if !input.trim().is_empty() => {
            serde_json::from_str(&input).unwrap_or_else(|_| serde_json::json!({}))
        }
        _ => serde_json::json!({}),
    }
}

// Socket client (`daemon_query` + helpers) moved to `crate::daemon_client`
// (Session A / step A1 of the touring-server split). See the re-export above.
