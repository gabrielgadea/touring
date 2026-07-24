# touring-identity — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 1599

## Overview

Entity identity registry for Touring — deterministic EntityId derivation from canonical name + admission criteria per RFC-004. Provides identity verification, homonimia detection, and entity resolution across the workspace.

## Key Types

`EntityId` | `EntityKind` | `Criterion` | `MatchKind` | `Resolution` | `Error`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | ~250 | Identity entry, public API, re-exports |
| `src/registry.rs` | 858 | — |
| `src/types.rs` | 502 | — |
| `src/schema.rs` | 171 | — |
| `src/error.rs` | 41 | — |

## Key Features

- **EntityId derivation**: Deterministic, not emergent — derives from canonical name + admission criteria (RFC-004)
- **Homonimia detection**: Detects same name in different crates/modules
- **Entity resolution**: MatchKind + Resolution for identity verification

## Integration Points

- touring-ast: entity resolution for symbol lookup
- touring-semantics: resolve-def uses EntityId
- touring-hooks: hook context uses EntityId for identity

## Technology

Pure Rust. No unsafe at crate level.