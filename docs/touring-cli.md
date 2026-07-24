# cli-anything-touring v2.0.0 — Behavioral Rules

> Priority: 80 — Apply to ALL sessions. Complements touring-system.md.
> The CLI is a **fast path** to Touring intelligence — use it when MCP is overkill or slow.

## When to Use CLI vs MCP

| Situation | Use CLI (`Bash` tool) | Use MCP (`mcp__touring__*`) |
|-----------|----------------------|----------------------------|
| Quick symbol lookup | `touring index find Name -j` | `touring_ast_find` |
| Blast radius check | `touring ast blast file -j` | `touring_graph(blast_radius)` |
| Wiring orphan check | `touring wiring orphans -j` | (no MCP equivalent) |
| Integration score | `touring wiring score module -j` | (no MCP equivalent) |
| Memory recall | `touring memory recall "query" -j` | `touring_memory_recall` |
| PII scan | `touring cortex pii "text" -j` | `touring_scan_pii` |
| Complex reasoning | — | `touring_mcts_search`, `touring_decompose` |
| Speculative validation | — | `touring_speculate` |
| AST surgery | — | `touring_ast_edit` |
| Unified system dashboard | `touring status -j` | (no MCP equivalent) |
| System diagnostics | `touring doctor -j` | (no MCP equivalent) |
| Memory store | `touring memory store key val -j` | `touring_memory_store` |
| Memory listing | `touring memory list --limit 10 -j` | `touring_memory_list` |
| Gotcha add | `touring gotcha add pattern desc -j` | (no MCP equivalent) |
| Gotcha match | `touring gotcha match file -j` | `touring_gotcha_match` |
| Wiring audit | `touring wiring audit -j` | (no MCP equivalent) |
| Learning reward | `touring learning reward tool 1.0 -j` | `touring_online_learn` |

**Rule**: Prefer CLI for read-only queries (<10ms). Prefer MCP for write operations and complex analysis.

## Mandatory CLI Checks

### Before Creating New Modules
```bash
# Check wiring status of the project — are there existing orphans?
touring wiring status -j | jq '.orphan_count'
```

### Before Editing Files
```bash
# Check blast radius
touring ast blast src/target_file.rs -j | jq '.blast_radius'
# Check wiring score of the module
touring wiring score src/target_file.rs -j | jq '.integration_score'
```

### After Creating pub Symbols
```bash
# Verify the new pub symbols are tracked
touring wiring orphans -j | jq '.[] | select(.module_file == "src/new_module.rs")'
```

### After Completing a Feature
```bash
# Full wiring audit — no orphans should remain
touring wiring status -j
# Verify integration scores are 1.0 for all modified modules
touring wiring modules -j | jq '.[] | select(.integration_score < 1.0)'
```

### TACO Phase 0: System Pre-flight
```bash
# 1. System health (daemon alive, circuit closed)
touring doctor -j | jq '.[] | select(.status != "ok")'

# 2. System dashboard (index, wiring, RL)
touring status -j | jq '{idx: .index.symbol_count, orphans: .wiring.orphan_count, rl_reward: .learning.ema_reward}'
```

### TACO Phase 4: Knowledge Capture
```bash
# Persist lesson learned
touring memory store "lesson:category:summary" "detailed description" --tier semantic --type lesson

# Inject RL reward for successful tool use
touring learning reward edit 1.0 "successful refactor"

# Register new gotcha if discovered
touring gotcha add "pattern" "description of the pitfall" --severity high
```

## Composable Pipelines

These patterns work in Bash via the `Bash` tool:

```bash
# Find all orphan pub symbols in the project
touring wiring orphans -j | jq -r '.[] | "\(.module_file):\(.symbol_name)"'

# List modules with low integration (< 50%)
touring wiring modules -j | jq '.[] | select(.integration_score < 0.5) | "\(.file_path): \(.integration_score * 100)%"'

# Find symbol definition + blast radius in one pipeline
touring index find MCTSEngine -j | jq '.[0].file_path' | xargs touring ast blast -j

# Check drift across all tracked metrics
touring evolution drift -j | jq '.metrics | to_entries[] | select(.value.trend == "degrading")'

# Memory entries with high access count (hot knowledge)
touring memory list -n 10 -s access_count -j | jq '.[] | "\(.key): x\(.access_count)"'

# Cognitive metrics snapshot
touring cognitive metrics -j | jq 'del(.status, .note)'

# Full system health check (TACO pre-flight)
touring doctor -j | jq -r '.[] | "\(.status | ascii_upcase) \(.name): \(.detail)"'

# Dashboard one-liner for prompts
touring status -j | jq -r '"idx:\(.index.symbol_count) orphans:\(.wiring.orphan_count) rl:\(.learning.ema_reward)"'

# Wiring audit — all issues in one view
touring wiring audit -j | jq '.orphans[], (.low_score_modules[] | "LOW: \(.file_path): \(.integration_score)")'

# Memory top-K most accessed entries
touring memory list --limit 10 --sort access_count -j | jq '.[] | "\(.key): x\(.access_count)"'

# Gotcha stats summary
touring gotcha stats -j | jq '"total: \(.total), resolved: \(.resolved), active: \(.total - .resolved)"'
```

## Sub-App Reference (~71 commands)

| Sub-App | Commands | Tier | Latency |
|---------|----------|------|---------|
| `index` | search, status, find, files | T1 SQLite | <10ms |
| `ast` | find, overview, blast | T1 SQLite | <10ms |
| `memory` | recall, store, list, stats | T1 SQLite | <10ms |
| `evolution` | insights, drift, tools | T1 SQLite | <10ms |
| `gotcha` | add, list, match, stats | T1 SQLite | <10ms |
| `wiring` | status, orphans, score, modules, audit | T1 SQLite | <10ms |
| `cortex` | pii, classify | T2 subprocess | <50ms |
| `context` | compile | T2 subprocess | <50ms |
| `flywheel` | status | T1 SQLite | <10ms |
| `cognitive` | metrics, engines | T2/T3 MCP | <200ms |
| `suggest` | next, skill | T3 MCP | ~200ms |
| `session` | start, checkpoint, list, assess | T3 MCP | ~200ms |
| `decompose` | create, add, get, update, validate, status | T3 MCP | ~200ms |
| `shadow` | validate | T3 MCP | ~200ms |
| `mcts` | search | T3 MCP | ~200ms |
| `learning` | status, reward | T3 MCP | ~200ms |
| `incremental` | status | T1 SQLite | <10ms |
| `mask` | test | T3 MCP | ~200ms |
| `meta` | status, doctor, --help, --version | T1-T3 mixed | <50ms |

## JSON Mode

All commands support `-j`/`--json` for machine-readable output. **Always use `-j` when piping to `jq`**.

Global JSON mode: `touring -j <subcommand>` applies to all subcommands.
