# Touring Workflows — Best Practices by Category

> Operational workflows for common Touring tasks. Consult after deciding which area you need to work in (PRE-EDIT, INTELLIGENCE, LEARNING, MEMORY, GENERATE, DECOMPOSE, TACO Phases). For one-line decisions consult the CLI ranks in `SKILL.md`.

## PRE-EDIT (mandatory before Edit/Write)

```bash
# ALWAYS execute in this order:
1. touring ast meta <file> --depth summary -j   # file metadata first
2. touring ast blast <file>                      # verify blast radius
3. touring pre-edit                              # score >= 0.8 required
4. touring index find <symbol>                   # VGP verification
```

**If `pre-edit` score < 0.8**:
- Analyze signals (CILA budget exhausted? rayon parallel?)
- Fix root cause; do not bypass

### TDG Grade Action Table

`touring ast tdg <file>` returns a letter grade (A+..F) computed from 6 orthogonal dimensions (complexity, coverage, duplication, churn, entropy, antipatterns). Use **before** Edit/Write to decide care level.

| Grade | Composite | Action |
|-------|-----------|--------|
| **A+** | `>= 0.95` | Edit freely |
| **A**  | `[0.90, 0.95)` | Edit freely |
| **B+** | `[0.85, 0.90)` | OK, consider light refactor |
| **B**  | `[0.80, 0.85)` | OK, consider light refactor |
| **C+** | `[0.75, 0.80)` | Cautious; plan mitigation |
| **C**  | `[0.70, 0.75)` | Cautious; plan mitigation |
| **D**  | `[0.60, 0.70)` | **STOP** — refactor before edit |
| **F**  | `< 0.60` | **STOP** — architectural review first |

### File Metadata Depth Levels

```bash
touring ast meta <file> --depth skeleton   # min: symbols + language + LOC
touring ast meta <file> --depth summary    # + quality + blast + fan + cognitive
touring ast meta <file> --depth full       # + call_graph + imports + todos + features
```

### Quality Metadata Triage

| Threshold | Action |
|-----------|--------|
| `blast_radius > 10` | Pause; ask for confirmation OR reduce scope |
| `quality_score < 0.5` | Focus on robustness OR justify the risk |
| Both critical | STOP — plan mitigation first |

## INTELLIGENCE (analysis before decisions)

```bash
# BLAST RADIUS
touring ast blast <file>                        # single file
touring ast blast-cross-feature <file>           # cross-feature deps

# WIRING AUDIT (weekly)
touring wiring audit -j                          # full orphans + modules
touring wiring orphans -j                        # orphan detection
touring wiring chains --rebuild                  # functional chains rebuild

# COGNITIVE + FILE KNOWLEDGE
touring cognitive metrics                        # graph health
touring file-knowledge extended <file>           # 23 metadata fields
```

## LEARNING (RL feedback loop)

```bash
# REWARD INJECTION (after every successful action)
touring learning reward orchestrate 1.0 "checkpoint_passed"
touring learning reward speculate 1.0 "shadow_validate_passed"
touring learning reward edit 1.0 "edit_quality_gates_passed"

# MONITOR
touring learning status                          # LinUCB state
touring evolution drift -j                       # structural alerts
touring evolution insights -j                    # tool effectiveness

# Anti-pattern: late reward injection
# DO: reward immediately after action
# DO: reward orchestrate -1.0 for false_positives
```

## MEMORY (knowledge persistence)

```bash
# Before L3+ refactors
touring memory store "refactor:<feature>" "<description>" --tier semantic --type lesson

# Recall patterns
touring memory recall "pattern: error handling"  # FTS5 + cosine
touring memory list --limit 20 --sort access_count

# Diary for agents
touring diary write <agent> "entry" --topic <topic> --aaak \
   --project <p> --task <id> --subtask <id>
touring diary read <agent> --project <p> --task <id> --last 5
touring diary projects <agent>                   # list project history
```

## GENERATE (code generation pipeline)

```bash
# DISCOVERY
touring generate list-kinds -j                   # 30 kinds
touring generate template-list -j                # 29 templates

# VERIFY (VGP, mandatory before generation)
touring generate verify --symbol <name>          # must pass first

# PIPELINE
touring generate render <kind> [--vars '{}']     # preview
touring generate plan-speculate --file <plan>    # shadow validate
touring generate plan-submit --file <plan>       # full commit (Draft→Verified→Rendered→Speculated→Committed)

# SCHEMA
touring generate schema-dump -j                  # JSON Schema for plans
```

## DECOMPOSE (task DAG lifecycle)

```bash
# CREATE
touring decompose create intent "implement feature" --origin=touring-cli --cila-level=3
touring decompose create bug "fix leak" --origin=external-agent
touring decompose create intent "rewrite auth" --origin=external-agent --cila-level=5

# ADD SUBTASKS (deps = comma-separated)
touring decompose add <task> sub_1 "research"
touring decompose add <task> sub_2 "implement"
touring decompose add <task> sub_3 "test" sub_1,sub_2

# VALIDATE + EXECUTE
touring decompose validate <task>                # detect cycles
touring decompose status                         # overview
touring decompose ready <task>                   # subtasks with deps done
touring decompose finalize <task> [N]            # archive + quality gate
```

### Decompose Bidirectional Flags (Pln2 + Pln3)

- `--origin=<val>` — explicit provenance (Pln2): `touring-cli`, `external-agent`, ...
- `--cila-level=<N>` — CILA refinement level (Pln3 R2), propagates to PlanModeSuggester

### task_digest + action_suggestions Pattern

Reuse this pattern for any integration that needs to surface Touring context at session start:

1. `task_digest::digest_pending_tasks(rt)` — lists tasks `mirrored_to_cc=0` in `additionalContext`
2. `detect_and_suggest_*(rt)` — 3 suggesters inject suggestions into `cc_action_suggestions`
3. CC acts with `suggestion_ref=<id>` → hook marks `consumed=1` → loop closed
4. Triple anti-loop: dedup on insert + consumed flag + digest `WHERE consumed=0`

## TACO Phase Protocol

| Level | Phases executed |
|-------|------------------|
| **L0-L1** | Solo mode — orchestrator resolves directly, zero subagents |
| **L2** | Phase 1 (scout) → Phase 5 (engineer) → validate |
| **L3** | Phase 1 → Phase 2 (architect) → Phase 5 → Phase 6 (audit) → validate |
| **L4+** | All phases (0, 1, 2, 3, 4, 4.5, 5, 6, 7) |

### FASE 0 — System Health Gate (BLOCKS everything)

```bash
cd <workspace_root>
cargo check --workspace 2>&1 | tail -5      # exit 0 required
touring doctor -j | jq '.[] | select(.status != "ok")'
```

| Condition | Action | Blocking |
|-----------|--------|----------|
| `cargo check` exit != 0 | report errors, BLOCK all phases | 🔴 YES |
| `daemon_socket = error` | report, fallback mode active | 🟡 DEGRADED |
| `knowledge_db = unhealthy` | BLOCK | 🔴 YES |
| `symbol_store = unhealthy` | BLOCK | 🔴 YES |
| `ema_reward = 0.0` + degraded | RL cold-start, BLOCK | 🔴 YES |
| `mean_td_error magnitude > 1e9` | RL overflow — degrade hint, continue | 🟡 DEGRADED |

### FASE 4.5 — Pre-Implementation Audit Gate (anti-FP)

Auditor can REJECT tasks marked FALSE_POSITIVE BEFORE engineers receive them.

| Pattern | Detection | Action |
|---------|-----------|--------|
| Task says "unwrap in production" but all unwraps in tests | grep test modules | REJECT |
| Task says "symbol X doesn't exist" but `touring index find` returns | `touring index find` | REJECT |
| Task says "compilation error" but `cargo check` exit = 0 | `cargo check` | REJECT |
| Task cites line N but file < N lines | `wc -l file` | REJECT |
| Task says "feature disabled" but consumer activated it | `touring wiring modules` | REJECT |
| Task says "orphan" but symbol has consumer=1 | `touring wiring orphans` | ACCEPT |

## Task Lifecycle

```
1. CREATE   → touring decompose create        → DAG composed
2. TRACK    → touring decompose add           → subtasks with depends_on
3. VALIDATE → touring decompose validate      → no cycles
4. EXECUTE  → tasks run + touring session     → checkpoint
5. ASSESS   → touring session assess          → quality scored
6. LEARN    → touring memory store + reward   → persistence
```

## Context Window Selection

| Channel | Latency | Use for |
|---------|---------|---------|
| CLI (`touring`) | <10ms | read-only queries (index, wiring, memory recall) |
| MCP (`mcp__touring__*`) | ~200ms | write ops (store, decompose, suggest) |
| Bash (speculate) | <200ms | speculative validation |

**Rule**: prefer CLI for read-only queries. MCP for writes and complex analysis.
