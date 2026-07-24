# touring-cortex — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 31823

## Overview

Context enrichment and neural processing engine for Touring — 60 modules providing enrichment pipeline, cache strategy, context compilation, and handlers for intelligence, tools, neural, and lifecycle events.

## Key Types

`CortexRuntime` | `Pipeline` | `CortexContext` | `DispatchError`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 112 | Library entry, public API |
| `src/handlers/enrichment.rs` | 2670 | — |
| `src/handlers/lifecycle.rs` | 2260 | — |
| `src/handlers/enforcement.rs` | 1623 | — |
| `src/pipeline.rs` | 1355 | — |
| `src/handlers/tools.rs` | 1113 | — |
| `src/enrichment.rs` | 1097 | — |
| `src/context.rs` | 1017 | — |
| `src/handlers/intelligence.rs` | 970 | — |
| `src/handlers/neural.rs` | 956 | — |
| `src/cache_strategy.rs` | 909 | — |
| `src/call_graph.rs` | 864 | — |
| `src/handlers/quality.rs` | 807 | — |
| `src/types.rs` | 798 | — |
| `src/cross_audit.rs` | 767 | — |
| `src/handlers/self_reflection.rs` | 664 | — |
| `src/handlers/test_generation.rs` | 634 | — |
| `src/handlers/mente.rs` | 624 | — |
| `src/handlers/dspy_compile.rs` | 623 | — |
| `src/handlers/incremental_indexing.rs` | 609 | — |
| `src/handlers/evolution.rs` | 543 | — |
| `src/fascicles/dispatcher.rs` | 542 | — |
| `src/signal_fusion.rs` | 519 | — |
| `src/handlers/reasoning_advanced.rs` | 514 | — |
| `src/fusion.rs` | 511 | — |
| `src/handlers/session.rs` | 505 | — |

## Key Features

- **Enrichment pipeline**: Multi-stage context enrichment
- **Cache strategy**: Adaptive caching for enrichment results
- **Context compiler**: Compiles session context for injection
- **Intelligence handlers**: Tool use prediction and suggestion
- **Neural handlers**: PII scanning, intent classification

## Integration Points

- touring-hooks: enrichment pipeline for pre-read
- touring-server: context compiler for session injection
- touring-learning: intelligence signals for RL

## Technology

Pure Rust. Tokio async. No unsafe at crate level.
