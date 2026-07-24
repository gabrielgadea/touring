# TACO Iter25 — EC40 + EC41: health_check enrichment + recent_bash_outcomes wired

**Data**: 2026-04-11
**Iteração**: 25
**ECs implementados**: EC40, EC41
**Arquivos modificados**: `server/mod.rs`, `graph_service.rs`, `async_knowledge.rs`
**Resultado**: 0 erros cargo check, 1782 tests passing (touring-hooks: 1452 + touring-server: 330), sem regressão

---

## EC40 — `touring_health` enriched com GraphService::stats()

### Problema
`touring_health` MCP tool retornava apenas `daemon_healthy: true` + `index` básico.
`GraphService::stats()` (enriquecida no EC38 com `knowledge_db` e métricas KB) nunca era
chamada pelo `health_check` handler — o LLM recebia saúde superficial sem contexto de KB.

### Mudança
**server/mod.rs** — handler `health_check` refatorado:
```rust
// EC40: Enrich health_check with GraphService stats (includes knowledge_db via EC38).
let graph_stats = self.graph_svc.stats().await;
let knowledge_db = graph_stats.get("knowledge_db").cloned()
    .unwrap_or(serde_json::Value::Null);

let mut output = serde_json::json!({
    "daemon_healthy": true,
    "index": {"symbol_count": symbol_count, "file_count": file_count},
    "knowledge_db": knowledge_db,
    "status": "ok",
});
```

**Design decisions**:
- `graph_svc.stats().await` chama async (inclui KB stats via EC38)
- Extrai apenas `knowledge_db` para manter o response focado (não duplica `symbol_count`)
- `unwrap_or(Null)` — backward compatible se KB indisponível
- `touring_health` agora expõe KB state sem API surface change

### Impacto
`touring_health` passa a expor `knowledge_db` com 6 métricas do KB.
LLM recebe visibilidade de saúde do knowledge DB na chamada de health check.
`GraphService::stats()` tem agora **2 callers**: `touring_project` (via `graph_ctx`) + `touring_health`.

---

## EC41 — `AsyncFileKnowledgeDB::recent_bash_outcomes()` wired em GraphService::stats()

### Problema
`AsyncFileKnowledgeDB::recent_bash_outcomes()` tinha **0 callers async** — método completo
que retorna `Vec<BashOutcomeRecord>` mas nunca invocado.
`GraphService::stats()` (chamada por `touring_project` + `touring_health`) ficava cega
a falhas bash recentes — sinal crítico para health assessment do projeto.

### Mudança
**async_knowledge.rs**: método `recent_bash_outcomes(limit)` já existia com signature:
```rust
pub async fn recent_bash_outcomes(&self, limit: usize) -> Result<Vec<BashOutcomeRecord>, AsyncKnowledgeError>
```

**graph_service.rs** — `GraphService::stats()` enriquecida:
```rust
// EC41: Wire recent_bash_outcomes() — first async caller of this method.
let recent_failures: Vec<serde_json::Value> = if let Some(ref adb) = self.async_knowledge {
    adb.recent_bash_outcomes(10).await.unwrap_or_default()
        .into_iter()
        .filter(|o| !o.success)
        .take(3)
        .map(|o| {
            let pattern = o.error_pattern.unwrap_or_default()
                .chars().take(80).collect::<String>();
            json!({"command": o.command_short, "file": o.file_context, "error": pattern})
        })
        .collect()
} else {
    vec![]
};

json!({
    "symbol_count": symbol_count,
    "file_count": file_count,
    "dependency_edge_count": dep_edges,
    "knowledge_db": kb_stats,
    "recent_failures": recent_failures,  // EC41: top 3 bash failures
})
```

**Design decisions**:
- Busca últimos 10 outcomes, filtra apenas `!success`, toma top 3 — signal compacto
- `error_pattern` truncado em 80 chars — evita flood de stack traces
- `vec![]` quando adb ausente — backward compatible
- `command_short` + `file_context` — contexto suficiente para diagnóstico
- Posicionado APÓS o `drop(idx)` — não bloqueia Mutex durante query async

### Impacto
`AsyncFileKnowledgeDB::recent_bash_outcomes()` agora tem **1 caller** (era 0).
`touring_project` e `touring_health` expõem `recent_failures: [{command, file, error}]`.
LLM recebe sinal de "últimas 3 falhas bash do projeto" — awareness proativa de problemas.

---

## Estado Acumulado de AsyncFileKnowledgeDB callers

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
| `recent_bash_outcomes` | EC41 | 1 (GraphService::stats) | EC41 |
| `record_access` | — | N (server) | pre-existing |
| `upsert` | — | 0 | (sync KB usado) |
| `record_coedit` | — | 0 | (intentional: sync) |

---

## GraphService::stats() — Response Completo (após EC38+EC41)

```json
{
  "symbol_count": 40275,
  "file_count": 1888,
  "dependency_edge_count": N,
  "knowledge_db": {
    "kb_file_count": N,
    "kb_relation_count": N,
    "kb_access_count": N,
    "kb_bash_count": N,
    "kb_edit_count": N,
    "kb_gotcha_count": N
  },
  "recent_failures": [
    {"command": "cargo build", "file": "src/main.rs", "error": "error[E0308]: mismatched types..."},
    {"command": "pytest", "file": null, "error": "ModuleNotFoundError: No module named..."}
  ]
}
```

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed
cargo test -p touring-server  → 330 passed, 0 failed
Total: 1782 passed, 0 failed
```
