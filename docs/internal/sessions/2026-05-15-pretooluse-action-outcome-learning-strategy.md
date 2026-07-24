---
title: "PreToolUse [*] Action-Outcome Learning Injector — Strategy"
date: 2026-05-15
status: proposal
author: TACO (touring-architect research + orchestrator synthesis)
commissioned_by: Gabriel Gadea
---

# Strategy: Universal PreToolUse `[*]` Action-Outcome Learning Injector

> **Commissioned 2026-05-15** — Gabriel pediu uma estratégia inteligente para um
> hook `PreToolUse [*]` que identifica a tool + a intenção e injeta o aprendizado
> acumulado sobre erros relacionados à ação iminente. Pesquisa conduzida por um
> `touring-architect` sobre o codebase real (`cli_suggester.rs`, outcome stores,
> `post_tool_rl`, JSONL watcher) + Context7. Toda afirmação sobre o sistema
> existente está ancorada em evidência `file:line`. FACT (verificado em código) /
> INFERENCE (0.7-0.9) / RECOMMENDATION separados.

## Executive Summary

Esta estratégia **estende** `crates/touring-hooks/src/cli_suggester.rs` (FACT:
1163 linhas) com três capacidades novas: (1) uma **ActionSignature** que torna
outcomes passados recuperáveis pelo *tipo* da ação tentada — não só por arquivo
ou comando; (2) **retrieval multi-fonte ranqueado** que funde gotchas + memory +
as tabelas `bash_outcomes`/`edit_history` numa única injeção top-K com budget;
(3) um **CC transcript miner** — pipeline offline genuinamente novo que transforma
os `.jsonl` UUID sob `~/.claude/projects/` em lições rotuladas `(erro → resolução)`
no mesmo substrato gotcha/memory que o hook PreToolUse já consulta. O loop de
feedback fecha via `post_tool_rl.rs`, que já emite `signal_tool_outcome()`.

---

## Part 1 — O que já existe (FACT, file:line)

### 1.1 `cli_suggester.rs` — capacidade atual
- FACT: `cli_suggester.rs:260-508` — sete classificadores cobrindo
  `Bash|Grep|Glob|Read|Edit|Write` apenas (`cli_suggester.rs:261-269`) — **não `[*]`**.
- Para cada match com `conf >= 0.7` injeta `MUST/SHOULD/MAY` + enrichment vivo
  (`symbol_in_index`, `file_indexed`, `dependent_count`, `pub_symbol_count`,
  até 3 `gotcha_matches`) em `additionalContext`.
- Enrichment (`cli_suggester.rs:795-855`): chamadas in-process diretas
  (`symbol_store.find_symbol()`, `knowledge.get_gotchas_for_file()`,
  `knowledge.get_dependents()`) — sem subprocess.
- Cache (`cli_suggester.rs:69-88`): `moka::sync::Cache<u64,()>` TTL 300s, chave
  `hash(tool_name ++ tool_input_json)` — é **supressor de spam**, não índice de retrieval.
- **NÃO faz hoje**: cobrir `Task*`/`mcp__*`/`TodoWrite`/`WebFetch`; consultar
  `bash_outcomes`/`edit_history`/`learning_tool_outcomes`; recuperar memory;
  ranquear cross-source; nenhum conceito de "action signature".

### 1.2 Outcome stores — schemas confirmados
- FACT `touring-foundation/src/schema/knowledge.rs:87-113`:
  `bash_outcomes(command, command_short, command_hash, exit_code, success, error_pattern, file_context, executed_at)` ·
  `edit_history(file_path, edit_type, summary, error_pattern, language, symbol_context, session_id, edited_at)` ·
  `gotchas(pattern, gotcha, severity, symbol_name, language, hit_count, prevented_errors, decay_score)`.
- FACT `schema/graph.rs:50-58`: `learning_tool_outcomes(tool_name, file_path, success, latency_ms, context_json, recorded_at)`.
- FACT `knowledge.rs:1140-1165`: `record_bash_outcome()` grava 1 linha/execução, chave
  `command_hash = sha256(cmd)` + `command_short` (primeiro token). `find_bash_outcomes()` recupera por `command_short`.
- FACT `pre_bash.rs:298`: pre_bash JÁ faz `db.find_bash_outcomes(short_key,10)` + `compose_relevant_context()`
  com ranking BM25 sobre `error_pattern` — **já funciona para Bash**, mas só dentro de pre_bash.

### 1.3 RL — por-tool, não por-ação
- FACT `post_tool_rl.rs:24-51`: `extract_tool_metadata()` extrai `tool_name/file_path/output/has_error`.
  `ImmediateReward` rastreia nível-de-tool (ex `"Edit"`), não nível-de-classe-de-ação.
- FACT `post_tool_rl.rs:276`: `session_bus.signal_tool_outcome(!has_error, quality)` dispara em todo tool use — **heartbeat do loop, já existe**.
- FACT: LinUCB arms indexados por `arm_id` inteiro, sem chave de ação normalizada.

### 1.4 `classify-intent` — nível CILA, não classe-de-ação
- FACT `touring-hooks-prediction/src/classifier.rs:1-17`: classificador CILA mapeia
  *prompts* para níveis L0-L6 (79 regex). NÃO classifica tool calls individuais; não está wired em PreToolUse.
- INFERENCE [0.85]: o hook `classify-intent` (`touring-cortex/src/handlers/neural.rs:612`) opera
  intent de sessão, não per-tool-call — não reutilizável para inferência de classe-de-ação sem adaptação.

### 1.5 `post_tool_failure.rs` — cria lição, file-scoped
- FACT `post_tool_failure.rs:185-196`: falha → `add_gotcha(pattern,…)` com `pattern = filename`.
  Loop parcial existe (falha → gotcha → injeção futura no mesmo arquivo) mas é **file-scoped, não action-scoped**.

### 1.6 JSONL watcher — observa telemetria do Touring, NÃO os transcripts do CC
- FACT `touring-server/src/ingest/watcher.rs:285-288`: `discover_jsonl_paths` varre
  `.claude/data/` + `.claude/metrics/` — arquivos do próprio Touring. NÃO varre `~/.claude/projects/<proj>/*.jsonl`.
- FACT `ingest/parser.rs:11-16`: 5 `SourceType` (`Compliance/Cost/Skill/HookTrace/Lna`) — sem `ConversationTranscript`.
- **GAP CONFIRMADO**: os transcripts do CC (`~/.claude/projects/.../*.jsonl`) — centenas de sessões de dados
  rotulados (`tool_use` que falhou seguido de correção) — estão **totalmente não-minerados**. Maior fonte inexplorada.

---

## Part 2 — A ActionSignature (decisão de design central)

```
ActionSignature = (tool_class, intent_class, context_qualifier)
```

- **tool_class** — agrupamento grosso: `Bash→bash`, `Edit/NotebookEdit→edit`, `Write→write`,
  `Read→read`, `Grep/Glob→search`, `Task/TodoWrite→task`, `WebFetch/WebSearch→web`, `mcp__*→mcp`, resto→`other`.
- **intent_class** — inferido do input em <1ms: `bash` → primeiro token (`command_short`, já existe em
  `post_bash.rs:277`); `edit/write/read` → extensão (`is_rust_file()` já em `cli_suggester.rs:224`);
  `search` → `symbol` vs `free-text` (já distinguido em `classify_grep()` `cli_suggester.rs:511`);
  `task/web/mcp` → nome da tool.
- **context_qualifier** — flags de risco derivadas do enrichment já em memória: `hi-blast`
  (`dependent_count>10`), `hi-complexity` (`cognitive_score>0.7`), `gotcha-active`, `no-index`,
  `new-symbol`, ou `plain`.

Exemplos: `cargo check` → `(bash,cargo,plain)`; `Edit big_file.rs` (17 deps, 2 gotchas) →
`(edit,rs,hi-blast+gotcha-active)`; `Grep "HookRuntime"` → `(search,symbol,plain)`.

Chave de storage: `outcome:<tool_class>:<intent_class>:<context_qualifier>` em `memory --tier semantic`
— imediatamente recuperável por `touring memory recall`. Computada em ~0.1ms de dados já disponíveis.

---

## Part 3 — Inferência de intenção (classificação rápida)

Estender (não substituir) `classify()` em `cli_suggester.rs:260` — já computa `cluster/symbol_hint/file_hint`.
Após `classify()` retornar, computar `compute_action_signature(tool, input, classifier, enrichment)`.
Para tools não classificadas (Task/mcp__*/WebFetch), classifier retorna `None` → signature ainda computada
com `intent_class = tool_name` e `qualifier` do enrichment se houver `file_path`. Latência estimada:
<2ms tools cobertas, <1ms tools novas.

---

## Part 4 — Retrieval multi-fonte + ranking

- **Fonte 1 — Gotchas** (existe): `get_gotchas_for_file()` já em `cli_suggester.rs:832` → promover ao ranking combinado.
- **Fonte 2 — `bash_outcomes`/`edit_history`** (parcial): mover `compose_relevant_context()` de
  `pre_bash.rs:292-315` para o path de retrieval compartilhado. `edit_history` por `language + error_pattern`
  (índice `idx_edit_error_ctx` já existe).
- **Fonte 3 — Memory**: `touring memory recall "outcome:<sig>"` — cresce com o loop de feedback.
- **Fonte 4 — Lições mineradas dos transcripts CC** (NOVO, Part 5) — caem em `gotchas`/`memory`, mesmo path de retrieval.
- **Ranking** (determinístico, sem ML): `score = severity_weight × recency_weight(half-life 30d) ×
  frequency_weight(satura em 5 hits) × signature_match_weight`. **Budget**: 800 chars para a seção
  error-learning; top-K por diversidade (sem dois itens com mesmo `error_pattern[:50]`).

---

## Part 5 — CC Transcript Mining Pipeline (genuinamente novo)

Componente novo: `crates/touring-server/src/ingest/transcript_miner.rs`.

- **Trigger**: offline, NÃO no hot path. (a) sweep no startup do daemon (offset-tracking via padrão
  `WatcherState` `watcher.rs:21-44`); (b) task background tokio a cada 5min. Estender `discover_jsonl_paths()`
  com 2º path `~/.claude/projects/**/*.jsonl`.
- **Algoritmo** (state machine): para cada record — `assistant tool_use` → pendente; `user tool_result`
  com `is_error:true` → guardar `failed_tool_use` + `error_text`; varrer próximos 3 turns assistant por
  `tool_use` corretivo (mesma tool/file) cujo result seja `is_error:false` → emitir
  `ErrorResolutionPair{tool_name, failed_input, error_text[:500], resolution_input, session_id, ts}`.
- **De-dup**: hash `(tool_name, sha256(error_text[:200]))` — se gotcha já existe, só incrementa `hit_count`.
- **Storage**: cada par → (1) linha `gotcha` (`pattern=filename/command_short`, severity `warning`) — surge
  no `get_gotchas_for_file()` existente; (2) entrada `memory tier=semantic` key `outcome:transcript:<tool>:<sha8>`
  — indexada no Tantivy para BM25.
- INFERENCE [0.65]: ~100 sessões × ~500 tool calls, ~10% falham, ~70% têm correção observável →
  ~3500 pares brutos → ~500-1000 gotchas únicos pós-dedup. Corpus significativo.
- Mining é 100% offline (`tokio::spawn` como a task de evolution `server/mod.rs:702`); sweep full estimado 200-500ms.

---

## Part 6 — O matcher `[*]` (rollout faseado)

- **Fase 1 (BAIXO RISCO)** — estender as 6 tools atuais com ActionSignature + retrieval multi-fonte.
  Sinal puramente aditivo. ~2-3 dias.
- **Fase 2 (MÉDIO)** — matcher `[*]` em settings.json, handler retorna `"{}"` para tools fora do conjunto
  + classifiers novos para `Task`/`TodoWrite`/`WebFetch` (alto valor: erro de escopo em `Task`, URLs ruins em `WebFetch`). ~1 dia.
- **Fase 3 (CONTÍNUO)** — conforme o miner gera lições para `mcp__*` etc., o retrieval passa a injetar
  contexto mesmo sem classifier per-tool.
- **Fail-open mandatório**: `run()` em `cli_suggester.rs:943` já retorna `"{}"` em erro; o path novo do `[*]`
  deve envolver tudo em `match {Ok(s)=>s, Err(_)=>"{}".into()}`. Exit 0 sempre.

---

## Part 7 — Loop de feedback

- Componentes existentes: `post_tool_rl.rs:276` `signal_tool_outcome()`; `post_tool_failure.rs:71`
  `record_failure_outcome()`; `post_bash.rs:276-305` `record_bash_outcome()`.
- **Elo faltante**: em `post_tool_rl.rs`, após `extract_tool_metadata()`, computar a MESMA `ActionSignature`
  e persistir `memory_store_quick("outcome:<sig>:failure"/"…:success", …, "semantic")` (<1ms, SQLite insert no post-hook async).
- **RL reward de qualidade de injeção**: quando contexto foi injetado E a tool subsequente teve sucesso →
  `reward_linucb("context_injection_helped", 0.3)`. Proxy `__action_sig_injected__` estendendo o padrão de
  `post_tool_rl.rs:64-70`.

---

## Part 8 — EXTEND vs NOVO + esforço

| Componente | Status | Esforço |
|---|---|---|
| ActionSignature | EXTEND `cli_suggester.rs` | 1d |
| Retrieval multi-fonte (bash_outcomes+gotchas+memory) | EXTEND (mover `compose_relevant_context` de pre_bash) | 1-2d |
| Ranking top-K + budget | EXTEND `render()` | 1d |
| Outcome write action-scoped | EXTEND `post_tool_rl.rs` (~10 linhas) | 0.5d |
| Registro `[*]` settings.json | NOVO config | 0.5d |
| **CC transcript miner** (`transcript_miner.rs`) | **NOVO** (+ `SourceType::ConversationTranscript`) | 3-4d |
| `discover_jsonl_paths()` → `~/.claude/projects/` | EXTEND `watcher.rs:288` | 0.5d |
| RL reward de injeção | EXTEND `post_tool_rl.rs` (~5 linhas) | 0.5d |

**Total: 8-10 engineer-days** (full) · **3-4 dias** (só Fase 1).

---

## Part 9 — Budget de latência

PreToolUse `[*]` dispara em TODA tool call. Cache TTL hit ~0.01ms (caso dominante). Caminho completo
com todas as fontes: **~3-5ms** para tools cobertas; **<1ms** para tools novas (classifier `None`).
Veredicto: confortável — uma tool call leva 10ms-10s; 3-5ms é imperceptível.

---

## Part 10 — Métricas de sucesso

- **Primária** — `repeated_error_rate = erros com error_pattern idêntico a erro >24h mais antigo / total`.
  Baseline já mensurável de `bash_outcomes`+`edit_history` HOJE. Alvo: **>20% de redução** em 30d.
- **Secundária** — `injection_quality_rate = calls com injeção E sucesso subsequente / calls com injeção`.
  Alvo >60%.
- **Terciária** — `transcript_lesson_utilization_rate` — lições mineradas recuperadas ≥1×. Alvo >30% em 30d.
Todas computáveis de dados já armazenados.

---

## Part 11 — Riscos

| Risco | Severidade | Mitigação |
|---|---|---|
| Formato JSONL do CC muda entre versões | Médio | Parser fail-open: arquivo sem `tool_use`/`tool_result` → skip silencioso |
| Context flooding no `[*]` | Médio | Budget 800 chars; `"{}"` se conf<0.7; cache TTL 300s |
| Falso-positivo do miner (correção ≠ fix) | Médio | Exigir `is_error:false` no result corretivo; `decay_score` decai entradas não usadas |
| Performance em sequências rápidas de tools | Baixo | Cache TTL; max ~10×5ms=50ms imperceptível |
| `classify-intent` não reutilizável per-tool | Confirmado | Estratégia usa `command_short`+extensão+`is_rust_file()` — já implementados |
| Miner varrendo milhares de arquivos | Baixo | Leituras incrementais por offset (padrão `WatcherState`); scan full só no 1º startup |

---

## Síntese — MVP vs visão completa

- **Fase 1 (3-4d, alta confiança)** — ActionSignature + retrieval `bash_outcomes`/`edit_history` +
  ranking + budget 800 chars + write action-scoped em `post_tool_rl.rs`. Fica nas 6 tools atuais, zero risco.
- **Fase 2 (2-3d, média)** — CC transcript miner. Maior corpus de lições (milhares de pares rotulados).
- **Fase 3 (1-2d, baixo risco)** — matcher `[*]`, classifiers `Task`/`WebFetch`, RL reward de injeção.

Respeita REGRA #0 (potencializar): nada do zero. `compose_relevant_context()` já existe em `pre_bash.rs`;
`get_gotchas_for_file()` já dispara; `record_bash_outcome()` já persiste; `JsonlWatcher` já sabe rastrear
offsets. A estratégia costura isso via uma chave de ActionSignature compartilhada, adiciona a fonte dos
transcripts CC, e fecha o loop pelo heartbeat `post_tool_rl` que já dispara.

---

## Implementation Progress

### Phase 1, Slice 1 — ActionSignature foundation (DONE — 2026-05-15)

O "lado da escrita" do sistema de aprendizado. Delegado a um touring-engineer.

**Entregue:**
- Novo módulo `crates/touring-hooks/src/action_signature.rs` —
  `ActionSignature { tool_class, intent_class, context_qualifier }` +
  enum `ContextQualifier` (`HiBlast | GotchaActive | NoIndex | NewSymbol | Plain`)
  + `to_key() -> "outcome:<tc>:<ic>:<cq>"` + `from_pre_tool()` (lado PreToolUse)
  + `from_post_tool()` (lado PostToolUse) + `sanitize_intent()`.
- Wired em `cli_suggester.rs::run()` — computa a signature, anexa `sig=<key>` ao
  `additionalContext` injetado.
- Wired em `post_tool_rl.rs::run()` — após `extract_tool_metadata()`, persiste
  `outcome:<sig>:{success,failure}` via `cli_memory_store` (tier semantic). **O
  substrato de aprendizado já começou a acumular outcomes action-scoped.**
- 35 testes unitários no módulo; total `touring-hooks` **3257 testes passam**.

**Correções VGP ao strategy doc (pegas pelo engineer):**
- O doc chutou `memory_store_quick` — a função real é `cli_memory_store`
  (`cli_handlers.rs:3069`).
- O doc assumiu `cognitive_score` em `EnrichmentData` — o campo NÃO existe → o
  qualifier `hi-complexity` foi **deferido** para uma slice posterior (quando
  `cognitive_score` for exposto).

**Gates:** `cargo check -p touring-hooks` + `cargo check --workspace` exit 0;
`action_signature.rs`/`cli_suggester.rs`/`post_tool_rl.rs` **clippy-clean** (um
IIFE `(||{…})()` redundante foi removido na validação FASE 6); 0 orphans novos.

**Finding (pré-existente, NÃO da Slice 1):** `cargo clippy -p touring-hooks
--lib` reporta **32 clippy errors pré-existentes** em 14 arquivos NÃO tocados
pela Slice 1 (`team_hooks.rs` ×8, `latency_marker.rs` ×3, `activity_hook.rs` ×4,
`cli_handlers*.rs` ×6, etc. — `incompatible_msrv`, `nonminimal_bool`,
`collapsible_if`, …). Os gates de wave baseados em `cargo check` nunca pegaram
isto (`cargo check` ≠ `cargo clippy`). Uma limpeza de clippy do `touring-hooks`
é uma tarefa separada recomendada.

### Phase 1, Slice 2 — multi-source retrieval + ranked injection (DONE — 2026-05-16)

O "lado da leitura". Delegado a um touring-engineer.

**Entregue** — `cli_suggester.rs` estendido com 12 símbolos: `LessonItem` +
`severity_weight`/`recency_weight`/`frequency_weight` (ranking Part 4.5) +
`age_days_from_sqlite` + `collect_db_lessons` / `query_edit_failures_by_language`
/ `collect_memory_lessons` / `collect_gotcha_lessons` (retrieval) + `rank_and_trim`
(top-K, budget 800 chars) + `truncate` + `retrieve_and_render_lessons`. `run()`
agora recupera lições de erros passados que casam a `ActionSignature` e injeta a
seção top-K no `additionalContext`.

Fontes: memory `outcome:<sig>:*` (escrito pela Slice 1) + `bash_outcomes` /
`edit_history` (rusqlite direto, `LIMIT`-bounded) + `enrichment.gotcha_matches`.
Ranking determinístico; fail-open (query DB falha → seção omitida).

**Gates:** `cargo check -p touring-hooks` + `--workspace` exit 0; **3277 testes
touring-hooks** (+20 vs Slice 1); `cli_suggester.rs` clippy-clean; 0 orphans.

**Gotcha:** `age_days_from_sqlite` (fórmula JDN) não valida range de mês.

### Clippy cleanup do `touring-hooks` (DONE — 2026-05-16)

O finding da Slice 1 (32 clippy errors pré-existentes) foi resolvido a pedido de
Gabriel ("corrija tudo"): `cargo clippy --fix` autofixou ~18; um engineer
corrigiu os 14 residuais — 5 MSRV (bump `rust-version` 1.75→1.80 em
`touring-hooks/Cargo.toml`); 2 `manual_clamp`; 2 `blocks_in_conditions`;
`explicit_counter_loop`; `ptr_arg` `&PathBuf`→`&Path` (8 fns de `activity_hook.rs`,
API-compatível); `empty_line_after_doc_comment`; `if_same_then_else`;
`too_many_arguments` → `#[allow]` justificado em `walk_dir_recursive`.
**`cargo clippy -p touring-hooks --lib` → 0 errors.** 3277 testes preservados.

### Findings resolvidos (2026-05-16)

A pedido de Gabriel ("ataque os 2 findings"):

- **F1 — 15 clippy warnings → 0.** 1 era touring-hooks (`unused import PathBuf`
  em `activity_hook.rs` — regressão do `&PathBuf`→`&Path` da cleanup, corrigida);
  11 em `inferlets` (`map_or`→`is_some_and`, `strip_prefix`, `to_vec`, `clamp`,
  `cloned`, if/else colapsado, `is_some`); 3 em `touring-bindings`
  (`wasm/cloudflare.rs` extern fns → `#[allow(improper_ctypes_definitions)]`
  justificado: ABI wasm32 Cloudflare Workers, não FFI C real).
  `cargo clippy -p touring-hooks --lib` → **0 errors + 0 warnings**.
- **F2 — `context_mode_e2e` SIGSEGV → eliminado.** Root cause: race de
  concorrência — os 16 testes de integração compartilhavam estado on-disk e
  corrompiam memória sob `cargo test` paralelo (passavam 16/16 com
  `--test-threads=1`). Fix: isolamento de estado por-teste (`tempfile`) +
  `serial_test` onde o fixture é compartilhado. Verificado 16/16 em paralelo,
  3 runs. Pré-existente, não-relacionado à Slice 1/2.
- **Finding novo — RESOLVIDO (2026-05-17):** `sandbox_executor::test_new2_cleanup_tee_removes_old_files`
  era flaky. **Root cause real:** NÃO era tempdir (cada teste já tem seu próprio
  `TempDir`) — era a env var **global** `TOURING_TEE_DIR`. O helper `with_tee_dir`
  fazia `std::env::set_var`/`remove_var` (mutação process-global); 2+ testes
  `test_new2_*` em threads paralelas sob `cargo test` sobrescreviam o mesmo slot,
  e um `remove_var` podia limpar a var enquanto outro teste estava mid-closure →
  `cleanup_tee(0)` operava num diretório diferente do que `store_tee` escreveu.
  O comentário do helper ("unique prefix → no collision") era falso: o *valor* é
  único mas a *chave* da env var é um slot global compartilhado.
  **Fix:** `static TEE_ENV_LOCK: std::sync::Mutex<()>` adquirido dentro de
  `with_tee_dir`, cobrindo toda a janela `set_var → closure → remove_var` —
  serializa exatamente os 5 testes env-sensitivos e nada mais. Zero mudança em
  produção, zero dependência nova (não precisou de `serial_test`), recovery de
  mutex poisoned para não cascatear falha. **Validado:** stress-test 30/30 green
  sob paralelismo default; `sandbox_executor` clippy-clean (0 hits).
  **Achado colateral (separado, fora deste escopo):** `cargo clippy -p
  touring-hooks --lib --tests` reporta 2 erros `needless_collect` pré-existentes
  (`cli_handlers_e2e` + 1 lib test) — débito não relacionado a esta correção
  (o finding F1 anterior só rodou `--lib`, por isso não os viu).

### Phase 1, Slice 3 — HiComplexity qualifier + RL injection reward (DONE — 2026-05-16)

**Part 1** — `EnrichmentData::cognitive_score: Option<f32>` adicionado a `cli_suggester.rs`.
Populado via `rt.ctx.knowledge.get_cognitive_enrichment(&rel)` (field 0, cast f64→f32).
Fail-open: qualquer erro → `None`.

**Part 2** — `ContextQualifier::HiComplexity` adicionado a `action_signature.rs` entre
`HiBlast` e `GotchaActive`. Threshold: `cognitive_score > 0.7` (strict). `as_str()` →
`"hi-complexity"`. `from_enrichment_with_cognitive` e `from_pre_tool_with_cognitive`
adicionados; `from_enrichment` e `from_pre_tool` delegam com `cognitive_score=None`
(zero breaking changes). `cli_suggester::run()` atualizado para chamar
`from_pre_tool_with_cognitive` passando `suggestion.enrichment.cognitive_score`.

**Part 3** — Flag one-shot `__meta__` / `__action_sig_lesson_injected__` escrito por
`cli_suggester::run()` quando `retrieve_and_render_lessons()` retorna `Some`. Lido e
limpo por `post_tool_rl::run()`: se flag="1" e `!has_error` → `inject_reward(+0.3,
"lesson_injected_and_succeeded")`. Fail-open em todos os paths. Espelha o padrão
existente `__context_injection_file__` em `post_tool_rl.rs:64-70`.

**Tests** — 7 novos testes em `action_signature.rs`: `qualifier_hi_complexity_above_threshold`,
`qualifier_hi_complexity_at_exact_threshold`, `qualifier_hi_blast_outranks_hi_complexity`,
`qualifier_hi_complexity_outranks_gotcha`, `to_key_hi_complexity`,
`from_pre_tool_with_cognitive_fires_hi_complexity`, `hi_complexity_as_str`.

**Gates:** `cargo check -p touring-hooks` exit 0; `cargo test -p touring-hooks --lib`
3283 passed (+6 vs Slice 2 baseline 3277); `cargo check --workspace` exit 0;
clippy 0 errors/warnings. Pré-existing failure: `sandbox_executor::test_new2_cleanup_tee_removes_old_files`
(filesystem race, unrelated).

**Phase 1 COMPLETE** — Slices 1, 2, 3 entregues.

### Phase 1 restante
- ~~**Slice 3** — qualifier `hi-complexity` em `ContextQualifier` quando
  `cognitive_score` for exposto em `EnrichmentData`; RL reward de qualidade de
  injeção (PostToolUse detecta sucesso após injeção de lição).~~ **DONE**

### Phase 2 — CC Transcript Miner (Part 5)

Phase 2 fatiada em 3: 2.1 parser foundation → 2.2 state machine → 2.3 integração.

#### Phase 2, Slice 2.1 — transcript parser foundation (DONE — 2026-05-16)

Fundação de parsing pura, zero I/O, fail-open. Delegado a um touring-engineer.

**Entregue:**
- Novo módulo `crates/touring-server/src/ingest/transcript_miner.rs` —
  `enum TranscriptRole { User, Assistant }`, `enum ContentBlock { ToolUse{id,
  tool_name, input} | ToolResult{tool_use_id, is_error, content_text} | Other }`,
  `struct ParsedTranscriptLine { role, session_id, timestamp, uuid, blocks }`,
  `fn parse_transcript_line(&str) -> Option<ParsedTranscriptLine>` (fail-open:
  JSON inválido / `type` ≠ user/assistant / sem `message.content` array → `None`).
  Helper privado achata `tool_result.content` (String OU array `{type,text}`).
- `SourceType::ConversationTranscript` adicionado a `parser.rs` (`default_tier`→
  "semantic", `entry_type`→"conversation_transcript", `parse_line`→`None` —
  discovery é path-based, não por filename keyword).
- `ingest/mod.rs` re-exporta os 4 símbolos públicos.
- 11 testes unitários (tool_use, tool_result is_error true/false/absent, content
  String vs array, linha malformada, `type:system`, content ausente).

**Gates (validados pelo orquestrador, FASE 6):** `cargo check -p touring-server`
exit 0; 11 testes pass; **0 clippy** nos 3 arquivos da slice (as 46 warnings do
crate são débito pré-existente de `touring-generator`, fora de escopo).

#### Phase 2, Slice 2.2 — error→resolution state machine (DONE — 2026-05-16)

State machine pura que transforma `&[ParsedTranscriptLine]` em pares
erro→resolução. Delegado a um touring-engineer.

**Entregue** — `transcript_miner.rs` estendido com 5 símbolos públicos:
- `struct ErrorResolutionPair { tool_name, failed_input, error_text,
  resolution_input, session_id, timestamp }`.
- `const ERROR_TEXT_MAX = 500` (truncamento char-boundary, sem panic em UTF-8
  multibyte) + `const RESOLUTION_SCAN_WINDOW = 3`.
- `fn extract_error_resolution_pairs(&[ParsedTranscriptLine]) ->
  Vec<ErrorResolutionPair>` — two-pass (build_indices + chain-scan).
- `fn dedup_key(&ErrorResolutionPair) -> String` (formato `"<tool>:<hash8>"`,
  `DefaultHasher`).

**Desvio de algoritmo (deliberado, documentado pelo engineer):** o spec original
previa scan independente por-falha; o teste `resolution_beyond_window` revelou
que falhas no meio de uma cascata achavam o sucesso dentro da própria janela
menor. O engineer pivotou para **chain-scan**: uma cascata contígua de erros =
*uma* lição (primeiro erro → sucesso final), emitida só se `chain_len ≤
RESOLUTION_SCAN_WINDOW`. Semântica de mining superior (uma cascata = um conserto)
— aceito na FASE 6.

**Finding corrigido pelo orquestrador (FASE 6, REGRA #0):** o pivô de algoritmo
deixou 2 campos mortos (`ToolUseEntry.tool_name` — redundante com a chave do map
`uses_by_tool`; `ToolResultEntry.stream_pos` — não usado pelo chain-scan). `cargo
check` (≠ clippy) os pegou como `dead_code`. Removidos (forçar uso seria
artificial). `cargo check` voltou a **0 warnings**.

**Gates:** `cargo check -p touring-server` 0 warnings/errors; **20 testes**
transcript_miner pass (+9 vs Slice 2.1); 0 clippy em `transcript_miner.rs`;
0 unwrap/panic em escopo de produção.

**Finding menor (não-bloqueante):** `parse_transcript_line` CC=16 (threshold 15,
excesso 1) — dispatch de parser inerentemente branchy; clippy `too_many_lines`
(>100) não dispara. Não perseguido.

#### Phase 2, Slice 2.3 — integração (sweep + storage + task tokio) (DONE — 2026-05-16)

A camada que faz o pipeline RODAR. Delegado a um touring-engineer.

**Entregue** — `transcript_miner.rs` + `server/mod.rs`:
- `fn discover_transcript_paths(&Path) -> Vec<PathBuf>` — glob
  `~/.claude/projects/*/*.jsonl`, determinístico, fail-open.
- `struct MinerState` (offsets por-arquivo, padrão `WatcherState`) +
  `struct TranscriptMiner` + `TranscriptMiner::new` + `TranscriptMiner::sweep` —
  sweep incremental por offset: lê só linhas novas, parseia, extrai pares,
  dedup via `MemoryStore::get`, persiste via `MemoryStore::store`.
- `struct MinerSweepStats { files_scanned, lines_read, pairs_mined,
  pairs_persisted, pairs_deduped }`.
- Task tokio em `server/mod.rs` — sweep no startup + a cada 300s; gated por
  `TOURING_TRANSCRIPT_MINER` (default ON, `=0` desabilita); fail-open `debug!`.
- 6 testes novos (discovery, offset incremental, dedup on resweep, E2E
  transcript→pares→`MemoryStore`).

**Correção VGP do engineer (tier):** o doc/prompt pediam `tier="semantic"` — não
existe. `MemoryTier::parse_tier` (`rlm.rs:63`) só aceita
`ephemeral/reflexive/working/session/reference/project/core`; `"semantic"` →
`Err(InvalidTier)` engolido silenciosamente → `pairs_persisted=0`. Trocado para
`"reference"` (knowledge persistente de projeto).

**Finding CRÍTICO de integração corrigido pelo orquestrador (FASE 6 — loop não-fechado):**
o engineer validou o *tier* mas não o *padrão de chave*. O miner escrevia
`outcome:transcript:<tool>:<hash8>`; o leitor da Phase 1
(`cli_suggester::collect_memory_lessons`, `cli_suggester.rs:1101`) consulta
`SQL LIKE 'outcome:<tool_class>:%:failure'`. Segmento 2 (`transcript` ≠
tool_class) e ausência do sufixo `:failure` → **o reader nunca selecionaria as
lições mineradas** — Phase 2 mineraria mas nunca injetaria. Fix (9 edits, 2
crates):
- `classify_tool_class` em `action_signature.rs` promovido a `pub` — única
  fonte de verdade compartilhada entre writer e reader.
- Novo helper `lesson_memory_key()` em `transcript_miner.rs`: a chave agora é
  `outcome:<tool_class>:transcript-<hash8>:failure`, que casa o `LIKE` do
  reader. `<tool_class>` vem do `classify_tool_class` compartilhado → writer e
  reader garantidamente alinhados.
- Teste de regressão `test_lesson_memory_key_honors_reader_contract` (6 tools)
  prova o contrato.

**Gates (FASE 6, orquestrador):** `cargo check -p touring-server` 0
warnings/errors; **27 testes** transcript_miner; **749/749** lib regression;
0 clippy em `transcript_miner.rs` + `action_signature.rs`; DB-path verificado
(`server/mod.rs:270` → `TouringConfig::memory_db_canonical` = mesmo DB do
reader). Loop Phase 2 ↔ Phase 1 **fechado e verificado**.

### Phase 2 COMPLETE — Slices 2.1, 2.2, 2.3 entregues. Pipeline transcript→lição→injeção operacional.

### Phase 3 — cobertura `Task`/`WebFetch` (DONE — 2026-05-16)

A última fase: estender o hook `cli-suggest` para além das 6 tools iniciais.

**Discovery (re-escopo):** `action_signature.rs` **já** tratava `task`/`web` —
`classify_tool_class` mapeia `Task`/`TodoWrite`→`task`, `WebFetch`/`WebSearch`→
`web`; `classify_intent_class:298` já tem o arm `task|web|mcp`. O único bloqueio
era `cli_suggester::classify()` (`:282` `_ => None`) → `run()` retornava `"{}"`
para esses tools. Phase 3 ficou menor que o estimado.

**Entregue:**
- `cli_suggester.rs` — `classify_task` (cluster `agent-delegation`) +
  `classify_webfetch` (cluster `web-fetch`, trata `WebFetch` + `WebSearch`) +
  `classify()` estendido para 8 arms. Retornar `Some` desbloqueia o caminho de
  `retrieve_and_render_lessons` (Phase 1+2) para os tool_classes `task`/`web`.
  7 testes novos.
- `~/.claude/settings.json` — 3 blocos `PreToolUse` novos (`Task`, `WebFetch`,
  `WebSearch`), cada um disparando `touring-hook cli-suggest`. 9→12 blocos.

**Decisão de design (desvio justificado do `[*]` literal):** o doc original
sugeria `matcher:"*"`. Realização final = blocos por-tool para os tools
*classificados*. Razão: tools sem classifier retornam `"{}"` de qualquer forma
(`classify()` `_ => None`), então `matcher:"*"` só dispararia o hook ~1ms/call
sem valor + arriscaria double-fire com os blocos por-tool existentes. Cobrir
exatamente os 9 tools com classifier é a realização correta e de menor risco.
`action_signature.rs` não foi tocado (já completo).

**Gates (FASE 6, orquestrador):** `cargo check -p touring-hooks` 0 warnings;
43 testes `cli_suggester`; **3291** lib touring-hooks (+7); 0 clippy em
`cli_suggester.rs`; `cargo check --workspace` **0 errors** (warnings residuais
só em `touring-bindings`/`touring-python`, não tocados nesta sessão);
`settings.json` JSON válido.

---

## ESTRATÉGIA COMPLETA — Phases 1+2+3 entregues (2026-05-16)

| Phase | Entrega | Estado |
|---|---|---|
| **1** | ActionSignature + retrieval multi-fonte + ranking + RL injection reward | ✅ 3 slices |
| **2** | CC transcript miner (parser + state machine + sweep + task tokio) | ✅ 3 slices |
| **3** | Classifiers `Task`/`WebFetch` + matchers `settings.json` | ✅ |

O hook PreToolUse agora: (1) identifica a `ActionSignature` da ação, (2)
recupera lições de erros passados — incluindo as mineradas dos transcripts CC —
que casam a signature, (3) injeta a seção top-K no `additionalContext`, (4)
aprende com o resultado via RL reward. Cobertura: Bash/Grep/Glob/Read/Edit/
Write/NotebookEdit/Task/WebFetch/WebSearch.

**DEPLOY (DONE — 2026-05-16):** `update-touring` rebuildou release + reinstalou
symlinks dual-target + reiniciou o daemon (exit 0). Um daemon `(deleted)` —
auto-spawnado por um hook da sessão durante o build de ~minutos, REGRA #2.5/#3
do `touring-rebuild.md` — foi corrigido com `update-touring --no-build`
(re-link + restart). Daemon agora roda `target/release/touring-hook` fresco.

**Verificação E2E (2026-05-16):** `touring-hook cli-suggest` no binário novo —
payload `Task` → `additionalContext` cluster `agent-delegation` (MUST/SHOULD);
payload `WebFetch` → cluster `web-fetch`; tool sem classifier (`TodoWrite`) →
`{}` gracioso. O transcript miner roda no startup do daemon + a cada 300s.
**Estratégia LIVE e verificada — Phases 1+2+3 operacionais em produção.**
