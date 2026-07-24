# TACO Iter50 — EC66: 5 anotações vestigiais em 3 arquivos

**Data**: 2026-04-11
**Iteração**: 50
**EC implementado**: EC66
**Arquivos modificados**:
- `crates/touring-cortex/src/handlers/test_generation.rs` (3 anotações)
- `crates/touring-simd/src/cortex.rs` (1 anotação)
- `crates/touring-core/src/embedding/client.rs` (1 anotação)
**Resultado**: 0 erros cargo check, 0 warnings, 720 tests (touring-cortex) + 222 tests (touring-simd) + 101 tests (touring-core), sem regressão

---

## EC66 — 5 anotações vestigiais deixadas por EC63/EC64

**Padrão detectado**: EC63 e EC64 wirearam corretamente os itens mortos, mas esqueceram de
remover as `#[allow(dead_code)]` annotations dos itens após o wiring. EC66 fecha esse gap.

### 1. `test_generation.rs:38,42,46` — MIN_BUDGET, MIN_CONFIDENCE, MCTS_ROLLOUTS

Constantes wireadas pelo EC63:
- `MIN_BUDGET` usado na linha 471: `if ctx.context_budget_remaining < MIN_BUDGET`
- `MIN_CONFIDENCE` usado na linha 491: `if confidence < MIN_CONFIDENCE`
- `MCTS_ROLLOUTS` usado nas linhas 154 e 488: `cases.truncate(MCTS_ROLLOUTS)` e `/ MCTS_ROLLOUTS as f64`

As 3 anotações eram vestigiais — EC66 as remove.

### 2. `cortex.rs:73` — `conflict_threshold` (MetacognitivePipeline)

Campo wireado pelo EC64 na linha 132:
```rust
// EC64: Apply pipeline-specific conflict_threshold to recompute conflict_detected.
quality.conflict_detected = quality.coefficient_of_variation > self.conflict_threshold;
```

Anotação vestigial — EC66 a remove.

### 3. `client.rs:159` — `model` (EmbedResponse)

Campo wireado pelo EC64 na linha 273:
```rust
if let Some(ref model_name) = embed_resp.model {
    tracing::debug!(model = %model_name, "GPU embedder: model reported by service");
}
```

Anotação vestigial — EC66 a remove.

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-cortex --lib → 720 passed, 0 failed
cargo test -p touring-simd --lib   → 222 passed, 0 failed
cargo test -p touring-core --lib   → 101 passed, 0 failed
```
