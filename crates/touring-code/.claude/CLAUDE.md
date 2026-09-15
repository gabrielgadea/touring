---
okf_version: "1.0"
type: CLAUDE
title: "touring-code — Unified code intelligence + Code-mode SDK surface"
description: "Crates that fuzed touring-ast + polyglot + language + semantics (W4) plus the code-mode-sinal F1-F6 SDK surface (journal.rs + sdk.rs + sdk_signal_mirror.rs). Auto-loaded by CC when sessions open files under this crate."
plan_id: docs
tags: [rules, crate, code-intelligence, code-mode, sdk]
timestamp: 2026-09-01T11:50:00-03:00
---

# touring-code — Crate Instructions

## What this crate does

Two responsibilities in one namespace:

1. **Unified code intelligence** (W4.2–W4.5): fuses 4 historically-separate
   crates under one tree:
   - `ast` (ex-touring-ast) — tree-sitter parsing + symbol store + surgery + call graph
   - `polyglot` (ex-touring-ast-polyglot) — ast-grep structural search
   - `languages` (ex-touring-language) — `Lang` enum + tier matrix
   - `semantics` (ex-touring-semantics) — Definition resolver + source-to-def

2. **Code-mode-sinal F1-F6 SDK surface** (01/09/2026): hybrid mirror+junction
   that materialises the in-sandbox SDK into a `SignalReport` consumable by
   the daemon and `touring kpi`.

## Modules (post code-mode-sinal)

| Module | Origin | Purpose |
|--------|--------|---------|
| `ast` | touring-ast W4.2 | tree-sitter parsing + symbol store + surgery + call graph |
| `polyglot` | touring-ast-polyglot W4.3 | ast-grep structural search |
| `languages` | touring-language W4.4 | `Lang` enum + tier matrix |
| `semantics` | touring-semantics W4.5 | Definition resolver + source-to-def |
| `journal` | code-mode-sinal F2 (NEW) | Parse `~/.claude/touring/run_journal.jsonl` into aggregate stats |
| `sdk` | code-mode-sinal F2 (NEW) | 8 hardcoded hook names + `SignalReport` JSON shape |
| `sdk_signal_mirror` | code-mode-sinal F3 (NEW) | Append-only JSONL sink written by PostToolUse |
| `types` | touring-types | shared type aliases |
| `error` | touring-errors | `Result<T>` + `Error` re-export |

## Code-mode-sinal F1-F6 (DAG `task_1788196388043002698`)

### F2 — S4 hybrid SDK surface (`sdk.rs` + `journal.rs`)
8 hooks hardcoded in `HookName::ALL` (names frozen by strategy v1.1 §D3):
`ast_meta`, `ast_blast`, `index_find`, `wiring_orphans`, `memory_recall`,
`pre_edit`, `tantivy_search`, `parallel`. Types are derived from the journal
via `signal_report_from_journal(path)` — fail-soft if the journal is
absent (returns `default` aggregate).

Public surface:
- `pub fn signal_report_from_journal(path: &Path) -> Result<SignalReport, SdkError>`
- `pub fn load_signal_report(path: &Path) -> Result<SignalReport, SdkError>`
- `pub fn parse_hook(s: &str) -> Result<HookName, SdkError>`
- `pub fn record_hook_call(mirror_path, hook, duration_ms, success)`
- `pub fn classify_bash_command(command: &str) -> Option<HookName>` — wave
  PostToolUse-wiring P3 (01/09): maps a real bash command line (`touring ast
  meta …`, env-prefix and binary-path tolerated) to its canonical hook.
  Precision-first: any ambiguity returns `None` (a false positive would
  inflate `signal_use`). Consumed by the `post_bash` handler in
  `touring-hook-handlers` to feed the mirror from CLI traffic.
- (`SdkValue`, an alias of `serde_json::Value` that no function returned, was
  removed in the cross-audit R2 orphan census, 15/09/2026.)

Tests: 8 (journal) + 5 (sdk) + 6 (mirror) = **19 unit tests**.

### F3 — PostToolUse sync (`sdk_signal_mirror.rs`)
Append-only JSONL sink at `~/.claude/touring/sdk_signal_mirror.jsonl`.
Crash-safe (one torn line is recoverable), fail-open at the source
(missing file → empty aggregate), and does NOT clobber base-report
entries (`augment_with_mirror` skips hooks with `call_count > 0`).

Public surface:
- `pub fn record(path, HookName, duration_ms, success) -> Result<PathBuf, MirrorError>`
- `pub fn read(path) -> Result<MirrorAggregate, MirrorError>`
- `pub fn augment_with_mirror(&mut SignalReport, &MirrorAggregate)`
- `pub fn signal_report_from_journal_and_mirror(journal, mirror) -> Result<SignalReport, Box<dyn Error>>`

### Examples (use `cargo run --example <name>`)
- `sdk_smoke` — emit Rust-only `SignalReport` and diff against `gen_sdk.py`
- `sdk_posttool_end_to_end` — record 9 calls, read back, build augmented report
- (kpi examples live in `touring-cli/examples/`, not here)

## How to run tests

```bash
cargo build -p touring-code                        # Build (release for kpi examples)
cargo test -p touring-code --lib --tests           # 19 unit tests
cargo run -p touring-code --example sdk_smoke          # cross-emitter sanity
cargo run -p touring-code --example sdk_posttool_end_to_end  # full round-trip
```

## Migration (for older callers)

```text
use touring_ast::X         → use touring_code::ast::X
use touring_ast_polyglot::X → use touring_code::polyglot::X
use touring_language::X     → use touring_code::languages::X
use touring_semantics::X    → use touring_code::semantics::X
```

The 3 NEW modules (`journal`, `sdk`, `sdk_signal_mirror`) have no
upstream — they were created in code-mode-sinal F1-F6 (01/09/2026) and
have NO migration path. New callers should import them directly.

## Critical invariants

- **Both modes co-exist**: the `ast/polyglot/languages/semantics` modules
  are unchanged from W4; the new `journal/sdk/sdk_signal_mirror` modules
  are additive. A diff against `main` shows 3 new files, ~750 LOC.
- **No daemon coupling**: `touring-code` is a pure library with zero
  IPC. The daemon lives in `touring-server` and embeds this crate.
- **JSONL invariants**: every line in `sdk_signal_mirror.jsonl` MUST
  parse as `HookCallEntry` (malformed lines are skipped, not abort).

## Lint hygiene (matches workspace-wide RBP-01)

```bash
cargo clippy -p touring-code --all-targets -- -D warnings   # 0 warnings
```

`#![cfg_attr(not(test), deny(missing_docs))]` enforces doc comments on
all public items — measured 0 violations across the 3 new modules.

## See also

- Bundle code-mode-sinal F1-F6: `docs/plans/2026-08-31-code-mode-sinal/`
- Strategy-doc D3 (hybrid SDK): `docs/plans/2026-08-31-code-mode-sinal/strategy-2026-08-31-yetzirah-v1.1.md`
- Daemon caller (re-embeds this crate): `crates/touring-server/src/cli/run.rs` (record_hook_call + query() wrap)
- Python emitter (CI without Rust toolchain): `scripts/gen_sdk.py`