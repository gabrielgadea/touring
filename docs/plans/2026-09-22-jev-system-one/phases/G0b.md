---
type: PhaseReport
title: "G0b — phase report"
description: "Causa raiz da prova 38/40 encontrada e corrigida: o daemon herdava TOURING_CODE_MODE=native do shell que o reiniciou, desligando os gates de"
plan_id: 2026-09-22-jev-system-one
okf_version: 0.1
tags: [loop, phase, G0b]
timestamp: 2026-09-22T22:26:36.848241-03:00
---

# G0b — phase report

**Status**: done

Part of the [bundle](/index.md) · [log](/log.md).

## Summary

Causa raiz da prova 38/40 encontrada e corrigida: o daemon herdava TOURING_CODE_MODE=native do shell que o reiniciou, desligando os gates de code mode da máquina inteira (doctor verde, contadores em zero). Correção nos DOIS launchers (daemon_spawn::PER_COMMAND_RELAXATIONS no Rust; env -u em launch_daemon_and_wait do update-touring), listas cruzadas por test_update_touring.py, porta deliberada TOURING_DAEMON_CODE_MODE, guard cruzado provando que as duas rotas limpam (mutação mata). Somado: o hook OUTER (loop_outer_arm) deixou de anunciar artefatos que não disparou. Propagado como 30.4.65 nos 3 projetos, com a prova comportamental 40/40 dentro do próprio pipeline.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/G0b.json](/knowledge/G0b.json).
