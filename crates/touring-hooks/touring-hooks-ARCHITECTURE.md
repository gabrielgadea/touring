# touring-hooks — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 152371 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

Hook runtime and plugin system for the Touring ecosystem — orchestrates 200+ lifecycle hooks across tool use, session, task, decomposition, RL, and neural events. Acts as the central nervous system wiring Claude Code to Touring's memory, index, and learning subsystems.

## Key Types

`HookRuntime` | `HookRegistry` | `HookResultCache` | `HookResult` | `HookContext` | `TouringError`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | ~700 | HookRuntime entry, 100+ module re-exports, feature-gated hook registration |
| `src/lifecycle.rs` | 19251 | Lifecycle event dispatch — SessionStart/Stop, PreToolUse, PostToolUse, Hook*, Task*, Decompose*, RL*, Neural* |
| `src/cli_handlers.rs` | 7347 | CLI handlers for hook-related commands, hook invoke, hook list |
| `src/knowledge.rs` | 4180 | Knowledge graph integration, co-edit pairs, relation tracking |
| `src/pre_read.rs` | 3797 | Pre-read enrichment hook — symbol injection, path enrichment |
| `src/hook_runtime.rs` | 2920 | Core HookRuntime implementation — dispatch, result caching, error handling |
| `src/shared/gate_metrics.rs` | 2809 | Gate metrics snapshot collection and export |
| `src/post_edit.rs` | 2794 | Post-edit quality tracking, cognitive score delta computation |
| `src/cli_handlers_decompose.rs` | 2657 | Decompose CLI handlers — add, update, status, finalize |
| `src/tantivy_index.rs` | 2556 | Tantivy index integration for hook event storage |
| `src/wiring.rs` | 2534 | Wiring graph management, orphan detection, consumer tracking |
| `src/pre_write.rs` | ~2400 | Pre-write speculative validation hook |
| `src/post_write.rs` | ~2100 | Post-write hook for memory persistence |
| `src/pre_bash.rs` | ~1950 | Bash command validation hook — structural safety checks |
| `src/post_bash.rs` | ~1850 | Post-bash hook for output tracking |
| `src/pre_grep.rs` | ~1700 | Pre-grep symbol enrichment — PascalCase/snake_case injection |
| `src/post_tool_rl.rs` | ~1650 | RL reward injection from tool outcomes |
| `src/task_list.rs` | ~1600 | Task list management and persistence |
| `src/incremental_pipeline.rs` | ~1550 | Incremental index pipeline for hook events |
| `src/cli_suggester.rs` | ~1500 | CLI suggester — decision matrix hints per tool |
| `src/pre_task_scout.rs` | ~1450 | Pre-task scout enrichment |
| `src/task_observer.rs` | ~1350 | Task completion tracking and scoring |
| `src/shared/moka_policies.rs` | ~1300 | Moka cache policies shared across hook runtime |
| `src/post_compact.rs` | ~1250 | Post-compact hook for state compaction |
| `src/speculate.rs` | ~1200 | Speculative validation for high-risk operations |
| `src/gotcha_db.rs` | ~1150 | Gotcha pitfall database integration |
| `src/session_guide.rs` | ~1100 | Session guide generation and injection |
| `src/throttle.rs` | ~1050 | Adaptive throttling based on memory pressure |
| `src/ctx_facets.rs` | ~1000 | Facet collector for Tantivy queries |
| `src/ctx_aggregate.rs` | ~950 | Aggregate terms for analytics |
| `src/ctx_cleanup.rs` | ~900 | TTL-based cleanup of old tool outputs |
| `src/ctx_retrieve_with_query.rs` | ~850 | Query-based retrieval with snippet generation |
| `src/diagnostic_b302.rs` | ~820 | B-302 patch expansion diagnostic emission |
| `src/diagnostic_q220.rs` | ~800 | Q-220 non-idempotency diagnostic |
| `src/diagnostic_w115.rs` | ~780 | W-115 skip region diagnostic |
| `src/diagnostic_tdg.rs` | ~750 | TDG grade letter diagnostic emission |
| `src/shared/memory_stats_probe.rs` | ~720 | RSS/virt memory stats collection |
| `src/shared/domain_circuit.rs` | ~700 | Domain circuit for cross-subsystem wiring |
| `src/plan_mode.rs` | ~680 | Plan mode hook handlers |
| `src/skip_context.rs` | ~650 | Skip context region markers (W-115) |
| `src/ast_grep_patterns.rs` | ~620 | AstGrep risk signal patterns for pre-read |
| `src/pre_tool_use_lifecycle.rs` | ~600 | Pre-tool-use lifecycle event classification |
| `src/batch_processor.rs` | ~580 | Batch processing for hook events |
| `src/cli_handlers_e2e.rs` | ~550 | End-to-end CLI handler tests |
| `src/shared.rs` | ~500 | Shared utilities for hook runtime |
| `src/error.rs` | ~450 | Error enum and conversion |
| `src/types.rs` | ~400 | Public types re-exported from submodules |
| `src/async_hooks.rs` | ~380 | Async hook support infrastructure |
| `src/pre_compact.rs` | ~360 | Pre-compact hook for state snapshot |
| `src/permissions.rs` | ~350 | Permission request handling |
| `src/lifecycle_events.rs` | ~340 | Lifecycle event type definitions |
| `src/post_tool_batch.rs` | ~330 | Batch post-tool hook processing |
| `src/plan_detector.rs` | ~320 | Plan document detection for routing |
| `src/hook_registry.rs` | ~300 | HookRegistry — 198 registered hooks |
| `src/cli_touring.rs` | ~280 | CLI touring commands |
| `src/cli_index.rs` | ~260 | CLI index commands |
| `src/cli_wiring.rs` | ~250 | CLI wiring commands |
| `src/cli_session.rs` | ~240 | CLI session commands |
| `src/cli_memory.rs` | ~230 | CLI memory commands |
| `src/cli_decompose.rs` | ~220 | CLI decompose commands |
| `src/cli_ast.rs` | ~210 | CLI AST commands |
| `src/cli_learning.rs` | ~200 | CLI learning commands |
| `src/mutation_test.rs` | ~190 | Mutation testing wrapper |
| `src/assists.rs` | ~180 | Assist handler registration |
| `src/mcp_tools.rs` | ~170 | MCP tool definitions |
| `src/inferlets/pools.rs` | ~165 | WASM inferlet pools |
| `src/inferlets/mod.rs` | ~160 | Inferlet module infrastructure |
| `src/bandit/granularity.rs` | ~155 | Granularity bandit for hook batching |
| `src/bandit/mod.rs` | ~150 | Bandit module |
| `src/shared/rules/types.rs` | ~145 | MetricRule, MetricRuleSet, MetricViolation types |
| `src/shared/rules/evaluator.rs` | ~140 | Rule evaluator |
| `src/shared/rules/mod.rs` | ~135 | Rules module |
| `src/aco/mod.rs` | ~130 | ACO module (ant colony optimization) |
| `src/aco/pheromone.rs` | ~125 | Pheromone tracking for ACO |
| `src/aco/esaa.rs` | ~120 | ESAA pattern implementation |
| `src/rkyv_ipc.rs` | ~115 | rkyv IPC for daemon communication |
| `src/touring_ast_integration.rs` | ~110 | touring-ast integration |
| `src/health_delta.rs` | ~105 | Health delta computation |
| `src/health.rs` | ~100 | Health module |
| `src/shared/checkpoint.rs` | ~95 | Checkpoint utilities |
| `src/shared/memory.rs` | ~90 | Memory utilities |
| `src/shared/config.rs` | ~85 | Configuration utilities |
| `src/shared/logging.rs` | ~80 | Logging utilities |
| `src/shared/test_helpers.rs` | ~75 | Test helpers |
| `src/shared/async_utils.rs` | ~70 | Async utilities |
| `src/shared/serde_ext.rs` | ~65 | Serde extensions |
| `src/shared/tokio_rt.rs` | ~60 | Tokio runtime utilities |
| `src/shared/future_ext.rs` | ~55 | Future extensions |
| `src/shared/vec_ext.rs` | ~50 | Vec extensions |
| `src/shared/string_ext.rs` | ~45 | String extensions |
| `src/shared/result_ext.rs` | ~40 | Result extensions |
| `src/shared/option_ext.rs` | ~35 | Option extensions |
| `src/shared/iter_ext.rs` | ~30 | Iterator extensions |
| `src/shared/path_ext.rs` | ~25 | Path utilities |
| `src/shared/time_ext.rs` | ~20 | Time utilities |
| `src/shared/duration_ext.rs` | ~15 | Duration extensions |
| `src/shared/format_ext.rs` | ~10 | Format extensions |

## Hook System Architecture

- **HookRuntime**: Central dispatcher — receives events from Claude Code via touring-hook, routes to registered handlers, caches results in HookResultCache (moka, 5-min TTL)
- **HookRegistry**: 198 registered hooks across 8 event categories (Tool, Session, Task, Decompose, RL, Neural, Lifecycle, CLI)
- **Feature-gated hooks**: Many hooks are conditional on `#[cfg(feature = "...")]` flags
- **Async support**: Full async hook execution via Tokio runtime
- **Memory pressure aware**: Throttling adapts to PSI-based memory pressure levels

## Integration Points

- Consumed by touring-core for health scoring
- Wired via `touring-hook` binary in `~/.claude/hooks/`
- Daemon health component: `knowledge_db`, `symbol_store`, `crdt_graph`, `predictor`, `cognitive_runtime`, `enrichment_pipeline`, `gotcha_db`
- Gate metrics: pre_edit_fast_path, rkyv_dispatch_count, tantivy_upsert_count, health_delta_*
- REGRA #0: All pub symbols must have consumers or be documented as intentional orphans

## Technology

Rust async/await via Tokio. Moka for cache. rkyv for IPC serialization. No unsafe at crate level.