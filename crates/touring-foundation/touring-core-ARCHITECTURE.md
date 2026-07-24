# touring-core — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 13686 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

Core shared library for Touring — provides embedding client, migration consolidation, domain circuit, diagnostic system, schema entity registry, checkpoint fingerprinting, and governor rate limiting. 62 modules that form the foundation all other Touring crates depend on.

## Key Types

`EmbeddingClient` (type alias) | `DomainCircuitBreaker` | `Diagnostic` | `EntityRegistry` | `EntityRegistryError` | `ResourceGovernor` | `TouringError`
|------|-----|----------------|
| `src/lib.rs` | 41 | Core entry, re-exports, public API |
| `src/embedding/client.rs` | 794 | Embedding client — semantic search, similarity |
| `src/migration/consolidation.rs` | 755 | Migration consolidation — schema upgrades |
| `src/diagnostic.rs` | 523 | Diagnostic system — error codes, severity, reporting |
| `src/shared/domain_circuit.rs` | 485 | Domain circuit — cross-subsystem wiring state |
| `src/config.rs` | 394 | Configuration loading and management |
| `src/types.rs` | 396 | Public type definitions |
| `src/char_classes/mod.rs` | 408 | Character class state machine |
| `src/schema/entity_registry.rs` | 383 | Entity registry — schema definitions |
| `src/checkpoint/fingerprint.rs` | 332 | Checkpoint fingerprinting |
| `src/governor/mod.rs` | 255 | Governor — rate limiting and throttling |
| `src/migration.rs` | 229 | Migration module |
| `src/error.rs` | 127 | `TouringError` enum |
| `src/schema/knowledge.rs` | 201 | Schema knowledge graph |
| `src/cgm/graph_attention.rs` | 196 | Graph attention mechanism |
| `src/failover/coordinator.rs` | 190 | Failover coordinator |
| `src/profile/aggregator.rs` | 188 | Profile aggregator |
| `src/health_events.rs` | 151 | Health event sourcing |
| `src/plugin/registry.rs` | 155 | Plugin registry |

## Key Features

- **Embedding Client**: Semantic embedding and similarity search
- **Domain Circuit**: Cross-subsystem wiring state tracking
- **Diagnostic System**: Unified diagnostic codes (RFC-001..005 framework)
- **Schema Entity Registry**: Schema version management and migration
- **Checkpoint Fingerprinting**: Content-addressed snapshots
- **Governor**: Rate limiting with adaptive throttling
- **Character Class State Machine**: Multi-language token classification

## Integration Points

- touring-hooks: core utilities, diagnostic codes, domain circuit
- touring-server: embedding client for semantic search
- touring-learning: governor for RL throttle
- touring-index: schema entity registry
- All crates depend on touring-core for shared types and utilities
- REGRA #0: All pub symbols must have consumers or be documented as intentional orphans

## Technology

Pure Rust. No unsafe. Tokio async. serde for serialization. No external crate dependencies beyond std.