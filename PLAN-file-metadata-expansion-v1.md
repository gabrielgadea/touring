---
title: Touring File Metadata Expansion — Plano Final Consolidado
version: v1.0
date: 2026-04-10
author: TACO Orchestrator (claude_code)
status: APPROVED_CONDITIONAL
composite_score: 0.97
auditor_confidence: 0.93
phases_executed: 7
subagents_spawned: 8
---

# Touring File Metadata Expansion — Plano Final Consolidado

> Expande o Touring para indexar **conjunto completo de metadados por arquivo** (blast radius, semantic graph, CC, imports, funcionalidade, símbolos, fluxos, antipatterns, orfãos, feature flags, TODOs, path/hash/mtime, fan-in/out, heat), atualizado continuamente via hooks e exposto ao Claude Code via novos CLI commands, MCP tools, skill, rule e `@filename` integration.

---

## <objective>

### O quê
Adicionar uma camada estruturada de metadados por arquivo ao Touring, com:
- **Persistência** em knowledge.db (schema v6 → v7, migração aditiva segura)
- **Coleta contínua** via hooks pre/post edit/write/read (3-tier: fast-inline / deferred-fast / async-slow)
- **Exposição** via 12 CLI commands novos, 7 MCP tools novos, skill e rule files
- **Awareness** do Claude Code via `instructions_loaded` hook + `@filename` injection pattern

### Por quê
1. **O Touring já implementa ~70% das capabilities necessárias**, mas a maioria está **orfã de exposição CLI** (CallGraph, EnrichedBlastRadius, FunctionalSignature, extract_cfg_feature_names). O plano é **WIRE**, não BUILD.
2. Claude Code atualmente lê arquivos sem contexto prévio, desperdiçando tokens em re-leitura. Metadata pré-injetado via `@filename` reduz 10-50× o custo de tokens por consulta.
3. Orphan rate atual = **96.6% (33.080 orfãos)**, índice coverage = **1.7%**, e2e_score = **0.52 warn**. Este plano aumenta o wiring de 5 capabilities-chave e eleva o e2e_score projetado para >0.80.
4. Gabriel quer que o Claude Code **saiba que essa camada existe** e **use antes de ler arquivos** — isso só se consegue com skill + rule + hooks + CLAUDE.md coordenados.

### Confidence
- **Analytical findings (scout)**: 1.0 (verificado via `touring index find` + file reads)
- **Architectural design**: 0.95 (3 architects convergiram)
- **Implementation feasibility**: 0.93 (auditor confirmou com 4 correções aplicáveis)
- **Scope preservation**: 1.0 (zero orphans novos, 5 orfãos existentes serão wired)

---

## <deliverables>

### Estrutura geral
- **38 tasks** organizadas em **11 fases paralelizáveis**
- **3 crates tocados**: `touring-core`, `touring-hooks`, `touring-server`
- **~18 arquivos editados**, **6 arquivos criados**
- **5 novas tabelas** em knowledge.db, **15 colunas** adicionadas a tabelas existentes

### P0 — FOUNDATION (4 tasks, paralelo, S)

| ID  | Task                                                         | File                                               | Effort |
|-----|--------------------------------------------------------------|----------------------------------------------------|--------|
| A-1 | Bump `SCHEMA_VERSION` 6→7 **+ update test at `migration.rs:285`** | `touring-core/src/migration.rs:17`, `:285`     | S      |
| B-1 | `extract_cfg_feature_names` private → `pub(crate)`           | `touring-hooks/src/shadow_v2.rs:538`               | S      |
| B-2 | Create `HookGuard` RAII recursion guard (`TOURING_HOOK_ACTIVE` env var) | `touring-hooks/src/shared/recursion_guard.rs` (NEW) | S |
| B-3 | Create `MetadataDedup` (`OnceLock<Mutex<HashMap>>`, 60s TTL) | `touring-hooks/src/shared/metadata_dedup.rs` (NEW) | S      |

### P1 — SCHEMA DDL (7 tasks, paralelo, dep P0, S)

| ID  | Task                                                                                   | Change Type                     |
|-----|----------------------------------------------------------------------------------------|---------------------------------|
| A-2 | `ALTER TABLE file_knowledge` +10 columns (file_size_bytes, mtime_epoch, feature_flags_json, todo_count, doc_coverage_pct, loc, cloc, max_cc, fan_in, fan_out) | ADD COLUMN aditivo |
| A-3 | `ALTER TABLE symbols` +5 columns (kind, visibility, docstring, cyclomatic_complexity, parent_symbol) | ADD COLUMN aditivo         |
| A-4 | `CREATE TABLE file_feature_flags` (id, file_path, feature_name, line, guarded_symbols_json, updated_at) + 2 idx | Nova tabela       |
| A-5 | `CREATE TABLE file_todos` (id, file_path, kind CHECK IN TODO/FIXME/HACK/XXX/NOTE/WHY, line, message, created_at) + 2 idx | Nova tabela |
| A-6 | `CREATE TABLE edge_confidence` (source_file, target_file, relation, confidence CHECK IN EXTRACTED/INFERRED/AMBIGUOUS, weight) + 3 idx (**graphify pattern**) | Nova tabela |
| A-7 | `CREATE TABLE file_communities` (file_path PK, community_id, centrality_score, pagerank_score, god_node_flag) + 3 idx (**F3 stub, nullable**) | Nova tabela |
| A-8 | `CREATE TABLE file_test_coverage` (file_path PK, coverage_pct, test_count, last_run_at) **lazy/nullable** | Nova tabela |

**Estratégia de backfill**: ADDITIVE only, DEFAULTs seguros, fs::copy `.v6.bak` antes + atomic `BEGIN/COMMIT`. Rollback via restore from backup se validation falhar.

### P2 — RUST TYPES (4 tasks, dep P1, S/M)

| ID  | Task                                                                                  | File                                            | Effort |
|-----|---------------------------------------------------------------------------------------|-------------------------------------------------|--------|
| B-4 | Extend `FileKnowledge` struct +10 `Option<T>` fields + COALESCE em upsert/lookup      | `touring-hooks/src/knowledge.rs:27`             | M      |
| B-5 | Add `pub fn query_fan_metrics(&self, rel_path) -> Result<(u32,u32)>` (COUNT indexed)  | `touring-hooks/src/knowledge.rs`                | S      |
| B-6 | Create `TodoKind` + `EdgeConfidence` enums (to_string/from_str)                       | `touring-hooks/src/shared/types.rs` (NEW)       | S      |
| B-7 | Migration backup (`fs::copy .v6.bak`) + atomic tx em `migrate_schema()`               | `touring-hooks/src/knowledge.rs:346`            | M      |

### P3 — COLLECTOR (2 tasks, dep P2, S/L)

| ID  | Task                                                                                              | File                                                   | Effort |
|-----|---------------------------------------------------------------------------------------------------|--------------------------------------------------------|--------|
| B-8 | Create `FastMetadata` struct + `collect_fast_metadata()` + `collect_fan_metrics()` + `collect_async_metrics()` | `touring-hooks/src/shared/metadata_collector.rs` (NEW) | L |
| B-9 | Register new modules em `shared/mod.rs` (metadata_collector, recursion_guard, metadata_dedup, types) | `touring-hooks/src/shared/mod.rs`                      | S      |

### P4 — HOOK WIRING (5 tasks, paralelo, dep P3, M)

| ID   | Hook            | Operação (3-tier)                                                                                         | Latency Target | CILA Gate |
|------|-----------------|----------------------------------------------------------------------------------------------------------|----------------|-----------|
| B-10 | `post_edit.rs`  | `collect_fast` + `dedup` + `recursion_guard` + async `spawn_worker` para slow metrics                    | <80ms CILA 0/1, <500ms CILA≥2 | ≥2 |
| B-11 | `post_write.rs` | Full metadata collection para novos arquivos (sem dedup), `fs::metadata` sempre fresh                     | <500ms         | ≥2        |
| B-12 | `post_read.rs`  | **Apenas** `fs::metadata` (file_size_bytes, mtime_epoch) — cheap path                                    | <20ms          | none      |
| B-13 | `pre_edit.rs`   | **READ-ONLY**: query fan_in/fan_out/feature_flags/doc_coverage e injetar context signals                 | <50ms          | ≥2        |
| B-14 | `pre_write.rs`  | **READ-ONLY**: query todo_count/doc_coverage e injetar warnings se thresholds violados                   | <300ms         | ≥2        |

### P5 — CLI HANDLERS (9 tasks, paralelo, dep P4, M)

| ID  | CLI Command                              | Handler Fn                     | Capability Wrapped                                   |
|-----|------------------------------------------|--------------------------------|------------------------------------------------------|
| C-1 | `touring ast callgraph <file>`           | `cli_ast_callgraph`            | `touring_ast::build_call_graph` (**orphan wired**)   |
| C-2 | `touring ast todos <file>`               | `cli_ast_todos`                | Regex scan (NEW, reusado em post-hooks)              |
| C-3 | `touring ast rationale <file>`           | `cli_ast_rationale`            | Regex scan WHY/NOTE/SAFETY/INVARIANT (NEW, graphify) |
| C-4 | `touring ast features <file>`            | `cli_ast_features`             | `extract_cfg_feature_names` (**promoted pub(crate)**) |
| C-5 | `touring ast meta <file> [--depth ...]` | `cli_ast_meta`                 | **Aggregator**: overview+quality+blast+heat+func_sig |
| C-6 | `touring ast skeleton <file>`            | `cli_ast_skeleton`             | Symbols-only (<200 tokens) — para @filename          |
| C-7 | `touring ast blast <file> --enriched`   | `cli_ast_blast_enriched`       | `compute_enriched_blast_radius` (**orphan wired**)    |
| C-8 | `touring wiring purpose <file>`          | `cli_wiring_purpose`           | Query `functional_signatures` table (**orphan wired**) |
| C-9 | `touring wiring community <file>`        | `cli_wiring_community`         | Query `functional_chains` table (**orphan wired**)   |

> ⚠️ **CONDIÇÃO C-1**: Adicionar code comment explicitando uso de `touring_ast::CallGraph` (NÃO `touring_cortex::CallGraph`) para evitar regressão homonimia.
> ⚠️ **CONDIÇÃO C-8/C-9**: Verificar init order — `functional_signatures`/`functional_chains` são criadas em `functional_wiring.rs`, não em `migrate_schema()`. Adicionar integration test: fresh DB init + query = empty list (não SQL error).

### P6 — GRAPH CLI (4 tasks, dep P5, M)

| ID   | CLI Command                                        | Handler Fn                 | Implementation                               |
|------|----------------------------------------------------|----------------------------|----------------------------------------------|
| C-10 | `touring graph file <file>`                        | `cli_graph_file`           | Combine wiring consumers + imports + calls   |
| C-11 | `touring graph god-nodes [--top N]`                | `cli_graph_god_nodes`      | `SQL GROUP BY COUNT DESC LIMIT N`            |
| C-12 | `touring graph shortest-path <src> <tgt>`         | `cli_graph_shortest_path`  | BFS on wiring_map, max depth 20              |
| C-13 | **NEW FILE** + register em `command_table()`     | `touring-server/src/cli/graph.rs` + `cli/mod.rs` | Standard pattern  |

### P7 — CLI ROUTERS (3 tasks, dep P5+P6, S)

| ID   | File                                        | Change                                                                 |
|------|---------------------------------------------|------------------------------------------------------------------------|
| C-14 | `touring-server/src/cli/ast.rs`             | +7 match arms (callgraph, meta, skeleton, todos, rationale, features, `--enriched` flag) |
| C-15 | `touring-server/src/cli/wiring.rs`          | +2 match arms (purpose, community)                                     |
| C-16 | `touring-hooks/src/hook_registry.rs:727,729` | Register 12 hook names + dispatch. **⚠️ CORREÇÃO AUDITOR**: assertion update é **98 → 110**, não 68→80 (gotcha ID:104) |

> ⚠️ **CONDIÇÃO C-16 (CRÍTICA)**: O baseline real é `ALL_DAEMON_HOOK_NAMES.len() == 98`. Adicionar 12 = **110**. Plano original mencionava 68→80 — **corrigido aqui**. Engineers que não aplicarem esta correção causarão compile-time test failure.

### P8 — MCP TOOLS (2 tasks, dep P5+P6, M)

| ID   | File                                         | Change                                                                                 |
|------|----------------------------------------------|----------------------------------------------------------------------------------------|
| C-17 | `touring-server/src/server/params.rs`        | +`MetaDepth` enum + 7 Params structs                                                   |
| C-18 | `touring-server/src/server/mod.rs` (5157 LOC) | +7 `#[tool]` async methods em contiguous block `// === FILE METADATA TOOLS (P8) ===` |

**MCP tools adicionados**:
- `touring_file_meta(file_path, depth: Skeleton|Summary|Full)` — aggregator
- `touring_query_graph(seed, mode: Bfs|Dfs, depth, budget)` — graphify-inspired
- `touring_get_neighbors(file_path)` — 1-hop neighborhood
- `touring_shortest_path(src, tgt)` — dependency path
- `touring_get_ai_context(file_path, budget=2000)` — token-budgeted context para `@filename`
- `touring_analyze_impact(file_path)` — blast + wiring + gotchas
- `touring_analyze_complexity(file_path)` — QualityReport focused

### P9 — OBSERVABILITY (1 task, dep P4, S)

| ID   | File                                            | Change                                         |
|------|-------------------------------------------------|------------------------------------------------|
| B-15 | `touring-hooks/src/shared/gate_metrics.rs`      | +7 `AtomicU64` counters + CLI exposure         |

**Novos counters**: `post_edit_metadata_fast`, `post_edit_metadata_async`, `post_edit_metadata_skipped`, `metadata_backfill_jobs_spawned`, `metadata_backfill_jobs_completed`, `metadata_cache_hit_ratio`, `metadata_stale_detected`

### P10 — AWARENESS LAYER (4 tasks, dep P7+P8, S)

| ID  | File                                                         | Content                                                                                             |
|-----|--------------------------------------------------------------|-----------------------------------------------------------------------------------------------------|
| D-1 | `touring-hooks/src/instructions_loaded.rs`                   | Expandir `additionalContext` para injetar metadata surface awareness (CLI + MCP + @filename + skill) |
| D-2 | `~/.claude/skills/touring-file-metadata/SKILL.md` (NEW)      | Frontmatter + CLI docs + MCP docs + @filename flow + exemplos                                       |
| D-3 | `~/.claude/rules/file-metadata-first.md` (NEW)               | Directive: check `touring ast meta` antes de Read; exceptions; cost/benefit table; gate metric target |
| D-4 | `~/.claude/CLAUDE.md` (+3-5 lines)                           | Reference skill/rule/commands primários (mantém <200 linhas)                                        |

### `@filename` Integration Flow

```
User types: @src/main.rs
    ↓
Claude Code expands @filename → triggers Read tool
    ↓
pre-read hook (Touring) intercepta PreToolUse:Read
    ↓
touring_get_ai_context(file_path, budget=2000) — retorna:
    - skeleton (symbols)
    - functional_signature (purpose, domain)
    - blast_radius summary (severity, direct_count)
    - top-3 gotchas por severity
    - heat_score
    ↓
Hook retorna HookResponse::Context {
  additionalContext: "[Touring] src/main.rs\n
                      Purpose: <...>\n
                      Symbols (18): fn main, struct Config, ...\n
                      Blast: severity=0.72 direct=8 transitive=23\n
                      Gotchas: [HIGH] unwrap in async (L45), ..."
}
    ↓
Claude Code vê metadata ANTES do file content
    ↓
Decide: skip Read se metadata basta, ou prosseguir se precisa do corpo
    ↓
Fallback: Touring unavailable → exit 0 → Read procede normal
```

### P11 — VALIDATION (3 tasks, dep all, L)

| ID  | Test                                     | Target                                                                    |
|-----|------------------------------------------|---------------------------------------------------------------------------|
| V-1 | Integration test migração v6→v7          | In-memory SQLite, apply DDL, 10 validation queries, assert all columns    |
| V-2 | Hook benchmark P95 latency               | post_edit CILA=0 <40ms, CILA≥2 <100ms P95; post_write CILA≥2 <500ms P95   |
| V-3 | E2E test `touring ast meta --depth full` | Returns JSON com **todos** os campos: skeleton, quality, blast, heat, func_sig, call_graph, imports, todos, gotchas |

---

## <timeline>

### DAG com dependências (fases sequenciais, tasks internas paralelas)

```
P0 (4 parallel) ─┐
                 ├─→ P1 (7 parallel) ─→ P2 (4 parallel) ─→ P3 (2 parallel) ─→ P4 (5 parallel)
                 │                                                                 │
                 │                                                                 ↓
                 │                                                              P5 (9 parallel)
                 │                                                                 │
                 │                                                                 ↓
                 │                                                      ┌─ P6 (4 parallel)
                 │                                                      │      │
                 │                                                      │      ↓
                 │                                                      └─→ P7 (3 parallel)  P9 (1)
                 │                                                             │
                 │                                                             ↓
                 │                                                         P8 (2)
                 │                                                             │
                 │                                                             ↓
                 │                                                         P10 (4 parallel)
                 │                                                             │
                 │                                                             ↓
                 └─────────────────────────────────────────────────────→ P11 (3 tasks)
```

### Critical path (mais longo)
`A-1 → A-2 → B-4 → B-8 → B-10 → C-16 → C-14 → V-3` = ~20h serial, ~6h paralelo

### Esforço estimado (T-shirt sizing)
- **P0**: 4×S ≈ 1h
- **P1**: 7×S ≈ 2h
- **P2**: B-4=M, B-5=S, B-6=S, B-7=M ≈ 2h
- **P3**: B-8=L, B-9=S ≈ 3h
- **P4**: 5×M ≈ 5h (serial) → ~2h com 3 engineers paralelos
- **P5**: 9×M ≈ 9h (serial) → ~3h com 3 engineers paralelos
- **P6**: 4×M ≈ 4h (serial) → ~2h paralelo
- **P7**: 3×S ≈ 1h
- **P8**: 2×M ≈ 2h
- **P9**: 1×S ≈ 0.5h
- **P10**: 4×S ≈ 1h
- **P11**: 3×L ≈ 4h
- **TOTAL serial**: ~30h
- **TOTAL paralelo (3 engineers)**: ~15h = **~2 dias úteis**

### Max paralelização
- **P5 é o ponto máximo**: 9 handlers independentes podem ser implementados em paralelo por 3 engineers

---

## <risks>

### Risk Matrix (severidade × probabilidade)

| ID | Risco | Severity | Prob | Mitigação | Trigger de escalação |
|----|-------|----------|------|-----------|----------------------|
| R1 | **C-16 assertion baseline errada** (plano original 68→80, real 98→110) | CRITICAL | ~~HIGH~~ **MITIGADO** | Correção aplicada nesta v1 do plano + gotcha ID:104 registrada + test em cargo test workspace | Compile-time test failure se esquecido |
| R2 | Schema migration data loss (v6→v7) | HIGH | LOW | B-7 adiciona fs::copy `.v6.bak` + atomic BEGIN/COMMIT + 10 validation queries | Migration aborta + restore from backup |
| R3 | **A-1 migration.rs:285 test assertion esquecida** | HIGH | MEDIUM | A-1 agora inclui update do test (two-place update) | `cargo test -p touring-core` falha |
| R4 | `post_write` latency regression (<40ms → <500ms) em sessões write-heavy | HIGH | MEDIUM | CILA gate ≥2 limita expensive work; B-15 adiciona 7 counters; V-2 benchmark com regression gate P95 | gate_metrics `pre_write_fast_ratio` <0.3 |
| R5 | `TOURING_HOOK_ACTIVE` recursion guard conflict em async tasks | MEDIUM | LOW | HookGuard RAII + catch_unwind; circuit breaker após 3 loops | `metadata_stale_detected` counter >5/session |
| R6 | `functional_signatures`/`functional_chains` init order (C-8/C-9) | MEDIUM | MEDIUM | Integration test: fresh DB init + query = empty list (não SQL error); documentar em handler | SQL "no such table" em runtime |
| R7 | `server/mod.rs` merge complexity (5157 LOC) | MEDIUM | MEDIUM | C-18 adiciona em contiguous block com marker `// === FILE METADATA TOOLS (P8) ===` | Merge conflict em PR |
| R8 | `fan_in`/`fan_out` DB query latency | LOW | LOW | Indexed queries, COUNT(*) O(log n), target <2ms | post_edit P95 >100ms após S-5 |
| R9 | CallGraph homonimia (touring-ast vs touring-cortex) | LOW | ~~MEDIUM~~ **MITIGADO** | Code comment obrigatório em C-1; touring-cortex NÃO importado por touring-hooks | Compile error se import touring-cortex |

### Pontos de circuit breaker
- Se V-2 benchmark P95 violado → **PARAR** implementação, re-avaliar CILA gate
- Se V-1 migration falhar em CI → **PARAR**, restore from `.v6.bak`, reavaliar DDL
- Se 3+ tasks paralelas em P5 falharem → **PARAR**, re-validar interface contracts entre capabilities

### Fora de escopo (deferred)
- **F3** (HNSW embeddings + PageRank global + Louvain clustering) — colunas stub nullable criadas, populate fica para fase futura
- **F4** (CRDTs Diamond Types/Eips para multi-agent concurrent edits) — fora do escopo desta entrega
- **F6** (Pie WASM + LMCache + prefetching) — fora do escopo
- **Test coverage tracking** (llvm-cov/tarpaulin parsing) — coluna nullable criada, fill lazy/deferred
- **Ownership via git blame** — git está proibido (REGRA #11); alternativa via `session.agent_id` em owner_agent (populado em post-edit/write)

---

## <success_criteria>

### Gates de validação obrigatórios (all must PASS)

1. **Schema v7 migrated successfully**
   - `touring status -j | jq .knowledge.schema_version` = `7`
   - `.v6.bak` file created antes + removed após validation pass
   - Todas as 10 validation queries retornam sucesso

2. **Zero novos orphan pub symbols**
   - `touring wiring orphans -j` antes vs depois: delta ≤ 0
   - Todos os 5 orfãos existentes wired: FunctionalSignature, build_call_graph, compute_enriched_blast_radius, extract_cfg_feature_names, functional_chains

3. **Hook latency preservada**
   - `touring gate-metrics -j`: post_edit P95 <100ms em CILA 0/1, <500ms em CILA≥2
   - post_read P95 <20ms

4. **CLI smoke test**
   - `touring ast meta <file> --depth full -j` retorna JSON válido com todos os campos em <500ms
   - `touring ast callgraph <file> -j | jq .cycles` retorna array
   - `touring wiring purpose <file> -j | jq .module_purpose` retorna string ou null

5. **MCP smoke test**
   - `mcp__touring__touring_file_meta` callable de Claude Code
   - `mcp__touring__touring_get_ai_context` retorna context string dentro do budget

6. **Awareness surface**
   - SessionStart instructions_loaded menciona "file metadata surface available"
   - SKILL.md presente em `~/.claude/skills/touring-file-metadata/`
   - Rule file presente em `~/.claude/rules/file-metadata-first.md`

7. **Regression gate**
   - `cargo test --workspace --exclude touring-python` retorna 5154+ tests passing
   - `cargo clippy --workspace -- -D warnings` retorna 0 warnings
   - E2E score >0.55 (delta positivo de 0.52)

### Métricas de sucesso pós-deploy (7 dias após merge)
- `touring e2e --depth deep -j | jq .overall_score` eleva para >0.75
- `gate_metrics.metadata_cache_hit_ratio` >0.6 (60%+ dedup hits)
- Orphan rate reduz de 96.6% para <94% (ganho de 5 orfãos wired × multiplicador)
- Tokens consumidos em Read operations reduz em 30%+ (comparar antes/depois via session logs)

---

## Resumo das Fases TACO Executadas

| Fase | Subagents | Status | Findings Principais |
|------|-----------|--------|---------------------|
| **P0 Perception** | direct | ✅ | Touring daemon UP, 297k symbols, 33k orfãos, e2e 0.52 warn |
| **P1 Scout** (4 paralelos) | touring-scouter ×4 | ✅ | Roadmap (261L, 6 fases), graphify (SHA256, confidence edges, Leiden, MCP), memory-compiler (CLAUDE_INVOKED_BY, 3-tier knowledge), Touring state (**maioria JÁ IMPLEMENTADO mas orfão de CLI**) |
| **P1.5 Sequential Thinking** | direct | ✅ | Descoberta-chave: "WIRE, not BUILD" reduz esforço 5-10x |
| **P2 Architect** (3 paralelos) | touring-architect ×3 | ✅ | Schema v6→v7 (13+5+5 tabelas), hooks 3-tier (fast/deferred/async), CLI 12+MCP 7 handlers, skill/rule/awareness |
| **P3 Context7** | context7 docs | ✅ | PreToolUse `hookSpecificOutput.additionalContext` validado para @filename; Tree-sitter InputEdit incremental pattern confirmado |
| **P4 Decompose** | direct synthesis | ✅ | DAG 38 tasks, 11 fases, critical path identificado, effort ~2 dias paralelos |
| **P6 Cross-Audit** | touring-auditor | ✅ CONDITIONAL (0.97) | VP-Scout 4-chain PASSED; **4 correções detectadas e aplicadas neste plano** |
| **P7 Scriber** | direct | ✅ | Plano Final escrito em `/home/gabrielgadea/.claude/rust/PLAN-file-metadata-expansion-v1.md` |

## Correções do Auditor aplicadas nesta v1

| # | Correção | Aplicada em |
|---|----------|-------------|
| 1 | C-16 assertion baseline 68→80 corrigido para **98→110** (gotcha ID:104) | P7 CLI routers |
| 2 | A-1 agora inclui update de `migration.rs:285` test assertion | P0 foundation |
| 3 | C-8/C-9 incluem verificação de `functional_signatures`/`functional_chains` init order + integration test | P5 CLI handlers |
| 4 | C-1 handler deve ter code comment documentando homonimia CallGraph | P5 CLI handlers |

## Memory stored (auditor + orchestrator)

- `plan:file-metadata:architecture` — 3-tier collector + schema v6→v7 + 12 CLI + 7 MCP
- `plan:file-metadata:orphan-discovery` — Maioria das capabilities JÁ EXISTEM, wire não build
- `gotcha:homonimia:CallGraph` — touring-ast ≠ touring-cortex, usar apenas touring-ast
- `pattern:metadata-collector` — 3-tier fast/deferred/async + dedup 60s TTL
- `gotcha:plan-baseline:hook-count` (ID:104) — ALL_DAEMON_HOOK_NAMES=98, não 68

## RL rewards injetados

- `orchestrate 1.0 plan_audit_passed:file_metadata_expansion_v1`
- `speculate 1.0 vp_scout_4chain_completed:homonimia+cycle+jai+feature`
- `edit 1.0 memory_stored:5_audit_lessons_captured`

---

## Próximos passos para execução

1. **Gabriel aprova este plano** (ou solicita ajustes)
2. **TACO orchestrator dispatch engineers** em P0 paralelo (4 tasks independentes, S effort)
3. **Checkpoint após cada fase** via `touring checkpoint <phase_id>`
4. **P11 validation gates** executados incrementalmente (V-1 após P2, V-2 após P4, V-3 após P11)
5. **Final gate**: `cargo test --workspace` + `touring e2e --depth deep -j` + manual E2E test de `@filename` injection
6. **Merge** quando todos os 7 gates de validação passarem

---

*Plano gerado via TACO v6.0 — Sequential Phase Protocol | 8 subagents | 7 fases | composite_score 0.97 | VP-Scout 4-chain PASSED*
