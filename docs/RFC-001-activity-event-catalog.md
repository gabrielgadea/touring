# RFC-001: Touring Activity Event Catalog

**Status**: Active  
**Type**: Specification  
**Layer**: ESAA / S1  
**Author**: TACO (Constitution v8.0 Draft)  
**Date**: 2026-05-09  
**Version**: 1.0.0  

---

## 1. Context and Motivation

ESAA (Event Sourcing for Autonomous Agents) prescribes an append-only event store as
the single source of truth for agentic state. Touring v8.0 adopted this principle in
S1 (Activity Log) and implemented it in `touring-activity` crate.

This RFC formalizes the complete event catalog, establishing canonical type names,
field semantics, error taxonomy, and invariants that all consumers (scouts, architects,
engineers, auditors) must respect.

**Relation to S1**: This RFC supersedes the S1 "gap" identified in the v8 master plan
analysis (line 94 of master plan: "GAP — diary AAAK approximates but is not append-only
with monotonic event_seq"). The `touring-activity` crate closes that gap.

---

## 2. Event Type Catalog

All events are JSON objects over a UTF-8 wire. The canonical schema is at
`crates/touring-activity/schemas/event.schema.json` (JSON Schema draft-07).

### 2.1 EventAction Enum (12 + 1 variants)

| Variant | Snake_case | Description | Payload required? |
|---------|------------|-------------|-------------------|
| `TaskStarted` | `task_started` | Subtask entered execution | optional |
| `TaskCompleted` | `task_completed` | Subtask finished successfully | optional |
| `ToolInvoked` | `tool_invoked` | Claude Code tool called | optional |
| `HookFired` | `hook_fired` | Touring hook lifecycle event | optional |
| `SessionStarted` | `session_started` | Touring session opened | optional |
| `SessionEnded` | `session_ended` | Touring session closed | optional |
| `LearningSignal` | `learning_signal` | RL reward or learning metric emitted | optional |
| `MemoryStored` | `memory_stored` | Semantic or episodic memory persisted | optional |
| `ErrorOccurred` | `error_occurred` | Non-fatal error captured | recommended |
| `WireIntegrated` | `wire_integrated` | Orphan pub symbol wired to a consumer | optional |
| `IndexRebuilt` | `index_rebuilt` | Touring symbol index completed | optional |
| `DaemonHealth` | `daemon_health` | Periodic daemon health snapshot | optional |
| `BoundaryViolation` | `boundary_violation` | VGP L5 path boundary violation (S4) | recommended |

**Note**: `BoundaryViolation` is defined in `event.rs:27` with a comment referencing
`VGP Layer 5 path boundary violation (S4/D4.6)`. It is NOT in the JSON Schema enum
(line 24-37 of `event.schema.json`) — this is a schema drift issue for D9.7 audit.

### 2.2 EventId Format

```
Format:  {nanoseconds}-{sha256_prefix}
Example: "1746758401234567890-a1b2c3d4e5f6"
```

- **nanoseconds**: `SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()`
- **sha256_prefix**: First 8 bytes (16 hex chars) of SHA-256 of combined
  `{nanoseconds}-{uuid_v4}` — provides uniqueness without full UUID

**Invariant**: EventId MUST be parseable with `EventId::parse()` which validates
`contains('-') && len() >= 20`.

### 2.3 Actor Types

```json
{ "type": "Agent",   "id": "touring-scouter" }
{ "type": "System",  "id": "taco-forge" }
{ "type": "Daemon",  "id": "touring-daemon" }
```

| Type | Description |
|------|-------------|
| `Agent` | Claude Code subagent or orchestrator |
| `System` | External tool (taco-forge, touring CLI, cargo) |
| `Daemon` | Touring daemon process |

### 2.4 Monotonic Seq Invariant

`event.seq` MUST be a strictly increasing unsigned integer per event store.
The first event has `seq = 1`. No gaps, no duplicates, no decreases.

**Enforcement**: `store.rs` appends only if `seq == last_seq + 1` (verified in
`touring-activity/src/store.rs`).

### 2.5 Projection Hash

`event.projection_hash` is a SHA-256 of the canonical encoding of all fields:

```
SHA256( id || seq.to_le_bytes() || action.to_string() || actor.display()
        || timestamp_ns.to_le_bytes() || payload_json )
```

Computed by `Event::compute_projection_hash()` (event.rs:142-160).

**Invariant**: `event.verify_projection()` MUST return `true` for any event retrieved
from the store. Events with hash mismatch are **corrupted** and MUST NOT be processed.

---

## 3. Payload Schema

`payload` is `Option<serde_json::Value>`. Convention per action:

| Action | Typical payload fields |
|--------|------------------------|
| `task_started` | `task_id`, `task_kind`, `origin` |
| `task_completed` | `task_id`, `duration_ms`, `outcome` |
| `tool_invoked` | `tool_name`, `args_hash`, `duration_ms`, `exit_code` |
| `hook_fired` | `hook_name`, `event_name`, `duration_ms` |
| `learning_signal` | `signal_type`, `tool`, `value`, `context` |
| `memory_stored` | `memory_key`, `tier`, `value_size_bytes` |
| `error_occurred` | `error_code`, `message`, `recoverable` |
| `wire_integrated` | `symbol`, `consumer_file`, `consumer_line` |
| `boundary_violation` | `task_kind`, `file_path`, `violation_kind`, `matched_pattern` |
| `daemon_health` | `components_status`, `memory_rss_mb`, `uptime_seconds` |

---

## 4. Error Codes for `output.rejected` (ESAA)

When an event cannot be appended due to invariant violation, the store emits an
`output.rejected` error with one of the following codes:

| Code | Meaning | Layer |
|------|---------|-------|
| `SEQ_GAP` | seq is not last_seq + 1 | store |
| `SEQ_DUPLICATE` | seq already exists in store | store |
| `INVALID_EVENTID` | EventId::parse returns None | store |
| `HASH_MISMATCH` | verify_projection() returns false | verify |
| `PAYLOAD_TOO_LARGE` | payload bytes > 1 MiB | store |
| `INVALID_ACTOR` | actor.type not in {Agent,System,Daemon} | store |
| `TIMESTAMP_REGRESSION` | timestamp_ns < last_event.timestamp_ns | store |

---

## 5. Invariants (Audit-Garantidas)

| # | Invariant | Test script |
|---|-----------|-------------|
| I1 | `seq` strictly monotonic across 10k+ events | audit script #1 |
| I2 | SHA-256 of event reproduces `projection_hash` exactly | audit script #2 |
| I3 | No `seq` gap > 1 between consecutive events | audit script #1 |
| I4 | All events pass `verify_projection()` | audit script #2 |
| I5 | `BoundaryViolation` events include `task_kind`, `file_path`, `violation_kind` | audit script #6 |

---

## 6. Reference Implementation

| File | Purpose |
|------|---------|
| `crates/touring-activity/src/event.rs` | EventAction, Actor, EventId, Event structs |
| `crates/touring-activity/src/store.rs` | Append-only store with seq enforcement |
| `crates/touring-activity/src/projection.rs` | Deterministic projection logic |
| `crates/touring-activity/src/verify.rs` | Replay verification |
| `crates/touring-activity/schemas/event.schema.json` | JSON Schema (draft-07) |

---

## 7. Schema Discrepancy (For D9.7 Audit)

The JSON Schema enum (lines 24-37 of `event.schema.json`) lists only 12 actions.
`BoundaryViolation` (event.rs:27) is NOT in the enum. This must be corrected in D9.7
audit script #6.

---

## 8. Examples

### Valid event

```json
{
  "id": "1746758401234567890-a1b2c3d4e5f6",
  "seq": 1,
  "action": "task_started",
  "actor": { "type": "Agent", "id": "touring-scouter" },
  "timestamp_ns": 1746758401234567890,
  "payload": {
    "task_id": "S9-D9.1",
    "task_kind": "Doc",
    "origin": "taco-orchestrator"
  },
  "projection_hash": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"
}
```

### BoundaryViolation event

```json
{
  "id": "1746758402000000001-b3c4d5e6f7a8",
  "seq": 42,
  "action": "boundary_violation",
  "actor": { "type": "Daemon", "id": "touring-daemon" },
  "timestamp_ns": 1746758402000000001,
  "payload": {
    "task_kind": "Spec",
    "file_path": "crates/foo/src/lib.rs",
    "violation_kind": "ForbiddenWrite",
    "matched_pattern": "crates/**/src/**"
  },
  "projection_hash": "b3c4d5e6f7a8b3c4d5e6f7a8b3c4d5e6f7a8b3c4d5e6f7a8b3c4d5e6f7a8b3c4"
}
```

---

## 9. Change Log

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-09 | Initial draft (Constitution v8.0) |

---

**RFC-001 v1.0.0 — Activity Event Catalog — ESAA S1 spec formalized**