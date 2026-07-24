//! Touring CLI command registry — the single source of truth for every subcommand.
//!
//! Split out of [`super::common`] (Thrust A cohesion): the ~850-line registry is
//! separated from the CLI primitives (flag parsing, output, daemon comms) it was
//! tangled with. Four builders mirror the four error-policy / concern groups that
//! already structured the table; [`command_table`] concatenates them in the
//! original display order, so `--help` output and name lookup are unchanged.

use super::common::{CommandDescriptor, ErrorPolicy, arg_or, flag_value, json_to_stdout};

/// Hook subcommands — exit 0 on error (hooks must never block user operations).
fn hook_commands() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "scan-pii",
            description: "PII detection (PreToolUse Write hook)",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::pii::run(),
        },
        CommandDescriptor {
            name: "prompt-enhance",
            description: "Prompt enhancement (UserPromptSubmit hook)",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| {
                let input = touring_hooks::HookRuntime::read_stdin().unwrap_or_default();
                let prompt = input.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                let output = touring_hooks::prompt_enhance::compose_json(prompt);
                match serde_json::to_string(&output) {
                    Ok(json) => {
                        json_to_stdout(&json);
                        Ok(())
                    }
                    Err(e) => Err(anyhow::anyhow!("prompt-enhance serialization failed: {e}")),
                }
            },
        },
        CommandDescriptor {
            name: "pre-read",
            description: "PreToolUse Read — inject file knowledge",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("pre-read"),
        },
        CommandDescriptor {
            name: "post-read",
            description: "PostToolUse Read — learn from file content",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("post-read"),
        },
        CommandDescriptor {
            name: "pre-bash",
            description: "PreToolUse Bash — recall past outcomes",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("pre-bash"),
        },
        CommandDescriptor {
            name: "post-bash",
            description: "PostToolUse Bash — capture command outcome",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("post-bash"),
        },
        CommandDescriptor {
            name: "pre-edit",
            description: "PreToolUse Edit — impact analysis",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("pre-edit"),
        },
        CommandDescriptor {
            name: "post-edit",
            description: "PostToolUse Edit — track changes",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("post-edit"),
        },
        CommandDescriptor {
            name: "pre-write",
            description: "PreToolUse Write — speculative validation",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("pre-write"),
        },
        CommandDescriptor {
            name: "post-write",
            description: "PostToolUse Write — quality verification",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("post-write"),
        },
        CommandDescriptor {
            name: "post-tool-rl",
            description: "PostToolUse RL — process reward signal",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("post-tool-rl"),
        },
        CommandDescriptor {
            name: "session-start",
            description: "SessionStart — load knowledge",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("session-start"),
        },
        CommandDescriptor {
            name: "session-stop",
            description: "Stop — persist session state",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| super::neural::run("session-stop"),
        },
        CommandDescriptor {
            name: "cortex",
            description: "Cortex consolidated hook engine",
            error_policy: ErrorPolicy::HookSilent,
            handler: |args| {
                let event = arg_or(args, 2, "pre-tool");
                crate::cortex::CortexRuntime::run(event).map_err(|e| anyhow::anyhow!("{e}"))
            },
        },
    ]
}

/// Standalone tool subcommands — exit 1 on error.
fn standalone_commands() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "classify-intent",
            description: "CILA intent classification",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |_args| super::classify::run(),
        },
        CommandDescriptor {
            name: "context",
            description: "Context compiler for subagent spawn",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::context::run(args),
        },
        CommandDescriptor {
            name: "run",
            description: "Execute code in the touring sandbox (code-mode without MCP): --lang + --code/--file/--stdin [--brief]",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::run::run(args),
        },
        // R3 — master CLI commands: thin wrappers over the Touring skill Layer-3
        // scripts (code-mode without MCP). See cli/master.rs.
        CommandDescriptor {
            name: "scout",
            description: "Symbol forensics — index/ast/wiring/memory/gotcha (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::scout(args),
        },
        CommandDescriptor {
            name: "read",
            description: "Read-comprehend — ast meta+blast+tdg+quality+semantic (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::read(args),
        },
        CommandDescriptor {
            name: "health",
            description: "Traffic-light health gate — doctor+status+drift, exit 0/1/2 (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::health(args),
        },
        CommandDescriptor {
            name: "guard",
            description: "Pre-edit GO/CAUTION/NO_GO gate — blast+tdg+gotcha+memory (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::guard(args),
        },
        CommandDescriptor {
            name: "map",
            description: "Workspace structure map — workspace-info+wiring chains (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::map(args),
        },
        CommandDescriptor {
            name: "blast",
            description: "Multi-file blast risk — blast+impact+cross-feature+cycles (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::blast(args),
        },
        // R5 — topic map (search + index + wiring + memory), also auto-invoked at SessionStart.
        CommandDescriptor {
            name: "investigate",
            description: "Topic map — search+index+wiring+memory for a symbol/feature (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::investigate(args),
        },
        // F1/ADW — loop-until-dry exploration with CCE v2 convergence contract.
        CommandDescriptor {
            name: "explore",
            description: "Loop-until-dry exploration — multi-lens ledger + CCE convergence contract (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::explore(args),
        },
        // F0/ADW — durable declarative agent workflows (spec + journal + resume).
        CommandDescriptor {
            name: "adw",
            description: "ADW runner — durable spec-driven workflows: lint/run/test/resume, Class-D verdicts (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::adw(args),
        },
        // F4/ADW — factory router: ticket → ADW, deterministic-first, RL feedback.
        CommandDescriptor {
            name: "factory",
            description: "Factory router — route/start tickets into library ADWs, RL-fed (Layer-3)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::master::factory(args),
        },
        // R3 — `audit`: CLI adapter over the run_audit engine (offensive CWE + 6 P0 quality),
        // mirroring the touring_audit MCP tool (engine once, two adapters). Exit code = verdict.
        CommandDescriptor {
            name: "audit",
            description: "Audit a file — offensive CWE/OWASP + 6 P0 quality gates (verdict→exit)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::audit::run(args),
        },
    ]
}

/// Configuration subcommands — exit 1 on error.
fn config_commands() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "init",
            description: "Initialize touring configuration with TOML profiles",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::init::run(args),
        },
        CommandDescriptor {
            name: "init-project",
            description: "W12.1 — Scaffold per-project .touring/{touring.toml,data,bin,hooks} (rustup-pattern)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::init_project::run(args),
        },
        CommandDescriptor {
            name: "toolchain",
            description: "W12.2 — Manage ~/.touring/ user-level toolchains (init|list|default|install|remove)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::toolchain::run(args),
        },
        CommandDescriptor {
            name: "update",
            description: "W12.3/F3 — Propagate a toolchain update to a project: re-link .touring/bin, record toolchain.lock (--rollback), restart per-project daemon",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::update::run(args),
        },
        CommandDescriptor {
            name: "component",
            description: "W12.3/F3 — Manage optional per-project components (list|add|remove) on top of the core toolchain binaries",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::component::run(args),
        },
        CommandDescriptor {
            name: "license",
            description: "F5/G3 — License & tier visibility (status; read-only, no gating)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::license::run(args),
        },
        CommandDescriptor {
            name: "migrate-from-global",
            description: "W12.7 — Copy ~/.claude/touring/ DBs to per-project .touring/data/ (with backup or --force)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::migrate_from_global::run(args),
        },
        CommandDescriptor {
            name: "overlay",
            description: "Workspace overlay: status, diff, sync",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::overlay::run(args),
        },
        CommandDescriptor {
            name: "conflict",
            description: "Merge conflict detection and resolution",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::conflict::run(args),
        },
        CommandDescriptor {
            name: "definitions",
            description: "Semantic definitions: classify, nodetypes, semantic-search",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::definitions::run(args),
        },
        CommandDescriptor {
            name: "cc-setup",
            description: "Claude Code MCP setup wizard",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |_args| super::init::run_cc_setup(),
        },
    ]
}

/// Daemon-powered subcommands — exit 1 on error.
fn daemon_commands() -> Vec<CommandDescriptor> {
    vec![
        CommandDescriptor {
            name: "assist",
            description: "Refactor assists: list-kinds, applicable, apply",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::assist::run(args),
        },
        CommandDescriptor {
            name: "activity",
            description: "Append-only event log: append | replay | verify | projection | status",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::activity::run(args),
        },
        CommandDescriptor {
            name: "ast",
            description: "AST symbol lookup, overview, blast radius",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::ast::run(args),
        },
        // C3 (coupling backlog) — intent-ranked tool discovery (progressive disclosure).
        CommandDescriptor {
            name: "search-tools",
            description: "Discover the right Touring tool/command for a natural-language intent",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::search_tools::run(args),
        },
        // C7 (coupling backlog) — RGAO task routing: 5-vector → CILA level + topology + phases.
        CommandDescriptor {
            name: "route",
            description: "Compute CILA level + topology + TACO phases from a task-scope vector (--depth/--files/--symbols/--cognitive/--coupling)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::route::run(args),
        },
        // C11 (coupling backlog) — budget conservation: Σ subtask budgets ≤ root, per dimension.
        CommandDescriptor {
            name: "budget-verify",
            description: "Verify decompose budget conservation Σ nodes ≤ root over ℕ⁶ (--root T,W,S,D,R,A --node ...)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::reason_tools::run_budget(args),
        },
        // C12 (coupling backlog) — MCTS tool-chain planning over a weighted tool graph.
        CommandDescriptor {
            name: "plan-chain",
            description: "Plan a low-cost tool chain via MCTS over a tool graph (--edge from,to,cost --start N --goal N)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::reason_tools::run_plan_chain(args),
        },
        // C14 (coupling backlog) — GED + cosine consistency gate for parallel-engineer merges.
        CommandDescriptor {
            name: "consistency",
            description: "Gate a parallel-engineer merge on GED + cosine distance (--a-nodes/--a-edges --b-nodes/--b-edges)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::reason_tools::run_consistency(args),
        },
        CommandDescriptor {
            name: "graph",
            description: "Graph topology: file deps, god-nodes, shortest-path, communities, flow, rename",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::graph::run(args),
        },
        // D.2 semantic primitives — resolve-def / find-references / rename
        CommandDescriptor {
            name: "resolve-def",
            description: "Resolve file:line:col to symbol definition",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::resolve_def::run(args),
        },
        CommandDescriptor {
            name: "find-references",
            description: "Find all references to symbol at file:line:col",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::find_references::run(args),
        },
        CommandDescriptor {
            name: "rename",
            description: "Rename symbol across all usages (dry-run or --apply)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::rename::run(args),
        },
        CommandDescriptor {
            name: "backup",
            description: "Backup a domain DB (knowledge|memory|graph|all) via VACUUM INTO",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::backup::run(args),
        },
        CommandDescriptor {
            name: "restore",
            description: "Restore a domain DB from a backup file",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::backup::run(args),
        },
        CommandDescriptor {
            name: "cognitive",
            description: "Cognitive engine: metrics, engines, search, explore",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::cognitive::run(args),
        },
        CommandDescriptor {
            name: "decompose",
            description: "DAG task decomposition management",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::decompose::run(args),
        },
        CommandDescriptor {
            name: "tasksfile",
            description: "Tasksfile YAML import, export, and validate",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::tasksfile::run(args),
        },
        CommandDescriptor {
            name: "devrcfile",
            description: "Devrcfile YAML import and export",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::devrcfile::run(args),
        },
        CommandDescriptor {
            name: "workflow",
            description: "Task execution visualization and analytics",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::workflow::run(args),
        },
        CommandDescriptor {
            name: "viz",
            description: "Graph visualization: workspace, blast, wiring, cycles, orphans, feature",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::viz::run(args),
        },
        CommandDescriptor {
            name: "repo-score",
            description: "Wave R1: 11-category 0-269 executive KPI dashboard with A+..F grade",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::repo_score::run(args),
        },
        CommandDescriptor {
            name: "kpi",
            description: "Wave R2: falsifiable commitments dashboard (--check, --snapshot)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::kpi::run(args),
        },
        CommandDescriptor {
            name: "repo-health",
            description: "Wave R3: auto-generated executive Markdown (--output <path>, --stdout)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::repo_health::run(args),
        },
        CommandDescriptor {
            name: "entity",
            description: "Entity Identity Registry — define, resolve, relate, list, delete",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::entity::run(args),
        },
        CommandDescriptor {
            name: "mutation-test",
            description: "Wave T1: cargo-mutants wrapper (--package, --threshold, --timeout, --jobs, --force, --cache-only)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::mutation_test::run(args),
        },
        CommandDescriptor {
            name: "diary",
            description: "Agent diary — write/read AAAK-format diary entries",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::diary::run(args),
        },
        CommandDescriptor {
            name: "source-change",
            description: "Transactional multi-file apply/preview (SourceChange)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::source_change::run(args),
        },
        CommandDescriptor {
            name: "generate",
            description: "Artifact generator: list-kinds, render, plan, verify",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::generate::run(args),
        },
        CommandDescriptor {
            name: "evolution",
            description: "Evolution drift, insights, tool stats",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::evolution::run(args),
        },
        CommandDescriptor {
            name: "flow",
            description: "Flow pipeline builder: list stages, run/validate YAML config",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::flow::run(args),
        },
        CommandDescriptor {
            name: "flywheel",
            description: "Flywheel component health",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::flywheel::run(args),
        },
        CommandDescriptor {
            name: "gotcha",
            description: "Pitfall database listing",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::gotcha::run(args),
        },
        CommandDescriptor {
            name: "incremental",
            description: "Parser cache status",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::incremental::run(args),
        },
        CommandDescriptor {
            name: "index",
            description: "Symbol index search, find, status, rebuild",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::index::run(args),
        },
        CommandDescriptor {
            name: "language",
            description: "Tier-based language support (list, <lang>)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::language::run(args),
        },
        CommandDescriptor {
            name: "learning",
            description: "RL engine status (LinUCB, QTable, bandit)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::learning::run(args),
        },
        CommandDescriptor {
            name: "memory",
            description: "Knowledge memory stats, recall, store, list",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::memory::run(args),
        },
        CommandDescriptor {
            name: "mcts",
            description: "Monte Carlo Tree Search decision engine",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::mcts::run(args),
        },
        CommandDescriptor {
            name: "profile",
            description: "Live heap profiling: `profile heap-dump [--output <path>]` | `profile flamegraph [--output <path>]`",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::profile::run(args),
        },
        CommandDescriptor {
            name: "projects",
            description: "Multi-project registry: list, add, remove, switch, info",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::projects::run(args),
        },
        CommandDescriptor {
            name: "session",
            description: "Session management (start, checkpoint, list, assess)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::session::run(args),
        },
        CommandDescriptor {
            name: "saga",
            description: "Distributed 2PC saga coordinator (register, begin, status, abort)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::saga::run(args),
        },
        CommandDescriptor {
            name: "search",
            description: "Unified search: unified, exact, fuzzy, bm25, index (semantic ANN indexing)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::search_unified::run(args),
        },
        CommandDescriptor {
            name: "find-code",
            description: "Unified code search via hybrid semantic + keyword fusion",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::find_code::run(args),
        },
        CommandDescriptor {
            name: "shadow",
            description: "Speculative validation in shadow branch",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::shadow::run(args),
        },
        CommandDescriptor {
            name: "skip",
            description: "Skip-region parser (list, validate)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::skip::run(args),
        },
        CommandDescriptor {
            name: "suggest",
            description: "RL-guided action and skill suggestions",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::suggest::run(args),
        },
        CommandDescriptor {
            name: "tantivy",
            description: "Tantivy BM25 full-text search (search, fuzzy, stats, suggest, reindex, gain, discover)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::tantivy::run(args),
        },
        // W7 (2026-06-11): shell completions + man page via clap_complete / clap_mangen.
        CommandDescriptor {
            name: "completions",
            description: "Generate shell completions (bash/zsh/fish/powershell/elvish) or a man page",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::completions::run(args),
        },
        // NEW-4 — TOML user filter DSL (Wave post-RTK 2026-05-08).
        CommandDescriptor {
            name: "filters",
            description: "User filter DSL (list, reload, validate <file>, path)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::filters::run(args),
        },
        CommandDescriptor {
            name: "ssr",
            description: "Structural Search & Replace (status, apply --pattern --replacement [--lang] [--stdin])",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::ssr::run(args),
        },
        CommandDescriptor {
            name: "wiring",
            description: "Wiring intelligence (orphans, modules, scores)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::wiring::run(args),
        },
        // L7-B Gamma: gate metrics, inferlets, jobs
        CommandDescriptor {
            name: "gate-metrics",
            description: "L7-B Gamma: enrichment gate observability (fast vs full counts)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::gate_metrics::run(args),
        },
        // R5/OP1: unified 6-dimension harness-quality KPI
        CommandDescriptor {
            name: "harness-metric",
            description: "R5/OP1: unified harness-quality metric (executable/inspectable/stateful/governed/performant/evolving + composite)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::harness_metric::run(args),
        },
        // S-09/R8: formal change-contract gating self-mutation
        CommandDescriptor {
            name: "change-contract",
            description: "S-09/R8: no-regression self-mutation gate (--pre/--post HarnessQuality JSON [--invariants])",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::change_contract::run(args),
        },
        // Wave 15 (2026-04-18): health_delta direct CLI exposure
        CommandDescriptor {
            name: "health-delta",
            description: "Wave 15: HealthDeltaCache + StreakCache inspection (status, reset)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::health_delta::run(args),
        },
        // D28: mcp-overhead self-report CLI
        CommandDescriptor {
            name: "mcp-overhead",
            description: "MCP overhead self-report (top-N costliest tools, json/table format)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::mcp_overhead::run(args),
        },
        // Wave C1.7 (2026-04-20): granularity bandit observability
        CommandDescriptor {
            name: "granularity",
            description: "Wave C1.7: task split-factor bandit (status, reset)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::granularity::run(args),
        },
        // Wave C D5 (2026-04-20): cascade queue CLI
        CommandDescriptor {
            name: "cascade",
            description: "Wave C D5: cascade queue (queue, drain)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::cascade::run(args),
        },
        CommandDescriptor {
            name: "inferlets",
            description: "L7-B Gamma: WASM inferlet pools (list, run)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::inferlets::run(args),
        },
        // D8.4 (2026-05-09): list installed inferlets from filesystem
        CommandDescriptor {
            name: "inferlet",
            description: "D8.4: list installed inferlets from ~/.claude/touring/inferlets/",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::handlers_inferlet::list(args),
        },
        CommandDescriptor {
            name: "plugin",
            description: "Plugin registry management (list, status, reload, unregister)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::plugin::run(args),
        },
        CommandDescriptor {
            name: "jobs",
            description: "L7-B Gamma: async Spawn & Poll background workers",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::jobs::run(args),
        },
        CommandDescriptor {
            name: "e2e",
            description: "Comprehensive E2E code analysis test (index+AST+wiring+quality+learning)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::e2e::run(args),
        },
        CommandDescriptor {
            name: "exec",
            description: "CEG gateway: gate a shell command through the X0..X9 pipeline",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run(args),
        },
        // S-12 (2026-05-29): speculative batch accept-prefix — gate N candidate
        // commands, accept the longest valid leading prefix (EAGLE analogue).
        CommandDescriptor {
            name: "exec-speculative",
            description: "CEG speculative: gate N candidate commands, accept the longest valid prefix",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_speculative(args),
        },
        // S-13 (2026-05-29): credibility-gated MCTS planning — a Deny candidate
        // (CEG credibility 0) is never selected over a credible one.
        CommandDescriptor {
            name: "plan-gated",
            description: "CEG gated planner: pick among N candidates via MCTS gated by CEG credibility",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_plan_gated(args),
        },
        // S-10 (2026-05-29): read/write-set parallelization safety — partition
        // actions into parallel waves via AccessDeclaration::conflicts_with.
        CommandDescriptor {
            name: "conflict-check",
            description: "CEG txn: declare write-sets, report which actions parallelize vs serialize",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_conflict_check(args),
        },
        // OP4 (§5.2.4): runtime dependency-aware locking — drives declarations
        // through a live TxnLockManager (acquire → Conflict(holder) → release →
        // serialized re-acquire), the transactional discipline the CRDT
        // eventual-convergence layer cannot provide alone.
        CommandDescriptor {
            name: "txn-acquire",
            description: "CEG txn: run r/w declarations through the live TxnLockManager (acquire/conflict/serialize)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_txn_acquire(args),
        },
        // OP7 Inspectable (§5.2.2): surface the non-terminal EvidenceBundle —
        // the five per-axis CEG sub-scores (static/vgp/predict/sandbox/gate)
        // behind a gate decision, instead of an opaque composite scalar.
        CommandDescriptor {
            name: "evidence",
            description: "CEG inspect: render the per-axis EvidenceBundle behind a command's gate decision (no execution)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_evidence(args),
        },
        // B-3 (§3.1.3): MCTS↔CEG verified-action-depth — draft an action chain
        // only as deep as the CEG verifies (a Deny truncates the draft-tree).
        CommandDescriptor {
            name: "plan-verified-depth",
            description: "CEG plan: MCTS drafts an action chain only to the CEG-verified depth (EAGLE draft-tree)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_verified_depth(args),
        },
        // B-5 (§3.2): distil historical bash-outcome substrate into an action
        // predictor — train_from_examples over the recent bash_outcomes, predict
        // the queried command's success probability + confidence.
        CommandDescriptor {
            name: "predict-action",
            description: "CEG learn: distil historical bash outcomes into an action predictor; predict a command's success",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_predict_action(args),
        },
        // S-08/A-A1: conformal calibration of the skill-selection gate (KnowNo).
        // Distils bash_outcomes into a split-conformal calibrator and reports the
        // data-derived firing threshold τ=1-q̂ (coverage ≥ 1-α), replacing the
        // hardcoded 0.7 cut. --command keys a durable-approval HITL override.
        CommandDescriptor {
            name: "calibrate-confidence",
            description: "Conformal calibration (KnowNo): data-derived skill-selection threshold τ=1-q̂ with a coverage guarantee + HITL defer",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_calibrate_confidence(args),
        },
        // Cold-start cluster (2026-05-30): opt-in cross-project RL warm-start.
        // Seeds this project's reward loop from another project's REAL bash_outcomes
        // (experience replay). No corpus → no-op (per-project isolation preserved).
        CommandDescriptor {
            name: "rl-warmstart",
            description: "Opt-in cross-project RL warm-start: replay REAL bash outcomes from a corpus DB into the reward loop (--corpus-db or TOURING_RL_WARMSTART_CORPUS)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_rl_warmstart(args),
        },
        // ES4 P1 (2026-05-30): durable action world-model liveness probe. The X4
        // PREDICT online model is now persisted + warm-loaded; this reports its
        // durable state (warm_loaded_entries > 0 ⇒ survived restart). --persist
        // forces an immediate atomic snapshot.
        CommandDescriptor {
            name: "world-model-status",
            description: "Durable action world model (ES4 P1): report persisted X4-PREDICT state; --persist forces a snapshot (survives daemon restart)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_world_model_status(args),
        },
        // ES2 P2 (2026-05-30): constitutional sink-token contract (EAGLE B-6).
        // Pins CLAUDE.md + rules/*.md by blake3 + per-claim structural attestation;
        // the live executable proof that flips the only result.cmd:null oracle row.
        CommandDescriptor {
            name: "attest-contract",
            description: "Attest the constitutional contract (EAGLE B-6): blake3 pin of CLAUDE.md + rules/*.md + per-claim structural verdicts; -j for JSON",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::exec::run_attest_contract(args),
        },
        // Wave 1 P2 (Sentrux master plan, 2026-05-09): workspace-level
        // quality signal aggregator — geometric mean over 5 root causes
        // (modularity, acyclicity, depth, equality, redundancy) on the
        // 0..=10000 Sentrux scale. Drives macro refactor direction via
        // the bottleneck field. Pure CLI, no daemon round-trip.
        CommandDescriptor {
            name: "quality-signal",
            description: "Workspace quality signal (Sentrux 0..=10000 + bottleneck root cause)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::quality_signal::run(args),
        },
        CommandDescriptor {
            name: "eval",
            description: "Reproducible benchmarks: token efficiency, blast radius, search quality, index performance",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::eval::run(args),
        },
        CommandDescriptor {
            name: "status",
            description: "Unified dashboard: daemon + index + wiring + sessions + composite_health_score",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::status::run(args),
        },
        // Wave 8 S6 (synergy maximization, 2026-04-26): cross-subsystem
        // wiring observability — lists active integrations + deferred
        // synergy opportunities. Subcommands: report (default), wired,
        // opportunities. Flag: -j / --json.
        CommandDescriptor {
            name: "synergy",
            description: "Cross-subsystem wiring report: active integrations + deferred opportunities",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::synergy::run(args),
        },
        CommandDescriptor {
            name: "doctor",
            description: "Diagnostics: socket, schema, circuit breaker, health",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::doctor::run(args),
        },
        // REGRA #19 (2026-05-23): canonical daemon lifecycle helper.
        // Replaces `pkill -f touring` anti-pattern — cmdline-aware singleton
        // detection protects MCP bridges + hook handlers of other CC sessions.
        CommandDescriptor {
            name: "daemon-ctl",
            description: "Canonical daemon lifecycle (status|restart|stop|reset) — REGRA #19, never pkill",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::daemon_ctl::run(args),
        },
        // D15 (Wave 2026-05-01): resource governor — unified context/token/memory management
        CommandDescriptor {
            name: "governor",
            description: "D15: ResourceGovernor — unified resource tracking (status, limits, reset, report)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::governor::run(args),
        },
        // D.3 (Wave 2026-04-28): diagnostics list + fix apply
        CommandDescriptor {
            name: "diagnostics",
            description: "D.3: list diagnostics for a file with optional fix candidates",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::diagnostics::run(args),
        },
        CommandDescriptor {
            name: "fix",
            description: "D.3: apply a named assist to fix a diagnostic (e.g. touring fix Q-201 foo.rs)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::fix::run(args),
        },
        // D.8 (Wave 2026-05-01): graph snapshot create/list/diff
        CommandDescriptor {
            name: "snapshot",
            description: "D.8: graph snapshot management (create <name>, list, diff <a> <b>)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::snapshot::run(args),
        },
        // D.9 (Wave 2026-05-01): clone detection via signature hashing
        CommandDescriptor {
            name: "clones",
            description: "D.9: detect duplicate/similar code via signature hashing (detect|list|stat|dot|mermaid)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::clones::run(args),
        },
        CommandDescriptor {
            name: "migrate",
            description: "DB consolidation migration (8 DBs → 3 domains)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::migrate::run(args),
        },
        // Pre-task-scout: LRU-cached PreToolUse scouting (exit 0 — never block tool execution)
        CommandDescriptor {
            name: "pre-task-scout",
            description: "PreToolUse scout with LRU cache (8s timeout, 1h TTL)",
            error_policy: ErrorPolicy::HookSilent,
            handler: |_args| {
                // Forward to daemon via daemon_query
                let input = super::read_stdin_safe();
                let output = super::daemon_query("cli-pre-task-scout", input)?;
                json_to_stdout(&output);
                Ok(())
            },
        },
        // P7 Pln2: search, query, metadata, session-summary, bench-run
        CommandDescriptor {
            name: "query",
            description: "Search symbols or docs: touring query symbols <query> | touring query docs <query>",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| {
                let sub = arg_or(args, 2, "symbols");
                let query = arg_or(args, 3, "");
                let top = flag_value(args, "--top").unwrap_or("10");
                let top_i: i64 = top.parse().unwrap_or(10);
                let (hook, payload) = match sub {
                    "docs" => (
                        "cli-search-docs",
                        serde_json::json!({"query": query, "top": top_i}),
                    ),
                    _ => (
                        "cli-search-symbols",
                        serde_json::json!({"query": query, "top": top_i}),
                    ),
                };
                let output = super::daemon_query(hook, payload)?;
                json_to_stdout(&output);
                Ok(())
            },
        },
        CommandDescriptor {
            name: "dsl-query",
            description: "DSL query against file knowledge: touring dsl-query 'lang = rust AND loc > 100'",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| {
                let dsl = arg_or(args, 2, "");
                let output =
                    super::daemon_query("cli-query-dsl", serde_json::json!({"query": dsl}))?;
                json_to_stdout(&output);
                Ok(())
            },
        },
        CommandDescriptor {
            name: "metadata-backfill",
            description: "Backfill file metadata into the knowledge index. Flags: --force (reindex existing), --parallel N",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| {
                let parallel: i64 = flag_value(args, "--parallel")
                    .unwrap_or("4")
                    .parse()
                    .unwrap_or(4);
                let force = args.iter().any(|a| a == "--force");
                let output = super::daemon_query(
                    "cli-metadata-backfill",
                    serde_json::json!({"parallel": parallel, "force": force}),
                )?;
                json_to_stdout(&output);
                Ok(())
            },
        },
        CommandDescriptor {
            name: "session-summary",
            description: "Query session file summaries: touring session-summary <file_path>",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| {
                let file_path = arg_or(args, 2, "");
                let output = super::daemon_query(
                    "cli-session-summary",
                    serde_json::json!({"file_path": file_path}),
                )?;
                json_to_stdout(&output);
                Ok(())
            },
        },
        CommandDescriptor {
            name: "file-knowledge",
            description: "FileKnowledgeEnriched: extended 23-field metadata for a file (base + cognitive + ecosystem + blake3 + coverage + community)",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| super::file_knowledge::run(args),
        },
        CommandDescriptor {
            name: "bench-run",
            description: "Store a benchmark run result: touring bench-run <name> --p50 <ms> --p95 <ms> --p99 <ms>",
            error_policy: ErrorPolicy::ExitOnError,
            handler: |args| {
                let bench_name = arg_or(args, 2, "");
                let p50: f64 = flag_value(args, "--p50")
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0.0);
                let p95: f64 = flag_value(args, "--p95")
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0.0);
                let p99: f64 = flag_value(args, "--p99")
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0.0);
                let samples: i64 = flag_value(args, "--samples")
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0);
                let output = super::daemon_query(
                    "cli-bench-run",
                    serde_json::json!({
                        "bench_name": bench_name,
                        "p50_ms": p50,
                        "p95_ms": p95,
                        "p99_ms": p99,
                        "samples": samples
                    }),
                )?;
                json_to_stdout(&output);
                Ok(())
            },
        },
    ]
}

/// Build the complete command table — the single source of truth for all touring
/// CLI commands. Adding a command = one entry in the matching builder above
/// (hook / standalone / config / daemon) + a handler function.
pub fn command_table() -> Vec<CommandDescriptor> {
    let mut table = hook_commands();
    table.extend(standalone_commands());
    table.extend(config_commands());
    table.extend(daemon_commands());
    table
}
