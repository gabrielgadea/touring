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

Call the **façade**, not `rkyv` directly. These functions keep the pre-0.8 call
shape on purpose, so the underlying version can move again without touching you:

```rust
use touring_rkyv::templates::*;

// Serialize — the `8192` is the old scratch hint; 0.8 manages scratch itself,
// and the parameter is kept purely so existing call sites did not have to change.
let event = ArchivedHookEvent { /* ... */ };
let bytes = touring_rkyv::to_bytes::<ArchivedHookEvent, 8192>(&event).unwrap();

// Validated access (rejects corrupt or foreign-version bytes)
let archived = touring_rkyv::check_archived_root::<ArchivedHookEvent>(&bytes).unwrap();

// Owned value back
let owned: ArchivedHookEvent = touring_rkyv::deserialize(archived).unwrap();

// Or straight from bytes, validated in one step
let owned: ArchivedHookEvent = touring_rkyv::from_bytes(&bytes).unwrap();
```

`touring_rkyv::archived_root` is the unvalidated (`unsafe`) counterpart — use it only
for bytes this process just produced. Anything arriving from a socket or a file on
disk goes through `check_archived_root` / `from_bytes`, which is what makes a
foreign-version archive fail loudly instead of being read as garbage.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `std` | ✅ | Standard library support |
| `validation` | ✅ | Byte validation. The name is kept for consumers; since 0.8 it forwards to rkyv's renamed `bytecheck` feature |

## Why Not touring-rkyv?

Some crates use raw rkyv instead of these templates for specific architectural reasons:

### touring-cognitive (`snapshot.rs`)

`GoTSnapshot` in touring-cognitive captures **complete GoT engine state** (max_depth, beam_width, pheromone_trails, created_at_secs) for session pause/resume. This is semantically different from the minimal `ArchivedGoTSnapshot` template which is a **cross-crate IPC format**. The two are not interchangeable.

### touring-generator (`RkyvFileSnapshotAdapter`)

Used for **internal pipeline snapshots** (speculative validation, plan rollback). These are ephemeral process-local artifacts, not cross-process IPC. The custom binary format and short-lived lifecycle justify raw rkyv usage.

## Adding New Templates

1. Define the type in `src/templates.rs` with `#[derive(Archive, Serialize, Deserialize, Debug)]`
2. Byte validation needs **no attribute** — since rkyv 0.8 the derive emits `CheckBytes`
   automatically whenever the `bytecheck` feature is on (this crate's `validation`
   feature forwards to it). The 0.7 opt-in `#[archive(check_bytes)]` no longer exists.
3. Add `#[rkyv(derive(Debug))]` for archived-type debug (0.7 spelled this `#[archive_attr(...)]`)
4. Add docstrings explaining purpose and which crate uses it
5. Export from `src/lib.rs` under `pub mod templates`
6. Add round-trip tests in `tests/round_trip.rs` — go through the façade
   (`touring_rkyv::to_bytes` / `deserialize`), never `rkyv::*` directly, so the
   suite guards the adapters instead of bypassing them
7. Update this README

> **Derives must come from `rkyv` directly.** The helper attributes above are only in
> scope when the derive macro is imported from the original crate, so a crate defining
> archived types declares `rkyv` as a dependency alongside `touring-rkyv`. Everything
> else — every function and the `AlignedVec` type — goes through the façade. That split
> is what let the 0.7→0.8 jump land in one crate instead of eighty-eight call sites.

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
