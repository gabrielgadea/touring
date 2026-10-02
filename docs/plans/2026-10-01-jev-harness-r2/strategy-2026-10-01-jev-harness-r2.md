---
type: Strategy
title: Estratégia r2 — do aluno sub-confiante à cascata calibrada em produção
description: Rodada 2 do ciclo Jev (NotebookLM 5 consultas + docs oficiais relidos + CCE convergido + scan da 02). Dez frentes W1-W10, cada uma ancorada num número medido do bundle v1 e num padrão oficial da documentação. A doc nomeia nossa condição (sub-confident calibration drift) e prescreve os remédios por preço: temperature scaling (grátis) → shadow-matrix (medição) → corpus ao piso (1.000) → starvation gate → composite scoring → score-gate de recall.
plan_id: 2026-10-01-jev-harness-r2
tags: [estrategia, jev, laya, calibracao, r2]
timestamp: 2026-10-01T19:58:00-03:00
okf_version: "0.1"
---

# Estratégia r2 — do aluno sub-confiante à cascata calibrada em produção

Parte do [bundle](/index.md). OUTER da rodada 2: [diagnostic](/diagnostics/touring-20261001T195317.md)
· ledger CCE convergido (2 rodadas secas) · pesquisa: NotebookLM (5 queries) +
docs.typesafe.ai (api/cookbooks relidos) + scan harness da touring-02.

## O que a rodada 1 construiu — e o que a telemetria acusa [F]

| Peça | Número medido | Leitura |
|---|---|---|
| Aluno laya (W4.2) | acc 0,926@τ0,6 · **suporte 22,5%** · p50 9,9ms | **sub-confident calibration drift** — a doc nomeia: "queda de suporte (ex.: 80%→22,5%) indica deriva sub-confiante" |
| keyword_fallback_rate | 0,9375 day-0 (braço OFF por opt-in) | o critério de rollback da doc (>0,95) se ARMA quando Gabriel ligar a flag — não está violado, está vigiado |
| recall | **242 candidatos, 0 injetados** (curated_share 0,0) | o gargalo é RECUPERAÇÃO (s54: 21/39 ausentes do recall), não ordenação |
| corpus G1 | 800 prompts | **abaixo do piso documentado (~1.000)** — caso oficial: 51,3%→79,4% com 1k |
| decision journal | latência `None` | campo exigido pela doc p/ SLA+rollback — ausente |
| pre_review_triage | 7 Nouls advisory | funciona; a doc recomenda **Score+pesos em código** para code-review |

## As dez frentes (cada uma: número → padrão oficial → ação)

### W1 — Temperature scaling para suporte [P0 · o mais barato]
A curva é sub-confiante: logit drift pós-fine-tune achata a distribuição (acerta mas
devolve 0,48–0,58 e falha o τ). Remédio oficial: **UM escalar T no val split**;
sub-confiança ⇒ T<1 afia e **sobe suporte sem retreinar**. `w43_calibrate.py` já tem
`fit_temperature` (LBFGS) — mudar o objetivo de "min ECE" para **max suporte@τ s.t.
acc ≥ 0,90**. [INFERENCE 0,75: suporte 22,5%→40–60% na mesma acurácia]

### W2 — Shadow-matrix (evidência sem ligar o braço) [P0 · medição pura]
Logar o que o aluno TERIA dito em todo prompt (mesmo abaixo do τ), `source:"laya_shadow"`.
Produz a matriz de concordância + a distribuição de confiança dos que caíram —
distingue "τ errado" de "modelo incerto" (a bifurcação da 02). Zero mudança de
comportamento (keyword segue). [FACT: infra do journal existe; delta pequeno em prompt_enhance.rs]

### W3 — Latência no decision journal [P0 · campo ausente]
`journal_intent_decision` ganha `latency_ms` (o KPI hoje devolve None — 02 mediu).
A doc exige o campo para SLA e rollback por latência (>500ms = critério). Pequeno.

### W4 — Corpus 800→1.200 no piso documentado [P1]
+400 rótulos teacher sobre prompts de PRODUÇÃO (o G1 veio de memórias — a deriva
mora na distribuição real). Custo em cascata: Jev rotula, professor Claude só nos
casos de desacordo (a doc: "uma tarde de rotulagem"). Depois: retreino + W1 de novo.

### W5 — Starvation gate armado [P1 · o critério vira código]
Quando Gabriel ligar `TOURING_LAYA_INTENT`: o KPI `decision` vigia `keyword_fallback_rate`;
**>0,95 após 7 dias COM a flag → o critério documentado de rollback dispara** (reporta
a Gabriel, nunca silêncio). Lei L2: o critério mora em código, não em revisão.

### W6 — Composite scoring semântico no pre-review [P1]
A doc: *"o modelo pontua, o código decide"* — 3 Scores (security risk 0–2, pattern
alignment, diff quality) compostos EM CÓDIGO com pesos em `touring.toml`, log por
dimensão (auditável, tuning sem re-prompt). Estende o triage atual (Nouls) com a
forma oficial de code-review.

### W7 — "Uncertain" como outcome de primeira classe [P2]
O cookbook novo (self-consistency): incerteza é um destino de roteamento
(humano/Sistema 2), não um binário forçado. Aplicar ao intent e ao futuro guardrail.

### W8 — Recall: do rerank ao score-gate [P1 · o gargalo real]
curated_share 0,0 (242/0) + 21/39 ausentes do recall. O cookbook "Classifying RAG
passages": **score por passagem, o código decide o que injeta**. Atacar a
recuperação (o recall nem retorna os candidatos) antes de qualquer rerank.

### W9 — Hierarquia só se >20 rótulos [P2 · restrição permanente]
Flat choice degrada após ~20–50 opções (77 flat → 0,425). O intent (8 classes) e o
memory_kind (~10) estão seguros; se kinds crescerem → beam-search hierárquico.
Documentado como invariante do trainer.

### W10 — Bash gate semântico (CEG × aluno) [P2]
O caso canônico da doc ("gate the bash tool"): comandos que o pattern-matcher não
classifica (formas novas) recebem reversibility Score do ALUNO (local, ~10ms) →
advisory no CEG. Fecha o buraco dos desconhecidos sem custo de API.

## Sequenciamento (preço × janela)

1. **W1+W2+W3 esta semana** — antes da regeneração do w45 em **08/10** (chegam medidos no relatório de 7 dias).
2. **W5** junto com o opt-in de Gabriel (`TOURING_LAYA_INTENT`).
3. **W4** na próxima wave de treino (corpus → retreino → W1 de novo).
4. **W6/W8/W10** como features da wave seguinte (cada uma vira ticket do DAG).

## Critérios de rollback (da doc, agora nomeados no bundle)

FP em alta confiança > 2–5% · starvation > 0,95 pós-ajuste com a flag ligada ·
deriva de ECE (bin-a-bin sobre ground-truth de overrides) · latência > 500ms ou
falhas repetidas de API. Qualquer um → reportar a Gabriel com o número, nunca degradar em silêncio.

## Estado do OUTER (Lei L3 — artefatos)

- Diagnostic: `diagnostics/touring-20261001T195317.md` ✓
- Ledger CCE: convergido (2 rodadas secas; "NOT a claim of completeness") ✓
- Esta estratégia ✓ → **HUMAN GATE**: aprovar W1-W10 (ou subconjunto) para o plano+DAG.
