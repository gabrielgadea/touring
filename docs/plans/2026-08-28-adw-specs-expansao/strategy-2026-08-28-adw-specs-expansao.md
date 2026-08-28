---
type: Strategy
title: "Expansão do portfólio de specs ADW — o modus operandi como comando"
description: "6 specs novos codificam processos que eram manuais: release-gate, memory-curation, exercise-idle-infra, code-mode-adherence, adw-curation, guard-sweep"
tags: [adw, specs, portfolio, code-mode, guards, curation]
timestamp: 2026-08-28T09:35:00-03:00
plan: /log.md
---

# Expansão do portfólio ADW (28/08/2026)

> Ordem de Gabriel: "expanda o escopo das specs do touring para potencializar todo o
> modus operandi do touring, do code mode, do próprio adw e do claude code".
> OUTER: strategy-loop + explore converged (13→0→0→1→0→0) + lente externa
> (portfolio 6×, molde error-teach, LangGraph da wave-mãe). Prior-art: 6/6 sem
> candidato acima do piso → `create_new`, todos registrados com
> `touring portfolio verdict`.

## O critério

Cada spec novo codifica um processo do modus operandi que ATÉ HOJE dependia de alguém
lembrar — a tese da adoção estrutural (touring-4-pillars: "adoption does not emerge
from availability; masters são os nós de código dos ADWs"). Nada half-baked: todos com
`[purpose]` completo (`when_not_to_use` incluso), contratos `FACT=`/`VERDICT=`,
lint 0 erros, e evidência comportamental de estreia.

## Os 6 specs (domínio → o manual que vira comando)

| Spec | Domínio | Processo codificado | Estreia (evidência) |
|---|---|---|---|
| `release-gate` | touring | check+clippy+e2e+elite com veredito por gate e PAUSA HUMANA antes do deploy (o deploy segue com update-touring/propagate-release.sh) | **completed** — 4 gates verdes |
| `memory-curation` | touring/TACO | memórias antigas verificadas CONTRA O CÓDIGO (incidente lint-cycles: 10 dias stale guiando decisão); veredito stale/valid/resolved por memória, sem edição automática | run real (workhorse, read-only) |
| `exercise-idle-infra` | meta | a lição que apareceu 3× numa wave (ZTE, router_accuracy, plan_refine_iters): infra com uso zero ganha exercício de estreia proposto por item, safe/colateral | run real (light) |
| `code-mode-adherence` | code mode | régua M1 → diagnóstico por failure_kind → calibração proposta com a diretriz E/A/M citada | run real (workhorse) |
| `adw-curation` | ADW (meta) | a F5 da wave-mãe como comando: diff library↔local, evidência de runs, lint 100%, vereditos promover/etiquetar com gate humano | **completed** — vereditos reais (pegou dependências locais que a triagem manual não pesou) |
| `guard-sweep` | Claude Code/CI | guard-existe-mas-não-roda: descobrir e rodar 100% dos guards + denunciar os fora-do-CI | **completed** — a estreia achou **3 guards falhando** e forçou as correções |

## O que a estreia do guard-sweep corrigiu no repo (REGRA #21)

1. 6 testes de rajada (`cli_suggester_tests.rs`) sem `serial(t3_env)` — anotados
   (3 novos, 3 com o segundo lock multi-key).
2. O guard serial não parseava a forma multi-key legítima `serial(a, b)` —
   parser corrigido (`_serial_groups`; verificador-usa-menos-que-o-extrator).
3. `CLAUDE.md` citava tools MCP que o guard phantom lia como crate — marker de
   proveniência movido para dentro da janela de contexto.

## Residuais conscientes

- Vereditos do adw-curation (promover instrument-first/freshness-audit/strategy-loop/
  chore; re-etiquetar error-teach e release-gate como locais por dependências deste
  repo) são PROPOSTAS — a aplicação é decisão de Gabriel na próxima curadoria.
- `xaudit-gates` parametrizável segue no backlog da wave-mãe.
