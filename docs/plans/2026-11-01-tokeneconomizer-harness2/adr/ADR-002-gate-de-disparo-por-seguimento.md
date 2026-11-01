---
type: ADR
title: "ADR-002 — O seguimento medido de cada família de injeção decide se ela continua sendo injetada"
description: "Ligar o injection_follow_ratio, hoje só um número global no KPI, a um gate por família no molde do FIRE_GATE do TokenEconomizer."
timestamp: 2026-11-01T18:00:00-03:00
status: proposta
---

# ADR-002 — Gate de disparo por seguimento medido (POTENCIALIZAR)

## Contexto

- **Temos a medição e não a usamos para decidir (FATO 1.0):** `touring kpi -j` → `context_budget`
  expõe `injection_follow_ratio` (0,598, piso 0,6, n=502, 1 transcrito em 01/11) e `by_family`
  (bytes, duplicação, blocos por família); `injection_policy` já tem níveis de supressão
  (`client/hooks/injection_policy.py`, `injection_level: full`). Nada liga um ao outro: uma família
  que ninguém segue continua injetada.
- **Externo (FATO 1.0):** `FIRE_GATE=0.40` sobre `FIRE_MIN_N=4` (`microskills/lifecycle.py:20-21`) —
  um fato ativo só é injetado enquanto o desperdício que ele evita de fato ocorre em ≥ 40% das
  tarefas que o exercitaram.
- **Linha de base (FATO, 1 transcrito):** 6.039 B injetados por turno contra teto de 2.000; a família
  `code-mode-contract` tem 90,9% de bytes duplicados; `evidence-guard` 108 blocos.

- **Rodada 2 (FATO 1.0, verificado):** o FIRE_GATE deles é vácuo em parte — `fired` cai para `scope`
  quando o detector não declara um disparo próprio (`microskills/distiller.py:292`), e nos fatos de
  ambiente `fired = exercised` (`microskills/envfacts.py:508`): a taxa dá 1,0 por construção. O nosso
  gate deve medir SEGUIMENTO observado (o que o `injection_follow_ratio` já faz), nunca ocorrência.
  E a mudança de nível só vale a partir da sessão seguinte, para não quebrar o cache do prefixo
  (`plugin/fold.py:43-52`).

## Opções

1. Limiar global (status quo: um número que só reprova o KPI).
2. **Gate por família**: cada família com n ≥ N e seguimento < τ desce de nível (`full` → `brief` →
   `off`) na política de injeção; volta quando o seguimento medido em sombra sobe.
3. Gate por item (por lição/por bloco): mais fino, exige identidade estável do bloco — fase 2.

## Decisão recomendada

**Opção 2** (confiança 0,75 — INFERÊNCIA). τ e N escolhidos POR MEDIÇÃO no corpus (não copiar 0,40/4
de SWE-Bench), com a regra de decisão escrita antes do dado (BACKLOG B2). Compõe com ADR-001: a
família `past-lessons` é a primeira candidata.

## Reversibilidade

Alta: a política já tem `level`; o gate só escolhe o nível. Kill switch = fixar `full`.

## Como medir

Bytes injetados por turno e seguimento por família, A/B por dia (desenho do AB34) depois de 08/11.
