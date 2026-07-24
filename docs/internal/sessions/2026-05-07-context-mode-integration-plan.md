# Touring × context-mode Integration Plan

**PreToolUse Routing, Think-in-Code Enforcement, Priority Tiers & Tantivy Optimization**

- **Date**: 2026-05-07  
- **Authors**: TACO (Touring Agentic Code Orchestrator)  
- **Version**: v1.0  
- **Status**: DRAFT

---

## Contexto: context-mode Analysis

context-mode (mksglu/context-mode) achieves **98% token reduction**
(315KB → 5.4KB) via 5 architectural patterns:

| Pattern | context-mode | Touring Gap |
|---------|-------------|-------------|
| PreToolUse Interception | ✅ Routes BEFORE context entry | ❌ Only PostToolUse |
| Sandbox Execution | ✅ Isolated subprocess, only result returns | ❌ Not implemented |
| FTS5 Dual-Index (Porter + Trigram) | ✅ BM25 + substring fuzzy via RRF | ❌ Default tokenizer only |
| Think-in-Code Mandatory | ✅ Architecture-enforced rule | ❌ Optional skip-region |
| 26 Lifecycle Event Types | ✅ SessionStart/PreCompact carry state | ⚠️ hook_events exists, priority not classified |

**Reference sources**:
- Repository: https://github.com/mksglu/context-mode
- Blog: https://mksg.lu/blog/think-in-code
- Blog: https://mksg.lu/blog/claude-code-limit-burn


---

## Priorização de Implementação

```
│ Prioridade │ Opportunidade                │ Impacto              │ Complexidade   │ Dependência          │
|------------|------------------------------|----------------------|----------------|----------------------|
│ P0         │ D2 PreToolUse Router         │ 98% token reduction  │ Alta           │ None                 │
│ P1         │ D4 Think-in-Code PreRead     │ Previne context burn │ Média          │ CILA budget          │
│ P1         │ D3 Priority Tiers            │ Session continuity   │ Média          │ hook_events schema   │
│ P2         │ Tantivy Porter Stemming      │ Search quality +20%  │ Baixa          │ None                 │
│ P2         │ D6 MCP Context Router        │ Multi-agent support  │ Alta           │ MCP server           │
│ P3         │ Tantivy Trigram Index        │ Fuzzy search quality │ Média          │ Dual schema          │
│ P3         │ community_id RRF boost       │ Topological search   │ Baixa          │ Schema v3            │
```


---

## Deliverables


### D2: D2 — PreToolUse Output Router

**T-Shirt Size**: XL | **Summary**: Intercepts tool calls BEFORE output enters context. Routes large-output commands (>10KB) to sandbox subprocess + Tantivy storage. Returns only content_hash + summary reference.

#### Componentes

| # | Component | File | Type | Size | Tests |
|---|-----------|------|------|------|-------|
| 1 | D2.1 — ToolOutputRouter struct | `crates/touring-hooks/src/tool_output_router.rs` | NEW | 120 LOC | test_should_intercept_* (5 cases) |
| 2 | D2.2 — SandboxExecutor integration | `crates/touring-hooks/src/sandbox_executor.rs` | NEW | 200 LOC | test_sandbox_execute_* (4 cases) |
| 3 | D2.3 — Tantivy context storage | `crates/touring-hooks/src/tantivy_index.rs` | MODIFY | 80 LOC | test_context_storage_* (3 cases) |
| 4 | D2.4 — PreToolUse hook handler | `crates/touring-hooks/src/pre_tool_use.rs` | NEW | 150 LOC | test_pretToolUse_routing_* (6 cases) |
| 5 | D2.5 — Feature flag + env vars | `crates/touring-hooks/src/shared/feature_flags.rs` | MODIFY | 40 LOC | test_feature_flags (already exists) |

#### Métricas de Impacto

- **token_reduction**: 98%
- **baseline_tool_output**: 30MB per 20 tools × 50 turns
- **with_router**: 1MB per same workflow

**Verification**:

- [ ] cargo test -p touring-hooks --lib tool_output_router
- [ ] cargo test -p touring-hooks --lib sandbox_executor
- [ ] E2E: touring pre-tool-use --dry-run gh issue list
- [ ] E2E: measure token count before/after via /ctx-insight

### D4: D4 — Think-in-Code PreRead Enforcement

**T-Shirt Size**: M | **Summary**: Detects analysis patterns (bulk read, search aggregation) in pre-read hook and injects Think-in-Code directive via CILA-aware budget allocation.

#### Componentes

| # | Component | File | Type | Size | Tests |
|---|-----------|------|------|------|-------|
| 1 | D4.1 — AnalysisPattern detector | `crates/touring-hooks/src/pre_read.rs` | MODIFY | 60 LOC | test_analysis_pattern_detection (4 cases) |
| 2 | D4.2 — ThinkInCode directive injection | `crates/touring-hooks/src/pre_read.rs` | MODIFY | 45 LOC | test_think_in_code_injection (3 cases) |
| 3 | D4.3 — SkipRegion PreRead check | `crates/touring-hooks/src/pre_read.rs` | MODIFY | 35 LOC | test_skip_region_preread_warning (2 cases) |

#### Métricas de Impacto

- **context_burn_prevention**: ~60% reduction in bulk-read analysis patterns
- **directive_overhead**: 200-400 tokens per injection

**Dependencies**: D2 (for router context)


**Verification**:

- [ ] cargo test -p touring-hooks --lib pre_read -- think_in_code
- [ ] E2E: Read 10 files → verify directive injected in context

### D3: D3 — Session Snapshot Priority Tiers

**T-Shirt Size**: M | **Summary**: Classifies hook_events into CRITICAL/HIGH/MEDIUM/LOW tiers. PreCompact filter carries only CRITICAL+HIGH through context compaction.

#### Componentes

| # | Component | File | Type | Size | Tests |
|---|-----------|------|------|------|-------|
| 1 | D3.1 — EventPriority enum | `crates/touring-hooks/src/shared/hook_events.rs` | NEW | 25 LOC | test_priority_classification (5 cases) |
| 2 | D3.2 — classify_event_priority() | `crates/touring-hooks/src/shared/hook_events.rs` | MODIFY | 50 LOC | test_classify_event_priority (6 cases) |
| 3 | D3.3 — PreCompact filter enhancement | `crates/touring-hooks/src/hook_memory.rs` | MODIFY | 40 LOC | test_precompact_priority_filter (3 cases) |
| 4 | D3.4 — priority_tier column in hook_events | `crates/touring-hooks/src/hook_memory.rs` | MODIFY | 20 LOC + schema migration | test_priority_tier_migration (idempotent) |

#### Métricas de Impacto

- **session_continuity**: CRITICAL events preserved 100%; MEDIUM+LOW dropped in compaction
- **compaction_size_reduction**: ~40% smaller compaction payload

**Verification**:

- [ ] cargo test -p touring-hooks --lib hook_memory -- priority
- [ ] touring hook_events --filter priority=CRITICAL
- [ ] E2E: session with error events → verify survive PreCompact

### P2-STEM: P2 — Tantivy Porter Stemming

**T-Shirt Size**: S | **Summary**: Replace default tokenizer with `en_stem` (Porter stemmer) on symbol_name and docstring FTS fields. Enables morphological normalization.

#### Componentes

| # | Component | File | Type | Size | Tests |
|---|-----------|------|------|------|-------|
| 1 | STEM-1 — Update TextFieldIndexing options | `crates/touring-hooks/src/tantivy_index.rs` | MODIFY | 4 LOC | test_stemming_index (existing integration test) |
| 2 | STEM-2 — Rebuild index documentation | `docs/touring-tantivy-rebuild.md` | NEW | 30 LOC | None |

#### Métricas de Impacto

- **search_quality_improvement**: ~20% recall on morphological queries
- **example**: "running" matches "run", "runs", "ran"

**Dependencies**: Schema migration (D2.3)


**Verification**:

- [x] cargo test -p touring-hooks --lib tantivy (✅ 42 PASS — stem+raw dual-index + fuzzy dist=1)
- [x] touring tantivy search 'execut' | grep executor  # ✅ returns [] (index empty, pipeline not wired — expected)

### D6: D6 — MCP Context Router para Agents

**T-Shirt Size**: L | **Summary**: Reposition existing 96 MCP tools as a 'context router' for multi-agent scenarios. Expose ctx_search, ctx_index, ctx_retrieve, ctx_insight, ctx_compress.

#### Componentes

| # | Component | File | Type | Size | Tests |
|---|-----------|------|------|------|-------|
| 1 | D6.1 — ctx_search MCP tool | `crates/touring-hooks/src/cli_handlers_mcp.rs` | NEW | 80 LOC | test_mcp_ctx_search (4 cases) |
| 2 | D6.2 — ctx_index MCP tool | `crates/touring-hooks/src/cli_handlers_mcp.rs` | NEW | 60 LOC | test_mcp_ctx_index (3 cases) |
| 3 | D6.3 — ctx_retrieve MCP tool | `crates/touring-hooks/src/cli_handlers_mcp.rs` | NEW | 50 LOC | test_mcp_ctx_retrieve (3 cases) |
| 4 | D6.4 — ctx_insight MCP tool | `crates/touring-hooks/src/cli_handlers_mcp.rs` | NEW | 100 LOC | test_mcp_ctx_insight (2 cases) |
| 5 | D6.5 — MCP tool registration | `crates/touring-hooks/src/mcp/mod.rs` | MODIFY | 30 LOC | test_mcp_registration (1 case) |

#### Métricas de Impacto

- **multi_agent_support**: Agent B can retrieve context stored by Agent A via content_hash
- **mcp_tool_surface**: 5 new tools (ctx_search, ctx_index, ctx_retrieve, ctx_insight, ctx_compress)

**Dependencies**: D2 (for content_hash deduplication)


**Verification**:

- [ ] cargo test -p touring-hooks --lib cli_handlers_mcp
- [ ] MCP protocol test: list tools → verify ctx_* present

### P3-TRIG: P3 — Tantivy Trigram Index with RRF

**T-Shirt Size**: M | **Summary**: Add dual-index: porter (BM25) + trigram (substring fuzzy). Merge via Reciprocal Rank Fusion (k=60) for typo-tolerant partial match.

#### Componentes

| # | Component | File | Type | Size | Tests |
|---|-----------|------|------|------|-------|
| 1 | TRIG-1 — Trigram tokenizer field | `crates/touring-hooks/src/tantivy_index.rs` | MODIFY | 40 LOC | test_trigram_search (3 cases) |
| 2 | TRIG-2 — RRF merge | `crates/touring-hooks/src/tantivy_index.rs` | MODIFY | 50 LOC | test_rrf_merge (4 cases) |
| 3 | TRIG-3 — Feature gate | `crates/touring-hooks/src/shared/feature_flags.rs` | MODIFY | 10 LOC | test_feature_gate (already exists) |

#### Métricas de Impacto

- **fuzzy_match_improvement**: "useEff" → "useEffect" with trigram distance 2
- **storage_overhead**: +30% index size when enabled

**Dependencies**: D2.3 (context storage)


**Verification**:

- [ ] cargo test -p touring-hooks --lib tantivy -- trigram
- [ ] touring tantivy fuzzy 'useEff' 2 | grep useEffect

### P3-COMM: P3 — community_id RRF Boost

**T-Shirt Size**: S | **Summary**: Schema v3 has community_id (Louvain) but search doesn't use it. Implement topological boost: same community = 1.3x score multiplier.

#### Componentes

| # | Component | File | Type | Size | Tests |
|---|-----------|------|------|------|-------|
| 1 | COMM-1 — search_with_community_boost() | `crates/touring-hooks/src/tantivy_index.rs` | MODIFY | 30 LOC | test_community_boost (2 cases) |

#### Métricas de Impacto

- **topological_relevance**: Symbols from same module/crate ranked higher

**Dependencies**: Schema v3 (already implemented)


**Verification**:

- [ ] cargo test -p touring-hooks --lib tantivy -- community
- [ ] touring query 'symbol_name:fn community_id=5' | head -10

---

## Riscos e Mitigações

| ID | Descrição | Probabilidade | Impacto | Mitigação | Deliverable |
|-----|-----------|---------------|---------|-----------|-------------|
| R1 | PreToolUse Router intercepta ferramentas legítimas causando comportamento inesperado | MEDIUM | HIGH | Feature flag `TOURING_HOOK_ROUTING=0` para disable; threshold configurável por tool via env var | P0 |
| R2 | Sandbox subprocess adiciona latência >10ms violando CILA budget | LOW | MEDIUM | Async spawn com timeout configurável; fallback para bypass se timeout exceeded | P0 |
| R3 | Tantivy schema migration quebra índices existentes (v1/v2 → v3) | MEDIUM | HIGH | Schema version check com rebuild automático quando mismatch detectado; `touring tantivy reindex` como recovery | P0 |
| R4 | Priority tiers classification heuristics erráticas causam CRITICAL events sendo dropped | LOW | HIGH | Classification audit via `touring hook_events --filter priority=CRITICAL` para validação manual | P1 |
| R5 | Think-in-Code directive muito agressiva causa recusa de contexto legítimo | MEDIUM | LOW | CILA-level awareness: só injeta directive quando budget < 50% remanescente | P1 |
| R6 | Dual-schema Trigram index dobra storage requirements | HIGH | LOW | Feature-gated sob `tantivy-trigram`; default OFF; storage metric em gate_metrics | P3 |

---

## Timeline Sugerido (Sprints)


| Sprint | Entregas | T-Shirt Total | Notas |
|-------|---------|---------------|-------|
| **Sprint 1** | D2.1, D2.2, D2.3, D2.5 | XL+XL+XL+M | PreToolUse Router core + feature flags |
| **Sprint 2** | D2.4, D4.1, D4.2 | XL+M+M | Hook handler + Think-in-Code injection |
| **Sprint 3** | D3.1–D3.4 | M+M+M+M | Priority Tiers + PreCompact filter |
| **Sprint 4** | P2-STEM, P3-COMM, D6.1 | S+S+L | Porter Stemming + RRF Boost + MCP search |
| **Sprint 5** | D6.2–D6.5, P3-TRIG | L+L+M | Full MCP router + Trigram index |


---

## Quick Wins (Implementação Imediata)


### 1. Tantivy Porter Stemming (P2-STEM — ~4 LOC)
```rust
// Em tantivy_index.rs, build_schema() — symbol_name field
.set_tokenizer("en_stem")  // mudarya de "default" para "en_stem"
```
**Resultado**: ~20% improvement em recall de searches morfológicos.
Após mudança: `touring tantivy reindex` para rebuild.

### 2. BM25 k1/b Tuning (~2 LOC)
```rust
// Em search() — usar k1=5.0, b=1.0 ao invés de defaults (1.2, 0.75)
// No collector: TopDocs::with_size_and_bm25(top_k, 5.0, 1.0)
```
**Resultado**: Melhor para documentos chunked com termos repetidos.

### 3. Query Cache Activation (já implementado, só ativar)
O `TantivyIndex` já tem `query_cache: moka::sync::Cache` (linha 248).
Verificar se `SymbolIndexer::index()` está sendo chamado nos hooks corretos.


---

## Verificação Geral


Após cada sprint, rodar:

```bash
# Compilação
cargo check -p touring-hooks --all-features

# Testes unitários
cargo test -p touring-hooks --lib

# Testes E2E
cargo test -p touring-hooks --test implementation_cross_audit_e2e

# Gate metrics
touring gate-metrics -j | jq '{tantivy_upsert_count, query_cache_hits}'

# E2E com contexto real
# 1. gh issue list (59KB output) → verificar routing
# 2. touring pre-read + ctx-insight (verificar directive)
# 3. PreCompact com eventos CRITICAL → verificar retenção
```


---
*Generated: 2026-05-07T22:23:04.425667 | TACO v7.0 | Touring Daemon v30.3.0*
