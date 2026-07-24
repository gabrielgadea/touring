# touring-generator — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 12554

## Overview

Code generation pipeline with VGP (Verified Generation Protocol) — 40 modules implementing typestate Draft→Verified→Rendered→Speculated→Committed pipeline, symbol verification, source change application, and plan validation.

## Key Types

`PlanExecutor` | `SourceChange` | `GenerateError` | `PlanExecutorHandle` | `RenderShape`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 137 | Library entry, public API, re-exports |
| `src/core/context.rs` | 4113 | — |
| `src/executor/typestate.rs` | 1480 | — |
| `src/validate/pipeline.rs` | 773 | — |
| `src/vgp/engine.rs` | 642 | — |
| `src/source_change/applier.rs` | 446 | — |
| `src/validate/boundary.rs` | 380 | — |
| `src/skip/parser.rs` | 373 | — |
| `src/source_change/text_edit.rs` | 362 | — |
| `src/plan/schema.rs` | 336 | — |
| `src/error.rs` | 332 | — |
| `src/template/engine.rs` | 271 | — |
| `src/source_change/snippet.rs` | 262 | — |
| `src/source_change/mod.rs` | 243 | — |
| `src/plan/result.rs` | 234 | — |
| `src/plan/contracts.rs` | 223 | — |
| `src/shape.rs` | 217 | — |
| `src/generator/kinds.rs` | 215 | — |
| `src/core/adapters/concolic_pre_tool_adapter.rs` | 168 | — |
| `src/source_change/fs_edit.rs` | 156 | — |
| `src/vgp/fuzzy.rs` | 147 | — |

## Key Features

- **VGP typestate pipeline**: 5-stage typestate (Draft→Verified→Rendered→Speculated→Committed)
- **Symbol verification**: VGP V1+V2+V3+V4 batch verification
- **Source change**: Applier for transactional code edits
- **Plan validation**: Schema validation for plan documents
- **Skip context parser**: Skip region handling (W-115)

## Integration Points

- taco-forge: perfect-create-* workflows invoke touring-generator
- touring-ast: AST analysis for symbol verification
- touring-server: generator tools via MCP
- REGRA #14: taco-forge canonical workflows use touring-generator as engine

## Technology

Pure Rust. Tokio async. No unsafe at crate level.
