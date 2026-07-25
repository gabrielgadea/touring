# Touring CLI — TIER 4-9 (load on demand)

Companion reference for `~/.claude/rules/touring-cli-index.md`. The rule keeps TIER 1-3 (CRÍTICOS / DIAGNÓSTICO / INTELLIGENCE) inline — consulted every session. This file holds TIER 4-9 (Session/Generate/Learning/Hooks/Search/Utility) loaded on demand when those workflows are active.

For per-module deep CLI reference see `touring-cli-*.md` siblings in this directory.

## TIER 4 — SESSION / CHECKPOINT (persiste estado)

| ★★★★☆ | Comando | Uso | Flag |
|--------|---------|---|------|
| `touring session start [id] type "<obj>"` | Inicia sessão | Carrega knowledge + RL state | Documente objective |
| `touring session assess [id]` | Avalia qualidade | Composite score + phase breakdown | Session close |
| `touring decompose create <type> "<desc>"` | Cria DAG | Pln2: --origin=<val>, Pln3: --cila-level=N | Sempre para tasks complexas |
| `touring decompose add <task> <subtask> [deps]` | Adiciona subtask | Deps via vírgula: sub1,sub2 | Task decomposition |
| `touring memory store <key> <val> --tier semantic` | Persiste lesson | Evita repetir erros | Antes de refactors |
| `touring mutation-test [--package P] [--threshold T] [--cache-only\|--force]` **T1+T2** | Mutation testing wrapper sobre cargo-mutants | Cache 7d em `<ws>/.touring-cache/mutation-test/`; alimenta R1 Testing + R2 KPI; advisory gate em `run_full_audit.sh` (skip se cargo-mutants ausente). Playbook: `docs/mutation-testing.md` |

## TIER 5 — CODE GENERATION (touring-generator pipeline)

| ★★★★☆ | Comando | Pipeline Stage | Output |
|--------|---------|----------------|--------|
| `touring generate list-kinds -j` | 30 GeneratorKind | Discovery | Lista completa |
| `touring generate verify --symbol <name>` | VGP verification | Verify | exists/not_found |
| `touring generate render <kind> [--vars '{}']` | Template render | Render | artifact preview |
| `touring generate plan-submit --file <path>` | Full pipeline | Draft→Verified→Rendered→Speculated→Committed | Atomic commit |
| `touring generate plan-speculate --file <path>` | Shadow validation | Speculate | shadow_validate score |

## TIER 6 — LEARNING / RL (feedback loop)

| ★★★★☆ | Comando | Efeito | Timing |
|--------|---------|--------|--------|
| `touring learning reward <tool> <val> [ctx]` | Injeta reward | Atualiza LinUCB + QTable | After successful action |
| `touring evolution drift -j` | Drift detection | alert_level: none\|degraded\|structural | Weekly check |
| `touring evolution insights -j` | Pattern insights | Tool effectiveness stats | Post-mortem |

## TIER 7 — HOOKS (Claude Code lifecycle)

| ★★★☆☆ | Comando | Hook | Quando |
|--------|---------|------|--------|
| `touring serve` | MCP server | — | Daemon startup (idle watchdog OPT-IN: `TOURING_IDLE_TIMEOUT_SECS>0`) |
| `touring pre-read` | Pre-read enrichment | Antes de Read | Context injection |
| `touring post-read` | Post-read learning | Depois de Read | Atualiza co-edit graph |
| `touring pre-write` | Speculative validation | Antes de Write | Anti-patterns check |
| `touring post-edit` | Quality tracking | Depois de Edit | Multi-language quality |
| `touring pre-grep` **D43** | Symbol enrichment | Antes de Grep | PascalCase/snake_case → 20 locations injected (P99=2ms) |
| `touring pre-glob` **D43** | Symbol enrichment | Antes de Glob | Delegates to pre-grep |
| `touring post-tool-failure` | Circuit breaker | After 5+ falhas | Auto-halt |
| `touring instructions-loaded` | Context injection | Session init | Stats do projeto |
| `touring cortex <event>` | Unified engine | Hook events | Fascicles dispatcher |

## TIER 8 — SEARCH / INDEX (read-only queries)

| ★★★☆☆ | Comando | Latência | Uso |
|--------|---------|---------|-----|
| `touring index status` | Index health | <10ms | Quick check |
| `touring index search <prefix>` | Prefix lookup | <10ms | Symbol discovery |
| `touring tantivy fuzzy "<query>" [dist]` | Fuzzy BM25 | <10ms | Misspellings |
| `touring tantivy suggest "<prefix>"` | Autocomplete | <10ms | Interactive search |
| `touring search symbols "<query>"` | BM25 rank | <10ms | Full-text search |

## TIER 9 — UTILITY (suporte)

| ★★☆☆☆ | Comando | Propósito | Exemplo |
|--------|---------|-----------|---------|
| `touring gotcha list [--file F]` | Pitfall DB | Known problems | `touring gotcha match <file>` |
| `touring memory recall "<query>"` | Memory search | FTS5 + cosine | Reuso de patterns |
| `touring diary write <agent> <entry>` | Agent diary | AAAK format | `--topic <topic> --aaak --project <p> --task <id> --subtask <id>` |
| `touring diary read <agent>` | Read diary | Filter entries | `--project <p> --task <id> --last N` |
| `touring diary projects <agent>` | List projects | W6 project index | Agent project history |
| `touring inferlets list -j` | WASM pools | Sandbox inference | L7-B feature |
| `touring jobs spawn <prog> [args]` | Background worker | Long-running tasks | Non-blocking ops |
| `touring decompose finalize <task>` | Archive task | Verifica completude | quality_threshold=N |
| `touring decompose ready [task]` | Ready subtasks | Pending com deps done | Filtro opcional |
| `touring health-delta status [path]` **W15** | Streak + counters | Per-path ou aggregate | `touring health-delta status src/foo.rs` |
| `touring health-delta reset <path>` **W15** | Clear streak+pre_health | Post-refactor checkpoint | `touring health-delta reset src/foo.rs` |

## QUICK CHEATSHEET — TIER 4-9 commands

```bash
# SESSION / DECOMPOSE
touring session start <id> type "<obj>"
touring session assess <id>
touring decompose create <type> "<desc>"
touring decompose add <task> <sub> [deps]

# MEMORY / LEARNING
touring memory store <key> <val> --tier semantic
touring memory recall "<query>"
touring learning reward <tool> <val> [ctx]

# GENERATE
touring generate list-kinds -j
touring generate verify --symbol <name>
touring generate plan-submit --file <plan>

# TANTIVY SEARCH
touring tantivy search "<query>"
touring tantivy fuzzy "<query>" 2
touring tantivy suggest "<prefix>"

# EVOLUTION
touring evolution drift -j
touring evolution insights -j

# INFERLETS (L7-B)
touring inferlets list -j
touring inferlets run <name> [<input>]

# JOBS (L7-B)
touring jobs spawn <prog> [args]
touring jobs poll <job_id>
touring jobs list
```
