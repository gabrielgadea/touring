# Touring CLI — Code Intelligence

> **Module**: 3/7 | **Version**: v4.27 | **Touring**: v30.3.0
> **Series**: Touring CLI Reference (consulta sob demanda) — `~/.claude/skills/Touring/references/touring-cli-*.md`
> **Index** (auto-load): `~/.claude/rules/touring-cli-index.md` (CLI RANKS Tier 1, 3, 8)
>
> **Last update**: Wave C (v4.27) added `touring assist` (10 handlers), `touring ssr` (semantic structural rewrite), `touring skip` (SkipContext region markers), `touring source-change` (transactional apply).

Comandos de **análise de código**: index (Symbol Store), AST (incl. blast-cross-feature), wiring (incl. F1 impact + F2 cycles), file-knowledge extended, cognitive engine. Use ANTES de editar — file metadata first é regra de ouro.

---

## 3. Index / AST (SymbolStore + Knowledge DB)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring index status` | `cli-index-status` | Saúde do índice de símbolos |
| `touring index search <query>` | `cli-index-search` | Busca prefixo no índice |
| `touring index find <symbol>` | `cli-index-find` | Encontra definições de símbolo |
| `touring index files [pattern]` | `cli-index-files` | Lista arquivos indexados |
| `touring index rebuild [--dir <path>]` | `cli-index-rebuild` | Reconstrói índice do diretório |
| `touring ast find <symbol>` | `cli-ast-find` | Lookup AST de símbolo |
| `touring ast overview <file>` | `cli-ast-overview` | Overview de símbolos em arquivo |
| `touring ast blast <file>` | `cli-ast-blast` | Análise de blast radius |
| `touring ast blast-cross-feature <file>` | `cli-ast-blast-cross-feature` | Blast radius com gating cross-feature: lista símbolos gated por feature flags + features afetadas (Wave cross-audit) |
| `touring ast highlight <file> [--lang N] [--start N] [--end N]` | (pure library, no daemon) | **Wave 5 v4.17** — syntect ANSI rendering. `--lang` override extension detection; `--start`/`--end` 1-indexed inclusive range; respeita `NO_COLOR` env var + `IsTerminal::is_terminal`. Solarized (dark) theme padrão. ~5-20ms cold, zero subsequente (Lazy SyntaxSet/ThemeSet). |
| `touring assist list-kinds` | (pure library) | Lista 10 assist handlers (add_missing_match_arms, auto_import, auto_wire, change_visibility, convert_to_guarded_return, extract_function, generate_impl, inline_call, merge_imports, move_module_to_file) |
| `touring assist applicable <file>:<line>:<col>` | (pure library) | Retorna assists aplicáveis na posição do cursor |
| `touring assist apply <kind> <file> <range>` | (pure library) | Aplica assist — produz SourceChange via Applier::commit() |
| `touring ssr --pattern <p> --replacement <r> [--path <f>] [--dry-run]` | (pure library) | **Wave B** — Semantic structural rewrite. Pattern `==>>` separator, VGP gate, Applier commit. |
| `touring skip list <file>` | (pure library) | Lista skip-regions em arquivo (// touring:skip-region markers) |
| `touring skip validate <file>` | (pure library) | Valida sintaxe de skip-regions |
| `touring source-change apply [--path <f>]` | (pure library) | Aplica SourceChange transactional via Applier::commit() |

## 10. Wiring Intelligence

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring wiring status` | `cli-wiring-status` | Summary de wiring |
| `touring wiring impact <symbol> [--depth N]` | `cli-wiring-impact` | **F1** Análise de impacto transitivo via BFS consumer walk. Retorna `{direct_consumers, max_depth, consumers: [{symbol, depth}]}`. Default depth=2. |
| `touring wiring cycles [--min-depth N] [--format json\|text]` | `cli-wiring-cycles` | **F2** Detecção de ciclos via Tarjan's SCC. `--min-depth` filtra por profundidade mínima. Formatos: `json` (máquina) ou `text` (human-readable). Retorna `{cycle_count, cycles: [{path: [node], depth}]}`. |
| `touring wiring orphans` | `cli-wiring-orphans` | Símbolos públicos sem consumidores |
| `touring wiring modules` | `cli-wiring-modules` | Scores de integração por módulo |
| `touring wiring score <file>` | `cli-wiring-modules` | Score de integração de um arquivo específico |
| `touring wiring audit` | `cli-wiring-orphans` + `cli-wiring-modules` | Auditoria completa: orphans + modules com score < 1.0 |
| `touring wiring suggest` | `cli-wiring-suggest` | Sugestões acionáveis de wiring baseadas em análise de orphans do FileKnowledgeDB |
| `touring wiring purpose <file>` | `cli-wiring-purpose` | Propósito funcional de um arquivo |
| `touring wiring community <file>` | `cli-wiring-community` | Community assignment (Louvain/Leiden) de um arquivo |
| `touring wiring chains [<file_path>] [--rebuild]` | `cli-wiring-chains` | Lista functional chains entre módulos. `--rebuild` reconstrói tabela; `<file_path>` filtra por módulo fonte. Retorna `{chain_count, chains: [{source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence}]}` (Wave cross-audit) |

**F1+F2 output examples**:

```bash
# F1: transitive impact analysis
./target/release/touring wiring impact HookRuntime --depth 2
# {"direct_consumers":68,"max_depth":1,"consumers":[...]}

# F2: cycle detection
./target/release/touring wiring cycles --format text
# cortex::cortex_dispatcher → cortex::pipeline_context → cortex::cortex_dispatcher
# cortex::cortex_dispatcher → cortex::hook_runtime → cortex::cortex_dispatcher
# cognitive::mcts_dispatcher → cognitive::pheromone_layer → cognitive::mcts_dispatcher
# generator::acquire_ctx → generator::commit_ctx → generator::acquire_ctx

./target/release/touring wiring cycles --min-depth 3 --format json
# {"cycle_count":2,"cycles":[{"path":["a","b","c","a"],"depth":3}]}
```

## 10b. File Knowledge (Extended Analysis — Wave cross-audit)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring file-knowledge extended <file_path>` | `cli-file-knowledge-extended` | Retorna 23 campos de metadados via LEFT JOINs: 10 base (file_path, language, line_count, symbol_count, read_count, last_read_at, content_hash, imports_json, symbols_json, notes) + 13 enrichment (cognitive_score, complexity_signal, fan_in/fan_out/doc_signal, integration_score, pub_symbol_count, import_count, re_export_count, blake3_hash, coverage_pct, community_id, modularity_score). Auto-cria tabelas ausentes (schema v8 self-heal). |

**Schema v8**: Adiciona 5 tabelas de enrichment — `cognitive_enrichment`, `module_ecosystem`, `file_blake3_registry`, `file_test_coverage`, `file_communities`.

**Self-healing**: Se tabelas estiverem ausentes (DB pré-v8), `CREATE TABLE IF NOT EXISTS` é executado inline — idempotente.

```bash
touring file-knowledge extended crates/touring-hooks/src/hook_runtime.rs -j
# → {"file_path": "...", "found": true, "cognitive_score": 0.72, "community_id": 3, ...}
```

## 11. Cognitive Engine

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring cognitive metrics` | `cli-cognitive-metrics` | Métricas do engine |
| `touring cognitive engines` | `cli-cognitive-engines` | Saúde dos sub-engines |

---

**Outros módulos**: [overview](touring-cli-overview.md) | [hooks](touring-cli-hooks.md) | [tasks](touring-cli-tasks.md) | [rl-quality](touring-cli-rl-quality.md) | [generate](touring-cli-generate.md) | [meta](touring-cli-meta.md)
