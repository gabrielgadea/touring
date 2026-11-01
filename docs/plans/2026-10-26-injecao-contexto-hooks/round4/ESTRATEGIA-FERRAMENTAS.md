---
type: Analysis
title: "Rodada 5 — o canal de ferramentas: o que domina os 67,9% da janela, e as estratégias para otimizá-lo"
description: "A rodada 5 mede o que domina os resultados e chamadas de ferramenta (67,9% da janela) e cruza com as estratégias externas (rtk, prime-agent, harness2, notebooks)."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# Rodada 5 — o canal de ferramentas: o que domina os 67,9% da janela

> ⚠ **CORRIGIDO EM 27/10/2026 — as tabelas de trio abaixo estão RETRATADAS.**
> Eram, em boa parte, artefato do instrumento (classe Bash pelo 1º token, retry
> por alvo, negação contada como erro; o `/tmp/measure_tool_trio.py` sumiu). Os
> números corretos vêm de `scripts/measure_tool_trio.py` (versionado, fixture de 9
> casos). Revisão: `../../2026-10-26-touring-janela-eficacia/REVISAO-2026-10-27.md`.
>
> Também corrigidos aqui: (1) o `cd` de 1,2 MB é prefixo de comando composto e são bytes de **chamada** que nenhum hook recupera; (2) "comprimir em `maybe_capture_and_persist`" só reduz a janela pelo canal `updatedToolOutput`, porque hoje ele soma `additionalContext`; (3) evicção na compactação: o `PreCompact` não tem saída de contexto.
>
> | medida (200 sessões recentes, 27/10) | valor |
> |---|---|
> | chamadas | 12.791 · resultado 23,0 MB |
> | retry (chamada byte-idêntica, janela 8 = predicado do G6) | **1,5%** |
> | negado por hook | **4,2%** — Edit 9,6% · Write 8,8% · `bash` 10,5% · `cp` 10,4% |
> | Edit retry | **4,3%** (não 61,5%) |
> | `cat` | `cat-write` 330 (heredoc; vazio = sucesso), 94,0% · `cat-read` 78, 94,9% |
> | `grep` | eficácia **91,0%** (exit 1 = sem match, não erro) |
> | `cd`/`export` | prefixos de comando composto (2.246/2.246 `cd`) — não são classe |
> | `ctx_execute` 95,1% | artefato: o alvo (`code`) ficou fora do `cmd_norm` |
> | maior canal de resultado | **Read 39,6%** (9,1 MB; 4,07 MB só texto) · `touring run` 15,4% · `python3` 12,0% |



> **Rodada 5** — otimização do canal de ferramentas. Complementa as rodadas 1-4
> (a injeção de contexto dos hooks). O foco agora é o **outro lado** da janela:
> os **resultados de ferramenta** (43,2%) e as **chamadas de ferramenta** (24,7%).
>
> Corpus: **118 sessões** com conteúdo (das 200 mais recentes de
> `-touring`, `-analise`, `-konverter`).

---

## 1. O que domina o canal de ferramentas

### 1.1 As chamadas (24,7% da janela)

| ferramenta | bytes de input | % |
|---|---:|---:|
| **Bash** | 4.679.654 | **28,9%** |
| **Write** | 3.609.408 | 22,3% |
| **Edit** | 2.713.188 | 16,8% |
| **SendMessage** | 1.042.456 | 6,4% |
| Agent | 320.847 | 2,0% |
| `touring_ctx_execute` | 164.196 | 1,0% |
| Read | 118.263 | 0,7% |
| outros | 2.731.088 | 16,9% |

**O Bash é o maior consumidor de chamadas** — e dentro dele:

| classe de comando | bytes | o que é |
|---|---:|---|
| `cd` | 1.220.290 | mudança de diretório — **o modelo repete o caminho inteiro** |
| `export` | 485.169 | variáveis de ambiente |
| `touring` | 476.537 | chamadas ao Touring |
| `TOURING_CODE_MOD` | 315.974 | o prefixo do code mode |
| `python3` | 230.718 | inline |
| `cat` | 180.963 | leitura crua |
| `timeout` | 139.912 | o wrapper |
| `git` | 95.085 | git |
| `echo` | 78.875 | marcadores |

**Achado**: o modelo **repete o caminho inteiro** (`cd /home/gabrielgadea/projects/touring && …`)
em vez de usar o caminho relativo — e o `cd` é o maior consumidor de bytes do Bash.

### 1.2 Os resultados (43,2% da janela)

| métrica | valor |
|---|---|
| total | **14.660.744 bytes** |
| resultados | **11.020** |
| **média por resultado** | **1.330 bytes** |
| persistidos ("Full output saved to") | **4.040.534 bytes em disco** (19 resultados) |

**Achado**: o volume está na **quantidade**, não no tamanho. A média é pequena
(1.330 bytes) — o problema é que o modelo faz **muitas** chamadas.

---

## 2. As estratégias externas (as fontes)

### 2.1 rtk (rtk-ai/rtk — 84,6k stars)

**O que é**: um proxy CLI em Rust que comprime a saída de comandos dev
(**60-90% menor**).

| comando | o que o rtk faz |
|---|---|
| `git status` | formato compacto, agrupado por estado |
| `git diff` | contexto reduzido, headers removidos |
| `git log` | só hash, autor e assunto |
| `git add/commit/push` | confirmação em vez de progresso |
| `cargo test` / `npm test` | **só falhas**; testes passando viram **contagem** |
| `pytest` | **só falhas**; traceback aparado |
| `go test` | NDJSON parseado, **só falhas** |

**A regra**: **o que passa não é contexto — é o que falha**. O rtk mede: *"the
compression engine"* é o componente que mais pesa (o `rtk-oss` não tem ele).

### 2.2 prime-agent (PrimeIntellect-ai/prime-agent)

**O que é**: um harness RLM (Recursive Language Model) — **contexto como
variáveis**, ferramentas como **código**, REPL Python persistente.

| estratégia | o que faz |
|---|---|
| **prompt-as-a-variable** | o contexto é uma variável, não um texto |
| **programmatic tool calling** | as ferramentas são chamadas por código, não por texto |
| **REPL Python persistente** | o estado sobrevive entre turnos |
| **compactação automática** | o que já serviu sai |

### 2.3 harness2 (já medido na rodada 4)

- `context.py:25-27`: **evidência como arquivos, prompt como ponteiro** — "nothing
  is truncated to fit a prompt".
- `spec.py:163-167`: **truncamento que fabrica defeito é pior que ausência** —
  piso duro de 10.000 chars.
- `prompts.py:254`: *"if you add always-on text, cut something else"*.

### 2.4 Os notebooks (já medidos na rodada 4)

- **context-mode** (`mksglu/context-mode`): hook programático **~98% de contexto
  economizado** vs arquivo-de-instrução ~60%.
- **xerj** (`xerj-org/xerj`): recuperar sob demanda leva a **2,7× menos tokens**
  que grep.
- **Trail-Code** (`trailhq/Trail-Code`): pre-injetar o mapa dá **−32% custo,
  −60% latência, correção igual (93%)**.
- **fast-jev-compaction**: **evicção, não resumo** — o que já cumpriu o papel sai.

---

## 3. O que o Touring **já tem** (e o que falta)

### 3.1 O que já existe

| o que | onde | o que faz |
|---|---|---|
| **captura de saída grande** | `crates/touring-hook-handlers/src/hooks/post_bash.rs:188,516` | `maybe_capture_and_persist` — captura e persiste |
| **dedup de injeção** | `crates/touring-hooks-shared/src/injection_dedup.rs` | deduplica o contexto injetado |
| **o teto de turno** | `crates/touring-hooks-shared/src/turn_budget.rs` | `directives_only` — só as diretivas |
| **o code mode** | `touring run` | o sandbox que executa o programa |

### 3.2 O que falta (as oportunidades)

| oportunidade | o que falta | o tamanho |
|---|---|---|
| **comprimir os resultados de ferramenta** | o `maybe_capture_and_persist` **captura**, mas não **comprime** — o modelo recebe o resultado inteiro | 14,7 MB de resultados |
| **o `cd` repetido** | o modelo repete o caminho inteiro — o harness pode **persistir o cwd** e o modelo usar o caminho relativo | 1,2 MB de `cd` |
| **o "só falhas"** | o rtk mede: `cargo test`/`pytest` viram **só falhas** — o Touring não faz isso | ? |
| **a evicção** | o fast-jev-compaction evita o que já cumpriu o papel — o Touring não tem isso | ? |

---

## 4. A estratégia (o que fazer)

### 4.1 O que mede o que importa

O rtk mede: **o que passa não é contexto — é o que falha**. O Touring mede: **o
que é injetado, não o que é usado**. A rodada 5 mede: **o que é chamado, não o
que é lido**.

### 4.2 A ordem de implementação

| # | ação | o que mede | o que custa |
|---|---|---|---|
| 1 | **comprimir os resultados** — o `maybe_capture_and_persist` passa a **comprimir** (só falhas, só o delta) | o rtk mede 60-90% de redução | o modelo recebe menos |
| 2 | **persistir o cwd** — o modelo usa o caminho relativo | o `cd` é 1,2 MB de bytes | o modelo repete menos |
| 3 | **o "só falhas"** — `cargo test`/`pytest` viram contagem | o rtk mede isso | o modelo vê menos |
| 4 | **a evicção** — o que já cumpriu o papel sai | o fast-jev mede isso | o contexto fica menor |

### 4.3 O que **não** fazer

- **Não comprima o que o modelo precisa** — o `pre-edit` (o fato) é o que funciona
  (75,6% de conformidade); comprimir o fato é pior que não injetar.
- **Não evicte o que o modelo vai usar** — o fast-jev mede: **evicção, não resumo**.
- **Não meça o que não é medido** — o rtk mede a redução; o Touring mede a
  conformidade; os dois são necessários.

---

## 5. O que a rodada 5 mediu

| o que | o número | onde |
|---|---|---|
| o canal de ferramentas | **67,9%** da janela (43,2% resultados + 24,7% chamadas) | a rodada 4 |
| as chamadas | **Bash domina** (28,9%) — e o `cd` é o maior consumidor (1,2 MB) | este documento |
| os resultados | **a média é 1.330 bytes** — o volume está na **quantidade** | este documento |
| o que o Touring já tem | **captura, dedup, teto de turno, code mode** | `post_bash.rs`, `injection_dedup.rs`, `turn_budget.rs` |
| o que falta | **compressão, cwd persistente, "só falhas", evicção** | as estratégias externas |

---

## 6. O que a rodada 5 recomenda

1. **Comprimir os resultados de ferramenta** — o `maybe_capture_and_persist`
   passa a **comprimir**: só falhas, só o delta. O rtk mede 60-90% de redução.
2. **Persistir o cwd** — o modelo usa o caminho relativo. O `cd` é 1,2 MB de
   bytes.
3. **O "só falhas"** — `cargo test`/`pytest` viram contagem. O rtk mede isso.
4. **A evicção** — o que já cumpriu o papel sai. O fast-jev mede isso.

---

## 7. Os limites honestos desta rodada

1. **Corpus = 118 sessões** — não é o universo de uso do Touring.
2. **A média de 1.330 bytes** é a média — o problema é a **quantidade**, não o
   tamanho.
3. **O rtk mede a redução** — o Touring mede a conformidade; os dois são
   necessários.
4. **O prime-agent é um harness** — a estratégia RLM é **contexto como variáveis**,
   não texto — mas o Touring já tem isso (o code mode).

---

## 8. Reprodução

```bash
# a medição
python3 /tmp/measure_tool_channel.py

# as fontes
curl -sL https://raw.githubusercontent.com/rtk-ai/rtk/master/README.md
curl -sL https://raw.githubusercontent.com/PrimeIntellect-ai/prime-agent/main/README.md
```

---

_Rodada 5 — o canal de ferramentas. O que domina, e as estratégias para otimizá-lo._