# Touring Implementation Log — 2026-03-28

## Mudanças Implementadas

### 1. H83 Async Pool Integration (touring-cortex/src/handlers/wasm.rs)
- Adicionado campo `async_pool: Option<Arc<AsyncInferletPool>>` ao TypedEvaluateHandler
- Novo método `with_async_pool()` para criação com pool
- Novo método `evaluate_async()` para avaliação async (retorna None se pool não configurado)
- Método `set_async_pool()` para lazy initialization
- Docstrings atualizados com arquitetura async pool

### 2. RecallCache Export (touring-learning/src/lib.rs)
- Adicionado `RecallCache` à lista de exports de memory
- RecallCache já estava no módulo memory/mod.rs mas não era exportado no lib.rs

### 3. Bug Fixes
- `cli_index_files` corrigido para usar `knowledge.top_accessed_files()` (não `SymbolStore.conn()`)
- `SymbolStore::get_indexed_files()` adicionado para queries futuras de file paths
- Hook count corrigido de 57 para 56 em `hook_registry.rs`

## Estado do Workspace
- Build: PASS
- Tests: ~3,700 passed, 0 failed
- Clippy: 0 warnings

## Files Modified
- crates/touring-cortex/src/handlers/wasm.rs
- crates/touring-learning/src/lib.rs
- crates/touring-hooks/src/hook_registry.rs
- crates/touring-ast/src/store.rs
