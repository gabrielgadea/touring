---
type: ExperimentReport
title: G1 — corpus e rótulos do professor
description: Amostragem estratificada dos dados do próprio Touring e rotulagem por um professor Claude em modo headless, com os números por tarefa e o que eles já dizem sobre o recall e as tags.
plan_id: 2026-09-22-jev-system-one
tags: [experiment, g1, corpus, teacher, labels]
timestamp: 2026-09-22T21:45:00-03:00
okf_version: "0.1"
---

# G1 — corpus e rótulos do professor

Parte do [bundle](/index.md); executa o gate G1 do [plano v2](/strategy-2026-09-22-jev-system-one.md).
Scripts: [g1_sample.py](/experiments/g1-corpus/g1_sample.py) e
[g1_label.py](/experiments/g1-corpus/g1_label.py).

**Onde os dados ficam.** O corpus tem prompts e memórias reais, e este repositório é
público: os arquivos vivem em `~/.claude/touring/g1-corpus/` (fora do repositório) e
aqui entram apenas os agregados. Os scripts são versionados; os dados não.

## Corpus (seed 20260922)

| Conjunto | n | Estratificação |
|---|---|---|
| Prompts humanos | 800 | por projeto: analise 480, touring 229, Work 44, home 45, tmp 2 |
| Memórias | 486 | por classe do prefixo da chave: 9 classes (66 nas maiores, 24 na menor) |
| Pares consulta → memória | 300 | 61 consultas reais; canais: ann 210, tfidf 89, outros 1 |

O filtro de payload automático **lê os prefixos do executor Rust**
(`AUTOMATED_PAYLOAD_PREFIXES`), então o corpus e o hook concordam por construção sobre
o que é um prompt humano — uma fonte só, não duas listas.

Os pares vêm do pipeline real: cada consulta é um prompt humano, e as candidatas são o
que o `touring memory recall` de fato devolveu.

## Rotulagem

Professor: Claude (`claude -p`, modelo sonnet), em lotes, com cache por hash, validação
contra o vocabulário e no máximo 3 tentativas com o erro de parse como correção.

| Tarefa | Rótulos | Lotes falhos | Tentativas | Confiança média |
|---|---|---|---|---|
| prompt_intent | 800/800 | 0 | 78 de 1ª, 2 de 2ª | 0,645 |
| memory_kind | 486/486 | 0 | 59 de 1ª, 2 de 2ª | 0,756 |
| recall_relevance | 300/300 | 0 | 30 de 1ª | 0,826 |

## O que os rótulos já mostram

**1. O recall entrega quatro quintos de ruído.** Só **21,3%** das memórias recuperadas
foram julgadas úteis para a consulta que as trouxe (64 de 300). Por canal: ann 49 de
210 (23,3%), tfidf 15 de 89 (16,9%). É a medida direta do P3, e o custo aparece em todo
prompt: o que não ajuda ocupa o orçamento de injeção.

**2. Um quinto do corpus "humano" era máquina — e isso mediu o G0.** Ao rotular,
descobriu-se que 160 dos 800 prompts (20%) começam com prefixos de payload automático
que a primeira lista do G0 não cobria: 134 deles são `Another Claude session sent a
message`, mais `<bash-input>`, `<bash-stdout>` e `[Cross-session idle notice]`. A lista
do executor passou de 8 para 14 prefixos, medida em vez de adivinhada.

O ponto que isso prova: entre os 160 payloads de máquina, o **professor** — um modelo
forte, não uma heurística — rotulou 115 como `general`, mas **45 como `analysis` ou
`debug`**. Ou seja, 28% receberiam um modo de trabalho mesmo com um classificador bom.
Por isso o G0 é um filtro determinístico e não um modelo melhor: a pergunta não existe.

**3. A distribuição de intents não é a que o classificador atual assume.** Nos 640
prompts humanos: analysis 220 (34%), general 174, code 80, debug 77, plan 50,
refactor 17, creative 13, test 9. O grosso do trabalho é **análise** — a classe que o
keyword classifier mais erra (ele mandou os dois pedidos de pesquisa profunda desta
sessão para GENERAL).

**4. Cada projeto tem um perfil.** No `analise`: general 192 e analysis 150. No
`touring`: analysis 80 e general 60. No `Work`: debug 21 de 44 (contagens antes do
filtro do item 2). Um classificador único com limiar único ignora isso; o aluno pode
receber o projeto como parte do estado.

**5. O rótulo fraco das memórias é mesmo fraco.** O professor concorda com o prefixo da
chave em **65,4%** dos casos. As discordâncias têm padrão: `pattern → gotcha` (21),
`lesson → report` (16), `lesson → gotcha` (16), `lesson → fix` (12). Ou seja, a chave
diz o que a pessoa quis arquivar, não o que o texto é.

**6. Onde o professor duvida.** 270 dos 640 prompts humanos saíram com confiança abaixo de 0,6 —
são os casos que a revisão humana deve olhar primeiro, e os que o aluno deve mandar para
o fallback.

## Próximo passo (gate do G1)

O gate pede concordância professor × humano ≥ 0,85 numa amostra de ~10%. A folha já
está gerada — 158 itens estratificados por faixa de confiança, com metade na faixa
duvidosa — em `~/.claude/touring/g1-corpus/review_sheet.md`, pelo
[g1_review_sheet.py](/experiments/g1-corpus/g1_review_sheet.py):

```bash
# marcar cada item (`=` concorda, ou escreva o rótulo certo) e então:
python3 docs/plans/2026-09-22-jev-system-one/experiments/g1-corpus/g1_review_sheet.py score
```

Essa revisão é do Gabriel: um segundo modelo concordando com o primeiro não é evidência
independente.

## Limites do método

- O professor é um LLM: seus rótulos são alvo suave, não verdade. A revisão humana é o
  que fecha o gate.
- Confiança auto-relatada não é calibrada; serve para priorizar revisão, não para decidir.
- A amostra por projeto reflete o volume de cada um, então `analise` domina o corpus.
- Prompts foram truncados em 700 caracteres, memórias em 900 e candidatas em 600.
