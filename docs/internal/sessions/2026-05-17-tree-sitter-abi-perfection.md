# Tree-sitter ABI Perfection — Research + Migration Plan

> **Date**: 2026-05-17 | **Author**: TACO Architect subagent
> **Scope**: Make every tree-sitter grammar in the touring workspace ABI-consistent so
> ast-grep can AST-parse ALL languages without panic or fallback.
> **Status**: Research + plan only — no code edits this document.
> **Sources**: workspace grep evidence (file:line cited), prior CEG research docs
> (`2026-05-17-ceg-best-practices.md` §ast-grep, `2026-05-17-ceg-deps-audit.md` §3),
> context7 (used via CEG research session), Cargo.lock resolved versions.

---

## 1. Root Cause Analysis

### 1.1 The ABI split — confirmed by Cargo.lock

The workspace carries **two parallel trees** of tree-sitter grammar crates in Cargo.lock:

| Grammar | ast-grep-language 0.36 pins (ABI 14) | Touring direct pins (ABI 14 or 15) |
|---------|---------------------------------------|------------------------------------|
| `tree-sitter-bash` | **0.23.3** (ABI 14) | **0.25.1** (ABI 15) |
| `tree-sitter-css` | **0.23.2** (ABI 14) | **0.25.0** (ABI 15) |
| `tree-sitter-go` | **0.23.4** (ABI 14) | **0.25.0** (ABI 15) |
| `tree-sitter-json` | **0.23.0** (ABI 14) | **0.24.8** (ABI 14/15 transition) |
| `tree-sitter-python` | 0.23.x (ABI 14) | 0.23.x (same) |
| `tree-sitter-rust` | 0.23.x (ABI 14) | 0.23.x (same) |
| `tree-sitter-typescript` | 0.23.x (ABI 14) | 0.23.x (same) |
| `tree-sitter-javascript` | 0.23.x (ABI 14) | 0.23.x (same) |
| `tree-sitter-html` | 0.23.x (ABI 14) | 0.23.x (same) |
| `tree-sitter-toml-ng` | n/a | 0.7.x (workspace) |
| `tree-sitter-yaml` | 0.23.x (ABI 14) | 0.7.x (workspace) |
| `tree-sitter-java` | 0.23.x (ABI 14) | 0.23.x (same) |
| `tree-sitter-md` | n/a | 0.5.x (workspace) |

**Cargo.toml workspace declarations** (lines 247-258, 486-487):
```
tree-sitter = "0.24"           # line 247 — ABI-14 runtime
tree-sitter-bash = "0.25"      # line 255 — ABI-15 grammar  ← MISMATCH
tree-sitter-css = "0.25"       # line 253 — ABI-15 grammar  ← MISMATCH
tree-sitter-go = "0.25"        # line 486 — ABI-15 grammar  ← MISMATCH
```

**ast-grep-language 0.36** (Cargo.lock lines 530-559) bundles its own ABI-14 copies:
`tree-sitter-bash 0.23.3`, `tree-sitter-go 0.23.4`, `tree-sitter-css 0.23.2`.

The touring runtime (`tree-sitter = "0.24"`) has its ABI-14 runtime. When
`touring-code` or `touring-hooks` hands an ABI-15 grammar object (from the 0.25 crates)
to the ast-grep APIs, ast-grep's internal `tree_sitter::Language` version check fires:
`LANGUAGE_VERSION (15) > MAX_VERSION (14)` → **panic** or "Unsupported ABI" error.

### 1.2 The panic sites (confirmed by memory B-FUZZ-001 / B-FUZZ-002)

- **Bash**: `tree-sitter-bash 0.25.1` (ABI 15) handed to ast-grep 0.36 which expects
  ABI ≤ 14. Manifests as "Unsupported tree-sitter ABI for bash: Incompatible language
  version 15" in hook noise.
- **Go**: `tree-sitter-go 0.25.0` (ABI 15) → `.expect()` in ast-grep's `node.rs:73`
  panics under ABI mismatch. B-FUZZ-002 classifies this as a **production bug** (Go
  polyglot broken in release mode). The bash validator in `bash_ast_validator.rs` already
  works around the Bash ABI issue by falling back to tokenizer-based analysis
  (comment at line 12: "avoids the ast-grep-language 0.36 / tree-sitter-language v15
  ABI mismatch that prevents direct AST parsing today").

### 1.3 Why this is the root cause (not a symptom)

Two options exist to resolve the split:

**Option A — Downgrade touring direct pins to ABI 14**: change `tree-sitter-bash = "0.25"` →
`"0.23"`, `tree-sitter-css = "0.25"` → `"0.23"`, `tree-sitter-go = "0.25"` → `"0.23"`.
This unifies at ABI 14 without touching ast-grep. Simple, low-risk. But keeps the
workspace on an older grammar set and does not eliminate the duplicate crates from
Cargo.lock. The ABI-14 grammars for bash, css, and go are older parsers with known
grammar bugs.

**Option B — Upgrade ast-grep to 0.42.x + all grammars to ABI 15**: bump
`ast-grep-core` and `ast-grep-language` to 0.42.x (which bundles tree-sitter v0.26.7,
ABI 15), then re-pin all touring grammar crates to ABI-15-compatible versions. This
eliminates the duplicate tree completely, upgrades to modern grammars, and future-proofs
the workspace. Requires API migration (see §4).

**Recommendation: Option B** — justified in §2.

---

## 2. Target Version Matrix

### 2.1 ABI compatibility table (tree-sitter versioning rules)

The tree-sitter runtime is **backward-compatible** (newer runtime loads older grammars)
but **not forward-compatible** (older runtime CANNOT load newer-ABI grammars). ABI
versions:

| tree-sitter runtime | Grammar ABI it generates | Max grammar ABI it loads |
|--------------------|--------------------------|--------------------------|
| 0.20.x             | 13                       | 13                       |
| 0.22.x             | 14                       | 14                       |
| 0.24.x             | 14                       | 14 (confirms mismatch)   |
| 0.25.x             | 15                       | 15 (+ loads 13/14)       |
| 0.26.x             | 15                       | 15 (+ loads 13/14)       |

`TREE_SITTER_LANGUAGE_VERSION` (grammar ABI) and `MIN_COMPATIBLE_LANGUAGE_VERSION` are
constants in the generated C grammar. A runtime checks: if grammar ABI > runtime's
`MAX_COMPATIBLE_VERSION` → runtime panics. If grammar ABI < runtime's
`MIN_COMPATIBLE_LANGUAGE_VERSION` → reject (very old grammars only).

### 2.2 ast-grep version ↔ tree-sitter ABI matrix

| ast-grep-core | tree-sitter runtime bundled | Grammar ABI expected | Notes |
|---------------|-----------------------------|----------------------|-------|
| 0.36.0        | 0.22.x / 0.24.x (ABI 14)   | ≤ 14                 | **current** |
| 0.38.x        | 0.25.x (ABI 15)             | ≤ 15                 | `LanguageExt` split, `StrDoc` moved |
| 0.39.x        | 0.25.x (ABI 15)             | ≤ 15                 | incremental fixes |
| 0.42.2        | 0.26.7 (ABI 15)             | ≤ 15                 | **recommended target** |

Source: CEG best-practices doc §ast-grep (2026-05-17), ast-grep CHANGELOG, tree-sitter
release notes.

### 2.3 Recommended target crate matrix (single ABI 15 plane)

| Crate | Current workspace pin | Target pin | Notes |
|-------|-----------------------|-----------|-------|
| `ast-grep-core` | `=0.36.0` | `=0.42.2` | Latest stable as of 2026-05-17 |
| `ast-grep-language` | `=0.36.0` | `=0.42.2` | Must move in lockstep |
| `tree-sitter` | `0.24` | `0.26` | Runtime upgrade to ABI-15-capable |
| `tree-sitter-bash` | `0.25` | `0.25` | Already ABI-15; keep exact |
| `tree-sitter-css` | `0.25` | `0.25` | Already ABI-15; keep exact |
| `tree-sitter-go` | `0.25` | `0.25` | Already ABI-15; keep exact |
| `tree-sitter-json` | `0.24` | `0.24` | Verify ABI at 0.24.8; likely ABI 15 |
| `tree-sitter-python` | `0.23` | `0.25` | Upgrade to ABI-15 release |
| `tree-sitter-rust` | `0.23` | `0.25` | Upgrade to ABI-15 release |
| `tree-sitter-typescript` | `0.23` | `0.25` | Upgrade to ABI-15 release |
| `tree-sitter-javascript` | `0.23` | `0.25` | Upgrade to ABI-15 release |
| `tree-sitter-html` | `0.23` | `0.25` | Upgrade to ABI-15 release |
| `tree-sitter-java` | `0.23` | `0.25` | Upgrade to ABI-15 release |
| `tree-sitter-toml-ng` | `0.7` | `0.7` | Verify ABI; update if needed |
| `tree-sitter-yaml` | `0.7` | `0.7` | Verify ABI; update if needed |
| `tree-sitter-md` | `0.5` | `0.5` | Verify ABI; update if needed |

After this bump, Cargo.lock will have **no duplicate grammar crates** — ast-grep-language
0.42 will resolve to the same ABI-15 grammar versions that touring-code already pins.

**Pre-bump verification step**: for any grammar crate where the ABI at the target version
is uncertain, run:
```bash
cargo metadata --format-version 1 | jq '.packages[] | select(.name == "tree-sitter-yaml") | .version'
# Then inspect the generated parser.c in that version's source for LANGUAGE_VERSION
```
Or simply attempt `cargo check` and look for ABI panic at runtime via the fuzz suite.

---

## 3. Workspace Map — All Call Sites

### 3.1 Direct ast-grep-core/language imports (file:line — verified by grep)

Files that import directly from `ast_grep_core` or `ast_grep_language`:

| File | Import | Impact of 0.42 migration |
|------|--------|--------------------------|
| `crates/touring-code/src/polyglot/search.rs:1` | `use ast_grep_core::{AstGrep, NodeMatch, Pattern, StrDoc}` | HIGH — 4 symbols change |
| `crates/touring-code/src/polyglot/search.rs:2` | `use ast_grep_language::SupportLang` | MED — SupportLang stable |
| `crates/touring-code/src/polyglot/rewrite.rs:1` | `use ast_grep_core::{AstGrep, Pattern}` | HIGH — AstGrep alias + generate() |
| `crates/touring-code/src/polyglot/lang.rs:1` | `use ast_grep_language::SupportLang` | MED — SupportLang stable |

### 3.2 Indirect consumers (via `touring_ast_polyglot` facade)

Files that use the facade and are shielded from direct ast-grep API changes (they only
call `search()`, `rewrite()`, `detect_lang()`, `Lang`):

| File | Import |
|------|--------|
| `crates/touring-hooks/src/shared/ast_grep_signal.rs:32` | `use touring_ast_polyglot::{search, Lang}` |
| `crates/touring-hooks/src/shared/risk_patterns.rs:16` | `use touring_ast_polyglot::Lang` |
| `crates/touring-hooks/src/shared/forbidden_patterns.rs:29` | `use touring_ast_polyglot::Lang` |
| `crates/touring-hooks/src/cli_handlers_polyglot.rs:33` | `use touring_ast_polyglot::{...}` |
| `crates/touring-server/src/server/tools_metadata.rs:396` | `use touring_ast_polyglot::{...}` |
| `crates/touring-generator/src/validate/polyglot.rs:14` | `use touring_ast_polyglot::{detect_lang, search, Lang}` |
| `crates/touring-ast-polyglot/tests/polyglot_e2e.rs:8` | `use touring_ast_polyglot::{...}` |

**These files do NOT need changes** — the `touring_ast_polyglot` facade absorbs the
migration. Only the 4 files in §3.1 plus the facade itself need updating.

### 3.3 Non-ast-grep tree-sitter consumers (direct grammar crate users)

`crates/touring-code` directly depends on all grammar crates (`Cargo.toml` lines 22-34)
via the tree-sitter parsing stack (W4.2 — `src/ast/` module). These are used for the
touring AST analysis path (NOT for ast-grep structural matching). The `src/ast/languages.rs`
`Lang` enum (distinct from the polyglot `Lang`) drives grammar selection for this path.

These consumers benefit from the ABI-15 grammar upgrades in terms of parser quality but
do not have ABI mismatch panics today (they go through `tree-sitter = "0.24"` which is
ABI-14-safe for the ABI-14 grammars it currently loads, and the ABI-15 grammars are not
handed to ast-grep in this path — they go to the native tree-sitter parser which at 0.24
still panics on ABI 15). Upgrading `tree-sitter` to `0.26` in this path is part of the
same atomic bump.

---

## 4. API Migration Checklist (0.36 → 0.42)

All breaking changes land in the **0.38 "decouple ast-grep from tree-sitter"** refactor.
Source: CEG best-practices doc §ast-grep, ast-grep 0.38 blog post, CHANGELOG.

### Break 1 — `AstGrep` is now an alias for `Root`

**Affects**: `search.rs:45` (`AstGrep::new`), `rewrite.rs:28` (`AstGrep::new`).

In 0.42, `AstGrep<L>` is a type alias for `Root<StrDoc<L>>`. The constructors and
methods are the same — this is largely a rename at the type level. The `use` import
`ast_grep_core::AstGrep` continues to work via the alias. No source change strictly
required unless the code references the type by name in positions where the alias is
not transparent (e.g. explicit generic bounds).

**Action**: verify `cargo check` passes without changes first; if not, replace
`AstGrep` with `Root` and `use ast_grep_core::Root`.

### Break 2 — `StrDoc` relocated to `ast_grep_core::tree_sitter`

**Affects**: `search.rs:1` (`use ast_grep_core::{..., StrDoc}`), `search.rs:63`
(`NodeMatch<'_, StrDoc<SupportLang>>`).

In 0.42, `StrDoc` is in `ast_grep_core::tree_sitter::StrDoc`, not `ast_grep_core::StrDoc`.

**Action** in `search.rs`:
```rust
// BEFORE (0.36):
use ast_grep_core::{AstGrep, NodeMatch, Pattern, StrDoc};

// AFTER (0.42):
use ast_grep_core::{AstGrep, NodeMatch, Pattern};
use ast_grep_core::tree_sitter::StrDoc;
```
And the `to_match` signature:
```rust
// BEFORE:
fn to_match(m: &NodeMatch<'_, StrDoc<SupportLang>>, names: &[String]) -> Match {

// AFTER: same — StrDoc is now just imported from a different path
```

### Break 3 — `LanguageExt` trait split

**Affects**: `lang.rs:39` (`as_ast_grep` returning `SupportLang`).

Methods for tree-sitter-specific language operations (e.g. `get_ts_language()`) were
moved off the `Language` trait into a new `LanguageExt` trait in `ast_grep_core::tree_sitter`.
If any call site calls tree-sitter-specific methods on `SupportLang`, it must add
`use ast_grep_core::tree_sitter::LanguageExt`.

The touring code only uses `SupportLang` as a type tag (passed to `AstGrep::new`
and `Pattern::try_new`) — it does not call tree-sitter-specific methods directly.
**Action**: likely no change needed; verify by checking if `cargo check` reports
missing `LanguageExt` import.

### Break 4 — Language generic bound removed from matcher APIs (0.38)

**Affects**: any code that was generic over `L: Language` in matcher/pattern positions.

Touring code is not generic over `L` — it uses the concrete `SupportLang` throughout.
**Action**: no change expected; verify by `cargo check`.

### Break 5 — `Pattern::try_new` signature

The fallible constructor `Pattern::try_new(pattern, lang)` is the same signature in
0.42. `Pattern::new` (panicking) also remains. No change needed — touring already uses
`try_new` (W11.6 fix applied).

**Action**: none.

### Summary — files requiring source edits

| File | Change required | Effort |
|------|----------------|--------|
| `crates/touring-code/src/polyglot/search.rs` | Rewrite `use` line (Break 2: StrDoc path) | S (1 line) |
| `crates/touring-code/src/polyglot/rewrite.rs` | Verify `AstGrep` alias transparency (Break 1) | S (compile-check) |
| `crates/touring-code/src/polyglot/lang.rs` | Verify `LanguageExt` not needed (Break 3) | S (compile-check) |
| `crates/touring-ast-polyglot/src/lib.rs` | Re-export check after breaks applied | S (compile-check) |

**Total direct ast-grep API break sites: 1 definite edit (search.rs:1), 3 compile-verify**.
The indirect consumers (7 files) need zero changes.

---

## 5. Migration Plan — Ordered Steps

### Step S-1: Snapshot (pre-condition)
```bash
# REGRA #11 — no git. Snapshot state via Touring memory.
touring memory store "ast-grep-abi-migration:pre-bump:$(date +%s)" \
  "Pre-bump state: ast-grep-core=0.36.0, tree-sitter=0.24, bash=0.25.1(ABI15), go=0.25.0(ABI15)" \
  --tier semantic
```

### Step S-2: Verify current test baseline
```bash
cd /home/gabrielgadea/.claude/rust
cargo test -p touring-code 2>&1 | tail -5
cargo test -p touring-hooks 2>&1 | tail -5
cargo +nightly fuzz build 2>&1 | tail -5   # W11.6 fuzz targets
```
Record counts. These are the regression gate.

### Step S-3: Grammar ABI pre-check for toml-ng / yaml / md
```bash
# For any grammar crate where ABI at target version is uncertain:
# Check the LANGUAGE_VERSION constant in the published source
cargo metadata --format-version 1 | \
  jq '.packages[] | select(.name | startswith("tree-sitter-")) | {name, version}'
# Cross-reference with tree-sitter ABI table in §2.1
```
If `tree-sitter-toml-ng 0.7`, `tree-sitter-yaml 0.7`, `tree-sitter-md 0.5` are ABI 14,
they need upgrading before S-4. If already ABI 15, proceed.

### Step S-4: ATOMIC multi-crate version bump in workspace Cargo.toml
Edit the `[workspace.dependencies]` section — all in one commit:

```toml
# Group 1: ast-grep stack
ast-grep-core = "=0.42.2"
ast-grep-language = "=0.42.2"

# Group 2: tree-sitter runtime
tree-sitter = { version = "0.26", features = ["wasm"] }

# Group 3: grammar crates — all ABI-15 releases
tree-sitter-python = "0.25"
tree-sitter-rust = "0.25"
tree-sitter-typescript = "0.25"
tree-sitter-javascript = "0.25"
tree-sitter-html = "0.25"
tree-sitter-css = "0.25"       # already 0.25, keep
tree-sitter-json = "0.24"      # 0.24.8 is ABI 15; verify
tree-sitter-bash = "0.25"      # already 0.25, keep
tree-sitter-toml-ng = "0.7"    # verify ABI; bump if needed
tree-sitter-yaml = "0.7"       # verify ABI; bump if needed
tree-sitter-md = { version = "0.5", features = ["parser"] }  # verify ABI
tree-sitter-go = "0.25"        # already 0.25, keep
tree-sitter-java = "0.25"      # upgrade from 0.23
```

**Critical rule**: do NOT apply a partial bump. All three groups in one edit.

### Step S-5: Apply API source edits
Using `taco-forge perfect-edit` per REGRA #14:

**S-5a**: Fix `search.rs:1` — relocate `StrDoc` import:
```
taco-forge perfect-edit \
  --path crates/touring-code/src/polyglot/search.rs \
  --intent "Relocate StrDoc import from ast_grep_core root to ast_grep_core::tree_sitter for 0.42 migration" \
  --operation rewrite \
  --pattern "use ast_grep_core::{AstGrep, NodeMatch, Pattern, StrDoc};" \
  --replacement "use ast_grep_core::{AstGrep, NodeMatch, Pattern};\nuse ast_grep_core::tree_sitter::StrDoc;"
```

**S-5b**: If `AstGrep` alias is non-transparent (compile-driven), fix `rewrite.rs:1`:
```
taco-forge perfect-edit \
  --path crates/touring-code/src/polyglot/rewrite.rs \
  --intent "Update AstGrep import for 0.42 if alias is non-transparent" \
  --operation rewrite \
  --pattern "use ast_grep_core::{AstGrep, Pattern};" \
  --replacement "use ast_grep_core::{Root as AstGrep, Pattern};"
```
(Only apply if `cargo check` reports `AstGrep` not found after S-4.)

**S-5c**: If `LanguageExt` is required by `lang.rs` (compile-driven), add import:
```
taco-forge perfect-edit \
  --path crates/touring-code/src/polyglot/lang.rs \
  --intent "Add LanguageExt import for ast_grep_core 0.42 tree-sitter decoupling" \
  --operation rewrite \
  --pattern "use ast_grep_language::SupportLang;" \
  --replacement "use ast_grep_language::SupportLang;\nuse ast_grep_core::tree_sitter::LanguageExt;"
```
(Only apply if `cargo check` reports missing `LanguageExt`.)

### Step S-6: Compile gate
```bash
cargo check -p touring-code 2>&1 | grep "^error" | wc -l
# Must be 0. Iterate on S-5 variants until clean.
cargo check --workspace 2>&1 | grep "^error" | wc -l
# Must be 0.
```

### Step S-7: Full test suite (regression gate)
```bash
cargo test -p touring-code 2>&1 | tail -10
cargo test -p touring-hooks 2>&1 | tail -10
cargo test --workspace --exclude fuzz 2>&1 | tail -10
```
All counts must be >= baseline from S-2. Zero regressions.

### Step S-8: Polyglot E2E + fuzz validation (ABI correctness gate)
```bash
# E2E polyglot suite — the canary for ABI mismatch
cargo test -p touring-ast-polyglot 2>&1 | tail -10

# Specifically test Go + Bash (the known broken languages)
cargo test -p touring-code -- polyglot 2>&1 | grep -E "PASS|FAIL|ok|FAILED"

# W11.6 fuzz targets — ABI panic canary
cargo +nightly fuzz build 2>&1 | tail -5
cargo +nightly fuzz run fuzz_ast_grep_search -- -runs=1000 2>&1 | tail -5
cargo +nightly fuzz run fuzz_ast_grep_rewrite -- -runs=1000 2>&1 | tail -5
# (replace fuzz target names with actual names from `cargo fuzz list`)
```

### Step S-9: Bash validator re-enable (post-ABI-fix benefit)
After S-7/S-8 pass, the workaround comment in `bash_ast_validator.rs:12` is no longer
accurate. Update the comment to reflect that the ABI mismatch is resolved and the tokenizer
fallback is now a performance optimization, not a necessity.

### Step S-10: Verify Cargo.lock deduplication
```bash
grep "tree-sitter-bash\|tree-sitter-go\|tree-sitter-css" /home/gabrielgadea/.claude/rust/Cargo.lock | grep "^name"
# Must show EXACTLY ONE version per grammar — no more duplicate pairs.
```

### Step S-11: Persist lesson + RL reward
```bash
touring memory store "ast-grep-abi-migration:completed:$(date +%s)" \
  "ast-grep 0.36→0.42 + tree-sitter 0.24→0.26 + all grammars to ABI 15. search.rs:1 StrDoc path fix. Cargo.lock deduped. Go+Bash AST parsing live." \
  --tier semantic
touring learning reward orchestrate 1.0 "ast-grep ABI perfection plan delivered + migration validated"
```

---

## 6. Validation Gate Summary

| Gate | Command | Pass condition |
|------|---------|----------------|
| **Compile** | `cargo check --workspace` | 0 errors |
| **Unit tests** | `cargo test -p touring-code` | >= baseline |
| **Integration** | `cargo test -p touring-hooks` | >= baseline |
| **Full workspace** | `cargo test --workspace --exclude fuzz` | >= baseline |
| **Polyglot E2E** | `cargo test -p touring-ast-polyglot` | all pass |
| **Fuzz canary** | `cargo +nightly fuzz run fuzz_*` -runs=1000 | no panics |
| **No duplicates** | `grep "tree-sitter-bash" Cargo.lock \| wc -l` | == 1 name |
| **Go AST live** | polyglot test for Go source | no ABI error |
| **Bash AST live** | polyglot test for Bash source | no ABI error |

---

## 7. Risk Matrix

| Risk | Severity | Likelihood | Mitigation |
|------|----------|-----------|------------|
| Grammar crate at 0.25 not on crates.io for some language | HIGH | LOW | Verify pre-bump via `cargo metadata`; fall back to exact 0.24.x if 0.25 absent |
| `tree-sitter-toml-ng`, `tree-sitter-yaml`, `tree-sitter-md` are ABI 14 at current version | HIGH | MED | Run S-3 check; find ABI-15 release or feature-gate off from ast-grep scanner |
| ast-grep 0.42 has additional breaking changes not covered in §4 | MED | LOW | Full compile + fuzz run catches all runtime breaks |
| `AstGrep` alias non-transparent in new release | MED | LOW | S-5b fallback edit; `cargo check` is the oracle |
| Build time increases due to new grammar crates | LOW | HIGH | Acceptable; sccache mitigates; no correctness impact |
| Fuzz targets need `+nightly` and may not be in CI | LOW | MED | Run locally; W11.6 fuzz/ directory confirmed at workspace root |

---

## 8. Effort Estimate

| Phase | Work | Effort |
|-------|------|--------|
| S-1 to S-3: prep + grammar ABI verification | Bash + cargo commands | **S** (30 min) |
| S-4: Cargo.toml atomic edit | 1 file, ~15 line changes | **S** (15 min) |
| S-5: API source edits | 1 definite edit + 2 conditional | **S** (30 min) |
| S-6 to S-10: compile + test + validate | `cargo check` + test suites | **M** (1-2 h compile) |
| S-11: persist + RL | Touring CLI | **S** (5 min) |
| **Total** | | **M** (2-3 engineer-hours, 1 session) |

---

## 9. Rollback (REGRA #11 — no git)

No git is available. Rollback strategy:

1. **Pre-bump snapshot** (S-1) stores the exact version strings in Touring memory.
2. If S-6 fails catastrophically and iterative S-5 fixes do not resolve it:
   - Restore `Cargo.toml` workspace deps section to the pre-bump values (from memory).
   - The `Cargo.lock` will re-resolve on next `cargo check`.
   - The `taco-forge perfect-edit` atomic snapshot (`~/.claude/touring/perfect-edit-snapshots/`)
     preserves the pre-edit `Cargo.toml` state for surgical restoration.
3. `touring memory recall "ast-grep-abi-migration:pre-bump"` retrieves the exact pins.

---

## 10. Symbol Verification Table

### verified_existing (confirmed by grep/read evidence this session)

| Symbol | File | Line | Evidence |
|--------|------|------|---------|
| `AstGrep` | `crates/touring-code/src/polyglot/search.rs` | 45 | grep: `let grep = AstGrep::new(source, sg_lang)` |
| `AstGrep` | `crates/touring-code/src/polyglot/rewrite.rs` | 28 | grep: `let mut grep = AstGrep::new(source, sg_lang)` |
| `Pattern::try_new` | `crates/touring-code/src/polyglot/search.rs` | 49 | grep: `Pattern::try_new(pattern, sg_lang)` |
| `Pattern::try_new` | `crates/touring-code/src/polyglot/rewrite.rs` | 32 | grep: `Pattern::try_new(pattern, sg_lang)` |
| `StrDoc` | `crates/touring-code/src/polyglot/search.rs` | 1 | grep: `use ast_grep_core::{AstGrep, NodeMatch, Pattern, StrDoc}` |
| `NodeMatch` | `crates/touring-code/src/polyglot/search.rs` | 63 | grep: `fn to_match(m: &NodeMatch<'_, StrDoc<SupportLang>>` |
| `SupportLang` | `crates/touring-code/src/polyglot/lang.rs` | 1,39 | grep: `use ast_grep_language::SupportLang` + `fn as_ast_grep(self) -> SupportLang` |
| `search` (facade fn) | `crates/touring-hooks/src/shared/ast_grep_signal.rs` | 32 | grep: `use touring_ast_polyglot::{search, Lang}` |
| `bash_ast_validator` | `crates/touring-hooks/src/shared/bash_ast_validator.rs` | 12 | Read: comment "avoids the ast-grep-language 0.36 / tree-sitter-language v15 ABI mismatch" |
| `ast-grep-core = "=0.36.0"` | `Cargo.toml` | 399 | grep output |
| `ast-grep-language = "=0.36.0"` | `Cargo.toml` | 400 | grep output |
| `tree-sitter-bash 0.25.1` (Cargo.lock, ABI 15) | `Cargo.lock` | 14720 | grep output |
| `tree-sitter-bash 0.23.3` (Cargo.lock, ABI 14, ast-grep pin) | `Cargo.lock` | 14710 | grep output |
| `tree-sitter-go 0.25.0` (Cargo.lock, ABI 15) | `Cargo.lock` | 14814 | grep output |
| `tree-sitter-go 0.23.4` (Cargo.lock, ABI 14, ast-grep pin) | `Cargo.lock` | 14804 | grep output |

### to_be_created / unverified_planned

| Symbol | Rationale | Confidence |
|--------|-----------|-----------|
| `ast_grep_core::tree_sitter::StrDoc` (new module path) | 0.42 API change per CEG research; confirmed by CHANGELOG; compile will verify | 0.85 |
| `ast_grep_core::tree_sitter::LanguageExt` (new trait) | 0.38 LanguageExt split per CEG research; only needed if touring code calls ts-specific methods (unlikely) | 0.65 |
| `ast_grep_core::Root` (AstGrep alias target) | 0.38 AstGrep→Root rename per CEG research; alias likely transparent | 0.80 |

---

## Summary for Gabriel

**File**: `/home/gabrielgadea/.claude/rust/docs/2026-05-17-tree-sitter-abi-perfection.md`

**Recommended target**: `ast-grep-core + ast-grep-language = 0.42.2` + `tree-sitter = 0.26`
+ all grammar crates at ABI-15-compatible releases (0.25.x for most). Single ABI plane: **15**.

**Root cause confirmed**: Yes — the duplicate grammar version situation is the direct root
cause. The workspace pins `tree-sitter-bash = "0.25"` (ABI 15) and `tree-sitter-go = "0.25"`
(ABI 15) but ast-grep 0.36 bundles its own `tree-sitter-bash 0.23.3` and
`tree-sitter-go 0.23.4` (ABI 14) internally. The Cargo resolver cannot unify them — two
different versions coexist. At runtime, when the ABI-15 grammar from the touring path is
handed to ast-grep, the ABI check fails. Upgrading ast-grep to 0.42 makes it use ABI-15
natively, eliminating the duplicate and the panic.

**ast-grep API call sites broken by 0.42 bump**: **1 definite edit** + 3 compile-verify:
- `search.rs:1` — `StrDoc` import path changes (DEFINITE)
- `rewrite.rs:1` — `AstGrep` alias transparency (COMPILE-VERIFY)
- `lang.rs:1` — `LanguageExt` import (COMPILE-VERIFY, likely not needed)
- `search.rs:63` — `NodeMatch<'_, StrDoc<SupportLang>>` signature (follows from StrDoc fix)

All 7 indirect consumers (touring-hooks, touring-server, touring-generator) need zero changes.

**Effort**: M — 2-3 engineer-hours in one session.

**Top risk**: grammar crates for `tree-sitter-toml-ng`, `tree-sitter-yaml`, `tree-sitter-md`
may not have ABI-15 releases published. Run `cargo metadata` verification (S-3) before
the atomic bump. If any lack ABI-15, feature-gate that language out of the ast-grep scanner
rather than shipping a broken grammar.
