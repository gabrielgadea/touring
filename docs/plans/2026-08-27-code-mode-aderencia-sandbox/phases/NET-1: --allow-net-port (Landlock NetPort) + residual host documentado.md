---
type: PhaseReport
title: NET-1: --allow-net-port (Landlock NetPort) + residual host documentado — phase report
description: NET-1 done, provado ponta-a-ponta com rede real: (1) --allow-net-port <p> (repeatable, clap Append) -> RunTunables.allow_net_ports -> Sandbo
plan_id: 2026-08-27-code-mode-aderencia-sandbox
tags: [loop, phase, NET-1: --allow-net-port (Landlock NetPort) + residual host documentado]
timestamp: 2026-08-27T23:30:57.614215-03:00
okf_version: "0.1"
---

# NET-1: --allow-net-port (Landlock NetPort) + residual host documentado — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

NET-1 done, provado ponta-a-ponta com rede real: (1) --allow-net-port <p> (repeatable, clap Append) -> RunTunables.allow_net_ports -> SandboxConfig.allow_net_ports -> apply_landlock_to passa ao builder (NetPort rules; vazio=deny-all intacto). (2) gate_run waiver: deny cuja unica razao e network(+subprocess do proprio cliente) vira advisory nomeando a concessao; X2 static_blocked nunca dispersa (curl+rm continua duro). Predicado corrigido por medicao: curl e network+subprocess. (3) Ao vivo: curl :443 com flag = HTTP 200 com advisory; curl :80 com flag de 443 = exit 7 (kernel nega); sem flag = deny-duro. Teste novo net1_allow_net_port + suites 2283 verdes.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/NET-1: --allow-net-port (Landlock NetPort) + residual host documentado.json](/knowledge/NET-1: --allow-net-port (Landlock NetPort) + residual host documentado.json).
