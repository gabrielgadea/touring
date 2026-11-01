---
type: Strategy
title: Estratégia — grafo de memória como padrão mínimo
description: Estratégia da diretiva de 30/08 — o corpo canônico é o contrato em docs/memory-graph-contract.md; este doc registra a decisão de rota e os fatos que a fundamentam.
plan_id: 2026-08-30-grafo-memoria-padrao-minimo
tags: [strategy, memory, graph]
timestamp: 2026-08-30T01:20:00-03:00
okf_version: "0.1"
---

# Estratégia

Corpo canônico: [`/docs/memory-graph-contract.md`](../../memory-graph-contract.md)
(contrato de 7 cláusulas → predicados, fases P1-P5, escada de enforcement).

Decisão de rota desta janela: a diretiva pedia **explorar/analisar/propor** — a
implementação das fases aguarda aprovação de Gabriel (HUMAN GATE do loop). O
contrato foi persistido como nó do próprio grafo
(`contrato:grafo-memoria-padrao-minimo:2026-08-30`, 2 arestas tipadas, leitura
de volta `total=1`) e a peer `analise-e0` recebeu o ponteiro para derivar o
contrato dela.

Fatos de grounding (executados nesta sessão): faceta desconhecida morre em
silêncio (`Facet::from_str_ci`→None); arestas já viajam no recall
(`attach_one_hop_links`) e o re-rank as ignora; `supersedes` é executável
(retire, rlm.rs:535); `knowledge/P1.json` real degenerado (1 nó, 0 relações).
