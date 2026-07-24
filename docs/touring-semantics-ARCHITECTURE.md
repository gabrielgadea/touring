# touring-semantics -- Architecture

> **Version**: v30.3.5 | **Updated**: 2026-04-30 | **Tests**: 61+ | **LOC**: ~400

## Overview

Unified semantic abstraction layer over tree-sitter syntax trees. Provides a single `Definition` enum that covers all symbol kinds (Function, Struct, Trait, Module, Variant, Macro, Field, Variable, Lifetime, Generic) across multiple languages (Rust, JS/TS, Python, Go), with a `Semantics` facade for resolving definitions from syntax nodes via parent-chain walking.

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `Definition` | definition.rs | Unified enum for all symbol kinds across languages |
| `DefinitionId` | definition.rs | Opaque u32 identifier for a definition occurrence |
| `DefinitionKind` | definition.rs | Discriminated Kind dispatch (Function/Struct/etc.) |
| `FileRange` | definition.rs | Source location (file, start..end) |
| `Usage` | definition.rs | A use-site of a Definition (ref to def) |
| `UsageKind` | definition.rs | Kind of usage (Read/Write/Call/etc.) |
| `Semantics` | semantics.rs | Facade: cached definition resolver from syntax node |
| `source_to_definition` | source_to_def.rs | Recursive parent-walking algorithm entry point |
| `lang_to_definition` | multi_lang.rs | Maps language-specific tree-sitter nodes to Definition |
| `LangDefinitionMapping` | multi_lang.rs | Language-specific Definition mapping table |

## Dependencies

| Crate | Why |
|-------|-----|
| `tree-sitter` | Core syntax tree parsing and node types |
| `touring-ast` | Languages enum (Lang) and ParsedFile |
| `touring-vfs` | Used transitively for file range resolution |
| `serde` | Serialization for Definition/Usage |
| `thiserror` / `anyhow` | Error handling |
| `once_cell` | Static initialization for language mappings |
| `indexmap` | Deterministic iteration for symbol tables |

## Feature Flags

None.

## Key Modules

| Module | Purpose |
|--------|---------|
| `definition` | Core types: `Definition` enum, `DefinitionId`, `FileRange`, `Usage`/`UsageKind` |
| `semantics` | `Semantics` facade with `resolve_definition()` and caches |
| `source_to_def` | Parent-walking algorithm: finds enclosing definition from any node |
| `multi_lang` | Language-specific node-to-Definition mapping via tree-sitter |

## Invariants

1. `DefinitionId` is assigned per occurrence, not per declaration -- the same declaration referenced from two call sites yields two distinct `DefinitionId` values.
2. `Semantics::resolve_definition()` is idempotent per node byte offset due to `def_cache`.
3. The 10 Rust-rich `Definition` variants (Function, Struct, Trait, Module, Variant, Macro, Field, Variable, Lifetime, Generic) carry full fidelity; the multi-language subset (Class, Interface, Enum, etc.) is a lowered representation mapping to those 10 variants.
4. `source_to_definition` walks only upward (parent chain) -- it never descends into child nodes.

## Tests

61+ tests. Run with:

```bash
cargo test -p touring-semantics
```