---
type: PhaseReport
title: SEG-1: UDP seccomp pre_exec + memoria hibrida (RLIMIT_AS 25% + cgroup qdo delegado) + read-narrowing roots enumerados c/ gate compat — phase report
description: SEG-1 done (3/3 medidos ao vivo): (1) UDP deny no KERNEL via seccompiler (build_udp_deny_bpf em enforce_linux + apply_seccomp_udp_to no funi
plan_id: 2026-08-27-code-mode-aderencia-sandbox
tags: [loop, phase, SEG-1: UDP seccomp pre_exec + memoria hibrida (RLIMIT_AS 25% + cgroup qdo delegado) + read-narrowing roots enumerados c/ gate compat]
timestamp: 2026-08-27T23:08:57.556229-03:00
okf_version: "0.1"
---

# SEG-1: UDP seccomp pre_exec + memoria hibrida (RLIMIT_AS 25% + cgroup qdo delegado) + read-narrowing roots enumerados c/ gate compat — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

SEG-1 done (3/3 medidos ao vivo): (1) UDP deny no KERNEL via seccompiler (build_udp_deny_bpf em enforce_linux + apply_seccomp_udp_to no funil; kill switch TOURING_SANDBOX_SECCOMP_DISABLED) — teste e2e prova PermissionError na CRIACAO do socket; X6 lexico continua na frente. (2) Memoria: RLIMIT_AS 80% do RAM fisico (leitura /proc/meminfo) — a 1a tentativa (25%) matou o deno (V8 CagedHeap ~40GB VA; medido: morre <=32GB, passa 48GB); 80% passa deno e contem leak infinito; cgroup NAO wired (systemd-run lancaria fora da arvore do filho, fora do pre_exec — documentado). (3) Read-narrowing: roots enumerados (sistema + mise/cargo/rustup + ~/.claude/touring + ~/projects) com kill switch TOURING_SANDBOX_READ_WIDE=1; gate compat 9/9 (python+numpy/pandas via venv, node, ts/deno, bash, ruby, perl, rustc); credenciais fechadas (~/.gnupg FECHADO medido). Suites ceg+server 2282 verdes.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/SEG-1: UDP seccomp pre_exec + memoria hibrida (RLIMIT_AS 25% + cgroup qdo delegado) + read-narrowing roots enumerados c/ gate compat.json](/knowledge/SEG-1: UDP seccomp pre_exec + memoria hibrida (RLIMIT_AS 25% + cgroup qdo delegado) + read-narrowing roots enumerados c/ gate compat.json).
