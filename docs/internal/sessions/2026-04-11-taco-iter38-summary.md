# TACO Iter38 — EC54: StringExt polyfill removido + extract_context usa stable Rust 1.77 API

**Data**: 2026-04-11
**Iteração**: 38
**EC implementado**: EC54
**Arquivos modificados**: `crates/touring-antt/src/monetary_parser.rs`, `crates/touring-antt/src/lib.rs`
**Resultado**: 0 erros cargo check, 1452 tests passing (touring-hooks), 88 tests passing (touring-antt), sem regressão

---

## EC54 — Remoção do polyfill `StringExt` + simplificação de `extract_context`

### Problema
`StringExt` (trait `pub(crate)`) tinha `#[allow(dead_code)]` e 0 callers em produção.
Implementava `floor_char_boundary` e `ceil_char_boundary` como polyfill para
`str::floor_char_boundary` / `str::ceil_char_boundary` — métodos que foram
**estabilizados no Rust 1.77.0** (março 2024).

`extract_context` em `monetary_parser.rs` usava 12 linhas de byte-walking manual
em vez do trait (que era o propósito do trait):

```rust
// Antes: byte-walking manual (12 linhas)
let safe_start = {
    let mut i = ctx_start;
    while i > 0 && !text.is_char_boundary(i) { i -= 1; }
    i
};
let safe_end = {
    let mut i = ctx_end;
    while i < text.len() && !text.is_char_boundary(i) { i += 1; }
    i
};
```

### Mudança

**monetary_parser.rs**:
1. Bloco de byte-walking em `extract_context` substituído pelos métodos estáveis:
```rust
// EC54: Use stable str::floor_char_boundary / ceil_char_boundary (Rust 1.77+).
let safe_start = text.floor_char_boundary(ctx_start);
let safe_end = text.ceil_char_boundary(ctx_end);
```

2. `StringExt` trait e `impl StringExt for str` removidos (totalizando ~40 linhas deletadas):
```rust
// Apenas comentário de tombstone ficou:
// NOTE: str::floor_char_boundary and str::ceil_char_boundary are stable since
// Rust 1.77 — the StringExt polyfill was removed in EC54 as redundant.
```

**lib.rs**: Re-export temporário `pub(crate) use monetary_parser::StringExt` removido.

### Design decisions
- **POTENCIALIZAR via remoção**: `#[allow(dead_code)]` seria a opção fácil;
  remover o polyfill é a ação correta — elimina código morto sem deixar rastro
- **Backward compat**: `rust-version = "1.75"` é o MSRV declarado, mas Rust 1.77+
  está instalado. Os testes `test_string_ext_floor`/`test_string_ext_ceil` continuam
  passando usando os métodos estáveis da stdlib diretamente
- **Tombstone comment**: O comentário que explica a remoção evita que o polyfill
  seja re-introduzido no futuro por engano

### Impacto
- `StringExt` polyfill removido — zero código morto restante em `monetary_parser.rs`
- `extract_context` reduzida de ~20 para ~8 linhas (12 linhas de byte-walking deletadas)
- Clareza aumentada: `floor_char_boundary` é um nome de stdlib reconhecível,
  não um método de trait obscuro

---

## Validação

```
cargo check --workspace       → Finished (0 errors)
cargo test -p touring-hooks   → 1452 passed, 0 failed, 1 ignored
cargo test -p touring-antt    → 88 passed, 0 failed, 0 ignored
```
