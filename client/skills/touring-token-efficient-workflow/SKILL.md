---
name: touring-token-efficient-workflow
description: Token-efficient workflow patterns for Touring MCP tools. Use detail_level=minimal first, follow _next_tools suggestions, and call touring_minimal_context as entry point.
---

# Touring Token-Efficient Workflow (v31/v32)

> 42 MCP tools, 10 new modules, 14.9x token savings (minimal), 478 tests passing

## Rules

1. **ALWAYS** call `touring_minimal_context(task="<your task>")` FIRST (~100 tokens)
2. **ALWAYS** use `detail_level: "minimal"` on ALL subsequent tool calls
3. Only escalate to `"standard"` or `"full"` for specific entities needing deeper inspection
4. Follow `_next_tools` suggestions in every response for optimal workflow
5. Maximum 3-4 tool calls per turn unless absolutely necessary

## Workflow Templates

### Review Changes
```
1. touring_minimal_context(task="review changes")
2. IF risk="low": touring_detect_changes(detail_level="minimal") → report summary
3. IF risk="medium/high":
   a. touring_detect_changes(detail_level="standard")
   b. touring_ast_find(symbol_name="<high-risk symbol>", detail_level="minimal")
   c. touring_wiring(action="orphans", detail_level="minimal") only if orphan_count > 100
4. Summarize: risk level, what changed, test gaps, improvements needed
```

### Debug Issue
```
1. touring_minimal_context(task="debug: <description>")
2. touring_gotcha(action="list", file_path="<suspect file>", detail_level="minimal")
3. touring_memory_recall(query="<error pattern>", detail_level="minimal")
4. touring_ast_find(symbol_name="<suspect function>", detail_level="minimal")
5. Only escalate to detail_level="full" for the ONE function that's the root cause
```

### Refactor
```
1. touring_minimal_context(task="refactor <module>")
2. touring_ast_overview(file_path="<target>", detail_level="minimal")
3. touring_graph(action="blast_radius", file_path="<target>", detail_level="minimal")
4. touring_speculate(file_path="<target>", content="<new code>")
5. touring_wiring_audit(detail_level="minimal") after changes
```

### Architecture Exploration
```
1. touring_minimal_context(task="explore architecture")
2. touring_wiring(action="status", detail_level="minimal")
3. touring_wiring(action="modules", detail_level="minimal") for top modules
4. touring_ast_overview(file_path="<key file>", detail_level="standard")
```

### Pre-Merge Check
```
1. touring_minimal_context(task="pre-merge check")
2. touring_detect_changes(detail_level="minimal") for risk score
3. IF risk > 0.4: touring_wiring_audit(detail_level="minimal")
4. IF gotcha_warnings > 0: touring_gotcha(detail_level="minimal")
5. Output: GO/NO-GO with 1-sentence justification
```

## Detail Level Guide

| Level | Tokens | When to use |
|-------|--------|-------------|
| `minimal` | ~20-50 | First call, scanning, counts only |
| `standard` | ~100-200 | After identifying targets, summaries |
| `full` | ~500-2000 | Deep inspection of ONE specific entity |
