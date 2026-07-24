# TACO Iter39 — EC55: ResultExt + OptionExt wired em nlp_bridge

**Data**: 2026-04-11
**Iteração**: 39
**EC implementado**: EC55
**Arquivos modificados**: `crates/touring-hooks/src/shared/result_ext.rs`, `crates/touring-hooks/src/nlp_bridge.rs`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), sem regressão

---

## EC55 — `ResultExt` + `OptionExt` wired em `nlp_bridge.rs`

### Problema
`ResultExt<T,E>` e `OptionExt<T>` em `shared/result_ext.rs` tinham `#[allow(dead_code)]`
e 0 callers em produção. Os traits fornecem contextual error handling via tracing:
- `ResultExt`: `unwrap_or_log`, `unwrap_or_warn`, `unwrap_or_debug`, `log_err`, `context`
- `OptionExt`: `unwrap_or_log`, `unwrap_or_warn`, `unwrap_or_debug`

Enquanto isso, `nlp_bridge.rs::extract_monetary_values` silenciava erros via
`.unwrap_or_default()` — perdendo contexto diagnóstico em falhas de parsing.

### Mudanças

**shared/result_ext.rs**: Removidos `#[allow(dead_code)]` de ambos os traits.

**nlp_bridge.rs** — 2 integrações:

1. `extract_monetary_values` usa `ResultExt::unwrap_or_debug` (EC55 first caller):
```rust
pub fn extract_monetary_values(text: &str) -> Vec<MonetaryValue> {
    // EC55: First production caller of ResultExt::unwrap_or_debug —
    // logs parse failures at DEBUG level instead of silently swallowing them.
    use crate::shared::result_ext::ResultExt;
    parse_monetary(text).unwrap_or_debug(Vec::new(), "nlp_bridge: monetary parse failed")
}
```

2. Nova função `dominant_category_or_debug` usa `OptionExt::unwrap_or_debug` (EC55 first caller):
```rust
pub fn dominant_category_or_debug(text: &str, fallback: &str) -> String {
    use crate::shared::result_ext::OptionExt;
    dominant_category(text)
        .unwrap_or_debug(fallback.to_string(), "nlp_bridge: no dominant keyword category found")
}
```

### Design decisions
- **DEBUG level**: Falhas de parsing monetário são esperadas em texto não-financeiro
  (não devem gerar ERROR/WARN no log de produção)
- **`use` local**: Import dentro da função para evitar poluição do scope do módulo
  e deixar explícito em qual site o trait é usado
- `dominant_category_or_debug` expõe um padrão utilitário genuíno para callers
  que precisam de um fallback string com observability

### Impacto
- `ResultExt::unwrap_or_debug` tem agora **1 caller real em produção** (era 0)
- `OptionExt::unwrap_or_debug` tem agora **1 caller real em produção** (era 0)
- `extract_monetary_values` não mais silencia erros de parsing — emite DEBUG log
- `dominant_category_or_debug` adiciona observability ao pipeline NLP

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
```
