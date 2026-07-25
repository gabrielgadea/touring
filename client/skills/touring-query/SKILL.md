---
name: touring-query
description: Query file metadata with a DSL. Use for filtered searches like "find all Rust files with >100 LOC and cognitive score > 0.7".
---
# touring-query

## DSL Syntax

```
touring query "field op value [AND|OR field op value]*"
```

## Fields

| Field | Type | Description |
|-------|------|-------------|
| lang | string | Language (rust, python, typescript, etc.) |
| loc | int | Lines of code |
| todos | int | TODO/FIXME count |
| fan_in | float | Fan-in signal |
| fan_out | float | Fan-out signal |
| cognitive_score | float | Cognitive complexity score |
| features | string | Feature flag name |

## Operators

`=`, `!=`, `>`, `<`, `>=`, `<=`, `LIKE`

## Examples

```bash
touring query "lang = rust AND loc > 100"
touring query "todos > 5 AND cognitive_score < 0.5"
touring query "lang = python AND fan_in > 3.0"
```
