---
type: LoopBundle
title: Documentação Touring — sistema, repositório e infraestrutura
description: Bundle do loop 2026-08-26 — F1 mapa de fusão, F2 reconciliação, F3 guard permanente, F6b arquivos indefinidos, F7 MkDocs.
plan_id: 2026-08-26-documentacao-touring
tags: [loop, bundle, documentacao, drift, crates]
timestamp: 2026-08-26T09:30:00-03:00
okf_version: "0.1"
---

# Bundle — Documentação Touring (sistema/repo/infra)

Escopo (Gabriel, 26/08): documentação do repositório, infraestrutura e sistema —
não planos, não trabalho operacional. Aprovado: F1–F3 + MkDocs; manter os
`ARCHITECTURE.md`; mover os indefinidos para pasta própria.

| Documento | Tipo | Estado |
|---|---|---|
| [strategy-2026-08-26-documentacao-sistema.md](/strategy-2026-08-26-documentacao-sistema.md) | Strategy | **aprovada** |
| [diagnostics/touring-20260826T080207.md](/diagnostics/touring-20260826T080207.md) | Diagnostic | executado |
| [phases/F1.md](/phases/F1.md) | PhaseReport | mapa de fusão — fechado |
| [phases/F6b.md](/phases/F6b.md) | PhaseReport | indefinidos movidos — fechado |
| [gen_fusion_map.py](/gen_fusion_map.py) | Script | gera a tabela de fusão verificada |
| [f1-inventario.json](/f1-inventario.json) | Data | 56 arquivos, núcleo × indefinidos |
| [log.md](/log.md) | Log | histórico |

Ledger CCE (fora do bundle, runtime):
`.touring-explore/documentacao-do-sistema-repositorio-e-infraestrutura-.ledger.json`
— convergido.
