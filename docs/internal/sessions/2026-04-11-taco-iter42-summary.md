# TACO Iter42 — EC58: VisitedTracker trim + quick_quality_context allow(dead_code) removido

**Data**: 2026-04-11
**Iteração**: 42
**EC implementado**: EC58
**Arquivos modificados**: `crates/touring-cognitive/src/got.rs`, `crates/touring-hooks/src/cli_handlers_scout.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 1450 tests (touring-hooks) + 400 tests (touring-cognitive), sem regressão

---

## EC58 — `VisitedTracker` impl + `quick_quality_context`

### Problema 1: `VisitedTracker` impl com `#[allow(dead_code)]` cobrindo o impl inteiro

`got.rs` tinha `#[allow(dead_code)]` no nível do `impl VisitedTracker` — suprimia warnings para
todo o bloco. Após análise granular:

**Métodos com callers em produção** (`explore_with_pheromone_bias`, `explore_subtree`):
- `new()` — linha 665: `VisitedTracker::new()`
- `next_generation()` — linha 682: `t.next_generation()`
- `current_generation()` — linha 683: `t.current_generation()`
- `visit_in_generation()` — linhas 689 e 803: `t.visit_in_generation(...)`

**Métodos com callers apenas em testes** (linhas 1054-1081):
- `visit()` — testes de ciclo simples (sem gen)
- `is_visited_in_gen()` — assertiva de estado geracional em testes
- `is_visited()` — convenience wrapper, chamado por test_is_visited_in_gen

### Mudanças em got.rs

1. Removido `#[allow(dead_code)]` do `impl VisitedTracker` (impl-level supressão eliminada)
2. Adicionado `#[allow(dead_code)]` granular nos 3 métodos test-only:
```rust
// EC58: test-only helper — no production caller; kept for unit test introspection.
#[allow(dead_code)]
fn visit(&mut self, node_id: NodeId) -> bool { ... }

// EC58: test-only helper — used for generational assertions in unit tests.
#[allow(dead_code)]
fn is_visited_in_gen(&self, node_id: NodeId, gen: u64) -> bool { ... }

// EC58: test-only helper — convenience wrapper around is_visited_in_gen.
#[allow(dead_code)]
fn is_visited(&self, node_id: NodeId) -> bool { ... }
```

### Design decision
- **Granular > blanket**: 4 métodos têm callers reais em produção, 3 são test-only.
  O blanket `#[allow(dead_code)]` escondia essa distinção. Agora é explícito.
- **Não removidos**: `visit`, `is_visited_in_gen`, `is_visited` têm callers em testes —
  removê-los quebraria as asserções de ciclo/geração nos unit tests de GoT.

### Problema 2: `quick_quality_context` com `#[allow(dead_code)]` stale

`cli_handlers_scout.rs:175` tinha `#[allow(dead_code)]` na função `quick_quality_context`.
Análise confirmou 2 callers em produção dentro do mesmo arquivo:
- Linha 274: path de falha de cache DB
- Linha 321: path principal de scout

A anotação era vestigial — foi adicionada quando a função foi criada e nunca removida
após ser wired.

### Mudanças em cli_handlers_scout.rs

Removido `#[allow(dead_code)]` de `quick_quality_context`.

---

## Validação

```
cargo check --workspace       → Finished (0 errors, 0 warnings)
cargo test -p touring-hooks   → 1450 passed, 0 failed, 1 ignored
cargo test -p touring-cognitive → 400 passed, 0 failed, 0 ignored
```
