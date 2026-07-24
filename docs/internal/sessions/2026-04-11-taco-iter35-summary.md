# TACO Iter35 — EC51: find_contradiction_cycles + node_count wired em CrossValidator

**Data**: 2026-04-11
**Iteração**: 35
**EC implementado**: EC51
**Arquivos modificados**: `crates/touring-antt/src/cross_validator.rs`, `crates/touring-antt/Cargo.toml`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), 88 tests passing (touring-antt), sem regressão

---

## EC51 — `ConsistencyGraph::find_contradiction_cycles()` + `node_count()` wired em `CrossValidator::validate()`

### Problema
`find_contradiction_cycles()` e `node_count()` em `ConsistencyGraph` tinham 0 callers
em produção — ambos anotados com `#[allow(dead_code)]`.

`find_contradiction_cycles()` usa Kosaraju SCC para detectar grupos de asserções que
se contradizem mutuamente (A contradiz B, B contradiz C, C contradiz A — ciclo). Este
sinal é mais forte que contradições pairwise: indica inconsistências circulares em
documentos regulatórios da ANTT.

`CrossValidator::validate()` detectava contradições pairwise mas nunca expunha
os ciclos de contradição — lacuna no observability do pipeline.

### Mudança

**cross_validator.rs** — adicionado ao final de `CrossValidator::validate()`:

```rust
// EC51: First caller of find_contradiction_cycles() + node_count() —
// surfaces cyclic contradictions (stronger signal than pairwise contradictions).
// Kosaraju SCC detects groups where assertions mutually contradict each other.
let cycles = self.graph.find_contradiction_cycles();
if !cycles.is_empty() {
    tracing::debug!(
        target: "touring_antt",
        node_count = self.graph.node_count(),
        cycle_count = cycles.len(),
        "cross_validator: contradiction cycles in assertion graph"
    );
}
```

**Cargo.toml** — adicionado `tracing = { workspace = true }` (touring-antt não tinha tracing).

**cross_validator.rs** — removidos `#[allow(dead_code)]` de `node_count()` e `find_contradiction_cycles()`.

### Design decisions
- `target: "touring_antt"` — permite filtrar por target nos logs de produção
- `if !cycles.is_empty()` — evita overhead de structured log para o caso trivial (sem ciclos)
- `node_count` no log — contexto do grafo (quantas asserções foram analisadas)
- `cycle_count` no log — métrica principal de interesse para monitoramento
- `tracing` adicionado ao Cargo.toml do crate (ausente antes de EC51)

### Impacto
- `find_contradiction_cycles()` tem agora **1 caller real em produção** (era 0)
- `node_count()` tem agora **1 caller real em produção** (era 0)
- `touring-antt` passa a emitir sinais de contradição cíclica via `tracing::debug!`
  com `target: "touring_antt"` — consumível por qualquer log pipeline

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
cargo test -p touring-antt    → 88 passed, 0 failed, 0 ignored
```
