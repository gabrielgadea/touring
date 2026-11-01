---
type: ExperimentReport
title: W44 — temperature scaling para suporte (S-1 do plano r2)
description: O experimento mais barato do bundle respondeu HONESTAMENTE: na régua do plano (precisão ≥ 0,90 na val) não existe T — FITTED=None, config intocada. Mas a curva medida nos dois splits delimita exatamente o que a temperatura compra (suporte 23%→40-46% @ T=0,8 com precisão 0,854) e corrige a expectativa da estratégia: "acc 0,926" era precisão@suportado (T=1,2: 0,9167 reproduz); a acurácia global do aluno é 0,59-0,62. A alavanca estrutural é corpus (W4), não temperatura.
plan_id: 2026-10-01-jev-harness-r2
tags: [w44, temperature, support, calibracao, s1]
timestamp: 2026-10-01T20:35:00-03:00
okf_version: "0.1"
---

# W44 — temperature scaling para suporte (S-1)

Parte do [bundle](/index.md). Gerador: `scripts/w44_support_fit.py` (grade 1-D
determinística T∈[0,4;1,2] passo 0,02; testes em `scripts/test_w44_support_fit.py` — 4 verdes).

## Resultado medido (val 120 itens, congelada, seed 20260922)

| T | suporte@τ0,6 | precisão@suportado | acc global |
|---|---|---|---|
| 0,4 | 0,775 | 0,742 | 0,625 (invariante) |
| 0,6 | 0,608 | 0,822 | ″ |
| 0,8 | **0,458** | **0,855** | ″ |
| 1,0 | 0,358 | 0,861 | ″ |
| 1,09 (atual) | 0,267 | 0,844 | ″ |
| 1,2 | 0,192 | 0,826 | ″ |

Test (diagnóstico, sem escrita — leak nunca): T=0,8 sup 0,40 prec 0,854 · T=1,2 sup 0,20 **prec 0,917 ≈ o "0,926" do W42** · acc 0,592.

## O que o experimento provou

1. **Acurácia global é invariante sob T** (argmax não muda) — a restrição literal do plano
   ("acc ≥ 0,90") é inatingível porque a acurácia global é **0,59–0,62**. O "0,926" da
   estratégia era **precisão@suportado** (reproduzido: 0,917@T=1,2). A expectativa [I 0,75]
   da estratégia morreu na régua — exatamente o que o protótipo existia para medir.
2. **Temperatura compra suporte com preço**: T=0,8 **dobra** o suporte (0,267→0,458) e até
   melhora a precisão vs a atual (0,844→0,855). Abaixo disso a precisão cai rápido.
3. **No piso 0,90: FITTED = None** — `rl_agent_config.json` INTOCADA (a resposta honesta,
   o script sai 1 e reporta a curva; nunca um T de mentira).
4. Instrumento auditado: `item["target"]` é a distribuição soft RLCD; o alvo duro é
   `item["label"]` (a primeira leitura deu acc=0,0 — pego pelo próprio grid).

## Decisão para Gabriel (não bloqueia S-2/S-3)

(a) **Piso 0,90 fica** → config como está; suporte ~23–27% até o retreino (W4 vira a alavanca real).
(b) **Piso 0,85** → T*=0,8: suporte ~40–46%, ~1 em 6 claims errados — a doc classifica
    intent-hint como ação de baixo risco ("harmless/cosmetic"), mas o piso é do plano, não meu.
(c) Meio-termo T=0,7 (interpolação: ~48% suporte, ~0,83 precisão).
