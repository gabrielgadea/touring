---
type: LoopBundle
title: "Economia de Contexto — o token como recurso físico no harness"
description: "Bundle OKF do plano de 04/09/2026: diagnóstico medido do custo de injeção e do gate OUTER, as sete frentes que o corrigem, e o registro das duas rodadas de execução."
plan_id: 2026-09-04-economia-de-contexto
okf_version: 0.1
tags: [context-engineering, harness, cila, injecao, outer, kpi]
timestamp: 2026-09-04T12:10:00-03:00
---

# Economia de Contexto — bundle

Lei que governa: *a janela é o recurso mais precioso; não se trata de usá-la
menos, e sim de fazer cada token atingir o máximo de utilidade e convergência
com o objetivo da tarefa.*

| Documento | Papel |
|---|---|
| [`plan.md`](/plan.md) | Plano Pln2 L4 — ground truth medido, 4 defeitos (D1-D4), 7 fases P0-P6 |
| [`log.md`](/log.md) | Registro das duas rodadas de execução e do que cada uma corrigiu do próprio plano |
| [`phases/S-4.3-classe-b.md`](/phases/S-4.3-classe-b.md) | Fechamento da classe B — o executor que materializa o registro do turno |
| [`phases/S-5.1b-fracao-usada.md`](/phases/S-5.1b-fracao-usada.md) | Fechamento da medição de reuso — e o veredito que refuta o digest por truncagem |
| `knowledge/` | Abstracts tipados (Hyper-Extract) das duas fases, ids determinísticos |
| `.baseline/orphans-scoped.txt` | Baseline de órfãos gravada pelo juiz (1557) — evidência da cláusula `orphans_base` |

## Estado

**13 de 13 subtarefas entregues.** DAG da 2ª rodada: `task_1788534325883539352`
(2/2 done); a 1ª rodada correu por `/goal`, sem DAG. Juiz de convergência:
`loop_converged.py` **exit 0**, 8 cláusulas — Platinum 0.937, órfãos 1557,
0 P0 BLOCK, cargo verde.

## Os dois resultados NEGATIVOS, registrados como resultados

Um plano honesto precisa poder dizer "medi e não há trabalho aqui", senão toda
fase vira dívida perpétua.

| fase | hipótese | veredito medido |
|---|---|---|
| **P6** CUR | o prefixo do SessionStart não quebra o cache | **confirmada** — CUR = 0,971 contra meta 0,80-0,90. Nada a corrigir. |
| **P5** digest | truncar os tool results dominantes economiza janela | **refutada na forma esboçada** — as linhas reusadas são uniformes pelos decis; cabeça+cauda cobre ≥90% do reuso em só 45% dos casos. O que vale é digest derivado do conteúdo (`--brief`) + spill. |

## Deploy

Nada propagado. `update-touring` muda o comportamento de toda sessão CC e
aguarda ordem de Gabriel. Enquanto o daemon carregar o binário antigo,
`touring kpi -j` **não** mostra `tool_output` — os números do log vêm do binário
de teste.

Memórias: `gate-barato-e-subproduto` · `extremo-uniforme-e-o-instrumento` ·
`assercao-mais-estreita-que-o-nome` · `binario-precede-a-fonte`
