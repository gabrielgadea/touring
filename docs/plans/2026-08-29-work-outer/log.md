---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-29-work-outer
tags: [loop, log]
timestamp: 2026-08-29T01:01:29.038473-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-29T22:58:53.751132-03:00 — p1_fail_loud done

P1 fail-loud: store responde ignored_facets (sempre presente, razão ensina as 7 facetas + suggestion Levenshtein); query e recall respondem unknown_facets (token que caiu para texto declarado). 5 testes novos, 217 memory verdes.

## 2026-08-29T22:58:53.867327-03:00 — p1_5_supersede done

P1.5: supersede reaponta memory_links do nó antigo (id determinístico recalculado, aresta de sucessão preservada, fail-open). Teste com mutação nomeada.

## 2026-08-29T23:01:05.367320-03:00 — p2_feromonio done

P2 feromônio estrutural: attach_one_hop_links movido para ANTES do rerank; rank key vira (classe_valor, é_traço, bucket_estrutura) — generated-by+>=2 arestas > alguma aresta > órfão, nunca cruzando classe de valor. 2 testes com contraprova.

## 2026-08-29T23:04:08.457973-03:00 — p3_contrato_visivel done

P3: resposta do store ganha contract:{key_shape,faceted,linked,provenance} advisory para kinds curados; KPI memory_graph_contract_share (janela 14d, predicado key_shape_ok compartilhado com o advisory — D8) + commitment advisory gte 0.5.

## 2026-08-29T23:06:24.518900-03:00 — p4_duas_projecoes done

P4: link_provenance no executor do close (generated-by para loop:<task> e decomp:<task>) + knowledge/Pn.json vira projeção do material real (nunca mais 1 nó/0 arestas). 4 testes.

## 2026-08-29T23:13:00.130555-03:00 — p5_arestas_derivadas done

P5: co-serviço durável (memory_coserved gravada pelo recall, top 10 served) + touring memory suggest-links (modo suggest do handler links: pares >= min_co sem aresta, marcados derived, apply command 1:1, nunca automático) + KPI memory_edge_density (a régua da escada). Teste e2e 1/1.
