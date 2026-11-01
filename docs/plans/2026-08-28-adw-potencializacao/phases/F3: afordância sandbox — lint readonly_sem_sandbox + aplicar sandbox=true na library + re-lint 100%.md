---
type: PhaseReport
title: F3: afordância sandbox — lint readonly_sem_sandbox + aplicar sandbox=true na library + re-lint 100% — phase report
description: F3 fechada: lint novo _lint_readonly_without_sandbox (dual do S-8.4) — leitor PROVADO (command_writes False ou readonly=true) fora do sandbo
plan_id: 2026-08-28-adw-potencializacao
tags: [loop, phase, F3: afordância sandbox — lint readonly_sem_sandbox + aplicar sandbox=true na library + re-lint 100%]
timestamp: 2026-08-28T09:07:58.056393-03:00
okf_version: "0.1"
---

# F3: afordância sandbox — lint readonly_sem_sandbox + aplicar sandbox=true na library + re-lint 100% — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

F3 fechada: lint novo _lint_readonly_without_sandbox (dual do S-8.4) — leitor PROVADO (command_writes False ou readonly=true) fora do sandbox ganha warning com remédio derivado; calibrado para exigir leitor SUBSTANTIVO (_FS_READING_COMMANDS/multiplexers/programas — echo/true não têm o que conter, ruído com cara de rigor). O lint apontou 3 nós reais da library (critic-panel::quorum, graph-pack::relations, worker-critic-pair::judge) — sandbox=true aplicado nos 3; elegíveis 3/3 (100%). Prova viva: test_shipped_critic_panel executa o quorum de verdade, agora via touring run/CEG — 248/248 verdes. Espelho client/ sincronizado (guard library_and_repo_mirror_agree).

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/F3: afordância sandbox — lint readonly_sem_sandbox + aplicar sandbox=true na library + re-lint 100%.json](/knowledge/F3: afordância sandbox — lint readonly_sem_sandbox + aplicar sandbox=true na library + re-lint 100%.json).
