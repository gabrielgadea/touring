---
name: touring-file-metadata
description: Query and manage file metadata via touring CLI. Use when needing file-level insights like LOC, language, quality scores, symbols, or blast radius.
---
# touring-file-metadata

## Commands

| Command | Purpose |
|---------|---------|
| `touring ast meta <file> --depth skeleton` | Minimal: symbols + language + LOC |
| `touring ast meta <file> --depth summary` | + quality + blast + fan + cognitive |
| `touring ast meta <file> --depth full` | + call_graph + imports + todos + features |
| `touring ast blast <file>` | Blast radius analysis |
| `touring ast overview <file>` | Symbol overview |
| `touring ast callgraph <file>` | Call graph relationships |
| `touring ast todos <file>` | TODO/FIXME annotations |
| `touring ast features <file>` | Feature flags in file |
| `touring ast skeleton <file>` | Pub symbols skeleton |

## When to Use
- Before editing a file: check blast radius and quality score
- When exploring: use skeleton depth for quick overview
- For code review: full depth shows all relationships
- To find TODOs: `touring ast todos` across project

## MCP Tools
- `mcp__touring__touring_ast_overview` — Symbol overview
- `mcp__touring__touring_ast_find` — Find symbol definitions
