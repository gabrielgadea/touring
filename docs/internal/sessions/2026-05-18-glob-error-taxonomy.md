# Glob Error Taxonomy — P8.6 Root-Cause Analysis

> **Wave**: CEG Pln2 FASE P8 | **Date**: 2026-05-18 | **Module**: `crates/touring-hooks/src/workflow/glob_diag.rs`
> **Data source**: 3,058 CC sessions, 1,645 Glob tool calls, 431 errors (26.2% error rate)

## Summary

Glob tool calls exhibit a **26.2% error rate** (431 out of 1,645 calls) across 3,058
forensic sessions. This is the highest error rate of any elite tool in the CEG P8 baseline.
The `glob_diag` module (P8.6) implements a pre-call validator that classifies patterns into
five root-cause categories and emits targeted `GlobValidationResult::Likely` or
`GlobValidationResult::Warn` hints before the call is issued.

## Error Rate vs. Baseline

| Tool | Calls | Errors | Error Rate |
|---|---|---|---|
| Glob | 1,645 | 431 | **26.2%** |
| BashFind (anti-pattern) | 3,494 | — | — |
| BashGrepRaw (anti-pattern) | 35,975 | — | — |

## Root-Cause Categories

### Category 1 — InvalidSyntax (~30% of errors, ~129 calls)

**Description**: Malformed glob pattern — unbalanced brackets `[`, braces `{`, or
empty alternatives `{}`.

**Examples**:
- `src/[lib.rs` — unclosed bracket
- `crates/{foo,bar` — unclosed brace
- `**/*.{rs,}` — empty alternative after trailing comma

**Prevention**: Validate bracket/brace depth before issuing the call. The `glob_diag`
validator checks depth via a character-by-character scan with escape handling.

**Hint emitted**: `Warn` — pattern will fail at runtime. Fix before calling.

### Category 2 — NonexistentBase (~25% of errors, ~108 calls)

**Description**: The literal path prefix before the first glob metacharacter (`*`, `?`,
`[`, `{`) does not exist on disk.

**Examples**:
- `crates/nonexistent-crate/src/**/*.rs` — `crates/nonexistent-crate/` does not exist
- `packages/frontend/src/**` — wrong directory name

**Prevention**: Extract the base path (up to the first metacharacter), then check
`Path::exists()` before calling.

**Hint emitted**: `Warn` — base directory does not exist.

### Category 3 — PatternTooBroad (~20% of errors, ~86 calls)

**Description**: Pattern is so broad it matches thousands of files, causing timeouts or
memory pressure. Typically `*` or `**` as the final path component with no extension filter.

**Examples**:
- `**/*` — every file in the workspace
- `crates/**` — all files under crates/ (including target/)
- `src/*` — all files under src/ without extension filter

**Note**: `**/*.rs` is NOT too broad (has extension filter). `**/{lib,main}.rs` is NOT
too broad (has explicit names).

**Prevention**: Check if the final path component is bare `*` or `**` with no `.ext`
or `{name}` qualifier.

**Hint emitted**: `Likely` — may time out or return excessive results.

### Category 4 — NoMatchTreatedAsError (~15% of errors, ~65 calls)

**Description**: The glob pattern is syntactically valid and the base exists, but the
caller fails when `0` files are returned, treating no-match as an error instead of an
empty result.

**Examples**:
- `src/**/*.test.rs` — valid pattern, but test files use `_test.rs` suffix
- `crates/*/tests/*.rs` — integration tests are under `tests/` at workspace root

**Prevention**: This is a caller-side contract issue. The `glob_diag` validator cannot
detect this at pattern-analysis time; it requires knowledge of the expected result count.
Document the expected result and add a zero-match guard in the caller.

**Hint emitted**: None from `glob_diag` (cannot detect statically).

### Category 5 — RelativePathAmbiguity (~10% of errors, ~43 calls)

**Description**: Pattern starts with `./` or `../`, which resolves relative to the
current working directory. In Claude Code hooks, the CWD is not always the workspace
root, causing patterns to resolve against unexpected directories.

**Examples**:
- `./src/**/*.rs` — resolves from CWD, not workspace root
- `../sibling-crate/src/*.rs` — relative traversal is fragile

**Prevention**: Always anchor glob patterns to workspace root (absolute path or
workspace-relative without `./` prefix). The `glob_diag` validator detects `../` and
`./` prefixes.

**Hint emitted**: `Likely` — relative path may resolve incorrectly.

## Category Share Distribution

```
InvalidSyntax          ████████████ 30%
NonexistentBase        ██████████   25%
PatternTooBroad        ████████     20%
NoMatchTreatedAsError  ██████       15%
RelativePathAmbiguity  ████         10%
                                   ─────
                                   100%
```

## P8.6 Validator — validate_glob_pattern()

The validator is implemented in `crates/touring-hooks/src/workflow/glob_diag.rs` and
is surfaced pre-call via X7 DECISION hints in the CEG pipeline.

```rust
pub fn validate_glob_pattern(
    pattern: &str,
    workspace_root: Option<&std::path::Path>,
) -> GlobValidationResult
```

**Return variants**:
- `GlobValidationResult::Ok` — pattern passes all checks, safe to call
- `GlobValidationResult::Likely { category, hint }` — probable issue, advisory warning
- `GlobValidationResult::Warn { category, hint }` — definite issue, strong warning

**Check order** (fail-fast):
1. `InvalidSyntax` — checked first (cheapest; pure string scan)
2. `RelativePathAmbiguity` — string prefix check
3. `NonexistentBase` — filesystem check (requires `workspace_root`)
4. `PatternTooBroad` — structural analysis of final component

## Integration — X7 DECISION (CEG Pln2)

The validator is called from the X7 DECISION path in `crate::gateway::decision` when
a Glob tool call is detected. The `GlobValidationResult` is converted to a
`GateDecision::canonical_fix` hint if `category` severity warrants surfacing.

Severity precedence (deny-wins): hard `Deny` from X2/X6 always takes precedence over
`GlobValidationResult::Warn`.

P8.7 (RL reward injection for correct Glob usage) is delivered in a subsequent wave.

## Taxonomy Struct

```rust
pub struct GlobErrorTaxonomy {
    pub total_calls: u32,   // 1645
    pub total_errors: u32,  // 431 (26.2% error rate)
    pub categories: Vec<GlobErrorEntry>,
}

pub struct GlobErrorEntry {
    pub category: GlobErrorCategory,
    pub estimated_share_pct: u8,   // of total errors
    pub description: &'static str,
    pub prevention: &'static str,
}
```

Access the process-global baseline via `GlobErrorTaxonomy::baseline()` (lazy-initialized
via `OnceLock`).

## Test Coverage

| Test | What it verifies |
|---|---|
| `baseline_total_calls` | `total_calls == 1645` |
| `baseline_error_rate_approx_26_pct` | error rate within ±2% of 26.2% |
| `all_five_categories_present` | all `GlobErrorCategory` variants in taxonomy |
| `category_shares_sum_to_100` | shares add up to 100% |
| `invalid_syntax_detected` | `{` + `[` detection |
| `relative_path_detected` | `../` and `./` prefix detection |
| `nonexistent_base_detected` | non-existent path base |
| `pattern_too_broad` | bare `*` / `**` final component |
| `clean_patterns_pass` | valid patterns return `Ok` |
| `hint_contains_category_label` | hint string contains category label |
| `label_uniqueness` | all 5 category labels are unique |
| `taxonomy_serde_roundtrip` | JSON serialize → deserialize roundtrip |

---

_Generated by TACO Engineer — CEG Pln2 FASE P8 Wave 2026-05-18_
