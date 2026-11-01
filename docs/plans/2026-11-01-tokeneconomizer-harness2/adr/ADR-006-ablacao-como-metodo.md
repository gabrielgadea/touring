---
type: ADR
title: "ADR-006 — Ablação por componente como método padrão para decidir hooks, com orçamento auditado por identidade"
description: "Do harness2: leave-one-out por componente e um ledger que falha se o número de execuções não fecha. Nosso AB34 é fatorial em 2 fatores; estender a ablação às famílias de injeção."
timestamp: 2026-11-01T18:00:00-03:00
status: proposta
---

# ADR-006 — Ablação como método

## Contexto

- **harness2 (FATO 1.0 no código; números do paper INFERÊNCIA 0,8 — extraídos por modelo do HTML):**
  - ablação por remoção de componente (Tabela 3, k=1: sem biblioteca −3,1/−1,8; sem escopo −2,9/−3,7;
    sem composição −1,6/−1,9; Tabela 6: sem skills −5,3, sem scripts −3,7, sem prompt −1,5);
  - os próprios autores declaram uma execução por célula, sem n/IC, e efeitos não aditivos;
  - `harness2/core/ledger.py:24-31` fixa o orçamento (R=4k rollouts, P=3k sessões) e `:89`
    `assert_identity` levanta `BudgetError` se as execuções registradas não fecham (duplicata antes da
    contagem);
  - `harness2/core/selection.py:71` (`assign_ids`) anonimiza candidatos com semente estável — o
    seletor não vê qual é qual.
- **Nosso:** o AB34 é fatorial 2×2 por dia (compactador × evicção) com pré-registro
  (`docs/plans/2026-10-31-ab34-compactador-eviccao/`). As ~23 famílias de injeção nunca foram ablacionadas
  uma a uma; a rodada 4 já recomendava "ablação sem reexecutar a melhoria" como método.

## Decisão recomendada

**INSPIRAR** (confiança 0,8): (a) desenho de ablação leave-one-family-out para as famílias de
injeção, com n por célula calculado ANTES (o defeito declarado do harness2 é exatamente n=1);
(b) **APROVEITAR a ideia do `assert_identity`** nos nossos runners de experimento — o runner falha se
as células executadas ≠ as pré-registradas (duplicata ou falta), o que hoje só um humano confere;
(c) anonimização das condições para qualquer juiz de LLM que avalie saída de experimento.

## Reversibilidade

Total (método).

## Como medir

BACKLOG B6: o próximo experimento depois do AB34 sai com o ledger de identidade e reprova se uma
célula faltar.
