# TACO Iteration 11 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: touring-hooks 1452/1452 PASS | touring-server 330/330 PASS
**Phase**: FASE 7 — Documentation by touring-scriber

---

## Overview

Iteration 11 delivers three changes that expand the co-edit temporal signal across three additional
surfaces: the pre-edit enrichment context (EC14), the E2E health checker (EC15), and the MCP
wiring_suggest handler (EC16).

Prior to this iteration:
- `compose_edit_context` had 11 signals. Co-edit data existed in TABLE_FILE_COEDITS but was not
  injected into the pre-edit hook context that Claude Code receives before every edit.
- `touring e2e --depth standard` had no visibility into whether TABLE_FILE_COEDITS was populated.
- The MCP `wiring_suggest` handler in `server/mod.rs` returned empty results when
  TABLE_WIRING_SUGGESTIONS was empty, even though the same co-edit computation available in
  `cli_handlers.rs` (EC12) could resolve the gap.

After Iter 11, every pre-edit hook call gains co-edit neighbor awareness (EC14), E2E phase_knowledge
validates TABLE_FILE_COEDITS health (EC15), and MCP `wiring_suggest` computes suggestions
on-demand via the same bidirectional co-edit formula (EC16).

A VP-Scout false positive was avoided: scout reports of "EC14/EC15/EC16 missing" were immediately
false-positived by Architect VP-Scout Chain 3 after implementation confirmed all three were complete.

---

## EC14 — compose_edit_context Signal 12: Co-Edit Neighbors

**File**: `crates/touring-hooks/src/pre_edit.rs`

**Location**: After line ~452, before `if parts.is_empty()` in `compose_edit_context`

**Problem**: `compose_edit_context` built an 11-signal context string for pre-edit hook responses.
TABLE_FILE_COEDITS was populated by post-edit hooks but never surfaced in pre-edit enrichment.
Claude Code had no awareness of temporal coupling when deciding how to edit a file.

**Fix**: Added Signal 12 — calls `db.get_coedit_neighbors(file_path, 5)` and injects:
```
"co-edits: N file(s) frequently edited together [file1.rs, file2.rs, ...]"
```
into the pre-edit context. When no co-edit history exists, signal is silently omitted
(no empty string injected).

**Design**: Uses the same `get_coedit_neighbors()` (sync, bidirectional) method used by EC12
and EC13, maintaining consistency across all co-edit consumers.

**Impact**: Every pre-edit hook call now includes temporal coupling awareness. Claude Code can
factor co-edit history into its pre-edit analysis alongside imports, blast radius, and wiring
signals. This completes the feedback loop: post-edit writes co-edit records → pre-edit reads
and surfaces them.

**Implementation size**: ~8-line addition before the `if parts.is_empty()` guard

---

## EC15 — phase_knowledge T7: Co-Edit Table Health Check

**File**: `crates/touring-hooks/src/cli_e2e.rs`

**Location**: `phase_knowledge` function, before `let coverage`

**Problem**: `touring e2e --depth standard` had no visibility into TABLE_FILE_COEDITS health.
The co-edit signal could be silently empty (cold-start DB or unpopulated table) without any
E2E diagnostic surfacing the gap.

**Fix**: Added T7 health check:
```sql
SELECT COUNT(*) FROM file_coedits
```
- If `coedit_pairs > 0`: T7 passes — co-edit signal active
- If `coedit_pairs == 0`: T7 issues a warning — co-edit signal cold (DB empty)
- `"coedit_pairs": coedit_pairs` added to PhaseResult metrics JSON

**Output schema addition**:
```json
{
  "phase": "knowledge",
  "checks": {
    "T7": {"status": "pass|warn", "coedit_pairs": 42}
  }
}
```

**Impact**: `touring e2e --depth standard` now reports co-edit signal health. Cold-start detection
allows operators to identify when the signal is inactive before relying on pre-edit enrichment
or wiring suggestions.

---

## EC16 — MCP wiring_suggest: Compute-On-Demand Fallback

**File**: `crates/touring-server/src/server/mod.rs`

**Location**: `wiring_suggest` handler, ~line 3910

**Problem**: The MCP `touring_wiring_suggest` tool returned empty results when TABLE_WIRING_SUGGESTIONS
was empty — the same gap EC12 fixed for the CLI handler (`cli_wiring_suggest` in cli_handlers.rs).
The MCP handler lacked the compute-on-demand phase that EC12 introduced.

**Fix**: Added compute-on-demand fallback mirroring EC12's Phase 2 logic:
1. When `suggestions` vec is empty and `orphan_symbol` is non-empty
2. Query TABLE_WIRING_MAP for the orphan file path by symbol name
3. Run bidirectional file_coedits SQL (same formula as `get_coedit_neighbors`):
   ```sql
   SELECT COALESCE(fc1.count,0) + COALESCE(fc2.count,0) as total_count, ...
   FROM file_coedits fc1 ... LEFT JOIN file_coedits fc2 ...
   ORDER BY total_count DESC LIMIT 5
   ```
4. Normalize scores: `score = total_count / max_count`
5. Return ephemeral results with `"source": "computed"`

**Key constraints preserved**:
- Read-only connection: no upsert (CLI handler in `cli_handlers.rs` handles caching separately)
- `let mut suggestions` replaces `let suggestions` to allow reassignment in fallback branch
- Error handling: DB errors in fallback return empty vec rather than propagating

**Architectural distinction vs EC12**:
- EC12 (`cli_handlers.rs`): compute + cache (upserts into TABLE_WIRING_SUGGESTIONS)
- EC16 (`server/mod.rs`): compute only, ephemeral (no upsert — avoids write on read-only connection)

**Impact**: MCP callers (`mcp__touring__touring_wiring_suggest`) now receive live suggestions
when TABLE_WIRING_SUGGESTIONS is empty. Eliminates the silent empty-result gap for MCP consumers.

---

## VP-Scout False Positive Avoided

**Claim**: "EC14/EC15/EC16 missing — implement three new co-edit surfaces."

**VP-Scout Chain 3 (Already Implemented)**:
After Engineers completed EC14, EC15, EC16, the Architect's VP-Scout Chain 3 confirmed all three
changes were in place. Scout report was immediately classified as a post-implementation confirmation,
not a new gap.

**Result**: 0 false positives reached Engineers in Iter 11.

---

## Decisions Made

### Decision 1 — EC14: Silent omission when no co-edit history

**Decision**: When `get_coedit_neighbors()` returns an empty vec, Signal 12 is not injected
(no `"co-edits: 0 file(s)"` string).

**Rationale**: An empty co-edit signal adds noise without value. The pre-edit context is already
information-dense; adding a zero-count line would waste context budget without providing
actionable information to Claude Code.

**Alternative considered**: Always inject the signal with count=0 to make the field predictable —
rejected because the consumer (Claude Code prompt) benefits from shorter context when no signal exists.

### Decision 2 — EC15: Warn not fail on coedit_pairs == 0

**Decision**: T7 issues a `warn` rather than `fail` when `coedit_pairs == 0`.

**Rationale**: An empty TABLE_FILE_COEDITS is expected at cold-start and is not a system error.
It becomes populated naturally as Claude Code makes edits. A hard fail would cause E2E to report
degraded health on fresh installations even though the system is functioning correctly.

**Alternative considered**: Fail T7 to force operator attention — rejected because it would
create false alarms on valid new deployments.

### Decision 3 — EC16: Read-only connection — no upsert in MCP handler

**Decision**: EC16 computes suggestions ephemerally without caching to TABLE_WIRING_SUGGESTIONS.

**Rationale**: The MCP handler uses a read-only DB connection. Adding upsert would require
a separate read-write connection or connection pool reconfiguration. The CLI handler (EC12) already
provides caching for CLI consumers. MCP consumers are willing to pay recompute cost per invocation
given typical call frequency.

**Alternative considered**: Open a separate write connection for caching — rejected due to
connection management complexity and the low expected MCP call frequency for `wiring_suggest`.

---

## Changes Made

| File | Change | Impact |
|------|--------|--------|
| `crates/touring-hooks/src/pre_edit.rs` | Signal 12 added to `compose_edit_context`: injects co-edit neighbors via `get_coedit_neighbors(file_path, 5)` | Every pre-edit hook call now includes temporal coupling awareness |
| `crates/touring-hooks/src/cli_e2e.rs` | T7 health check added to `phase_knowledge`: `SELECT COUNT(*) FROM file_coedits` → pass/warn + `coedit_pairs` metric | `touring e2e --depth standard` now reports TABLE_FILE_COEDITS health |
| `crates/touring-server/src/server/mod.rs` | `wiring_suggest` handler gains compute-on-demand fallback using bidirectional co-edit SQL; `let suggestions` → `let mut suggestions` | MCP `touring_wiring_suggest` returns live results when cache empty |

---

## Validation Results

| Suite | Result |
|-------|--------|
| `cargo check --workspace` | exit 0 — 0 errors |
| `cargo test -p touring-hooks --lib` | 1452/1452 PASS |
| `cargo test -p touring-server --lib` | 330/330 PASS |

---

## Architectural Impact

### Before Iter 11

```
compose_edit_context (pre_edit.rs)
  → Signals 1-11 (imports, blast, wiring, etc.)
  → co-edit data exists in TABLE_FILE_COEDITS but NOT surfaced here

touring e2e --depth standard (cli_e2e.rs)
  → phase_knowledge: T1-T6 checks
  → TABLE_FILE_COEDITS health: invisible

MCP touring_wiring_suggest (server/mod.rs)
  → query TABLE_WIRING_SUGGESTIONS
  → always empty in production → return []
```

### After Iter 11

```
compose_edit_context (pre_edit.rs)
  → Signals 1-11 (unchanged)
  → Signal 12: get_coedit_neighbors(file_path, 5)
    → "co-edits: N file(s) frequently edited together [...]"
    → omitted silently when no co-edit history

touring e2e --depth standard (cli_e2e.rs)
  → phase_knowledge: T1-T6 (unchanged) + T7
    → T7: SELECT COUNT(*) FROM file_coedits
    → pass if coedit_pairs > 0, warn if 0
    → PhaseResult.metrics["coedit_pairs"] = N

MCP touring_wiring_suggest (server/mod.rs)
  → Phase 1: query TABLE_WIRING_SUGGESTIONS
    → if cached: return {suggestions, source: "cached"}
  → Phase 2 (fallback when empty):
    → find orphan file in TABLE_WIRING_MAP
    → bidirectional co-edit SQL (A→B + B→A counts)
    → normalize scores 0.0-1.0
    → return {suggestions, source: "computed"} — no upsert
```

---

## Connection to Pln2 Goals

Iter 11 advances multiple Pln2 dimensions:

**(a) Completude do sinal co-edit**: The co-edit temporal signal is now active in 5 surfaces:
1. `post_edit.rs` — writes records (EC6, Iter 6)
2. `async_knowledge.rs` — async READ method `get_coedits_from` (EC10, Iter 9)
3. `cli_handlers.rs::cli_wiring_suggest` — compute+cache (EC12, Iter 10)
4. `cli_handlers.rs::cli_ast_blast` — blast radius enrichment (EC13, Iter 10)
5. `pre_edit.rs::compose_edit_context` — pre-edit enrichment Signal 12 (EC14, Iter 11)
6. `cli_e2e.rs::phase_knowledge` — health monitoring T7 (EC15, Iter 11)
7. `server/mod.rs::wiring_suggest` — MCP compute-on-demand (EC16, Iter 11)

**(b) Observabilidade E2E**: T7 in phase_knowledge makes the co-edit signal's population status
visible in every `touring e2e --depth standard` run.

**(d) Aplicabilidade via MCP**: EC16 ensures MCP consumers (`mcp__touring__touring_wiring_suggest`)
have the same live-suggestion capability as CLI consumers (EC12).

---

## Issues Encountered

None. All three ECs implemented cleanly. VP-Scout Chain 3 confirmed no false positives reached
Engineers.

---

## Next Steps

- [ ] Monitor `coedit_pairs` metric in `touring e2e` output over time to track TABLE_FILE_COEDITS growth
- [ ] Consider adding Signal 12 co-edit context to `touring pre-write` enrichment (parallel to EC14 for Write tool)
- [ ] Consider EC17: `touring cognitive metrics` — surface co-edit signal coverage as a cognitive metric
- [ ] Consider LeidenCluster integration in wiring_suggest Phase 2 to add community-overlap scoring alongside co-edit scoring
