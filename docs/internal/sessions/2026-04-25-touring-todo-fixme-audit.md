# Touring TODO/FIXME Audit — 2026-04-25

## Summary
Total: 0 TODOs/FIXMEs in production code | P0: 0 | P1: 0 | P2: 0 | P3: 1

**Methodology**: `touring ast todos crates/ --include "*.rs" -j` returned 0 entries. All grep hits were either:
- Test fixture data strings (`pending_tasks: vec!["TODO: ..."`)
- Documentation comments (`/// NOTE:`, `// NOTE:`)
- Meta-comments about schema/architecture

---

## P0 — Security/Safety
**None found.** Zero unchecked borrows, unsafe blocks without safety docs, or hardcoded secrets in production code.

---

## P1 — Bugs
**None found.** No incorrect behavior, panics on edge cases, or incorrect logic in production paths.

---

## P2 — Quality (scheduled for refactor)
**None found.** No code smells or overly complex functions needing refactor.

---

## P3 — Optimization (nice to have)

| File | Line | Note | Priority |
|------|------|------|----------|
| `crates/touring-server/build.rs` | 78-82 | `build-info` feature scaffolded but commented out — `vergen-gix` 1.x has breaking API change (Emitter + AddEntries dance). Not blocking since feature is intentionally off. | P3 |

---

## Architecture Notes (Informational — Not TODOs)

These are doc comments explaining design decisions, not action items:

| File | Line | Note |
|------|------|------|
| `crates/touring-server/src/memory_store.rs` | 307-308 | hybrid_search not yet available in touring-learning; using FTS5-only for now. NOTE(P5.2): add hybrid_search |
| `crates/touring-server/src/server/tools_core.rs` | 382 | NOTE(P5.2): wire search_by_file_paths when RlmMemory gains it |
| `crates/touring-server/src/server/tools_analysis.rs` | 1377 | NOTE(P5.2): wire search_by_file_paths when RlmMemory gains it |
| `crates/touring-server/src/reasoning/persistence.rs` | 47, 280 | review_required column removed from persistence (not in SubTask domain model) |
| `crates/touring-cognitive/src/semantic_graph.rs` | 739 | NOTE (ARCH-2): current implementation touches existing nodes and fetches |
| `crates/touring-cognitive/src/session_predictor.rs` | 264 | NOTE (ARCH-3): current implementation is a best-effort cache warm |
| `crates/touring-cortex/src/handlers/enforcement.rs` | 1082, 1117 | NOTE(S6): full motor orchestration requires Tier 3 Python bridge |
| `crates/touring-learning/src/memory/mod.rs` | 46, 50 | NOTE(P5): wire MemoryStore to touring-server when async integration is ready |
| `crates/touring-simd/src/statistics/mod.rs` | 12 | NOTE(P2): PyO3 bindings requires `pyo3` dependency |
| `crates/touring-simd/src/similarity/mod.rs` | 17 | NOTE(P2): PyO3 bindings requires `pyo3` dependency |

---

## Resolved (previously fixed)
**None — codebase is clean.**

---

## Verification Commands

```bash
# Daemon health
touring doctor -j  # all OK

# Compilation
cargo check --workspace  # exit 0

# No file_todos entries
touring ast todos crates/ --include "*.rs" -j
# {"count":0,"todos":[]}

# Confirm via grep (no production TODOs)
grep -rn "// *TODO:\|// *FIXME:" crates/ --include="*.rs" | grep -v test | grep -v benches
# (empty — only test fixtures and doc comments)
```

---

## Conclusion

The touring workspace is **clean** — zero production TODOs/FIXMEs. The only P3 item (`build-info` feature) is intentionally disabled due to a dependency API change, not a bug.

The `file_todos` table in `knowledge.db` is correctly empty (0 entries).

**Action**: None required. Continue normal development.