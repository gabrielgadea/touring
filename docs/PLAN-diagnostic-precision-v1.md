# PLAN: Diagnostic Precision Improvement — Root Cause Analysis + Improvement Plan

> **Version**: v1.0 | **Date**: 2026-04-12
> **Trigger**: 13 false positives in a single session (integration Pln2 metadata+generator)
> **Cost**: ~200k tokens wasted on FP investigation, agent respawns, failed approaches
> **Method**: Evidence-based analysis from session 2026-04-12

---

## 1. Root Cause Analysis — Evidence from Session 2026-04-12

### RC1: Plan Docs as Stale Source of Truth (CRITICAL)

**Evidence**: Both Pln2 plans (82 + 126 = 208 tasks) were written weeks ago. When scouts analyzed them, they assumed all tasks were still pending. VP-Scout only caught FPs when scouts explicitly grep'd the codebase.

**Impact**: 13 false positives — 7 from initial scout, 6 more from W2 (ALL CLI commands already implemented).

| False Positive | Plan Said | Reality |
|---------------|-----------|---------|
| SCHEMA_VERSION 6→7 | "Bump to 7" | Already 7 (migration.rs:17) |
| blake3 dependency | "Add to workspace" | Already `blake3 = "1.5.5"` |
| blake3 hash.rs | "Create adapter" | Already exists with `content_hash()` |
| wiring_suggest | "Implement" | cli_handlers.rs:385 + MCP tool + DB table |
| moka dependency | "Add to Cargo.toml" | Already `moka = "0.12"` |
| Hook count = 98 | "Patch both asserts" | Already 99 at lines 732+734 |
| symbol_events_log | "Not wired" | `insert_symbol_event` at knowledge.rs:1484, wired in post_edit:429 + post_write:175 |
| touring search CLI | "Create handler" | command_table:466 + hook_registry:719 + cli_handlers:3185 |
| touring query CLI | "Create handler" | command_table:484 + hook_registry:721 + cli_handlers:3269 |
| metadata-backfill | "Create handler" | command_table:495 + hook_registry:722 + cli_handlers:3398 |
| session-summary | "Create handler" | command_table:506 + hook_registry:723 + cli_handlers:3448 |
| bench-run | "Create handler" | command_table:517 + hook_registry:724 + cli_handlers:3483 |
| W4 leiden.rs | "Create module" | Already existed (19k bytes, abr 12 02:14) |

**Root Cause**: Plan documents describe INTENT at time of writing, not current state. No mechanism auto-invalidates plan tasks when code catches up.

### RC2: Background Agents Fail Silently (HIGH)

**Evidence**: W3 engineer "completed" but produced NO files (query_dsl.rs, scip_emit.rs didn't exist). W5 engineer same. Agent result summary showed "Good. I have all the context needed..." — the last thought before context exhaustion, not a completion summary.

**Impact**: 2 agents consumed ~100k tokens each with zero output. Had to redo work manually.

**Root Cause**: 
- Agent completion notification only shows last message, not success/failure of file writes
- No post-agent verification step (check if expected files exist)
- Agent definitions are 600-740 lines each — agents spend most of their context absorbing the prompt instead of doing work

### RC3: Architecture Decisions Not Validated (HIGH)

**Evidence**: `include!()` macro was proposed for server/mod.rs split. Rust doesn't support `include!()` inside `impl` blocks. Discovered at compilation time after the engineer had already extracted 4800 lines.

**Impact**: ~30 minutes wasted on failed approach. Had to pivot to separate `impl` blocks.

**Root Cause**: No `cargo check` validation of architectural approach BEFORE full extraction. Should have tested with 1 method first.

### RC4: Touring Memory FP Feedback Loop Broken (MEDIUM)

**Evidence**: `touring memory recall "false_positive"` returned 0 entries. The FP lessons stored in this session used key prefix `fp:pln2:` but recall with "false_positive" couldn't find them.

**Impact**: FP patterns from previous sessions are not discoverable. Same FPs will repeat.

**Root Cause**: Inconsistent memory key naming. No standardized FP storage format. `memory recall` is keyword-based FTS5 and doesn't match partial key prefixes well.

### RC5: Hook System Generates Constant Noise (LOW but persistent)

**Evidence**: EVERY Bash call triggers:
```
⚡ Bash failure: Exit code 2
ls: não foi possível acessar '/home/gabrielgadea/.claude/rust/crates/touring-memory/src/': Arquivo ou diretório inexistente
```

**Impact**: ~50+ noise messages per session. Pollutes context, costs tokens, distracts from real errors.

**Root Cause**: A hook (probably pre-bash or post-bash) tries to ls `crates/touring-memory/src/` which doesn't exist. The check is hardcoded for a crate that was never created.

### RC6: Agent Definition Bloat (MEDIUM)

**Evidence**: Agent definitions total 3401 lines across 5 files (avg 680 lines each). A Sonnet agent with 200k context window spends ~15-20% of its context just absorbing the prompt.

**Impact**: Agents have less context for actual work. Complex tasks exhaust context before completion.

**Root Cause**: Agent definitions try to be exhaustive — every possible CLI command, every VP-Scout chain, every edge case. The result is agents that know everything in theory but can't do anything in practice due to context pressure.

### RC7: TACO Protocol Overhead for Simple Tasks (MEDIUM)

**Evidence**: The full TACO 7-phase protocol was invoked for a plan integration task. Phases 0-4 consumed ~300k tokens of context before any code was written. 3 parallel scout agents + 1 architect + 3 engineers + 1 auditor + 1 scriber = 9 agent spawns.

**Impact**: High token cost. Some phases (Context7, full decompose DAG) added no value. Agents competed for file access.

**Root Cause**: CILA routing doesn't effectively downgrade protocol complexity. All tasks get L4+ treatment regardless of actual complexity.

---

## 2. Touring Resources: Usage Audit

### 2.1 Well-Used Resources

| Resource | Usage Quality | Notes |
|----------|--------------|-------|
| `touring doctor -j` | ✅ Good | Phase 0 gate works correctly |
| `touring status -j` | ✅ Good | Dashboard snapshot reliable |
| `touring index find` | ✅ Good | Symbol lookup is fast and accurate |
| `touring memory store` | ⚠️ Fair | Stores lessons but key format inconsistent |
| `touring wiring orphans` | ✅ Good | Accurate orphan detection |
| `touring learning reward` | ✅ Good | RL rewards injected consistently |

### 2.2 Under-Used Resources

| Resource | Current Usage | Should Be |
|----------|--------------|-----------|
| `touring gotcha match <file>` | Rarely used | **MANDATORY before editing any file** — prevents known pitfalls |
| `touring memory recall` | Used but FTS5 misses key-prefix queries | **Need structured queries** — `touring memory list --sort access_count` is better for patterns |
| `touring e2e --depth deep` | Almost never used | **Should run at session end** to catch regressions |
| `touring evolution drift` | Never used in practice | **Should detect degrading scout accuracy** |
| `touring detect_changes` | Never used in sessions | **Should replace manual grep for impact analysis** |
| `touring shadow validate` | Documented but not enforced | **MANDATORY before file writes** — catches syntax errors |
| `touring incremental status` | Never checked | **Cache health affects scout accuracy** |

### 2.3 Misused Resources

| Resource | Problem | Fix |
|----------|---------|-----|
| Plan docs read by scouts | Treated as ground truth for code state | **Read code first, use plans only for intent/context** |
| `touring wiring suggest` MCP | Plan said "implement" but already existed | **Check `touring index find` BEFORE proposing implementations** |
| Agent `run_in_background` | Agents "complete" without verification | **Add post-agent file existence check** |
| VP-Scout chains | Defined in scouter.md but not enforced | **Make chains produce verifiable CLI output, not prose** |

---

## 3. Improvement Plan — 5 Targeted Fixes

### Fix 1: Mandatory Code-First Verification Gate (CRITICAL)

**Problem**: Scouts read plan docs and assume tasks are pending.

**Solution**: Add a `VERIFY_BEFORE_REPORT` gate to touring-scouter.md:

```
RULE: Before reporting ANY finding as "NOT_IMPLEMENTED" or "PENDING":
1. Run `touring index find <symbol>` — if count > 0, it EXISTS
2. Run `grep -rn <pattern> crates/ | head -5` — if matches, it EXISTS  
3. Run `cargo check --workspace 2>&1 | tail -3` — if exit 0, it COMPILES

If ANY of these 3 checks finds the item, classify as "ALREADY_IMPLEMENTED".
Plan docs are INTENT, not STATE. Code is STATE.
```

**Effort**: S (2h) — edit touring-scouter.md

### Fix 2: Agent Output Verification Protocol (HIGH)

**Problem**: Background agents "complete" without producing artifacts.

**Solution**: After every background agent completes, the orchestrator MUST:

```python
# Post-agent verification (add to TACO orchestrator)
for expected_file in agent.expected_outputs:
    if not os.path.exists(expected_file):
        log(f"AGENT FAILED: {agent.name} did not create {expected_file}")
        # Respawn with smaller scope or do manually
```

Implement in TACO-subagent.md:
- Engineer agents MUST declare `expected_files: [...]` in their JSON output
- Orchestrator checks file existence after agent completes
- If missing → respawn with focused prompt OR escalate to orchestrator

**Effort**: M (4h) — edit TACO-subagent.md + orchestrator behavior

### Fix 3: Compact Agent Definitions (HIGH)

**Problem**: 680 lines avg per agent definition. Agents exhaust context absorbing prompts.

**Solution**: Reduce each agent definition to <200 lines:
- Move VP-Scout chains to a separate reference file (`~/.claude/rules/VP-Scout.md` — already exists!)
- Move CLI command reference to `~/.claude/rules/touring-cli-commands.md` (already exists!)
- Agent definition = role + mandatory steps + output format ONLY
- Reference files loaded via `@rule` directive only when needed

**Target**:
| Agent | Current | Target | Method |
|-------|---------|--------|--------|
| touring-scouter | 638 lines | <150 lines | Extract chains to VP-Scout.md reference |
| touring-architect | 692 lines | <150 lines | Extract CLI ref to touring-cli-commands.md |
| touring-engineer | 720 lines | <150 lines | Extract VGP to vgp-protocol.md reference |
| touring-auditor | 736 lines | <200 lines | Extract checklist to audit-protocol.md |
| touring-scriber | 615 lines | <150 lines | Extract templates to doc-templates.md |

**Effort**: L (8h) — refactor all 5 agent definitions

### Fix 4: Fix Hook Noise + Memory Key Format (MEDIUM)

**Problem A**: Every Bash call generates `touring-memory` noise.
**Problem B**: Memory keys are inconsistent (`fp:pln2:`, `gotcha:hook_registry:`, etc).

**Solution A**: Find and fix the hook that checks for `crates/touring-memory/src/`:
```bash
grep -rn "touring-memory" ~/.claude/hooks/ ~/.claude/settings.json
```
Remove or guard the check.

**Solution B**: Standardize memory key format:
```
Pattern: <category>:<scope>:<identifier>
Examples:
  fp:session:2026-04-12:schema_v7     # false positive
  lesson:engineer:include_macro        # lesson learned
  pattern:split:separate_impl_blocks   # reusable pattern
  gotcha:hook_registry:dual_assert     # known pitfall
```
Add key format validation to `touring memory store`.

**Effort**: S (2h)

### Fix 5: CILA-Based TACO Pruning (MEDIUM)

**Problem**: Full 7-phase TACO protocol for every task, regardless of complexity.

**Solution**: Enforce CILA routing with actual pruning:

| CILA | What Happens | Agents Spawned |
|------|-------------|----------------|
| L0-L1 | Orchestrator does it directly | 0 |
| L2 | 1 scout (foreground) + implement | 1 |
| L3 | 1 scout + 1 engineer | 2 |
| L4 | Scout + architect + engineer + audit | 4 |
| L5-L6 | Full 7-phase | 5-9 |

Current behavior: EVERYTHING gets L4+ treatment. Fix: classify intent FIRST, then route.

**Effort**: M (4h) — edit TACO-subagent.md routing logic

---

## 4. Prioritized Execution Order

```
Fix 1 (Code-First Gate)     ─── S (2h) ─── IMMEDIATE
  ↓
Fix 4 (Hook Noise + Keys)   ─── S (2h) ─── IMMEDIATE  
  ↓
Fix 2 (Agent Output Verify)  ─── M (4h) ─── This week
  ↓
Fix 3 (Compact Agents)       ─── L (8h) ─── This week
  ↓
Fix 5 (CILA Pruning)         ─── M (4h) ─── Next week
```

**Total effort**: ~20h
**Expected impact**: 80%+ reduction in false positives, 50%+ reduction in wasted tokens

---

## 5. Success Metrics

| Metric | Current | Target | Measurement |
|--------|---------|--------|-------------|
| False positives per session | 13 (this session) | ≤ 2 | Count FPs in audit phase |
| Agent success rate (files created) | 2/5 (40%) | ≥ 4/5 (80%) | Check expected_files after completion |
| Agent definition size | 680 lines avg | <200 lines avg | `wc -l agents/*.md` |
| Hook noise per session | ~50 messages | 0 | Count `touring-memory` warnings |
| Memory FP recall accuracy | 0/13 found | ≥ 10/13 | `touring memory recall "fp:"` |
| TACO overhead for L2 tasks | 7 phases | 2 phases | Phase count per CILA level |

---

## 6. Appendix: Evidence Trail

### A. Session Token Budget Breakdown (estimated)

| Phase | Tokens | Value Added |
|-------|--------|------------|
| Scout agents (3 parallel) | ~170k | HIGH (found FPs) |
| Architect (sequential-thinking) | ~20k | HIGH (designed plan) |
| W0 engineer (server split) | ~100k | HIGH (successful refactor) |
| W3 engineer (FAILED) | ~55k | **ZERO** (no files created) |
| W4 engineer (redundant) | ~70k | **LOW** (file already existed) |
| W5 engineer (FAILED) | ~63k | **ZERO** (no changes made) |
| Auditor | ~105k | HIGH (found 6 more FPs) |
| Manual fixes | ~50k | HIGH (actually created files) |
| **Total** | ~633k | **~40% wasted** |

### B. The touring-memory Hook Noise

The persistent `ls: não foi possível acessar '/home/gabrielgadea/.claude/rust/crates/touring-memory/src/'` appears because a hook (likely `pre-bash` or `post-bash`) checks for a `touring-memory` crate that was planned but never created. To fix:

```bash
# Find the offending hook
grep -rn "touring-memory" ~/.claude/hooks/ ~/.claude/settings*.json
# Remove or guard the check
```

---

*Analysis produced directly from session evidence. No agents spawned for this meta-analysis — that would be the ultimate false positive.*
