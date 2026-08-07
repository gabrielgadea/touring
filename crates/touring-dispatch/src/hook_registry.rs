//! S10: Centralized hook registry — single source of truth.
//!
//! All hook names and their handlers are declared here. Both the thin client
//! (`main.rs`) and the daemon (`daemon.rs`) import from this module instead
//! of maintaining separate lists that can drift out of sync.
//!
//! ## Architecture
//!
//! - `ALL_DAEMON_HOOK_NAMES`: `&[&str]` used by the thin client to decide
//!   which hooks should be forwarded to the daemon (vs. standalone execution).
//! - `build_dispatch_table()`: returns a `HashMap<&'static str, HookHandler>`
//!   used by the daemon to dispatch requests via O(1) lookup.

use std::collections::HashMap;

use crate::daemon::HookHandler;

/// All hooks handled by the daemon (require warm HookRuntime).
///
/// This is the **single source of truth** for daemon-routed hooks.
/// The thin client in `main.rs` uses this to decide whether to attempt
/// the daemon fast-path before falling back to standalone execution.
///
/// T3.10: Built dynamically to match `#[cfg(feature)]` gates in
/// `build_dispatch_table()`. When a feature is disabled, both the
/// dispatch entry AND the name list entry are omitted — preventing
/// `registry_names_match_dispatch_table` from failing.
///
/// Categories:
/// - Pre-hooks: inject context before tool execution
/// - Post-hooks: capture outcomes after tool execution
/// - Session: lifecycle management
/// - Team (N1): Agent Teams <-> ACO gateway
/// - Lifecycle (S4): intelligent event handlers
pub fn all_daemon_hook_names() -> Vec<&'static str> {
    let mut names = Vec::with_capacity(64);

    // Pre-hooks (feature-gated)
    #[cfg(feature = "pre-hooks")]
    {
        names.extend_from_slice(&[
            "pre-read",
            "pre-bash",
            "pre-edit",
            "pre-edit-prevention",
            "pre-grep",
            "pre-glob",
            "pre-write",
            "pre-tool-use",
            "cli-suggest",
            "ceg-observe",
        ]);
    }
    // Post-hooks (feature-gated)
    #[cfg(feature = "post-hooks")]
    {
        names.extend_from_slice(&[
            "post-read",
            "post-bash",
            "post-edit",
            "post-write",
            "post-tool-batch",
            "post-tool-rl",
            "post-tool-failure",
            "post-tool-use",
        ]);
    }
    // Session (feature-gated)
    #[cfg(feature = "session-hooks")]
    {
        names.extend_from_slice(&["session-start", "session-stop"]);
    }
    // Team (N1) — always-on
    names.extend_from_slice(&[
        "teammate-idle",
        "task-completed",
        "task-created",
        "task-validation",
        "task-metrics",
        "task-escalation",
    ]);
    // Team gate hooks (anti-limbo + bootstrap) — always-on
    names.extend_from_slice(&["teammate-idle-gate", "subagent-bootstrap"]);
    // Lifecycle (S4) — always-on
    names.extend_from_slice(&[
        "subagent-start",
        "subagent-stop",
        "file-changed",
        "cwd-changed",
        "pre-compact",
        "post-compact",
        "worktree-create",
        "instructions-loaded",
        "enter-plan-mode",
        "exit-plan-mode",
        "stop",
        // TACO Phase Protocol: UserPromptSubmit captures the raw user prompt
        // before CILA classification, enabling attribution and budget injection.
        // Emitted by `prompt_enhance.rs` with hookEventName: "UserPromptSubmit".
        "user_prompt_submit",
        // Claude Code Task ↔ Touring Decompose sync (R7-Sync)
        "task-sync-create",
        "task-sync-update",
        "task-sync-list",
        // Claude Code Task output/get sync (R8-Sync)
        "task-sync-output",
        "task-sync-get",
        // Claude Code Task stop/delete sync (R9-Sync)
        "task-sync-stop",
        "task-sync-delete",
    ]);
    // CLI telemetry handlers (daemon-side query handlers for touring <subsystem> <command>)
    names.extend_from_slice(&[
        "cli-learning-status",
        "cli-learning-reward",
        "cli-granularity-status",
        "cli-granularity-reset",
        "cli-granularity-hint",
        "cli-cascade-queue-status",
        "cli-cascade-queue-drain",
        // PLN2: saga distributed 2PC coordinator
        "cli-saga-register",
        "cli-saga-prepare",
        "cli-saga-decide",
        "cli-saga-delta",
        "cli-saga-begin",
        "cli-saga-status",
        "cli-saga-abort",
        "cli-wiring-status",
        "cli-wiring-orphans",
        "cli-wiring-modules",
        "cli-wiring-suggest",
        "cli-wiring-impact",
        "cli-wiring-cycles",
        "cli-cognitive-metrics",
        "cli-cognitive-engines",
        "cli-flywheel-status",
        "cli-incremental-status",
        "cli-gotcha-list",
        "cli-gotcha-add",
        "cli-gotcha-match",
        "cli-gotcha-stats",
        "cli-memory-stats",
        "cli-memory-recall",
        "cli-memory-credit",
        "cli-memory-store",
        "cli-memory-list",
        "cli-memory-reindex",
        "cli-evolution-drift",
        "cli-evolution-insights",
        "cli-evolution-tools",
        // D10-S1: touring-web health dashboard handlers
        "cli-doctor",
        "cli-status",
        // P1.6 (2026-04-25): mpatch-fuzzy dry-run patch preview
        "cli-mpatch-preview",
    ]);
    // CLI session handlers
    names.extend_from_slice(&[
        "cli-session-start",
        "cli-session-checkpoint",
        "cli-session-list",
        "cli-session-assess",
    ]);
    // CLI suggest handlers
    names.extend_from_slice(&["cli-suggest-next", "cli-suggest-skill"]);
    // CLI decompose handlers
    names.extend_from_slice(&[
        "cli-decompose-create",
        "cli-decompose-add",
        "cli-decompose-get",
        "cli-decompose-update",
        "cli-decompose-validate",
        "cli-decompose-status",
        "cli-decompose-event",
        "cli-decompose-finalize",
        "cli-decompose-ready",
    ]);
    // CLI mcts handlers
    names.push("cli-mcts-search");
    // CLI shadow handlers
    names.push("cli-shadow-validate");
    // CLI flow pipeline handler — REMOVED: cli-flow was registered in names
    // but never implemented (no cli_flow handler exists). cli-mcp-overhead
    // takes its place — handler existed at lines 1262-1264 but was missing
    // from all_daemon_hook_names(). Fixed 2026-05-03 cross-audit.
    // CLI mcts handlers
    names.extend_from_slice(&[
        "cli-index-status",
        "cli-index-search",
        "cli-index-find",
        "cli-index-files",
        "cli-index-rebuild",
        "cli-index-ingest",
    ]);
    // CLI ast handlers
    names.extend_from_slice(&[
        "cli-ast-find",
        "cli-ast-overview",
        "cli-ast-blast",
        "cli-ast-semantic",
        "cli-ast-quality",
        "cli-ast-calls",
        "cli-ast-heat",
        "cli-ast-modules",
        "cli-ast-scope",
        "cli-ast-detail",
        "cli-ast-imports",
        "cli-ast-grep",
        "cli-ast-scan",
        // D.2: resolve-def / find-references / rename
        "cli-resolve-def",
        "cli-find-references",
        "cli-rename",
    ]);
    // CLI e2e handler
    names.push("cli-e2e");
    // CLI pre-task-scout handler (LRU-cached scouting for PreToolUse)
    names.push("cli-pre-task-scout");
    // CLI prompt-enhance handler
    names.push("cli-prompt-enhance");
    // CLI hook-memory handlers
    names.push("cli-hook-memory-store");
    names.push("cli-hook-memory-recall");
    // CLI entity disambiguation handlers
    names.push("cli-entity-resolve");
    names.push("cli-entity-list");
    names.push("cli-entity-generic-patterns");
    // L7-B Gamma: gate metrics + inferlets + jobs handlers
    names.push("cli-gate-metrics");
    // R5/OP1: unified 6-dimension harness-quality metric
    names.push("cli-harness-metric");
    // S-09/R8: formal change-contract gating self-mutation
    names.push("cli-change-contract");
    // B-5/R10: distill historical bash-outcome substrate into an action predictor
    names.push("cli-predict-action");
    // S-08/A-A1: conformal calibration of the skill-selection gate (KnowNo)
    names.push("cli-calibrate-confidence");
    // Cold-start cluster: opt-in cross-project RL warm-start (real experience replay)
    names.push("cli-rl-warmstart");
    // ES4 P1: durable action world-model liveness probe (persist + warm-load)
    names.push("cli-world-model-status");
    // ES2 P2: constitutional sink-token contract attestation (EAGLE B-6)
    names.push("cli-attest-contract");
    // ES1 P4 (CAH roadmap 2026-05-30): standalone SMT service (`touring
    // prove-claim`) — JSON adapter that calls `touring-offensive`'s
    // `prove_claim` with stub/Z3/CVC5 backend selection.
    names.push("cli-prove-claim");
    // Wave 6 (2026-05-23): CEG observability boundary fix — CLI→daemon mirror
    names.push("cli-gate-event");
    // D28: MCP overhead self-report handler
    names.push("cli-mcp-overhead");
    // Wave 26: Tokio runtime metrics
    names.push("cli-tokio-metrics");
    // Wave 15: health_delta direct CLI exposure
    names.push("cli-health-delta-status");
    names.push("cli-health-delta-reset");
    // Wave I (Phase 5, 2026-04-24): grow-only audit trail query
    names.push("cli-health-delta-history");
    // Wave A A.4: profile_query CLI exposure
    names.push("cli-profile-status");
    // A.2.5 (2026-04-29): skip-region CLI exposure
    names.push("cli-skip-list");
    names.push("cli-skip-validate");
    // D31: touring definitions CLI (classify / node-types / semantic-search)
    names.extend_from_slice(&[
        "cli-definitions-classify",
        "cli-definitions-nodetypes",
        "cli-definitions-semantic-search",
    ]);
    // D27: plugin DI CLI (list/status/reload/unregister)
    names.extend_from_slice(&[
        "cli-plugin-list",
        "cli-plugin-status",
        "cli-plugin-reload",
        "cli-plugin-unregister",
    ]);
    // B.1 (2026-04-29): SSR CLI exposure
    names.push("cli-ssr-status");
    names.push("cli-ssr-apply");
    names.push("cli-inferlets-list");
    names.push("cli-inferlets-run");
    names.push("cli-jobs-spawn");
    names.push("cli-jobs-poll");
    names.push("cli-jobs-list");
    names.push("cli-jobs-drop");
    // P7 Pln2: new AST/wiring/search/session/bench handlers
    names.extend_from_slice(&[
        "cli-ast-callgraph",
        "cli-ast-todos",
        "cli-ast-rationale",
        "cli-ast-features",
        "cli-ast-meta",
        "cli-ast-skeleton",
        "cli-ast-tdg",
        "cli-gotcha-sync",
        "cli-gotcha-init",
        "cli-repo-score",
        "cli-kpi",
        "cli-repo-health",
        "cli-mutation-test",
        "cli-ast-blast-enriched",
        "cli-wiring-purpose",
        "cli-search-symbols",
        "cli-search-docs",
        "cli-query-dsl",
        "cli-metadata-backfill",
        "cli-session-summary",
        "cli-bench-run",
    ]);
    // Tantivy BM25 full-text search handlers
    names.extend_from_slice(&[
        "cli-tantivy-search",
        "cli-tantivy-fuzzy",
        "cli-tantivy-stats",
        "cli-tantivy-suggest",
        "cli-tantivy-reindex",
        "cli-wiring-community",
    ]);
    // Wave cross-audit: functional chains, cross-feature blast radius, enriched file knowledge
    names.extend_from_slice(&[
        "cli-wiring-chains",
        "cli-ast-blast-cross-feature",
        "cli-file-knowledge-extended",
        "cli-file-knowledge-stats",
        "cli-file-knowledge-populate",
        "cli-file-knowledge-audit",
        "cli-repair-wiring",
    ]);
    // Pln3 bidirectional action suggestions (expose to CLI as `touring suggest action|list|stats|gc|consumed`)
    names.extend_from_slice(&[
        "cli-suggest-action",
        "cli-suggestion-list",
        "cli-suggestion-stats",
        "cli-suggestion-gc",
        "cli-suggestion-consumed",
    ]);
    // Feature B (2026-04-24): workflow CLI
    names.extend_from_slice(&[
        "cli-workflow-run",
        "cli-workflow-stats",
        "cli-workflow-slowest",
        "cli-workflow-compare",
        "cli-workflow-resume",
        "cli-workflow-status",
        // Wave 8 collateral fix (2026-04-26): tasksfile hooks were registered
        // in the dispatch table (line 1277-1281) but missing from this names
        // list, causing `dispatch_table_entries_are_in_registry` test to fail.
        "cli-tasksfile-validate",
        "cli-tasksfile-export",
        "cli-devrcfile-import",
        "cli-devrcfile-export",
    ]);
    // Graph-viz (D3, D6): visual encoding integration + flow A→B path enumeration
    names.extend_from_slice(&[
        "cli-graph-flow",
        "cli-viz-workspace",
        "cli-viz-blast",
        "cli-viz-wiring",
        "cli-viz-cycles",
        "cli-viz-orphans",
        "cli-viz-feature",
    ]);
    // ACP (feature `acp-protocol`). Gate idêntico ao de `build_dispatch_table`:
    // as duas listas são fontes da verdade paralelas, e até 03/08/2026 só a
    // tabela conhecia estes dois hooks. Sob `--all-features` eles ficavam
    // despacháveis porém invisíveis à introspecção — `all_daemon_hook_names` é
    // o que enumera a superfície do daemon. `dispatch_table_and_registry_are_in_sync`
    // é o teste que guarda a paridade; ele reprovava exatamente aqui.
    #[cfg(feature = "acp-protocol")]
    names.extend_from_slice(&["cli-acp-message", "cli-acp-discover"]);

    names
}

/// Backward-compatible constant for callers that need a static slice.
/// With all default features enabled, this is identical to the previous
/// `ALL_DAEMON_HOOK_NAMES` constant.
pub const ALL_DAEMON_HOOK_NAMES: &[&str] = &[
    // Pre-hooks
    "pre-read",
    "pre-bash",
    "pre-edit",
    "pre-edit-prevention",
    "pre-grep",
    "pre-glob",
    "pre-write",
    "pre-tool-use",
    "cli-suggest",
    "ceg-observe",
    // Post-hooks
    "post-read",
    "post-bash",
    "post-edit",
    "post-write",
    "post-tool-batch",
    "post-tool-rl",
    "post-tool-failure",
    "post-tool-use",
    // Session
    "session-start",
    "session-stop",
    // Team (N1)
    "teammate-idle",
    "task-completed",
    "task-created",
    "task-validation",
    "task-metrics",
    "task-escalation",
    // Team gate hooks (anti-limbo + bootstrap)
    "teammate-idle-gate",
    "subagent-bootstrap",
    // Lifecycle (S4)
    "subagent-start",
    "subagent-stop",
    "file-changed",
    "cwd-changed",
    "pre-compact",
    "post-compact",
    "worktree-create",
    "instructions-loaded",
    "stop",
    // TACO Phase Protocol: UserPromptSubmit — emitted by prompt_enhance.rs
    "user_prompt_submit",
    // Plan mode lifecycle (R7)
    "enter-plan-mode",
    "exit-plan-mode",
    // Claude Code Task ↔ Touring Decompose sync (R7-Sync)
    "task-sync-create",
    "task-sync-update",
    "task-sync-list",
    // Claude Code Task output/get sync (R8-Sync)
    "task-sync-output",
    "task-sync-get",
    // Claude Code Task stop/delete sync (R9-Sync)
    "task-sync-stop",
    "task-sync-delete",
    // CLI telemetry handlers
    "cli-learning-status",
    "cli-learning-reward",
    "cli-granularity-status",
    "cli-granularity-reset",
    "cli-granularity-hint",
    "cli-saga-register",
    "cli-saga-prepare",
    "cli-saga-decide",
    "cli-saga-delta",
    "cli-saga-begin",
    "cli-saga-status",
    "cli-saga-abort",
    "cli-cascade-queue-status",
    "cli-cascade-queue-drain",
    "cli-wiring-status",
    "cli-wiring-orphans",
    "cli-wiring-modules",
    "cli-wiring-suggest",
    "cli-wiring-impact",
    "cli-wiring-cycles",
    "cli-cognitive-metrics",
    "cli-cognitive-engines",
    "cli-flywheel-status",
    "cli-incremental-status",
    "cli-gotcha-list",
    "cli-gotcha-add",
    "cli-gotcha-match",
    "cli-gotcha-stats",
    "cli-memory-stats",
    "cli-memory-recall",
    "cli-memory-credit",
    "cli-memory-store",
    "cli-memory-list",
    "cli-memory-reindex",
    "cli-evolution-drift",
    "cli-evolution-insights",
    "cli-evolution-tools",
    // D10-S1: touring-web health dashboard handlers
    "cli-doctor",
    "cli-status",
    // CLI session handlers
    "cli-session-start",
    "cli-session-checkpoint",
    "cli-session-list",
    "cli-session-assess",
    // CLI suggest handlers
    "cli-suggest-next",
    "cli-suggest-skill",
    // CLI decompose handlers
    "cli-decompose-create",
    "cli-decompose-add",
    "cli-decompose-get",
    "cli-decompose-update",
    "cli-decompose-validate",
    "cli-decompose-status",
    "cli-decompose-event",
    "cli-decompose-finalize",
    "cli-decompose-ready",
    // CLI mcts handlers
    "cli-mcts-search",
    // CLI shadow handlers
    "cli-shadow-validate",
    // CLI index handlers
    "cli-index-status",
    "cli-index-search",
    "cli-index-find",
    "cli-index-files",
    "cli-index-rebuild",
    "cli-index-ingest",
    // CLI ast handlers
    "cli-ast-find",
    "cli-ast-overview",
    "cli-ast-blast",
    "cli-ast-semantic",
    "cli-ast-quality",
    "cli-ast-calls",
    "cli-ast-heat",
    "cli-ast-modules",
    "cli-ast-scope",
    "cli-ast-detail",
    "cli-ast-imports",
    "cli-ast-grep",
    "cli-ast-scan",
    // D.2: resolve-def / find-references / rename
    "cli-resolve-def",
    "cli-find-references",
    "cli-rename",
    // CLI e2e handler
    "cli-e2e",
    // CLI pre-task-scout handler
    "cli-pre-task-scout",
    // CLI prompt-enhance handler
    "cli-prompt-enhance",
    // CLI hook-memory handlers
    "cli-hook-memory-store",
    "cli-hook-memory-recall",
    // CLI entity disambiguation handlers
    "cli-entity-resolve",
    "cli-entity-list",
    "cli-entity-generic-patterns",
    // L7-B Gamma: gate metrics + inferlets + jobs handlers
    "cli-gate-metrics",
    // R5/OP1: unified 6-dimension harness-quality metric
    "cli-harness-metric",
    // S-09/R8: formal change-contract gating self-mutation
    "cli-change-contract",
    // B-5/R10: distill historical bash-outcome substrate into an action predictor
    "cli-predict-action",
    // S-08/A-A1: conformal calibration of the skill-selection gate (KnowNo)
    "cli-calibrate-confidence",
    // Cold-start cluster: opt-in cross-project RL warm-start (real experience replay)
    "cli-rl-warmstart",
    // ES4 P1: durable action world-model liveness probe (persist + warm-load)
    "cli-world-model-status",
    // ES2 P2: constitutional sink-token contract attestation (EAGLE B-6)
    "cli-attest-contract",
    // ES1 P4 (CAH roadmap 2026-05-30): standalone SMT service handler
    "cli-prove-claim",
    // Wave 6 (2026-05-23): CEG observability boundary fix — CLI→daemon mirror
    "cli-gate-event",
    // Wave 26: Tokio runtime metrics
    "cli-tokio-metrics",
    // Wave 15: health_delta direct CLI exposure
    "cli-health-delta-status",
    "cli-health-delta-reset",
    // Wave I (Phase 5): grow-only audit trail query
    "cli-health-delta-history",
    // Wave A A.4: profile_query CLI exposure
    "cli-profile-status",
    // A.2.5 (2026-04-29): skip-region CLI exposure
    "cli-skip-list",
    "cli-skip-validate",
    // D27: plugin DI CLI
    "cli-plugin-list",
    "cli-plugin-status",
    "cli-plugin-reload",
    "cli-plugin-unregister",
    // L7-B Gamma: inferlets + jobs handlers
    "cli-inferlets-list",
    "cli-inferlets-run",
    "cli-jobs-spawn",
    "cli-jobs-poll",
    "cli-jobs-list",
    "cli-jobs-drop",
    "cli-ast-todos",
    "cli-ast-rationale",
    "cli-ast-features",
    "cli-ast-meta",
    "cli-ast-skeleton",
    "cli-ast-tdg",
    "cli-gotcha-sync",
    "cli-gotcha-init",
    "cli-repo-score",
    "cli-kpi",
    "cli-repo-health",
    "cli-mutation-test",
    "cli-ast-blast-enriched",
    "cli-wiring-purpose",
    "cli-search-symbols",
    "cli-search-docs",
    "cli-query-dsl",
    "cli-metadata-backfill",
    "cli-session-summary",
    "cli-bench-run",
    // Tantivy BM25 full-text search handlers
    "cli-tantivy-search",
    "cli-tantivy-fuzzy",
    "cli-tantivy-stats",
    "cli-tantivy-suggest",
    "cli-tantivy-reindex",
    // Wiring community handler
    "cli-wiring-community",
    // Wave cross-audit: functional chains, cross-feature blast radius, enriched file knowledge
    "cli-wiring-chains",
    "cli-ast-blast-cross-feature",
    "cli-file-knowledge-extended",
    "cli-file-knowledge-stats",
    "cli-file-knowledge-populate",
    "cli-file-knowledge-audit",
    "cli-repair-wiring",
    // Pln3 bidirectional action suggestions
    "cli-suggest-action",
    "cli-suggestion-list",
    "cli-suggestion-stats",
    "cli-suggestion-gc",
    "cli-suggestion-consumed",
    // Feature B (2026-04-24): workflow CLI
    "cli-workflow-run",
    "cli-workflow-stats",
    "cli-workflow-slowest",
    "cli-workflow-compare",
    "cli-workflow-resume",
    "cli-workflow-status",
    // P1.6 (2026-04-25): mpatch-fuzzy dry-run patch preview
    "cli-mpatch-preview",
    // Wave 8 collateral fix (2026-04-26): tasksfile hooks were registered
    // in dispatch table but missing from this constant.
    "cli-tasksfile-validate",
    "cli-tasksfile-export",
    // Wave 9 collateral fix (2026-04-26): same drift for devrcfile pair —
    // dispatch table (lines ~1294/1297) and `all_daemon_hook_names()`
    // (lines ~290/291) both list these hooks, but the backward-compat
    // constant lagged behind. REGRA #0 — encontrar é corrigir.
    "cli-devrcfile-import",
    "cli-devrcfile-export",
    // Graph-viz (D3, D6): visual encoding + flow A→B
    "cli-graph-flow",
    "cli-viz-workspace",
    "cli-viz-blast",
    "cli-viz-wiring",
    "cli-viz-cycles",
    "cli-viz-orphans",
    "cli-viz-feature",
    // D31: touring definitions CLI (classify / node-types / semantic-search)
    "cli-definitions-classify",
    "cli-definitions-nodetypes",
    "cli-definitions-semantic-search",
];

/// Build the O(1) dispatch table for daemon hook handlers.
///
/// Called once via `OnceLock::get_or_init` in `daemon.rs`.
/// Feature-gated hooks use `#[cfg]` on the insert statements.
/// All handlers are wrapped with LatencyMarker for observability.
pub fn build_dispatch_table() -> HashMap<&'static str, HookHandler> {
    let mut m: HashMap<&'static str, HookHandler> = HashMap::new();

    // ── Pre-hooks (feature-gated) ──────────────────────────────────
    #[cfg(feature = "pre-hooks")]
    {
        m.insert("pre-read", |rt, v| {
            crate::pre_read::run_returning(rt, v).to_json()
        });
        m.insert("pre-bash", |rt, v| {
            crate::pre_bash::run_returning(rt, v).to_json()
        });
        m.insert("pre-edit", |rt, v| {
            // HDG-8: pre-edit needs &mut HookRuntime for cognitive_mcts_trigger bridge.
            crate::pre_edit::run_returning(rt, v).to_json()
        });
        m.insert("pre-edit-prevention", |rt, v| {
            crate::pre_edit_prevention::run_returning(rt, v).to_json()
        });
        m.insert("pre-grep", |rt, v| {
            crate::pre_grep::run_returning(rt, v).to_json()
        });
        m.insert("pre-glob", |rt, v| {
            crate::pre_glob::run_returning(rt, v).to_json()
        });
        m.insert("pre-write", |rt, v| {
            crate::pre_write::run_returning(rt, v).to_json()
        });
        // S-01 (elite-harness 2026-05-29) — universal CEG observability hook.
        // Runtime-free (the `_rt` is ignored): drives X0..X7 against every
        // code-bearing tool call for `ceg_*` counter side-effects, fail-open.
        m.insert("ceg-observe", |_rt, v| {
            crate::ceg_adapter::run_observe_only(v).to_json()
        });
        m.insert("pre-tool-use", |rt, v| {
            crate::pre_tool_use::run_returning(rt, v).to_json()
        });
        // PreToolUse classifier — suggests the best Touring CLI command(s) for
        // (tool_name, tool_input). Returns JSON-as-String directly (no adapter).
        m.insert("cli-suggest", |rt, v| crate::cli_suggester::run(rt, v));
    }

    // ── Post-hooks (feature-gated) ─────────────────────────────────
    #[cfg(feature = "post-hooks")]
    {
        m.insert("post-read", |rt, v| {
            crate::post_read::run_returning(rt, v).to_json()
        });
        // Sprint 4.6 fix (2026-05-23): use run_returning + .to_json() pattern
        // like every other hook entry in this dispatch table. The previous
        // code called `crate::post_bash::run(rt, v)` whose body ends with
        // `run_returning(input).emit()` → `process::exit(0)` — which kills
        // the ENTIRE daemon process every time post-bash fires. That was the
        // root cause of intermittent silent daemon death documented in
        // Sprint 4.5 (no panic_log entry, no graceful_shutdown log, no
        // signal — clean exit_group(0) from a tokio worker thread under
        // load, verified via Sprint 4.6 strace wrapper). The .emit() path
        // is correct for the one-shot touring-hook binary (where exit is
        // intentional) but FATAL when reached from the long-running daemon
        // dispatch. Switching to run_returning(...).to_json() returns the
        // serialized response to the caller (the daemon actor) instead of
        // calling exit.
        m.insert("post-bash", |rt, v| {
            crate::post_bash::run_returning(rt, v).to_json()
        });
        m.insert("post-edit", |rt, v| {
            crate::post_edit::run_returning(rt, v).to_json()
        });
        m.insert("post-write", |rt, v| {
            crate::post_write::run_returning(rt, v).to_json()
        });
        m.insert("post-tool-batch", |rt, v| {
            crate::post_tool_batch::run_post_tool_batch(rt, v).to_json()
        });
        m.insert("post-tool-rl", |rt, v| {
            let _ = crate::post_tool_rl::run(rt, v);
            // R132: Generator hints on RL error signal — closes the RL degradation → generator loop.
            // post-tool-rl was completely silent. Now emits targeted generator hints when tool fails:
            // Edit/Write failure → IncrementalPatch + Test to fix and capture the partial change.
            // Bash failure → Test generator to add coverage for the failing scenario.
            // Fires only on error to avoid noise on success paths (success → String::new()).
            let tool_name = v
                .get("tool_name")
                .or_else(|| v.pointer("/tool_input/tool_name"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let has_error = v
                .get("exit_code")
                .and_then(|c| c.as_i64())
                .map(|c| c != 0)
                .unwrap_or_else(|| {
                    v.get("tool_output")
                        .or_else(|| v.get("output"))
                        .and_then(|o| o.as_str())
                        .map(|o| {
                            o.contains("error[E") || o.contains("FAILED") || o.contains("panicked")
                        })
                        .unwrap_or(false)
                });
            if has_error && !tool_name.is_empty() {
                let truncated_tool = &tool_name[..tool_name.len().min(30)];
                let hint = match tool_name {
                    "Edit" | "Write" | "MultiEdit" => serde_json::json!({
                        "type": "rl_edit_error",
                        "tool": truncated_tool,
                        "actions": [
                            {"cmd": "touring generate render IncrementalPatch --vars '{\"file_path\":\"<target_file>\"}'", "purpose": "capture partial change"},
                            {"cmd": "touring generate render Test --vars '{\"module_name\":\"rl_edit_regression\"}'", "purpose": "add regression test"}
                        ]
                    }),
                    "Bash" => serde_json::json!({
                        "type": "rl_bash_error",
                        "tool": truncated_tool,
                        "actions": [
                            {"cmd": "touring generate render Test --vars '{\"module_name\":\"bash_regression\"}'", "purpose": "test failing scenario"},
                            {"cmd": "touring memory recall", "purpose": "find failure patterns", "query": format!("failure:{}", truncated_tool)}
                        ]
                    }),
                    _ => serde_json::Value::Null,
                };
                serde_json::to_string(&hint).unwrap_or_default()
            } else {
                String::new()
            }
        });
        m.insert("post-tool-failure", |rt, v| {
            crate::post_tool_failure::run_returning(rt, v).to_json()
        });
        m.insert("post-tool-use", |rt, v| {
            crate::post_tool_use::run_returning(rt, v).to_json()
        });
    }

    // ── Session hooks (feature-gated) ──────────────────────────────
    #[cfg(feature = "session-hooks")]
    {
        m.insert("session-start", |rt, v| {
            let _ = crate::session_hooks::run_session_start(rt, v);
            String::new()
        });
        m.insert("session-stop", |rt, v| {
            let _ = crate::session_hooks::run_session_stop(rt, v);
            String::new()
        });
    }

    // ── Team hooks — N1: Agent Teams ↔ ACO Gateway (always-on) ────
    m.insert("teammate-idle", |rt, v| {
        let _ = crate::team_hooks::run_teammate_idle(rt, v);
        String::new()
    });
    m.insert("task-completed", |rt, v| {
        let _ = crate::team_hooks::run_task_completed(rt, v);
        // R20-S1: Execute finalize + RL reward + session assess inline — no longer hint-only.
        // TaskCompleted event → cli_decompose_finalize (archive) + cli_learning_reward (RL signal).
        let task_id = v.get("task_id").and_then(|s| s.as_str()).unwrap_or("unknown");
        let success = v.get("success").and_then(|b| b.as_bool()).unwrap_or(true);
        let (reward_val, outcome_label) = if success { (1.0_f64, "success") } else { (-0.5_f64, "failed") };

        // R32-S3: Ensure ::validate and ::implement are terminal before finalize.
        // Fixes archive failure when TaskCompleted fires while subtasks are still pending.
        // Uses success/failure status to mirror the task outcome into the DAG.
        let terminal_status = if success { "completed" } else { "failed" };
        let _ = crate::cli_handlers::cli_decompose_update(rt, &serde_json::json!({
            "task_id": format!("{task_id}::validate"),
            "status": terminal_status,
        }));
        let _ = crate::cli_handlers::cli_decompose_update(rt, &serde_json::json!({
            "task_id": format!("{task_id}::implement"),
            "status": terminal_status,
        }));

        // 1. Auto-finalize: archive DAG, inject RL 1.0 internally if all subtasks terminal.
        let finalize_payload = serde_json::json!({"task_id": task_id});
        let finalize_result = crate::cli_handlers::cli_decompose_finalize(rt, &finalize_payload);
        let archived = finalize_result.contains("\"archived\":true");

        // 2. Direct RL reward injection — no manual step needed.
        let reward_payload = serde_json::json!({
            "tool": "orchestrate",
            "reward": reward_val,
            "context": format!("task:{task_id}:{outcome_label}")
        });
        let _ = crate::cli_handlers::cli_learning_reward(rt, &reward_payload);
        tracing::debug!(task_id, success, reward_val, archived, "task-completed: RL reward + finalize applied");

        // R36-S3: Diary hint — emit concrete `touring diary write` command so Claude Code
        // persists AAAK-format agent memory for this task completion event.
        let diary_hint = format!(
            " | diary: run `touring diary write claude_code \"#{task_id}:{outcome_label}\" --aaak` to persist agent memory"
        );
        // R122-S2: Generator hints on task-completed — surfaces all matching GeneratorKind
        // scaffold commands for the completed task subject. Closes the CC TaskCreate→TaskCompleted
        // sync loop: the same multi-kind hints that guided implementation now appear at completion
        // to help Claude Code verify, iterate, or document the generated artifact.
        let subject_at_completion = v.get("task_subject").and_then(|s| s.as_str()).unwrap_or("");
        let generator_hint = if subject_at_completion.len() > 3 {
            let hints = crate::lifecycle::collect_subject_generator_hints(subject_at_completion);
            if !hints.is_empty() {
                format!(" | produced: {}", hints.join(" | "))
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        // R37-S4: Auto-assess Touring session on task completion — closes the session lifecycle.
        // Mirrors R37-S3 (TaskStop → session_assess) for the TaskCompleted event path.
        // Uses cli_session_assess directly (same logic as session_assess_on_task_stop in lifecycle.rs).
        let assess_hint = {
            let assess_payload = serde_json::json!({"session_id": task_id});
            let assess_result = crate::cli_handlers::cli_session_assess(rt, &assess_payload);
            match serde_json::from_str::<serde_json::Value>(&assess_result) {
                Ok(v) => {
                    if let Some(score) = v.get("quality_score").and_then(|s| s.as_f64()) {
                        format!(" | session-assessed: quality={score:.2}")
                    } else {
                        String::new()
                    }
                }
                _ => String::new(),
            }
        };
        // R128: Wiring-orphan check + ConsumerGenerator hint — closes artifact→consumer_wiring loop.
        // After a successful task, new pub symbols may have been introduced. Surface orphan check
        // and ConsumerGenerator scaffold hint so Claude Code can immediately wire new artifacts.
        let wiring_orphan_hint = crate::lifecycle::maybe_wiring_orphan_hint_on_complete(task_id);
        let consumer_gen_hint = if !wiring_orphan_hint.is_empty() {
            let truncated_id = &task_id[..task_id.len().min(40)];
            format!(
                " | consumer-wire: run `touring generate render ConsumerGenerator \
                --vars '{{\"source_module\":\"{truncated_id}\",\"event\":\"task:{truncated_id}:completed\"}}'` \
                to scaffold consumer for new artifacts if orphans detected"
            )
        } else {
            String::new()
        };
        if success {
            let finalize_status = if archived { "DAG archived" } else { "DAG partial (pending subtasks)" };
            // R23-S2: Auto-store lesson on task completion — no manual step needed.
            let _ = crate::cli_handlers::cli_memory_store(rt, &serde_json::json!({
                "key": format!("task:{task_id}:completed"),
                "value": format!("Task {task_id} completed successfully — finalize_status={finalize_status}"),
                "tier": "semantic",
                "entry_type": "lesson",
            }));
            // R130: Store artifact subject mapping — enables `touring memory recall "artifact:<task_id>"`.
            // Closes the gap: completed task subject → memory graph → cross-session retrieval.
            // Future sessions can `touring memory recall "artifact:<task_id>"` to find the artifact.
            if subject_at_completion.len() > 3 {
                let truncated_subject = &subject_at_completion[..subject_at_completion.len().min(200)];
                let _ = crate::cli_handlers::cli_memory_store(rt, &serde_json::json!({
                    "key": format!("artifact:{task_id}"),
                    "value": format!("Task {task_id} artifact: {truncated_subject} — finalize={finalize_status}"),
                    "tier": "semantic",
                    "entry_type": "lesson",
                }));
            }
            // R134: Plan-recall replay hint — enables rapid iteration on the same subject.
            // After a successful task, Claude Code can replay the same generator pipeline
            // on a new subject by recalling the most similar plan from memory. This closes
            // the task-completed → plan-replay → next-iteration loop that allows automation
            // of repeated artifact generation without re-planning from scratch each time.
            let plan_replay_hint = if subject_at_completion.len() > 3 {
                let short_subject = &subject_at_completion[..subject_at_completion.len().min(60)];
                format!(
                    " | replay: run `touring generate plan-recall --query \"{short_subject}\"` \
                    to find and replay generator plan on a new subject"
                )
            } else {
                String::new()
            };
            serde_json::json!({
                "event": "task-completed",
                "task_id": task_id,
                "finalize_status": finalize_status,
                "RL": "reward_signal",
                "rl_reward": reward_val,
                "outcome": outcome_label,
                "lesson_stored": true,
                "diary_hint": diary_hint.trim(),
                "generator_hint": generator_hint.trim(),
                "assess_hint": assess_hint.trim(),
                "wiring_orphan_hint": wiring_orphan_hint.trim(),
                "consumer_gen_hint": consumer_gen_hint.trim(),
                "plan_replay_hint": plan_replay_hint.trim(),
                "verify_cmd": format!("touring decompose get {}", task_id)
            }).to_string()
        } else {
            let _ = crate::cli_handlers::cli_decompose_update(rt, &serde_json::json!({"task_id": task_id, "status": "failed"}));
            // R23-S2: Auto-store failure lesson — closes the manual-store gap from the hint.
            let _ = crate::cli_handlers::cli_memory_store(rt, &serde_json::json!({
                "key": format!("task:{task_id}:failed"),
                "value": format!("Task {task_id} failed — investigate root cause; run `touring memory recall \"task:{task_id}\"` for history"),
                "tier": "semantic",
                "entry_type": "lesson",
            }));
            // R131: Recovery generator hints for task failure — symmetric with R126 (task-escalation).
            // Surfaces ErrorCatalog (document failure pattern), recovery plan-suggest, and evolution
            // drift check so Claude Code can immediately scaffold a recovery path after failure.
            let truncated_id = &task_id[..task_id.len().min(40)];
            let recovery_hints = format!(
                " | error-catalog: run `touring generate render ErrorCatalog \
                --vars '{{\"crate_name\":\"{truncated_id}\",\"error_codes\":[]}}'` \
                to document failure pattern \
                | recovery: run `touring generate plan-suggest --intent \"recover: {truncated_id}\"` \
                to scaffold recovery plan \
                | drift: run `touring evolution drift -j` to detect systemic degradation"
            );
            serde_json::json!({
                "event": "task-completed(failed)",
                "task_id": task_id,
                "finalize_status": "failed",
                "rl_reward": reward_val,
                "outcome": outcome_label,
                "lesson_stored": true,
                "diary_hint": diary_hint.trim(),
                "generator_hint": generator_hint.trim(),
                "assess_hint": assess_hint.trim(),
                "recovery_hints": recovery_hints.trim(),
                "review_cmd": format!("touring memory recall \"task:{}\"", task_id)
            }).to_string()
        }
    });
    m.insert("task-created", |rt, v| {
        let _ = crate::team_hooks::run_task_created(rt, v);
        // R20-S2: Scaffold 3 standard subtasks directly — closes the sync gap with PostToolUse(TaskCreate).
        // TaskCreated event → cli_decompose_add ×3 → scout/implement/validate DAG wired immediately.
        let task_id = v.get("task_id").and_then(|s| s.as_str()).unwrap_or("unknown");
        let subject = v.get("task_subject").and_then(|s| s.as_str()).unwrap_or("");
        let s1 = format!("{task_id}::scout");
        let s2 = format!("{task_id}::implement");
        let s3 = format!("{task_id}::validate");
        let _ = crate::cli_handlers::cli_decompose_add(rt, &serde_json::json!({
            "task_id": task_id, "subtask_id": &s1,
            "description": "scout: research context, blast radius, and wiring before changes",
            "depends_on": [],
        }));
        let _ = crate::cli_handlers::cli_decompose_add(rt, &serde_json::json!({
            "task_id": task_id, "subtask_id": &s2,
            "description": "implement: apply changes with VGP verification and speculative validation",
            "depends_on": [&s1],
        }));
        let _ = crate::cli_handlers::cli_decompose_add(rt, &serde_json::json!({
            "task_id": task_id, "subtask_id": &s3,
            "description": "validate: cargo test + wiring orphans + memory store lesson",
            "depends_on": [&s2],
        }));
        // R36-S1: Auto-start Touring session for each CC task — every task gets its own session.
        // TaskCreated event → cli_session_start → SQLite session row + cognitive init.
        // Session ID = task_id so later checkpoint/assess calls link back automatically.
        let session_started = {
            let sess_payload = serde_json::json!({
                "session_id": task_id,
                "task_type": "task",
                "objective": &subject[..subject.len().min(200)],
            });
            let result = crate::cli_handlers::cli_session_start(rt, &sess_payload);
            result.contains("started") || result.contains("session_id")
        };
        let session_hint = if session_started {
            format!(" | session:{task_id} started")
        } else {
            String::new()
        };
        // R28-S2: RL +0.1 intent signal — task creation marks the beginning of tracked work.
        // Injects a small positive reward so the RL engine builds a signal for task creation intent.
        let _ = crate::cli_handlers::cli_learning_reward(rt, &serde_json::json!({
            "tool": "orchestrate",
            "reward": 0.1_f64,
            "context": format!("task:{task_id}:created"),
        }));
        // R122-S1: Upgrade task-created from first-match-wins (suggest_generator_for_task_subject)
        // to all-matching-kinds dispatch (collect_subject_generator_hints) — surfaces ALL relevant
        // GeneratorKind hints when a task subject spans multiple artifact domains (e.g. "add rust
        // module with openapi spec and tests" → RustModule + OpenApiSpec + Test in one pass).
        let generator_hint = if subject.len() > 3 {
            let hints = crate::lifecycle::collect_subject_generator_hints(subject);
            if !hints.is_empty() {
                format!(" | {}", hints.join(" | "))
            } else {
                format!(" | generator: `touring generate plan-suggest --intent \"{}\"` to scaffold artifacts",
                    &subject[..subject.len().min(80)])
            }
        } else { String::new() };
        // R33-S3: Recall past plans for the same subject — surfaces related generator plans
        // from memory so the engineer can resume or adapt prior work instead of starting fresh.
        let recall_hint = if subject.len() > 3 {
            let short_subject = &subject[..subject.len().min(60)];
            let recall_payload = serde_json::json!({"query": short_subject, "limit": 1});
            let recall_result = crate::cli_handlers::cli_memory_recall(rt, &recall_payload);
            if recall_result.contains("\"entries\"") && recall_result.contains("\"value\"") {
                format!(" | past-plans: run `touring generate plan-recall --query \"{short_subject}\"` to find related plans")
            } else {
                String::new()
            }
        } else { String::new() };
        serde_json::json!({
            "event": "task-created",
            "task_id": task_id,
            "scaffolded": "scout→implement→validate",
            "generator_hint": generator_hint.trim(),
            "recall_hint": recall_hint.trim(),
            "session_hint": session_hint.trim(),
            "RL": "reward_signal",
            "rl_reward": 0.1,
            "verify_cmd": format!("touring decompose ready {}", task_id)
        }).to_string()
    });
    m.insert("task-metrics", |rt, v| {
        // R124: Quality-driven generator hints — closes the quality→generator feedback loop.
        // Extract context from HookResponse directly (no .emit() — that diverges via process::exit).
        // High quality (≥0.8): preserve pattern via DiaryEntry + SkillDocument generators.
        // Low quality (>0 and <0.5): improve coverage via Test + Benchmark generators.
        // Low success rate (<0.5): surface memory recall for similar failure patterns.
        let response = crate::hooks_task_lifecycle::run_task_metrics(rt, v);
        let (base_context, event_name) = match response {
            crate::hook_runtime::HookResponse::Context { context, event_name } => (context, event_name),
            other => return other.to_json(),
        };
        let quality_score = v.get("quality_score").and_then(|q| q.as_f64()).unwrap_or(0.0);
        let task_id = v.get("task_id").and_then(|s| s.as_str()).unwrap_or("unknown");
        let quality_hint = if quality_score >= 0.8 {
            format!(
                " | high-quality({quality_score:.2}): preserve pattern — \
                run `touring generate render DiaryEntry --vars '{{\"agent\":\"claude_code\",\"task_id\":\"{task_id}\"}}'` \
                | run `touring generate render SkillDocument --vars '{{\"skill_name\":\"{task_id}\"}}'`"
            )
        } else if quality_score > 0.0 && quality_score < 0.5 {
            format!(
                " | low-quality({quality_score:.2}): improve coverage — \
                run `touring generate render Test --vars '{{\"module_name\":\"{task_id}\"}}'` \
                | run `touring generate render Benchmark --vars '{{\"module_name\":\"{task_id}\"}}'`"
            )
        } else {
            String::new()
        };
        let success_rate = v.get("success_rate").and_then(|s| s.as_f64()).unwrap_or(1.0);
        let success_hint = if success_rate < 0.5 {
            format!(
                " | low-success({:.0}%): run `touring memory recall \"failure:{task_id}\"` for similar past patterns",
                success_rate * 100.0
            )
        } else {
            String::new()
        };
        tracing::debug!(task_id, quality_score, success_rate, "task-metrics: R124 quality-driven generator hints");
        let combined = format!("{base_context}{quality_hint}{success_hint}");
        crate::hook_runtime::HookResponse::Context { context: combined, event_name }.to_json()
    });
    m.insert("task-validation", |rt, v| {
        // Fix: use pattern-matching on HookResponse instead of .emit() (which diverges via
        // process::exit, making all subsequent RL/R125 logic dead code). This also makes
        // the handler safe to call from tests and unit code.
        let response = crate::hooks_task_lifecycle::run_task_validation(rt, v);
        let (base_context, event_name) = match response {
            crate::hook_runtime::HookResponse::Context { context, event_name } => (context, event_name),
            other => return other.to_json(),
        };
        // R24-S1: Close RL feedback loop for DAG validation outcomes.
        // Valid → reward +0.5 (planning quality signal); invalid → -0.3 + lesson stored.
        let task_id = v.get("task_id").and_then(|s| s.as_str()).unwrap_or("unknown");
        let is_valid = base_context.contains("validation passed");
        let (reward_val, outcome_label) = if is_valid { (0.5_f64, "valid") } else { (-0.3_f64, "cycle_detected") };
        let _ = crate::cli_handlers::cli_learning_reward(rt, &serde_json::json!({
            "tool": "orchestrate",
            "reward": reward_val,
            "context": format!("task-validation:{task_id}:{outcome_label}"),
        }));
        if !is_valid {
            let _ = crate::cli_handlers::cli_memory_store(rt, &serde_json::json!({
                "key": format!("task:{task_id}:validation_failed"),
                "value": format!("Task {task_id} DAG validation failed — check for dependency cycles"),
                "tier": "semantic",
                "entry_type": "lesson",
            }));
        }
        tracing::debug!(task_id, is_valid, reward_val, "task-validation: RL reward injected");
        // R125: When DAG validation passes, surface generator hints for the validated task.
        // Closes the loop: valid DAG → what to scaffold next for this task subject.
        let gen_hint = if is_valid && task_id.len() > 3 {
            let hints = crate::lifecycle::collect_subject_generator_hints(task_id);
            if !hints.is_empty() {
                format!(" | scaffold-next: {}", hints.join(" | "))
            } else {
                let truncated = &task_id[..task_id.len().min(60)];
                format!(" | scaffold-next: run `touring generate plan-suggest --intent \"{truncated}\"` to choose generator")
            }
        } else {
            String::new()
        };
        let combined = format!("{base_context}{gen_hint}");
        crate::hook_runtime::HookResponse::Context { context: combined, event_name }.to_json()
    });

    // ── Team gate hooks — anti-limbo + bootstrap (returns JSON output) ────
    m.insert("teammate-idle-gate", |rt, v| {
        let result = crate::team_hooks::run_teammate_idle_gate(rt, v);
        serde_json::to_string(&result).unwrap_or_default()
    });
    m.insert("subagent-bootstrap", |_rt, _v| {
        crate::team_hooks::subagent_bootstrap_context()
    });

    // ── Subagent lifecycle — S4: intelligent handlers ────────────────
    m.insert("subagent-start", |rt, v| {
        crate::lifecycle::handle_subagent_start(rt, v)
    });
    m.insert("subagent-stop", |rt, v| {
        let result = crate::team_hooks::run_subagent_stop_gate(rt, v);
        serde_json::to_string(&result).unwrap_or_default()
    });

    // ── S4: Lifecycle hooks — intelligent handlers ─────────────────
    m.insert("file-changed", |rt, v| {
        crate::lifecycle::handle_file_changed(rt, v)
    });
    m.insert("cwd-changed", |rt, v| {
        crate::lifecycle::handle_cwd_changed(rt, v)
    });
    m.insert("pre-compact", |rt, v| {
        crate::lifecycle::handle_pre_compact(rt, v)
    });
    m.insert("post-compact", |rt, v| {
        crate::post_compact_handler::run(rt, v)
    });
    m.insert("worktree-create", |rt, v| {
        crate::lifecycle::handle_worktree_create(rt, v)
    });
    // R7-S3: EnterPlanMode — surface generator suggestions at plan entry
    m.insert("enter-plan-mode", |rt, v| {
        crate::lifecycle::handle_enter_plan_mode(rt, v)
    });
    // R7-S4: ExitPlanMode — persist planning state at plan exit
    m.insert("exit-plan-mode", |rt, v| {
        crate::lifecycle::handle_exit_plan_mode(rt, v)
    });
    // R7-Sync: Claude Code Task ↔ Touring Decompose synchronization
    m.insert("task-sync-create", |rt, v| {
        crate::lifecycle::handle_task_sync_post_create(rt, v)
    });
    m.insert("task-sync-update", |rt, v| {
        crate::lifecycle::handle_task_sync_post_update(rt, v)
    });
    m.insert("task-sync-list", |rt, v| {
        crate::lifecycle::handle_task_sync_post_list(rt, v)
    });
    // R8-Sync: TaskOutput + TaskGet → Touring memory/decompose bridge
    m.insert("task-sync-output", |rt, v| {
        crate::lifecycle::handle_task_sync_post_output(rt, v)
    });
    m.insert("task-sync-get", |rt, v| {
        crate::lifecycle::handle_task_sync_post_get(rt, v)
    });
    // R9-Sync: TaskStop + TaskDelete → decompose cancelled + RL signal
    m.insert("task-sync-stop", |rt, v| {
        crate::lifecycle::handle_task_sync_post_stop(rt, v)
    });
    m.insert("task-sync-delete", |rt, v| {
        crate::lifecycle::handle_task_sync_post_delete(rt, v)
    });
    m.insert("instructions-loaded", |rt, v| {
        crate::instructions_loaded::run_returning(rt, v).to_json()
    });
    m.insert("stop", |rt, v| crate::stop::run_returning(rt, v).to_json());
    // TACO Phase Protocol: UserPromptSubmit — captures raw user prompt for
    // project attribution and CILA budget injection (prompt_enhance.rs).
    m.insert("user_prompt_submit", |rt, v| {
        crate::prompt_enhance::run_user_prompt_submit(rt, v).to_json()
    });

    // ── Task lifecycle hooks — S9: task escalation ───────────────
    // R126: Fix .emit() divergence bug + add generator hints for blocked/failed tasks.
    // Escalation events surface plan-suggest and memory recall so the orchestrator
    // can immediately scaffold a recovery plan or look up past failure patterns.
    m.insert("task-escalation", |rt, v| {
        let response = crate::hooks_task_lifecycle::run_task_escalation(rt, v);
        let (base_context, event_name) = match response {
            crate::hook_runtime::HookResponse::Context { context, event_name } => (context, event_name),
            other => return other.to_json(),
        };

        let task_id = v.get("task_id").and_then(|s| s.as_str()).unwrap_or("unknown");
        let failure_reason = v
            .get("failure_reason")
            .or_else(|| v.get("reason"))
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        let _ = crate::cli_handlers::cli_learning_reward(rt, &serde_json::json!({
            "tool": "orchestrate",
            "reward": -0.5_f64,
            "context": format!("task-escalation:{task_id}:failure"),
        }));
        let _ = crate::cli_handlers::cli_memory_store(rt, &serde_json::json!({
            "key": format!("task:{task_id}:escalation"),
            "value": format!("Task {task_id} escalated — reason: {failure_reason}"),
            "tier": "semantic",
            "entry_type": "lesson",
        }));

        tracing::debug!(task_id, failure_reason, "task-escalation: R126 RL penalty injected");

        let reason_kw = failure_reason.to_lowercase();
        let recovery_kind = if reason_kw.contains("dependency") || reason_kw.contains("conflict") {
            "resolve dependency conflicts"
        } else if reason_kw.contains("timeout") || reason_kw.contains("blocked") {
            "decompose blocked task into smaller subtasks"
        } else if reason_kw.contains("test") || reason_kw.contains("fail") {
            "add test coverage and fix failing tests"
        } else {
            "recover from task failure"
        };
        let truncated_id = &task_id[..task_id.len().min(40)];
        let recovery_hint = format!(
            " | recovery: run `touring generate plan-suggest --intent \"{recovery_kind}: {truncated_id}\"` \
            to scaffold a recovery plan \
            | run `touring memory recall \"escalation:{task_id}\"` for past failure patterns"
        );

        let combined = format!("{base_context}{recovery_hint}");
        crate::hook_runtime::HookResponse::Context { context: combined, event_name }.to_json()
    });

    // ── CLI command handlers (touring <subsystem> <command>) ─────────
    // These are NOT Claude Code hooks — they are daemon-side query handlers
    // for the CLI's telemetry/monitoring subcommands.
    m.insert("cli-learning-status", |rt, v| {
        crate::cli_handlers::cli_learning_status(rt, v)
    });
    m.insert("cli-learning-reward", |rt, v| {
        crate::cli_handlers::cli_learning_reward(rt, v)
    });
    m.insert("cli-granularity-status", |rt, v| {
        crate::cli_handlers::cli_granularity_status(rt, v)
    });
    m.insert("cli-granularity-reset", |rt, v| {
        crate::cli_handlers::cli_granularity_reset(rt, v)
    });
    m.insert("cli-granularity-hint", |rt, v| {
        crate::cli_handlers::cli_granularity_hint(rt, v)
    });
    m.insert("cli-cascade-queue-status", |rt, v| {
        crate::cli_handlers::cli_cascade_queue_status(rt, v)
    });
    m.insert("cli-cascade-queue-drain", |rt, v| {
        crate::cli_handlers::cli_cascade_queue_drain(rt, v)
    });
    // PLN2: saga distributed 2PC coordinator
    m.insert("cli-saga-register", |rt, v| {
        crate::cli_handlers::cli_saga_register(rt, v)
    });
    m.insert("cli-saga-prepare", |rt, v| {
        crate::cli_handlers::cli_saga_prepare(rt, v)
    });
    m.insert("cli-saga-decide", |rt, v| {
        crate::cli_handlers::cli_saga_decide(rt, v)
    });
    m.insert("cli-saga-delta", |rt, v| {
        crate::cli_handlers::cli_saga_delta(rt, v)
    });
    m.insert("cli-saga-begin", |rt, v| {
        crate::cli_handlers::cli_saga_begin(rt, v)
    });
    m.insert("cli-saga-status", |rt, v| {
        crate::cli_handlers::cli_saga_status(rt, v)
    });
    m.insert("cli-saga-abort", |rt, v| {
        crate::cli_handlers::cli_saga_abort(rt, v)
    });
    m.insert("cli-wiring-status", |rt, v| {
        crate::cli_handlers::cli_wiring_status(rt, v)
    });
    m.insert("cli-wiring-orphans", |rt, v| {
        crate::cli_handlers::cli_wiring_orphans(rt, v)
    });
    m.insert("cli-wiring-modules", |rt, v| {
        crate::cli_handlers::cli_wiring_modules(rt, v)
    });
    m.insert("cli-wiring-suggest", |rt, v| {
        crate::cli_handlers::cli_wiring_suggest(rt, v)
    });
    m.insert("cli-wiring-impact", |rt, v| {
        crate::cli_handlers::cli_wiring_impact(rt, v)
    });
    m.insert("cli-wiring-cycles", |rt, v| {
        crate::cli_handlers::cli_wiring_cycles(rt, v)
    });
    // ACP (Agent Client Protocol) handlers — bridges CLI dispatch to ACP socket protocol
    #[cfg(feature = "acp-protocol")]
    {
        m.insert("cli-acp-message", |rt, v| {
            crate::cli_handlers::cli_acp_message(rt, v)
        });
        m.insert("cli-acp-discover", |rt, v| {
            crate::cli_handlers::cli_acp_discover(rt, v)
        });
    }
    m.insert("cli-cognitive-metrics", |rt, v| {
        crate::cli_handlers::cli_cognitive_metrics(rt, v)
    });
    m.insert("cli-cognitive-engines", |rt, v| {
        crate::cli_handlers::cli_cognitive_engines(rt, v)
    });
    m.insert("cli-flywheel-status", |rt, v| {
        crate::cli_handlers::cli_flywheel_status(rt, v)
    });
    m.insert("cli-incremental-status", |rt, v| {
        crate::cli_handlers::cli_incremental_status(rt, v)
    });
    m.insert("cli-gotcha-list", |rt, v| {
        crate::cli_handlers::cli_gotcha_list(rt, v)
    });
    m.insert("cli-gotcha-add", |rt, v| {
        crate::cli_handlers::cli_gotcha_add(rt, v)
    });
    m.insert("cli-gotcha-match", |rt, v| {
        crate::cli_handlers::cli_gotcha_match(rt, v)
    });
    m.insert("cli-gotcha-stats", |rt, v| {
        crate::cli_handlers::cli_gotcha_stats(rt, v)
    });
    m.insert("cli-memory-stats", |rt, v| {
        crate::cli_handlers::cli_memory_stats(rt, v)
    });
    m.insert("cli-memory-recall", |rt, v| {
        crate::cli_handlers::cli_memory_recall(rt, v)
    });
    m.insert("cli-memory-credit", |rt, v| {
        crate::cli_handlers::cli_memory_credit(rt, v)
    });
    m.insert("cli-memory-store", |rt, v| {
        crate::cli_handlers::cli_memory_store(rt, v)
    });
    m.insert("cli-memory-list", |rt, v| {
        crate::cli_handlers::cli_memory_list(rt, v)
    });
    // S-04 (elite-harness 2026-05-29): ANN corpus backfill — `touring memory reindex`.
    m.insert("cli-memory-reindex", |rt, v| {
        crate::cli_handlers::cli_memory_reindex(rt, v)
    });
    // Hook-memory bridge handlers — wires hook events into knowledge graph
    m.insert("cli-hook-memory-store", |rt, v| {
        crate::cli_handlers::cli_hook_memory_store(rt, v)
    });
    m.insert("cli-hook-memory-recall", |rt, v| {
        crate::cli_handlers::cli_hook_memory_recall(rt, v)
    });
    // L7-B Gamma: gate metrics handler
    m.insert("cli-gate-metrics", |rt, v| {
        crate::cli_handlers::cli_gate_metrics(rt, v)
    });
    // R5/OP1: unified 6-dimension harness-quality metric handler
    m.insert("cli-harness-metric", |rt, v| {
        crate::cli_handlers::cli_harness_metric(rt, v)
    });
    // S-09/R8: formal change-contract gating self-mutation handler
    m.insert("cli-change-contract", |rt, v| {
        crate::cli_handlers::cli_change_contract(rt, v)
    });
    // B-5/R10: distill historical bash-outcome substrate into an action predictor
    m.insert("cli-predict-action", |rt, v| {
        crate::cli_handlers::cli_predict_action(rt, v)
    });
    // S-08/A-A1: conformal calibration of the skill-selection gate (KnowNo)
    m.insert("cli-calibrate-confidence", |rt, v| {
        crate::cli_handlers::cli_calibrate_confidence(rt, v)
    });
    // ES4 P1: durable action world-model liveness probe (persist + warm-load)
    m.insert("cli-world-model-status", |rt, v| {
        crate::cli_handlers::cli_world_model_status(rt, v)
    });
    // ES2 P2: constitutional sink-token contract attestation (EAGLE B-6)
    m.insert("cli-attest-contract", |rt, v| {
        crate::cli_handlers::cli_attest_contract(rt, v)
    });
    // ES1 P4 (CAH roadmap 2026-05-30): standalone SMT service
    m.insert("cli-prove-claim", |rt, v| {
        crate::cli_handlers::cli_prove_claim(rt, v)
    });
    // Cold-start cluster: opt-in cross-project RL warm-start (real experience replay)
    m.insert("cli-rl-warmstart", |rt, v| {
        crate::cli_handlers::cli_rl_warmstart(rt, v)
    });
    // Wave 6 (2026-05-23): CEG observability boundary fix — CLI→daemon mirror
    m.insert("cli-gate-event", |rt, v| {
        crate::cli_handlers::cli_gate_event(rt, v)
    });
    // D28: MCP overhead self-report handler
    m.insert("cli-mcp-overhead", |rt, v| {
        crate::cli_handlers::cli_mcp_overhead(rt, v)
    });
    // Wave 26 (2026-04-21): Tokio runtime metrics
    m.insert("cli-tokio-metrics", |rt, v| {
        crate::cli_handlers::cli_tokio_metrics(rt, v)
    });
    // Wave 15 (2026-04-18): health_delta direct CLI exposure
    m.insert("cli-health-delta-status", |rt, v| {
        crate::cli_handlers::cli_health_delta_status(rt, v)
    });
    m.insert("cli-health-delta-reset", |rt, v| {
        crate::cli_handlers::cli_health_delta_reset(rt, v)
    });
    // Wave I (Phase 5, 2026-04-24): grow-only audit trail query
    m.insert("cli-health-delta-history", |rt, v| {
        crate::cli_handlers::cli_health_delta_history(rt, v)
    });
    // Wave A A.4 (2026-04-29): profile_query CLI exposure
    m.insert("cli-profile-status", |rt, v| {
        crate::cli_handlers::cli_profile_status(rt, v)
    });
    // P1.6 (2026-04-25): mpatch-fuzzy dry-run patch preview
    m.insert("cli-mpatch-preview", |rt, v| {
        crate::cli_handlers::cli_mpatch_preview(rt, v)
    });
    // L7-B Gamma: job registry handlers
    m.insert("cli-jobs-spawn", |rt, v| {
        crate::cli_handlers::cli_jobs_spawn(rt, v)
    });
    m.insert("cli-jobs-poll", |rt, v| {
        crate::cli_handlers::cli_jobs_poll(rt, v)
    });
    m.insert("cli-jobs-list", |rt, v| {
        crate::cli_handlers::cli_jobs_list(rt, v)
    });
    // L7-B Audit: cli-jobs-drop — completes spawn→poll→list→DROP lifecycle
    m.insert("cli-jobs-drop", |rt, v| {
        crate::cli_handlers::cli_jobs_drop(rt, v)
    });
    // B.1 (2026-04-29): SSR CLI handlers
    m.insert("cli-ssr-status", |rt, v| {
        crate::cli_handlers::cli_ssr_status(rt, v)
    });
    m.insert("cli-ssr-apply", |rt, v| {
        crate::cli_handlers::cli_ssr_apply(rt, v)
    });
    // L7-B Gamma: inferlet WASM runtime handlers
    m.insert("cli-inferlets-list", |rt, v| {
        crate::cli_handlers::cli_inferlets_list(rt, v)
    });
    // A.2.5 (2026-04-29): skip-region CLI handlers
    m.insert("cli-skip-list", |rt, v| {
        crate::cli_handlers::cli_skip_list(rt, v)
    });
    m.insert("cli-skip-validate", |rt, v| {
        crate::cli_handlers::cli_skip_validate(rt, v)
    });
    m.insert("cli-inferlets-run", |rt, v| {
        crate::cli_handlers::cli_inferlets_exec(rt, v)
    });
    // D27 (2026-05-03): plugin registry CLI handlers
    m.insert("cli-plugin-list", |rt, v| {
        crate::cli_handlers::cli_plugin_list(rt, v)
    });
    m.insert("cli-plugin-status", |rt, v| {
        crate::cli_handlers::cli_plugin_status(rt, v)
    });
    m.insert("cli-plugin-reload", |rt, v| {
        crate::cli_handlers::cli_plugin_reload(rt, v)
    });
    m.insert("cli-plugin-unregister", |rt, v| {
        crate::cli_handlers::cli_plugin_unregister(rt, v)
    });
    m.insert("cli-evolution-drift", |rt, v| {
        crate::cli_handlers::cli_evolution_drift(rt, v)
    });
    m.insert("cli-evolution-insights", |rt, v| {
        crate::cli_handlers::cli_evolution_insights(rt, v)
    });
    m.insert("cli-evolution-tools", |rt, v| {
        crate::cli_handlers::cli_evolution_tools(rt, v)
    });
    // D10-S1: touring-web health dashboard handlers
    m.insert("cli-doctor", |rt, v| crate::cli_handlers::cli_doctor(rt, v));
    m.insert("cli-status", |rt, v| crate::cli_handlers::cli_status(rt, v));
    // CLI session handlers
    m.insert("cli-session-start", |rt, v| {
        crate::cli_handlers_session::cli_session_start(rt, v)
    });
    m.insert("cli-session-checkpoint", |rt, v| {
        crate::cli_handlers_session::cli_session_checkpoint(rt, v)
    });
    m.insert("cli-session-list", |rt, v| {
        crate::cli_handlers_session::cli_session_list(rt, v)
    });
    m.insert("cli-session-assess", |rt, v| {
        crate::cli_handlers_session::cli_session_assess(rt, v)
    });
    // CLI suggest handlers
    m.insert("cli-suggest-next", |rt, v| {
        crate::cli_handlers::cli_suggest_next(rt, v)
    });
    m.insert("cli-suggest-skill", |rt, v| {
        crate::cli_handlers::cli_suggest_skill(rt, v)
    });
    // CLI decompose handlers
    m.insert("cli-decompose-create", |rt, v| {
        crate::cli_handlers::cli_decompose_create(rt, v)
    });
    m.insert("cli-decompose-add", |rt, v| {
        crate::cli_handlers::cli_decompose_add(rt, v)
    });
    m.insert("cli-decompose-get", |rt, v| {
        crate::cli_handlers::cli_decompose_get(rt, v)
    });
    m.insert("cli-decompose-update", |rt, v| {
        crate::cli_handlers::cli_decompose_update(rt, v)
    });
    m.insert("cli-decompose-validate", |rt, v| {
        crate::cli_handlers::cli_decompose_validate(rt, v)
    });
    m.insert("cli-decompose-status", |rt, v| {
        crate::cli_handlers::cli_decompose_status(rt, v)
    });
    m.insert("cli-decompose-event", |rt, v| {
        crate::cli_handlers::cli_decompose_event(rt, v)
    });
    m.insert("cli-decompose-finalize", |rt, v| {
        crate::cli_handlers::cli_decompose_finalize(rt, v)
    });
    m.insert("cli-decompose-ready", |rt, v| {
        crate::cli_handlers::cli_decompose_ready(rt, v)
    });
    m.insert("cli-tasksfile-validate", |rt, v| {
        crate::cli_handlers_decompose::cli_tasksfile_validate(rt, v)
    });
    m.insert("cli-tasksfile-export", |rt, v| {
        crate::cli_handlers_decompose::cli_tasksfile_export(rt, v)
    });
    m.insert("cli-devrcfile-import", |rt, v| {
        crate::cli_handlers_decompose::cli_devrcfile_import(rt, v)
    });
    m.insert("cli-devrcfile-export", |rt, v| {
        crate::cli_handlers_decompose::cli_devrcfile_export(rt, v)
    });
    // D31: touring definitions CLI (classify / node-types / semantic-search)
    m.insert("cli-definitions-classify", |rt, v| {
        crate::cli_handlers_decompose::cli_definitions_classify(rt, v)
    });
    m.insert("cli-definitions-nodetypes", |rt, v| {
        crate::cli_handlers_decompose::cli_definitions_nodetypes(rt, v)
    });
    m.insert("cli-definitions-semantic-search", |rt, v| {
        crate::cli_handlers_decompose::cli_definitions_semantic_search(rt, v)
    });
    // Feature B+C: workflow run/stats/slowest/compare
    // (definitions live in cli_handlers_decompose, not cli_handlers)
    m.insert("cli-workflow-run", |rt, v| {
        crate::cli_handlers_decompose::cli_workflow_run(rt, v)
    });
    m.insert("cli-workflow-stats", |rt, v| {
        crate::cli_handlers_decompose::cli_workflow_stats(rt, v)
    });
    m.insert("cli-workflow-slowest", |rt, v| {
        crate::cli_handlers_decompose::cli_workflow_slowest(rt, v)
    });
    m.insert("cli-workflow-compare", |rt, v| {
        crate::cli_handlers_decompose::cli_workflow_compare(rt, v)
    });
    m.insert("cli-workflow-resume", |rt, v| {
        crate::cli_handlers_decompose::cli_workflow_resume(rt, v)
    });
    m.insert("cli-workflow-status", |rt, v| {
        crate::cli_handlers_decompose::cli_workflow_status(rt, v)
    });
    // CLI mcts handlers
    m.insert("cli-mcts-search", |rt, v| {
        crate::cli_handlers::cli_mcts_search(rt, v)
    });
    // CLI shadow handlers
    m.insert("cli-shadow-validate", |rt, v| {
        crate::cli_handlers::cli_shadow_validate(rt, v)
    });
    // CLI index handlers (extracted to cli_handlers_index.rs)
    m.insert("cli-index-status", |rt, v| {
        crate::cli_handlers_index::cli_index_status(rt, v)
    });
    m.insert("cli-index-search", |rt, v| {
        crate::cli_handlers_index::cli_index_search(rt, v)
    });
    m.insert("cli-index-find", |rt, v| {
        crate::cli_handlers_index::cli_index_find(rt, v)
    });
    m.insert("cli-index-files", |rt, v| {
        crate::cli_handlers_index::cli_index_files(rt, v)
    });
    m.insert("cli-index-rebuild", |rt, v| {
        crate::cli_handlers_index::cli_index_rebuild(rt, v)
    });
    m.insert("cli-index-ingest", |rt, v| {
        crate::cli_handlers_index::cli_index_ingest(rt, v)
    });
    // CLI ast handlers (3 existing + 8 new = 11 total, in cli_handlers_index.rs)
    m.insert("cli-ast-find", |rt, v| {
        crate::cli_handlers_index::cli_ast_find(rt, v)
    });
    m.insert("cli-ast-overview", |rt, v| {
        crate::cli_handlers_index::cli_ast_overview(rt, v)
    });
    m.insert("cli-ast-blast", |rt, v| {
        crate::cli_handlers_index::cli_ast_blast(rt, v)
    });
    m.insert("cli-ast-semantic", |rt, v| {
        crate::cli_handlers_index::cli_ast_semantic(rt, v)
    });
    m.insert("cli-ast-quality", |rt, v| {
        crate::cli_handlers_index::cli_ast_quality(rt, v)
    });
    m.insert("cli-ast-calls", |rt, v| {
        crate::cli_handlers_index::cli_ast_calls(rt, v)
    });
    m.insert("cli-ast-heat", |rt, v| {
        crate::cli_handlers_index::cli_ast_heat(rt, v)
    });
    m.insert("cli-ast-modules", |rt, v| {
        crate::cli_handlers_index::cli_ast_modules(rt, v)
    });
    m.insert("cli-ast-scope", |rt, v| {
        crate::cli_handlers_index::cli_ast_scope(rt, v)
    });
    m.insert("cli-ast-detail", |rt, v| {
        crate::cli_handlers_index::cli_ast_detail(rt, v)
    });
    m.insert("cli-ast-imports", |rt, v| {
        crate::cli_handlers_index::cli_ast_imports(rt, v)
    });
    // Polyglot ast-grep handler (JS/TS/Python/Go/etc structural search + rewrite)
    m.insert("cli-ast-grep", |rt, v| {
        crate::cli::polyglot::cli_ast_grep(rt, v)
    });
    // Wave Q2: batch structural scan with YAML rules
    m.insert("cli-ast-scan", |rt, v| {
        crate::cli::polyglot::cli_ast_scan(rt, v)
    });
    // D.2: resolve-def / find-references / rename
    m.insert("cli-resolve-def", |rt, v| {
        crate::cli_handlers_semantics::cli_resolve_def(rt, v)
    });
    m.insert("cli-find-references", |rt, v| {
        crate::cli_handlers_semantics::cli_find_references(rt, v)
    });
    m.insert("cli-rename", |rt, v| {
        crate::cli_handlers_semantics::cli_rename(rt, v)
    });
    // Native Rust prompt enhancement — replaces prompt_enhancer.py
    m.insert("cli-prompt-enhance", |rt, v| {
        crate::cli_handlers::cli_prompt_enhance(rt, v)
    });
    // CLI e2e handler
    m.insert("cli-e2e", |rt, v| crate::cli_e2e::cli_e2e(rt, v));
    // CLI pre-task-scout handler (LRU-cached PreToolUse scouting)
    m.insert("cli-pre-task-scout", |rt, v| {
        crate::cli_handlers::cli_pre_task_scout(rt, v)
    });
    // Entity disambiguation handlers
    m.insert("cli-entity-resolve", |rt, v| {
        crate::cli_handlers_entity::cli_entity_resolve(rt, v)
    });
    m.insert("cli-entity-list", |rt, v| {
        crate::cli_handlers_entity::cli_entity_list(rt, v)
    });
    m.insert("cli-entity-generic-patterns", |rt, v| {
        crate::cli_handlers_entity::cli_entity_generic_patterns(rt, v)
    });
    // P7 Pln2: new AST/wiring/search/session/bench handlers
    m.insert("cli-ast-callgraph", crate::cli_handlers::cli_ast_callgraph);
    m.insert("cli-ast-todos", crate::cli_handlers::cli_ast_todos);
    m.insert("cli-ast-rationale", crate::cli_handlers::cli_ast_rationale);
    m.insert("cli-ast-features", crate::cli_handlers::cli_ast_features);
    m.insert("cli-ast-meta", crate::cli_handlers::cli_ast_meta);
    m.insert("cli-ast-skeleton", crate::cli_handlers::cli_ast_skeleton);
    m.insert("cli-ast-tdg", crate::cli_handlers::cli_ast_tdg);
    // Wave Q3: gotcha YAML sync + bootstrap
    m.insert("cli-gotcha-sync", crate::cli_handlers::cli_gotcha_sync);
    m.insert("cli-gotcha-init", crate::cli_handlers::cli_gotcha_init);
    // Wave R1: 11-category 0-269 executive KPI dashboard
    m.insert("cli-repo-score", crate::cli::repo_score::cli_repo_score);
    // Wave R2: Falsifiable Commitments Dashboard (commitments.yaml)
    m.insert("cli-kpi", crate::cli::kpi::cli_kpi);
    // Wave R3: auto-generated executive Markdown report
    m.insert("cli-repo-health", crate::cli::repo_health::cli_repo_health);
    // Wave T1: cargo-mutants wrapper (mutation testing as TIER 4 quality gate)
    m.insert(
        "cli-mutation-test",
        crate::cli_handlers_mutation_test::cli_mutation_test,
    );
    m.insert(
        "cli-ast-blast-enriched",
        crate::cli_handlers::cli_ast_blast_enriched,
    );
    m.insert(
        "cli-wiring-purpose",
        crate::cli_handlers::cli_wiring_purpose,
    );
    m.insert(
        "cli-search-symbols",
        crate::cli_handlers::cli_search_symbols,
    );
    m.insert("cli-search-docs", crate::cli_handlers::cli_search_docs);
    m.insert("cli-graph-flow", crate::cli_handlers::cli_graph_flow);
    m.insert("cli-viz-workspace", crate::cli_handlers::cli_viz_workspace);
    m.insert("cli-viz-blast", crate::cli_handlers::cli_viz_blast);
    m.insert("cli-viz-wiring", crate::cli_handlers::cli_viz_wiring);
    m.insert("cli-viz-cycles", crate::cli_handlers::cli_viz_cycles);
    m.insert("cli-viz-orphans", crate::cli_handlers::cli_viz_orphans);
    m.insert("cli-viz-feature", crate::cli_handlers::cli_viz_feature);
    m.insert("cli-query-dsl", crate::cli_handlers::cli_query_dsl);
    m.insert(
        "cli-metadata-backfill",
        crate::cli_handlers::cli_metadata_backfill,
    );
    m.insert(
        "cli-session-summary",
        crate::cli_handlers::cli_session_summary,
    );
    m.insert("cli-bench-run", crate::cli_handlers::cli_bench_run);
    // Tantivy BM25 full-text search handlers
    m.insert(
        "cli-tantivy-search",
        crate::cli_handlers::cli_tantivy_search,
    );
    m.insert("cli-tantivy-fuzzy", crate::cli_handlers::cli_tantivy_fuzzy);
    m.insert("cli-tantivy-stats", crate::cli_handlers::cli_tantivy_stats);
    m.insert(
        "cli-tantivy-suggest",
        crate::cli_handlers::cli_tantivy_suggest,
    );
    m.insert(
        "cli-tantivy-reindex",
        crate::cli_handlers::cli_tantivy_reindex,
    );
    // Wiring community handler
    m.insert(
        "cli-wiring-community",
        crate::cli_handlers::cli_wiring_community,
    );
    // Wave cross-audit: functional chains, cross-feature blast radius, enriched file knowledge
    m.insert("cli-wiring-chains", crate::cli_handlers::cli_wiring_chains);
    m.insert(
        "cli-ast-blast-cross-feature",
        crate::cli_handlers::cli_ast_blast_cross_feature,
    );
    m.insert(
        "cli-file-knowledge-extended",
        crate::cli_handlers::cli_file_knowledge_extended,
    );
    m.insert(
        "cli-file-knowledge-stats",
        crate::cli_handlers_file_knowledge::cli_file_knowledge_stats,
    );
    m.insert(
        "cli-file-knowledge-populate",
        crate::cli_handlers_file_knowledge::cli_file_knowledge_populate,
    );
    m.insert(
        "cli-file-knowledge-audit",
        crate::cli_handlers_file_knowledge::cli_file_knowledge_audit,
    );
    m.insert(
        "cli-repair-wiring",
        crate::cli_handlers_wiring_repair::cli_repair_wiring,
    );
    // Pln3 bidirectional action suggestions (CLI: `touring suggest action|list|stats|gc|consumed`)
    m.insert(
        "cli-suggest-action",
        crate::cli_handlers::cli_suggest_action,
    );
    m.insert(
        "cli-suggestion-list",
        crate::cli_handlers::cli_suggestion_list_pending,
    );
    m.insert(
        "cli-suggestion-stats",
        crate::cli_handlers::cli_suggestion_stats,
    );
    m.insert("cli-suggestion-gc", crate::cli_handlers::cli_suggestions_gc);
    m.insert(
        "cli-suggestion-consumed",
        crate::cli_handlers::cli_suggestion_mark_consumed,
    );

    m
}

#[cfg(test)]
#[path = "hook_registry_tests.rs"]
mod tests;
