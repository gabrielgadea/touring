# Touring Assist Framework Reference

> **Assists**: Refactor-as-CLI framework with 10 high-value handlers
> **Crate**: `touring-assists` (`crates/touring-assists/`)
> **Skill version**: v4.27.0 (2026-04-30)
> **CLI**: `touring assist {list-kinds,applicable,apply}` | **MCP**: `touring_assist_apply`

---

## Overview

The `touring-assists` crate provides a refactor-as-CLI framework modeled after rust-analyzer's `ide-assists`. Ten assist handlers analyze code context, produce `SourceChange` artifacts, and can be applied atomically across multiple files.

**Architecture**:

```
AssistHandler = fn(&mut Assists, &AssistContext) -> Option<()>
Assist        = AssistId + Label + Group + Target + LazySourceChange
Assists       = accumulator with add/add_with_group/add_group methods
AssistCatalog = registry mapping AssistId → AssistHandler
```

**Key modules**:
- `src/framework/assist.rs` — `Assist`, `AssistTarget`, `LazySourceChange`
- `src/framework/assists.rs` — `Assists` accumulator
- `src/framework/context.rs` — `AssistContext` (file_id, range, content, selected_text)
- `src/framework/catalog.rs` — `AssistCatalog` registry
- `src/handlers/*.rs` — 10 concrete handlers

---

## CLI Commands

### `touring assist list-kinds`

Lists all registered assist kinds.

```
Available assist kinds:
  add_missing_match_arms
  auto_import
  auto_wire
  change_visibility
  convert_to_guarded_return
  extract_function
  generate_impl
  inline_call
  merge_imports
  move_module_to_file
```

### `touring assist applicable <file>:<line>:<col>`

Returns applicable assists for the given cursor position. Runs assist handlers against the file and returns those that match.

### `touring assist apply <kind> <file> <range>`

Applies the specified assist kind at the given file:range. Emits `SourceChange` and commits via `Applier`.

---

## The 10 Assist Handlers

### 1. `add_missing_match_arms`

For `match` expressions on enums, suggests arms for unhandled variants.

**When applicable**: cursor inside a `match` expression where some enum variants lack arms.

**SourceChange**: adds new arms for each missing variant.

**RFC-100**: `A-100`

---

### 2. `auto_import`

For unresolved symbols, finds candidate via `touring index find` and inserts `use` statement.

**When applicable**: cursor on a symbol that is not in scope but exists in the index.

**SourceChange**: inserts `use crate::path::Symbol;` at appropriate scope.

**RFC-100**: `A-101`

---

### 3. `auto_wire`

For orphan pub symbols (zero consumers), suggests insertion points based on `touring wiring suggest` output.

**When applicable**: cursor on or near an orphan pub symbol.

**SourceChange**: adds `use crate::path::orphan_symbol;` in the best consumer candidate.

**RFC-100**: `A-102`

> **Primary offensive against 199.832 orphan pub symbols.**

---

### 4. `change_visibility`

Changes visibility modifier: `pub` ↔ `pub(crate)` ↔ `pub(super)` ↔ `private`.

**When applicable**: cursor on a `pub`, `pub(crate)`, or `pub(super)` item.

**SourceChange**: replaces visibility keyword.

**RFC-100**: `A-103`

---

### 5. `convert_to_guarded_return`

Converts `if condition { body } else { return; }` to `if !condition { return; } body`.

**When applicable**: cursor on an `if-else` with `return` in the else branch.

**SourceChange**: restructures the conditional as early-return guard.

**RFC-100**: `A-104`

---

### 6. `extract_function`

Extracts a code block into a new function. Works in Rust + JS/TS via tree-sitter.

**When applicable**: cursor inside a code block with free vars that can become parameters.

**SourceChange**: creates new function + replaces original with call site.

**RFC-100**: `A-105`

---

### 7. `generate_impl`

For a type `T`, generates `impl Trait for T` skeleton with required methods.

**When applicable**: cursor on a type that implements (or should implement) a trait.

**SourceChange**: inserts `impl Trait for Type { ... }` block.

**RFC-100**: `A-106`

---

### 8. `inline_call`

Inverse of `extract_function`. Replaces call site with body, substituting params.

**When applicable**: cursor on a function call where the body is available.

**SourceChange**: substitutes the call with the function body.

**RFC-100**: `A-107`

---

### 9. `merge_imports`

Combines adjacent `use` statements with shared prefix.

**When applicable**: cursor among a group of `use` statements with common prefix.

**SourceChange**: collapses into `use crate::{A, B, C};`.

**RFC-100**: `A-108`

---

### 10. `move_module_to_file`

Converts `mod foo { ... }` to `mod foo;` + new `foo.rs` file.

**When applicable**: cursor inside a `mod` block.

**SourceChange**: uses `FileSystemEdit::CreateFile` for new file + edits original to `mod foo;`.

**RFC-100**: `A-109`

---

## Assist Context

```rust
pub struct AssistContext<'a> {
    pub file_id: FileId,
    pub file_path: &'a str,
    pub content: &'a str,
    pub range: Range<usize>,
}

impl AssistContext<'_> {
    pub fn selected_text(&self) -> &str {
        &self.content[self.range.clone()]
    }
}
```

---

## SourceChange Integration

Each handler produces a `SourceChange` (from `touring-generator::source_change`). The applier commits atomically:

```rust
let result = applier.commit(&source_change, &mut files, path_for);
assert!(matches!(result, ApplyResult::Committed { .. }));
```

See `touring-generator::source_change::Applier` for the two-phase (shadow-validate + commit) transactional applier.

---

## RFC-100 Codes

| Code | Name | Severity | Description |
|------|------|----------|-------------|
| `A-100` | `AssistApplied` | info | Assist applied successfully |
| `A-101` | `AssistRejected` | warning | Assist rejected (not applicable or ambiguous) |
| `A-102` | `AssistAmbiguous` | info | Multiple applicable assists — user choice required |

Per-handler codes: `A-100` through `A-109` (one per handler). Counter `assist_apply_count` tracks usage.

---

## Telemetry Counters

```
assist_apply_count         — total assists applied
assist_rejection_count     — assists that were not applicable
assist_<handler>_count     — per-handler apply count
```

Exposed via `touring gate-metrics -j`.

---

## Framework Architecture

```
src/
├── lib.rs                     — crate root, re-exports
├── framework/
│   ├── mod.rs
│   ├── assist.rs             — Assist, AssistTarget, LazySourceChange
│   ├── assists.rs            — Assists accumulator
│   ├── catalog.rs            — AssistCatalog registry
│   └── context.rs            — AssistContext
└── handlers/
    ├── mod.rs
    ├── add_missing_match_arms.rs
    ├── auto_import.rs
    ├── auto_wire.rs
    ├── change_visibility.rs
    ├── convert_to_guarded_return.rs
    ├── extract_function.rs
    ├── generate_impl.rs
    ├── inline_call.rs
    ├── merge_imports.rs
    └── move_module_to_file.rs
```

**Tests**: `crates/touring-assists/tests/e2e_assist_pipeline.rs` (14 E2E tests, all PASS)

---

## Reference Map

- Assists framework: `references/touring-cli-assists.md` (this file)
- SourceChange transactional: `touring-generator/src/source_change/applier.rs`
- SkipContext (region markers): `references/touring-cli-intelligence.md`
- CharClasses (multi-lang): `references/integrations.md`
- MCP tools: `references/mcp_tools.md`
