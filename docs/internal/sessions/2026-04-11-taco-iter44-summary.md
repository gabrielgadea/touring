# TACO Iter44 — EC60: 3 `#[allow(dead_code)]` clusters em 3 crates

**Data**: 2026-04-11
**Iteração**: 44
**EC implementado**: EC60
**Arquivos modificados**:
- `crates/touring-antt/src/cross_validator.rs`
- `crates/touring-hooks/src/shared/metadata_dedup.rs`
- `crates/touring-hooks/src/shared/async_runtime.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 1451 tests (touring-hooks, +1 novo) + 88 tests (touring-antt), sem regressão

---

## EC60 — 3 mudanças em 3 arquivos

### 1. `cross_validator.rs` — 2 anotações analisadas

#### `has_contradiction_in_scc` (linha 311) — remoção vestigial

A função `fn has_contradiction_in_scc` era privada e tinha `#[allow(dead_code)]`. Análise
granular revelou que ela É chamada na mesma struct por `find_contradiction_cycles`:

```rust
pub fn find_contradiction_cycles(&self) -> Vec<Vec<String>> {
    let sccs = kosaraju_scc(&self.graph);
    sccs.into_iter()
        .filter(|scc| scc.len() > 1)
        .filter(|scc| self.has_contradiction_in_scc(scc))  // ← caller linha 302
        ...
}
```

Remoção pura da anotação — zero risco.

#### `get_index` (linha 288) — annotation legítima, doc adicionado

`pub fn get_index` é chamada APENAS em `#[cfg(test)]` (linha 899). Como `cargo check` NÃO
compila módulos `#[cfg(test)]`, o compilador não enxerga o caller e emite aviso. A anotação é
**necessária** (padrão EC58). Adicionado comentário explicativo:

```rust
// EC60: test-only helper — cargo check does not compile #[cfg(test)] modules,
// so the test caller at line 899 is invisible to the dead_code lint. Annotation
// is intentional: the method is kept for unit test introspection.
#[allow(dead_code)]
pub fn get_index(&self, id: &str) -> Option<NodeIndex> { ... }
```

### 2. `metadata_dedup.rs` — `invalidate` wired via test (padrão EC58)

`MetadataDedup::invalidate` tinha `#[allow(dead_code)]` com 0 callers. Como cargo check
não compila testes, adicionar um test é a forma correta de wiring sem remover a annotation.

**Doc atualizado**:
```rust
/// EC60: test-only helper — cargo check does not compile #[cfg(test)] modules,
/// so the caller in `dedup_cache_invalidate` is invisible to the dead_code lint.
/// Annotation is intentional: method is kept for unit test introspection and
/// future production use (e.g., force-refresh after explicit file overwrite).
#[allow(dead_code)]
pub fn invalidate(&self, key: &DedupKey) { ... }
```

**Novo teste adicionado** (`dedup_cache_invalidate`):
```rust
#[test]
fn dedup_cache_invalidate() {
    // EC60: exercises MetadataDedup::invalidate round-trip.
    // mark → is_duplicate(true) → invalidate → is_duplicate(false)
    let dedup = MetadataDedup::with_max_capacity(100, 60);
    let key = DedupKey { file_path: "src/lib.rs".to_string(), content_hash: "1714000000".to_string() };

    assert!(!dedup.check_and_mark(key.clone()));  // first: not dup
    assert!(dedup.is_duplicate(&key));             // now it's cached
    dedup.invalidate(&key);                        // clear the entry
    assert!(!dedup.is_duplicate(&key));            // fresh again
}
```

Resultado: 1450 → **1451 tests** em touring-hooks.

### 3. `async_runtime.rs` — `AsyncTaskBuilder::spawn` removido

`AsyncTaskBuilder::spawn` tinha `#[allow(dead_code)]` e 0 callers. A implementação era enganosa:

```rust
// ANTES (removido):
pub fn spawn<F>(self, f: F) -> std::pin::Pin<Box<dyn std::future::Future<Output = F::Output> + Send>>
{
    TokioRuntime::record_spawn();
    // Note: actual spawn deferred to runtime that has tokio context
    Box::pin(f)  // ← NÃO spawn real — apenas envolve em Box::pin
}
```

O método chamava `TokioRuntime::record_spawn()` e retornava `Box::pin(f)` — sem tokio context,
sem spawn real. Era uma abstração incompleta que não spawava nada. Todos os spawns reais do
codebase usam `handle.spawn(...)` diretamente. Remoção elimina a confusão.

---

## Padrões consolidados neste EC

| Cenário | Ação |
|---------|------|
| Annotation em fn com caller no mesmo impl | REMOVER (vestigial) |
| Annotation em pub fn com caller apenas em `#[cfg(test)]` | MANTER + doc "test-only" |
| Método sem callers + implementação enganosa | REMOVER método |
| Método sem callers + semântica válida | WIRING via test + doc (padrão EC58) |

---

## Validação

```
cargo check --workspace       → Finished (0 errors, 0 warnings)
cargo test -p touring-hooks   → 1451 passed (+1), 0 failed, 1 ignored
cargo test -p touring-antt    → 88 passed, 0 failed, 0 ignored
```
