# touring-python — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 3456 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

Python bindings for Touring subsystems — PyO3-based Python bindings exposing ACO, NLP, SIMD, AST, RL, cognitive, and financial subsystems to Python environment. Enables Python-first tooling to consume Touring internals.

## Key Types

`PyMonetaryValue` | `PyKeywordMatcher` | `PySemanticChunk` | `PyAcoGraph` | `TrackerStatus`

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 94 | PyO3 module entry, Python bindings init |
| `src/aco_bindings.rs` | 1028 | — |
| `src/nlp_bindings.rs` | 664 | — |
| `src/simd_bindings.rs` | 468 | — |
| `src/ast_bindings.rs` | 404 | — |
| `src/rl_bindings.rs` | 240 | — |
| `src/rust_semantic_bindings.rs` | 196 | — |
| `src/ast_rl_bridge.rs` | 144 | — |
| `src/financial_bindings.rs` | 99 | — |
| `src/cognitive_bindings.rs` | 76 | — |
| `src/exceptions.rs` | 43 | — |

## Key Features

- **ACO bindings**: Ant Colony Optimization via PyO3
- **NLP bindings**: Natural language processing
- **SIMD bindings**: SIMD vector operations
- **AST bindings**: Rust AST analysis from Python
- **RL bindings**: Reinforcement learning from Python

## Integration Points

- touring-learning: ACO via Python bindings
- touring-simd: SIMD operations from Python
- touring-ast: AST analysis from Python
- Python tooling: external Python tools consume Touring subsystems

## Technology

PyO3 for Python bindings. No unsafe at crate level.
