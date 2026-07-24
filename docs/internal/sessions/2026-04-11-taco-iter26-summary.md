# TACO Iter26 — EC42: predict_coedit_files wired via RRF upgrade em resolve_ctx

**Data**: 2026-04-11
**Iteração**: 26
**EC implementado**: EC42
**Arquivo modificado**: `graph_service.rs`
**Resultado**: 0 erros cargo check, 330 tests passing (touring-server), sem regressão

---

## EC42 — `predict_coedit_files()` wired — upgrade de coedit_files para RRF

### Problema
`GraphService::predict_coedit_files()` existia com **0 callers** em produção.
O método implementa RRF (Reciprocal Rank Fusion) sobre 3 sinais:
- co-edits históricos (33%): `TABLE_FILE_COEDITS` via `get_coedits_from()`
- imports (33%): `SymbolIndex.dependencies` (AST)
- blast_radius (33%): `SymbolIndex.reverse_deps`

Em `resolve_ctx()`, o campo `coedit_files` era populado apenas via `adb.get_coedits_from()`
— apenas 1/3 do sinal disponível. Os outros 2/3 (imports + blast_radius) eram ignorados,
mesmo já estando disponíveis no contexto da função.

### Problema Técnico: Deadlock Potencial
`resolve_ctx()` adquiria `self.index.lock()` na linha 343 e mantinha o lock até o fim
da função (inclusive durante todas as queries KB async). `predict_coedit_files()` também
adquire `self.index.lock()` internamente. Chamar diretamente causaria deadlock.

### Solução
**Dois passos atômicos**:

1. **`drop(idx)` explícito** após último uso do índice (pós `neighbor_files.sort()`):
```rust
// EC42: drop idx before predict_coedit_files — that method re-acquires self.index.
// Without this explicit drop, calling predict_coedit_files would deadlock since both
// resolve_ctx and predict_coedit_files try to acquire the same Arc<Mutex<SymbolIndex>>.
drop(idx);
drop(indices);
```

2. **Substituição do bloco `coedit_files`**:
```rust
// EC42: Upgrade coedit signal from raw DB (get_coedits_from) to RRF-predicted.
// predict_coedit_files() fuses three signals via CoEditPredictor::predict_next_files():
//   - co-edits (1/3 weight): historical TABLE_FILE_COEDITS pairs
//   - imports (1/3 weight): files this file depends on (SymbolIndex.dependencies)
//   - blast_radius (1/3 weight): files that import this file (reverse_deps)
// First real caller of predict_coedit_files() — activates full RRF over all 26 MCP tools.
let coedit_files: Vec<String> = self
    .predict_coedit_files(file, 5)
    .await
    .into_iter()
    .map(|(path, _score)| path)
    .collect();
```

### Design Decisions
- `drop(idx)` e `drop(indices)` reordenados — ambos dropped antes das queries async
- `predict_coedit_files` já lida com `async_knowledge == None` (retorna co-edits vazio, usa imports+blast_radius)
- Interface de `coedit_files` preservada: `Vec<String>` — zero API change
- RRF scores são descartados (`.map(|(path, _score)| path)`) — suficiente para o LLM

### Impacto
`predict_coedit_files()` agora tem **1 caller real** (era 0) em `resolve_ctx`.
**Todos os 26 MCP tools** que expõem `graph_ctx.coedit_files` passam a usar sinal RRF.
Qualidade do sinal: de 1-signal (co-edits históricos) para 3-signal RRF (co-edits + imports + blast_radius).

---

## Estado de GraphService methods (após EC42)

| Método | Callers | EC |
|--------|---------|-----|
| `resolve_ctx` | 26+ (server/mod.rs) | original |
| `stats` | 2 (touring_project, touring_health) | EC38+EC40 |
| `hotspots` | 2 (2 MCP tools in server/mod.rs) | original |
| `predict_coedit_files` | 1 (resolve_ctx) | **EC42** |
| `expand_neighbors` | 0 (test only) | candidato EC43 |
| `update_focus` | N (server/mod.rs) | original |
| `inject` | 26+ (server/mod.rs) | original |
| `compute_confidence_modifier` | 1 (resolve_ctx) | original |

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-server  → 330 passed, 0 failed
```

---

## Próximo candidato: EC43

`expand_neighbors()` — definido em `graph_service.rs:556`, testado em `graph_service_e2e.rs:264`
mas tem **0 callers** em produção (server/mod.rs). Candidato para wiring em um MCP tool
que se beneficia de expansão de contexto de vizinhos.
