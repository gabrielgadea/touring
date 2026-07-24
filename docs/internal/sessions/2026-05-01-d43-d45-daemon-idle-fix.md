# Session Report — D43 + D45 + Daemon Idle Fix

> **Date**: 2026-05-01 | **Version delta**: skill v4.27.0 → v4.28.0 | **Hook Registry**: 176 → 178
> **Master plan deliveries**: D43 (CC2) + D45 (CC4) — Wave 2 token-saving suite
> **Companion fix**: daemon idle timeout configurable (root cause of recurring "Connection refused" at SessionStart)

---

## Summary

Three integrated changes shipped in a single session, all production-verified:

| Item | Type | Location | Test cycle |
|------|------|----------|------------|
| Daemon idle timeout fix | L1 bugfix | `crates/touring-hooks/src/daemon.rs` | 3 unit tests in E2E suite + live monitor 60s+ stable |
| **D43** PreToolUse Grep/Glob enrichment | L2 feature (master plan W2) | `crates/touring-hooks/src/{pre_grep,pre_glob}.rs` | 15 unit + 4 registry + 20 E2E = 39 tests |
| **D45** `Bash(touring *)` permission auto-add | XS sub-task (master plan W7) | `~/.claude/settings.json` | Idempotent merge + JSON validity check |

Shared validation: `cargo check --workspace` PASS · 3284/3284 lib tests PASS · 0 clippy warnings on touring-hooks · `touring doctor -j` 5/5 ok · daemon stable without self-shutdown.

---

## 1. Daemon idle timeout — root cause + fix

### Root cause (confidence 1.0)

Old code in `daemon.rs:452`:

```rust
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
```

A watchdog at line 593 called `graceful_shutdown` after 300s of inactivity, **removing socket and lock files**. On a developer workstation with CC sessions resuming after long idle gaps, every `SessionStart` hit the cold-start race window between previous-daemon-exit and auto-respawn-ready, surfacing as `Connection refused (os error 111)` and `composite_health_score=0.5` in the SessionStart context block.

### Fix — opt-in via env var

```rust
fn idle_timeout_secs() -> u64 {
    std::env::var("TOURING_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)  // ← default disabled
}
```

The watchdog now only spawns when `idle_timeout_secs() > 0`, and re-reads the env var on every tick (operators can flip the timeout without restarting the daemon).

### Trade-offs

| Decision | Cost | Benefit |
|----------|------|---------|
| Default disabled (`0`) | Daemon RSS stays resident (~92 MB observed; ~4 TB virt is mostly mmap, normal) | Eliminates SessionStart cold-start race; CC sessions land on a healthy daemon every time |
| Re-read env per tick | One extra `getenv` syscall per 30 s | Dynamic config — bump `TOURING_IDLE_TIMEOUT_SECS=3600` for cloud/container deployments without rebuild |

### Validation

- Daemon uptime monitored 60 s post-fix → stable, single PID, no respawn cycle.
- `strings <release/touring-hook>` confirms new strings: `idle watchdog disabled (set TOURING_IDLE_TIMEOUT_SECS>0 to enable)`, `idle watchdog enabled`, `TOURING_IDLE_TIMEOUT_SECS`.
- `touring doctor -j` returns 5/5 ok consistently.

---

## 2. D43 — PreToolUse Grep/Glob symbol enrichment

### Purpose

When Claude Code invokes `Grep` or `Glob` with a symbol-like pattern (PascalCase / snake_case / camelCase), inject a context block listing the symbol's locations from the index. CC then frequently reads the file:line directly and skips the Grep — measurable token saving with sub-millisecond cost.

### Algorithm (`pre_grep.rs`)

```
disable env? → Allow
extract pattern → None? → Allow
is_symbol_like(pattern)? → false? → Allow
runtime.infra.symbol_store.find_symbol(pattern)
  empty? → record_pre_grep_zero_results() → Allow
  ≥1 result? → format_enrichment + record_pre_grep_enrichment → Context
```

**Whitelist regex** (length 3..=50, ≥3 ASCII letters, first char alphabetic|`_`, all chars alphanumeric|`_`): accepts `HookRuntime`, `cli_pre_grep`, `getUserId`; rejects free text, regex meta, glob paths, too-short, too-long, all-underscores.

**Performance**: P99 = **2 ms** (target spec <50 ms — **25× margin**).

### Files

| File | LOC | Purpose |
|------|-----|---------|
| `crates/touring-hooks/src/pre_grep.rs` (NEW) | 280 + 15 unit tests | Core handler |
| `crates/touring-hooks/src/pre_glob.rs` (NEW) | 25 | Delegate to `pre_grep::run_returning` |
| `crates/touring-hooks/tests/d43_pre_grep_glob_e2e.rs` (NEW) | 305 / 20 tests | Cross-component E2E |
| `crates/touring-hooks/src/lib.rs` | +4 lines | `pub mod` registrations |
| `crates/touring-hooks/src/main.rs` | +6 lines | Dispatch arms + help |
| `crates/touring-hooks/src/hook_registry.rs` | +6 lines | `all_daemon_hook_names`, `ALL_DAEMON_HOOK_NAMES`, dispatch table, count tests (184→186 / 182→184) |
| `crates/touring-hooks/src/shared/gate_metrics.rs` | +30 lines | 2 atomic counters, 2 record_* fns, 2 snapshot fields with `#[serde(default)]`, Default init, 2 fixture sites |

### Counters (gate-metrics)

```bash
touring gate-metrics -j | jq '{
  pre_grep_enrichment_count,
  pre_grep_zero_results_count
}'
```

- `pre_grep_enrichment_count` — increments on every Context emission (≥1 symbol resolved).
- `pre_grep_zero_results_count` — increments on every silent Allow following 0 lookup results. Watchdog metric: if it dominates `enrichment_count`, the whitelist regex is too permissive.

### Disable switch (R48 mitigation)

Set `TOURING_DISABLE_PREGREP=1` in the **daemon's** environment (not the CLI client's — they are separate processes). The handler short-circuits to `Allow` even on perfect matches. Useful for A/B benchmarking the token-saving uplift.

### settings.json wiring (depends on D42 cc-setup, but installed manually here)

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Grep", "hooks": [{ "type": "command",
        "command": "$HOME/.claude/hooks/touring-hook pre-grep", "timeout": 2 }] },
      { "matcher": "Glob", "hooks": [{ "type": "command",
        "command": "$HOME/.claude/hooks/touring-hook pre-glob", "timeout": 2 }] }
    ]
  }
}
```

Coexists with the existing `Grep|Glob` matcher pointing to `gitnexus-hook.cjs` — both fire on every Grep/Glob invocation.

### Acceptance criteria — 5/5 met

1. ✅ PascalCase pattern produces enrichment with locations
2. ✅ Free text > 50 chars / regex meta → silent pass-through
3. ✅ Pattern with 0 symbols → silent pass-through (zero false positive)
4. ✅ P99 latency < 50 ms — measured 2 ms
5. ✅ ≥14 tests — delivered 39 (15 unit + 4 registry + 20 E2E)

---

## 3. D45 — `Bash(touring *)` permission auto-add

`~/.claude/settings.json::permissions.allow` gained 4 entries (idempotent):

```json
[
  "Bash(touring *)",
  "Bash(update-touring *)",
  "Bash(touring-bootstrap *)",
  "Bash(touring-mcp *)"
]
```

Removes the approval prompt for every `touring` invocation. The merge script verifies idempotency by checking the exact permission string against the existing `allow` array.

---

## 4. Hook registry invariant bump

| Counter | Before | After | Delta |
|---------|--------|-------|-------|
| `all_daemon_hook_names().len()` | 184 | **186** | +2 (`pre-grep`, `pre-glob`) |
| `ALL_DAEMON_HOOK_NAMES.len()` | 182 | **184** | +2 |
| Skill `Hook Registry` claim | 176 | **178** | +2 |

Test `registry_has_expected_count` updated; `no_duplicate_hook_names` and `registry_names_match_dispatch_table` continue to pass.

---

## 5. End-to-end proof (verified)

```text
[1/8] doctor: 5/5 ok
[2/8] daemon exe: /home/gabrielgadea/.claude/rust/target/release/touring-hook (NOT deleted)
[3/8] strings binary: pre-grep, pre-glob, TOURING_DISABLE_PREGREP, "=== Touring symbol enrichment ===" present
[4/8] settings.json: 1 Grep matcher + 1 Glob matcher + 4 touring permissions
[5/8] E2E PascalCase HookRuntime → 20 symbol locations + hookEventName=PreToolUse
[6/8] E2E free text "the quick brown fox" → silent {}
[7/8] counters monotonic: enrichment 1→2 (+1), zero_results 0→1 (+1)
[8/8] daemon env: TOURING_IDLE_TIMEOUT_SECS unset → watchdog DISABLED
```

Live test invocation (CC payload shape):

```bash
echo '{"hook_event_name":"PreToolUse","tool_name":"Grep","tool_input":{"pattern":"HookRuntime","path":"crates"}}' \
  | CLAUDE_PROJECT_DIR=/home/gabrielgadea/.claude/rust touring-hook pre-grep
# → {"hookSpecificOutput":{"additionalContext":"=== Touring symbol enrichment ===\nPattern 'HookRuntime' resolved to 20 symbol locations:\n  - crates/touring-hooks/src/runtime.rs:202:0 → def\n... 26 lines total\n=== End enrichment ===\n","hookEventName":"PreToolUse"}}
```

---

## 6. Lessons (persisted to memory)

| Key | Tier | Content |
|-----|------|---------|
| `fix:daemon-idle-shutdown-2026-05-01` | semantic | Idle timeout root cause + L1 fix + selective build pattern |
| `feat:d43-pre-grep-enrichment-2026-05-01` | semantic | Algorithm, whitelist regex, P99 metrics, daemon project_root caveat |
| `feat:d43-d45-settings-json-2026-05-01` | semantic | Idempotent JSON merge pattern, coexistence with gitnexus-hook |
| `audit:cross-audit-d43-d45-2026-05-01-COMPLETE` | semantic | Full validation matrix, 8/8 gate proof, zero orphans |

Recall via `touring memory recall "<query>"`.

---

## 7. Cross-references

- **Master plan**: `~/.claude/rust/docs/2026-04-30-graph-viz-capability-parity-master-plan.md` §D43, §D45 — both annotated DELIVERED 2026-05-01
- **Skill changelog**: `~/.claude/skills/Touring/references/changelog.md` v4.28.0
- **Skill master**: `~/.claude/skills/Touring/SKILL.md` — version line + Hook Registry count bumped
- **CLI ranks**: `~/.claude/rules/touring-cli-index.md` — Hook Registry count bumped
- **Touring rebuild rule**: `~/.claude/rules/touring-rebuild.md` — env var documentation extended
- **E2E test source**: `crates/touring-hooks/tests/d43_pre_grep_glob_e2e.rs`

---

## 8. What is intentionally NOT done (out of scope)

- **D42 — `touring init --cc-setup` installer**: settings.json was edited directly here. The full installer (with backup, dry-run, multi-project registry, `include_str!` embedded hooks) ships in master plan Wave 7.
- **D44 — speckit-style slash commands**: 11 `touring.*.md` commands gated on Wave 7 + Context7 verification of the `handoffs:` frontmatter feature (Risk R49).
- **Idle-timeout opt-in CLI flag**: `TOURING_IDLE_TIMEOUT_SECS` is the env-var contract. A `--idle-timeout-secs N` flag on the daemon entry-point would duplicate the surface for negligible operator benefit; deferred.
