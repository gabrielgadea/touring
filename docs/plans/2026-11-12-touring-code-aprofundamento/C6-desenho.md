# C6 — superfície do `IncrementalPipeline` (nota de desenho, rascunho 1)

Método: grep em todo `crates/` (excl. `target/`, `docs/`) por nome de método, receptor conferido
na linha; cadeia transitiva até o chamador final; confirmação final pelo compilador (passo 2).
Produção = não-teste, não-bench, não-exemplo. Arquivo: `crates/touring-code/src/ast/incremental_pipeline.rs`
(1760 l, 45 `pub fn`, 4 `pub struct`).

## Veredito por símbolo (P = produção, T = só teste/bench)

| Símbolo (tipo::fn) | Chamadores de produção | Só teste/bench | Veredito |
|---|---|---|---|
| Incremental::new | cortex `incremental_indexing.rs:81`, `neural.rs:23` | e2e, benches | fica |
| Incremental::process_file | cortex `incremental_indexing.rs:201`, `neural.rs:394`; hook-runtime `reindex.rs:180` | muitos | fica |
| Incremental::process_edit | cortex `incremental_indexing.rs:188`; hook-runtime `reindex.rs:156` | in-file | fica |
| Incremental::get_symbols | interno `process_edit:208`; via Shared → `HookRuntime::get_cached_symbols` | hook-runtime tests | fica (contrato do lead) |
| Incremental::get_document | hook-runtime `reindex.rs:142` | in-file | fica |
| Incremental::cache_stats | via Shared → `HookRuntime::pipeline_cache_stats` | cortex `neural.rs:996` | fica |
| Incremental::with_symbol_store | via Shared → `hook_runtime.rs:655` | in-file | fica |
| Incremental::cached_tree | só `SharedPipeline::has_cached_tree` (mesmo arquivo) | — | privado |
| Incremental::clear_cache | só `SharedPipeline::clear_cache` | in-file | a decidir (ver Shared) |
| Incremental::with_cache_capacity | 0 | 0 | remove (doc cita env que ninguém lê) |
| Incremental::memory_usage_mb | 0 | 0 | remove |
| Incremental::symbol_cache_stats / symbol_cache | 0 | 0 | remove |
| Incremental::with_memory_budget_mb | 0 (+ `max_memory_bytes`, `evict_if_over_budget` dependem) | 0 | **decisão do lead** (guarda de memória) |
| Incremental::queue_for_lazy / ensure_loaded / pending_lazy_count / prefetch_from_heat | 0 | e2e ×~14, bench, 2 in-file | **decisão do lead** (REGRA #0: heat produzido em produção) |
| Incremental::process_edit_with_heat, prefetch_for_session | 0 | in-file aco_tests / 0 | **decisão do lead** (mesmo cluster) |
| Prioritized::{new,pipeline,enqueue,enqueue_with_priority,next_hot,pending_count} | 0 | bench, in-file | **decisão do lead** |
| Shared::new | hooks-shared `parser_cache.rs:44` | e2e | fica |
| Shared::with_symbol_store | hook-runtime `hook_runtime.rs:655` | — | fica |
| Shared::with_read / with_write | hook-runtime `reindex.rs:142,156,180` | — | fica |
| Shared::process_file | hook-runtime `impls_symbols.rs:75`, `hook_runtime_ext.rs:92` ← cli `index.rs:991` | e2e | fica |
| Shared::has_cached_tree | hook-runtime `reindex.rs:121` | — | fica |
| Shared::get_symbols / cache_stats | via `HookRuntime` (chamador final: só teste) | in-file | fica (contrato) |
| Shared::process_edit | 0 (`reindex.rs` usa `with_write`) | 0 | remove |
| Shared::clear_cache | 0 | in-file | remove com `Incremental::clear_cache` |
| Shared::invalidate_dependents | 0 | 0 | remove |
| AsyncShared::* (8) + feature `async-pipeline` | 0 | 7 in-file | remove |

## Pendências
- Decisão do lead sobre o cluster heat/lazy e o orçamento de memória (mensagem enviada).
- Arquivos fora da lista que a remoção exige: `touring-code/Cargo.toml`, `src/ast/mod.rs`, benches.
