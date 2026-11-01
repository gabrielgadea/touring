---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-27-code-mode-aderencia-sandbox
tags: [loop, log]
timestamp: 2026-08-27T21:31:07.764961-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-27T22:27:29.858327-03:00 — RUN-1: deno runtime-aware args + venv read-only (pandas/pydantic/httpx) + preflight sandbox-runtimes status done

RUN-1 done: (1) deno runtime-aware (eval/eval --ext=ts medidos ao vivo; resolve_language_args_for + resolve_args_with_runtime; wrapper compat sem quebrar API; LANGUAGE_CANDIDATES fonte única); TS executa via deno (ts-deno: 42). (2) venv read-only: setup-venv no host instalou pandas 3.0.5/pydantic 2.13.4/httpx 0.28.1 (py 3.14.7); sandbox monta via PYTHONPATH fora dos write roots; pandas importa DE DENTRO (prova). (3) preflight touring sandbox-runtimes status: 7/11 resolvidos com path, ausentes nomeados (fail-LOUD, A12). Suites ceg+server 2280 verdes; 2 testes novos (deno args, preflight).

## 2026-08-27T23:08:57.556229-03:00 — SEG-1: UDP seccomp pre_exec + memoria hibrida (RLIMIT_AS 25% + cgroup qdo delegado) + read-narrowing roots enumerados c/ gate compat done

SEG-1 done (3/3 medidos ao vivo): (1) UDP deny no KERNEL via seccompiler (build_udp_deny_bpf em enforce_linux + apply_seccomp_udp_to no funil; kill switch TOURING_SANDBOX_SECCOMP_DISABLED) — teste e2e prova PermissionError na CRIACAO do socket; X6 lexico continua na frente. (2) Memoria: RLIMIT_AS 80% do RAM fisico (leitura /proc/meminfo) — a 1a tentativa (25%) matou o deno (V8 CagedHeap ~40GB VA; medido: morre <=32GB, passa 48GB); 80% passa deno e contem leak infinito; cgroup NAO wired (systemd-run lancaria fora da arvore do filho, fora do pre_exec — documentado). (3) Read-narrowing: roots enumerados (sistema + mise/cargo/rustup + ~/.claude/touring + ~/projects) com kill switch TOURING_SANDBOX_READ_WIDE=1; gate compat 9/9 (python+numpy/pandas via venv, node, ts/deno, bash, ruby, perl, rustc); credenciais fechadas (~/.gnupg FECHADO medido). Suites ceg+server 2282 verdes.

## 2026-08-27T23:30:57.614215-03:00 — NET-1: --allow-net-port (Landlock NetPort) + residual host documentado done

NET-1 done, provado ponta-a-ponta com rede real: (1) --allow-net-port <p> (repeatable, clap Append) -> RunTunables.allow_net_ports -> SandboxConfig.allow_net_ports -> apply_landlock_to passa ao builder (NetPort rules; vazio=deny-all intacto). (2) gate_run waiver: deny cuja unica razao e network(+subprocess do proprio cliente) vira advisory nomeando a concessao; X2 static_blocked nunca dispersa (curl+rm continua duro). Predicado corrigido por medicao: curl e network+subprocess. (3) Ao vivo: curl :443 com flag = HTTP 200 com advisory; curl :80 com flag de 443 = exit 7 (kernel nega); sem flag = deny-duro. Teste novo net1_allow_net_port + suites 2283 verdes.

## 2026-08-27T23:48:47.121768-03:00 — PreCompact snapshot

Loop active. Pending: [SDK-1: orchestrate Node SDK + sub-calls paralelas (pool 10) + TOURING_SCRATCH_DIR + MED-1 régua aderência + escada trust thresholds,OUT-1: streaming de saída,DOC-1: diretrizes E/A/M publicadas em docs/code-mode.md + A14 cap-3-retries + strategy v-final + memory]. Resume: `touring decompose ready task_1787877917778583270`.

## 2026-08-28T08:10:34.790641-03:00 — PreCompact snapshot

Loop active. Pending: [SDK-1: orchestrate Node SDK + sub-calls paralelas (pool 10) + TOURING_SCRATCH_DIR + MED-1 régua aderência + escada trust thresholds,OUT-1: streaming de saída,DOC-1: diretrizes E/A/M publicadas em docs/code-mode.md + A14 cap-3-retries + strategy v-final + memory]. Resume: `touring decompose ready task_1787877917778583270`.
