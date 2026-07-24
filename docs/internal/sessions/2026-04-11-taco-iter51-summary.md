# TACO Iter51 — EC67: evaluate_async wired via spawn_blocking

**Data**: 2026-04-11
**Iteração**: 51
**EC implementado**: EC67
**Arquivos modificados**:
- `crates/touring-cortex/src/handlers/wasm.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 720 tests (touring-cortex), sem regressão

---

## EC67 — `evaluate_async` wired em `TypedEvaluateHandler`

### Contexto

`evaluate_async()` existia como stub com `#[allow(dead_code)]` — tinha um comentário
dizendo "In touring-server context, this would use spawn_blocking" mas nunca implementou
isso. O corpo chamava `self.evaluate(input)` (sync) diretamente, ignorando a pool.

### Wire implementado

```rust
async fn evaluate_async(
    &self,
    input: &TypedEvaluateInput,
) -> Option<Result<TypedPluginResult, String>> {
    // Pool must be configured — confirms async context with backpressure.
    self.async_pool.as_ref()?;

    // EC67: Dispatch typed WASM evaluation off the async runtime thread.
    // Arc clones are cheap — WasmModule is shared-ownership across all
    // concurrent evaluations; no data is copied.
    let registry = Arc::clone(&self.registry);
    let input_owned = input.clone();

    Some(
        tokio::task::spawn_blocking(move || {
            let plugin_name = input_owned.plugin_name.as_deref().unwrap_or("default");
            let module = match registry.get(plugin_name) {
                Some(m) => m,
                None => return Err(format!("plugin '{plugin_name}' not found")),
            };
            let ctx = TypedPluginContext::new(&input_owned.input)
                .with_params(input_owned.params.clone())
                .with_max_fuel(input_owned.max_fuel.unwrap_or(MAX_FUEL));
            module.call_evaluate_typed(&ctx)
        })
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))
        .and_then(|r| r),
    )
}
```

### Por que este design

| Decisão | Razão |
|---------|-------|
| `spawn_blocking` em vez de `pool.evaluate()` | `AsyncInferletPool::evaluate()` usa `PluginContext` não-tipado; o wire mantém `TypedPluginContext` (interface scored 0-100) |
| `Arc::clone(&self.registry)` | `PluginRegistry` é `Arc`-shared — clone barato, zero cópia de dados |
| `self.async_pool.as_ref()?` | Pool como gate: `None` → sync path, `Some` → async dispatch |
| Anotação `#[allow(dead_code)]` removida | Método agora está wired e é chamável de contextos tokio |

### Impacto

`TypedEvaluateHandler::evaluate_async()` agora é uma implementação real: dispatch
CPU-bound WASM evaluation para thread blocking do tokio, evitando stall do runtime.
touring-server pode invocar este método em handlers async sem bloquear o event loop.

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-cortex --lib → 720 passed, 0 failed
```
