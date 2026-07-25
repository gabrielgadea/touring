---
# Touring Agent Shared Base

> This file contains shared patterns for all touring agents.
> Global rules (VP-Scout.md, touring-cli-index.md, TACO-subagent.md, file-metadata-first.md) are auto-loaded — do NOT duplicate their content in individual agent definitions. Detailed CLI docs (7 cluster modules) live in `~/.claude/skills/Touring/references/touring-cli-*.md` (consulta sob demanda).

## MANDATORY: Invoke Touring Skill

**BEFORE any agent executes any task, it MUST invoke the Touring skill:**

```
Skill("Touring")
```

This activates the complete Touring CLI integration (~125 CLI commands, 88 MCP tools, **176 hook registry entries**, **45 synergy WIRED_PAIRS** after Wave 12) including VGP symbol verification, blast radius analysis, wiring audit, memory persistence, **health_delta dynamic quality tracking (Waves 9-19)**, **query result cache (5 hot paths)**, **B-301 6-dim TDG composite gate (Wave 12)**, **B-302 PatchExpansion diagnostic (Wave 12)**, and all touring intelligence commands. This is MANDATORY for ALL touring agents.

### Wave 12 (2026-04-27) — Awareness for ALL agents

| Subsystem | What changed |
|-----------|--------------|
| **B-301 RefactorRequired** | `pre_edit::compose_quality_evolution` now consumes `tdg.composite` (6-dim weighted) instead of recomputing 1-dim `avg_complexity` proxy. Tracing event includes `grade=A+..F`. Threshold preserved (blast > 20 AND composite < 0.40). |
| **B-302 PatchExpansion** | NEW RFC-100 code. `pre_write::emit_b302_if_low_confidence_expansion()` wires the orphan `PatchComplexityDelta::compute()`. Emitted when mpatch fuzzy expand + confidence < 0.7. Severity: Warning. |
| **`cli_mpatch_preview`** | Response JSON gains optional `b302_diagnostic` field (object when fires, `null` otherwise). Backward compat preserved. |
| **`gate_metrics::diagnostic_b302_emitted_count`** | New AtomicU64 counter. Helper `record_diagnostic_b302_emitted()`. Visible in `touring gate-metrics -j` and `touring synergy --with-metrics`. |
| **REGRA #13 SKILL HYGIENE** | Constitutional rule in `~/.claude/CLAUDE.md`: Anthropic limits (`name` hyphen-case ≤ 64, `description` ≤ 1024, body < 500 lines) + 5-step pre-edit protocol + anti-pollution. |

```bash
# Pre-flight extras for Wave 12 awareness
touring gate-metrics -j | jq '{
  b302_emitted: .diagnostic_b302_emitted_count,
  tdg_emitted: .diagnostic_tdg_emitted_count,
  wiring_emitted: .diagnostic_wiring_finding_emitted_count
}'
touring synergy wired -j | jq '.wired_pairs[] | select(.wave == "v4.24.0 W12")'
```

```bash
# After invoking Touring skill, proceed with pre-flight checks
touring doctor -j | jq '.[] | select(.status != "ok")'
touring status -j | jq '{idx: .index.symbol_count, orphans: .wiring.orphan_count, rl: .learning.ema_reward, hd_outstanding: .health_delta.outstanding, hd_alerts: .health_delta.streak_alert_count}'

# WIRING ANOMALY CHECK: se orphan_count > total_pub_symbols → WIRING_DB_ANOMALY
# Neste caso, todos os reports de orphan devem usar Chain 7 (grep verification)

# PREDICTIVE WAVE HEALTH: se todos os counters = 0 → hooks não estão disparando
touring gate-metrics -j | jq '{blast_inject: .blast_inject_count, hd_record: .health_delta_record_count, cache_ratio: .query_cache_hit_ratio}'
# Se blast_inject_count = 0 e hd_record_count = 0 = daemon degraded ou hooks não configurados
```

## Pre-Flight (ALL agents — execute before any task)

```bash
touring doctor -j | jq '.[] | select(.status != "ok")'
touring status -j | jq '{idx: .index.symbol_count, orphans: .wiring.orphan_count, rl: .learning.ema_reward}'

# Wave 15-16: health_delta state (streak alerts, recovery, cache hit ratio)
touring gate-metrics -j | jq '{
  hd_record: .health_delta_record_count,
  hd_regression: .health_delta_regression_count,
  hd_improvement: .health_delta_improvement_count,
  hd_streak_alert: .health_delta_streak_alert_count,
  hd_recovery: .health_delta_recovery_count,
  cache_ratio: .query_cache_hit_ratio
}'
```

## Dynamic Quality Loop (Waves 9-19 — unified edit+generate feedback)

Both **CC direct edits** and **generator pipeline commits** now feed the SAME `health_delta` cache + streak tracking + RL reward system:

| Path | Hook → health_delta | Signal surfaced |
|------|---------------------|-----------------|
| Edit tool | pre_edit → post_edit | V7 `⚙ health-delta: old=X new=Y Δ=±Z` in post_edit issues |
| Write tool | pre_write → post_write | same hint in post_write all_issues |
| Read tool | pre_read (Signal warning hint) | `⚠ regression streak: N consecutive declines` |
| Generator | Speculated::commit() per artifact | RL reward `generator_health_delta` |

### Agent responsibility

Agents should **check streak state** before suggesting refactors — if a file has `regression_streak >= 3`, prioritize recovery over expansion:

```bash
touring health-delta status <file_path>
# {"regression_streak": 3, "warning_hint": "⚠ regression streak: ...", ...}
```

## Quality Gates (ALL agents)

| Gate | Pass Criteria |
|------|--------------|
| Functional | Tests pass, output matches spec |
| Robust | Error handling present, errors not silenced |
| Readable | Clear names, obvious flow |
| Documented | Docstrings on public symbols |
| Secure | No secrets exposed, inputs validated |
| No Regression | Existing test suite green |

Composite score >= 1.0 required. Below = REJECT.

## JSON Output Format (ALL agents)

Response MUST be ONLY valid raw JSON. No markdown fences, no prose.
First character = `{`, last character = `}`.

```json
{"role":"ROLE","status":"completed|failed|partial","result":{...},"quality_gates":{...},"composite_score":1.0,"issues":[],"next_recommendations":[]}
```

## RL Reward (ALL agents — after successful action)

```bash
touring learning reward <tool> 1.0 "<context>"
```

## References (auto-loaded, do NOT copy into agent definitions)

- **Touring CLI Index** (auto-load: ranks Tier 1-9 + cheatsheet + tabela de 7 módulos): `~/.claude/rules/touring-cli-index.md`
- **Touring CLI Detalhes** (consulta sob demanda, 7 módulos por cluster funcional):
  - `~/.claude/skills/Touring/references/touring-cli-overview.md` — Arquitetura 3-camadas, daemon actor, dispatch, flags
  - `~/.claude/skills/Touring/references/touring-cli-hooks.md` — 24 lifecycle hooks + 2 neural
  - `~/.claude/skills/Touring/references/touring-cli-intelligence.md` — index, ast, wiring (F1/F2), file-knowledge, cognitive
  - `~/.claude/skills/Touring/references/touring-cli-tasks.md` — session, decompose+workflow, diary, memory, tantivy
  - `~/.claude/skills/Touring/references/touring-cli-rl-quality.md` — RL, evolution, gate-metrics + Predictive Wave, rkyv
  - `~/.claude/skills/Touring/references/touring-cli-generate.md` — touring-generator (24) + L7-B (inferlets/jobs/mpatch/MCP)
  - `~/.claude/skills/Touring/references/touring-cli-meta.md` — meta-comandos + summary + TACO workflow
- **CLI Skill master** (RANKED guide): `~/.claude/skills/Touring/SKILL.md` — CLI COMMAND RANKS v5.0
- **VP-Scout verification**: `~/.claude/skills/Touring/references/VP-Scout-rule.md`
- **TACO protocol**: `~/.claude/skills/Touring/references/TACO-subagent-rule.md`
- **File metadata**: `~/.claude/rules/file-metadata-first.md`

---

## Extended Pre-Flight (Full Protocol)

```bash
# System health — if daemon unhealthy, STOP and report before proceeding
touring doctor -j | jq '.[] | select(.status != "ok") | {name, status, detail}'

# Dashboard snapshot
touring status -j | jq '{idx: .index.symbol_count, orphans: .wiring.orphan_count, rl: .learning.ema_reward}'

# E2E health baseline
touring e2e -j | jq '{score: .composite_score, phases: .phases}'

# Session start — replace <role> and <objective> with agent-specific values
touring session start "touring-<role>-$(date +%s)" <role> "<objective>"

# Past lessons on this domain
touring memory recall "<domain_keywords>" -j | jq '.entries[:5]'
touring memory list --limit 10 --sort access_count -j

# Gotcha baseline
touring gotcha stats -j
```

---

## Checkpoint Gate — Common Protocol (ALL agents)

**Before returning output, verify ALL required fields are present.**

### Common Failure Rules (apply to ALL agents)

```
IF ANY CHECKPOINT FAILS:
  - status MUST be "partial" or "failed"
  - composite_score MUST be < 1.0
  - issues[] MUST contain: "CHECKPOINT FAILED: [specific reason]"
  - OUTPUT IS REJECTED — orchestrator will NOT accept this output
```

### Common Required Fields (ALL agents)
```
□ role — matches agent definition
□ status — "completed|failed|partial"
□ quality_gates — all 6 gates present with float values
□ composite_score — float >= 0.0
□ issues — array (may be empty)
□ next_recommendations — array (may be empty)
```

**Role-specific checkpoint fields** are defined in each agent's own CHECKPOINT GATE section.

### RL Reward — Mandatory on success
```bash
touring learning reward orchestrate 1.0 "<agent>: task completed"
```

### Validator
```bash
python3 ~/.claude/lib/plan_generator/checkpoint_validator.py <role> <output.json>
```

---

## Hard Rules — Common Subset (ALL agents)

> Common hard rules apply to EVERY touring agent. Individual agents add role-specific rules on top.

1. **Pre-flight FIRST** — `touring doctor` + `touring status` + `touring e2e` before anything else
2. **VP-Scout MANDATORY** — all applicable chains for every finding/integration before reporting
3. **CLI over inference** — `touring index find` before assuming any symbol exists or is absent
4. **Blast radius always** — `touring ast blast` for every file in the change/analysis set
5. **Wiring audit always** — `touring wiring audit` for every session involving module integrations
6. **Memory recall always** — `touring memory recall` for past lessons on the same topic
7. **Gotcha check always** — `touring gotcha match <file>` for every file analyzed or modified
8. **RL reward MANDATORY** — `touring learning reward` after every successful action (NEVER SKIP)
9. **No false positives** — VP-Scout BLOCKED_* items are removed from output with explicit reason cited
10. **JSON only** — when invoked as TACO subagent, return ONLY raw JSON (first char `{`, last char `}`)
11. **CHECKPOINT enforced** — output REJECTED if required fields missing (see Checkpoint Gate above)
12. **Evidence citations required** — every finding/decision cites specific CLI output or file:line reference

---

## Discovery Depth Reference (Common)

Standard CLI commands by discovery need:

| Need | Commands |
|------|----------|
| Symbol location | `touring index find <sym> -j` + `touring ast find <sym> -j` |
| File structure | `touring ast overview <file> -j` |
| Blast radius | `touring ast blast <file> -j` |
| Orphan symbols | `touring wiring orphans -j` |
| Integration scores | `touring wiring modules -j` |
| File wiring | `touring wiring score <file> -j` |
| Full wiring audit | `touring wiring audit -j` |
| E2E health (quick) | `touring e2e -j` (~50ms) |
| E2E health (standard) | `touring e2e --depth standard -j` (~500ms) |
| E2E health (deep) | `touring e2e --depth deep -j` (~2s) |
| Past patterns | `touring memory recall "<query>" -j` |
| File gotchas | `touring gotcha match <file> -j` |
| Compilation truth | `cargo check --workspace 2>&1 \| grep "^error\[" \| wc -l` |
| Feature gates | `grep -r 'feature = "<name>"' --include="Cargo.toml" -l` |

**Depth levels by task type:**

| Depth | When | Commands |
|-------|------|----------|
| Minimal | Quick symbol lookup | index find + ast find |
| Standard | Integration analysis | blast + wiring + e2e standard |
| Deep | Architecture / refactor | e2e deep + full wiring + all-file blast |

---

## D7 RL Feedback Loop — Known False Positive Check (ALL agents)

Before analyzing any finding or opportunity, check if it is a known FALSE POSITIVE.

### MANDATORY KEY FORMAT (CRITICAL — wrong format = FP loop broken)

FP memory entries MUST use these key formats:
```
fp:task:<task_id>:<short_name>     — FP específico de task (ex: fp:task:S-2:orphan_false)
fp:pattern:<pattern_name>          — Padrão de FP recorrente (ex: fp:pattern:wiring_stale)
fp:file:<basename>:<reason>        — FP associado a arquivo (ex: fp:file:hook_runtime:no_unwrap_prod)
```

**WRONG** (caminhos de arquivo NÃO são entradas FP válidas):
```
fp:task:.claude/rust/crates/...    ← ERRADO: isso é um caminho de arquivo
```

### D7 Pre-Check Protocol

```bash
# Check known FP PATTERNS before scouting
touring memory recall "fp:pattern:" -j | jq '.entries[:10]'
touring memory recall "fp:task:" -j | jq '.entries[:10]'

# IMPORTANT: If entries returned have values that look like file paths or edit actions
# (e.g. "edited:Edit:...", ".claude/rust/..."), those are NOT real FP records.
# The D7 FP loop was broken — verify with: jq '.entries[] | select(.key | startswith("fp:pattern:"))'
# Real FP entries have values like: "wiring stale: grep found consumer in plan_mode/enter.rs"

# If finding matches known FP pattern → BLOCKED_FP — include in report with FP evidence
```

**FP Feedback Loop — when FP detected during implementation:**
```bash
# Use CORRECT key format — task-specific:
touring learning reward orchestrate -1.0 "false_positive: <task_id> rejected at <phase>"
touring memory store "fp:task:<task_id>:<short_name>" "<reason_why_it_was_false>" --tier semantic --type lesson

# Use CORRECT key format — recurring pattern:
touring memory store "fp:pattern:<pattern_name>" "<description of pattern and why it's a FP>" --tier semantic --type lesson
```

### Known FP Patterns (cataloged 20/04/2026)

```
fp:pattern:orphan_wiring_stale     — wiring DB pode ter staleness; sempre confirmar via grep
fp:pattern:plan_doc_as_state       — plan docs são INTENÇÃO não estado; usar cargo check
fp:pattern:homonymia_aco           — ACO em touring-simd ≠ ACO em touring-hooks
fp:pattern:compilation_inference   — nunca inferir erros de compilação de plan docs
fp:pattern:feature_consumer_check  — feature opcional pode já estar ativada por consumer
```
