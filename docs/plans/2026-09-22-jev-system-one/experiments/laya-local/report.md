---
type: ExperimentReport
title: Laya local na RTX 4060 sobre decisões do Touring
description: Latência, VRAM e acurácia zero-shot dos checkpoints abertos do Laya em quatro decisões tiradas dos dados do próprio Touring, contra a heurística que o Touring usa hoje.
plan_id: 2026-09-22-jev-system-one
tags: [experiment, laya, local, rtx4060, zero-shot]
timestamp: 2026-09-22T14:40:00-03:00
okf_version: "0.1"
---

# Laya local na RTX 4060 sobre decisões do Touring

Parte do [bundle](/index.md). Scripts: [probe.py](/experiments/laya-local/probe.py) e
[probe_intent.py](/experiments/laya-local/probe_intent.py). Resultados brutos:
[results.json](/experiments/laya-local/results.json) e
[results_intent.json](/experiments/laya-local/results_intent.json).

## Ambiente

NVIDIA GeForce RTX 4060 Laptop (8 GB), driver 610.57, torch 2.14.0+cu130,
transformers 5.17.0, pacote `laya` 0.3.5, venv isolado no scratchpad. Checkpoints
`convaiinnovations/laya` (ModernBERT-large, 421M, contexto 512) e o subfolder
`multilingual` (mmBERT-base, 322M, contexto 1024). Zero-shot: nenhum ajuste nem
temperatura refeita. Instruções e critérios em inglês; conteúdo como está (pt-BR e inglês).

## Latência e memória

| Checkpoint | 1 pergunta p50 / p95 | 10 perguntas | 50 perguntas | pico de VRAM |
|---|---|---|---|---|
| laya (inglês, 421M) | 14,4 / 14,7 ms | 34,6 ms | 128,7 ms | 3,6 GB |
| laya-multilingual (322M) | 6,4 / 6,9 ms | 15,3 ms | 56,7 ms | 3,2 GB |

Estado curto, GPU sincronizada, carregamento excluído. O pico inclui os dois modelos
carregados e as ativações do lote. Para comparar: o TCP até `api.typesafe.ai`
(AWS us-west-2) leva 209–231 ms a partir desta máquina, antes de qualquer inferência.

## Acurácia zero-shot

| Prova | Gabarito | Laya (melhor config.) | Hoje no Touring | Referência |
|---|---|---|---|---|
| T1 tipo de memória, 5 classes, 150 itens | prefixo da chave escolhido ao gravar | 0,50 (router) | tag `kind` derivada: 0,067 | acaso 0,20 |
| T1 só com confiança ≥ 0,8 | idem | 0,76 em 41 itens (27%) | — | — |
| T2 roteamento de tasks do DAG, 118 itens | `task_type` mapeado para ADW | 0,37 (inglês) | regex do factory: 0,10 (casa 36%) | classe majoritária 0,58 |
| T3 relevância do recall, 12 itens | 2 memórias sobre o tema | 0 de 2 no topo | BM25/RRF trouxe 10 irrelevantes | — |
| T4 intent do prompt, 25 dourados | rótulos dos testes do `prompt_enhance` | 0,88 (16/16 com conf ≥ 0,8) | keyword: 1,00 (por construção) | — |
| T4 textos reais, 12 itens | rotulados à mão | 0,42 | keyword: 0,83 | — |

Por classe (T1, router): gotcha 0,83, snippet 0,80, strategy 0,50, audit 0,33,
lesson 0,03. Em T2 o Laya acerta `audit` (0,79) e erra muito `explore-plan`.

## O que isto permite afirmar

1. **Latência e custo resolvem o problema de orçamento.** 6–15 ms por decisão cabem num
   hook de UserPromptSubmit e em qualquer tarefa em segundo plano; o Jev, daqui, gasta
   mais que isso só em rede.
2. **Zero-shot não basta nos dados do Touring.** Em duas provas o Laya fica abaixo da
   classe majoritária ou não acha o relevante. Isso confirma o que o próprio autor
   escreve: "a fast base to specialise, not a zero-shot decision engine".
3. **A confiança carrega sinal.** Na faixa ≥ 0,8 a precisão sobe muito (T1: 0,76; T4
   dourado: 16/16). Mas o checkpoint multilíngue, que sai sem temperaturas ajustadas,
   errou uma notificação com 0,90 — calibrar antes de confiar.
4. **O classificador de intent atual erra onde mais importa.** Os dois prompts de
   pesquisa profunda desta sessão saíram como GENERAL (CILA baixo), e o hook injetou
   "modos" em notificações automáticas durante a própria sessão (TEST, CODE, DEBUG).

## Limites do método

- Gabaritos imperfeitos: prefixos de chave e `task_type` são rótulos fracos; o `task_type`
  "plan" pode descrever uma feature.
- Amostras pequenas (12 a 150 itens); números indicativos, confiança ~0,7.
- As notificações de T4 foram encurtadas; o erro ao vivo aconteceu no texto completo.
- Um único prompt por decisão, sem engenharia de critérios nem shortlist.
- O Jev não foi medido (não há chave nesta máquina); nenhuma comparação direta.
