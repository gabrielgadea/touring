#!/usr/bin/env python3
"""Master Plan A-W2.P3 — deterministic per-domain extractor for cli_handlers.rs.

Moves a set of named `pub fn`/`fn` blocks (brace-balanced, doc-comments &
attributes included) from `cli_handlers.rs` into `cli/<domain>.rs`, then inserts
facade `pub use crate::cli::<domain>::cli_X;` lines so every existing reference
(`crate::cli_handlers::cli_X`, internal dispatch arms, hook_registry) keeps
resolving unchanged. Adds `pub mod <domain>;` to `cli/mod.rs`.

Atomic + per-domain reversible: backs up cli_handlers.rs + cli/mod.rs before
touching them. Usage:
    cli_domain_extract.py extract <domain>
    cli_domain_extract.py revert  <domain>
"""
import re
import sys
from pathlib import Path

ROOT = Path.home() / ".claude/rust/crates/touring-hooks/src"
CORE = ROOT / "cli_handlers.rs"
CLI_DIR = ROOT / "cli"
MODRS = CLI_DIR / "mod.rs"
BACKUP_DIR = Path.home() / ".claude/touring/cli-extract-backups"

# Per-domain config: ordered list of function names to move (handlers + exclusive
# private helpers), the module doc header, and the import preamble. Functions are
# emitted into cli/<domain>.rs in the order listed. Facade `pub use` lines are
# generated only for the `pub fn` entries.
DOMAINS = {
    "wiring": {
        "doc": "//! CLI wiring handlers (`cli_wiring_*`) — extracted from cli_handlers.rs (A-W2.P3).\n//!\n//! Dependency/orphan/cycle/impact/suggest analysis over the wiring graph.",
        "imports": (
            "use crate::cli_handlers::{WiringModuleStatus, WiringOrphan, WiringStatus};\n"
            "use crate::rfc100_emission::Rfc100Emitter;\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
            "use touring_analysis::e2e::schema_guard;\n"
        ),
        "fns": [
            "cli_wiring_status",
            "cli_wiring_orphans",
            "cli_wiring_modules",
            "cli_wiring_impact",
            "cli_wiring_cycles",
            "cli_wiring_suggest",
            "process_single_wiring_suggest",
            "cli_wiring_purpose",
            "cli_wiring_community",
            "cli_wiring_chains",
        ],
    },
    "viz": {
        "doc": "//! CLI viz handlers (`cli_viz_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::cli_handlers::{VizEdgeData, VizGraphData, VizNodeData};\n"
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_viz_workspace",
            "cli_viz_cycles",
            "cli_viz_orphans",
            "cli_viz_blast",
            "cli_viz_wiring",
            "cli_viz_feature",
            "build_wiring_graph_data",
        ],
    },
    "gotcha": {
        "doc": "//! CLI gotcha handlers (`cli_gotcha_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::cli_handlers::GotchaEntry;\n"
            "use crate::knowledge::Gotcha;\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
            "use touring_analysis::e2e::schema_guard;\n"
        ),
        "fns": [
            "cli_gotcha_list",
            "cli_gotcha_add",
            "cli_gotcha_match",
            "cli_gotcha_stats",
            "cli_gotcha_sync",
            "cli_gotcha_init",
        ],
    },
    "saga": {
        "doc": "//! CLI saga handlers (`cli_saga_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_saga_register",
            "cli_saga_prepare",
            "cli_saga_decide",
            "cli_saga_delta",
            "cli_saga_begin",
            "cli_saga_status",
            "cli_saga_abort",
        ],
    },
    "tantivy": {
        "doc": "//! CLI tantivy handlers (`cli_tantivy_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
            "#[cfg(feature = \"tantivy-fts\")]\n"
            "use crate::cli_handlers::normalize_to_relative;\n"
            "#[cfg(feature = \"tantivy-fts\")]\n"
            "use rusqlite::params;\n"
            "#[cfg(feature = \"tantivy-fts\")]\n"
            "use touring_analysis::e2e::schema_guard;\n"
        ),
        "fns": [
            "cli_tantivy_search",
            "cli_tantivy_fuzzy",
            "cli_tantivy_stats",
            "cli_tantivy_suggest",
            "cli_tantivy_reindex",
        ],
    },
    # ---- Leva 2 (A-W2.P3) ----
    "memory": {
        "doc": "//! CLI memory handlers (`cli_memory_*`) — extracted from cli_handlers.rs (A-W2.P3).\n//!\n//! Recall (RRF-fused federated), store, reindex, stats, list. Shared helpers\n//! (`semantic_or_hash_embedding`, `discover_canonical_dbs`,\n//! `memory_recall_sql_federated`, `touring_claude_dir`) stay in cli_handlers.rs\n//! and are referenced via `crate::cli_handlers::`.",
        "imports": (
            "use crate::cli_handlers::{\n"
            "    discover_canonical_dbs, memory_recall_sql_federated,\n"
            "    semantic_or_hash_embedding, semantic_text_embedding, touring_claude_dir,\n"
            "    GotchaStats, KnowledgeStats, ARCTIC_QUERY_PREFIX,\n"
            "};\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
            "use touring_analysis::e2e::schema_guard;\n"
        ),
        "fns": [
            "cli_memory_stats",
            "cli_memory_recall",
            "memory_recall_query_embedding",
            "memory_recall_rrf_merge_n",
            "memory_recall_tfidf",
            "cli_memory_store",
            "cli_memory_reindex",
            "cli_memory_list",
            "memory_list_order_clause",
            "parse_memory_row",
        ],
    },
    "workflow": {
        "doc": "//! CLI workflow handlers (`cli_workflow_*`) — extracted from cli_handlers.rs (A-W2.P3).\n//!\n//! Workflow-stage telemetry queries over the decompose tables. The shared\n//! `ensure_decompose_tables` helper stays in cli_handlers.rs.",
        "imports": (
            "use crate::cli_handlers::ensure_decompose_tables;\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
        ),
        "fns": [
            "cli_workflow_run",
            "cli_workflow_stats",
            "cli_workflow_slowest",
            "cli_workflow_compare",
        ],
    },
    "plugin": {
        "doc": "//! CLI plugin handlers (`cli_plugin_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_plugin_list",
            "cli_plugin_status",
            "cli_plugin_reload",
            "cli_plugin_unregister",
        ],
    },
    "jobs": {
        "doc": "//! CLI async-jobs handlers (`cli_jobs_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_jobs_spawn",
            "cli_jobs_poll",
            "cli_jobs_list",
            "cli_jobs_drop",
        ],
    },
    "learning": {
        "doc": "//! CLI learning handlers (`cli_learning_*`) — extracted from cli_handlers.rs (A-W2.P3).\n//!\n//! RL status snapshot + reward submission. `inject_synthetic_tool_rewards`\n//! (shared bootstrap helper) stays in cli_handlers.rs.",
        "imports": (
            "use crate::cli_handlers::{inject_synthetic_tool_rewards, LearningStatus};\n"
            "use crate::runtime::HookRuntime;\n"
            "use touring_learning::bandit::ContextualBandit;\n"
        ),
        "fns": [
            "cli_learning_status",
            "cli_learning_reward",
        ],
    },
    "inferlets": {
        "doc": "//! CLI inferlets handlers (`cli_inferlets_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_inferlets_list",
            "cli_inferlets_exec",
        ],
    },
    "cognitive": {
        "doc": "//! CLI cognitive handlers (`cli_cognitive_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_cognitive_metrics",
            "cli_cognitive_engines",
        ],
    },
    "gate": {
        "doc": "//! CLI gate-metrics handlers (`cli_gate_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_gate_metrics",
            "cli_gate_event",
        ],
    },
    "granularity": {
        "doc": "//! CLI granularity-bandit handlers (`cli_granularity_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_granularity_status",
            "cli_granularity_reset",
            "cli_granularity_hint",
        ],
    },
    "cascade": {
        "doc": "//! CLI cascade-queue handlers (`cli_cascade_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_cascade_queue_status",
            "cli_cascade_queue_drain",
        ],
    },
    "acp": {
        "doc": "//! CLI ACP-protocol handlers (`cli_acp_*`) — extracted from cli_handlers.rs (A-W2.P3).\n//!\n//! Feature-gated behind `acp-protocol`. Dispatches to wiring handlers, which\n//! live in `cli/wiring.rs` (re-exported from cli_handlers).",
        "imports": (
            "#[cfg(feature = \"acp-protocol\")]\n"
            "use crate::cli_handlers::{\n"
            "    cli_wiring_cycles, cli_wiring_impact, cli_wiring_modules, cli_wiring_orphans,\n"
            "    cli_wiring_status, cli_wiring_suggest,\n"
            "};\n"
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_acp_message",
            "cli_acp_discover",
        ],
    },
    "health": {
        "doc": "//! CLI health-delta handlers (`cli_health_delta_*`) — extracted from cli_handlers.rs (A-W2.P3).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_health_delta_status",
            "cli_health_delta_reset",
            "cli_health_delta_history",
        ],
    },
    # ---- Leva 3 (A-W2.P4) ----
    "ssr": {
        "doc": "//! CLI SSR (structural search & replace) handlers (`cli_ssr_*`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Wraps `touring_ast::ssr` prebuilt-rule introspection and rule application.",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_ssr_status",
            "cli_ssr_apply",
        ],
    },
    "skip": {
        "doc": "//! CLI skip-region handlers (`cli_skip_*`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Self-contained skip-region parser (mirrors `post_edit::parse_skip_regions`)\n//! to avoid a circular dependency on `touring-generator`. The `SkipRegionRaw`\n//! struct stays in cli_handlers.rs (promoted `pub(crate)`) and is imported.",
        "imports": (
            "use crate::cli_handlers::SkipRegionRaw;\n"
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_skip_list",
            "cli_skip_validate",
            "parse_skip_regions_raw",
        ],
    },
    "hook": {
        "doc": "//! CLI hook-memory handlers (`cli_hook_memory_*`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Store/recall over the `hook_events` table via `SqliteHookMemoryBridge`.",
        "imports": (
            "use crate::hook_memory::{\n"
            "    HookEvent, HookMemoryBridge, MemoryTier, SqliteHookMemoryBridge,\n"
            "};\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
        ),
        "fns": [
            "cli_hook_memory_store",
            "cli_hook_memory_recall",
        ],
    },
    "search": {
        "doc": "//! CLI search handlers (`cli_search_*`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! LIKE-based symbol + doc search over the wiring_map / file_knowledge tables.",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
            "use touring_analysis::e2e::schema_guard;\n"
        ),
        "fns": [
            "cli_search_symbols",
            "cli_search_docs",
        ],
    },
    "ast": {
        "doc": "//! CLI AST analysis handlers (`cli_ast_*`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Callgraph/todos/rationale/features/meta/skeleton/tdg/blast queries. Uses\n//! fully-qualified `crate::ast_bridge::*` and `crate::shared::query_cache::*`\n//! paths; the exclusive helpers `compute_churn_score` and\n//! `detect_language_from_ext` move alongside the handlers. The shared\n//! `normalize_to_relative` helper stays in cli_handlers.rs and is imported.",
        "imports": (
            "use crate::cli_handlers::normalize_to_relative;\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
            "use touring_analysis::e2e::schema_guard;\n"
            "use touring_foundation::diagnostic::DiagnosticCode;\n"
        ),
        "fns": [
            "cli_ast_callgraph",
            "cli_ast_todos",
            "cli_ast_rationale",
            "cli_ast_features",
            "cli_ast_meta",
            "detect_language_from_ext",
            "compute_churn_score",
            "cli_ast_skeleton",
            "cli_ast_tdg",
            "cli_ast_blast_enriched",
            "cli_ast_blast_cross_feature",
        ],
    },
    "suggest": {
        "doc": "//! CLI suggestion handlers (`cli_suggest_*`, `cli_suggestion_*`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! LinUCB-bandit next-action/skill hints + suggestion-record lifecycle\n//! (mark consumed, stats, list pending, GC) over the decompose tables. The\n//! shared `ensure_decompose_tables` and `keyword_skill_match` helpers stay in\n//! cli_handlers.rs (the latter retains its in-place `#[cfg(test)]` coverage).",
        "imports": (
            "use crate::cli_handlers::{ensure_decompose_tables, keyword_skill_match};\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
        ),
        "fns": [
            "cli_suggest_next",
            "cli_suggest_skill",
            "cli_suggest_action",
            "cli_suggestion_mark_consumed",
            "cli_suggestion_stats",
            "cli_suggestion_list_pending",
            "cli_suggestions_gc",
        ],
    },
    "decompose": {
        "doc": "//! CLI decompose/DAG handlers (`cli_decompose_*`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Task/subtask CRUD + DAG validation (cycle detection) + status/finalize/ready\n//! over the decompose tables. `cli_decompose_finalize`/`cli_decompose_ready`\n//! are thin wrappers delegating to `crate::cli_handlers_decompose::*` (fully\n//! qualified). The shared `ensure_decompose_tables` helper stays in\n//! cli_handlers.rs.",
        "imports": (
            "use crate::cli_handlers::ensure_decompose_tables;\n"
            "use crate::runtime::HookRuntime;\n"
            "use crate::schemas::validate_payload;\n"
            "use rusqlite::params;\n"
        ),
        "fns": [
            "cli_decompose_create",
            "cli_decompose_mark_mirrored",
            "cli_decompose_add",
            "cli_decompose_get",
            "cli_decompose_update",
            "cli_decompose_validate",
            "cli_decompose_status",
            "cli_decompose_finalize",
            "cli_decompose_ready",
            "cli_decompose_event",
        ],
    },
    "query": {
        "doc": "//! CLI graph/query handlers (`cli_graph_flow`, `cli_query_dsl`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! BFS path enumeration over the wiring graph + a small DSL→SQL query parser\n//! over file_knowledge. Both use fully-qualified `crate::shared::*` and\n//! `rusqlite::params_from_iter` paths.",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
            "use touring_analysis::e2e::schema_guard;\n"
        ),
        "fns": [
            "cli_graph_flow",
            "cli_query_dsl",
        ],
    },
    "knowledge": {
        "doc": "//! CLI knowledge/metadata handlers (`cli_metadata_backfill`, `cli_session_summary`, `cli_bench_run`, `cli_file_knowledge_extended`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Metadata backfill, session summary, benchmark recording, and the extended\n//! 23-field file-knowledge view. Uses fully-qualified `crate::health_delta::*`\n//! and `crate::shared::*` paths; the shared `normalize_to_relative` helper and\n//! the one-shot `FK_EXTENDED_DDL_DONE` DDL guard stay in cli_handlers.rs and are\n//! imported.",
        "imports": (
            "use crate::cli_handlers::{normalize_to_relative, FK_EXTENDED_DDL_DONE};\n"
            "use crate::runtime::HookRuntime;\n"
            "use rusqlite::params;\n"
            "use touring_analysis::e2e::schema_guard;\n"
        ),
        "fns": [
            "cli_metadata_backfill",
            "cli_session_summary",
            "cli_bench_run",
            "cli_file_knowledge_extended",
        ],
    },
    "metrics": {
        "doc": "//! CLI runtime-metrics handlers (`cli_mcp_overhead`, `cli_tokio_metrics`, `cli_profile_status`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Lightweight introspection over MCP overhead, the Tokio runtime, and the\n//! profile aggregator. All dependencies are fully-qualified (`crate::mcp_overhead::*`,\n//! `crate::shared::gate_metrics::*`, `touring_foundation::profile::*`).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_mcp_overhead",
            "cli_tokio_metrics",
            "cli_profile_status",
        ],
    },
    "predict": {
        "doc": "//! CLI prediction/world-model handlers (`cli_predict_action`, `cli_world_model_status`, `cli_agentic_rl_status`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Action-outcome prediction (CEG X4), world-model snapshot, and agentic-RL\n//! status. All dependencies are fully-qualified (`crate::action_signature::*`,\n//! `crate::agentic_rl::*`, `crate::gateway::*`, `crate::shared::*`,\n//! `touring_foundation::*`).",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_predict_action",
            "cli_world_model_status",
            "cli_agentic_rl_status",
        ],
    },
    "contract": {
        "doc": "//! CLI change/harness-contract handlers (`cli_change_contract`, `cli_attest_contract`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! Change-contract recording + harness-contract attestation over the gateway\n//! contract types. Dependencies are fully-qualified (`crate::approval_store::*`,\n//! `crate::cli_suggester::*`, `crate::gateway::*`); the shared `touring_claude_dir`\n//! helper stays in cli_handlers.rs and is imported.",
        "imports": (
            "use crate::cli_handlers::touring_claude_dir;\n"
            "use crate::runtime::HookRuntime;\n"
        ),
        "fns": [
            "cli_change_contract",
            "cli_attest_contract",
        ],
    },
    "rlsearch": {
        "doc": "//! CLI RL search/validate handlers (`cli_mcts_search`, `cli_shadow_validate`) — extracted from cli_handlers.rs (A-W2.P4).\n//!\n//! MCTS reasoning search + shadow (speculative) validation. Uses\n//! `touring_ast::{speculate_v2, Lang}` for the shadow validator and\n//! fully-qualified `touring_cognitive::reasoning_engine::*` for MCTS.",
        "imports": (
            "use crate::runtime::HookRuntime;\n"
            "use touring_ast::{speculate_v2, Lang};\n"
        ),
        "fns": [
            "cli_mcts_search",
            "cli_shadow_validate",
        ],
    },
}


def find_fn_block(lines, name):
    """Return (start_idx, end_idx_exclusive, is_pub) for the top-level fn `name`.

    start_idx is the first line of the leading doc-comment/attribute run that
    immediately precedes the signature. Matches only column-0 `pub fn NAME(` /
    `fn NAME(` / `pub fn NAME<` etc. — never call-sites inside a match.
    """
    sig_re = re.compile(r"^(pub\s+)?fn\s+" + re.escape(name) + r"\s*[<(]")
    sig_idx = None
    for i, ln in enumerate(lines):
        if sig_re.match(ln):
            sig_idx = i
            break
    if sig_idx is None:
        return None
    is_pub = lines[sig_idx].startswith("pub ")
    # Walk backwards over contiguous doc-comments (///, //!) and attributes (#[...]).
    start = sig_idx
    j = sig_idx - 1
    while j >= 0:
        s = lines[j].lstrip()
        if s.startswith("///") or s.startswith("//!") or s.startswith("#["):
            start = j
            j -= 1
        else:
            break
    # Brace-balance from the signature to find the closing line.
    depth = 0
    seen_open = False
    end = sig_idx
    for k in range(sig_idx, len(lines)):
        depth += lines[k].count("{") - lines[k].count("}")
        if "{" in lines[k]:
            seen_open = True
        if seen_open and depth == 0:
            end = k
            break
    return (start, end + 1, is_pub)


def backup(domain):
    BACKUP_DIR.mkdir(parents=True, exist_ok=True)
    (BACKUP_DIR / f"cli_handlers.{domain}.bak").write_text(CORE.read_text())
    (BACKUP_DIR / f"mod.{domain}.bak").write_text(MODRS.read_text())


def revert(domain):
    cb = BACKUP_DIR / f"cli_handlers.{domain}.bak"
    mb = BACKUP_DIR / f"mod.{domain}.bak"
    if not cb.exists():
        print(f"no backup for {domain}", file=sys.stderr)
        sys.exit(1)
    CORE.write_text(cb.read_text())
    MODRS.write_text(mb.read_text())
    tgt = CLI_DIR / f"{domain}.rs"
    if tgt.exists():
        tgt.unlink()
    print(f"reverted {domain}")


def extract(domain):
    cfg = DOMAINS[domain]
    backup(domain)
    text = CORE.read_text()
    lines = text.split("\n")  # no keepends; rejoin with \n

    # Locate every block; collect (start, end, name, is_pub).
    blocks = []
    for name in cfg["fns"]:
        res = find_fn_block(lines, name)
        if res is None:
            print(f"ERROR: fn {name} not found", file=sys.stderr)
            sys.exit(2)
        s, e, is_pub = res
        blocks.append((s, e, name, is_pub))

    # Sanity: no overlaps.
    by_start = sorted(blocks, key=lambda b: b[0])
    for a, b in zip(by_start, by_start[1:]):
        if a[1] > b[0]:
            print(f"ERROR: overlap {a[2]} and {b[2]}", file=sys.stderr)
            sys.exit(3)

    # Build the moved-body text in the configured order.
    moved_parts = []
    for name in cfg["fns"]:
        s, e, _, _ = next(b for b in blocks if b[2] == name)
        moved_parts.append("\n".join(lines[s:e]).rstrip("\n"))
    moved_body = "\n".join(moved_parts)

    # Remove blocks from core (delete from highest start to lowest).
    to_remove = sorted(blocks, key=lambda b: b[0], reverse=True)
    for s, e, _, _ in to_remove:
        del lines[s:e]

    # Insert facade `pub use` lines after the last existing `pub use` line.
    pub_use_idxs = [i for i, ln in enumerate(lines) if ln.startswith("pub use crate::")]
    insert_at = (max(pub_use_idxs) + 1) if pub_use_idxs else 0
    facade = [
        f"pub use crate::cli::{domain}::{name};"
        for (_, _, name, is_pub) in by_start
        if is_pub
    ]
    lines[insert_at:insert_at] = facade

    CORE.write_text("\n".join(lines))

    # Write cli/<domain>.rs.
    out = cfg["doc"] + "\n\n" + cfg["imports"] + "\n" + moved_body + "\n"
    (CLI_DIR / f"{domain}.rs").write_text(out)

    # Add `pub mod <domain>;` to cli/mod.rs if absent.
    mod_text = MODRS.read_text()
    decl = f"pub mod {domain};"
    if decl not in mod_text:
        MODRS.write_text(mod_text.rstrip("\n") + "\n" + decl + "\n")

    n_pub = sum(1 for b in by_start if b[3])
    print(f"extracted {domain}: {len(blocks)} blocks ({n_pub} pub fn), facade lines added")


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in ("extract", "revert"):
        print("usage: cli_domain_extract.py extract|revert <domain>", file=sys.stderr)
        sys.exit(1)
    cmd, dom = sys.argv[1], sys.argv[2]
    if dom not in DOMAINS:
        print(f"unknown domain {dom}", file=sys.stderr)
        sys.exit(1)
    if cmd == "extract":
        extract(dom)
    else:
        revert(dom)
