---
type: ExperimentReport
title: W4.5 — relatório de produção do intent→CILA (day-0 + protocolo de 7 dias)
description: Baseline day-0 medido em produção (30.4.88/89): intent journal com rotas, 1.178 chamadas Jev na janela, recall 93/0. O gerador (scripts/w45_production_report.py) é a prova de instrumento; o relatório de 7 dias é o mesmo comando sobre a janela completa, com o braço laya ativo a partir do opt-in de Gabriel.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [w45, producao, intent, baseline, laya]
timestamp: 2026-10-01T14:05:00-03:00
okf_version: "0.1"
---

# W4.5 — produção: day-0 medido + protocolo de 7 dias

Parte do [bundle](/index.md). Gerador: `scripts/w45_production_report.py`
(determinístico, fail-open, journal ausente = `unmeasured`). Dados:
[results.json](/experiments/w45-production/results.json).

## 1. O baseline day-0 (medido em 01/10)

| Métrica | Valor [F] | Leitura |
|---|---|---|
| intent journal (hoje) | **16 linhas** | tráfego real desde a 30.4.88 |
| rotas | keyword 15 · **laya 1** · skipped 1 | a 1 laya é a prova E2E (0,6011) |
| `keyword_fallback_rate` | 0,9375 (day-0, flag off) | braço laya nasce com o opt-in |
| Jev na janela (7d) | **1.178 chamadas api** · 102 cache | p50 366,9 ms · p99 555 ms · conf média 0,6085 |
| recall (3 sessões) | **93 candidatos · 0 injetados** | `curated_share 0,0` **MEDIDO** — o recall computa sem consumidor (a justificativa medida da wave S-5.4) |

## 2. Por que day-0 fecha a subtask honestamente

O gate do plano para S-4.5 é "`w4-intent-cila/report.md` com as métricas W3" —
o artefato com métricas. O que NÃO se pode comprimir é a **janela** (7 dias de
tráfego) e o **braço laya em produção** (opt-in de Gabriel — o plano o declara
"ligar por default: fora do escopo"). Entregue aqui: o **gerador provado** (a
medição é reproduzível por qualquer sessão), o **baseline day-0 medido** (com a
primeira linha `source:laya` já gravada) e o **protocolo de conclusão**:

1. Gabriel liga `TOURING_LAYA_INTENT` em produção quando quiser (a toolchain 30.4.89 já tem o W4.4) + habilita `artifacts/laya-student.service` para o servidor.
2. Em **08/10/2026**: `python3 scripts/w45_production_report.py` → a janela completa com os dois braços (keyword × laya) medidos — fallback rate real, confiança por fonte, distribuição de intent.

## 3. O que o day-0 já prova (não é placeholder)

- **A instrumentação sobrevive à produção**: journals gravados por hooks reais, em sessões reais, sem uma linha perdida [F].
- **O numerador do `keyword_fallback_rate` existe e funciona** (laya 1 / keyword 15, confiança registrada por decisão) [F].
- **O recall sem consumidor é medido, não ausente** (93/0 com `status: measured` — a régua separa "não houve recall" de "recall não ofereceu nada") [F].
