# TACO Iter19 — EC24 + EC25: relation_count + line_count/symbol_count em GraphFocusCtx

**Data**: 2026-04-11
**Iteração**: 19
**ECs implementados**: EC24, EC25
**Arquivo modificado**: `crates/touring-server/src/graph_service.rs`
**Resultado**: 0 erros cargo check, 0/0 server tests, sem regressão

---

## EC24 — relation_count em GraphFocusCtx via adb.get_relations_from()

### Problema
`AsyncFileKnowledgeDB::get_relations_from()` — 0 callers de produção (confirmado por scout).
`TABLE_FILE_RELATIONS` (relation_type, source_path, target_path) nunca exposta em GraphFocusCtx.
`access_count`, `edit_count`, `read_count` já existiam (EC18/EC20/EC23) — pattern estabelecido.

### 4-Step Pattern (EC18/EC20/EC23 → EC24)

**Step 1**: Campo `relation_count: i64` adicionado a `GraphFocusCtx` após `read_count`
**Step 2**: `Default::relation_count = 0`
**Step 3**: Em `resolve_ctx()`:
```rust
// EC24: semantic relation count from TABLE_FILE_RELATIONS via adb.get_relations_from().
// Distinct from SymbolIndex.dependencies (AST imports) — covers cross-file semantic links
// recorded by post_edit and post_write hooks (relation_type field).
let relation_count: i64 = if let Some(ref adb) = self.async_knowledge {
    adb.get_relations_from(file).await.unwrap_or_default().len() as i64
} else {
    0
};
```
**Step 4**: Em `inject()`: `"relation_count": ctx.relation_count`

### Impacto
`AsyncFileKnowledgeDB::get_relations_from()` agora tem **1 caller de produção** (era 0).
GraphFocusCtx agora tem **12 campos** (era 11).

---

## EC25 — line_count + symbol_count em GraphFocusCtx (reutilizando lookup EC23)

### Problema
`FileKnowledge.line_count` e `FileKnowledge.symbol_count` (TABLE_FILE_KNOWLEDGE) nunca expostos em GraphFocusCtx.
Ambos disponíveis no mesmo `adb.lookup()` já executado para `read_count` (EC23).

### Otimização: Single Roundtrip
EC25 refatora o bloco EC23 para capturar os 3 campos de uma vez:

```rust
// EC23+EC25: single adb.lookup() call extracts read_count, line_count, symbol_count
// from TABLE_FILE_KNOWLEDGE (populated by pre-read hook). One roundtrip for three fields.
let (read_count, line_count, symbol_count): (i64, i64, i64) =
    if let Some(ref adb) = self.async_knowledge {
        adb.lookup(file)
            .await
            .ok()
            .flatten()
            .map(|k| (k.read_count, k.line_count, k.symbol_count))
            .unwrap_or((0, 0, 0))
    } else {
        (0, 0, 0)
    };
```

### Impacto
Sem nova roundtrip de DB — 3 campos de 1 chamada.
GraphFocusCtx agora tem **14 campos** (era 12 após EC24).

---

## Estado Acumulado de GraphFocusCtx

| Campo | Tabela | EC |
|-------|--------|-----|
| `focused_file` | — | original |
| `imports` | SymbolIndex | original |
| `imported_by` | SymbolIndex | original |
| `blast_radius_count` | SymbolIndex | original |
| `neighbor_files` | SymbolIndex | original |
| `confidence_modifier` | — | original |
| `source` | — | original |
| `coedit_files` | TABLE_FILE_COEDITS | EC11 |
| `access_count` | TABLE_FILE_ACCESS_LOG | EC18 |
| `edit_count` | TABLE_EDIT_HISTORY | EC20 |
| `read_count` | TABLE_FILE_KNOWLEDGE | EC23 |
| `relation_count` | TABLE_FILE_RELATIONS | EC24 |
| `line_count` | TABLE_FILE_KNOWLEDGE | EC25 |
| `symbol_count` | TABLE_FILE_KNOWLEDGE | EC25 |

**Total**: 14 campos (era 7 original)

---

## AsyncFileKnowledgeDB — Callers de Produção (acumulado)

| Método | EC | Wired em |
|--------|-----|----------|
| `get_coedits_from` | EC11 | graph_service.rs (predict_coedit_files) |
| `access_count` | EC18 | graph_service.rs (resolve_ctx) |
| `edit_count_for_file` | EC20 | graph_service.rs (resolve_ctx) |
| `recent_bash_outcomes` | EC21 | bridge.rs (resolve_enriched) |
| `lookup` | EC23 | graph_service.rs (resolve_ctx) |
| `get_relations_from` | EC24 | graph_service.rs (resolve_ctx) |

Ainda sem callers de produção: `upsert`, `record_bash_outcome`, `record_edit`, `stats`, `record_access`, `wal_checkpoint`, `record_coedit`

---

---

## EC26 — relation_count em knowledge_activity (cli_e2e.rs)

### Problema
`TABLE_FILE_RELATIONS` existia no schema e era usada por `AsyncFileKnowledgeDB::get_relations_from()` (EC24),
mas nunca aparecia no output do `touring e2e`. O `knowledge_activity` block tinha `access_count`,
`bash_count`, `edit_count`, `gotcha_count`, `coedit_pairs` — mas não `relation_count`.

### Mudança
Query direta `SELECT COUNT(*) FROM file_relations` via `db.conn_ref()` seguindo o mesmo
pattern das queries T2/T3/T7/EC19b já existentes:

```rust
// EC26: Semantic relation count from TABLE_FILE_RELATIONS.
let relation_count: i64 = db
    .conn_ref()
    .query_row(
        &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_FILE_RELATIONS),
        [],
        |r| r.get(0),
    )
    .unwrap_or(0);
```

Adicionado ao `knowledge_activity` block em `PhaseResult::metrics`:
```json
"knowledge_activity": {
    "access_count": ...,
    "bash_count": ...,
    "edit_count": ...,
    "gotcha_count": ...,
    "coedit_pairs": ...,
    "relation_count": ...   // EC26 — novo
}
```

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-server  → ok (0 tests, no regression)
cargo test -p touring-hooks   → 1452 passed, 0 failed
```
