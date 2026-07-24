# touring-loom-proofs — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 0

## Overview

Loom concurrency proof-of-concept crate for Touring — contains loom proofs for concurrent data structures used in touring-core and touring-hooks. Verifies memory safety under concurrent access.

## Key Types

N/A (loom proofs only)

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 11 | Loom proofs entry |

## Key Features

- **Loom proofs**: Formal verification of concurrent correctness
- **Memory safety**: Proof that concurrent data structures are safe

## Integration Points

- touring-core: concurrent data structure proofs
- touring-hooks: concurrent hook runtime proofs

## Technology

Rust with loom crate for concurrent testing. No unsafe at crate level.
