# TACO Iter46 — EC62: pipeline.rs FilterCache + plugin.rs wasm_bytes + rlm_integration.rs fix

**Data**: 2026-04-11
**Iteração**: 46
**EC implementado**: EC62
**Arquivos modificados**:
- `crates/touring-cortex/src/pipeline.rs`
- `crates/touring-hooks/src/plugin.rs`
- `crates/touring-antt/src/rlm_integration.rs` (correção EC59 retroativa)
**Resultado**: 0 erros cargo check, 0 warnings, 1451 tests (touring-hooks) + 88 tests (touring-antt), sem regressão

---

## EC62 — 3 clusters em 3 arquivos

### 1. `pipeline.rs` — `FilterCache`: 3 anotações analisadas

| Método | Status antes | Ação EC62 |
|--------|-------------|-----------|
| `clear()` (linha 158) | `#[allow(dead_code)]` | REMOVER — chamado nas linhas 377 e 433 de `execute()` (E5-S6) |
| `is_empty()` (linha 171) | `#[allow(dead_code)]` | MANTER + doc "EC62: test-only" — caller apenas em linha 1145 (test) |
| `touch()` (linha 181) | `#[allow(dead_code)]` | WIRE em `get_or_compute` — gap E5-S7 preenchido |

#### `clear()` — anotação vestigial

`FilterCache::clear()` era chamado em 2 pontos da produção:
- Linha 377: após handler sync com `needs_cache_invalidation`
- Linha 433: após handler async com `needs_cache_invalidation`

A anotação era completamente vestigial. Removida + doc adicionado.

#### `touch()` — gap E5-S7 preenchido

`get_or_compute` usava `cache.peek(&key)` (read-only) que NÃO atualiza a posição LRU.
O `touch()` foi projetado exatamente para isso (E5-S7 doc), mas nunca foi chamado.

**Antes** (gap):
```rust
if let Some(indices) = cache.peek(&key) {
    return indices.clone(); // LRU não atualizado — entrada pode ser evicted
}
```

**Depois** (EC62):
```rust
if let Some(indices) = cache.peek(&key) {
    let result = indices.clone();
    drop(cache); // liberar read lock antes de touch() adquirir write lock
    self.touch(&key); // EC62: E5-S7 — atualizar posição LRU após peek hit
    return result;
}
```

### 2. `plugin.rs` — `LoadedPlugin::wasm_bytes`

`wasm_bytes: Vec<u8>` era carregado em `load_plugin()` mas nunca lido após atribuição.
Wire via `execute_hook` — surfaceia o tamanho nos resultados skeleton:

```rust
// EC62: wire wasm_bytes — surface WASM size in skeleton output for diagnostics.
output: format!(
    "[skeleton] {} hook executed (wasm_size={})",
    hook,
    plugin.wasm_bytes.len()
),
```

### 3. `rlm_integration.rs` — Correção retroativa de EC59

**Problema detectado**: EC59 removeu 6 anotações afirmando que tinham "callers em produção"
dentro de `process_document`. Mas `process_document` é `pub(crate)` com callers **apenas
em `#[cfg(test)]`** — o compilador não compila tests em `cargo check`, então ele vê
`process_document` como dead code, e tudo que ela referencia também é dead.

O `cargo check --workspace` de EC59 reportou 0 warnings provavelmente por cache incremental —
os 4 warnings aparecem ao compilar `touring-antt` do zero.

**Itens restaurados com anotação EC62-style:**

| Item | Tipo | Razão |
|------|------|-------|
| `CHUNK_PREFIX` | `const` | usado apenas em `process_document` (test-only) |
| `PatternFrequencyTracker::record` | `fn` privado | chamado apenas de `process_document` |
| `NlpPipeline::chunker_config` | campo privado | lido apenas via `config_hash()` → `process_document` |
| `NlpPipeline::config_hash()` | `fn` privado | chamado apenas de `process_document` |
| `NlpPipeline::process_document()` | `pub(crate)` | callers apenas em `#[cfg(test)]` |
| `NlpPipeline::clear_all()` | `pub(crate)` | sem callers em produção |

**Lição aprendida**: Quando `cargo check` após remoção de anotações reporta 0 warnings,
verificar se houve recompilação real (não incremental). Usar `cargo clean -p <crate>` antes
de afirmar que 0 warnings é o estado correto.

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-hooks → 1451 passed, 0 failed, 1 ignored
cargo test -p touring-antt  → 88 passed, 0 failed, 0 ignored
```
