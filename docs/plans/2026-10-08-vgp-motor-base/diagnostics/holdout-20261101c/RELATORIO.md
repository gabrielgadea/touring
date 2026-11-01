---
type: Report
title: "Held-out 20261101c — construído e selado sem julgamento"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793544410603554012::heldout-20261101c
okf_version: 1
timestamp: 2026-11-01
tags: ["#kind:report", "#domain:vgp", "#status:sealed"]
---

# Held-out 20261101c

## Resultado

Corpus construído e selado **sem qualquer julgamento**. O J16 fica para depois do release de 04/11/2026.

| Medida | Resultado |
|---|---:|
| `sha_tree` | `b5a819f38c57d227` |
| Arquivos no corpus | 3.015 |
| Bytes | 23.662.250 |
| Negativos de arquivo | 600 |
| Negativos inline | 371 |
| Total | 971 |
| `ts` ausente nas entradas | 0 |
| Depois do corte | 0 |
| INCONCLUSIVO-RECONSTRUÇÃO no corpus | 0 |
| Overlays do estado no instante | 1.439 |
| Semente | 20261103 |
| Corte | `2026-10-09T18:14:31.894Z` |
| Tempo de construção | 5.372,6 s |
| Vivo e temporal intactos | sim |

## Regra aplicada

O construtor `8d166db3` congela o fecho de módulos locais pelo Git vigente no `ts`, incluindo `src/`, `python/` e `packages/*/{src,python}`. Dependências locais não reconstruíveis foram recusadas e contadas fora do denominador; nenhuma entrou no corpus. Pacotes de terceiros ficam fora da regra, como declarado no pré-registro.

O verificador independente do fecho terminou com zero lacunas. Há 140 entradas com módulos no fecho (971 − 831 sem overlays), profundidade máxima 17.

## Selo e disjunção

- `confere_selo.py --selftest`: passou.
- Conferência depois do selo: `b5a819f38c57d227` intacto.
- Cruzamento contra vivo, temporal e todos os held-outs anteriores, incluindo `20261101`: **zero sobreposição nas quatro chaves**, com controle positivo ativo.
- O motor e o juiz **não rodaram** neste corpus.

## Artefatos

- `selo.json`: hashes, regra e proveniência.
- `validacao_pre_selo.json`: contagens e zero incompletos.
- `sobreposicao_20261101c.json`: disjunção e controle positivo.
- `congelados.json`: 11 instrumentos fixados antes da construção.

Nenhum conteúdo do corpus foi copiado ao repositório; somente scripts, metadados, hashes e agregados foram commitados.
