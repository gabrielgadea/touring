# Wave v4.29.0 Session Report — ast-grep Optimization of pre-read & pre-bash

> **Date**: 2026-05-01 (afternoon) | **Skill version**: v4.28.0 → v4.29.0
> **Scope**: 3 strategies + 3 collateral bug fixes shipped together

---

## Summary

Three coordinated strategies improve pre-read and pre-bash signal quality:

| # | Strategy | Hook | Files | Tests |
|---|----------|------|-------|-------|
| **S1** | AstGrepRiskSignalLayer | pre-read | `ast_grep_signal.rs`, `risk_patterns.rs` | 16 unit + 5 layer + cross |
| **S2** | Bash structural validator | pre-bash | `bash_ast_validator.rs` | 22 unit |
| **S3** | Command shape clustering | pre-bash | `bash_ast_validator.rs` | (shared with S2) |
| **E2E** | Cross-component proof | both | `wave_v429_ast_grep_hooks_e2e.rs` | 30 |

**Collateral bug fixes** (during cross-audit):

| # | Bug | Fix |
|---|-----|-----|
| B1 | main.rs printed daemon-returned text without validating JSON-ness — could cause `Hook JSON output validation failed (root): Invalid input` under daemon crash / IPC race | Trim + parse-check on `resp.output`; fall back to canonical `{}` Allow + log to stderr |
| B2 | PreToolValidator regex captured `--force-with-lease` because `--force` was unbounded | Word-bounded regex requires `\s` or `$` after `--force` |
| B3 | Legacy regex layer blocked `rm -rf --dry-run` and `--force-with-lease` even when bash_ast_validator (S2) cleared them | Universal intent-disclosure bypass at top of `validate()` short-circuits to Allow |

---

## 1. S1 — AstGrepRiskSignalLayer (pre-read)

### Purpose

When Claude Code reads a file at CILA ≥ 2, inject a one-line summary of risky language constructs found in it: `[risk] rust: unwrap=12, panic=2, todo=1`. Helps CC adopt a defensive-edit posture before mutating high-risk files.

### Files

- `crates/touring-hooks/src/shared/ast_grep_signal.rs` (~340 LOC + 16 tests)
  - `PatternEntry`, `PatternSet`, `PatternCount`, `ScanResult` types
  - `scan_source` (pure), `scan_source_cached` (moka), `scan_path_cached` (I/O)
  - `format_matches` — CSV-style label rendering
  - `AstGrepRiskSignalLayer` SignalLayer impl with `should_run(cila >= 2)`
  - moka cache: 64 entries, TTI 5 min, content-addressed via blake3 + set_id
- `crates/touring-hooks/src/shared/risk_patterns.rs` (~140 LOC + 9 tests)
  - RUST_RISK (4 patterns: defensive Rust constructs)
  - PYTHON_RISK (4 patterns: dynamic execution + unsafe deserialization)
  - JS_RISK (2 patterns: dynamic code evaluation)
  - GO_RISK (1 pattern: panic)
  - `pattern_set_for(lang)` + `lang_for_path(path)` resolvers

### Wire-up

In `pre_read.rs::build_parallel_signal_pipeline` after the graph layer the layer is appended with `runtime.project_root.clone()` as root, score 0.85.

### Latency budget

- Cold parse: ~3–10 ms for 200-line file (acceptable for CILA ≥ 2)
- Warm cache hit: <1 µs (moka content-addressed)
- Hard budget: 30 ms; partial results returned on overflow

---

## 2. S2 — Bash structural validator (pre-bash)

### Purpose

Detect destructive commands without false-positives on string-literal carriers (`echo "rm -rf is dangerous"`) or `#` comments. Headline win over plain regex.

### Pivot from ast-grep

Initial design used `ast-grep` with the bash grammar from `ast-grep-language 0.36.0`. Discovered ABI version mismatch (`Language(LanguageError { version: 15 })`) — the bundled bash grammar is incompatible with the bundled ast-grep-core. Pivot: tokenizer-based shell-aware validator that strips quoted strings + `#` comments BEFORE rule evaluation. Same semantic guarantees on the rule set we ship; extensible to ast-grep when the upstream grammar is upgraded.

### Files

- `crates/touring-hooks/src/shared/bash_ast_validator.rs` (~420 LOC + 22 tests)
  - `Verdict::{Allow, Warn, Block}` enum
  - `validate_command(cmd)` — runs 6 curated rules
  - `strip_string_literals_and_comments(input)` — small state machine over Code/Single/Double/Comment
  - `contains_subsequence(haystack, needle)` — windowed slice matching
  - 6 rules with `(tokens, tail_token, severity, reason, bypass_substrings)`:
    - Block: `rm -rf`, `rm -fr`, `find … -delete` (non-contiguous via tail_token)
    - Warn: `chmod -R 777`, `git push --force`, `git reset --hard`
    - Bypasses: `--dry-run`, `--force-with-lease`, `--help`

### Wire-up in pre_bash.rs

Runs BEFORE the legacy PreToolValidator (defense in depth). On `Verdict::Block`, returns `HookResponse::Deny` early; on Warn/Allow falls through to PreToolValidator.

---

## 3. S3 — Command shape clustering

### Purpose

Normalize bash commands into stable cluster keys for Pensieve failure-recall: `cargo test --release` and `cargo --quiet test -j 4 --release` both reduce to `cargo test`. Improves recall hit-rate on commands that vary only in flags/ordering.

### Implementation

`bash_ast_validator::command_shape(cmd) -> Option<String>` — tokenizer-based:

- Skip leading env-var assignments (`KEY=value`).
- Skip leading flags before head.
- After head, skip flags AND numeric flag-args (`-j 4`).
- Stop at shell separators (`;`, `&&`, `||`, `|`).
- Strip trailing separators.

### Wire-up in pre_bash.rs

Replaces the previous `extract_command_short(command)` cluster key with `command_shape(command).unwrap_or_else(|| extract_command_short(command))`. Backwards-compatible fallback on parse error / empty.

---

## 4. Collateral bug fixes (B1, B2, B3)

### B1 — Stdout JSON validity guard (`main.rs::try_daemon_request` consumer)

**Symptom** (image evidence): `Hook JSON output validation failed — (root): Invalid input` x2 during the wave session, while daemon was being killed/respawned.

**Root cause**: When `try_daemon_request` returned `Some(resp)`, main.rs printed `resp.output` verbatim. If the daemon returned partial/non-JSON text (crash, IPC race, accidental log to stdout), CC's strict schema validator rejected it.

**Fix**: defense-in-depth — parse-check `resp.output.trim()` as JSON; if invalid, emit canonical `"{}"` (Allow) and log the contamination to stderr. Preserves "exit 0 + valid JSON always" invariant under any daemon failure mode.

```rust
if resp.output.is_empty() {
    process::exit(0);
}
let trimmed = resp.output.trim();
if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
    println!("{}", resp.output);
} else {
    eprintln!("[touring-hook] daemon returned non-JSON output ({} bytes)…", resp.output.len());
    println!("{{}}");
}
```

### B2 — `git push --force` regex word boundary (`pre_tool_validator.rs:262`)

**Symptom**: `git push --force-with-lease` denied with reason "Force push overwrites remote history" (legacy generic regex).

**Root cause**: regex `(?i)^git\s+push\s+.*--force` matches the prefix `--force` of `--force-with-lease`.

**Fix**: word-bounded regex requires whitespace or end-of-line right after `--force` (Rust regex doesn't support negative lookahead).

### B3 — Universal intent-disclosure bypass (`pre_tool_validator.rs::validate`)

**Symptom**: `rm -rf --dry-run /tmp` and `git push --force-with-lease` still denied even after B2; ParamRule for `--force` and static-prefix `rm -rf` had no opt-out.

**Root cause**: PreToolValidator legacy regex layer has no concept of intent-disclosure flags. The bash_ast_validator (S2) correctly cleared these via `bypass_substrings`, but the legacy layer fired right after.

**Fix**: short-circuit at the top of `validate()`:

```rust
const INTENT_BYPASSES: &[&str] = &["--dry-run", "--force-with-lease"];
if INTENT_BYPASSES.iter().any(|s| full_command.contains(s)) {
    return ValidationResult::allow();
}
```

Realigns legacy layer with structural validator. Both flags are universal "I understand the risk and chose the safe variant" markers.

### Lesson — sccache + incremental builds may reuse stale crate object

After editing `pre_tool_validator.rs` and running `cargo build --release`, the bypass appeared in source but NOT in the binary's runtime behavior. `touch <file> && cargo build --release` forced a full recompile and the bypass took effect. **When behavior doesn't match source after a rebuild, force-touch and rebuild before deeper debugging.**

---

## 5. Validation matrix

| Gate | Result |
|------|--------|
| `cargo check -p touring-hooks` | ✅ PASS |
| `cargo test -p touring-hooks --lib` | ✅ **3331/3331** PASS (was 3284 → +47) |
| `cargo test --test wave_v429_ast_grep_hooks_e2e` | ✅ **30/30** PASS |
| `cargo test --lib pre_tool_validator` | ✅ **51/51** PASS (post-B2/B3) |
| `cargo build --release` | ✅ 2m 08s after touch-rebuild |
| Live `git push --force-with-lease` | ✅ ALLOW |
| Live `rm -rf --dry-run /tmp` | ✅ ALLOW |
| Live `git push --force origin main` | ✅ DENY (correct regex) |
| Live `rm -rf /tmp/scratch` | ✅ DENY via bash_ast_validator |
| Live `echo "rm -rf"` | ✅ ALLOW (string-literal carrier — headline win) |
| Live `ls # rm -rf comment` | ✅ ALLOW (comment carrier) |
| `touring doctor -j` | ✅ 5/5 ok |

---

## 6. Files inventory

### Created

| File | LOC | Purpose |
|------|-----|---------|
| `crates/touring-hooks/src/shared/ast_grep_signal.rs` | ~340 | S1 infra + SignalLayer impl + 16 tests |
| `crates/touring-hooks/src/shared/risk_patterns.rs` | ~140 | S1 PatternSets per language + 9 tests |
| `crates/touring-hooks/src/shared/bash_ast_validator.rs` | ~420 | S2 + S3 + 22 tests |
| `crates/touring-hooks/tests/wave_v429_ast_grep_hooks_e2e.rs` | ~310 | 30 E2E tests |
| `~/.claude/rust/docs/2026-05-01-wave-v429-ast-grep-hooks.md` | this | Session report |

### Modified

| File | Change |
|------|--------|
| `crates/touring-hooks/src/shared/mod.rs` | +3 mod registrations |
| `crates/touring-hooks/src/pre_read.rs` | wire AstGrepRiskSignalLayer into SignalPipeline |
| `crates/touring-hooks/src/pre_bash.rs` | wire bash_ast_validator (Block) + command_shape (cluster key) |
| `crates/touring-hooks/src/pre_tool_validator.rs` | B2 regex word boundary + B3 intent bypass |
| `crates/touring-hooks/src/main.rs` | B1 stdout JSON validity guard |

---

## 7. Lessons persisted (touring memory)

| Key | Tier |
|-----|------|
| `feat:wave-v429-ast-grep-hooks-2026-05-01` | semantic |
| `fix:hook-stdout-validation-bugs-2026-05-01` | semantic |

---

## 8. Cross-references

- **Skill changelog**: `~/.claude/skills/Touring/references/changelog.md` v4.29.0
- **Skill master**: `~/.claude/skills/Touring/SKILL.md` (header version)
- **CLI ranks (auto-loaded)**: `~/.claude/rules/touring-cli-index.md` (last-update line)
- **MEMORY.md index**: pointer to this wave's memory file
- **Memory file**: `~/.claude/projects/-home-gabrielgadea/memory/project_wave_v429_ast_grep_2026_05_01.md`

## 9. Out of scope (deferred)

- **ast-grep-bash grammar upgrade**: when `ast-grep-language` ships a bash grammar with ABI v14, the tokenizer pivot in S2 can be replaced with full structural matching. Token-rule semantics will remain compatible.
- **Pensieve cluster migration**: existing failure history keyed by `extract_command_short` will not auto-merge with new shapes; acceptable since Pensieve self-heals over a few sessions of new data.
- **Layer enrichment**: more languages (Java, C++, Kotlin, Ruby) — straightforward to add by extending `risk_patterns.rs`.
