# TACO Iter24 — EC38 + EC39: AsyncFileKnowledgeDB.stats() wired + gotcha_count em GraphFocusCtx

**Data**: 2026-04-11
**Iteração**: 24
**ECs implementados**: EC38, EC39
**Arquivos modificados**: `graph_service.rs`, `async_knowledge.rs`
**Resultado**: 0 erros cargo check, 1782 tests passing (touring-hooks: 1452 + touring-server: 330), sem regressão

---

## EC38 — AsyncFileKnowledgeDB::stats() wired em GraphService::stats()

### Problema
`GraphService::stats()` retornava apenas 3 campos do `SymbolIndex`:
```json
{"symbol_count": N, "file_count": N, "dependency_edge_count": N}
```
`AsyncFileKnowledgeDB::stats()` tinha **0 callers async** — método completo mas nunca invocado.
Todas as MCP tools que chamam `touring_project` / `touring_index_status` ficavam cegas ao
estado do knowledge DB (quantos arquivos conhecidos, quantas relações, quantas falhas bash).

### Mudança
`GraphService::stats()` enriquecido com chamada async a `adb.stats()`:
```rust
let kb_stats = if let Some(ref adb) = self.async_knowledge {
    match adb.stats().await {
        Ok(s) => json!({
            "kb_file_count": s.file_count,
            "kb_relation_count": s.relation_count,
            "kb_access_count": s.access_count,
            "kb_bash_count": s.bash_count,
            "kb_edit_count": s.edit_count,
            "kb_gotcha_count": s.gotcha_count,
        }),
        Err(_) => json!(null),
    }
} else {
    json!(null)
};
```

**Design decisions**:
- `drop(idx)` antes da chamada async para liberar o Mutex lock antes do await
- `null` quando adb não inicializado — backward compatible, não quebra consumers existentes
- 6 métricas do KB expostas: file_count, relation_count, access_count, bash_count, edit_count, gotcha_count

### Backward Compatibility
Teste `test_load_symbols_from_db_warm_start` checks `stats["symbol_count"]` e `stats["file_count"]`
— ambos ainda existem no JSON enriquecido. **0 regressões**.

### Impacto
`AsyncFileKnowledgeDB::stats()` agora tem **1 caller** (era 0).
`touring_project` e `touring_index_status` MCP tools expõem `knowledge_db` com métricas do KB.
LLM recebe visibilidade completa do estado do sistema de conhecimento em cada tool call.

---

## EC39 — gotcha_count: i64 em GraphFocusCtx (field #17)

### Problema
`GraphFocusCtx` tinha `file_notes` (EC28) com texto de gotchas truncado, mas nenhum
campo numérico direto para "quantos gotchas existem para este arquivo". O LLM precisava
parsear o texto de `file_notes` para inferir presença de pitfalls — sinal fraco e impreciso.

### Mudança

**async_knowledge.rs**: novo método `gotcha_count_for_file()`:
```rust
pub async fn gotcha_count_for_file(&self, file_path: &str) -> Result<i64, AsyncKnowledgeError> {
    // SELECT COUNT(*) FROM gotchas
    // WHERE ?1 LIKE '%' || pattern || '%'
    //   AND COALESCE(decay_score, 1.0) > 0.1
    //   AND resolved_at IS NULL
}
```
Query espelha exatamente `get_gotchas_for_file()` do sync KB — filtros idênticos (decay, resolved).

**graph_service.rs** — 5 mudanças atômicas:
1. Campo `gotcha_count: i64` em struct `GraphFocusCtx` (field #17)
2. `gotcha_count: 0` em `Default` impl
3. Query EC39 em `resolve_ctx()`: `adb.gotcha_count_for_file(file).await.unwrap_or(0)`
4. `gotcha_count` na struct initialization
5. `"gotcha_count": ctx.gotcha_count` em `inject()` output

### Impacto
`AsyncFileKnowledgeDB::gotcha_count_for_file()` tem **1 caller** imediatamente.
`GraphFocusCtx` agora tem **17 campos** (era 16 após EC37).
`graph_ctx.gotcha_count` é sinal direto: "N pitfalls conhecidos para este arquivo".

---

## Estado Acumulado de GraphFocusCtx fields

| Campo | EC | Source | Table |
|-------|-----|--------|-------|
| `focused_file` | original | params | — |
| `imports` | original | SymbolIndex | dependencies |
| `imported_by` | original | SymbolIndex | reverse_deps |
| `blast_radius_count` | original | len(imported_by) | — |
| `neighbor_files` | original | imports ∪ imported_by | — |
| `confidence_modifier` | original | blast_radius_count | — |
| `source` | original | enum | — |
| `coedit_files` | GS-EC11 | AsyncFileKnowledgeDB | TABLE_FILE_COEDITS |
| `access_count` | EC18 | AsyncFileKnowledgeDB | TABLE_FILE_ACCESS_LOG |
| `edit_count` | EC20 | AsyncFileKnowledgeDB | TABLE_EDIT_HISTORY |
| `read_count` | EC23 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.read_count |
| `relation_count` | EC24 | AsyncFileKnowledgeDB | TABLE_FILE_RELATIONS |
| `line_count` | EC25 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.line_count |
| `symbol_count` | EC25 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.symbol_count |
| `file_notes` | EC28 | AsyncFileKnowledgeDB | TABLE_FILE_KNOWLEDGE.notes |
| `bash_failures` | EC37 | AsyncFileKnowledgeDB | TABLE_BASH_OUTCOMES (success=0) |
| `gotcha_count` | EC39 | AsyncFileKnowledgeDB | TABLE_GOTCHAS (active, unresolved) |

**Total campos**: 17

---

## AsyncFileKnowledgeDB callers acumulados

| Método | ECs | Callers | Primeiro caller |
|--------|-----|---------|-----------------|
| `lookup` | EC23+EC25+EC28 | N (resolve_ctx) | EC23 |
| `access_count` | EC18 | N (resolve_ctx) | EC18 |
| `edit_count_for_file` | EC20 | N (resolve_ctx) | EC20 |
| `get_relations_from` | EC24 | N (resolve_ctx) | EC24 |
| `get_coedits_from` | GS-EC11 | N (predict_coedit_files) | GS-EC11 |
| `bash_failures_for_file` | EC37 | 1 (resolve_ctx) | EC37 |
| `gotcha_count_for_file` | EC39 | 1 (resolve_ctx) | EC39 |
| `stats` | EC38 | 1 (GraphService::stats) | EC38 |
| `record_access` | — | N (server) | pre-existing |
| `upsert` | — | 0 | (sync KB usado) |
| `record_coedit` | — | 0 | (intentional: sync) |
| `recent_bash_outcomes` | — | 0 | (candidato futuro) |

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed
cargo test -p touring-server  → 330 passed, 0 failed
Total: 1782 passed, 0 failed
```
