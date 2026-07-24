# RFC-003 — CRDT Semantics + Merge Protocol

**Status**: NORMATIVE (for holons declaring Profile P3)
**Version**: 1.0.0
**Date**: 2026-04-24
**Editor**: Gabriel Gadea
**Depends on**: THSF-SPEC-v1.0.0, RFC-001, RFC-002
**Reference implementation**:
`~/.claude/tools/holon/holon.py::CRDTStore` (Fase 1 baseline),
`touring-hooks::health_delta_audit` (Fase 5 Wave I)

---

## 1. Purpose

Layer 4 of THSF — Knowledge Sync — allows holons to share stateful
information (lessons, counters, rewards, audit events) without central
authority. This RFC specifies the canonical CRDT types, their wire
representation, merge semantics, and persistence format.

---

## 2. Design goals

1. **No broker** — any two holons can merge independently.
2. **Commutative merges** — order of merges doesn't matter.
3. **Grow-only primacy** — historical data is never lost.
4. **SQLite-native** — state lives in a single-file DB for auditability.
5. **Optional Automerge wrapping** — for complex nested state.

---

## 3. Standard CRDT types

### 3.1 LWW-Register (Last-Write-Wins Register)

**Purpose**: hold a single mutable value where the latest writer wins on
conflict.

**State**: `(value: T, timestamp_ms: u64, actor_id: string)`.

**Merge rule**:
```
merge(a, b):
  if a.timestamp_ms > b.timestamp_ms:      return a
  if b.timestamp_ms > a.timestamp_ms:      return b
  if a.actor_id > b.actor_id:              return a   # deterministic tie-break
  return b
```

**Wire encoding** (JSON):
```json
{
  "type": "lww-register",
  "value": "...",
  "timestamp_ms": 1714000000000,
  "actor_id": "holon-foo/session-abc123"
}
```

**SQLite schema**:
```sql
CREATE TABLE IF NOT EXISTS lww_registers (
  key          TEXT    PRIMARY KEY,
  value        BLOB    NOT NULL,
  timestamp_ms INTEGER NOT NULL,
  actor_id     TEXT    NOT NULL
);
```

### 3.2 G-Set (Grow-Only Set)

**Purpose**: accumulating observations that never need removal — audit
trails, capability-hit counters, lessons, gotchas.

**State**: `Set<Element>` where `Element` is any hashable value with a
stable serialization.

**Merge rule**:
```
merge(a, b):
  return a ∪ b      # set union
```

**Wire encoding** (JSON):
```json
{
  "type": "g-set",
  "elements": [
    {"key": "e1", "payload": "..."},
    {"key": "e2", "payload": "..."}
  ]
}
```

**SQLite schema**:
```sql
CREATE TABLE IF NOT EXISTS g_set_entries (
  set_name      TEXT    NOT NULL,
  element_key   TEXT    NOT NULL,
  payload       BLOB,
  added_by      TEXT    NOT NULL,
  added_at_ms   INTEGER NOT NULL,
  PRIMARY KEY (set_name, element_key)
);
```

Uniqueness is by `element_key`; `payload` is additional data attached to
the element. Once inserted, rows MUST NOT be updated or deleted (see §5).

### 3.3 PN-Counter (Positive-Negative Counter)

**Purpose**: distributed integer counter supporting increment and
decrement across actors.

**State**: per-actor `(increments: u64, decrements: u64)`. Value =
`sum(inc) - sum(dec)`.

**Merge rule**:
```
merge(a, b):
  for each actor_id:
    result[actor_id].inc = max(a[actor_id].inc, b[actor_id].inc)
    result[actor_id].dec = max(a[actor_id].dec, b[actor_id].dec)
```

**SQLite schema**:
```sql
CREATE TABLE IF NOT EXISTS pn_counters (
  counter_name TEXT    NOT NULL,
  actor_id     TEXT    NOT NULL,
  increments   INTEGER NOT NULL DEFAULT 0,
  decrements   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (counter_name, actor_id)
);
```

### 3.4 OR-Set (Observed-Remove Set) — v1.1+

Reserved for future RFC-003a. Supports additions and tombstone-based
removal without violating commutativity. Not required for v1.0.0
conformance.

---

## 4. Actor identity

Every write operation MUST carry an `actor_id` that uniquely identifies
the writing holon session. Recommended format:

```
<holon-name>/<session-id>
```

Examples:
- `touring-master/session-dfc952ad`
- `analise-geo-engine/session-2026-04-24T13-30-00Z`
- `konverter-portal/sess-xyz`

**Constraints**:
- MUST match `^[a-z0-9][a-z0-9_./-]{0,127}$`.
- MUST be stable within a session (a long-running daemon uses a single
  actor_id for its lifetime).
- MUST be unique across sessions of the same holon (append a timestamp,
  UUID, or monotonic counter).

**Why**: actor_id is the tie-breaker in LWW conflicts (§3.1) and enables
per-actor auditing.

---

## 5. Grow-only invariant

### 5.1 Rule

Once a row is inserted into a `g_set_entries` or an audit-style table, it
MUST NOT be UPDATEd or DELETEd by any normative operation.

### 5.2 Enforcement

SQLite triggers SHOULD enforce:

```sql
CREATE TRIGGER IF NOT EXISTS g_set_no_update
BEFORE UPDATE ON g_set_entries
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'g_set_entries: UPDATE forbidden (CRDT invariant)');
END;

CREATE TRIGGER IF NOT EXISTS g_set_no_delete
BEFORE DELETE ON g_set_entries
FOR EACH ROW
BEGIN
  SELECT RAISE(ABORT, 'g_set_entries: DELETE forbidden (CRDT invariant)');
END;
```

### 5.3 Exceptions

The following operations MAY touch grow-only data and are NOT violations:

- **VACUUM** (SQLite housekeeping, rewrites file but preserves semantics)
- **Snapshot export** for backup (read-only)
- **Schema migration** that preserves all rows (adds columns, never drops)
- **Retention policy**: if configured, a holon MAY move rows to an
  `archived_<table>` with the same schema. The archive is still append-only.

### 5.4 Auditability

Every table following the grow-only pattern SHOULD include:
- `timestamp_ms` (when added)
- `actor_id` (who added)
- `event_id` surrogate key (for external reference)

Example (Fase 5 Wave I reference impl):

```sql
CREATE TABLE IF NOT EXISTS health_delta_events (
  event_id           INTEGER PRIMARY KEY AUTOINCREMENT,
  file_path          TEXT    NOT NULL,
  old_health         REAL,
  new_health         REAL    NOT NULL,
  delta              REAL,
  outcome            TEXT    NOT NULL CHECK(outcome IN ('improvement','regression','neutral')),
  regression_streak  INTEGER NOT NULL DEFAULT 0,
  improvement_streak INTEGER NOT NULL DEFAULT 0,
  timestamp_ms       INTEGER NOT NULL,
  actor_id           TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_health_events_ts
  ON health_delta_events(timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_health_events_path_ts
  ON health_delta_events(file_path, timestamp_ms);
```

---

## 6. Merge protocol

### 6.1 Exchange

Two holons wishing to merge state:

1. Each exports its current state as a JSON envelope (§3).
2. Envelopes are exchanged via Layer 3 — typically `cli` adapter with a
   `crdt-export` / `crdt-import` capability pair.
3. Each side applies the received envelope using the merge rules for the
   type.
4. Both sides converge to the same state (mathematical guarantee from
   CRDT properties).

### 6.2 Export format

A holon's full Layer 4 state is serialized as:

```json
{
  "thsf_crdt_export": "1.0",
  "actor_id": "...",
  "generated_at_ms": 1714000000000,
  "registers": [ ... lww-register entries ... ],
  "sets":      [ ... g-set entries ...      ],
  "counters":  [ ... pn-counter state ...   ]
}
```

### 6.3 Frequency

Merge cadence is NOT normative — the spec leaves it to implementations.
The reference (`holon symbiosis --merge`) runs daily via systemd timer.

### 6.4 Failure mode

If a merge operation crashes mid-way:
- Source holon's state MUST be unchanged (atomic write to temp file, rename).
- Target holon MUST roll back the in-progress transaction.
- Exchange MUST be restartable from the last successful checkpoint.

---

## 7. Clock skew tolerance

Distributed clocks diverge. The spec requires merge correctness under
skew up to ±1 hour (C4.4 in THSF-SPEC §3.4).

### 7.1 Why 1 hour

- NTP routinely keeps drift within seconds.
- VM pausing can introduce minute-scale skew.
- Hibernation or offline sync can produce hour-scale skew.
- Anything beyond 1 hour likely indicates a broken clock and SHOULD
  trigger a `clock-skew` diagnostic.

### 7.2 Impact by type

| Type | Skew tolerance |
|---|---|
| LWW-Register | Bounded skew causes bounded staleness — tolerable |
| G-Set | Insensitive to skew (union is associative) |
| PN-Counter | Insensitive to skew (max per-actor) |

### 7.3 Recommendation

Implementations SHOULD stamp writes with the local monotonic clock, not
wall-clock, where available. Merge logic maps monotonic → wall-clock via
a per-actor offset.

---

## 8. Automerge bridge (optional)

For complex nested state (JSON documents), implementations MAY use
`automerge` as a drop-in CRDT backend:

```python
from automerge import Automerge

class AutomergeCRDTStore(CRDTStore):   # Liskov-compatible
    def __init__(self, db_path, actor_id):
        self._doc = Automerge.load(db_path) if db_path.exists() else Automerge.from({})
        self._actor_id = actor_id

    def set_lww(self, key, value):
        with self._doc.transact() as tx:
            tx[key] = value

    def merge(self, other_bytes):
        self._doc.merge(Automerge.load(other_bytes))

    def save(self, db_path):
        db_path.write_bytes(self._doc.save())
```

Automerge is an **implementation detail** — it MUST produce a state
identical (observably) to the reference SQLite-backed impl for all
standard operations. Non-standard Automerge features (e.g. rich text,
cursor tracking) are out of scope for THSF v1.0.

---

## 9. Persistence format

### 9.1 File layout

```
<holon>/.holon/
  manifest.toml
  state.db          ← SQLite database (reference impl)
  state.db-wal      ← WAL file (SQLite detail)
  state.db-shm      ← shared-memory file (SQLite detail)
  schemas/
    *.json          ← capability schemas
  archives/         ← optional, grow-only retention target
    state-<date>.db
```

### 9.2 WAL mode

SQLite-backed implementations SHOULD enable WAL mode:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
```

WAL gives better concurrency (readers don't block writers) and crash
safety (writes are fsync'd before commit).

### 9.3 Backup

Backup the three files together (`.db`, `.db-wal`, `.db-shm`) OR use
SQLite's online backup API:

```sql
-- In the holon's runtime:
VACUUM INTO '/path/to/backup/state-YYYY-MM-DD.db';
```

---

## 10. Example — full round trip

### 10.1 Actor A records a lesson

```python
store_a = CRDTStore(db_path="a/.holon/state.db", actor_id="holon-a/sess-1")
store_a.g_set_add(
    set_name="lessons",
    element_key="lesson-2026-04-24-001",
    payload={"text": "Never use --no-verify; it destroyed 162 modules in April 2026."},
)
```

### 10.2 Actor B records a different lesson

```python
store_b = CRDTStore(db_path="b/.holon/state.db", actor_id="holon-b/sess-7")
store_b.g_set_add(
    set_name="lessons",
    element_key="lesson-2026-04-24-002",
    payload={"text": "Moka cache TTL=60s matches query_cache invariants."},
)
```

### 10.3 Merge

```bash
# Export from A, import into B
holon crdt export a/.holon/state.db > /tmp/a-export.json
holon crdt import b/.holon/state.db < /tmp/a-export.json

# Now B sees both lessons. Symmetric: export B, import into A.
holon crdt export b/.holon/state.db > /tmp/b-export.json
holon crdt import a/.holon/state.db < /tmp/b-export.json
```

### 10.4 Verification

Both sides return identical output:

```bash
holon crdt list a/.holon/state.db --set lessons
# lesson-2026-04-24-001  holon-a/sess-1  "Never use ..."
# lesson-2026-04-24-002  holon-b/sess-7  "Moka cache TTL=60s ..."

holon crdt list b/.holon/state.db --set lessons
# (identical output)
```

---

## 11. Diagnostics

| Code | Meaning | Severity |
|---|---|---|
| `thsf-crdt-001` | CRDT export format invalid | error |
| `thsf-crdt-002` | Actor ID missing or malformed | error |
| `thsf-crdt-003` | Grow-only invariant violated (UPDATE/DELETE) | error |
| `thsf-crdt-004` | Clock skew > 1 hour detected | warning |
| `thsf-crdt-005` | Unknown CRDT type in envelope | error |
| `thsf-crdt-006` | Merge transaction failed — rollback | error |
| `thsf-crdt-007` | Schema migration would drop rows | error |

---

## 12. Security & privacy

### 12.1 No PII by default

Holons SHOULD NOT store personally identifiable information (PII) in
CRDT state without explicit user consent. The framework provides no
encryption at rest.

### 12.2 Redaction

If redaction is required, it MUST be implemented as a **new write** that
supersedes the old (LWW) or a **tombstone** (OR-Set in v1.1+). Actual
deletion violates the grow-only invariant.

### 12.3 Cross-host sync

In Fase 7 (libp2p) the CRDT export is transmitted over GossipSub. Messages
MUST be signed with the actor's ed25519 key. Verification is normative.

---

## 13. Version history

| Version | Date | Summary |
|---|---|---|
| 1.0.0 | 2026-04-24 | Initial (Fase 8 D8.2.c) |

---

*End of RFC-003. Next: RFC-004 (WIT Interfaces Standard).*
