<!-- touring-cli-index-tiers-2-3.md — movido do SKILL/rules em 08/10/2026 (compactação 65). VERBATIM: TIER 2, TIER 3, Context Windows e a tabela Per-Module Detailed Reference. -->

## TIER 2 — DIAGNÓSTICO (saúde do sistema)

| ★★★★☆ | Comando | Output Chave | Quando |
|-------|---------|--------------|--------|
| `touring doctor -j` | Health check | daemon_socket, daemon_health, circuit_breaker, project_db | Antes de fases críticas |
| `touring status -j` | Dashboard | index.symbol_count, wiring.orphan_count, learning.ema_reward, **health_delta** (W16), **composite_health_score** (W8 S3) | Session start |
| `touring synergy [report\|wired\|opportunities] [-j] [--with-metrics]` **W8 S6 + W9 S9** | Cross-subsystem wiring observability + live counter enrichment | 43 wired_pairs ativos + 7 deferred opportunities | Auditar synergy interno + medir activity |
| `touring gate-metrics -j` | Gate metrics | pre_edit_fast_path, rkyv_dispatch_count, tantivy_upsert_count, **health_delta_*** (W12-13), **query_cache_*** (W17-18), **t3_turn_first_passed/t3_turn_fused** (T3-B, 25/08) | L7-B observability |
| `touring learning status` | RL status | LinUCB arms, EMA reward, converging state | Monitorar aprendizado |
| `touring health-delta status [path]` **W15** | Per-path streak state OR aggregate counters | regression_streak, warning_hint, alert_threshold | Inspecionar trend por arquivo |
| `touring health-delta reset <path>` **W15** | Clear streak+pre_health | `{reset:true,file_path:<path>}` | Após refactor checkpoint |

## TIER 3 — INTELLIGENCE (análise profunda)

| ★★★★☆ | Comando | Output | Quando |
|--------|---------|--------|--------|
| `touring wiring impact <symbol> [--depth N]` | **F1** Transitive impact | `{direct_consumers, max_depth, consumers}` | Análise de blast radius de símbolo específico |
| `touring wiring cycles [--min-depth N] [--format json\|text]` | **F2** Cycle detection | `{cycle_count, cycles: [{path, depth}]}` | Detectar ciclos de dependência (Tarjan SCC) |
| `touring ast blast-cross-feature <file>` | Cross-feature blast | Símbolos gated por feature + features afetadas | Multi-crate analysis |
| `touring ast rust-semantic <file.rs>` | Deep Rust semantics (syn) | generics, trait bounds, lifetimes, derives, unsafe/async counts, semantic_complexity | Before editing Rust — match surrounding conventions |
| `touring ast format-rust <file.rs>` | rustfmt-clean output | Formatted Rust source | After generating/editing Rust; no `rustfmt` binary needed |
| `touring ast workspace-info [<dir>]` | cargo_metadata intel | packages, features, dependents_of, packages_with_feature | Decide target crate; cross-crate blast radius |
| `touring ast grep <file> <pattern> [--rewrite <r>] [--lang <name>]` | Polyglot structural search + rewrite (ast-grep) | `{matches:[{text,start_line,start_col,metavars}]}` ou `{rewritten, source}` | JS/TS/Python/Go/etc — metavars `$VAR` / `$$$VAR` |
| `touring ast highlight <file> [--lang N] [--start N] [--end N]` **W5** | syntect ANSI rendering | colored source ou plain (NO_COLOR) | Visualizar source com syntax color |
| `touring wiring audit -j` | Full audit | orphans + modules com score < 1.0 | FASE 4.5 audit |
| `touring wiring chains [--rebuild]` | Functional chains | source→sink module relationships | Entender integração |
| `touring file-knowledge extended <file>` | 23 campos metadata | cognitive_score, community_id, modularity_score, etc | Análise profunda |
| `touring tantivy search "<query>"` | BM25 search | Ranked hits com cognitive_score | Symbol search |
| `touring cognitive metrics` | Cognitive runtime status | `has_graph`/`has_predictor`/`initialized` (flags de disponibilidade do runtime; node/edge count + focus_cache hit_rate são enhancement pendente) | Verificar runtime cognitivo ativo |

---

## Context Windows

| Tipo | Latência | Quando Usar |
|------|----------|-------------|
| CLI (`touring`) | <10ms | Read-only queries (index, wiring, memory recall) |
| MCP (`mcp__touring__*`) | ~200ms | Write ops (store, decompose, suggest) |
| Bash (speculate) | <200ms | Validação especulativa |

**Regra**: Preferir CLI para queries readonly (<10ms). MCP para writes e análise complexa.

---

## Per-Module Detailed Reference (consulta sob demanda)

| # | Módulo | Escopo | Path |
|---|--------|--------|------|
| 1 | **Overview** | Arquitetura 3-camadas, daemon actor pattern, dispatch table, global flags, formato wire | `references/touring-cli-overview.md` |
| 2 | **Hooks** | 24 lifecycle hooks + 2 neural | `references/touring-cli-hooks.md` |
| 3 | **Intelligence** | Code analysis: index, ast, wiring, file-knowledge extended, cognitive | `references/touring-cli-intelligence.md` |
| 4 | **Tasks** | session, decompose (workflow B3-B6), diary (W6), memory, tantivy FTS | `references/touring-cli-tasks.md` |
| 5 | **RL & Quality** | RL suggest/shadow/mcts/learning + Quality evolution/gotcha/flywheel/e2e/gate-metrics + 9 Predictive Wave Counters | `references/touring-cli-rl-quality.md` |
| 6 | **Generate** | touring-generator (24 subcommands, typestate) + L7-B (inferlets WASM, async jobs, mpatch fuzzy, 4 MCP tools) | `references/touring-cli-generate.md` |
| 7 | **Meta** | Meta-comandos, tabela resumo (~120 comandos), TACO workflow phase 0-4, integridade do hook registry | `references/touring-cli-meta.md` |
