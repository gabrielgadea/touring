# Cross-Audit: SourceChange Transactional Fix — 2026-04-30

## Problem
The original `Applier::commit()` had a critical bug: it wrote text edits to DISK (via `path_for`) BEFORE validating filesystem operations (Phase 2). This meant:
1. Text edits reached disk even if fs_edits would fail
2. Rollback was impossible — disk writes couldn't be undone
3. Transactional guarantee was BROKEN

## Fix Applied
Rewrote `commit()` to defer ALL disk writes until AFTER all validations pass:
- **Phase 1**: Apply text edits to in-memory `files` map only (no disk writes)
- **Phase 2**: Validate ALL fs_edits (dry-run, no disk changes)
- **Phase 3**: Only if Phases 1+2 succeed → write text edits to disk + execute fs_edits
- On any failure in Phase 1 or 2: rollback in-memory `files` map, no disk writes occur
- On fs_edit failure in Phase 3: rollback text disk writes + return RolledBack

## Files Changed
- `touring-generator/src/source_change/applier.rs` — commit() transactional rewrite
- `touring-assists/tests/e2e_assist_pipeline.rs` — 5 tests fixed
- `touring-generator/tests/source_change_tests.rs` — 1 test fixed

## Test Results
- touring-assists E2E: 14/14 PASS
- touring-generator source_change_tests: 11/11 PASS
- touring-incremental-salsa: 11/11 PASS
- cargo check: OK

## Key Insight
Tests using `path_for |_| None` (no disk writes) with `RolledBack` assertion were WRONG:
- With `path_for = None` no disk writes happen
- fs_edit failure returns `Invalid` (not `RolledBack`)
- Rollback only applies to DISK writes that already happened
- Correct assertion for `path_for=None`: `ApplyResult::Invalid`
- Correct assertion for `path_for=Some(path)` with failing fs_edit: `ApplyResult::RolledBack`
