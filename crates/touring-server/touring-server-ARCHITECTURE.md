# touring-server — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 61147 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

Main touring daemon server and CLI entry point — handles MCP tool execution, CLI dispatch, Tantivy search integration, and context mode operations. Exports 74 CLI command handlers and 88+ MCP tools across reasoning, analysis, generation, and infrastructure.

## Key Types

`TouringServer` | `TaskDecomposer` | `SubTask` | `CheckpointManager` | `SessionManager` | `MemoryRecallInput` | `MemoryStoreInput` | `MinimalContext` | `GranularityHint` | `GateResult` | `ChangeRiskReport` | `WiringAuditResult`
|------|-----|----------------|
| `src/lib.rs` | 100 | Server entry, module re-exports |
| `src/reasoning/decomposer.rs` | 2005 | `TaskDecomposer`, `SubTask`, `Task`, `RetryPolicy`, `ComplexityHint`, `ParallelGroup`, `CilaLevel`, `DeadlineBehavior` |
| `src/server/tools_infra.rs` | 1961 | Tool infrastructure — execute, invoke, suggest |
| `src/server/tools_analysis.rs` | 1807 | Analysis tools — ast, wiring, memory, index queries |
| `src/server/tools_core.rs` | 1633 | Core tool implementations — session, decompose, learning |
| `src/tools/generator_tools.rs` | 1623 | Generator tools — create, edit, refactor via touring-generator |
| `src/context_compiler.rs` | 1134 | Context compiler for session injection |
| `src/cli/migrate.rs` | 1095 | CLI migration commands |
| `src/server/tools_metadata.rs` | 1055 | Metadata tools — symbol info, file knowledge |
| `src/cli/generate.rs` | 922 | CLI generation commands |
| `src/server/mod.rs` | 913 | `TouringServer` struct, MCP server definition |
| `src/cli/common.rs` | 906 | CLI shared utilities, `ErrorPolicy` |
| `src/reasoning/persistence.rs` | 792 | `CheckpointManager`, `PersistedTask`, `PersistedSubTask` |
| `src/observation_masker.rs` | 727 | Observation masking for RL |
| `src/graph_service.rs` | 603 | Graph service for wiring |
| `src/session/manager.rs` | 493 | `SessionManager`, `Session`, `SessionStatus` |
| `src/tools/memory_tools.rs` | 550 | `MemoryRecallInput`, `MemoryStoreOutput`, `MemoryTools` |
| `src/tools/context_tools.rs` | 480 | `MinimalContext`, `ContextStats`, `ToolSuggestion` |
| `src/tools/refactor_preview.rs` | 420 | `RefactorPreview`, `RenameEdit`, `RefactorStore` |
| `src/tools/risk_scoring.rs` | 400 | `ChangeRiskReport`, `CriticalityScore`, `Hotspot` |
| `src/tools/wiring_audit.rs` | 380 | `WiringAuditResult`, `WiringAuditFull`, `ModuleScore` |
| `src/tools/session_hints.rs` | 446 | `SessionHint`, `ToolCall`, `SessionHintEngine` |
| `src/tools/ast_tools.rs` | 468 | `AstOverviewTool`, `SymbolAtLineArgs`, `FileContent` |
| `src/cli/decompose.rs` | ~600 | CLI decompose handlers |
| `src/cli/session.rs` | ~580 | CLI session commands |
| `src/cli/index.rs` | ~550 | CLI index commands |
| `src/cli/wiring.rs` | ~619 | CLI wiring commands |
| `src/cli/memory.rs` | ~500 | CLI memory commands |
| `src/cli/learning.rs` | ~480 | CLI learning commands |
| `src/cli/ast.rs` | ~450 | CLI AST commands |
| `src/cli/touring.rs` | ~420 | CLI touring commands |
| `src/session/manager.rs` | 493 | `SessionManager`, `Session` |
| `src/tools/summary_cache.rs` | 360 | `SummaryCache` |
| `src/tools/drift.rs` | 340 | `DriftInput`, `DriftOutput`, `DriftMetricResult` |
| `src/tools/hybrid_search.rs` | 310 | `HybridResult` |
| `src/tools/clone_tools.rs` | 350 | `DetectClonesParams`, `CloneGroup` |
| `src/tools/project_tools.rs` | 380 | `ProjectTools`, `ProjectInfo`, `ProjectAction` |
| `src/tools/cluster_tools.rs` | 370 | `MemoryClustersOutput`, `ClusterInfo` |
| `src/tools/ctx_execute_tools.rs` | 320 | `CtxExecuteInput`, `CtxExecuteOutput`, `CtxExecuteError` |
| `src/agent_diary.rs` | 559 | Agent diary for AAAK |
| `src/rl_mapping.rs` | 390 | RL mapping utilities |
| `src/scip_emit.rs` | 280 | SCIP emission for code intelligence |

## CLI Architecture (74 .rs files in src/cli/)

The touring CLI lives in `src/cli/` as 74 command handlers:
- `cli/decompose.rs` — task decomposition DAG management
- `cli/session.rs` — session lifecycle (start/assess/stop)
- `cli/index.rs` — symbol index operations
- `cli/wiring.rs` — wiring graph queries
- `cli/memory.rs` — memory store/recall
- `cli/learning.rs` — RL reward injection
- `cli/ast.rs` — AST analysis commands
- `cli/touring.rs` — touring status/doctor/synergy
- `cli/migrate.rs` — migration tools

## MCP Tools (88+ tools)

Organized into: reasoning (decompose, suggest), analysis (ast, wiring, index), generation (create, edit, assist), infrastructure (session, memory, learning, health).

## Integration Points

- Tantivy index: all search operations via touring-tantivy
- touring-ast: AST analysis via tour AST integration
- touring-learning: RL via LinUCB + Q-table
- touring-hooks: hook runtime for lifecycle events
- Daemon socket: `/tmp/touring-daemon-1000.sock` (RPC)
- REGRA #0: All pub symbols must have consumers or be documented as intentional orphans

## Technology

Rust async via Tokio. rkyv for IPC serialization. Tantivy for search. `#![forbid(unsafe_code)]`. No unsafe at crate level.
