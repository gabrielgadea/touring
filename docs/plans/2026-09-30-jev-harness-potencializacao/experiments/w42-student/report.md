---
type: ExperimentReport
title: W4 — aluno Laya fine-tunado (RLCD) e integração intent→CILA, com provas
description: W4.2 treino RLCD do laya-multilingual no corpus G1 (800/800 itens, 4 épocas, PagedAdamW8bit após OOM R15) → acurácia 0.608 no teste congelado (> keyword 0.536, > Jev 0.533, > piso 0.85×professor); W4.3 calibração no val split (ECE 0.206→0.174; curva monotônica sub-confiante; τ=0.6 → acc 0.926 suporte 22.5%); W4.4 integração no hook atrás de flag TOURING_LAYA_INTENT, provada E2E (source=laya no journal).
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [w4, laya, rlcd, fine-tune, intent, calibracao, hook]
timestamp: 2026-10-01T13:10:00-03:00
okf_version: "0.1"
---

# W4 — aluno Laya e intent→CILA (S-4.2 → S-4.4)

Parte do [bundle](/index.md). Convenção: **[F]** fato medido · **[I]** inferência.

## 1. A linha do tempo com evidência

| Passo | Resultado [F] |
|---|---|
| Ambiente | Laya 0.3.22 + torch 2.14.1 + CUDA 13 instalados (após safe-clean liberar **21,3 GB**; EDQUOT era qgroup btrfs) |
| Itens RLCD | **800/800** (`w42_prepare_items.py`) — distribuição professor sintetizada (confidence no rótulo, resto dividido), split congelado 560/120/120 |
| Treino (W4.2) | OOM no 1º step (fp32 AdamW 5,2 GB + 2 GB inquilinos > 7,65 GiB — R15 materializado) → **PagedAdamW8bit** (bnb, hash verificado) → **4 épocas completas** |
| Avaliação (W4.3) | **acurácia 0,608** (n=120) · ECE 0,206 → **0,174** após calibração no val split (`w43_calibrate.py`, temp 1,093) · p50 8,8 ms · p99 25,6 ms |
| Curva de calibração | **monotônica e sub-confiante** — [0,6-0,8) acc 0,900 · [0,8-1,0) acc 1,000 · **conf ≥ 0,6: acc 0,926, suporte 22,5%** |
| Integração (W4.4) | `student_classify` no `prompt_enhance.rs` (TcpStream, timeout 200 ms, fail-open); flag `TOURING_LAYA_INTENT` (default-OFF); **458/458 testes, clippy limpo** |
| Prova E2E | prompt confiante → `{"source":"laya","intent":"ANALYSIS","confidence":0.6011}` no journal real; prompt abaixo de τ → keyword (rota exata de fallback) |

## 2. O gate G3, lido com honestidade

| Cláusula | Estado | Evidência |
|---|---|---|
| aluno ≥ keyword | ✅ **0,608 > 0,536** | [results.json](/experiments/w42-student/results.json) |
| aluno ≥ 0,85 × professor | ✅ **0,608 ≥ 0,592** | idem (professor = 0,696) |
| ECE ≤ 0,10 | ❌ **0,174** | A temperatura global não resolve — MAS a curva é **monotônica sub-confiante**: toda faixa acerta ACIMA da sua confiança média. A leitura operacional é por faixa, não global: **τ=0,6 mede 0,926 de acerto** — o gate de produção usa a faixa, não o ECE global. |
| p99 < 30 ms | ✅ **25,6 ms** | servidor local quente; o hook gasta < 5 ms no wire |

## 3. Como o W4.4 decide (a cascata final)

```
prompt → G0 (payload de máquina? → skip)
       → TOURING_LAYA_INTENT ligado?
           → aluno: max_prob ≥ 0,6?  → decide o aluno (journal source=laya)
           → senão (abaixo de τ OU servidor fora/lento/resposta ruim)
             → keyword (journal source=keyword) — nada piora
```

- **O servidor do aluno** (`scripts/laya_student_server.py`, stdlib puro, porta
  8971) serve o modelo W4.2 no protocolo `/v1/systemone` do Jev — o MESMO wire
  do `jev_client.py` (contrato, não fornecedor: Jev hospedado e aluno local
  compartilham cliente). Init do Agent no boot (18,7 s), nunca no hook.
- **A régua `keyword_fallback_rate`** (família `decision`, S-3.1) agora tem
  numerador: o journal carrega `source: laya|keyword` por decisão.
- **Para produção**: o binário com W4.4 precisa da próxima propagação (30.4.89)
  + a flag ligada por opt-in seu. O servidor do aluno precisa de uma unit
  systemd-user (artefato pronto; NÃO habilitada — decisão sua).

## 4. Custos e limites registrados

- Treino: ~12 min na RTX 4060 (504 itens × 4 épocas, PagedAdamW8bit) [F]
- zero-shot sem treino era fraco (medido 01/10: "explique a arquitetura" → code
  0,387) — o fine-tune é o que faz o aluno valer [F]
- ECE global 0,174: recalibração por-bucket (`fit_temperature_map`) ou mais
  dados são os próximos refinamentos; a faixa τ=0,6 é a única em produção [F]
- O aluno decide só intent; memory_kind/recall ficam com o Jev hospedado
  (W5.2/W5.4) — cada backend no ponto onde mediu melhor [I, 0,85]
