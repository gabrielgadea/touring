# touring-capnp-server — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 1513

## Overview

Cap'n Proto RPC server for Touring — provides high-performance RPC server using Cap'n Proto serialization. 17 modules for low-latency inter-service communication.

## Key Types

`CapnpServer` | `RpcHandler` | `Error`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 75 | Library entry, public API |
| `src/generator_health.rs` | 378 | — |
| `src/discover.rs` | 358 | — |
| `src/server.rs` | 204 | — |
| `src/holon_impl.rs` | 197 | — |
| `src/embed.rs` | 187 | — |
| `src/bin/touring_capnp.rs` | 114 | — |

## Key Features

- **Cap'n Proto RPC**: High-performance RPC server
- **Streaming**: Streaming RPC support
- **Error handling**: Cap'n Proto error integration

## Integration Points

- touring-server: RPC server for daemon communication
- touring-wasm: WASM plugin RPC support

## Technology

Pure Rust. capnp-rpc for RPC. No unsafe at crate level.
