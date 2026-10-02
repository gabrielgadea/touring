---
type: Contract
title: "Contrato de eventos do shadow r3 (S-1)"
description: "Três tipos de linha JSONL, arquivos, flag, semântica do ciclo de vida da oferta e regras de privacidade. A fonte executável é scripts/r3_event_schema.py; este documento é o mapa para quem implementa."
plan_id: 2026-10-02-jev-harness-r3
tags: [contrato, shadow, privacidade, r3]
timestamp: 2026-10-02T13:25:00-03:00
okf_version: "0.1"
---

# Contrato de eventos do shadow r3

Fonte executável: [`scripts/r3_event_schema.py`](/scripts/r3_event_schema.py) + [`scripts/r3_use_cases.py`](/scripts/r3_use_cases.py)
(testes: `scripts/test_r3_event_schema.py`, 8 testes, 7 mutantes mortos). Plano: [plan](/plan.md) · critérios: [CRITERIOS-ACEITACAO](/CRITERIOS-ACEITACAO.md).

## 1. Arquivos e flag

| arquivo | quem escreve | como |
|---|---|---|
| `~/.claude/touring/logs/r3_events.jsonl` | **emissor Rust**, dentro do daemon | `append_json_line` (1 `write_all` O_APPEND, atômico); falha de I/O é **engolida** e contada (`dropped`), nunca propaga |
| `~/.claude/touring/logs/r3_shadow.jsonl` | **avaliador Python**, processo à parte | um único escritor; offset do `r3_events.jsonl` guardado em `~/.claude/touring/logs/r3_shadow.offset` |

**Flag:** `TOURING_R3_SHADOW` (`1`/`true` liga) **ou** `[shadow] r3 = true` em `.touring/touring.toml`; precedência
env > toml; **default OFF**. Com a flag OFF: **zero I/O, zero alocação extra** no caminho do hook, e o valor devolvido
é idêntico (teste: `r3_flag_off_does_no_io`).

## 2. As três linhas (esquema FECHADO — chave desconhecida é violação)

**`a2_offer_closed`** — emitida UMA vez, quando a oferta de rota fecha:

```json
{"v":1,"kind":"a2_offer_closed","use_case":"a2_outcome","ts_ms":1790956285165,"session_hash":"9f86d081884c",
 "mode":"code","offered_secs":1790956280,"closed_by":"consumed",
 "cmd_heads":["touring run --lang bash --code 'ls'"],"baseline_value":1.0,"baseline_followed":true}
```

- `closed_by`: `consumed` (o classificador devolveu `Some` e a oferta foi consumida, `cli_suggester.rs:4885-4886`) · `window`
  (3 Bash sem desfecho — **K = 3**, E1.4) · `replaced` (nova oferta na mesma sessão, `record_route_offer :4536`).
- `cmd_heads`: **até 3** comandos Bash que seguiram a oferta, **cada um truncado a 80 chars**.
- `baseline_value`: o `Some(v)` de `classify_route_outcome` que fechou a oferta (`1.0`/`0.5`/`0.0`); `null` se nenhum.
  `baseline_followed` = `baseline_value > 0`. **Isto é o B0 vivo**, gravado pelo emissor sem recomputar nada.

**`a1_block`** — emitida ANTES de escrever o `additionalContext` de uma injeção de **PreToolUse**
(`hook_runtime.rs:163-184`):

```json
{"v":1,"kind":"a1_block","use_case":"a1_relevance","ts_ms":1790956285200,"session_hash":"9f86d081884c",
 "hook":"pre-bash","bytes":912,"first_line_sha":"ab12cd34ef567890",
 "first_line_head":"[TOURING SUGGEST · code-mode-loop · conf=0.95]"}
```

- **O emissor NÃO classifica a família** (a tabela vive em `touring-cli`, `context_budget.rs:141`); o avaliador a porta e prova
  paridade com o KPI em S-3. **Não altera o `additionalContext`** — S4 prova com diff = 0 bytes.
- Escopo online do r3 = PreToolUse (`touring-suggest`, `code-mode-deny`, `past-lessons`). `session-start` e `post-tool-echo`
  são cobertos só pelo replay.

**`shadow_decision`** — escrita pelo avaliador, uma por (evento × braço):

```json
{"v":1,"kind":"shadow_decision","use_case":"a2_outcome","ts_ms":1790956285900,"arm":"B-L",
 "event_ref":{"kind":"a2_offer_closed","ts_ms":1790956285165,"session_hash":"9f86d081884c"},
 "decision":{"followed":true,"p":0.93},"baseline_agrees":true,"latency_ms":31,"model":"laya-multilingual"}
```

## 3. Privacidade (S6 — provada por teste, não prometida)

1. **Nenhuma string > 80 chars em qualquer linha**, em qualquer chave.
2. **Esquema fechado**: um campo novo não pode carregar um prompt às escondidas (chave desconhecida = violação).
3. **Sessão só por `session_hash`** (12 hex de sha256); o id cru nunca é gravado (`hash_session`).
4. **Jev API só recebe features redigidas** (`redact_for_jev`): `use_case, kind, mode, closed_by, n_cmds, first_token,
   baseline_value_bucket, hook, bytes_bucket, family, tool_class` — valores **sem espaço em branco** (não podem ser texto livre).
5. `scan_privacy(linha)` é uma segunda rede, independente do esquema (chaves proibidas: `prompt`, `text`, `command`, `cmd`,
   `content`, `body`, `message`, `code`).
6. O aluno **local** (loopback) pode ver os `cmd_heads` de ≤ 80 chars; o Jev API nunca.

## 4. Obrigações

| quem | obrigação |
|---|---|
| **emissor (S-8, S-9)** | flag OFF ⇒ zero I/O; falha de emissão **nunca** muda o retorno do hook; 1 append por evento; `session_hash` via sha256[:12] do id de sessão |
| **avaliador (S-11)** | offset durável; avaliador parado ⇒ a fila só cresce, o emissor segue; valida cada linha lida com `validate_event` e **descarta com contagem** a inválida; nunca envia texto ao Jev |
| **KPI (S-10)** | `unmeasured` (nunca 0) sem linha; agrupa por `use_case`; **não** lê nem altera o journal do KPI `decision` |
| **extratores (S-2, S-3)** | produzem datasets com os MESMOS campos que o emissor (para o replay medir a mesma coisa que a produção) |
