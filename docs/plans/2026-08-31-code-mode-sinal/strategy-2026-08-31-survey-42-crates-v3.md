---
type: Strategy
title: "Strategy v3.0 — code-mode-sinal-canal (F3.0 final)"
description: "Estratégia final pós-validação F3.0 (5 cenários reais). 17 MUST + 23 SHOULD + 17 NICE + 2 gaps. F2 Opção A com MUST-have primeiro."
plan_id: 2026-08-31-code-mode-sinal
bundle: docs/plans/2026-08-31-code-mode-sinal
okf_version: "0.1"
version: "3.0"
timestamp: "2026-08-31T15:32:35-03:00"
---

# Strategy v3.0 (F3.0 final)

## TL;DR

5 marcos de exploração concluídos:
- F1.5 cadeia 7-camadas
- F1.6 5 bridges (2325 LOC)
- F1.7 9 crates (30 funções SDK)
- F1.8 42 crates (45 funções)
- F1.9 19 restantes (57 funções)
- F2.0 foundation::code_mode investigation
- F3.0 validação prática 5 cenários (17 MUST + 23 SHOULD + 17 NICE + 2 gaps)

## Recomendação F2

**Opção A**: `crates/touring-code/src/sdk/` (~650 LOC)
- `types.rs`: 17 MUST-have tipadas
- `stub.py.j2`: Protocol Python
- `gen.rs`: generator do `run_journal.jsonl`

**Decisão aguardando**: Gabriel aprovar ou propor alternativa.

## Status final

| Marco | Resultado |
|---|---|
| Briah | ratio 1.0 |
| Yetzirah v1.0+v1.1 | 4 decisões aplicadas |
| F1.5-F1.9 + F2.0 | cadeia + bridges + 42 crates + 57 funções SDK + REUSAR foundation |
| F3.0 | validação prática: 17 MUST + 23 SHOULD + 17 NICE + 2 gaps |
| DAG F1 | done (quality 0.92) |
| DAG F2-F6 | pending (priority 64) — aguardam code work |

## Próximo passo

Gabriel, escolha:
1. Aprovar F2 Opção A + MUST-have (recomendado)
2. Investigar paralelos externos
3. Parar entrega

---

_v3.0 — 2026-08-31 15:32 BRT | Strategy final pós-F3.0_