# TACO Iter49 — EC65: 6 itens em 6 arquivos + 3 fixos de anotação

**Data**: 2026-04-11
**Iteração**: 49
**EC implementado**: EC65
**Arquivos modificados**:
- `crates/touring-server/src/server/params.rs` (2 clusters)
- `crates/touring-learning/src/rl/burn_transformer.rs` (2 clusters)
- `crates/touring-learning/src/n1/pheromone_integration.rs`
- `crates/touring-learning/src/n1/basic_generator.rs`
- `crates/touring-simd/src/gpu/mod.rs` (fix warning)
- `crates/touring-learning/src/data/checkpoint.rs` (fix warning)
**Resultado**: 0 erros cargo check, 0 warnings, 791 tests (touring-learning) + 330 tests (touring-server), sem regressão

---

## EC65 — 6 clusters + 3 fixos de warning

### 1. `params.rs:827` — `MemoryClustersParams` (anotação vestigial)

Struct usada em `server/mod.rs` como `Parameters<MemoryClustersParams>` em dois handlers.
Anotação era completamente vestigial. Removida + campos feature-gated receberam anotações
targeted com doc EC65-style explicando `async-memory` feature gate.

### 2. `params.rs:487` — `CheckpointParams::operation` (backwards-compat serde)

Campo serde-only para compatibilidade com clients legados. Anotação é INTENCIONAL — 
`cargo check` não vê o campo sendo lido pelo serde. Doc atualizado para EC-style:
```
// EC65: backwards-compat serde field — legacy clients send `operation`; we must
// deserialize it to avoid parse errors, but the field is intentionally ignored.
```

### 3. `burn_transformer.rs:50` — `LEARNING_RATE` (anotação vestigial)

Constante usada na linha 158: `let model = optim.step(LEARNING_RATE, model, grads);`
O comment dizia "used by train_step which is pub API not yet wired to a caller" — o 
comentário se referia a `train_step`, não à constante. `LEARNING_RATE` já tem caller.
Anotação vestigial removida.

### 4. `burn_transformer.rs:131` — `train_step()` (re-export adicionado)

A função era `pub` dentro do módulo privado `inner`, sem re-export. O comment dizia
"callers must re-export train_step themselves". 

Wire: re-exportado no módulo externo:
```rust
// EC65: re-export train_step — pub API was trapped inside the private `inner` mod.
#[cfg(feature = "burn-transformer")]
pub use inner::train_step;
```

### 5. `pheromone_integration.rs:100` + `basic_generator.rs` — `best_tool_for()` wired

`best_tool_for()` consultava o pheromone bus pelo tool com maior força acumulada, mas
nenhum caller existia. Wire: usado em `select_tool_for_objective()` como fallback 
pheromone-guided ANTES do default `"Read"`:

```rust
// EC65: Consult pheromone bus as learned fallback before defaulting to "Read".
// best_tool_for() returns the tool with the highest accumulated pheromone strength,
// encoding past execution success across all tool calls for this agent.
if let Some((pheromone_tool, _strength)) = self.integrator.best_tool_for(&desc_lower) {
    return pheromone_tool;
}
```

**Impacto**: `BasicGenerator` agora usa aprendizado ACO como sinal de seleção, não apenas
pattern matching estático. Tool selecionado reflete experiência acumulada.

### 6. `basic_generator.rs:140` — `to_cli_name()` wired em `build_tool_call()`

Método que converte "touring_index_find" → "touring index find" nunca tinha caller.
Wire em `build_tool_call()`:
```rust
// EC65: wire to_cli_name — convert "touring_index_find" → "touring index find"
// for human-readable expected_output strings.
expected_output: Some(format!("{} result", self.to_cli_name(tool_name))),
```

---

## Fixos de warning introduzidos por EC65

| Warning | Causa | Fix |
|---------|-------|-----|
| `gpu_url`, `embedding_dim` never read | Campos em `HttpGpuBackend` — impl usa WGPU compute, não HTTP URL | Anotação targeted com doc sobre HTTP fallback path |
| `timestamp` never read | Campo em `CheckpointData::AcoEvolution` — populado por serde, ignorado em match | Anotação targeted com doc |
| `title`, `tasks` never read | Campos em `CheckpointData::Pln2Phase` — populado por serde, ignorado via `..` | Anotação targeted com doc |

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-learning --lib → 791 passed, 0 failed
cargo test -p touring-server --lib  → 330 passed, 0 failed
```
