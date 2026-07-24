---
name: graph-viz-wave-6
description: Wave 6 (Agent UX) — Deliverable D26 find_code super-tool MCP
type: project
related_files:
  - graph-viz-master-plan_OVERVIEW.md
  - graph-viz-master-plan_STATUS.md
  - graph-viz-master-plan_WAVES_1_2.md
  - graph-viz-master-plan_WAVE_3.md
  - graph-viz-master-plan_WAVE_4.md
  - graph-viz-master-plan_WAVE_5.md
  - graph-viz-master-plan_WAVE_6.md
  - graph-viz-master-plan_WAVE_7.md
  - graph-viz-master-plan_WAVE_8.md
  - graph-viz-master-plan_DEPENDENCIES.md
---

# Wave 6 — Agent UX (find_code super-tool)

**Target**: v31.2.0 | **Data**: 2026-05-02

---

## D26 — `touring find_code(description)` super-tool MCP 🔴 PENDENTE (0%)

**Dependencies**:
- D13 (intent classification)
- D24 (hybrid scoring)
- D25 (asymmetric embeddings)
- Fallback: D4 (RRF only) se Wave 5 não deployed

---

### API Design

**MCP Tool**:
```rust
pub struct FindCodeParams {
    pub query: String,
    pub intent: Option<QueryIntent>,
    pub focus_languages: Option<Vec<String>>,
    pub token_limit: Option<usize>,
    pub include_tests: bool,
    pub max_results: Option<usize>,
}

pub struct FindCodeResponse {
    pub matches: Vec<CodeMatch>,
    pub total_tokens: usize,
    pub intent_detected: QueryIntent,
    pub strategy_used: SearchStrategy,  // "hybrid" | "rrf-only" | "bm25-only"
}
```

**CLI mirror**: `touring find-code <description>`

---

### Pipeline Orchestrado

1. `detect_intent(query)` — D13
2. Hybrid search (Wave 5: D22+D23+D24) OU fallback BM25-only
3. Filter por `focus_languages`
4. Exclude tests se `!include_tests`
5. Format compact (token-efficient), respeita `token_limit`

**Compact output format**:
```
file:line:col
  symbol_name
  one-line context
```

---

### Arquivos Afetados

```
touring-server/src/cli/find_code.rs      🔴 NEW (~250 LOC)
touring-server/src/mcp/tools/find_code.rs 🔴 NEW (~150 LOC)
touring-hooks/src/cli_handlers.rs         🟡 cli_find_code handler
touring-server/src/mcp/tools/mod.rs        🟡 register new tool
```

---

### Graceful Degradation

Se Wave 5 (D22-D25) não deployed:
- `strategy_used = "rrf-only"` — usa D4 RRF + tantivy + index find
- Sem dense embeddings, semantic search cai para keyword

---

### Acceptance Criteria

1. `touring find-code "where do we validate JWT tokens"` retorna ≤ 5 matches relevantes
2. Token count < `token_limit` se especificado
3. Fallback mode se Wave 5 indisponível
4. Teste em ≥ 3 projetos diferentes

**Testes**: 18 unit + 4 integration

---

## VALIDAÇÃO GATE WAVE 6

```bash
# super-tool MCP
echo '{"method": "tools/call", "params": {"name": "touring_find_code", "arguments": {"query": "where do we validate JWT tokens", "token_limit": 1000}}}' | touring mcp-test

# CLI mirror
touring find-code "where do we handle retries" --token-limit 500 --format compact

# graceful degradation
TOURING_DISABLE_HYBRID=1 touring find-code "auth flow" -j | jq '.strategy_used'  # → "rrf-only"
```