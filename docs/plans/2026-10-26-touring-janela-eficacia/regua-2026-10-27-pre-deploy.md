---
type: Measurement
title: "Régua — ponto 0 (antes do deploy de 27/10 10:29)"
plan_id: 2026-10-26-touring-janela-eficacia
okf_version: 1
timestamp: 2026-10-27
---

# Régua — ponto 0, pré-deploy (S-15, D1a)

**Comando**: `python3 scripts/measure_tool_trio.py --sessions 200 --until 2026-10-27T13:29:00Z --extras --json`
(JSON completo: `regua-2026-10-27-pre-deploy.json`). **Corpus**: as 200 sessões mais
recentes de `-touring`/`-analise`/`-konverter`, registros **antes de 10:29 BRT de
27/10**: o deploy feito pela sessão `touring-a1` (`update-touring`, SEG-3), que
pôs no ar a wave S-\* desta frente. Instrumento: 14 fixtures verdes.

## Deny → próxima ação, por gate (S-13)

| gate | n | recuperou | repetiu | negado de novo |
|---|---:|---:|---:|---:|
| G10 exec-burst | 91 | 97,8% | 0% | 2,2% |
| G11 orçamento de bypass | 50 | 96,0% | 0% | 4,0% |
| G9 | 16 | 87,5% | 0% | 12,5% |
| G7 re-inspeção | 30 | 83,3% | 0% | 16,7% |
| G3 edit-sem-read | 63 | 74,6% | 0% | **25,4%** |
| **G6 redundant-exact-call** | 24 | **12,5%** | **87,5%** | 0% |
| OUTER (efeito) | 13 | 53,8% | 30,8%* | 15,4% |

\* O OUTER **pede** para refazer a mesma edição, então ali a repetição é a
recuperação. Ainda não sei distinguir os 149 denials classificados como "outro"
(cabeçalho fora do formato `[G<n> …]`); o parser do gate é o próximo ajuste.

**Leitura**: a REASON do **G6** não ensina. Depois da negação, o modelo repete a
chamada idêntica em 7 de 8 casos (o mesmo comportamento que esta sessão teve com
o G6 em 26/10). O **G3** é negado de novo em 1 de 4. São os dois primeiros
alvos do S-13, com a rubrica: invariante, próximo comando exato, proibição e
prefixo.

## Tamanho do resultado (S-14)

| classe | n | p50 | p95 | > 10 KB | bytes > 10 KB |
|---|---:|---:|---:|---:|---:|
| **Read** | 860 | 2.331 | 21.309 | 13,6% | **78,9%** |
| Bash:python3 | 2.346 | 624 | 3.846 | 0,9% | 11,7% |
| Bash:sed | 380 | 2.227 | 6.841 | 1,1% | 4,9% |
| Bash:cat-write | 331 | 361 | 4.415 | 0,3% | 5,8% |
| Bash:touring run | 1.278 | 1.872 | 8.557 | 0,5% | 2,0% |
| Bash:grep | 818 | 959 | 4.098 | 0,1% | 1,3% |

**Leitura**: fora do Read, os resultados grandes são raros, e o compactador de
Bash do S-14 tem pouco onde agir. O volume está nas **leituras grandes de
arquivo**. Mexer nelas muda o que o modelo pediu, então é candidato a reabrir o
T-A5, não uma ação aprovada.

## Read superado (S-16)

**16,8%** dos 9,09 MB de resultado de Read são de arquivos que foram editados,
escritos ou relidos **depois**. É o que o mod `session.compact` determinístico do
B3m tiraria na compactação, trocando pelo ponteiro do original.
