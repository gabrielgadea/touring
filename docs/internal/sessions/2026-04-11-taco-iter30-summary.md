# TACO Iter30 — EC46: count_import_cycles wired via WiringReport.cycle_count

**Data**: 2026-04-11
**Iteração**: 30
**EC implementado**: EC46
**Arquivo modificado**: `crates/touring-analysis/src/wiring/mod.rs`
**Resultado**: 0 erros cargo check, 198 tests passing (touring-analysis), sem regressão

---

## EC46 — `count_import_cycles()` wired em `analyze_wiring()` via `WiringReport.cycle_count`

### Problema
`cycle_detection::count_import_cycles()` existia com **0 callers em produção** fora dos seus
próprios testes unitários. A função usa o algoritmo de Kosaraju SCC para detectar ciclos de
importação circular no grafo de dependências extraído de `wiring_map`.

`analyze_wiring()` chamava `count_orphans` e `analyze_chains` mas **não chamava**
`count_import_cycles`. O `WiringReport` retornado por essa função — consumido por todos os
comandos de wiring health — não expunha informação sobre dependências circulares.

### Mudança

**wiring/mod.rs** — 3 incrementos atômicos:

1. **Novo campo `cycle_count: usize` em `WiringReport`**:
```rust
/// Circular import cycles detected (Kosaraju SCC, nodes > 1).
///
/// Zero means no circular dependencies in the tracked import graph.
/// A non-zero value flags architectural issues that should be resolved.
#[serde(default)]
pub cycle_count: usize,
```

2. **Chamada a `count_import_cycles()` em `analyze_wiring()`**:
```rust
// EC46: First production caller of count_import_cycles() — surfaces circular
// import chains (Kosaraju SCC) in every tool that reads WiringReport.
// Non-blocking: returns 0 gracefully when wiring_map has no cycles.
let cycle_count = cycle_detection::count_import_cycles(conn);
```

3. **Campo populado no retorno**:
```rust
WiringReport {
    ...
    cycle_count,
    score,
}
```

**Design decisions**:
- `#[serde(default)]` — retrocompatível com leitores que não conhecem o campo (JSON antigo)
- `cycle_detection::count_import_cycles(conn)` — usa path qualificado (sem `use`) para clareza
- Posição no struct: antes de `score` (campo diagnóstico, não faz parte da fórmula de score)
- Score formula inalterada — `cycle_count` é métrica diagnóstica, não penalizante no score v2

### Testes adicionados

```rust
#[test]
fn test_cycle_count_zero_no_cycles()  // linear chain a→b: cycle_count=0
#[test]
fn test_cycle_count_detects_mutual_import()  // a↔b mutual: cycle_count=1
```

### Impacto
`count_import_cycles()` tem agora **1 caller real em produção** (era 0).
Todos os consumidores de `WiringReport` — `touring wiring status`, `touring e2e`,
`touring_project` MCP tool, `analyze_wiring_incremental` — passam a expor o campo
`cycle_count` automaticamente via JSON serialization.

O LLM pode agora detectar ciclos de dependência via qualquer ferramenta de wiring health,
sem precisar chamar `detect_import_cycles()` separadamente.

---

## Estado de cycle_detection após EC46

| Função | Callers | EC |
|--------|---------|-----|
| `detect_import_cycles` | 1 (count_import_cycles) | pré-existente |
| `count_import_cycles` | 1 (analyze_wiring) | **EC46** |

**Todos os símbolos públicos de `cycle_detection` têm agora pelo menos 1 caller em produção.**

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test --workspace        → touring-analysis: 198 passed, 0 failed
  test wiring::tests::test_cycle_count_zero_no_cycles ... ok
  test wiring::tests::test_cycle_count_detects_mutual_import ... ok
```
