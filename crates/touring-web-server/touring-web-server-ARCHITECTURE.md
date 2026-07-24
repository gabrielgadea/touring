# touring-web-server — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 1705 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

Axum-based HTTP server binary that serves the touring-web WASM frontend. Provides static file serving, API endpoints, and WebSocket upgrade for real-time streaming.

## Key Types

`AppState` | `AppError` | `SnapshotStore` | `WsState`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 1378 | Server state, AppState, AppError, route handlers |
| `src/main.rs` | 10 | Binary entry point |
| `src/snapshots.rs` | 106 | SnapshotStore for WASM state persistence |
| `src/socket.rs` | 82 | WebSocket upgrade and handler, WsState |

## Responsibilities

- Serves touring-web WASM bundle as static assets
- Provides API endpoints for touring-core RPC
- WebSocket handler for real-time streaming (health delta, progress updates)
- Manages session snapshots for hot reload

## Integration Points

- `touring-web`: WASM frontend served as static assets
- `touring-core`: RPC API endpoints
- touring daemon: WebSocket streaming for real-time updates

## Technology

Pure Rust. `#![forbid(unsafe_code)]`. Axum web framework. No unsafe at crate level.
