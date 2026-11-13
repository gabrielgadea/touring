# C7 — heat e orçamento de memória do pipeline do daemon (nota de desenho, SEM código)

Pedido do Gabriel (12/11): «ligue os produtores de heat» · «ligue o orçamento de memória do pipeline».
NÃO é refactor: muda o comportamento do daemon. Base: HEAD `a3bfb950` + WIP do C6. Tudo vem de leitura do código em 12/11.

## 1. O que existe hoje

- **Produtores**: `post_edit.rs:782-783` (`record_edit`) e `pre_read.rs:231-232` (`record_access`), em `runtime.learning.heat_map: RefCell<HeatMap>` (`hook_runtime.rs:206`; criado em `:734`, capacidade 100, só em memória: nasce vazio a cada boot). Leitor de produção: nenhum.
- **Pipeline do daemon**: `infra.pipeline: Option<SharedPipeline>` (`hook_runtime.rs:370`), construído em `:655` (`SharedPipeline::with_symbol_store`), sem teto. Entra por `extract_symbols_via_pipeline` (`reindex.rs:111`; `process_edit` em `:156`, `process_file` em `:180`; chamado em `:439` por `reindex_file_with_old`) e por `HookRuntime::process_file` (`hook_runtime_ext.rs:92`, `impls_symbols.rs:75`), que o `index rebuild` chama para TODO arquivo (`index.rs:991`).
- **Orçamento**: `with_memory_budget_mb` (`incremental_pipeline.rs:125`), `memory_usage_mb` (`:131`), `evict_if_over_budget` (`:138`, só chamado em `process_file:275`). Três defeitos latentes que armar o teto torna vivos: (i) vítima = `documents.keys().next()`, ordem arbitrária de HashMap, e pode ser o arquivo recém-inserido (um `process_edit` direto nele erra "sem load prévio"; pelo `reindex.rs:121` cai em reparse completo); (ii) solta só o `RopeDocument`, não a árvore (`parser`) nem o `symbol_cache` — o teto mede rope, e quanto a árvore pesa na RSS é o que M2 mede; (iii) `process_edit` (`:172`) cresce o rope sem checar o teto.
- **Medido (esta máquina, i9-14900HX, 62 GB)**: fonte do workspace = 3.782 arquivos de código, 64,3 MB (34,2 MB `.rs`) + 2.027 `.md`, 18,2 MB (sem `target/.git/site/client`); daemon vivo com RssAnon 1.512.796 kB. A parcela do pipeline é DESCONHECIDA: nada a expõe (`pipeline_cache_stats`, `hook_runtime_ext.rs:112`, só tem chamador em teste).

## 2. Ligação A — orçamento

- `SharedPipeline::with_memory_budget_mb(self, mb: usize) -> Self` (consome o RwLock, delega ao builder existente) e `SharedPipeline::stats(&self) -> PipelineStats { documents, trees, memory_mb, budget_mb, evictions }`. Em `hook_runtime.rs:655`: `.map(|s| s.with_memory_budget_mb(mb))`, com `mb` de `pipeline_memory_budget_mb(raw: Option<&str>) -> usize` (pura, testável) alimentada por `TOURING_PIPELINE_MEMORY_MB` ao lado do `TOURING_CACHE_TTL_SECS` (`:668`).
- **Knob**: `TOURING_PIPELINE_MEMORY_MB` (inteiro, MB; ausente, `0` ou inválido = sem teto = o daemon de hoje). Vale no ambiente do DAEMON, no boot.
- **Contrato novo** (substitui o interno; sem duas políticas): o arquivo recém-processado nunca é vítima; ordem determinística (empate por caminho); o teto também é checado após `process_edit`; a vítima perde documento, árvore (`parser.invalidate_tree`) e entrada do `symbol_cache`.

## 3. Ligação B — heat

| Leitor já escrito | Veredito (contra o código) |
|---|---|
| `process_edit_with_heat` (`:628`) em `reindex.rs:156` | NÃO. Chama `record_edit` outra vez (post_edit já grava em `:782`: contagem em dobro) e SOBRESCREVE `blast_radius_weight` com `ln_1p(symbols_added)/10`, o tamanho do delta da edição, não o peso do arquivo; cobre só o ramo incremental |
| `PrioritizedPipeline` (`:673`) | NÃO como está: embrulha um `IncrementalPipeline` PRÓPRIO + HeatMap próprio; no daemon seria um 2º pipeline (memória em dobro). Consumível só se `inner`/`pipeline()` saírem e sobrar a fila (`enqueue`/`next_hot`), usada no walk em lote `file_knowledge.rs:178-227` e no `rebuild` (quente primeiro) |
| `prefetch_from_heat` (`:407`) → `prefetch_for_session` (`:380`) | SIM, mas o efeito é só pós-evicção/pós-boot (1ª edição incremental, `reindex.rs:121`); com heat vazio no boot é no-op (Q2). Gatilho: `run_session_start` (`session_hooks.rs:28`), `N` fixo |

- **Leitor principal (novo, mesma primitiva `HeatMap::get_priority_order`)**: `SharedPipeline::enforce_budget(&self, heat: &HeatMap, keep: &str) -> usize` — o heat ordena a evicção (arquivo sem entrada no heat = mais frio; empate por caminho). Chamado por `extract_symbols_via_pipeline` (recebe `runtime.learning.heat_map.try_borrow()`; borrow falho = ordem por caminho, o hook nunca bloqueia) e por `HookRuntime::process_file`. É o único leitor com efeito observável em regime: QUAL arquivo sai quando o teto aperta. Nenhum dos três já escritos cumpre isso sozinho; é decisão do lead (Q1).

## 4. Contador observável

`touring status -j` (RPC `cli-status`, `hook_registry.rs:1115`; handler `health.rs:398`, JSON em `:465-472`) ganha `"pipeline": {documents, trees, memory_mb, budget_mb, evictions}` e `"heat": {tracked: HeatMap::len(), consulted: n de enforce com heat não vazio, prefetched: n}`. Dá a `pipeline_cache_stats` o primeiro chamador de produção.

## 5. Como medir o default (sem inventar número)

- **M1**: com o status armado e SEM teto, amostrar `touring status -j | jq .pipeline` de hora em hora na máquina do Gabriel (i9-14900HX, 62 GB, daemon de ~24 h) por ≥ 5 dias úteis, incluindo ≥ 1 `index rebuild`; guardar `memory_mb` p50/p99/max e `documents`.
- **M2**: `examples/pipeline_memory_probe.rs` (precedente `rebuild_memory_probe.rs`), sob `systemd-run --user --scope -p MemoryMax=8G`: carrega o workspace inteiro (3.782 arquivos / 64,3 MB hoje) e imprime `memory_usage_mb()`, nº de árvores e RssAnon antes/depois, ou seja, MB anônimos por MB de rope.
- **Default** = max(M1) arredondado para cima: nunca evicta no uso normal e só contém crescimento patológico; o número sai da medição. Até M1/M2 rodarem o default é `0` (sem mudança de comportamento por padrão).

## 6. Testes RED→GREEN

1. `touring-code`: `over_budget_evicts_the_coldest_not_the_hottest` — RED: `enforce_budget` não existe (hoje a vítima é ordem de HashMap).
2. `the_file_just_processed_is_never_its_own_victim` — teto 1 MB, arquivo 2 MB; RED: `get_document` é `None` após `process_file`.
3. `eviction_releases_tree_and_symbol_cache` — RED: `has_cached_tree` segue `true`.
4. `process_edit_respects_the_budget` — RED: o rope passa do teto.
5. `touring-hook-runtime`: `pipeline_memory_budget_mb` (ausente/0/"abc"/"64") e `runtime_pipeline_carries_the_budget` — RED: sem knob.
6. `touring-cli`: `cli_status_reports_pipeline_and_heat` após reindex + evicção — RED: chaves ausentes.
7. B2: `session_start_prefetches_the_hottest` — RED: nenhum chamador de `prefetch_*`.

## 7. Arquivos tocados

`touring-code/src/ast/incremental_pipeline.rs` · `touring-hook-runtime/src/{hook_runtime.rs,hook_runtime_ext.rs,runtime/impls_symbols.rs,shared/reindex.rs}` · `touring-hook-handlers/src/hooks/session_hooks.rs` (só B2) · `touring-cli/src/cli/health.rs` · `touring-code/.claude/CLAUDE.md` · `GLOSSARY.md`. Não tocar: `post_edit.rs`, `pre_read.rs` (produtores ficam como estão).

## 8. Riscos

- Com teto, a 1ª edição de um arquivo evictado vira `process_file` (reparse completo) em vez de incremental: custo a medir em `was_incremental` e no p99 do reindex (sob `hw-slot`).
- O `with_write` do RwLock fica retido durante `enforce_budget`: O(n log n) em n documentos (3.782 após um rebuild) mais o ranking do heat (capacidade 100).
- O HeatMap guarda 100 entradas: com 3.782 documentos após o rebuild, ~97% não têm heat e a evicção deles cai em "ordem por caminho"; os 100 quentes sobrevivem, e é só isso que se garante.
- O teto cobre só `infra.pipeline`. O processo tem OUTRO `IncrementalPipeline` real, estático e sem teto, no cortex (`touring-cortex/src/handlers/incremental_indexing.rs:42,81`); o `FileParserCache` (`touring-hooks-shared/src/parser_cache.rs:44`) só cria pipelines vazios (`get_or_create`, sem parse). O status deve somar o do cortex ou o número subestima o daemon.
- Heat não persiste: depois de um restart do daemon (`update-touring`, várias por dia) o prefetch não tem o que aquecer.

## 9. Perguntas ao lead

- **Q1**: o aceite «nenhum símbolo do cluster órfão» não fecha sem decidir `process_edit_with_heat` (ligar duplica heat) e `PrioritizedPipeline` (ligar duplica pipeline). Recomendo: reduzir `PrioritizedPipeline` à fila e ligá-la ao walk em lote; remover `process_edit_with_heat` e seus 2 testes (contraria «não remover», por isso pergunto); `prefetch_*` + `enforce_budget` como leitores.
- **Q2**: semear o heat no boot a partir de `file_knowledge` via `FileHeat::from_existing` (hoje só `cli_ast_heat` o usa; falta um `HeatMap::insert`)? Sem isso B2 é no-op após restart.
- **Q3**: `cli_ast_heat` (`index.rs:1989`) calcula `edits` como `COUNT(*)` de linhas de `wiring_map` do módulo (`:2001`), que não é contagem de edições, e não lê o heat do runtime. Fazê-lo ler `rt.learning.heat_map` muda a saída de `touring ast heat`: dentro do C7?
- **Q4**: soltar a árvore junto muda o contrato «teto = bytes de rope». Recomendo sim (a árvore é o que pesa na RSS; M2 mede quanto).

## 10. Atualização (round 3, 12/11/2026) — decisões do lead depois do M2

O knob deixou de ser «ausente = sem teto»: ausente = `default_pipeline_budget_mb(rebuild_memory_hard_mb())` = `max(64, floor((H/8 − 260)/1,43))`; `0` = sem teto (kill switch); inválido = o default + aviso no log. `touring status -j` ganhou `pipeline.budget_source` (`env|default|disabled`). Contexto completo: `c7-relatorio.json` > `round3`.
