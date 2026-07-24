# touring-integration-tests — Architecture

> **Version**: v0.1.0 | **Updated**: 2026-05-11 | **LOC**: 0

## Overview

Integration test suite for Touring ecosystem — end-to-end tests validating cross-subsystem integration, CLI commands, MCP tools, and daemon lifecycle. Located in `tests/` directory rather than `src/`.

## Key Types

N/A (test-only crate)

## Module Map

| File | LOC | Responsibility |
|------|-----|----------------|
| `src/lib.rs` | 6 | Test entry point |
| `tests/` | — | E2E test modules |

## Key Features

- **CLI E2E tests**: End-to-end CLI command validation
- **MCP tool tests**: MCP tool integration tests
- **Daemon lifecycle tests**: Daemon startup/shutdown validation
- **Cross-subsystem tests**: Integration between touring-hooks, touring-server, touring-learning

## Integration Points

- touring-server: CLI integration tests
- touring-hooks: hook lifecycle tests
- touring-learning: RL integration tests

## Technology

Test-only crate. No unsafe at crate level.
