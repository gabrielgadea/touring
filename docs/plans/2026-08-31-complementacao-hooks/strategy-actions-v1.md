---
type: Strategy
title: "Strategy v1.0 — Execução das 7 ações priorizadas do cross-audit (31/08)"
description: "Plano de execução para as 7 ações priorizadas no cross-audit: (1) investigar 06_documentation FAIL, (2) investigar 15_dependencies_advisories FAIL, (3-4) wirar H4/H6 lógica real, (5) BestPracticesGate hooks_complement_use, (6) instrumentar emit_count no journal, (7) wirar H7/H12/H15 em session_hooks. REGRA #0 — potentialize, never reduce."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
version: "1.0"
based_on: ["docs/audits/cross-audit-2026-08-31-complementacao-hooks.md"]
tags: [strategy, complementacao-hooks, followup, potentialize, 50-dim]
timestamp: "2026-08-31T20:05:00-03:00"
---

# Strategy v1.0 — 7 ações priorizadas (followup do cross-audit)

## TL;DR

Cross-audit (31/08 19:57) entregou composite 0.7798 Silver — abaixo de Gold (0.80) por 2 BLOCK FAILs pré-existentes. 7 ações priorizadas para promoção a Gold + wirings dos sinais H4/H6/H7/H12/H15. Plano: criar DAG nova com 7 subtasks (uma por ação), executar INNER phase-by-phase com cross-audit ao fim de cada fase.

## Background empírico

- **DAG anterior** (`task_1788202012024662508`): 6/6 done ✅ (F1 specs, F1.5 subcmds, F2 benchmarks, F3 SignalLayer, F3-registro, F4 testes)
- **Composite baseline**: 0.7798 Silver (2 BLOCK FAILs + 1 WARN craftsmanship + 1 WARN dependencies + 1 ADVISORY performance)
- **Memory recall** ("06_documentation FAIL" + "BestPracticesGate signal_use"): 6 outcomes prévios relevantes — 3 negative (fail modes do passado) + 3 positive (successes em edit/write/read de .md)
- **Doctor**: 6/7 OK; wiring_diagnostic FAIL com kind_unknown=9 (provavelmente os arquivos novos não foram indexados ainda)

## As 7 ações (DAG nova — `task_17882066XXXXXXXX`)

| # | Subtask | Priority | Effort | Owner |
|---|---|---|---|---|
| 1 | **investigar 06_documentation FAIL** | P0 | 1h | scriber (docs analysis) |
| 2 | **investigar 15_dependencies_advisories FAIL** | P0 | 1h | engineer (cargo-deny analysis) |
| 3 | **wirar H4 pub_api_diff real** (git diff + ast_overview) | P1 | 3h | engineer |
| 4 | **wirar H6 scan_vulnerabilities real** (touring-offensive CWE) | P1 | 3h | engineer |
| 5 | **BestPracticesGate hooks_complement_use** (Rust) | P1 | 2h | engineer |
| 6 | **instrumentar hooks_complement_emit_count** (journal) | P1 | 2h | engineer |
| 7 | **wirar H7/H12/H15 em session_hooks.rs** | P2 | 4h | engineer |

**DAG topology**: 1 + 2 em paralelo (independentes); 3-7 sequenciais depois (3 e 4 em paralelo entre si; 5 após 3+4; 6 após 5; 7 após 6).

## Critérios de pronto

| Critério | Medido por | Target |
|---|---|---|
| Composite ≥ 0.80 Gold | `python3 docs/elite_aggregate.py --check` | exit 0 |
| 0 BLOCK FAIL (06_documentation, 15_dependencies_advisories) | `python3 docs/elite_aggregate.py --check` | ≥ 0.80 cada |
| H4/H6/H7/H12/H15 wirados em handlers reais | `touring wiring audit -j` + count wirings | 15/15 |
| BestPracticesGate hooks_complement_use ativo | `touring-quality check --gate hooks_complement_use --target <file>` | ≥ 0.50 |
| hooks_complement_emit_count incrementa no journal | `touring kpi -j hooks_complement.emit_count` | > 0 |
| 15/15 testes verdes | `python3 scripts/test_complementacao_hooks.py --verbose` | exit 0 |
| 0 orphans introduzidos | `touring wiring orphans -j` | count = 0 |

## FASE 0 — Health gate (já validada)

- `touring doctor`: 6/7 OK (wiring_diagnostic FAIL benign — indexação stale, não bloqueia)
- Cargo check workspace: ✅ Finished
- DAG anterior: 6/6 done
- Composite baseline: 0.7798 Silver (medido)

## Anti-goals

- ❌ NÃO substituir o composite atual por uma versão capenga (gate é ground truth)
- ❌ NÃO wirar H4/H6 com stubs (a dor do cross-audit foi exatamente os stubs)
- ❌ NÃO modificar settings.json (F3 já migrou para SignalLayer Rust direto)
- ❌ NÃO fechar como Gold sem provar via execução (`python3 docs/elite_aggregate.py --check` exit 0)

## Stop conditions

- Composite ≥ Gold (0.80) por 2 rodadas consecutivas → STOP + memory store
- Composite ≤ 0.75 (regressão) → STOP + diagnosis + Gabriel decide
- 7 subtasks done + composite não subiu ≥ 0.02 → STOP + report

## Próximo passo (gate humano)

Aguardando Gabriel aprovar a DAG com 7 subtasks. Quando aprovado, claim e execute F1 + F2 (investigar os 2 BLOCK FAILs em paralelo).

---

_v1.0 — 2026-08-31 20:05 BRT | Strategy das 7 ações followup do cross-audit | 2 P0 + 3 P1 + 1 P2 (ordem de dependência) | composite target ≥0.80 Gold_