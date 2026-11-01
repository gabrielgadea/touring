---
type: Report
title: "Retroativo 20261101 — deriva de dependências locais"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793544410603554012::retroativo
okf_version: 1
timestamp: 2026-11-01
tags: ["#kind:report", "#domain:vgp", "#status:measured"]
---

# Diagnóstico retroativo do held-out 20261101

## Escopo

Medição de sensibilidade da reconstrução, conforme `../../PREREGISTRO-DEPENDENCIAS-LOCAIS-2026-11-01.md`. Não é novo veredito de release, não reclassifica a verdade dos rótulos e não reescreve J14/J15.

Foram examinadas as 600 entradas de arquivo + 400 inline. O resolvedor corrigido (`ab256a5b`) reconstruiu os módulos locais pelo Git no `ts`; o motor 30.4.102 comparou os estados atual × histórico apenas no subconjunto alterado, reutilizando os vereditos atuais já gravados no J15.

## Números

| Medida | Resultado |
|---|---:|
| Entradas | 1.000 (600 arquivo + 400 inline) |
| Dependências locais examinadas | 21.684 |
| Dependências diferentes entre `ts` e hoje | 5.146 |
| **Entradas com ao menos uma dependência alterada** | **119/1.000 (11,9%)** |
| Entradas INCONCLUSIVO-RECONSTRUÇÃO | 53/1.000 |
| Entradas alteradas reconstruíveis e comparadas | 109/119 |
| `unknown` no estado atual | 51/109 |
| `unknown` no estado histórico | 59/109 |
| Pares conhecidos nos dois estados | 39/109 |
| Pares indeterminados por `unknown` em ao menos um lado | 70/109 |
| **Mudanças de veredito efetivo nos pares conhecidos** | **0/39** |
| `deny → não-deny` | 0/39 |
| `não-deny → deny` | 0/39 |
| Sem mudança conhecida | 39/39 |

Dez entradas alteradas não tinham fecho reconstruível e não foram ao motor. Os 53 casos INCONCLUSIVO-RECONSTRUÇÃO cobrem toda a população, não apenas as alteradas.

## Leitura

A exposição estrutural ao viés de J14/J15 é de **119 entradas**, mas a mudança de veredito observada é **zero somente entre os 39 pares conhecidos**. Os outros 70 pares comparados ficam indeterminados; não é válido anunciar “zero mudanças em 109” nem concluir que o viés total seja zero.

O caso `10607309d143`, cuja perícia documental já havia comprovado a deriva, ficou `deny` no estado atual e `unknown` no histórico. Ele não entra no 0/39 nem é contado como confirmação adicional.

A agregação independente de `porid.json` conferiu 1.000 entradas, 109 comparações, 39 pares conhecidos, 70 indeterminados e zero mudanças conhecidas.

## Limite

Pacotes de terceiros não foram congelados. Mudança de veredito mede sensibilidade da reconstrução, não verdade do rótulo. Nenhum conteúdo do corpus foi incluído neste diretório: apenas código, metadados e agregados.
