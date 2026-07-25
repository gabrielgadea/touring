# MCP Tools Reference

> **Version**: v30.3.0 (Wave W4 — task_1780763041476850005, 2026-06-06)
> **Active curated set**: 22 tools (19 kept from 102 historical + 3 new) under `--features mcp-curated`
> **Legacy set**: 102 historical tools under `--features mcp-legacy` (default until 2026-07-06, then flipped)
> See `rust/docs/2026-06-06-mcp-curated-migration.md` for the full migration guide.

## 🎯 Curated Set (22 tools — the strategic target)

The new strategic surface. All 22 ship with `--features mcp-curated`. Below is the canonical reference; build commands at the bottom.

### A. CORE WORKFLOW (5)

| Tool | Params | Description |
|------|--------|-------------|
| `touring_ast_meta` | `path` | File metadata (LOC, blast_radius, quality, cognitive, fan_in/out) |
| `touring_ast_find` | `symbol_name` | VGP symbol lookup |
| `touring_tantivy_search` | `query` | BM25 full-text search over symbol index |
| `touring_memory_recall` | `query, top_k?` | Knowledge graph FTS5 + cosine |
| `touring_wiring_audit` | `—` | Pub-symbol tracking (REGRA #0) |

### B. WRITES (3)

| Tool | Params | Description |
|------|--------|-------------|
| `touring_memory_store` | `key, value, tier?, type?` | Persist lesson/pattern |
| `touring_source_change` | `file_path, content, dry_run?` | Transactional multi-file apply |
| `touring_decompose` | `action, ...` | DAG task management |

### C. DIAGNOSTICS (4 — including new FamilyRouter)

| Tool | Params | Description |
|------|--------|-------------|
| `touring_status` | `family?` (composite\|integration.*\|evolution\|generator\|index) | **NEW W1.2** — FamilyRouter consolidating 9 historical `*_status` tools |
| `touring_gate_metrics` | `—` | Counter snapshot (rl, ceg, enrichment, hook_dispatch_latency) |
| `touring_evolution_drift` | `severity?` | Evolution drift detection |
| `touring_quality_signal_compute` | `path?` | Workspace quality signal (Sentrux 0..=10000) |

### D. CONTEXT EFFICIENCY (2)

| Tool | Params | Description |
|------|--------|-------------|
| `touring_ctx_smart` | `path` | 2-line file summary (<50ms) |
| `touring_ctx_explain` | `counter` | Counter explainer |

### E. WORKFLOW PRIMITIVES (3)

| Tool | Params | Description |
|------|--------|-------------|
| `touring_session` | `action, id?, ...` | Lifecycle: start/checkpoint/assess |
| `touring_checkpoint` | `action, file_path?` | Durable state file |
| `touring_minimal_context` | `detail_level?` | Compact tool listing |

### F. ADVANCED (2)

| Tool | Params | Description |
|------|--------|-------------|
| `touring_generator_validate_plan` | `plan_json` | Plan schema validation |
| `touring_generator_speculate_plan` | `plan_json` | Shadow validation pre-commit |

### G. NEW (3 — W2)

| Tool | Params | Description |
|------|--------|-------------|
| `touring_tdg` | `path, minimal?` | **NEW W2.1** — TDG grade letter A+..F wrapper |
| `touring_hook_metrics` | `subsystem?` | **NEW W2.2** — hook p50/p99 latency aggregator |
| `touring_cortex_classify` | `text` | **NEW W2.3** — unified intent classification (CILA L0-L6) |

## 📜 Legacy Reference (102 tools — for migration only)

The historical 102-tool surface remains accessible via `--features mcp-legacy`
(default until 2026-07-06). This reference documents the legacy for users
who need it during the 30-day deprecation window. After 2026-08-05, the
legacy feature is removed entirely.

> **Note**: The original 209-line reference below describes the historical 102
> tools. For the new strategic surface, see the **Curated Set** section above.
> To avoid this bloat, consult `touring --help` (114 CLI subs) or
> `touring wire --mcp-list` (planned W5) for the live inventory.

### Historical note (v4.27.0)

Originally this document described all 96-102 Touring MCP tools grouped by
function. After W4 (task_1780763041476850005) the strategic surface is 22
curated tools; the 102 legacy tools remain for 30d with `#[deprecated]`
hints and CLI fallbacks. See migration guide for replacement paths.

## Discovery (AST/Index)

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_ast_find` | `symbol_name` | Find symbols by name across project files |
| `touring_ast_overview` | `file_path` | Extract symbols from source code with TOON format |
| `touring_ast_edit` | `file_path, symbol_name, new_body` | AST surgery: replace symbol body |
| `touring_index_status` | — | Get project index status and configuration |

## File Metadata

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_file_metadata` | `file_path` | Get LOC, language, quality, cognitive, blast_radius |

## Memory & Knowledge

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_memory_store` | `key, value, tier?, type?` | Store memory entry in RLM + SemanticRecall |
| `touring_memory_recall` | `query` | Search RLM + SemanticRecall (FTS5 + cosine similarity) |
| `touring_memory_clusters` | `action` | Pattern clustering via HNSW-based lazy clustering |
| `touring_checkpoint` | `action, file_path?` | Create checkpoint file |

## Learning & Evolution

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_learn_pattern` | `action` | TD(lambda) Q-learning: update Q-value or query best action |
| `touring_online_learn` | — | Show real-time RL engine status |
| `touring_evolve` | `action` | Self-improvement engine: extract patterns, drift report, RL rewards |
| `touring_evolution_drift` | — | Query drift detection results |
| `touring_evolution_status` | — | Get status of self-evolution engine |

## Planning & Decomposition

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_decompose` | `action` | Task decomposition: create/delete plans, add subtasks with DAG |
| `touring_mcts_search` | `root_state?` | Monte Carlo Tree Search for multi-step planning |
| `touring_suggest` | `action` | RL-guided next action and skill suggestions |

## Wiring & Quality

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_wiring` | `action` | Wiring intelligence: orphan detection, integration scores |
| `touring_wiring_audit` | — | Complete audit: orphans + low-score modules |
| `touring_gotcha` | `action` | Pitfall database: list/add/match known issues |
| `touring_insights` | — | Query accumulated operational insights |

## Search (Tantivy BM25)

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_tantivy_search` | `query, top_k?` | BM25 ranked full-text search over symbols |
| `touring_tantivy_fuzzy` | `query, distance?, top_k?` | Fuzzy search with edit distance tolerance |
| `touring_tantivy_stats` | — | Index health metrics |
| `touring_tantivy_suggest` | `prefix, top_k?` | Prefix-based autocomplete |
| `touring_tantivy_reindex` | — | Rebuild Tantivy index |

## SCIP Export

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_scip_emit` | `file_path` | Emit SCIP-compatible symbol intelligence document |

## Context & Masking

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_mask_context` | `content` | Compress large tool result observations |
| `touring_incremental_status` | — | Parser cache hit rate status |
| `touring_scan_pii` | `text` | Detect Brazilian PII (CPF, CNPJ, etc.) |

## Session & Project

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_session` | `action` | Session lifecycle: start, checkpoint, assess, end, list |
| `touring_classify_intent` | `prompt` | CILA intent classification (L0-L6) |
| `touring_project` | — | Get project configuration and status |
| `touring_resolve_project` | `file_path` | Resolve file path to project root |

## Jobs / L7-B

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_spawn_worker` | `tool_name, program, args` | Spawn background worker |
| `touring_poll_worker` | `job_id` | Poll job status |
| `touring_list_jobs` | — | List all jobs |
| `touring_drop_job` | `job_id` | Drop job from registry |

## Advanced

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_graph` | `action` | Dependency graph, blast radius, import extraction |
| `touring_refactor` | `action` | Safe refactoring: analyze impact, rename with AST |
| `touring_file_ops` | `action` | File operations: read, write, append, delete, find, glob, tree |
| `touring_wasm_plugin` | `plugin_name, input?` | Execute WebAssembly plugin with fuel metering |
| `touring_profile_query` | `section?, top_n?, include_percentiles?` | Query in-process hdrhistogram profile aggregator (p50/p90/p99/p999 latency by label) |
| `touring_ssr_apply` | `pattern, replacement, source, lang?` | Apply Structural Search & Replace (SSR) rule to source code |

## Tool → Action Mapping

### Tools that require `action` parameter

| Tool | Actions válidos |
|------|-----------------|
| `touring_decompose` | create, add_subtask, update_status, get_plan, list_tasks, get_ready_subtasks, validate_order |
| `touring_session` | start, checkpoint, assess, end, list, get |
| `touring_suggest` | next_action, similar_patterns, skill_recommendation, code_pattern |
| `touring_learn_pattern` | update, get_q, best_action, reset_traces |
| `touring_evolve` | extract_patterns, update_qtable, auto_learn, consolidate_memory, drift_report, recommend |
| `touring_wiring` | status, orphans, modules |
| `touring_memory_clusters` | list, stats, members, similar |
| `touring_graph` | index, blast_radius, dependency_path, imports, query |
| `touring_gotcha` | list, stats |
| `touring_refactor` | analyze, rename, validate, preview |
| `touring_checkpoint` | checkpoint/create |
| `touring_file_ops` | read, write, append, delete, find, stat, exists, mkdir, copy, move, glob, tree, list |

### Tools that do NOT require `action` parameter

| Tool |
|------|
| `touring_index_status` |
| `touring_memory_store` |
| `touring_memory_recall` |
| `touring_classify_intent` |
| `touring_scan_pii` |
| `touring_mcts_search` |
| `touring_evolution_drift` |
| `touring_evolution_status` |
| `touring_online_learn` |
| `touring_mask_context` |
| `touring_incremental_status` |
| `touring_ast_find` |
| `touring_ast_overview` |
| `touring_ast_edit` |
| `touring_project` |
| `touring_resolve_project` |
| `touring_wiring_audit` |
| `touring_insights` |
| `touring_wasm_plugin` |
| `touring_tantivy_search` |
| `touring_tantivy_fuzzy` |
| `touring_tantivy_stats` |
| `touring_tantivy_suggest` |
| `touring_tantivy_reindex` |
| `touring_scip_emit` |
| `touring_file_metadata` |
| `touring_spawn_worker` |
| `touring_poll_worker` |
| `touring_list_jobs` |
| `touring_drop_job` |

## Assists & SourceChange (Wave C v4.27.0)

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_assist_list_kinds` | — | List 10 assist handlers |
| `touring_assist_applicable` | `file_path, line, col` | Get applicable assists at cursor |
| `touring_assist_apply` | `kind, file_path, range` | Apply assist — produces SourceChange via Applier |
| `touring_ssr_apply` | `pattern, replacement, file_path?` | Semantic structural rewrite |
| `touring_source_change_apply` | `change_json, file_path?` | Apply SourceChange transactionally |
| `touring_resolve_def` | `file_path, line, col` | LSP resolve_definition |
| `touring_find_references` | `file_path, line, col` | LSP find_references |
| `touring_rename` | `file_path, line, col, new_name` | LSP rename symbol |
| `touring_fix_apply` | `fix_json` | Apply LSP code fix |

## Profile (Wave A v4.25.0)

| Tool | Params | Descrição |
|------|--------|-----------|
| `touring_profile_query` | `file_path` | Query profile data for file |
| `touring_profile_dump` | `output_path?` | Dump all profile data to JSON |

## Context Router — `ctx_*` (D6 + Wave 2026-05-08)

The `ctx_*` family in `touring-hooks::cli_handlers_mcp` is the multi-agent context router (D6 master plan). After Wave 2026-05-08 cross-audit, 4 new tools were added (3 master plan initiatives + 1 D6 wiring) bringing the total to **9 ctx_* tools**.

| Tool | Params | Initiative | Descrição |
|------|--------|------------|-----------|
| `ctx_search` | `router, query, top_k` | D6.1 | BM25 + 3-way RRF (porter ⊕ trigram ⊕ fuzzy). Honors I-02 PhraseQuery + I-03 5× heading boost. |
| `ctx_search_throttled` | `router, query, top_k, session_id` | I-10 | Wraps `ctx_search` with 3-tier per-session throttling (Tier1 ≤3, Tier2 4-8 reduces top_k, Tier3 ≥9 redirects to batch). |
| `ctx_index` | `router, doc: ToolOutputDoc` | D6.2 | Persists capture in tool_outputs index (I-05 TTL skip + I-06 JSON tool_args + I-09 DateField dual-write). |
| `ctx_retrieve` | `router, content_hash` | D6.3 | Legacy retrieval — returns stored prefix-truncated summary. |
| `ctx_retrieve_with_query` | `router, content_hash, query` | I-04 | Smart-snippet variant — runs SnippetGenerator over indexed `summary` field with HTML strip. |
| `ctx_insight` | `router, query` | D6.4 | Combined symbol search + tool_outputs summary BM25 search. |
| `ctx_compress` | `conn, session_id` | D6.5 | PreCompact filter — returns counts of CRITICAL/HIGH events that survive compaction (D3.3 + I-13 5-tier). |
| `ctx_session_guide` | `guide: SessionGuide` | I-15 | Renders structured 15-section Markdown guide for SessionStart context resume. |
| `ctx_aggregate` | `router, field_name, max_buckets` | I-07 | Group symbols by field (e.g. `symbol_kind`, `crate_name`, `language`); top buckets sorted desc by count. |
| `ctx_facets` | `router, prefix, max_buckets` | I-08 | Hierarchical facet drill-down `/Lang/Crate/Kind/Visibility`. Prefix narrows scope. |
| `ctx_cleanup` | `router, retention_secs?` | I-05 | Manual retention purge — defaults to `TOURING_TOOL_OUTPUTS_RETENTION_SECS` (14d). |

Constants:
- `CTX_MCP_TOOL_NAMES: &[&str]` — registry of 9 tool names for MCP server iteration.
- `ctx_mcp_tool_count() -> usize` — returns 9.

All tools are feature-gated under `tantivy-fts` (default ON). Disabled feature → JSON envelope `{"ok": false, "error": "tantivy_disabled"}`.

Module: `crates/touring-hooks/src/cli_handlers_mcp.rs`.
