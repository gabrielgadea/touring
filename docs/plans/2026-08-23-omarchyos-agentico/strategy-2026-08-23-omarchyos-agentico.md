---
type: Strategy
title: OmarchyOS Agêntico — estratégia consolidada
description: Intenção, objetivos mensuráveis, princípios e limites do programa que funde o Agentic OS (ARMS) com o Omarchy 4.0 Quattro, Touring e Herdr em disco dedicado.
plan_id: 2026-08-23-omarchyos-agentico
tags: [strategy, omarchy, agentic-os, loop-engineering]
timestamp: 2026-08-23T13:12:00-03:00
okf_version: "0.1"
---

# OmarchyOS Agêntico — estratégia

Part of the [bundle](/index.md). Plano executável: [plan.md](/plan.md). Análise de origem (3 rodadas): [`/docs/2026-08-23-agentic-os-omarchy-fusao.md`](../../2026-08-23-agentic-os-omarchy-fusao.md) §9.

## Intenção

Um único sistema em que **o agente é um só** (Claude Code com as skills/rules/memória do TACO), as **superfícies são várias** (shell Quickshell do Omarchy, menu, barra, Herdr, timers, Routines na nuvem) e o **sistema nervoso é o Touring** (índice, memória, ADW, convergência). O Omarchy entra como substrato AI-first que já cobre ~60 % das afordâncias do ARMS; o que falta é *glue* versionado em `client/omarchy/`.

## Objetivos (mensuráveis — §5 do plano)

1. `validate_all.sh` 9/9 exit 0 no Omarchy instalado em `KP102L1HDJDW`, com o Pop!_OS intocado em `KP102L1H5AWP`.
2. `touring doctor` 7/7 e `touring e2e ≥ 0.85` **no Omarchy** (baseline do Pop: 0.854), memória Touring preservada (597 MB de DBs viajam no kit).
3. Skills deck: 1 botão (menu e widget) gera artefato + linha em `~/Work/runs.log`.
4. Rotinas: ≥ 5 timers `systemd --user` + 1 Routine cloud disparada com o laptop fechado.
5. ADW × Herdr: 1 fluxo com 2 ramos termina `done` com `pane read` no journal.
6. Memória: teste cego 3/3 pelo router `~/Work/CLAUDE.md`.
7. Command centre (3 widgets) sobrevive a reboot; 30 dias sem voltar ao Pop por necessidade.

## Princípios (herdados da v3, §9.1) + 2 novos

1. Um agente, várias superfícies; Touring = sistema nervoso. 2. *Show, don't store* = Lei L3 (evidência em disco). 3. Afordância > persuasão. 4. Deny-by-default (`--permission-mode auto` no launcher, `acceptEdits` em headless, nunca `bypass` no deck). 5. Bottom-up. 6. Isolamento físico (dois NVMe). 7. Reprodutível (`client/omarchy/`, `cidata` sem segredos, plugin em git).
8. **Front-load o reversível**: tudo que pode ser construído e validado no Pop acontece antes do único passo irreversível (P0 → D1 → P1).
9. **Um executor, três superfícies**: `omarchy-skill-run` é chamado pelo menu, pelo widget, pelos timers e pelo ADW — métricas e logs únicos.

## O que NÃO fazer

Identificar discos por `nvme0/1` · preservar Windows em partição (Docker/KVM cobre) · converter `.deb` do Claude Desktop · `bypassPermissions` por padrão · editar `$OMARCHY_PATH` · `sudo` em scripts de background (Quattro usa `pkexec`) · Hermes/OpenClaw como 2º cérebro · comprar RUBRIC para começar · copiar `~/.claude/plugins` (801 MB de cache) ou `~/.claude/projects/*.jsonl` · versionar `cidata` com `disk_encryption`.

## Decisões abertas que gatilham fases

D1 `wipefs` (→ P1) · D2 username `gabrielgadea` (→ P0) · D3 passphrase/GPU (→ P1) · D4 repo do plugin (→ P5.4) · D5 rotinas cloud (→ P7.3) · D6 Pop após 30 d (→ CLOSE) · D7 `analise` + hooks legados (→ P3.2). Detalhe e defaults: plano §8.
