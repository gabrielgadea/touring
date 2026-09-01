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
| [RETOMAR-AQUI.md](/RETOMAR-AQUI.md) | ResumePointer | **ler primeiro ao retomar** |
| [strategy-2026-08-26-documentacao-sistema.md](/strategy-2026-08-26-documentacao-sistema.md) | Strategy | **aprovada** (F1–F3 + MkDocs) |
| [diagnostics/touring-20260826T080207.md](/diagnostics/touring-20260826T080207.md) | Diagnostic | executado |
| [phases/F1.md](/phases/F1.md) | PhaseReport | mapa de fusão — fechado |
| [phases/F2.md](/phases/F2.md) | PhaseReport | reconciliação — fechado |
| [phases/F3.md](/phases/F3.md) | PhaseReport | guard permanente — fechado |
| [phases/F6b.md](/phases/F6b.md) | PhaseReport | indefinidos movidos — fechado |
| [phases/F7.md](/phases/F7.md) | PhaseReport | site MkDocs — fechado |
| [gen_fusion_map.py](/gen_fusion_map.py) | Script | gera a tabela de fusão verificada |
| [f1-inventario.json](/f1-inventario.json) | Data | 56 arquivos, núcleo × indefinidos |
| [log.md](/log.md) | Log | histórico |

**DAG**: `task_1787743226070943011` (5/5 done) · **commit**: `fc95f28` na branch
`safety/2026-08-24-c2-w0-subcall-identity` · **convergência**: `loop_converged.py
--rust-full` exit 0.

Ledger CCE (fora do bundle, runtime):
`.touring-explore/documentacao-do-sistema-repositorio-e-infraestrutura-.ledger.json`
— convergido.

## Drift Semântico Scan (31/08/2026)

| Artefato | Path | Evidência |
|---|---|---|
| Script | `scripts/drift_semantic_scan.py` | Gate F2.1 Diamond (1.000) — CMDi remediado (zero `shell=True`) |
| Output | `knowledge/scan-2026-08-31.json` | 110 KB, 1096 .md files varridos, 7 categorias |
| Strategy revisada | `strategy-2026-08-31-retomar-decisao-drift.md` § Achados do Drift Semântico Scan | Classificação F4/F5 (subset cirúrgico) vs W6/W10 (amplo → guard automatizado) |

**Conclusão do scan**: drift massivo em todas as 7 categorias. Recomendação revisada: F4/F5 manual (~89 arquivos) + W6/W10 vira guard automatizado no CI.
