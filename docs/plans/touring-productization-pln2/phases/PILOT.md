---
type: PhaseReport
title: PILOT — phase report
description: PILOT (D3) konverter per-project COMPLETO — 1ª instalação real end-to-end: touring toolchain init (~/.touring criado do zero) + install --fr
plan_id: unknown
tags: [loop, phase, PILOT]
timestamp: 2026-07-24T15:42:11.288017-03:00
okf_version: "0.1"
---

# PILOT — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

PILOT (D3) konverter per-project COMPLETO — 1ª instalação real end-to-end: touring toolchain init (~/.touring criado do zero) + install --from-source ~/projects/touring 30.3.0 (1ª toolchain versionada imutável, 4 bins) + default + init-project konverter (pin 30.3.0, bins linkados) + shim resolve project_bin em sessão real (TRACE, exit 0) + [daemon] per_project=true + daemon próprio auto-spawnado pelo hook DA TOOLCHAIN (exe pinado) + touring update com restart per-project + validate_pilot.sh 9/9 ALL PASS + daemon global intacto. 2 FINDINGS REAIS: (1) BUG CORRIGIDO — restart per-project herdava env do invocador (TOURING_PROJECT_ROOT/cwd do workspace de quem chamou → DBs no projeto errado, contaminação cruzada); fix: project_root_for_socket deriva root do próprio socket (<root>/.touring/daemon.sock) e spawn pina CLAUDE_PROJECT_DIR+TOURING_PROJECT_ROOT+cwd — correto por construção p/ todo caller (update/daemon-ctl); provado via /proc/<pid>/environ antes(root errado)/depois(root=konverter); unit 17/17 + redeploy + toolchain reinstalada --force. (2) doctor project_db é resolução CLIENT-side, não do daemon consultado — false-negative em diagnóstico multi-daemon; candidato a melhoria (daemon expor root próprio no health). Gotchas: sessão CC exporta TOURING_DAEMON_SOCKET (precedência sobre walk-up — testar com env -u); component list mostra *.old do dev dir (cosmético). CO-EVOLUÇÃO CLAUDE.md 3 camadas: criado ~/projects/touring/CLAUDE.md (fonte canônica: update-touring, ponte from-source, gates, débitos); reescrita seção Touring do konverter/.claude/CLAUDE.md (drift 2-gerações 'symlink to analise' → tabela per-project pin/lock/bins/daemon/dados + operação); delta da constituição root ~/.claude/CLAUDE.md PROPOSTO aguardando aprovação de Gabriel (modelo 4 camadas + regra de resolução + rule nova touring-per-project).

## Knowledge

Typed abstract: [/knowledge/PILOT.json](/knowledge/PILOT.json).
