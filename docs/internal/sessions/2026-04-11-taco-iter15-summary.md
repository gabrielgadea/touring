# TACO Iteration 15 — Implementation Summary

**Date**: 2026-04-11
**Status**: COMPLETE
**Cargo check**: exit 0 (0 errors)
**Tests**: touring-hooks 1/1 PASS, touring-server 0 failed
**Phase**: FASE 6+7 — Audit + Documentation by touring-auditor

---

## Overview

Iteration 15 delivers EC20 — per-file edit frequency signal added to both
`AsyncFileKnowledgeDB` (data layer) and `GraphFocusCtx` (graph context surface).

EC20 completes the read/write activity duality in `GraphFocusCtx`: after EC18
(Iter13) added `access_count` (reads), EC20 adds `edit_count` (writes). Graph
consumers now have a full activity profile for each file.

---

## EC20 — `edit_count_for_file` + `GraphFocusCtx.edit_count`

### Files modified

| File | Change |
|------|--------|
| `crates/touring-hooks/src/async_knowledge.rs` | New `pub async fn edit_count_for_file()` |
| `crates/touring-server/src/graph_service.rs` | 4 changes: struct field, Default, resolve_ctx, inject |

### async_knowledge.rs — new method

```rust
/// Edit count for a specific file (from TABLE_EDIT_HISTORY).
/// EC20: per-file edit frequency — "how hot is this file for editing" signal.
/// Exact match on file_path column (no LIKE approximation).
pub async fn edit_count_for_file(&self, file_path: &str) -> Result<i64, AsyncKnowledgeError> {
    let path = file_path.to_string();
    let conn = self.pool.get().await.map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

    conn.interact(move |conn| {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE file_path = ?1", schema_guard::TABLE_EDIT_HISTORY),
            [&path],
            |row| row.get(0),
        )
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))
    })
    .await
    .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
}
```

**Position**: line 317, immediately after `access_count()` — mirrors sibling placement.

### graph_service.rs — 4-step GraphFocusCtx field pattern

1. **Struct field** (line 54): `pub edit_count: i64,` with doc comment referencing EC20
2. **Default impl** (line 69): `edit_count: 0`
3. **resolve_ctx()** (lines 372-373): `let edit_count = adb.edit_count_for_file(file).await.unwrap_or(0)`
4. **inject()** (line 504): `"edit_count": ctx.edit_count` in JSON output

---

## Validation

| Check | Result |
|-------|--------|
| `cargo check --workspace` | exit 0, 0 errors |
| `cargo test -p touring-hooks` | 1/1 PASS |
| `cargo test -p touring-server` | 0 failed |
| `edit_count_for_file` in async_knowledge.rs | lines 317-334, verified |
| `edit_count` field in GraphFocusCtx | lines 54, 69, 372-373, 388, 504, verified |
| EC18 paridade (access_count) | identical 4-step pattern confirmed |

---

## Design Decisions

**Exact path match (`WHERE file_path = ?1` not `LIKE ?1%`)**: `resolve_ctx()` receives
the canonical absolute path already. Exact match is faster (index-friendly) and avoids
false positives from paths sharing a prefix.

**`unwrap_or(0)` not `?`**: Observational signal — missing file or empty table returns 0
rather than propagating Err into the graph context resolution path. Consistent with EC13,
EC18 precedent.

**Position after `access_count()`**: Semantic proximity — both methods are per-file
frequency queries on the knowledge DB. Keeping them adjacent makes the read/write
duality explicit in the source layout.

---

## Key Insights

**Read/write activity duality**: `GraphFocusCtx` now carries both `access_count`
(read frequency, TABLE_FILE_ACCESS_LOG) and `edit_count` (write frequency,
TABLE_EDIT_HISTORY). Hot-read files = optimization candidates; hot-edit files =
change-risk candidates.

**4-step GraphFocusCtx pattern** (codified as pattern:rust:graphfocusctx-field-template):
(1) field in struct, (2) Default::0, (3) resolve_ctx populate via
`adb.method().await.unwrap_or(0)`, (4) inject emit in JSON. Used by GS-EC11, EC18, EC20.
This is the canonical template for all future GraphFocusCtx field additions.

**`edit_count_for_file` closes the observability gap**: EC5/EC6 (Iters 7-8) wired
`record_edit()` into `post_edit` and `post_write`, causing `TABLE_EDIT_HISTORY` to
accumulate data. EC20 is the first method to expose per-file edit frequency — making
the write-side of the system self-consistent at the graph context level.

---

## Memory Stored

| Key | Type |
|-----|------|
| `audit:iter15:ec20:edit-count-graph-focus-ctx` | lesson |
| `doc:iter15:ec20:edit-count-for-file` | lesson |
| `pattern:rust:graphfocusctx-field-template` | pattern |

## RL Rewards

| Tool | Value | Context |
|------|-------|---------|
| orchestrate | 1.0 | iter15-audit: EC20 verified 0 errors |
| edit | 1.0 | iter15-scriber: PLAN+session report updated |
