# TACO Iter27 — EC43: expand_neighbors wired via touring_graph action "neighbors"

**Data**: 2026-04-11
**Iteração**: 27
**EC implementado**: EC43
**Arquivos modificados**: `server/mod.rs`, `server/params.rs`
**Resultado**: 0 erros cargo check, 330 tests passing (touring-server), sem regressão

---

## EC43 — `expand_neighbors()` wired — nova action "neighbors" em touring_graph

### Problema
`GraphService::expand_neighbors()` existia com **0 callers** em produção.
O método retorna `imports ∪ imported_by` (deduped, sorted, limitado) para um arquivo —
designado para "query expansion" pelo docstring mas nunca exposto ao LLM via MCP.

`touring_graph` tinha 6 actions: index, blast_radius, dependency_path, imports, query, reload.
Nenhuma delas expunha a vizinhança completa de 1-hop de um arquivo.

### Mudança

**server/mod.rs** — novo arm `"neighbors"` no match de `touring_graph`:
```rust
"neighbors" => {
    let file = p.file_path
        .ok_or_else(|| McpError::invalid_params("'file_path' required for neighbors action", None))?;

    // EC43: First production caller of GraphService::expand_neighbors().
    // Returns imports ∪ imported_by (deduped, sorted) up to 20 files — used for
    // context/query expansion when the LLM needs the full 1-hop neighborhood.
    let neighbors = self.graph_svc.expand_neighbors(&file, 20).await;
    let neighbor_count = neighbors.len();

    serde_json::json!({
        "action": "neighbors",
        "file": file,
        "neighbors": neighbors,
        "neighbor_count": neighbor_count,
    })
}
```

**Atualizações de metadados**:
- Tool description: `"..., neighbors (1-hop expansion)"`
- Error message: lista de actions válidas inclui `neighbors`
- `params.rs` docstring: atualizado para listar `neighbors`

### Design Decisions
- `limit = 20` hardcoded — coerente com uso de `neighbor_files` em resolve_ctx (que não tem limite)
- Usa `p.file_path` (já existe em GraphParams) — zero mudanças de schema
- `graph_ctx` injetado via `inject()` como todos os outros actions — consistente
- `neighbor_count` incluído para conveniência do consumer

### Diferença entre "neighbors" e GraphFocusCtx.neighbor_files
| Aspecto | `neighbors` action | `neighbor_files` em graph_ctx |
|---------|--------------------|-----------------------------|
| Trigger | Explícito via `action: "neighbors"` | Automático em todo tool call |
| Limite | 20 | Sem limite (todos) |
| Deduplicate | Sim (HashSet) | Sim (HashSet) |
| Sorted | Sim | Sim |
| Source | `expand_neighbors()` | Mesmo código inline |

### Impacto
`GraphService::expand_neighbors()` tem agora **1 caller real** (era 0).
`touring_graph` passa de 6 para **7 actions**.
LLM pode agora pedir vizinhança expandida de qualquer arquivo via `action: "neighbors"`.

---

## Estado de GraphService methods (após EC43)

| Método | Callers | EC |
|--------|---------|-----|
| `resolve_ctx` | 26+ (server/mod.rs) | original |
| `stats` | 2 (touring_project, touring_health) | EC38+EC40 |
| `hotspots` | 2 (2 MCP tools in server/mod.rs) | original |
| `predict_coedit_files` | 1 (resolve_ctx) | EC42 |
| `expand_neighbors` | 1 (touring_graph "neighbors") | **EC43** |
| `update_focus` | N (server/mod.rs) | original |
| `inject` | 26+ (server/mod.rs) | original |
| `compute_confidence_modifier` | 1 (resolve_ctx) | original |

**Todos os métodos públicos de GraphService agora têm pelo menos 1 caller em produção.**

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-server  → 330 passed, 0 failed
```
