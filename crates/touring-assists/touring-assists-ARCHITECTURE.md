# touring-assists — Architecture

> **Version**: v30.3.5 | **Updated**: 2026-05-11 | **LOC**: 1964

## Overview

Refactor-as-CLI framework — 10 assist handlers powered by tree-sitter AST analysis. Provides labeled SourceChange actions (auto_wire, extract_function, inline_call, etc.) that flow through the touring-generator pipeline.

## Key Types

`AssistId` | `AssistHandler` | `AssistGroup` | `LazySourceChange` | `Assist`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 18 | Crate entry, module re-exports |
| `handlers/generate_impl.rs` | 230 | Generate impl block for trait/interface |
| `handlers/merge_imports.rs` | 206 | Merge duplicate import statements |
| `handlers/change_visibility.rs` | 192 | Toggle pub/private visibility |
| `handlers/extract_function.rs` | 171 | Extract AST range into new function |
| `handlers/inline_call.rs` | 161 | Inline function body at call site |
| `framework/assist.rs` | 143 | Assist / LazySourceChange type definitions |
| `handlers/auto_wire.rs` | 142 | Auto-wire orphan symbols to consumers |
| `handlers/add_missing_match_arms.rs` | 140 | Add missing match arms from enum variants |
| `handlers/move_module_to_file.rs` | 108 | Move module to dedicated file |
| `handlers/convert_to_guarded_return.rs` | 98 | Convert early-return guard to if-else |
| `handlers/format_rust_preserve.rs` | 92 | Format rust with doc comment preservation |
| `framework/assists.rs` | 58 | Assists collection and group operations |
| `handlers/mod.rs` | 57 | Handler registry and routing |
| `handlers/auto_import.rs` | 52 | Auto-add missing import statements |
| `framework/context.rs` | 43 | AssistContext — project root + AST access |
| `handlers/conversions.rs` | 32 | Type conversion helpers |
| `framework/macros.rs` | 19 | Assist handler proc-macro definitions |

## Key Features

- **10 assist handlers**: auto_wire, extract_function, inline_call, auto_import, generate_impl, merge_imports, change_visibility, add_missing_match_arms, move_module_to_file, convert_to_guarded_return
- **LazySourceChange**: deferred rendering avoids AST cost when listing only
- **AST-based**: tree-sitter for language-aware transformations
- **Framework**: AssistHandler trait + AssistGroups for organization

## Integration Points

- touring-ast: AST parsing and symbol extraction
- touring-generator: SourceChange flows through render/commit pipeline
- touring-core: Error types and config

## Technology

Pure Rust. tree-sitter for AST. No unsafe at crate level.
