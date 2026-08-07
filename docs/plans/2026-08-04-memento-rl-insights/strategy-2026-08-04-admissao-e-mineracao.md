---
okf_version: "1.0"
type: Strategy
title: "Estratégia de admissão e mineração — o banco de casos já minera sucessos; nós os descartamos na leitura"
description: "Terceira rodada. Investigação do transcript_miner refuta três afirmações minhas da rodada 2: o banco não é um log de erros mas 3.448 pares erro→reparo validados, o critério de admissão já existe, e nada precisa mudar em touring-contracts. Estratégia em 4 movimentos."
tags: [rl, memento, cbr, case-bank, admission-criterion, attribution, utility-problem]
timestamp: "2026-08-04T21:40:00-03:00"
plan_id: 2026-08-04-memento-rl-insights
---

# Estratégia de admissão e mineração

> A rodada 2 concluiu que "minerar sucessos" era a restrição que amarrava tudo,
> bloqueada por uma mudança de contrato. A investigação do `transcript_miner`
> refutou isso. **O problema não é minerar sucessos — é parar de descartar os que
> já foram minerados.**

## 1. Três correções ao que eu afirmei na rodada 2

| Afirmei | O código diz | Evidência |
|---|---|---|
| "o banco é, por construção, um **log de erros**" | é um **log de reparos** | `transcript_miner.rs:463` — `} else { // Success: this is the resolution candidate` |
| "minerar sucessos exige **mudar `touring-contracts`** (não há recall)" | nada precisa mudar; o pareamento acontece no miner, que tem **os dois lados** do transcript | `transcript_miner.rs:437-481` percorre o stream e pareia in-loco |
| "`read ECONNRESET` é ruído mesmo rotulado" | eu olhei só o campo `error`; **o reparo está anexado** e é acionável | valores parseados: `{error, resolution_input, session_id, timestamp, tool}` |

A terceira é a mais instrutiva: julguei a utilidade de um caso lendo metade dele.

## 2. O que o banco realmente contém

`RESOLUTION_SCAN_WINDOW = 3`, documentado no próprio código:

> *"How many subsequent same-tool `ToolUse`s to scan forward when looking for a
> resolution. If no successful result is found within this window the failure is
> silently dropped (**unresolved failures are not actionable lessons**)."*

Isto **é** um critério de admissão por informatividade — exatamente o que propus
construir na rodada 2, já implementado. Uma falha só vira caso se foi **resolvida**.

Medido no banco vivo:

| | |
|---|---|
| entradas `outcome:*` | 3.478 |
| pares `transcript-` (erro→reparo) | **3.448** |
| falhas puras `ceg-` (dry-run bloqueado) | 30 |
| pares que carregam `resolution_input` | **3.448 / 3.448 (100 %)** |

Amostras reais:

```
error: "File content (56869 tokens) exceeds maximum allowed tokens (25000). Use offset…"
  → resolution_input: { file_path: … }              # o Read paginado que funcionou

error: "File has not been read yet. Read it first before writing to it."
  → resolution_input: { content: "#!/usr/bin/env python3…" }   # o Write que passou
```

Cada entrada é um **par contrastivo validado**: o que falhou (com o erro) **e** o
que funcionou. É o formato mais rico do CBR — e é exatamente o que as duas seções
do Memento (`Positive` / `Negative`) querem consumir.

**O banco não é 99,1 % negativo. É 100 % pares contrastivos, rotulados como
negativos por um sufixo de chave.**

## 3. Por que a classe `positive` fica vazia

Três camadas descartam a metade positiva, todas minhas:

1. `lesson_memory_key` grava `:failure` como literal — o sufixo descreve o
   *gatilho* do caso, não o seu *valor*.
2. `case_value` (rodada 1) mapeia `:failure → 0.0` — trata reparo validado como
   fracasso.
3. `partition_cases` (rodada 2) manda os 3.448 para `negative`.

E o `resolution_input` — a metade que importa — nunca é exposto: fica dentro de
um blob JSON no campo `value`, que o recall devolve como texto opaco.

## 4. A estratégia — 4 movimentos, ordenados por (valor ÷ risco)

### M1 · Decompor o par de reparo (nenhum dado novo)

Ler `value` como `{error, resolution_input}` e emitir **duas evidências de uma
entrada**:

- para `positive`: **o reparo** — "quando você encontrar *este erro*, faça *isto*"
- para `negative`: o input que falhou + o erro — "não faça isto"

Isso é literalmente o formato `When: / Did:` do LangMem e as duas seções do
Memento, saindo de uma única linha do banco.

| | |
|---|---|
| **Efeito** | classe `positive` sai de 29 → até 3.477, sem minerar nada |
| **Risco de swamping** | **nenhum** — o cap por classe (4) já existe; muda a classificação, não o volume |
| **Risco real** | o `value` é JSON não-tipado; parse deve ser **fail-open** (não parseou → classe atual) |
| **Custo** | ~40 linhas em `cli/memory.rs` + testes |

### M2 · Separar reparo de falha pura (correção de defeito meu)

Duas populações compartilham o sufixo `:failure` e **não são a mesma coisa**:

- `transcript-` (3.448): reparo validado → carrega positivo
- `ceg-` (30): dry-run bloqueado, sem resolução → negativo genuíno

`case_value` trata as duas igual. Isso é um defeito no que eu entreguei nas
rodadas 1 e 2, não uma melhoria opcional — REGRA #21.

### M3 · O laço de atribuição (o único sinal de utilidade real)

**`access_count` não é utilizável como sinal de utilidade.** Medido:

- é incrementado na **escrita** (`ceg_impls.rs:167`, `INSERT OR REPLACE … +1`)
- e num caminho de **leitura por chave exata** (`rlm.rs:546`, `RlMemory::get`)
- o recall real (FTS5 / LIKE / ANN / TF-IDF) **não incrementa nada**
- as duas populações foram gravadas por caminhos diferentes: curadas média 0,9
  (máx 52), `outcome:*` média 2,7 (máx 1.363)

Ninguém faz `get("outcome:bash:transcript-00021e9b:failure")` 1.363 vezes. Um
contador que soma escritas e lookups-por-chave, e ignora o recall, **não responde
"este caso ajudou?"**.

> Consequência desconfortável: a decisão de 02/08 de filtrar os `outcome:*` foi
> justificada por "as 8 entradas mais recuperadas do store". Se `access_count`
> mede sobretudo re-gravação, essa métrica não media recuperação. Não afirmo que
> a decisão estava errada — afirmo que **a evidência que a sustentou não é
> interpretável**, e que M3 é o que a torna verificável.

**O desenho**: generalizar o `DecisionLedger` da Fase 2. Ele já faz
"registre a escolha → credite o resultado" para braços do bandit; um caso
recuperado é a mesma estrutura.

```
recall(query)  →  ledger.record("case:<key>", served_for=query_sig)
gate passa/falha →  ledger.credit(query_sig)  →  outcome_reward += α·(r − outcome_reward)
```

Isso **é** a Eq. 9 do Memento (episodic control: `Q_EC` = média dos Q históricos
das interações com aquele caso), usando a coluna `outcome_reward` que a Fase 3 já
criou. Fecha o ciclo completo: recuperar → usar → medir → reforçar → recuperar
melhor.

E converte meu ceticismo em medição: em vez de eu argumentar se
`read ECONNRESET` ajuda, o ledger responde.

### M4 · Admissão para o que ainda falta

Quase resolvido. Sobram duas lacunas pequenas:

- **CEG** (30 falhas puras): não têm resolução. Corretas como negativas — nada a
  fazer além de M2 as distinguir.
- **Fecho de fase**: `loop_phase_close.py` tem o veredito de `loop_converged.py`
  (exit 0/1) em mãos e já chama `memory store`. Uma linha `--reward` grava o
  sinal mais forte que o Touring tem. Fora do workspace (`~/.claude/skills/`).

> O Touring tem um reward **melhor que o do paper**: o Memento usa LLM-as-judge
> (`client:565`); aqui o juiz é `cargo test` + `clippy` + as 6 cláusulas de
> convergência.

## 5. O que NÃO fazer

| Tentação | Por quê não |
|---|---|
| Podar por `access_count` (85 % "nunca reusados") | o contador é ininterpretável (§M3) — podar por ele apagaria casos bons |
| Remover o filtro de prefixo agora | ele é o default que o senhor mediu; M1 torna o canal rotulado útil, M3 torna a decisão verificável. Medir antes de mexer |
| Escrever caso a cada execução bem-sucedida | trocaria um swamping por um maior — e é desnecessário, o miner já filtra por resolubilidade |
| Mudar `touring-contracts` | era o bloqueio que eu citei; não existe |

## 6. Sequenciamento e o que falsifica cada passo

| # | Move | Depende de | Falsificado por |
|---|---|---|---|
| M2 | separar reparo/falha | — | os 30 `ceg-` terem resolução também (não têm) |
| M1 | decompor o par | M2 | `resolution_input` não ser acionável na maioria — inspecionar amostra de 30 |
| M3 | atribuição | M1 | reparos servidos não correlacionarem com gates verdes |
| M4 | fecho de fase | — | independente |

**M2 → M1 primeiro**: são o mesmo arquivo, sem dado novo, e transformam o que já
existe. M3 é o investimento estrutural que torna todo o resto mensurável — e o
único caminho para responder se o banco vale o que ocupa.

---

Antecessoras: `/strategy-2026-08-04-memento-rl.md` (formalismo) ·
`/strategy-2026-08-04-memento-rodada-2.md` (rótulo e balanço). Log: `/log.md`.
