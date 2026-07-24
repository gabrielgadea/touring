# TACO Iter52 — EC68: test_generation + wasm registrados no pipeline

**Data**: 2026-04-11
**Iteração**: 52
**EC implementado**: EC68
**Arquivos modificados**:
- `crates/touring-cortex/src/handlers/mod.rs` (pub mod + register_all + BUILTIN_HANDLER_COUNT)
- `crates/touring-cortex/src/handlers/wasm.rs` (H83→H105, conflito resolvido)
**Resultado**: 0 erros cargo check, 0 warnings, 740 tests (touring-cortex, era 720), sem regressão

---

## EC68 — 2 handlers fantasmas registrados no pipeline

### Problema detectado

`test_generation.rs` e `wasm.rs` existiam como **arquivos físicos** mas NÃO estavam
declarados em nenhum `mod` statement. Eram literalmente fantasmas — o compilador Rust
nunca os incluía na compilação da crate. O resultado: 20+ testes e 2 handlers totalmente
inacessíveis.

Diagnóstico: `mod.rs` lista 13 módulos; há 30 arquivos `.rs` em `handlers/` — 16 são
não-declarados. EC68 resolve os 2 mais relevantes (os trabalhados nesta sessão).

### Fix em mod.rs

```rust
// Adicionado ao final das declarações de módulos:
pub mod test_generation;
pub mod wasm;

// Adicionado ao final de register_all():
test_generation::register(pipeline);
wasm::register(pipeline);
```

### Fix de conflito H83

`wasm.rs` chamava seu handler de `"H83_TypedEvaluate"` — conflito com `integration.rs`
que já usa `"H83_integration_completeness"`. Renomeado para `"H105_TypedEvaluate"`
(próximo número disponível após H104).

### BUILTIN_HANDLER_COUNT

```rust
// Antes: 84
// Depois: 86
/// v11.2: 86 handlers (+1 test_generation: H102 TestGenerationHandler;
///         +1 wasm: H105 TypedEvaluateHandler)
pub const BUILTIN_HANDLER_COUNT: usize = 86;
```

### Impacto

| Antes | Depois |
|-------|--------|
| `TestGenerationHandler` (H102) não compilado | Compilado e registrado no pipeline |
| `TypedEvaluateHandler` (H83 → H105) não compilado | Compilado e registrado no pipeline |
| 720 testes cortex | 740 testes cortex (+20 dos novos módulos) |
| `evaluate_async()` inacessível | Disponível para callers tokio |

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-cortex --lib → 740 passed, 0 failed (era 720)
```
