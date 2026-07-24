# TACO Iter34 — EC50: create_task() wired em touring_decompose

**Data**: 2026-04-11
**Iteração**: 34
**EC implementado**: EC50
**Arquivos modificados**: `crates/touring-server/src/server/mod.rs`, `crates/touring-server/src/reasoning/decomposer.rs`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), 330 tests passing (touring-server), sem regressão

---

## EC50 — `TaskDecomposer::create_task()` wired em `touring_decompose` handler

### Problema
`create_task()` tinha `#[allow(dead_code)]` — a convenience wrapper que delega para
`create_task_with_cila` com nível CILA padrão 3. O único site de produção
(`server/mod.rs:1902`) chamava `create_task_with_cila` diretamente com `cila_level = p.cila_level.unwrap_or(3)`,
bypassando o wrapper mesmo quando nenhum nível explícito era fornecido.

### Mudança

**server/mod.rs** — `"create"` arm do `touring_decompose` handler:

```rust
// Antes:
let cila_level = p.cila_level.unwrap_or(3);
let task_id = dec.create_task_with_cila(&task_type, &description, cila_level);

// Depois (EC50):
let cila_level = p.cila_level;
// EC50: First production caller of create_task() — routes to the convenience
// wrapper when no explicit cila_level is provided (default L3 semantics).
let task_id = match cila_level {
    Some(level) => dec.create_task_with_cila(&task_type, &description, level),
    None => dec.create_task(&task_type, &description),
};
```

**decomposer.rs** — removido `#[allow(dead_code)]` de `create_task()`.

### Design decisions
- `match p.cila_level { Some(level) => ..., None => ... }` — semântica explícita:
  quando nenhum nível é fornecido, rota pelo wrapper natural (L3 default)
- Equivalência semântica total: `create_task()` delega para `create_task_with_cila(_, _, 3)`
- `#[allow(dead_code)]` removido — método tem agora 1 caller real em produção
- Zero mudança de comportamento: todos os testes passam sem alteração

### Impacto
`create_task()` tem agora **1 caller real em produção** (era 0 — apenas testes).
O path de criação de task sem nível CILA explícito agora passa pelo wrapper
semântico correto, em vez de duplicar a lógica `unwrap_or(3)` inline.

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
cargo test -p touring-server  → 330 passed, 0 failed, 0 ignored
```
