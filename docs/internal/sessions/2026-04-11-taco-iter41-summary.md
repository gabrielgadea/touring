# TACO Iter41 — EC57: FileParserCache + MemoryBus::Debug + ResultExt/OptionExt wired em pre_bash

**Data**: 2026-04-11
**Iteração**: 41
**EC implementado**: EC57
**Arquivos modificados**: `crates/touring-hooks/src/shared/parser_cache.rs`, `crates/touring-hooks/src/post_write.rs`, `crates/touring-hooks/src/ann_memory/mod.rs`, `crates/touring-hooks/src/cortex_dispatcher.rs`, `crates/touring-hooks/src/branch_fs.rs`, `crates/touring-hooks/src/pre_bash.rs`, `crates/touring-hooks/src/shared/mod.rs`
**Resultado**: 0 erros cargo check, 0 warnings, 1450 tests passing, sem regressão

---

## EC57 — Múltiplas correções de dead_code + wiring de production callers

### Problema 1: `FileParserCache` com 8 `#[allow(dead_code)]` vestígiais

Métodos `clear` e `get_or_create` já tinham callers em produção (`pre_edit.rs`, `pre_read.rs`)
mas ainda carregavam `#[allow(dead_code)]`. `invalidate` era orphan — moka's `get_with` é
idempotente, então re-warm após write sem invalidate era no-op.

**Mudanças em parser_cache.rs:**
- Removidos `#[allow(dead_code)]` de `clear` e `get_or_create` (vestigiais)
- Removido `#[allow(dead_code)]` de `invalidate` (agora tem caller em post_write.rs)
- Adicionado doc em `invalidate` explicando semantics de invalidade + re-create
- Removido `with_capacity` (wrapper redundante de `new`, 0 callers)
- Removido `is_empty` (0 callers, redundante com `len() == 0`)

**Mudanças em post_write.rs (EC57 first caller de `invalidate`):**
```rust
// EC57: Invalidate the stale pipeline entry first — moka's get_with returns
// the existing entry if present, so without invalidate the "warm" is a no-op
// after a file write that changed content.
let cache = POST_WRITE_PARSER_CACHE.get_or_init(FileParserCache::new);
let path_buf = std::path::PathBuf::from(file_path);
cache.invalidate(&path_buf);
let _ = cache.get_or_create(&path_buf);
```

### Problema 2: `MemoryBus` sem `Debug` bound → CortexDispatcher não compilava

`CortexDispatcher` tem `#[derive(Debug)]` + campo `memory_bus: Option<Arc<dyn MemoryBus>>`.
`MemoryBus: Send + Sync` mas não `Debug` → E0277 no derive.

**Fix em ann_memory/mod.rs:**
```rust
pub trait MemoryBus: Send + Sync + std::fmt::Debug {
```
`AnnMemoryRecall` já derivava `Debug` → implementação compliant automaticamente.

**Fix em cortex_dispatcher.rs:**
- Removido `AnnMemoryRecall` do import (unused — só `MemoryBus` era necessário)

### Problema 3: Imports `ResultExt`/`OptionExt` unused em pre_bash.rs e branch_fs.rs

Hook revertia remoção de imports em `pre_bash.rs`. Solução: wire genuíno nos dois arquivos.

**branch_fs.rs:**
```rust
// EC57: ResultExt::unwrap_or_debug — logs if system clock is before UNIX_EPOCH.
.unwrap_or_debug(std::time::Duration::ZERO, "branch_fs: system clock before UNIX_EPOCH")
```

**pre_bash.rs — 2 integrações:**
1. `OptionExt::unwrap_or_debug` no CILA level cache lookup:
```rust
.unwrap_or_debug(3, "pre_bash: session CILA level not in cache — defaulting to 3")
```
2. `ResultExt::unwrap_or_debug` no BM25 rank id parse:
```rust
.map(|r| r.id.parse::<usize>().unwrap_or_debug(0, "pre_bash: BM25 rank id is non-numeric"))
```

**shared/mod.rs:** Removido `OptionExt` do re-export `pub(crate) use result_ext::ResultExt` —
`OptionExt` só é importado localmente onde necessário.

---

## Validação

```
cargo check --workspace       → Finished (0 errors, 0 warnings)
cargo test -p touring-hooks   → 1450 passed, 0 failed, 1 ignored
```
