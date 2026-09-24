# touring-python — Architecture

> **Version**: v0.2.0 | **Updated**: 2026-09-24 | **Constraints**: `#![forbid(unsafe_code)]`

## Overview

O cdylib `claude_learning_kernel`: PyO3-based Python bindings exposing ACO,
NLP, SIMD, AST, RL, cognitive, and financial subsystems to Python. **Two
crates, one contract each (24/09/2026)**: the `#[pymodule]` invocation lives
in THIS crate (`src/lib.rs`) — a cdylib only exports symbols of its own
crate — and the full implementation + the registration body live in
`touring-bindings` (`src/python/`, feature `bind-python`), which exposes
`pub fn register_all`. Before this split was made explicit, the invocation
lived in the rlib and the shim referenced nothing, so the linker dropped the
rlib and the `.so` shipped ZERO `PyInit` symbols — every import died with a
green build (the analise without Wilson/drift/`py_parse_monetary` since
23/08). The versioned build recipe (maturin) is `pyproject.toml`; the
export-table regression guard is `tests/pymodule_export.rs`.

## Key Types (registered via `register_all`)

`PyMonetaryValue` | `PyKeywordMatcher` | `PySemanticChunk` | `PyAcoGraph` | `TrackerStatus` — importable as `claude_learning_kernel.X` or, in the rust_bridge.py form, `claude_learning_kernel.claude_learning_kernel.X`.

## Module Map

| Crate / file | Responsibility |
|------|-----|
| `src/lib.rs` (este crate) | the `#[pymodule]` invocation + delegation to `register_all` |
| `pyproject.toml` | the versioned maturin recipe (wheel / `.so` for a target interpreter) |
| `touring-bindings/src/python/mod.rs` | `register_all` — the full registration body + exceptions |
| `touring-bindings/src/python/{aco,ast,ast_rl_bridge,cognitive,exceptions,financial,nlp,rl,rust_semantic,simd}_bindings.rs` | the per-subsystem registrations (called by `register_all`) |
| `tests/pymodule_export.rs` | regression guard: content invariant + `nm -D` proof that `PyInit_claude_learning_kernel` is exported |

## Key Features

- **ACO bindings**: Ant Colony Optimization via PyO3
- **NLP bindings**: Natural language processing
- **SIMD bindings**: SIMD vector operations
- **AST bindings**: Rust AST analysis from Python
- **RL bindings**: Reinforcement learning from Python

## Integration Points

- touring-intelligence::rl (era touring-learning): ACO via Python bindings
- touring-simd: SIMD operations from Python
- touring-code::ast (era touring-ast): AST analysis from Python
- Python tooling: external Python tools consume Touring subsystems

## Technology

PyO3 for Python bindings. No unsafe at crate level.
