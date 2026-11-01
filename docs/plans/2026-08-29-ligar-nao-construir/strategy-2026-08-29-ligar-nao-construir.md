---
type: Strategy
title: Ligar, não construir — R1-R6 do ciclo de aprendizado
description: Fechar os furos medidos na auditoria 29/08 usando estruturas já compiladas em crates/ (REGRA #0 — ligar, não construir).
plan_id: 2026-08-29-ligar-nao-construir
tags: [strategy, rl, learning, memory, kpi]
timestamp: 2026-08-29T13:30:00-03:00
okf_version: "0.1"
---

# Estratégia — Ligar, não construir (R1-R6)

## Origem

Auditoria da inteligência (29/08, ordem de Gabriel): registro ✅ forte, processamento
✅ compondo (outcome_reward 1,70%→7,87% em 4 dias; negativos 16→154), **uso-pelo-motor
⚠️ é o gargalo**. Memória: `assessment:ciclo-aprendizado-touring:2026-08-29`.
Gabriel aprovou as 6 recomendações verbatim via `/loop-engineering` (HUMAN GATE do
passo 9 cumprido pela invocação).

## As 6 fases (ordem por dependência e valor)

| Fase | R | Entrega | Critério de aceite (medível) |
|---|---|---|---|
| P1 | R2 | Meters STUB duráveis: `arm_native`/`arm_both`/`inspect_burst_share`/`pillar_induction_ratio` leem DISCO como o braço `code` (`code_mode_arm.json` é o template) | os 4 KPIs saem de STUB após 1 evento + restart do daemon não zera a evidência |
| P2 | R1 | Produtor do canal de resolução de gotcha (verbo `resolve` alimentando o que `touring.gotcha.resolution` lê) | KPI sai de 0.0 ao resolver 1 gotcha real; resolução persiste restart |
| P3 | R6 | `learning status` truncado para shape+norma (sem 40KB de pesos) | saída < 2KB; campos shape/norm presentes; nenhum consumidor quebra |
| P4 | R5 | Re-rank eleva lição curada: calibrar `rerank_by_case_value` / peso de `lesson`/`gotcha` curados | `curated_recall_share` sobe em medição A/B local (recalls de amostra) |
| P5 | R3 | Negativos agrupados por CONTEXTO (`adw_gate_reject:<nó>`); se família ≥ ~30 → 1º ciclo `dspy_compile` real; senão, critério monitorável entregue | medição publicada; ciclo DSPy rodado OU gatilho documentado com query |
| P6 | R4 | `experiment_log` ligado ao A/B do `variant_archive` (o registro de experimento deixa de ter 1 chamador) | 1 experimento real gravado e legível de volta |

## Princípios de execução

- **REGRA #0**: cada fase LIGA estrutura existente; nenhuma peça nova de infraestrutura.
- **D8**: onde houver texto declarando comportamento, o executor prova (teste cruzado).
- **Fail-direction**: evidência durável em disco nunca inventa valor; ausência fica ausente (E4).
- Gates por fase: cargo test crates tocados + clippy -D warnings + prova comportamental viva.
- Convergência: `loop_converged.py` exit 0 (Lei L2), nunca autoavaliação.

## Fontes externas (lente external do ledger)

Herdadas do bundle autoresearch (rodada 2, 25/08): Context7
`/nousresearch/hermes-agent-self-evolution` (laço 6 passos DSPy+GEPA);
`/websites/dspy_ai` (métrica `dspy.Prediction(score, feedback)`, piso 30-300
exemplos); survey MSR ~300 papers (autonomia exige verificador determinístico
independente).

## Ligações

- Diagnóstico: [/diagnostics/](diagnostics/) · Ledger CCE: `.touring-explore/ligar-nao-construir-r1-r6-aprendizado.ledger.json` (converged)
- Plano-mãe do motor: `docs/plans/2026-08-25-autoresearch-rl-intelligence/` (P4/P5 pendentes lá são desbloqueados por P5 daqui)
