# TACO Iter48 — EC64: 5 itens em 5 arquivos

**Data**: 2026-04-11
**Iteração**: 48
**EC implementado**: EC64
**Arquivos modificados**:
- `crates/touring-simd/src/gpu/mod.rs`
- `crates/touring-cortex/src/types.rs`
- `crates/touring-core/src/embedding/client.rs`
- `crates/touring-simd/src/cortex.rs`
- `crates/touring-learning/src/memory/async_rlm.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 222 tests (touring-simd) + 791 tests (touring-learning), sem regressão

---

## EC64 — 5 clusters em 5 arquivos

### 1. `gpu/mod.rs:237` — `HttpGpuBackend` (anotação vestigial)

`HttpGpuBackend` é usado em `touring-hooks/src/ann_memory/mod.rs` sob `#[cfg(feature = "gpu-compute")]`:
```rust
use touring_simd::gpu::HttpGpuBackend;
let backend = Arc::new(HttpGpuBackend::new(url, PATH_EMBED_DIM));
```
Anotação completamente vestigial. Removida.

### 2. `types.rs:95` — `HookEvent::as_str()` (test-only, annotation mantida)

`as_str()` só é chamada em `#[cfg(test)]` (linhas 490-537 do mesmo arquivo). `cargo check` não
compila tests, portanto a anotação é INTENCIONAL. Atualizado o comment para doc EC64-style:
```rust
// EC64: test-only helper — cargo check does not compile #[cfg(test)] modules,
// so the callers at types.rs:490-537 are invisible to the dead_code lint.
```

### 3. `client.rs:159` — `EmbedResponse::model` (wired em debug log)

O campo `model: Option<String>` era deserializado do JSON de resposta do GPU service mas
nunca lido. Wire via `tracing::debug!`:
```rust
// EC64: wire model field — log which model produced the embeddings for diagnostics.
if let Some(ref model_name) = embed_resp.model {
    tracing::debug!(model = %model_name, "GPU embedder: model reported by service");
}
```

### 4. `cortex.rs:73` — `MetacognitivePipeline::conflict_threshold` (gap E5-S7 preenchido)

O campo `conflict_threshold: f64 = 0.3` era setado em `new()` mas nunca lido. Doc dizia:
"Conflict threshold (coefficient of variation) for triggering Wilson weighting."

`reconcile_with_wilson` usa um threshold interno `CONFLICT_CV_THRESHOLD` hardcoded. O campo
`conflict_threshold` de `MetacognitivePipeline` foi projetado para permitir que callers AJUSTEM
essa sensibilidade, mas o gap E5-S7 nunca foi preenchido.

**Wire em `resolve()`**:
```rust
let (fused_value, _fused_conf, mut quality) =
    reconcile_with_wilson(&values, &confidences, self.wilson_confidence);

// EC64: Apply pipeline-specific conflict_threshold to recompute conflict_detected.
// This overrides the internal CONFLICT_CV_THRESHOLD from reconcile_with_wilson,
// giving callers control over conflict sensitivity.
quality.conflict_detected = quality.coefficient_of_variation > self.conflict_threshold;
```

### 5. `async_rlm.rs:64` — `WriteOp::Delete` (delete() method adicionado)

O variant `Delete { key, tier }` existia no enum `WriteOp` e era HANDLED em `apply_batch`:
```rust
WriteOp::Delete { key, tier } => {
    if let Err(e) = guard.delete(&key, tier) { ... }
}
```
Mas NUNCA era construído — sem callers. Wire: adicionado `delete()` público + `sync_delete()` helper privado, seguindo o padrão exato de `store_typed()`:
- Remove da cache in-memory via `try_write` + `HashMap::remove`
- Enfileira `WriteOp::Delete` no background channel
- Fallback síncrono via `guard.delete()` se channel fechado
- Removida anotação `#[allow(dead_code)]` do variant (agora tem caller em `delete()`)

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-simd  → 222 passed, 0 failed
cargo test -p touring-learning → 791 passed, 0 failed
```
