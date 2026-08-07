---
type: LoopBundle
title: Recuperação de contexto nos loops + avaliação da memória
description: Bundle OKF da run que deu um leitor ao checkpointer e mediu o uso real da memória do Touring.
plan_id: 2026-08-02-loop-memory-recovery
tags: [loop, memory, recall, compaction]
timestamp: 2026-08-02T14:45:00-03:00
okf_version: "0.1"
---

# Bundle — recuperação de contexto + avaliação da memória

| Documento | Tipo | Conteúdo |
| --- | --- | --- |
| [strategy-2026-08-02-loop-memory-recovery.md](/strategy-2026-08-02-loop-memory-recovery.md) | Strategy | fixes R1-R3, avaliação medida da memória, 3 comandos quebrados |
| [log.md](/log.md) | Log | histórico cronológico (notas de PreCompact caem aqui) |
| [diagnostics/touring-20260802T143853.md](/diagnostics/touring-20260802T143853.md) | Diagnostic | baseline medido no início da run |

Ledger CCE: `.touring-explore/memory-recall-context-recovery-resume-after-compac.ledger.json`.
