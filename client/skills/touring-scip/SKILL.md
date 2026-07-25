---
name: touring-scip
description: Export SCIP (Source Code Intelligence Protocol) documents from the touring symbol index. Use when needing symbol locations, definitions, and references for editor integrations or CI reports.
---

# touring-scip

Export SCIP-compatible symbol intelligence from Touring's index.

## When to use

- Generating symbol reports for code review or CI
- Building editor integrations (LSP bridges)
- Exporting symbol definitions and references for external tools
- Auditing symbol coverage across the codebase

## CLI Usage

```bash
# Emit SCIP document for a single file
touring scip emit <file_path> -j

# Example
touring scip emit crates/touring-hooks/src/knowledge.rs -j
```

## Rust API

The `ScipEmitter` is in `touring-server/src/scip_emit.rs`:

```rust
use touring_server::scip_emit::{ScipEmitter, ScipDocument};

let emitter = ScipEmitter::new(&symbol_store);
let doc: ScipDocument = emitter.emit_document("src/main.rs")?;

// Batch export
let docs = emitter.emit_batch(100); // top 100 files
```

## Output Format

```json
{
  "relative_path": "src/knowledge.rs",
  "language": "rust",
  "occurrences": [
    {
      "line": 42,
      "col_start": 7,
      "col_end": 25,
      "symbol": "insert_symbol_event",
      "role": "definition"
    }
  ]
}
```

## Integration with Query DSL

Combine with `touring query` for filtered exports:

```bash
# Find all function definitions in shared/
touring query "kind:fn AND file:shared/*" -j
```
