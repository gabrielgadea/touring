# TACO Iter40 — EC56: self_reflection.rs wired + ResultExt/OptionExt trimmados

**Data**: 2026-04-11
**Iteração**: 40
**EC implementado**: EC56
**Arquivos modificados**: `crates/touring-cortex/src/handlers/self_reflection.rs`, `crates/touring-hooks/src/shared/result_ext.rs`, `crates/touring-hooks/src/integration_tests.rs`
**Resultado**: 0 erros cargo check, 1450 tests passing (touring-hooks), sem regressão

---

## EC56 — `compose_suggestion` wired + trim de `ResultExt`/`OptionExt`

### Problema 1: `compose_suggestion` tinha `#[allow(dead_code)]`

`compose_suggestion` em `self_reflection.rs` estava com `#[allow(dead_code)]` e 0 callers.
O método formata um relatório de reflexão por símbolo com dados de `result.dimensions`.

### Mudanças em self_reflection.rs

1. Removido `#[allow(dead_code)]` de `ReflectionDim`, `ReflectionResult`, e `compose_suggestion`
2. Removido campo `name: &'static str` de `ReflectionDim` (não usado em nenhum caller)
3. `compose_suggestion` atualizado para usar `result.dimensions` (first caller do campo):
```rust
fn compose_suggestion(&self, result: &ReflectionResult, symbol: &str) -> String {
    let conf_pct = (result.composite * 100.0).round() as i32;
    let corr_count = result.corrections.len();
    let dims: Vec<String> = result.dimensions
        .iter()
        .map(|(k, v)| format!("{}={:.0}%", k, v * 100.0))
        .collect();
    let status = if result.passes { "pass" } else { "needs_work" };
    format!(
        "reflection[{}]: {} (conf={}%, corrections={}, dims=[{}])",
        status, symbol, conf_pct, corr_count, dims.join(", "),
    )
}
```
4. `handle_post` agora chama `compose_suggestion` para debug logging:
```rust
tracing::debug!(
    target: "touring_cortex",
    suggestion = %self.compose_suggestion(&result, symbol),
    "H101: self-reflection complete"
);
```

### Problema 2: `ResultExt`/`OptionExt` tinham métodos sem callers

Após wiring de `unwrap_or_debug` (EC55), os métodos `unwrap_or_log`, `unwrap_or_warn`,
`log_err`, `context` (ResultExt) e `unwrap_or_log`, `unwrap_or_warn` (OptionExt)
continuaram com 0 callers em produção.

### Mudanças em result_ext.rs

- Removidos todos os métodos com 0 callers — apenas `unwrap_or_debug` permanece em ambos os traits
- Removidos imports desnecessários (`warn`, `error`) de `tracing`
- Testes internos atualizados para usar `unwrap_or_debug`
- Resultado: traits minimalistas com exatamente 1 método cada (o que é usado em produção)

### Mudanças em integration_tests.rs

- `test_result_option_ext_e2e` atualizado para usar `unwrap_or_debug` em vez de `unwrap_or_log`
- Bloco `context()` removido (método não existe mais)
- Testes equivalentes preservados para `ResultExt::unwrap_or_debug` e `OptionExt::unwrap_or_debug`

### Design decisions
- **Trim aggressivo**: Manter apenas o que é usado evita que a interface cresça com métodos fantasma
- **Baseline de testes**: 1452 → 1450 é resultado correto (2 testes de métodos removidos = eliminados junto)
- **POTENCIALIZAR via remoção**: Código morto em trait é mais perigoso que código morto em função — cria superfície de API falsa

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1450 passed, 0 failed, 1 ignored
```
