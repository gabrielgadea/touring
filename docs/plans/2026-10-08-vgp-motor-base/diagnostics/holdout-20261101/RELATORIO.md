---
type: Report
title: "Held-out 20261101 — selado, disjunto e aguardando o release 30.4.102"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793487928776429345::HOLD11
okf_version: 1
timestamp: 2026-10-31
tags: ["#kind:report", "#domain:harness", "#status:sealed"]
---

# Held-out 20261101 — selo e bloqueio do J15

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## 1. Identidade

| Item | Valor |
|---|---|
| Corpus | `~/.claude/touring/vgp/corpus_holdout_disjunto_20261101` |
| Selo | `sha_tree 6c228591f1f404f5` · 2.478 arquivos · 16.400.909 bytes |
| Semente / corte | `20261101` · `2026-10-09T18:14:31.894Z` |
| Adjudicador | `fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c` |
| Construtor | `a206a6d08f91970671346809daaad2decae3ff0265f0d275ac93d02a14cb4967` |
| Índices | `holdout_meta.json` `f478d4f4dafa…` · `entradas.jsonl` `a08b0950c554…` · `entradas_inline_neg.jsonl` `437a133fd1eb…` |
| Conferência | `confere_selo.py`: selftest ok e `selo INTACTO` (rc 0) |

O parcial 20261031 e o corpus 20261031b foram incluídos nas exclusões, além de vivo, temporal e todos os held-outs anteriores. O `20261031b` já estava gasto no J14 e não foi reutilizado como fonte de julgamento.

## 2. Baldes e estado no instante

| Balde | Selecionado | Regra dos 300 |
|---|---:|---|
| Arquivo | **600** | passa |
| Inline agregado | **400** = c 200 + heredoc 200 + touring_run 0 + tmp 0 | passa |

A validação pré-selo encontrou **0 `ts=""` e 0 entradas posteriores ao corte**. O universo de arquivo contabiliza 1.979 exclusões `ts_ausente`. Estado no instante: 1.000 entradas · 873 overlays · **0 incompletas** · 0 lacunas independentes · fecho máximo 47 módulos. Vivo e temporal permaneceram intactos.

## 3. Sobreposição nas quatro chaves

Foram medidos ids, tool_use_ids, shas de conteúdo+contexto e comandos, com controle positivo de cada corpus contra si mesmo:

- vivo e temporal: **0/0/0/0**;
- 20261010, 20261011, `bak-20261009-pre`, `staging_evidencia`, `bak-2026-10-09`: **0/0/0/0**;
- 20261013 e 20261013f: **0/0/0/0**;
- parcial 20261031: **0/0/0/0**;
- 20261031b: **0/0/0/0**.

`OK_zero_em_todos_os_cruzamentos: true` · `controle_positivo_enxerga: true` · duplicatas internas de conteúdo: **0**.

## 4. Processo e bloqueio do J15

- Construção: `rodar_construtor_20261101.sh`, rc 0, 1.236,7 s, sob `hw-slot`.
- Validação/selo/cruzamento: `selar_e_cruzar_20261101.sh`, rc 0.
- A primeira tentativa deu rc=126 porque o runner compartilhado não tinha bit executável; foi corrigida com `chmod`, sem alterar parâmetros ou corpus.
- **Nenhum motor ou juiz rodou no 20261101.** O J15 fica para o binário do release 30.4.102, quando a touring-00 avisar que está instalado.

O conserto do motor que motiva o release foi entregue no commit `6f72b473`: `exit`/`quit` como helpers do site, com `exec`/`return` limpos e `trap` preservado como deny verdadeiro. Detalhes e arquivos para a fusão: `../fix-builtins/RELATORIO.md`.

Nenhum conteúdo de corpus foi copiado para o repositório: apenas metadados, selo, scripts e números.
