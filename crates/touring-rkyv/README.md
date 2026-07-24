# touring-rkyv — Zero-Copy Serialization Templates

> Zero-copy serialization templates for cross-crate IPC and persistence in the Touring workspace.

## Purpose

This crate provides a **centralized library of rkyv Archived types** used across multiple Touring crates for zero-copy serialization. Instead of each crate defining its own structurally identical rkyv types, they import from `touring_rkyv::templates::*`.

**Key principle**: Types here are designed for **cross-crate data sharing**. For internal/pipeline snapshots with different lifecycles, crates may use raw rkyv directly (see [Why Not touring-rkyv?](#why-not-touring-rkyv) below).

## Template Types (13 Total)

### Hook Event Templates (IPC)

| Type | Purpose | CheckBytes |
|------|---------|-----------|
| `ArchivedHookEvent` | Hook event record for zero-copy IPC | ✅ |
| `ArchivedEventRecord` | RL learning — tool outcome tracking | ✅ |

### Symbol & Index Templates

| Type | Purpose | CheckBytes |
|------|---------|-----------|
| `ArchivedSymbol` | Zero-copy symbol index snapshots | ✅ |
| `ArchivedIndexSnapshot` | Dependency edge snapshot for blast_radius | ✅ |

### RL Learning Templates

| Type | Purpose | CheckBytes |
|------|---------|-----------|
| `ArchivedLearningParamsSnapshot` | QTable learning parameters | ✅ |
| `ArchivedQTableSnapshot` | QTable state for persistence | ✅ |
| `ArchivedLinUCBArmSnapshot` | Single LinUCB arm | ✅ |
| `ArchivedLinUCBSnapshot` | LinUCB bandit state | ✅ |

### CRDT Graph Templates

| Type | Purpose | CheckBytes |
|------|---------|-----------|
| `ArchivedCrdtEdge` | CRDT edge for graph snapshots | ✅ |
| `ArchivedNodeWeight` | Node weight entry | ✅ |
| `ArchivedGraphSnapshot` | Full CRDT graph state | ✅ |

### Cognitive / GoT Templates

| Type | Purpose | CheckBytes |
|------|---------|-----------|
| `ArchivedGotNodeSnapshot` | Single GoT node | ✅ |
| `ArchivedGoTSnapshot` | Complete GoT session | ✅ |

## Usage

```rust
use touring_rkyv::templates::*;
use rkyv::{Archive, Deserialize, Serialize};

// Serialize
let event = ArchivedHookEvent { /* ... */ };
let bytes = rkyv::to_bytes::<ArchivedHookEvent, 8192>(&event).unwrap();

// Deserialize (validated)
let archived = unsafe { rkyv::archived_root::<ArchivedHookEvent>(&bytes) };
let owned: ArchivedHookEvent = Deserialize::<ArchivedHookEvent, _>::deserialize(
    archived, &mut rkyv::Infallible
).unwrap();
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | ✅ | Standard library support |
| `validation` | ✅ | Enables `check_bytes` validation |

## Why Not touring-rkyv?

Some crates use raw rkyv instead of these templates for specific architectural reasons:

### touring-cognitive (`snapshot.rs`)

`GoTSnapshot` in touring-cognitive captures **complete GoT engine state** (max_depth, beam_width, pheromone_trails, created_at_secs) for session pause/resume. This is semantically different from the minimal `ArchivedGoTSnapshot` template which is a **cross-crate IPC format**. The two are not interchangeable.

### touring-generator (`RkyvFileSnapshotAdapter`)

Used for **internal pipeline snapshots** (speculative validation, plan rollback). These are ephemeral process-local artifacts, not cross-process IPC. The custom binary format and short-lived lifecycle justify raw rkyv usage.

## Adding New Templates

1. Define the type in `src/templates.rs` with `#[derive(Archive, Serialize, Deserialize, Debug)]`
2. Add `#[archive(check_bytes)]` for byte validation on deserialization
3. Add `#[archive_attr(derive(Debug))]` for archived type debug
4. Add docstrings explaining purpose and which crate uses it
5. Export from `src/lib.rs` under `pub mod templates`
6. Add round-trip tests in `tests/round_trip.rs`
7. Update this README

## Architecture

```
touring-rkyv (templates)
    ├── ArchivedHookEvent      ← touring-hooks (IPC)
    ├── ArchivedEventRecord    ← touring-hooks (RL event sourcing)
    ├── ArchivedSymbol         ← touring-index (symbol snapshots)
    ├── ArchivedIndexSnapshot ← touring-hooks (dependency_cache refactored)
    ├── ArchivedLearning*     ← touring-learning (RL persistence)
    ├── ArchivedCrdt*         ← touring-learning (CRDT graph)
    └── ArchivedGot*          ← touring-cognitive (GoT snapshots)
```

## Duplication Risk

**Important**: Several crates previously defined local types structurally identical to these templates:
- `touring-hooks/src/dependency_cache.rs`: local `IndexSnapshot` → **Refactored to use template** ✅
- `touring-cognitive/src/snapshot.rs`: local `GotNodeSnapshot` / `GoTSnapshot` → **Kept local** (different schema)

Before adding a new template, check if a structurally identical type already exists locally in a consumer crate. If the schemas match and the crate already depends on touring-rkyv, refactor the local type to use the template.
