# Análise Complementar: Metadata Pln2 → Unified Touring Excellence Pln2

> **Date**: 2026-04-12
> **Propósito**: Mapear o que o `PLAN-file-metadata-expansion-v2-squared.md` JÁ IMPLEMENTOU que afeta o `PLAN-unified-touring-excellence-pln2.md`, corrigir premissas falsas, e identificar gaps remanescentes.

---

## 1. Premissas do Unified Plan que Precisam Correção

O Unified plan foi construído com dados da sessão de diagnóstico (4 agentes). Vários desses dados estão **desatualizados** porque o metadata plan já implementou mudanças significativas.

| Premissa do Unified | Realidade Verificada | Impacto |
|---------------------|---------------------|---------|
| Hook count = 98-99 | **113** (hook_registry.rs:784) | +14 hooks desde o diagnóstico. Gotcha sobre hook count baseline está wrong. |
| SCHEMA_VERSION = 6 | **7** (migration.rs:17) | Schema já migrou 6→7. Unified plan não precisa cobrir isso. |
| BLAKE3 = "não existe" | **EXISTS** (Cargo.toml:91, blake3 = "1.5.5") | BLAKE3 adapter já criado. Early-exit em post_edit + post_write DONE. |
| Tantivy = "a criar" | **DEP EXISTS** (Cargo.toml:140, tantivy = "0.22") | Dep adicionada mas **TantivySearchEngine struct NÃO existe**. Versão é 0.22, Unified quer 0.24. |
| ScipEmitter = "a criar" | **EXISTS** (scip_emit.rs:135, full impl + tests) | U16 do Unified DONE. |
| wiring suggest = "a implementar" | **EXISTS** (cli_handlers.rs:385, two-phase compute+cache) | Parte do wiring suggest DONE. |
| touring query DSL = "a criar" | **EXISTS** (query_dsl.rs, full parser) | CLI command operacional. |
| 12 DB tables = "a criar" | **ALL 12 EXIST** (knowledge.rs:366-479) | Todas tabelas criadas. |
| FastMetadata struct = "a criar" | **EXISTS** (metadata_collector.rs:11) | 11 campos implementados. |
| LeidenCommunityDetector = "a criar" | **EXISTS** (touring-learning + touring-hooks) | Clustering implementado. |
| IncrementalPipeline = "orphan" | **WIRED** (hook_runtime + parser_cache) | Já conectado ao pipeline. |
| symbol_events_log = "a criar" | **WIRED** (post_edit + post_write) | Append-only event log funcional. |
| session_file_summary = "a criar" | **WIRED** (session_hooks + instructions_loaded) | Hot files persistence funcional. |
| MetadataDedup = "OnceLock<Mutex<HashMap>>" | **moka::sync::Cache** (metadata_dedup.rs:21) | Já upgradeado para bounded cache. |
| FileParserCache = "a criar" | **EXISTS + moka** (parser_cache.rs) | Já implementado com TTL. |
| AsyncFileKnowledgeDB = "não usado" | **FULLY WIRED** (record_edit, record_bash, record_access, wal_checkpoint, stats, get_coedits_from, edit_count_for_file) | 7 methods wired across 6 hooks. |
| GraphService co-edit = "vec![]" | **LIVE** (real co-edit data, RRF 3-signal blend) | Terceiro sinal RRF ativo. |
| cognitive_bridge = "orphan" | **MODULE EXISTS** (lib.rs:36 pub mod) | Parcialmente integrado. |

---

## 2. Overlap Matrix: Unified Fixes × Metadata Implementation

| Unified Fix | Status via Metadata | Ação Necessária |
|------------|--------------------|--------------------|
| **U1** (CILA Router) | NOT COVERED | Implementar — metadata plan não toca CILA routing |
| **U2** (Hook Noise) | PARTIALLY — hook count mudou para 113, mais hooks = mais noise potential | Implementar — noise source ainda existe |
| **U3** (Gotcha Purge) | NOT COVERED | Implementar — gotcha #46 ainda existe |
| **U4** (Enhancer SC) | NOT COVERED | Implementar |
| **U5** (Tantivy deps) | PARTIALLY — tantivy 0.22 já existe | **Upgrade 0.22→0.24** apenas |
| **U6** (Schema+Tokenizer) | PARTIALLY — 12 tables + schema existem | **Criar Tantivy schema** (15 campos) — DB schema ≠ Tantivy schema |
| **U7** (Engine Core) | NOT DONE — dep existe mas TantivySearchEngine não | Implementar — core engine |
| **U8** (Query Engine) | NOT DONE | Implementar — BM25+fuzzy+facets query engine |
| **U9** (Hook Wiring) | PARTIALLY — post_edit/post_write já wired para metadata | **Adicionar Tantivy writer channel** aos hooks existentes |
| **U10** (Session Wiring) | DONE — session_hooks já tem wal_checkpoint + session_file_summary | **Adicionar Tantivy warmup** on session_start |
| **U11** (CLI+Registry) | PARTIALLY — 4 CLI commands criados + hook_registry=113 | **Adicionar 5 Tantivy CLI** commands |
| **U12** (Code-First Gate) | NOT COVERED | Implementar — VP-Scout update |
| **U13** (Agent Slim) | NOT COVERED | Implementar — agent definitions não tocados |
| **U14** (RL Warmup) | NOT COVERED | Implementar — RL update_count ainda 1 |
| **U15** (Tantivy MCP) | NOT DONE | Implementar |
| **U16** (SCIP Emitter) | **DONE** (scip_emit.rs:135) | **SKIP** — já implementado com tests |
| **U17** (Memory via Tantivy) | NOT DONE | Implementar — Tantivy engine precisa existir primeiro |
| **U18** (Agent Verify) | NOT COVERED | Implementar |
| **U19** (E2E Calibration) | PARTIALLY — cli_e2e.rs enriched com knowledge_activity | **Adicionar Tantivy stats** ao E2E |
| **U20** (Self-Healing) | PARTIALLY — session_hooks tem wal_checkpoint | **Adicionar health gate + drift check** |
| **U21** (Tantivy Metrics) | NOT DONE | Implementar |
| **U22** (Tests+Benchmarks) | PARTIALLY — validations feitas per-iteration | **Faltam criterion benchmarks** e integration test suite completa |

---

## 3. Tasks do Metadata Plan Ainda Pendentes

De 82 tasks planejadas, ~25 foram implementadas (Iterations 6-15). As **~57 tasks restantes** se agrupam em:

### 3a. Já Cobertas pelo Unified Plan (não duplicar)

| Metadata Task | Unified Fix Equivalente |
|---------------|------------------------|
| T-1/T-2/T-3 (Tantivy integration) | U5-U8, U15, U21 |
| C-scip (SCIP emit) | **DONE** (U16 pode ser SKIPPED) |
| P14 (Python infra bridge) | Não coberto — adicionar ao Unified |

### 3b. Exclusivas do Metadata Plan (não estão no Unified)

| ID | Task | Esforço | Prioridade |
|----|------|---------|-----------|
| B-4 | FileKnowledge struct +15 Optional fields | M | HIGH — schema definido mas struct não extendido |
| B-5 | query_fan_metrics + update_fan_counters + upsert_cognitive_score | M | HIGH — functions para novos campos |
| B-6 | TodoKind + EdgeConfidence enums | S | MEDIUM |
| B-feature | FeatureFlagExtractor trait + 4 impls | M | MEDIUM |
| B-10/11/12/13/14 | Hook wiring expansion (rayon, inject_reward, @filename) | L | HIGH |
| C-1 a C-9 | CLI handlers Pln1 (callgraph, todos, rationale, features, meta, skeleton, blast, wiring-purpose, wiring-community) | L | HIGH |
| C-14/C-15 | touring search symbols/docs | M | HIGH (Tantivy-dependent) |
| C-22 a C-26 | CLI routers update + hook_registry | M | HIGH |
| C-27/C-28/C-29 | MCP params + server split + tool delegators | L | CRITICAL — server/mod.rs still 5000+ LOC |
| W-1/W-2/W-3 | Wiring suggest engine improvements | M | MEDIUM (base já existe) |
| B-15/OTL-1/OTL-2 | Observability (gate metrics + OTEL) | S | MEDIUM |
| D-1 to D-7 | Awareness layer (skills + rules) | M | LOW (skill files já existem) |
| P14 IMPL-1 to IMPL-5 | Python infra bridge | M | MEDIUM |
| V-1 to V-6 | Validation suite completa | L | HIGH |

### 3c. Feitas por Metadata que Unified NÃO Sabia

Estes items já implementados MUDAM a baseline do Unified plan:

| Implementação | Impacto no Unified |
|---------------|-------------------|
| **BLAKE3 early-exit** em post_edit + post_write | Performance hooks melhorada — baseline de latência é menor |
| **moka Cache** para FileParserCache e MetadataDedup | Escalabilidade — bounded, TTL-based, melhor que DashMap proposto |
| **AsyncFileKnowledgeDB 7 methods wired** | Integração — async DB path funcional, não "stub" |
| **GraphService RRF 3-signal blend** | Intelligence — co-edit signal live, blast radius enriched |
| **cli_wiring_suggest two-phase** | Wiring — compute-on-demand com cache, não read-only |
| **cli_e2e enriched** com knowledge_activity | E2E — mais sinais disponíveis para calibração |
| **113 hooks** (not 98-99) | Infrastructure — 14+ hooks adicionados |
| **session_file_summary + instructions_loaded v2** | Context — hot files persistentes, reduz re-read |

---

## 4. Unified Plan Revisado — Ajustes Necessários

### Fixes que podem ser SKIPPED (já implementados):

| Fix | Razão | Savings |
|-----|-------|---------|
| U16 (SCIP Emitter) | scip_emit.rs:135 EXISTS, full impl | -5h |
| Parte de U9 (Hook Wiring) | post_edit/post_write/session_hooks já wired | -1.5h |
| Parte de U10 (Session Wiring) | session_file_summary + wal_checkpoint já funcional | -0.5h |
| Parte de U19 (E2E Calibration) | cli_e2e enriched com knowledge_activity | -1h |

### Fixes que precisam de AJUSTE:

| Fix | Ajuste | Impacto |
|-----|--------|---------|
| U5 (Tantivy deps) | Upgrade 0.22→0.24, não add from scratch | -0.5h |
| U11 (CLI+Registry) | Hook count baseline = 113 (not 98-99) | Calibrar asserts |
| U21 (Tantivy Metrics) | gate_metrics já tem metadata_cache_hit e metadata_backpressure_dropped | Adicionar counters Tantivy, não recriar base |

### Fixes NOVOS que precisam ser ADICIONADOS ao Unified:

| Fix | Fonte | Esforço | Descrição |
|-----|-------|---------|-----------|
| **U23** | Metadata B-4/B-5 | M (3h) | FileKnowledge struct extension + query/update functions |
| **U24** | Metadata C-1 to C-9 | L (6h) | 9 CLI handlers Pln1 (callgraph, todos, features, meta, etc.) |
| **U25** | Metadata C-28 | L (6h) | server/mod.rs split (~5000→600 LOC) — CRITICAL debt |
| **U26** | Metadata P14 | M (3h) | pln2_integration.py — Python infra bridge |
| **U27** | Metadata V-1 to V-6 | L (5h) | Validation suite (migration tests, criterion benchmarks, E2E) |

---

## 5. Resumo Quantitativo

| Métrica | Metadata Plan | Implementado | Pendente | Coberto pelo Unified | GAP (exclusivo metadata) |
|---------|--------------|-------------|----------|---------------------|------------------------|
| Tasks totais | 82 | ~25 (30%) | ~57 (70%) | ~20 | **~37 tasks exclusivas** |
| DB tables | 12 | **12 (100%)** | 0 | — | 0 |
| CLI commands | 25 | **4** (16%) | 21 | 5 (Tantivy) | **16 CLI commands** |
| MCP tools | 15 | ~3 | ~12 | 5 (Tantivy) | **~7 MCP tools** |
| Hooks wired | 15 | ~8 | ~7 | — | **~7 hook wirings** |
| server/mod.rs split | 5000→600 | NOT DONE | YES | NOT COVERED | **CRITICAL gap** |
| Python bridge | 1 file | NOT DONE | YES | NOT COVERED | **MEDIUM gap** |
| Validation suite | 6 tasks | PARTIAL | YES | PARTIAL | **HIGH gap** |

### Effort delta para Unified Plan

| Original Unified | Savings (já implementado) | Additions (metadata gaps) | **Revised Total** |
|-----------------|--------------------------|---------------------------|-------------------|
| 47h | -8h (U16, partial U9/U10/U19) | +23h (U23-U27) | **~62h (parallel: ~38h)** |

---

## 6. Recomendação de Priorização

```
PRIORITY 1 — IMMEDIATE (Unified Wave 0 unchanged):
  U1 (CILA), U2 (Hook Noise), U3 (Gotcha), U4 (Enhancer) — 3h

PRIORITY 2 — TANTIVY ENGINE (Unified Wave 1 + metadata adjustment):
  U5 (upgrade 0.22→0.24), U6, U7, U8 — 10h
  NOTE: Tantivy 0.22 dep already exists, schema 12 tables exist

PRIORITY 3 — CRITICAL DEBT (NEW from metadata):
  U25 (server/mod.rs split) — 6h
  U24 (9 CLI handlers) — 6h
  These are BLOCKING for sustainable growth of CLI/MCP surface

PRIORITY 4 — UNIFIED WAVE 2 + METADATA WIRING:
  U9 (add Tantivy to existing hooks), U11, U12, U13, U14 + U23 — 12h

PRIORITY 5 — SURFACE + VALIDATION:
  U15 (MCP), U17, U18, U19, U20, U21, U22 + U26, U27 — 14h

PRIORITY 6 — DEFERRED:
  Metadata P14 (Python bridge) — when TACO phase gates are needed
  Metadata D-1 to D-7 (skills) — some already exist
```

---

## 7. Hook Count Timeline

| Momento | Hook Count | Fonte |
|---------|-----------|-------|
| Pln1 baseline (spec) | 98 | PLAN-file-metadata-expansion-v1.md |
| Diagnostic Pln1 (assumed) | 98-99 | PLAN-diagnostic-precision-v1.md |
| Metadata plan target | 111 (98+13) | PLAN-file-metadata-expansion-v2-squared.md |
| **Current reality** | **113** | hook_registry.rs:784 assert |
| Unified plan target (Tantivy) | 113+5 = **118** | 5 Tantivy hooks a adicionar |

---

*Complementary analysis produced from code verification of 17 critical items + 82-task plan cross-reference. All claims verified by grep/read of source code.*
