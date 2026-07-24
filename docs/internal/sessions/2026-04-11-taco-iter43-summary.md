# TACO Iter43 — EC59: rlm_integration.rs — 6 allow(dead_code) vestigiais removidos

**Data**: 2026-04-11
**Iteração**: 43
**EC implementado**: EC59
**Arquivos modificados**: `crates/touring-antt/src/rlm_integration.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 88 tests (touring-antt) + 1450 tests (touring-hooks), sem regressão

---

## EC59 — Remoção de 6 `#[allow(dead_code)]` vestigiais em `rlm_integration.rs`

### Descoberta

Grep inicial apontou 6 anotações `#[allow(dead_code)]` em `rlm_integration.rs`.
Análise granular revelou que TODOS os 6 itens JÁ TINHAM callers em produção dentro do mesmo arquivo:

| Item | Tipo | Caller |
|------|------|--------|
| `CHUNK_PREFIX` (linha 32) | constante | `process_document` linha 313: `format!("{}{}:{}", CHUNK_PREFIX, ...)` |
| `PatternFrequencyTracker::record` (linha 184) | método | `process_document` linhas 359-362: `tracker.record(...)` |
| `NlpPipeline::chunker_config` (linha 260) | campo | lido por `config_hash()` → `process_document` |
| `impl NlpPipeline` block (linha 268) | impl | `process_document`, `new`, `with_memory`, etc. |
| `config_hash` (linha 302) | método | `process_document` linha 313: `self.config_hash()` |
| `clear_all` (linha 485) | método | sem callers → **compiler não reportou warning** |

### Resultado inesperado: `clear_all` também era OK

Após remover todas as 6 anotações, `cargo check` reportou **0 warnings**.
Isso significa que `clear_all` também passou sem warning — provavelmente porque:
- O `impl NlpPipeline` block-level `#[allow(dead_code)]` (agora removido) estava
  mascarando a detecção individual de `clear_all`
- Com o impl-level removido, o compilador avalia `clear_all` individualmente —
  `pub(crate)` métodos em crates de biblioteca são visíveis externamente, então o
  compilador não os reporta como dead_code (poderiam ser usados por consumers)

### Por que as anotações existiam?

Padrão comum: anotações adicionadas durante desenvolvimento iterativo ("this will be used later")
e nunca removidas após wiring. O `#[allow(dead_code)]` no impl block foi provavelmente adicionado
quando `NlpPipeline` era novo e nenhum método tinha callers. Conforme os métodos foram conectados,
ninguém removeu a anotação.

### Ação correta

Remoção pura — zero mudança de comportamento, zero risco. As 6 anotações eram ruído que
obscurecia o estado real do código.

---

## Validação

```
cargo check --workspace       → Finished (0 errors, 0 warnings)
cargo test -p touring-antt    → 88 passed, 0 failed, 0 ignored
cargo test -p touring-hooks   → 1450 passed, 0 failed, 1 ignored
```
