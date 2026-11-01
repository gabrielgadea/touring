---
type: ADR
title: "ADR-005 — Medir o custo em tokens (com cache) por mecanismo, contando o nosso próprio overhead"
description: "Hoje medimos bytes injetados e custo total da sessão; o TokenEconomizer mede custo por mecanismo incluindo as próprias chamadas. Sem isso, nenhuma das outras decisões tem desfecho em custo."
timestamp: 2026-11-01T18:00:00-03:00
status: proposta
---

# ADR-005 — Custo por mecanismo

## Contexto

- **Nosso (FATO 1.0):** `context_budget` mede BYTES por família; custo só agregado da sessão
  (`client/hooks/status_monitor.sh:112` lê `total_cost_usd`; `client/skills/Touring/scripts/adw.py:2239`
  por nó ADW). Os transcritos têm registros `cost-state` e `api-request-shape` (vistos em 01/11) que
  nenhum KPI lê. A rodada 4 registrou que custo em tokens/dólar do Touring não foi medido
  (`docs/plans/2026-10-26-injecao-contexto-hooks/round4/`, resumo em `f0_rodada4`).
- **Externo (FATO 1.0):** `tokeneconomizer/ledger.py`, `pricing.py`, `plugin/accounting.py` — o custo
  reportado INCLUI as side-calls do próprio plugin; a manchete −18,4%..−30,9% já é líquida.
- **Por que importa:** bytes no prefixo cacheado custam ~1/10 de bytes novos; uma decisão de poda
  (ADR-003) ou de supressão (ADR-002) julgada em bytes pode otimizar o que já é barato.

## Decisão recomendada

**APERFEIÇOAR o `touring kpi`** com custo em tokens por família e por superfície (entrada nova,
leitura de cache, escrita de cache), a partir dos registros de uso do transcrito, e com o nosso
overhead (hooks, side-calls, subagentes) contado na mesma conta (confiança 0,85 — é medição; o risco é
atribuição, não valor).

## Reversibilidade

Total (só leitura).

## Como medir

BACKLOG B5: o KPI fecha com a fatura (soma por mecanismo = custo total da sessão ± 2%) em 10 sessões
sorteadas.
