---
type: HarnessScan
title: "Harness scan r3 — ground truth VGP: onde o Jev/Laya potencializa o Touring (A1Relevance, A2Outcome)"
description: "Apêndice do plano r3. Toda afirmação com file:linha verificado por leitura ou execução, 02/10/2026. 6 seções: caminho de emissão, caminho de desfecho, esquemas de dados, infra r2 reutilizável, aluno local, privacidade."
plan_id: 2026-10-02-jev-harness-r3
date: 2026-10-02
okf_version: "0.1"
tags: [harness-scan, vgp, jev, laya, ground-truth, r3]
---

# Harness scan t03 — ground truth VGP (r3, 02/10/2026)

100% leitura. Nenhum edit fora deste arquivo. Sem cargo, sem commit, sem `crates/**`.
Precedente de estilo: `docs/plans/2026-09-30-jev-harness-potencializacao/harness-scan-t02.md`.

## 1. CAMINHO DE EMISSÃO (A1 — gate de relevância)

### 1.1 Onde cada família nasce

| família | função que monta o texto | hook que entrega |
|---|---|---|
| `touring-suggest` | `crates/touring-cli/src/cli_suggester.rs:2279` (template `"{1a linha}"`); bloco real medido em `crates/touring-cli/src/cli_suggester.rs:6311` | `cli-suggest` (28.490/7d) |
| `code-mode-deny` | `crates/touring-cli/src/cli_suggester.rs:5174` `code_mode_gates` | `pre-bash` (16.186/7d) |
| `loop-outer` | ADW/flow, fora do suggester | `instructions-loaded` (5.211/7d) |
| `past-lessons` | `crates/touring-cli/src/cli_suggester.rs:103` (doc: era 35,1% da injeção, 948 KB) | `pre-bash` |
| `session-start` | `crates/touring-hook-handlers/src/hooks/instructions_loaded.rs:56` (`collect_all_knowledge_summary_parts`) | `SessionStart:startup` |
| `post-tool-echo` | `crates/touring-hook-runtime/src/hook_trace.rs:94` `set_route` | `post-tool-rl` (25.323/7d) |
| **`other`** | **catch-all — `crates/touring-cli/src/cli/context_budget.rs:184` `return "other"`** | todos |

**`other` não é uma família: é o resto.** A taxonomia é UMA fonte
(`crates/touring-cli/src/cli/context_budget.rs:141` `injection_family`, tabela em `:142-178`) e o catch-all está em
`:184`. Quem produz seus maiores blocos, pelo comentário do próprio código
(`crates/touring-cli/src/cli/context_budget.rs:192`): `[gabrielgadea]` 384×, `[generic]` 188×,
`post-tool-batch: batch of 1 tools …` 422×. Medido nos 3 transcripts mais recentes:
`skill_listing` (89.976 B, 1ª linha `- academic-research-writer`) e o bloco
`=== REMEMBER ===` (21.894 B) — ou seja, **`other` é majoritariamente listagem de skills e
buffer de memória, não nudge de decisão**.

### 1.2 Onde o `additionalContext` é composto (o ponto de gate)

`crates/touring-hook-runtime/src/hook_runtime.rs:163` cria
`{"additionalContext": truncated}`; a escrita definitiva em `:184` e `:204`; a variante com
`updatedInput` em `:229`. A fachada `crates/touring-hook-runtime/src/hook_runtime_ext.rs:484` é a segunda montagem. Handler
específico: `prompt_enhance.rs:1044-1070`.

**Um gate antes da injeção cabe em `hook_runtime.rs:163-184`** — é o único ponto por onde
passa toda injeção de `PreToolUse`.

### 1.3 Onde o KPI `context_budget` é calculado

`crates/touring-cli/src/cli/context_budget.rs` (1.130+ linhas, o arquivo inteiro do KPI).
- `injection_family` `:141` · `count_propositions` `:196`
- vereditos e `failing` `:717-728`
- JSON emitido `:730-760` · entrada `pub fn context_budget(project_root)` `:780`
- tetos em `touring_foundation::cila`: `INJECTION_CEIL_BYTES_PER_TURN` (`:748`),
  `INJECTION_DUPLICATE_RATIO_CEIL` (`:750`), `INJECTION_FOLLOW_RATIO_FLOOR` (`:753`)

**Serão as 3 métricas do plano A1.**

## 2. CAMINHO DE DESFECHO (A2 — "a oferta foi seguida?")

### 2.1 O fluxo, ponta a ponta

| etapa | file:linha | o que decide |
|---|---|---|
| **emite a oferta** | `crates/touring-cli/src/cli_suggester.rs:5670` → `record_route_offer` `:4533` | guarda `(mode, offered_secs)` em `route_offers()` `:4536`; `bump_arm(…Offered)` `:4544` |
| **consome/desconta** | `crates/touring-cli/src/cli_suggester.rs:4377` `take_route_offer` | `get` + `invalidate` — **uma oferta por vez, consumida na leitura** `:4380` |
| **classifica o desfecho** | `crates/touring-cli/src/cli_suggester.rs:4482` `classify_route_outcome` | `cmd.contains("touring run ")` \| `contains("touring exec ")` → `Some(1.0)`; se `ProgramShape::Trivial` → `Some(0.5)` `:4500-4503`; fora disso, `None` |
| **recompensa** | `crates/touring-cli/src/cli_suggester.rs:4880` `claim_route_reward` | chama o classificador `:4885`, consome a oferta `:4886`, `bump_arm(…Followed)` `:4888`, `…Economical` se `value ≥ 1.0` `:4890-4891` |
| **consome no hook** | `hook_handlers/hooks/post_tool_rl.rs:297-315` | `cmd` vem de `/tool_input/command` `:293-296`; `inject_reward("code_mode:{mode}", value, "t3_route_followed"\|"t3_route_bypassed")` `:301-309` |
| **crédito de bypass** | `crates/touring-cli/src/cli_suggester.rs:3814` `credit_sanctioned_route` | invalida o ledger de bypass `:3815` |

**O predicado é `cmd.contains(...)` — duas strings.** `TOURING_CODE_MODE=native`,
`TOURING_GATE_OK=1` e `TOURING_CODE_GATES_DISABLED=1` são reconhecidos e **não desligam
nada** (`:4483-4489`) — estão na lista `BYPASS` para que o classificador os pontue como
recusa, não como sucesso.

### 2.2 Onde os contadores são incrementados

`bump_arm` — `crates/touring-cli/src/cli_suggester.rs:4755`, **escritor único** (durável + volátil na mesma
passada, `:4769-4777`): `record_code_mode_arm_offered` `:4772`,
`record_code_mode_arm_followed` `:4776`. Eixo `ArmAxis` `:4746-4753` (`Offered`/
`Followed`/`Economical`). Arquivo durável: `.claude/touring/code_mode_arm.json`, caminho em
`crates/touring-cli/src/cli_suggester.rs:4578` e lido por `kpi/code_mode.rs:22,49`.

`pillar_induction_{emitted,followed}` é **check**, não KPI: sai de
`gate-metrics` e aparece em `touring kpi -j → checks[]` como
*"Coupling pillar-induction"* — **não existe** como chave de topo do JSON.

### 2.3 Testes que cobrem o fluxo

`cli_suggester_tests.rs:3959-4001` — `claim_route_reward` retorna `None` sem oferta (`:3972`),
`Some` com `touring run --lang bash --code 'ls'` (`:3975`), `None` com `touring run --code 'x'`
sem espaço (`:3983`); `take_route_offer` consome uma vez (`:3999-4001`).

### 2.4 Onde roda, e cabe um worker?

`record_route_offer` roda no **`PreToolUse`**, `claim_route_reward` no **`PostToolUse`**
(`post_tool_rl.rs`) — o docstring em `:288` chama este "o primeiro hook depois do deny com
runtime mutável". Medido (M8/E1a, 7 dias): `cli-suggest` p50 **3 ms** / p95 **396 ms**,
100% rota `daemon`.

**O caminho quente é síncrono e sem canal** — `post_tool_rl.rs` não cria `thread::spawn`,
`spawn_blocking`, `mpsc` nem actor (grep: só um `SessionBus` de evidência em `:357`). Logo
**cabe** um `mpsc::bounded` + worker para empurrar o evento de desfecho: o `post_tool_rl`
já é fail-open por desenho (`:290-291`), e o desfecho não altera a decisão. O que **não**
cabe é consultar o aluno no caminho síncrono — o cliente HTTP tem timeout de **200 ms**
(`prompt_enhance.rs:400-406`), e a latência p95 do suggester já é 396 ms.

## 3. ESQUEMAS DE DADOS

| arquivo | bytes | janela | campos |
|---|---|---|---|
| `.claude/touring/code_mode_arm.json` | **156 B** | contador cumulativo | `both`/`code`/`native` × `{offered, followed, economical}`. Amostra: `{"code":{"economical":179,"followed":208,"offered":475}}` |
| `~/.claude/touring/run_journal.jsonl` | 23.374 L | desde 23/08 | `ts, language, exit_code, failure_kind, duration_ms, bytes_elided, code_hash_stdout_bytes` |
| `~/.claude/touring/logs/jev_journal.jsonl` | 1.523 L | desde 30/09 | `ts, model, question_ids, route_taken, cache_key, latency_ms, input_tokens, model_reported, answers_digest, fallback_reason` |
| `~/.claude/touring/hook_trace.jsonl` | 465.201 L | 7+ dias | `ts_ms, hook, route, exit_reason, elapsed_ms, session_id, pid, ppid, stdin_bytes, stdin_state` |

**Amostra com texto, truncada a 40 chars** (`run_journal`, 1ª linha boa):
`{"ts":"1787540216","language":"bash","exit_code":0,"code_hash_stdout_bytes":102400}`
— **sem texto: o código é gravado por hash**, por desenho.
`hook_trace` (truncada): `{"ts_ms":"1788315654074","hook":"cli-suggest","route":"daemon","exit_reason":"daemon-json","session_id":"prova-reset"}`.

### 3.1 Dá para juntar oferta → desfecho?

**Sim, e em escala.** `hook_trace` carrega `session_id` + `ts_ms`, e a oferta é um
`pre-bash` enquanto o desfecho é um `post-tool-rl`. Medido: **592 sessões distintas** com
ambos os eventos, 51.243 `pre-bash` e 81.296 `post-tool-rl`. A ordem se dá pelo `ts_ms`.

**n reconstruível por família** (7 dias, par por sessão): `code-mode-deny` até 51.243
pares · `post-tool-echo` até 81.296 · `touring-suggest` — **não** é
reconstruível do `hook_trace` (o suggester emite a oferta, mas o texto do nudge não
carimba o hook que o entregou; precisa dos transcripts). **Estimativa conservadora:
50k+ pares rotulados já hoje, sem sombra.**

## 4. INFRA DO R2 REUTILIZÁVEL

| peça | file:linha | nota |
|---|---|---|
| `parse_student_answer_raw` | `crates/touring-hook-runtime/src/prompt_enhance.rs:460` | já é "shadow sem τ" |
| **`journal_student_shadow`** | `prompt_enhance.rs:486-496` | grava `{ts_ms, intent, confidence, source:"laya_shadow", path:"shadow", latency_ms, skipped_automated}` — **`intent` é enum, nunca o prompt** |
| `student_classify` | `crates/touring-hook-runtime/src/prompt_enhance.rs:503` | `Option<(Intent, f64)>` — `None` em qualquer falha |
| `read_window` / `within_window` | `crates/touring-cli/src/cli/kpi/decision.rs:66` / `:96` | janela de 7 dias |
| `JevTally` | `crates/touring-cli/src/cli/kpi/decision.rs:429-448` | **aqui entra `use_case`** |

**Onde um `use_case` entra com menor blast: `JevTally.by_question`**
(`crates/touring-cli/src/cli/kpi/decision.rs:440`) é o precedente exato — o próprio docstring (`:436-439`) registra o
achado que o motivou: *"o journal mistura cinco chamadas diferentes e as latências as tratam
todas como se fossem uma. Um p99 agregado de seis perguntas não diz nada sobre nenhuma."*
`use_case` é a mesma falha uma camada acima (a pergunta vs. **para que**). `blast_radius =
1`.

**Blast radius medido** (`touring ast blast <file>`):

| arquivo | blast_radius | observação |
|---|---|---|
| `prompt_enhance.rs` | **48** | o caro — já tem código em edição por outra sessão |
| `cli_suggester.rs` | **22** | o segundo mais caro |
| `kpi.rs` | 4 | |
| `kpi/decision.rs` | **1** | barato: `JevTally` e os readers vivem aqui |
| `scripts/laya_student_server.py` | — | Python, sem blast no AST Rust |

**Recomendação de ordem**: começar por `decision.rs` (1) para o schema, e tratar
`cli_suggester.rs`/`prompt_enhance.rs` como **mudança L3** com plano próprio.

## 5. ALUNO LOCAL

| item | valor medido |
|---|---|
| wire | `POST /v1/systemone {state, model, questions}` (`scripts/laya_student_server.py:5`) |
| porta | `LAYA_STUDENT_PORT`, default **8971** (`scripts/laya_student_server.py:24`) |
| modelo | `laya-w42-intent`, dir `~/.claude/touring/laya-w42-intent/output` (`scripts/laya_student_server.py:21-22`) |
| transporte | HTTP cru em loopback, **sem TLS** (`prompt_enhance.rs:311,333`) — validado por `is_loopback_host` `:413` |
| **timeout** | **200 ms** connect/read/write (`prompt_enhance.rs:400-406`) |
| serviço | unit `laya-student` **inativa**; porta 8971 não escutando |
| **VRAM agora** | **8.188 MiB total · 1.938 usados · 5.897 livres · 0% util** |

**Treinar um 2º especialista**: `w42_train.py:5-6` declara a receita de 8 GB —
`micro_batch=1, grad_accum=64 (efetivo 64), gradient checkpointing, 4 épocas`;
`:51` troca `AdamW` por `PagedAdamW8bit` (o OOM R15: fp32 = 5,2 GB de estado e o cartão
tinha ~2 GB de inquilinos). Último treino registrado: **800/800 itens · 4 épocas · ~12 min na
RTX 4060** (`FINAL-REPORT.md:118`).

**Cabe um 2º especialista na RTX 4060?** **Cabe em VRAM, com uma condição.** 5.897 MiB
livres contra os ~2 GB de inquilinos que já discomfort o treino anterior: um especialista
estreito cabe se **não** coexistir com outro em inferences. Como o servidor é
`ThreadingHTTPServer` (`:17`) com o modelo único em `MODEL_DIR` (`:21`), a 2ª cabeça exige
outro processo + outra porta — o que é compatível com o `host:porta` já parametrizado.

## 6. PRIVACIDADE

**O que hoje NÃO sai texto, por desenho (verificado, não suposto):**

| arquivo | texto do usuário/comando? |
|---|---|
| `run_journal.jsonl` | **NÃO** — `code_hash_stdout_bytes`, sem o código |
| `jev_journal.jsonl` | **NÃO** — `question_ids` (ids), `answers_digest`; `input_tokens` é contagem |
| `code_mode_arm.json` | **NÃO** — só contadores |
| `hook_trace.jsonl` | **NÃO** — `stdin_bytes` é contagem, não conteúdo |
| `intent_journal` (`journal_student_shadow`) | **NÃO** — `intent` é **enum** (`crates/touring-hook-runtime/src/prompt_enhance.rs:490`), `confidence` numérica |

**Onde o texto existe e pode ser capturado**: os **transcripts** em
`~/.claude/projects/…` (comandos e prompts crus), e o `command` que
`post_tool_rl.rs:293-296` lê de `/tool_input/command` — esse é o único lugar no caminho
quente onde o comando cru está em memória de processo.

| destino | pode receber |
|---|---|
| **aluno LOCAL (Laya)** | ✅ texto cru é aceitável: o modelo roda na máquina, loopback, sem TLS e sem rede. É a única opção que pode ver o comando inteiro. |
| **Jev API (TypeSafe)** | ❌ **só features derivadas** — `question_ids`, `max_prob`, `latency_ms`, o enum de intent. **Nunca** o comando ou o prompt: sairia da máquina e entraria num serviço de terceiro. |

**O gate A1 precisa saber disto**: a features para o gate de relevância pode ser o
*desfecho* (segue/não-segue, `code_mode_arm.json` + `hook_trace`) mais o **hash** da
assinatura de 1ª linha do bloco — **sem o texto**. Isso basta para o gate e mantém o aluno
local fora do caminho síncrono (o timeout de 200 ms e a latência p95 de 396 ms não
permitem).
