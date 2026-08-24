---
okf_version: "1.0"
type: LoopBundle
title: "Topologia Honesta — rodada 3 das quatro fontes"
description: "Bundle da terceira rodada: releitura integral das quatro fontes, sete lacunas verificadas e o plano de onze entregas em cinco trilhas que faz o grafo ADW dizer a verdade sobre si mesmo."
plan_id: 2026-08-19-topologia-honesta
dag: task_1787118866702163488
tags: ["#kind:bundle", "#domain:adw", "#domain:loop-engineering", "#artifact:index", "#status:done"]
timestamp: 2026-08-19T02:55:00-03:00
---

# Bundle — Topologia Honesta

Terceira rodada do programa iniciado em [`2026-08-18-graph-engineering-flow-portfolio`](../2026-08-18-graph-engineering-flow-portfolio/index.md).
As duas anteriores entregaram o **agent graph** (11 entregas) e a **integridade do juiz**.
Esta releu as quatro fontes primárias perguntando o que ainda **não** foi implementado.

## Conteúdo

| Documento | Tipo | O que traz |
|---|---|---|
| [`plano.html`](/plano.html) | Plan | **O plano completo**: 11 entregas, 5 trilhas, gates, riscos e as 3 decisões pendentes |
| [`plan.md`](/plan.md) | Plan | O contrato legível por máquina: entregas, DAG, dependências |
| [`phases/P1.md`](/phases/P1.md) | PhaseReport | Fecho da fase de implementação (gates + evidência) |
| [`knowledge/P1.json`](/knowledge/P1.json) | Knowledge | Abstract tipado do fecho (Hyper-Extract) |
| [`log.md`](/log.md) | Log | Histórico cronológico |

## Estado — CONCLUÍDO 19/08/2026

DAG `task_1787118866702163488` — **11 subtarefas registradas, 0 ciclos**, 7 prontas para começar em paralelo.

**11/11 entregas construídas.** Gate de convergência `loop_converged.py` **exit 0**, 8 cláusulas,
tier Platinum 0,9361.

| Gate | Resultado |
|---|---|
| `test_adw.py` | **156** testes (baseline 136) |
| `test_variant_archive.py` + `test_judge_attest.py` + flow guard | **72** |
| Rust (`touring-cli` + `touring-hooks`) | **1.402**, 0 falhas · clippy `-D warnings` exit 0 |
| Guards do repo | **197** |
| Espelho `client/` | limpo, 278 arquivos |
| `touring adw lint` na library | 8/8 carregados, 0E 0W |

Três achados que a implementação produziu, e que o plano não previa: (1) `cli_decompose_update`
e `ensure_decompose_tables` existem **duplicados**, e o gêmeo vivo não é o que estava em
`handlers/`; (2) uma fixture de teste declarava um nó `build` com `allowed_tools = ["Read"]` —
o lint `dead_node` a pegou; (3) o ledger de promoção mostrou que **só 2 dos 8** fluxos
embarcados já completaram uma execução real.

Fontes primárias (transcrições + PDF): `../2026-08-18-graph-engineering-flow-portfolio/sources/`.
