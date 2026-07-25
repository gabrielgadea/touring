# Touring Agents (TACO Subagent Pool)

> Six specialized agents under the TACO protocol. Invoke via the `Agent` tool. All return raw JSON (no markdown). Consult when delegating scout/architect/engineer/audit/scribe work.

## Agent Pool

| Agent | When to use | Tools |
|-------|------------|------|
| **touring-scouter** | Scouting, integration discovery, blast radius, VP-Scout verification, orphan symbols | Bash, Glob, Grep, Read, LS |
| **touring-architect** | Architecture design, integration planning, MCTS planning, Context7 best practices | + WebFetch, TodoWrite, WebSearch |
| **touring-engineer** | Feature implementation, refactoring, VGP-verified code generation, speculative validation | + Edit, Write (`mode="acceptEdits"`) |
| **touring-auditor** | Cross-audit, purpose fidelity verification, E2E test creation, scope maximization | + Edit, Write, TodoWrite |
| **touring-scriber** | Documentation, changelogs, ADRs, design decisions, institutional memory | + Edit, Write |

## Invocation Patterns

```python
# Scouter — scouting and orphan symbol discovery
Agent(subagent_type="touring-scouter", prompt="analyze wiring orphans in touring-hooks")

# Architect — design with Context7
Agent(subagent_type="touring-architect", prompt="design architecture for feature X with Context7 best practices")

# Engineer — VGP-verified implementation (REQUIRES mode="acceptEdits")
Agent(subagent_type="touring-engineer", mode="acceptEdits", prompt="implement feature Y with VGP")

# Auditor — full cross-audit
Agent(subagent_type="touring-auditor", prompt="audit module Z for purpose fidelity and integration")

# Scriber — full documentation
Agent(subagent_type="touring-scriber", prompt="document all changes from session <id>")
```

## Delegation Rules

1. **Always** invoke `Skill("Touring")` before delegating to an agent
2. **Never** delegate without first running `touring doctor -j` + `touring status -j`
3. **Engineer agents must use `mode="acceptEdits"`** — without it the agent cannot edit files and returns `composite_score=0`
4. **Pure JSON return** — agents return only JSON, never markdown
5. **Checkpoint validation** — run `~/.claude/lib/plan_generator/checkpoint_validator.py <role> <output.json>` after each agent task

## TACO Phase → Agent Mapping

| Phase | Agent | Mode | Purpose |
|-------|-------|------|---------|
| FASE 1 | touring-scouter | Bash, Glob, Grep, Read, LS | Discovery + VP-Scout chains |
| FASE 2 | touring-architect | + WebFetch, TodoWrite, WebSearch | Architecture + MCTS + Context7 |
| FASE 4.5 | touring-auditor | + Edit, Write, TodoWrite | Pre-implementation gate (FALSE_POSITIVE filter) |
| FASE 5 | touring-engineer | `mode="acceptEdits"` | VGP-verified implementation |
| FASE 6 | touring-auditor + code-reviewer | parallel | Cross-audit (read-only) |
| FASE 7 | touring-scriber | + Edit, Write | Final documentation |

## Quick Reference — Common Agent Tasks

| Task | Agent | Prompt template |
|------|-------|----------------|
| Find orphan symbols | touring-scouter | `"find orphan pub symbols in <module>"` |
| Map blast radius | touring-scouter | `"analyze blast radius for <file>"` |
| VP-Scout verification | touring-scouter | `"run VP-Scout verification on <finding>"` |
| Design architecture | touring-architect | `"design architecture for <feature> with Context7"` |
| MCTS planning | touring-architect | `"run MCTS search for <architecture_state>"` |
| Implement feature | touring-engineer | `"implement <feature> with VGP verification"` |
| Refactor module | touring-engineer | `"refactor <module> maintaining wiring integrity"` |
| Cross-audit | touring-auditor | `"audit <module> for purpose fidelity and integration"` |
| Create E2E tests | touring-auditor | `"create E2E tests proving <feature> functionality"` |
| Document changes | touring-scriber | `"document all changes from <session>"` |
| Generate changelog | touring-scriber | `"generate changelog for version X.Y.Z"` |

## Parallel Execution Guidance

When FASE 5 has 3+ engineers, distribute scope to minimize file-level conflicts:

**Strategy A — by CRATE (preferred for multi-crate)**:
```
Engineer-1: touring-analysis  (crates/touring-analysis/src/)
Engineer-2: touring-hooks     (crates/touring-hooks/src/)
Engineer-3: touring-cognitive (crates/touring-cognitive/src/)
→ each engineer edits files of ONE crate → zero conflict
```

**Strategy B — by MODULE (within same crate)**:
```
Engineer-1: pre_tool_use/  (entry module)
Engineer-2: task_list/      (data module)
Engineer-3: plan_mode/       (output module)
→ disjoint by directory → conflict risk ~0
```

**Anti-pattern**: multiple engineers editing the same file (e.g., `hook_runtime.rs`) → merge conflicts, degraded composite_score, respawn required.

## PARCER Profiles (S3 — Wave 2026-05-08)

Each agent is defined by a **PARCER profile** (6-dimension ESAA schema):

| Dimension | Purpose |
|-----------|---------|
| **Persona** | Role description + identity constraints + failure default |
| **Audience** | Primary consumer (orchestrator) + calibration |
| **Rules** | Hard (mandatory) + soft (best-effort) behavioral rules |
| **Context** | What the agent injects/never injects before execution |
| **Execution** | Ordered phase sequence (Phase 0 → N) |
| **Response** | Output format schema (`schema_ref`) |

**Profile locations** (source of truth):
```
~/.claude/agents/touring-scouter.parcer.yaml
~/.claude/agents/touring-architect.parcer.yaml
~/.claude/agents/touring-engineer.parcer.yaml
~/.claude/agents/touring-auditor.parcer.yaml
~/.claude/agents/touring-scriber.parcer.yaml
```

**JSON Schema** (structural validation): `~/projects/touring/crates/touring-server/schemas/parcer-profile.schema.json`

**CLI validation**:
```bash
touring profile validate --agent scouter   # validate scouter profile
touring profile list                         # list all profiles
touring profile diff --agent engineer        # diff against memory (REGRA #11)
```

**SubagentStop hook** (D3.4): `run_subagent_stop_gate` in `team_hooks.rs` validates the subagent's final JSON output against its PARCER `response.format.schema_ref` contract. On validation failure, the hook blocks the stop with exit code 2 and feedback explaining the validation error.

| Role | Required output fields | Key constraint |
|------|----------------------|----------------|
| scouter | `status`, `findings` | — |
| architect | `status`, `context_snapshot`, `confidence >= 0.5` | confidence minimum |
| engineer | `status`, `composite_score >= 1.0` | composite ≥ 1.0 |
| auditor | `status`, `confidence >= 0.8`, `e2e_proof` | confidence ≥ 0.8 |
| scriber | `status`, `documentation_created` | — |

## Post-Agent Verification (V1-V4)

After each engineer agent completes, the orchestrator runs:

- **V1 Output Parse**: agent JSON parses cleanly; `status="completed"`; `composite_score >= 1.0`
- **V2 Expected Files**: `expected_files: [...]` listed and exist on disk
- **V3 Compilation**: `cargo check --workspace 2>&1 | grep "^error\[" | wc -l` (Rust engineers only)
- **V4 Wiring Orphans**: `touring wiring orphans -j | jq '.count'` compared to baseline

**Auto-respawn** allowed once per agent; respawn with ONLY the failing files/errors as context.

See `~/.claude/rules/TACO-subagent.md` for the complete protocol.
