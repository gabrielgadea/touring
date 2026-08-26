---
type: PhaseReport
title: P2c — phase report
description: P2c fechado — a fonte da oferta corrigida por MEDICAO. O braco nascera preso ao fuse T3 e mediu-se t3_turn_fused=0: o gatilho nao ocorre nes
plan_id: 2026-08-25-autoresearch-rl-intelligence
tags: [loop, phase, P2c]
timestamp: 2026-08-25T21:33:49.260164-03:00
okf_version: "0.1"
---

# P2c — phase report

**Status**: done

Part of the [bundle](/index.md) · [plan](/plan.md) · [log](/log.md).

## Summary

P2c fechado — a fonte da oferta corrigida por MEDICAO. O braco nascera preso ao fuse T3 e mediu-se t3_turn_fused=0: o gatilho nao ocorre neste modelo de execucao porque o PostToolUse de cada chamada fecha o turno. A oferta passou a ser gravada tambem no deny code-mode (code_mode_gates), que e o que dispara de fato. record_route_offer virou escritor UNICO dos dois sitios. PROVA AO VIVO no binario implantado: deny -> offered_code=1; chamada intermediaria ilegivel NAO consumiu a oferta (correcao de ordem classificar-antes-de-reivindicar); touring run -> followed_code=1; t3_turn_fused permaneceu 0 o tempo todo. BLOQUEADOR achado no caminho (a partir da observacao de Gabriel sobre a sequencia de bashs): contadores de gate-metrics sao volateis — t3_turn_first_passed caiu 2->0 num restart — logo uma politica com piso de amostra nunca promoveria entre deploys. A decisao passou a ler evidencia DURAVEL por projeto (.claude/touring/code_mode_arm.json), provada em disco: {code:{offered:1,followed:1}}. Falha de escrita virou warn em vez de silencio.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/P2c.json](/knowledge/P2c.json).
