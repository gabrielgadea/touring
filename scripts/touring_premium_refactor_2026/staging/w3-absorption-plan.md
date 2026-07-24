# W3.2 — Anemic Crate Absorption Plan

Generated: 2026-05-11T23:50:23.247886+00:00

## Thresholds
- src_loc < 500
- pub_count < 10
- fan_in ≤ 2

## Summary: 6 anemic crates

| Crate | LOC | Pub | Fan-in | Target |
|-------|-----|-----|--------|--------|
| `touring-integration-tests` | 6 | 0 | 0 | `?` |
| `touring-loom-proofs` | 11 | 0 | 0 | `?` |
| `touring-semantic-spike` | 67 | 0 | 0 | `?` |
| `touring-wasm-client` | 0 | 0 | 0 | `?` |
| `touring-wasm-common` | 0 | 0 | 0 | `?` |
| `touring-wasm-server` | 0 | 0 | 0 | `?` |

## Per-crate absorption steps

### `touring-integration-tests` → `TARGET`

- Src LOC: 6
- Pub items: 0
- Consumers (0): (none)
- [ ] Copy `src/` into target crate as module
- [ ] Remove from `workspace.members` in root Cargo.toml
- [ ] Update consumers: `use touring_integration_tests::*` → `use TARGET::tests::*`
- [ ] cargo check --workspace passes

### `touring-loom-proofs` → `TARGET`

- Src LOC: 11
- Pub items: 0
- Consumers (0): (none)
- [ ] Copy `src/` into target crate as module
- [ ] Remove from `workspace.members` in root Cargo.toml
- [ ] Update consumers: `use touring_loom_proofs::*` → `use TARGET::proofs::*`
- [ ] cargo check --workspace passes

### `touring-semantic-spike` → `TARGET`

- Src LOC: 67
- Pub items: 0
- Consumers (0): (none)
- [ ] Copy `src/` into target crate as module
- [ ] Remove from `workspace.members` in root Cargo.toml
- [ ] Update consumers: `use touring_semantic_spike::*` → `use TARGET::spike::*`
- [ ] cargo check --workspace passes

### `touring-wasm-client` → `TARGET`

- Src LOC: 0
- Pub items: 0
- Consumers (0): (none)
- [ ] Copy `src/` into target crate as module
- [ ] Remove from `workspace.members` in root Cargo.toml
- [ ] Update consumers: `use touring_wasm_client::*` → `use TARGET::client::*`
- [ ] cargo check --workspace passes

### `touring-wasm-common` → `TARGET`

- Src LOC: 0
- Pub items: 0
- Consumers (0): (none)
- [ ] Copy `src/` into target crate as module
- [ ] Remove from `workspace.members` in root Cargo.toml
- [ ] Update consumers: `use touring_wasm_common::*` → `use TARGET::common::*`
- [ ] cargo check --workspace passes

### `touring-wasm-server` → `TARGET`

- Src LOC: 0
- Pub items: 0
- Consumers (0): (none)
- [ ] Copy `src/` into target crate as module
- [ ] Remove from `workspace.members` in root Cargo.toml
- [ ] Update consumers: `use touring_wasm_server::*` → `use TARGET::server::*`
- [ ] cargo check --workspace passes

