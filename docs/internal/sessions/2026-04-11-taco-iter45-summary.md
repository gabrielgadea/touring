# TACO Iter45 — EC61: monetary_parser.rs — primary_set wired + 2 campos mortos removidos

**Data**: 2026-04-11
**Iteração**: 45
**EC implementado**: EC61
**Arquivo modificado**: `crates/touring-antt/src/monetary_parser.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 88 tests (touring-antt), sem regressão

---

## EC61 — `MonetaryPatterns`: 3 `#[allow(dead_code)]` em campos de struct privada

### Contexto

`MonetaryPatterns` é uma `struct` privada com 5 campos, 3 dos quais tinham `#[allow(dead_code)]`:

| Campo | Tipo | Status antes |
|-------|------|--------------|
| `primary_set` | `RegexSet` | `#[allow(dead_code)]` — 0 callers |
| `primary_patterns` | `Vec<Regex>` | Usado em loop at line 293 |
| `currency_pattern` | `Regex` | `#[allow(dead_code)]` — "reserved for future use" |
| `multiplier_pattern` | `Regex` | `#[allow(dead_code)]` — "reserved for future use" |
| `number_pattern` | `Regex` | Usado em parse_monetary |

### Análise granular

**`currency_pattern`** e **`multiplier_pattern`**: Construídos em `new()` mas nunca lidos.
As funções `detect_currency` e `detect_multiplier` já implementam a lógica via `contains()` —
mais eficiente do que regex para buscas de strings literais curtas. Wiring esses campos
tornaria o código mais lento sem ganho funcional. Decisão: **remover**.

**`primary_set`**: Construída via `RegexSet::new(&primary_strings)` mas nunca usada.
`RegexSet` tem a semântica ideal de fast pre-filter: verifica se QUALQUER dos N patterns
bate em O(n_chars) em vez de O(N × n_chars) como a iteração sobre `primary_patterns`.
Decisão: **wire como fast pre-filter em `parse_monetary`**.

### Mudanças

**`MonetaryPatterns` struct** — removidos 2 campos:
```rust
// ANTES: 5 campos (2 mortos)
struct MonetaryPatterns {
    #[allow(dead_code)] primary_set: RegexSet,
    primary_patterns: Vec<Regex>,
    #[allow(dead_code)] currency_pattern: Regex,   // ← removido
    #[allow(dead_code)] multiplier_pattern: Regex, // ← removido
    number_pattern: Regex,
}

// DEPOIS: 3 campos (0 mortos)
struct MonetaryPatterns {
    primary_set: RegexSet,      // ← annotation removida, agora tem caller
    primary_patterns: Vec<Regex>,
    number_pattern: Regex,
}
```

**`MonetaryPatterns::new()`** — removidas 2 construções de Regex:
```rust
// removido:
let currency_pattern = Regex::new(r"(?i)(R\$|US\$|€|EUR|USD|BRL)").map_err(...)?;
let multiplier_pattern = Regex::new(r"(?i)(bilh[õo]es?|milh[õo]es?|mil)").map_err(...)?;
// removidos de Ok(Self { ... })
```

**`parse_monetary()`** — fast pre-filter wired:
```rust
// EC61: Fast pre-filter via RegexSet — O(n_chars) single pass.
// If no primary pattern matches at all, skip the per-pattern loop entirely.
if !patterns.primary_set.is_match(text) {
    return Ok(vec![]);
}
```

### Impacto de performance

Para textos sem valores monetários (caso comum em logs, código, comentários):
- **Antes**: 6 iterações `Regex::find_iter()` cada uma varrendo o texto inteiro
- **Depois**: 1 `RegexSet::is_match()` → early return

Para textos COM valores monetários: nenhuma mudança de comportamento.

---

## Validação

```
cargo check --workspace     → Finished (0 errors, 0 warnings)
cargo test -p touring-antt  → 88 passed, 0 failed, 0 ignored
```
