# Touring CLI — Index & CLI Command Ranks (slim)

> **Auto-load** | **Version**: v5.3 (slim) | **Touring**: v30.3.0
> **TIER 4-9 (Session / Generate / Learning / Hooks / Search / Utility)**: `~/.claude/skills/Touring/references/touring-cli-tiers-4-9.md` (load on demand)
> **Per-module deep reference** (7 módulos): `~/.claude/skills/Touring/references/touring-cli-{overview,hooks,intelligence,tasks,rl-quality,generate,meta}.md`
> **Mirror full** (skill master): `~/.claude/skills/Touring/SKILL.md`

This rule keeps **TIER 1-3** (consulted every session) + Quick Cheatsheet (most-used commands) + Golden Rules. TIER 4-9 loaded on demand via the reference above.

---

## TIER 1 — CRÍTICOS (sempre usar, impactam diretamente qualidade)

| ★★★★★ | Comando | Porque | Como Usar |
|--------|---------|--------|-----------|
| `touring ast meta <file> --depth summary -j` | File metadata first | blast_radius/quality_score/cognitive_score/fan_in/fan_out **para arquivos indexados**; em cache-miss cai para `on_disk_fallback` (só metadata estrutural, esses campos vêm `null`). Para valores garantidos use `ast tdg` (quality), `ast blast` (blast radius), `file-knowledge extended` (cognitive) | Obrigatório antes de qualquer Edit |
| `touring pre-edit` | Pre-edit hook | Score composto 0-1 com CILA budget + rayon parallel signals | Corre antes de cada Edit, score deve ser >= 0.8 |
| `touring ast blast <file>` | Blast radius | Árvore completa de dependências antes de mudanças | Sempre antes de refactors L3+ |
| `touring index find <symbol>` | Symbol lookup (VGP) | Verifica se símbolo existe antes de criar/usar | Sempre antes de gerar código |
| `touring wiring orphans -j` | Orphan detection | Identifica pub symbols sem consumidores (potencialização) | After creating new pub fn/mod |
| `touring e2e -j` | E2E health | Score composto 0-1 de todo o sistema | Antes de mudanças arriscadas |
| `touring explore <topic>` | Loop-until-dry exploration (F1/CCE) | Ledger multi-lente persistente + contrato de convergência — "uma rodada nunca basta" institucionalizado | Antes de planejar/afirmar cobertura de um tema |
| `touring adw run <name> --var k=v` | ADW runner durável (F0) | Workflows declarativos (.touring/adw/*.toml) com journal fsync'd, `--resume-run`, Class-D, ZTE, racing; `lint`/`test`/`from-template`/`race` | Trabalho multi-nó com agentes headless gatados |
| `touring factory route\|start "<ticket>"` | Factory router (F4) | Ticket → ADW da library, determinístico-primeiro + RL feedback; KPIs `touring.adw.*` em `touring kpi` | Intake de tarefas para a esteira ADW |

## TIER 2 — DIAGNÓSTICO (saúde do sistema)

| ★★★★☆ | Comando | Output Chave | Quando |
|-------|---------|--------------|--------|
| `touring doctor -j` | Health check | daemon_socket, daemon_health, circuit_breaker, project_db | Antes de fases críticas |
| `touring status -j` | Dashboard | index.symbol_count, wiring.orphan_count, learning.ema_reward, **health_delta** (W16), **composite_health_score** (W8 S3) | Session start |
| `touring synergy [report\|wired\|opportunities] [-j] [--with-metrics]` **W8 S6 + W9 S9** | Cross-subsystem wiring observability + live counter enrichment | 43 wired_pairs ativos + 7 deferred opportunities | Auditar synergy interno + medir activity |
| `touring gate-metrics -j` | Gate metrics | pre_edit_fast_path, rkyv_dispatch_count, tantivy_upsert_count, **health_delta_*** (W12-13), **query_cache_*** (W17-18) | L7-B observability |
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

## QUICK CHEATSHEET — Comandos mais usados

```bash
# PRE-EDIT (OBRIGATÓRIO)
touring ast meta <file> --depth summary -j   # file metadata first
touring ast blast <file>                     # blast radius
touring pre-edit                             # score >= 0.8
touring index find <symbol>                  # symbol exists? (VGP)

# DIAGNOSTICS
touring doctor -j                            # health check
touring status -j | jq '{index, orphans, rl, composite_health_score}'
touring synergy -j                           # wired_pairs + opportunities
touring gate-metrics -j                     # L7-B metrics

# MASTERS (Layer-3)
touring explore <topic>                      # loop-until-dry exploration (CCE)
touring adw run <name>                       # durable agent workflow (spec+journal)

# WIRING
touring wiring audit -j                      # full audit
touring wiring orphans -j                    # orphans
touring wiring suggest --top 20 -j           # suggestions
touring wiring impact <symbol> --depth 2     # blast of symbol
touring wiring cycles --min-depth 2          # Tarjan SCC

# AST DEEP
touring ast rust-semantic <file.rs>          # syn semantics
touring ast workspace-info                   # cargo_metadata
touring ast grep <file> <pattern> --rewrite <r>  # polyglot rewrite
```

**TIER 4-9 commands** (session/decompose/memory/generate/tantivy/evolution/inferlets/jobs): see `~/.claude/skills/Touring/references/touring-cli-tiers-4-9.md`.

---

## REGRAS DE OURO

1. **FILE METADATA FIRST** — `touring ast meta <file> --depth summary` antes de qualquer Edit
2. **SEMPRE** executar `touring doctor -j` antes de atividades críticas
3. **SEMPRE** usar `touring index find` antes de criar novos símbolos
4. **SEMPRE** usar `touring shadow validate` (TIER 5) antes de Edit/Write
5. **SEMPRE** usar `touring wiring audit` após criar novos módulos pub
6. **NUNCA** ignorar orphan symbols — indicam código não utilizado
7. **SEMPRE** persistir lessons aprendidas via `touring memory store` (TIER 4)
8. **USAR** `touring evolution` (TIER 6) para atualizar padrões após erros
9. **VERIFICAR** sempre se o comando existe na tabela acima ou no reference TIER 4-9 antes de usar
10. **VGP** — verificar símbolos via `touring generate verify --symbol <name>` (TIER 5) antes de gerar código
11. **AGENTS** — usar touring-scouter/architect/engineer/auditor/scriber para tarefas especializadas

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
