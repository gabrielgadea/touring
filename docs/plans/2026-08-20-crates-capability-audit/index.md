---
okf_version: "1.0"
type: LoopBundle
title: "Auditoria de alcance das capacidades dos crates"
description: "Bundle OKF da exploração exaustiva dos 42 crates do Touring e do alcance real dessas capacidades pelos fluxos ADW e pelo harness loop-engineering"
plan_id: 2026-08-20-crates-capability-audit
tags: [adw, loop-engineering, capability-reach]
timestamp: 2026-08-20T02:30:00-03:00
---

# Bundle — Auditoria de alcance das capacidades dos crates

| documento | tipo |
|---|---|
| [strategy-2026-08-20-capability-reach.md](./strategy-2026-08-20-capability-reach.md) | Strategy (rodada 1 — alcance pelos fluxos) |
| [strategy-2026-08-20-rodada2-inalcancavel.md](./strategy-2026-08-20-rodada2-inalcancavel.md) | Strategy (rodada 2 — alcance pelo binário e pela composição) |
| [strategy-2026-08-20-rodada3-runtime.md](./strategy-2026-08-20-rodada3-runtime.md) | Strategy (rodada 3 — o que morre em runtime) |
| [strategy-2026-08-20-rodada4-validacao.md](./strategy-2026-08-20-rodada4-validacao.md) | Strategy (rodada 4 — o validador da validação) |
| [diagnostics/touring-20260820T022335.md](./diagnostics/touring-20260820T022335.md) | Diagnostic |
| [log.md](./log.md) | Log |

Ledger CCE: `.touring-explore/capacidades-dos-crates-touring-que-potencializam.ledger.json`
(4 rodadas, `dry_signal: present`, verdict `pass`).

## Fases executadas (19/19)

| documento | tipo |
|---|---|
| [phases/D1.md](./phases/D1.md) | PhaseReport |
| [phases/D2.md](./phases/D2.md) | PhaseReport |
| [phases/D3.md](./phases/D3.md) | PhaseReport |
| [phases/E1.md](./phases/E1.md) | PhaseReport |
| [phases/E2.md](./phases/E2.md) | PhaseReport |
| [phases/E4.md](./phases/E4.md) | PhaseReport |
| [phases/E5.md](./phases/E5.md) | PhaseReport |
| [phases/E6.md](./phases/E6.md) | PhaseReport |
| [phases/F1.md](./phases/F1.md) | PhaseReport |
| [phases/F4.md](./phases/F4.md) | PhaseReport |
| [phases/F7.md](./phases/F7.md) | PhaseReport |
| [phases/G1.md](./phases/G1.md) | PhaseReport |
| [phases/G10.md](./phases/G10.md) | PhaseReport |
| [phases/G11.md](./phases/G11.md) | PhaseReport |
| [phases/G2.md](./phases/G2.md) | PhaseReport |
| [phases/G4.md](./phases/G4.md) | PhaseReport |
| [phases/G7.md](./phases/G7.md) | PhaseReport |
| [phases/G8.md](./phases/G8.md) | PhaseReport |
| [phases/G9.md](./phases/G9.md) | PhaseReport |

## Veredito

19 de 19 subtasks fechadas. **18 entregues; E2 recusada por decisão humana** (Gabriel,
20/08/2026) — `settings.json` é portão humano no protocolo, e a lacuna de detecção de PII
fica registrada como ABERTA e CONHECIDA, não silenciada.

Cinco afirmações minhas caíram sob verificação e estão corrigidas **no lugar**, com data:
`predict-action` como capacidade funcional (era constante), `judge_attest` como duplicata
do `attest-contract` (são complementares), "nenhum caminho verifica idade" (o TTL existe),
`mpatch` como órfão (é opcional-que-avisa), e a tabela de 42 crates (3 não eram crates).
