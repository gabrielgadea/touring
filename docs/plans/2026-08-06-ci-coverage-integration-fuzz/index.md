---
okf_version: "0.1"
type: LoopBundle
title: "CI do touring — coverage, integration e fuzz vermelhos"
description: "Bundle da rodada de 06/08/2026: verificar se os três jobs seguem vermelhos, achar a causa medida de cada um e sincronizar CI com a realidade da codebase."
plan_id: 2026-08-06-ci-coverage-integration-fuzz
tags: [loop, ci, coverage, integration, fuzz]
timestamp: "2026-08-06T13:30:00-03:00"
---

# Bundle — CI vermelho (coverage · integration · fuzz)

Objetivo: confirmar por execução se os três jobs seguem vermelhos, estabelecer a
causa **medida** de cada um e deixar o CI sincronizado com a realidade in loco
da codebase.

## Documentos

| Doc | Conteúdo |
|---|---|
| [/strategy-2026-08-06-ci-red-jobs.md](/strategy-2026-08-06-ci-red-jobs.md) | Diagnóstico completo, causa por job, correções e o que fica no gate humano |
| [/diagnostics/touring-20260806T131302.md](/diagnostics/touring-20260806T131302.md) | Diagnóstico determinístico (health, 50-dim, wiring, memory, estrutura) |
| [/log.md](/log.md) | Histórico cronológico da rodada |

## Veredito de entrada

Run `30757323428` (main, 02/08): 5 jobs verdes, 3 vermelhos — coverage,
integration e fuzz, exatamente os citados. `HEAD == origin/main`, sem commits
pendentes.

## Ledger CCE

`.touring-explore/ci-do-touring--coverage--integration-tests-e-fuz.ledger.json`
— 3 rodadas de exploração até o sinal de esgotamento (`dry_signal: present`),
`evidence_report` com veredito `pass`.
