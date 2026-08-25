---
okf_version: "1.0"
type: Strategy
title: "Geração sem Write — harness de intenção sob contrato"
description: "Estratégia do programa aprovado por Gabriel em 25/08: o LLM emite GeneratorPlan (decisão estruturada), o executor materializa/prova/commita; ligar o pipeline existente e corrigir o que o exercício revelar"
tags: [strategy, geracao-sem-write, code-mode, generator]
timestamp: 2026-08-25T00:15:00-03:00
plan_id: 2026-08-25-geracao-sem-write
---

# Estratégia — Geração sem Write

**Tese** (aprovada por Gabriel, 25/08): inverter o CodeAct — o modelo emite
**decisão estruturada** (`GeneratorPlan`), um executor determinístico
materializa (typestate Draft→Verified→Rendered→Speculated→Committed),
o candidato prova-se no sandbox ANTES de tocar a árvore, e o "pronto" é
exit code (`loop_converged.py`, Lei L2). O Write do LLM é exilado para o
sandbox (semente de novidade → `--harvest` → composição).

**Método** (ordem de Gabriel): *"começa por onde esta sessão terminou:
ligando o pipeline e corrigindo o que o exercício revelar"* — exercitar-infra
é o método (`exercitar-infra-como-metodo`); 3 comandos de exercício já
revelaram o drift suggest↔schema↔serde (F1, corrigida nesta wave).

## Fundamento externo

- **ESAA-Core** (arXiv:2602.23193): event sourcing como fonte da verdade;
  log append-only, arquivos como projeção, single-writer, vocabulário
  versionado com `maps_to`, inventário fechado de reject codes
  ([[referencia:esaa-core-event-sourcing:2026-08-25]]).
- **aco/generators** (analise, 64k LOC): tríade execute/validate/rollback
  por spec; `learning.py` com Wilson lower-bound + KS drift = efetividade
  POR GERADOR (a portar em F3).

## DAG — task_1787624211878968599

| Fase | Escopo | Depende |
|---|---|---|
| F1 | Fonte única do vocabulário: `GeneratorPlan::skeleton` + serde(default) + testes da junta + exercício vivo | — |
| F2 | Exercitar pipeline ponta a ponta (plan-submit em alvo scratch; corrigir cada junta) | F1 |
| F3 | Learning por gerador (Wilson/KS do aco) | F2 |
| F4 | Promoção harvest→template | F2 |
| F5 | Gate de novidade (rota sandbox-seed) | F3, F4 |
| F6 | Event-sourcing do ciclo (ESAA): log append-only, projeção replayável, single-writer | F2 |
| F7 | Inventário fechado de reject codes (gates G1-G7 + CEG) + vocabulário `maps_to` | F1 |

**Ressalvas em pé**: "perfeito" é relativo à bateria de gates (nó 114 DGM);
a cauda de novidade muda de endereço, não desaparece.

Evidência OUTER: `diagnostics/` neste bundle + ledger CCE + exploração viva
desta sessão (aco aberto, schema exercitado, ESAA clonado e lido).
