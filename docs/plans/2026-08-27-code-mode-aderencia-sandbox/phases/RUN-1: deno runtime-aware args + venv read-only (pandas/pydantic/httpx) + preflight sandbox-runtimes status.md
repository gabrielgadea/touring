---
type: PhaseReport
title: RUN-1: deno runtime-aware args + venv read-only (pandas/pydantic/httpx) + preflight sandbox-runtimes status — phase report
description: RUN-1 done: (1) deno runtime-aware (eval/eval --ext=ts medidos ao vivo; resolve_language_args_for + resolve_args_with_runtime; wrapper compa
plan_id: 2026-08-27-code-mode-aderencia-sandbox
tags: [loop, phase, RUN-1: deno runtime-aware args + venv read-only (pandas/pydantic/httpx) + preflight sandbox-runtimes status]
timestamp: 2026-08-27T22:27:29.858327-03:00
okf_version: "0.1"
---

# RUN-1: deno runtime-aware args + venv read-only (pandas/pydantic/httpx) + preflight sandbox-runtimes status — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

RUN-1 done: (1) deno runtime-aware (eval/eval --ext=ts medidos ao vivo; resolve_language_args_for + resolve_args_with_runtime; wrapper compat sem quebrar API; LANGUAGE_CANDIDATES fonte única); TS executa via deno (ts-deno: 42). (2) venv read-only: setup-venv no host instalou pandas 3.0.5/pydantic 2.13.4/httpx 0.28.1 (py 3.14.7); sandbox monta via PYTHONPATH fora dos write roots; pandas importa DE DENTRO (prova). (3) preflight touring sandbox-runtimes status: 7/11 resolvidos com path, ausentes nomeados (fail-LOUD, A12). Suites ceg+server 2280 verdes; 2 testes novos (deno args, preflight).

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/RUN-1: deno runtime-aware args + venv read-only (pandas/pydantic/httpx) + preflight sandbox-runtimes status.json](/knowledge/RUN-1: deno runtime-aware args + venv read-only (pandas/pydantic/httpx) + preflight sandbox-runtimes status.json).
