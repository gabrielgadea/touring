---
okf_version: "1.0"
type: Strategy
title: "Code mode antes da declaração — apagar o quarto onde a inferência prematura mora"
description: "Medição da sessão 24/08 (4% de adoção, 33 rajadas, 135 round-trips colapsáveis) e seis estratégias cujo critério de projeto é reduzir estados de conhecimento parcial, não pedir melhor comportamento dentro deles"
tags: [code-mode, afordancia, medicao, antipadrao, protocolo]
timestamp: 2026-08-24T18:25:00-03:00
plan_id: 2026-08-24-code-mode-antecipa
scope: /home/gabrielgadea/projects/touring
---

# Code mode antes da declaração

## A medição (fatos, 1.0)

Sessão `2f2d716c`, medida pelo próprio transcript via `touring run`:

| métrica | valor |
| --- | --- |
| chamadas Bash | **674** |
| que usam `touring run` | **30 (4%)** |
| inspeção pura (grep/sed/cat/ls/find/sqlite3) | 270 (40%) |
| rajadas de ≥3 inspeções consecutivas | **33** (maior: **14**) |
| round-trips colapsáveis em 1 chamada | **135** |
| exit code lido após pipe sem `pipefail` | **3** |

4% numa sessão em que eu **construí e consertei o code mode**. E o nudge
`code-mode-loop conf=0.95` disparou dezenas de vezes — eu o li e segui adiante.
É a reprodução exata de `protocol-adherence-diagnosis`: MUST com confiança 0.95
ignorado na própria sessão que o emitiu.

## O mecanismo (por que code mode ANTECIPA, e não apenas acelera)

Uma rajada de N inspeções cria **N−1 estados de conhecimento parcial**. A rajada
de 14 desta sessão teve 13 momentos em que eu segurava evidência incompleta —
e foi de dentro de um desses momentos que saiu cada declaração prematura do dia:

- o comentário do `arm_marker` ("sandboxar quebraria a escrita") — nunca executado, **falso**;
- o `mem-vazio` classificado como "decisão de arquitetura entre dois candidatos" — ler o código mostrou **um outlier**;
- `EXIT=0` reportado três vezes a partir do `tail`, não do comando.

Um programa só produz **zero** estados intermediários.

> **Code mode não me disciplina na hora de falar. Ele apaga o quarto em que a
> fala prematura pode acontecer.**

Daí o critério que ordena tudo abaixo:

> **A estratégia REDUZ estados parciais, ou pede melhor comportamento dentro
> deles?** Só a primeira funciona — medido duas vezes, em mim, hoje.

Corolário duro: **nenhum nudge novo.** A camada de anúncio já foi medida e
reprovada. O que falta não é aviso, é `U(a)`.

## As seis estratégias

### E1 — A pergunta define a unidade, não o arquivo · *teeth no contador que já existe*

**Regra**: pergunta que atravessa ≥3 arquivos/fatos → a PRIMEIRA ação é um
programa. Não "considere usar": precondição.

O contador de rajada **já existe** (`CODE_MODE_WINDOW_SECS`, `scan_class_key`,
dispara na 3ª busca atômica da janela). Não falta detecção — falta consequência.
A escalada honesta: da 4ª inspeção da mesma classe na janela, o hook **nega** em
vez de sugerir, entregando o `touring run` já derivado como remédio.

É o único ponto onde o hook pode mudar `U(a)` em vez de conversar.
Risco: negar exploração legítima. Mitigação: só a 4ª+ da MESMA classe, sempre
com o comando pronto, bypass documentado. **Custo: baixo** (o contador existe).

### E2 — O instrumento antes da leitura · *lint determinístico*

`EXIT=$?` depois de um pipeline sem `set -o pipefail` lê o exit do `tail`.
Três vezes hoje; três vezes eu reportei "exit 0" de um comando que não medi.

Lint de string, PreToolUse, zero LLM: `AntipatternKind::ExitCodeThroughPipe`,
na enum que já existe (`workflow/baseline.rs`). Bloqueia a medição malformada
**antes** que a saída dela vire afirmação.

**Custo: baixo. Confiança: a mais alta do conjunto.** É por onde começar.

### E3 — A contrafactual precisa de um run_id · *alta precisão por construção*

Afirmação sobre o que ACONTECERIA ("seria", "quebraria", "faria", "impediria")
é a única classe **infalsificável por construção** — e foi exatamente onde eu
errei no `arm_marker`.

PreToolUse em Write/Edit varrendo as linhas ADICIONADAS por modais
contrafactuais em comentário, exigindo citação de `run_id`. Preciso porque
contrafactual é raro: pouco ruído, e mira o pior erro do dia.

**Custo: médio.**

### E4 — Veredito sem evidência é recusado pelo lint · *estrutural, no grafo*

Em `adw.py`: um nó que emite `VERDICT=`/`METRIC=` DEVE ler de ≥1 nó `code`
(`{{nodes.X.summary}}`). Espelho invertido de `_lint_fake_waiting`, e
`node_data_reads` já computa o grafo de dados.

Torna a medição **upstream por construção** em todo fluxo publicado — o
executor chama o medidor antes do sintetizador porque o grafo não compila de
outro jeito.

**Custo: baixo-médio.**

### E5 — A colheita torna a segunda medição mais barata que a asserção · *já existe*

`touring run --harvest <slug>` persiste o programa como `#kind:snippet` e o
matricula na escada de confiança. É a **única** estratégia que compõe: cada
medição colhida baixa `C(tokens)` da próxima, até medir custar menos que
inventar.

Exercitado agora: `snippet:auditoria-sessao-tooluse`.
E `touring run --file` **já existia** — a fricção de dois passos (heredoc para
o scratchpad, depois `cat`) que eu inventei hoje não estava lá. Parte do 4% não
é afordância faltando; é afordância existente e não lida.

**Custo: zero.** É uso, não construção.

### E6 — O Stop hook é o único executor que vê a minha prosa

E1–E4 pegam o artefato; nenhum deles impede uma frase. A superfície da mensagem
não tem executor — **exceto** o Stop hook, que hoje me pegou: eu rodei o juiz,
declarei convergência e nunca entreguei o veredito ao marcador.

Extensão barata: turno com ≥20 chamadas Bash e **zero** `touring run` recebe um
bloqueio com o diagnóstico da rajada. Grosseiro, mas é o único lugar onde a
declaração e um executor se encontram.

**Custo: baixo.**

## Ordem recomendada

**E2** (determinístico, 3 instâncias medidas) → **E5** (custo zero, compõe) →
**E4** (estrutural, fecha os fluxos) → **E1** (teeth; exige cuidado com
falso-bloqueio) → **E6** → **E3**.

## Observação lateral, não verificada (< 0.7)

O `harvest_hint` promete recuperar o snippet por
`touring memory query '#kind:snippet #lang:python'`. Essa query **não** devolve
o snippet recém-criado; `#kind:snippet` sozinha devolve. Não isolei se falta a
tag `#lang:python` ou se o problema é a query conjuntiva. Mesma família do
`mem-vazio`: o anúncio documenta um endereço de recuperação diferente do que a
escrita usou.
