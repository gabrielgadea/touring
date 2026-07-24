#!/usr/bin/env python3
"""
Generate Implementation Plan: Touring × context-mode Integration
===============================================================

Generates: ~/.claude/rust/docs/2026-05-07-context-mode-integration-plan.md

Priority Table:
  P0 | D2 PreToolUse Router      | 98% token reduction  | Alta   | None
  P1 | D4 Think-in-Code PreRead  | Previne context burn | Média  | CILA budget
  P1 | D3 Priority Tiers         | Session continuity   | Média  | hook_events schema
  P2 | Tantivy Porter Stemming   | Search quality +20%  | Baixa  | None
  P2 | D6 MCP Context Router     | Multi-agent support  | Alta   | MCP server
  P3 | Tantivy Trigram Index     | Fuzzy search quality | Média  | Dual schema
  P3 | community_id RRF boost   | Topological search   | Baixa  | Schema v3

Usage:
  python3 scripts/generate_context_mode_plan.py
"""

import os
from datetime import datetime

OUTPUT_PATH = os.path.expanduser(
    "~/.claude/rust/docs/2026-05-07-context-mode-integration-plan.md"
)

PLAN = {
    "title": "Touring × context-mode Integration Plan",
    "subtitle": "PreToolUse Routing, Think-in-Code Enforcement, Priority Tiers & Tantivy Optimization",
    "date": datetime.now().strftime("%Y-%m-%d"),
    "authors": ["TACO (Touring Agentic Code Orchestrator)"],
    "version": "v1.0",
    "status": "DRAFT",
}


PRIORITY_TABLE = [
    ("P0", "D2 PreToolUse Router",    "98% token reduction",   "Alta",  "None"),
    ("P1", "D4 Think-in-Code PreRead","Previne context burn", "Média", "CILA budget"),
    ("P1", "D3 Priority Tiers",       "Session continuity",    "Média", "hook_events schema"),
    ("P2", "Tantivy Porter Stemming", "Search quality +20%",   "Baixa", "None"),
    ("P2", "D6 MCP Context Router",  "Multi-agent support",    "Alta",  "MCP server"),
    ("P3", "Tantivy Trigram Index",   "Fuzzy search quality",  "Média", "Dual schema"),
    ("P3", "community_id RRF boost", "Topological search",     "Baixa", "Schema v3"),
]

RISKS = [
    {
        "id": "R1",
        "description": "PreToolUse Router intercepta ferramentas legítimas causando comportamento inesperado",
        "probability": "MEDIUM",
        "impact": "HIGH",
        "mitigation": "Feature flag `TOURING_HOOK_ROUTING=0` para disable; threshold configurável por tool via env var",
        "priority": "P0",
    },
    {
        "id": "R2",
        "description": "Sandbox subprocess adiciona latência >10ms violando CILA budget",
        "probability": "LOW",
        "impact": "MEDIUM",
        "mitigation": "Async spawn com timeout configurável; fallback para bypass se timeout exceeded",
        "priority": "P0",
    },
    {
        "id": "R3",
        "description": "Tantivy schema migration quebra índices existentes (v1/v2 → v3)",
        "probability": "MEDIUM",
        "impact": "HIGH",
        "mitigation": "Schema version check com rebuild automático quando mismatch detectado; `touring tantivy reindex` como recovery",
        "priority": "P0",
    },
    {
        "id": "R4",
        "description": "Priority tiers classification heuristics erráticas causam CRITICAL events sendo dropped",
        "probability": "LOW",
        "impact": "HIGH",
        "mitigation": "Classification audit via `touring hook_events --filter priority=CRITICAL` para validação manual",
        "priority": "P1",
    },
    {
        "id": "R5",
        "description": "Think-in-Code directive muito agressiva causa recusa de contexto legítimo",
        "probability": "MEDIUM",
        "impact": "LOW",
        "mitigation": "CILA-level awareness: só injeta directive quando budget < 50% remanescente",
        "priority": "P1",
    },
    {
        "id": "R6",
        "description": "Dual-schema Trigram index dobra storage requirements",
        "probability": "HIGH",
        "impact": "LOW",
        "mitigation": "Feature-gated sob `tantivy-trigram`; default OFF; storage metric em gate_metrics",
        "priority": "P3",
    },
]

DELIVERABLES = [
    {
        "id": "D2",
        "name": "D2 — PreToolUse Output Router",
        "t_shirt": "XL",
        "summary": (
            "Intercepts tool calls BEFORE output enters context. Routes large-output "
            "commands (>10KB) to sandbox subprocess + Tantivy storage. Returns only "
            "content_hash + summary reference."
        ),
        "components": [
            {
                "name": "D2.1 — ToolOutputRouter struct",
                "file": "crates/touring-hooks/src/tool_output_router.rs",
                "new": True,
                "description": (
                    "Core router with size_threshold, blocked_commands list, "
                    "should_intercept() decision logic."
                ),
                "size": "120 LOC",
                "tests": "test_should_intercept_* (5 cases)",
            },
            {
                "name": "D2.2 — SandboxExecutor integration",
                "file": "crates/touring-hooks/src/sandbox_executor.rs",
                "new": True,
                "description": (
                    "PolyglotExecutor wrapper for cross-language code execution. "
                    "Captures stdout/stderr up to 100MB. Returns SandboxResult."
                ),
                "size": "200 LOC",
                "tests": "test_sandbox_execute_* (4 cases)",
            },
            {
                "name": "D2.3 — Tantivy context storage",
                "file": "crates/touring-hooks/src/tantivy_index.rs",
                "modify": True,
                "description": (
                    "Extend SymbolDoc with context_reference, content_hash fields. "
                    "Add retrieve_context(ref_id) method."
                ),
                "size": "80 LOC",
                "tests": "test_context_storage_* (3 cases)",
            },
            {
                "name": "D2.4 — PreToolUse hook handler",
                "file": "crates/touring-hooks/src/pre_tool_use.rs",
                "new": True,
                "description": (
                    "Hook handler that intercepts Bash/Read/Grep tools. "
                    "Routes to router. Emits ContextReference HookResponse."
                ),
                "size": "150 LOC",
                "tests": "test_pretToolUse_routing_* (6 cases)",
            },
            {
                "name": "D2.5 — Feature flag + env vars",
                "file": "crates/touring-hooks/src/shared/feature_flags.rs",
                "modify": True,
                "description": (
                    "TOURING_HOOK_ROUTING=0|1, TOURING_ROUTING_THRESHOLD_KB=10, "
                    "TOURING_BLOCKED_TOOLS=docker,kubectl,curl"
                ),
                "size": "40 LOC",
                "tests": "test_feature_flags (already exists)",
            },
        ],
        "metrics": {
            "token_reduction": "98%",
            "baseline_tool_output": "30MB per 20 tools × 50 turns",
            "with_router": "1MB per same workflow",
        },
        "dependencies": [],
        "verification": [
            "cargo test -p touring-hooks --lib tool_output_router",
            "cargo test -p touring-hooks --lib sandbox_executor",
            "E2E: touring pre-tool-use --dry-run gh issue list",
            "E2E: measure token count before/after via /ctx-insight",
        ],
    },
    {
        "id": "D4",
        "name": "D4 — Think-in-Code PreRead Enforcement",
        "t_shirt": "M",
        "summary": (
            "Detects analysis patterns (bulk read, search aggregation) in pre-read hook "
            "and injects Think-in-Code directive via CILA-aware budget allocation."
        ),
        "components": [
            {
                "name": "D4.1 — AnalysisPattern detector",
                "file": "crates/touring-hooks/src/pre_read.rs",
                "modify": True,
                "description": (
                    "detect_analysis_pattern() identifies BulkRead, SearchAnalysis, "
                    "Aggregation patterns from tool_name + tool_input."
                ),
                "size": "60 LOC",
                "tests": "test_analysis_pattern_detection (4 cases)",
            },
            {
                "name": "D4.2 — ThinkInCode directive injection",
                "file": "crates/touring-hooks/src/pre_read.rs",
                "modify": True,
                "description": (
                    "inject_think_in_code_directive() adds mandatory directive to "
                    "HighSignalContext when analysis pattern detected + CILA < 50%."
                ),
                "size": "45 LOC",
                "tests": "test_think_in_code_injection (3 cases)",
            },
            {
                "name": "D4.3 — SkipRegion PreRead check",
                "file": "crates/touring-hooks/src/pre_read.rs",
                "modify": True,
                "description": (
                    "pre_read_skip_region_check() reads file before processing to "
                    "warn about skip-regions that would block edits."
                ),
                "size": "35 LOC",
                "tests": "test_skip_region_preread_warning (2 cases)",
            },
        ],
        "metrics": {
            "context_burn_prevention": "~60% reduction in bulk-read analysis patterns",
            "directive_overhead": "200-400 tokens per injection",
        },
        "dependencies": ["D2 (for router context)"],
        "verification": [
            "cargo test -p touring-hooks --lib pre_read -- think_in_code",
            "E2E: Read 10 files → verify directive injected in context",
        ],
    },
    {
        "id": "D3",
        "name": "D3 — Session Snapshot Priority Tiers",
        "t_shirt": "M",
        "summary": (
            "Classifies hook_events into CRITICAL/HIGH/MEDIUM/LOW tiers. "
            "PreCompact filter carries only CRITICAL+HIGH through context compaction."
        ),
        "components": [
            {
                "name": "D3.1 — EventPriority enum",
                "file": "crates/touring-hooks/src/shared/hook_events.rs",
                "new": True,
                "description": "EventPriority::{Critical, High, Medium, Low} variants.",
                "size": "25 LOC",
                "tests": "test_priority_classification (5 cases)",
            },
            {
                "name": "D3.2 — classify_event_priority()",
                "file": "crates/touring-hooks/src/shared/hook_events.rs",
                "modify": True,
                "description": (
                    "Pattern-matches event_type + outcome_linked + content_hash "
                    "to assign priority tier."
                ),
                "size": "50 LOC",
                "tests": "test_classify_event_priority (6 cases)",
            },
            {
                "name": "D3.3 — PreCompact filter enhancement",
                "file": "crates/touring-hooks/src/hook_memory.rs",
                "modify": True,
                "description": (
                    "precompact_filter() uses priority tiers to decide which "
                    "events carry through compaction."
                ),
                "size": "40 LOC",
                "tests": "test_precompact_priority_filter (3 cases)",
            },
            {
                "name": "D3.4 — priority_tier column in hook_events",
                "file": "crates/touring-hooks/src/hook_memory.rs",
                "modify": True,
                "description": "ALTER TABLE hook_events ADD COLUMN priority_tier TEXT DEFAULT 'LOW'",
                "size": "20 LOC + schema migration",
                "tests": "test_priority_tier_migration (idempotent)",
            },
        ],
        "metrics": {
            "session_continuity": "CRITICAL events preserved 100%; MEDIUM+LOW dropped in compaction",
            "compaction_size_reduction": "~40% smaller compaction payload",
        },
        "dependencies": [],
        "verification": [
            "cargo test -p touring-hooks --lib hook_memory -- priority",
            "touring hook_events --filter priority=CRITICAL",
            "E2E: session with error events → verify survive PreCompact",
        ],
    },
    {
        "id": "P2-STEM",
        "name": "P2 — Tantivy Porter Stemming",
        "t_shirt": "S",
        "summary": (
            "Replace default tokenizer with `en_stem` (Porter stemmer) on symbol_name "
            "and docstring FTS fields. Enables morphological normalization."
        ),
        "components": [
            {
                "name": "STEM-1 — Update TextFieldIndexing options",
                "file": "crates/touring-hooks/src/tantivy_index.rs",
                "modify": True,
                "description": (
                    "Change .set_tokenizer(\"default\") to .set_tokenizer(\"en_stem\") "
                    "on symbol_name and docstring fields."
                ),
                "size": "4 LOC",
                "tests": "test_stemming_index (existing integration test)",
            },
            {
                "name": "STEM-2 — Rebuild index documentation",
                "file": "docs/touring-tantivy-rebuild.md",
                "new": True,
                "description": (
                    "Document that en_stem requires `touring tantivy reindex` after change."
                ),
                "size": "30 LOC",
                "tests": None,
            },
        ],
        "metrics": {
            "search_quality_improvement": "~20% recall on morphological queries",
            "example": '"running" matches "run", "runs", "ran"',
        },
        "dependencies": ["Schema migration (D2.3)"],
        "verification": [
            "cargo test -p touring-hooks --lib tantivy",
            "touring tantivy search 'execut' | grep executor  # should match",
        ],
    },
    {
        "id": "D6",
        "name": "D6 — MCP Context Router para Agents",
        "t_shirt": "L",
        "summary": (
            "Reposition existing 96 MCP tools as a 'context router' for multi-agent "
            "scenarios. Expose ctx_search, ctx_index, ctx_retrieve, ctx_insight, ctx_compress."
        ),
        "components": [
            {
                "name": "D6.1 — ctx_search MCP tool",
                "file": "crates/touring-hooks/src/cli_handlers_mcp.rs",
                "new": True,
                "description": (
                    "hook_events search with priority_filter + time_range + "
                    "BM25 ranking over event content."
                ),
                "size": "80 LOC",
                "tests": "test_mcp_ctx_search (4 cases)",
            },
            {
                "name": "D6.2 — ctx_index MCP tool",
                "file": "crates/touring-hooks/src/cli_handlers_mcp.rs",
                "new": True,
                "description": (
                    "Store tool output + metadata in hook_events with content_hash. "
                    "Returns reference_id."
                ),
                "size": "60 LOC",
                "tests": "test_mcp_ctx_index (3 cases)",
            },
            {
                "name": "D6.3 — ctx_retrieve MCP tool",
                "file": "crates/touring-hooks/src/cli_handlers_mcp.rs",
                "new": True,
                "description": (
                    "Retrieve full content by content_hash from TantivyIndex. "
                    "Supports pagination."
                ),
                "size": "50 LOC",
                "tests": "test_mcp_ctx_retrieve (3 cases)",
            },
            {
                "name": "D6.4 — ctx_insight MCP tool",
                "file": "crates/touring-hooks/src/cli_handlers_mcp.rs",
                "new": True,
                "description": (
                    "Session analytics: tool usage breakdown, error rate, "
                    "explore/execute ratio, 15 metrics."
                ),
                "size": "100 LOC",
                "tests": "test_mcp_ctx_insight (2 cases)",
            },
            {
                "name": "D6.5 — MCP tool registration",
                "file": "crates/touring-hooks/src/mcp/mod.rs",
                "modify": True,
                "description": "Register ctx_* tools in MCP server registry.",
                "size": "30 LOC",
                "tests": "test_mcp_registration (1 case)",
            },
        ],
        "metrics": {
            "multi_agent_support": "Agent B can retrieve context stored by Agent A via content_hash",
            "mcp_tool_surface": "5 new tools (ctx_search, ctx_index, ctx_retrieve, ctx_insight, ctx_compress)",
        },
        "dependencies": ["D2 (for content_hash deduplication)"],
        "verification": [
            "cargo test -p touring-hooks --lib cli_handlers_mcp",
            "MCP protocol test: list tools → verify ctx_* present",
        ],
    },
    {
        "id": "P3-TRIG",
        "name": "P3 — Tantivy Trigram Index with RRF",
        "t_shirt": "M",
        "summary": (
            "Add dual-index: porter (BM25) + trigram (substring fuzzy). "
            "Merge via Reciprocal Rank Fusion (k=60) for typo-tolerant partial match."
        ),
        "components": [
            {
                "name": "TRIG-1 — Trigram tokenizer field",
                "file": "crates/touring-hooks/src/tantivy_index.rs",
                "modify": True,
                "description": (
                    "Add symbol_name_trigram field with trigram tokenizer. "
                    "Add search_trigram() method."
                ),
                "size": "40 LOC",
                "tests": "test_trigram_search (3 cases)",
            },
            {
                "name": "TRIG-2 — RRF merge",
                "file": "crates/touring-hooks/src/tantivy_index.rs",
                "modify": True,
                "description": (
                    "Implement search_with_rrf() merging porter + trigram hits. "
                    "k=60 constant for rank fusion."
                ),
                "size": "50 LOC",
                "tests": "test_rrf_merge (4 cases)",
            },
            {
                "name": "TRIG-3 — Feature gate",
                "file": "crates/touring-hooks/src/shared/feature_flags.rs",
                "modify": True,
                "description": "Feature flag `tantivy-trigram` default OFF.",
                "size": "10 LOC",
                "tests": "test_feature_gate (already exists)",
            },
        ],
        "metrics": {
            "fuzzy_match_improvement": '"useEff" → "useEffect" with trigram distance 2',
            "storage_overhead": "+30% index size when enabled",
        },
        "dependencies": ["D2.3 (context storage)"],
        "verification": [
            "cargo test -p touring-hooks --lib tantivy -- trigram",
            "touring tantivy fuzzy 'useEff' 2 | grep useEffect",
        ],
    },
    {
        "id": "P3-COMM",
        "name": "P3 — community_id RRF Boost",
        "t_shirt": "S",
        "summary": (
            "Schema v3 has community_id (Louvain) but search doesn't use it. "
            "Implement topological boost: same community = 1.3x score multiplier."
        ),
        "components": [
            {
                "name": "COMM-1 — search_with_community_boost()",
                "file": "crates/touring-hooks/src/tantivy_index.rs",
                "modify": True,
                "description": (
                    "Filter hits by community_id, apply 1.3x score boost to "
                    "same-community results."
                ),
                "size": "30 LOC",
                "tests": "test_community_boost (2 cases)",
            },
        ],
        "metrics": {
            "topological_relevance": "Symbols from same module/crate ranked higher",
        },
        "dependencies": ["Schema v3 (already implemented)"],
        "verification": [
            "cargo test -p touring-hooks --lib tantivy -- community",
            "touring query 'symbol_name:fn community_id=5' | head -10",
        ],
    },
]


def render_table_row(priority, opportunity, impact, complexity, dependency, widths=(10, 28, 20, 14, 20)):
    def fmt(s, w):
        return s.ljust(w)[:w]
    sep = "│"
    return sep + "".join(
        f" {fmt(x, w)} " + sep
        for x, w in zip([priority, opportunity, impact, complexity, dependency], widths)
    )


def render_deliverable(d: dict) -> str:
    lines = []
    lines.append(f"\n### {d['id']}: {d['name']}\n")
    lines.append(f"**T-Shirt Size**: {d['t_shirt']} | **Summary**: {d['summary']}\n")

    lines.append("#### Componentes\n")
    lines.append("| # | Component | File | Type | Size | Tests |")
    lines.append("|---|-----------|------|------|------|-------|")
    for i, comp in enumerate(d["components"], 1):
        kind = "NEW" if comp.get("new") else "MODIFY" if comp.get("modify") else "—"
        lines.append(
            f"| {i} | {comp['name']} | `{comp['file']}` | {kind} | "
            f"{comp['size']} | {comp.get('tests', '—')} |"
        )

    if d.get("metrics"):
        lines.append("\n#### Métricas de Impacto\n")
        for k, v in d["metrics"].items():
            lines.append(f"- **{k}**: {v}")

    if d.get("dependencies"):
        lines.append(f"\n**Dependencies**: {', '.join(d['dependencies'])}\n")

    lines.append("\n**Verification**:\n")
    for v in d.get("verification", []):
        lines.append(f"- [ ] {v}")

    return "\n".join(lines)


def generate() -> str:
    lines = []

    # Header
    lines.append(f"# {PLAN['title']}\n")
    lines.append(f"**{PLAN['subtitle']}**\n")
    lines.append(f"- **Date**: {PLAN['date']}  ")
    lines.append(f"- **Authors**: {', '.join(PLAN['authors'])}  ")
    lines.append(f"- **Version**: {PLAN['version']}  ")
    lines.append(f"- **Status**: {PLAN['status']}\n")

    # Context
    lines.append("---\n\n## Contexto: context-mode Analysis\n")
    lines.append("""context-mode (mksglu/context-mode) achieves **98% token reduction**
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
""")

    # Priority Table
    lines.append("\n---\n\n## Priorização de Implementação\n")
    widths = (10, 28, 20, 14, 20)
    header = render_table_row("Prioridade", "Opportunidade", "Impacto", "Complexidade", "Dependência", widths)
    separator = "|" + "".join("-" * (w + 2) + "|" for w in widths)
    lines.append("```\n" + header + "\n" + separator)
    for row in PRIORITY_TABLE:
        lines.append(render_table_row(*row, widths))
    lines.append("```\n")

    # Deliverables
    lines.append("\n---\n\n## Deliverables\n")
    for d in DELIVERABLES:
        lines.append(render_deliverable(d))

    # Risks
    lines.append("\n---\n\n## Riscos e Mitigações\n")
    lines.append("| ID | Descrição | Probabilidade | Impacto | Mitigação | Deliverable |")
    lines.append("|-----|-----------|---------------|---------|-----------|-------------|")
    for r in RISKS:
        lines.append(
            f"| {r['id']} | {r['description']} | {r['probability']} | "
            f"{r['impact']} | {r['mitigation']} | {r['priority']} |"
        )

    # Timeline
    lines.append("\n---\n\n## Timeline Sugerido (Sprints)\n")
    lines.append("""
| Sprint | Entregas | T-Shirt Total | Notas |
|-------|---------|---------------|-------|
| **Sprint 1** | D2.1, D2.2, D2.3, D2.5 | XL+XL+XL+M | PreToolUse Router core + feature flags |
| **Sprint 2** | D2.4, D4.1, D4.2 | XL+M+M | Hook handler + Think-in-Code injection |
| **Sprint 3** | D3.1–D3.4 | M+M+M+M | Priority Tiers + PreCompact filter |
| **Sprint 4** | P2-STEM, P3-COMM, D6.1 | S+S+L | Porter Stemming + RRF Boost + MCP search |
| **Sprint 5** | D6.2–D6.5, P3-TRIG | L+L+M | Full MCP router + Trigram index |
""")

    # Quick Wins
    lines.append("\n---\n\n## Quick Wins (Implementação Imediata)\n")
    lines.append("""
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
""")

    # Verification
    lines.append("\n---\n\n## Verificação Geral\n")
    lines.append("""
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
""")

    # Footer
    lines.append(f"\n---\n*Generated: {datetime.now().isoformat()} | TACO v7.0 | Touring Daemon v30.3.0*\n")

    return "\n".join(lines)


if __name__ == "__main__":
    import sys
    os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
    content = generate()
    with open(OUTPUT_PATH, "w", encoding="utf-8") as f:
        f.write(content)
    sys.stdout.write(f"Plan: {OUTPUT_PATH} ({content.count(chr(10))} lines)\n")