# touring-rkyv — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 809

## Overview

Zero-copy serialization templates for Touring IPC — rkyv-based templates for daemon ↔ client communication, saga IPC, and plan template serialization. 13 template types with byte-validation on deserialization.

## Key Types

`SagaMessage` | `ArchivedHookEvent` | `ArchivedSymbol` | `FrameError` | `SagaError`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 57 | Crate entry, module re-exports, IPC public API |
| `src/saga_ipc.rs` | 318 | Distributed saga coordination messages (2PC prepare/decide/abort) |
| `src/templates.rs` | 220 | 13 archived template types (HookEvent, Symbol, IndexSnapshot, QTable, LinUCB, CRDT graph, etc.) |
| `src/ipc.rs` | 214 | Low-level rkyv serializer/deserializer for cross-process messaging |

## Key Features

- **Zero-copy**: rkyv for fast IPC with memory mapping
- **Byte validation**: #[archive(check_bytes)] on all template types
- **Saga IPC**: 2PC distributed saga coordination messages
- **13 template types**: HookEvent, Symbol, IndexSnapshot, QTable, LinUCB, ESAA events, CRDT graphs, GotNodeSnapshot

## Integration Points

- touring-server: daemon RPC communication
- touring-learning: RL state IPC (QTable, LinUCB snapshots)
- touring-hooks: hook event IPC (ArchivedHookEvent, ArchivedIndexSnapshot)
- touring-cognitive: GoTSnapshot local types (NOT shared — engine-specific state)

## Technology

Pure Rust. rkyv for zero-copy serialization. No unsafe at crate level.
