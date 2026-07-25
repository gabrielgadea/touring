---
name: touring-search
description: Search symbols and documentation via touring BM25 FTS5. Use when looking for symbols, patterns, or documentation across the codebase.
---
# touring-search

## Commands

| Command | Purpose |
|---------|---------|
| `touring search symbols "<query>" --top 10` | BM25 ranked symbol search |
| `touring search docs "<query>"` | Search knowledge context/docs |
| `touring index find <symbol>` | Exact symbol lookup |
| `touring index search <prefix>` | Prefix search in index |

## When to Use
- Finding symbol definitions: `touring search symbols "SymbolName"`
- Finding documentation: `touring search docs "error handling"`
- Quick lookup: `touring index find ExactSymbol`
