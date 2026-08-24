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

## Três linhas desta tabela são uma chamada só

`touring read <file>` funde `ast meta` + `ast overview` + semântica; `touring blast
<file>` funde `ast blast` + ciclos. Encadear `meta` → `overview` → `blast` à mão
paga três round-trips pelo que o master devolve em um — e é o mesmo ganho
`U(a)=P·V−C(tokens)` do Code Mode, aplicado à descoberta. Use os atômicos abaixo
quando quiser **um campo específico** que já sabe nomear; use o master quando a
pergunta ainda é "o que é este arquivo". Ref: `Touring/references/skill-operating-principles.md` (P1).

## When to Use
- Before editing a file: check blast radius and quality score
- When exploring: use skeleton depth for quick overview
- For code review: full depth shows all relationships
- To find TODOs: `touring ast todos` across project

## MCP Tools
- `mcp__touring__touring_ast_overview` — Symbol overview
- `mcp__touring__touring_ast_find` — Find symbol definitions
