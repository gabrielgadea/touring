---
plan_id: 2026-08-28-work-outer
type: Strategy
title: "Hang do lifecycle + force-gate da corrida workspace — estratégia por evidência"
description: "Investigação instrumentada do hang intermitente do touring-dispatch::lifecycle e o gate force:true no handler cli-mutation-test, com as potencializações derivadas"
tags: [lifecycle, mutation-test, hang, force-gate, diagnostico]
timestamp: 2026-08-28T21:05:00-03:00
plan: /index.md
---

# Estratégia — hang do lifecycle e force-gate (28/08/2026, noite)

## Evidência coletada (OUTER)

| Fato | Fonte |
|---|---|
| 2 hangs reais: 32 threads `lifecycle::test` presas (3h46 na 1ª; foto via `/proc/<pid>/task/*/comm`) | sessão 28/08, janela 18h-21h |
| Ambas as ocorrências em janela de carga extrema (load 26) + restarts de daemon (propagação/deploy) | cronologia da sessão |
| Serializado (`--test-threads=1`): 1324/1324 em 21s, sempre verde | 3 execuções |
| Campanha de reprodução instrumentada: **15 rodadas paralelas, 0 hangs** (1 rodada lenta ambígua sob contenção do explore) | `scripts/diag_lifecycle_hang.sh` |
| Estático: lifecycle/ não tem lock global próprio; fixtures usam TempDir+HookRuntime isolados; `tantivy_stream` é try_send não-bloqueante; ENV_LOCK do reindex é função-local | leitura dirigida |

## Decisões

1. **Não aplicar fix cego**: root cause não flagrado (wchan) → mudar locks às cegas
   é o anti-padrão "correção do defeito lido, sem cura provada" (dois-locks-para-um-recurso).
2. **Converter hang futuro em falha nomeada**: suíte pesada do dispatch deve rodar
   via nextest (slow-timeout/terminate) ou `cargo test` com timeout externo — nunca
   cargo test cru sem teto. O instrumento de captura (`diag_lifecycle_hang.sh`)
   fica versionado: no flagra, fotografa threads/wchan/fds antes de matar.
3. **Force-gate no handler** (executado): payload sem `package` = corrida workspace
   de horas → recusa falante `workspace_requires_force` com os dois remédios; cache
   e cache_only intactos. Origem: teste unitário disparou corrida real no daemon.
4. **Probe = executor** (executado): `cargo_mutants_available` agora consulta
   `$CARGO_HOME/bin` como o cargo faz — o `binary_not_found` falso do daemon sem
   PATH especial morre.

## Convergência

CCE ledger converged: true (lente external marcada: doc do cargo sobre resolução
de subcomandos; ICE do gcc16 Arch). Gates: touring-cli 6/6 mutation + clippy;
hooks-core 21/21 mutation + clippy. Deploy final leva force-gate + probe ao
binário vivo com provas baratas (bare payload → recusa; corrida por pacote → intacta).
