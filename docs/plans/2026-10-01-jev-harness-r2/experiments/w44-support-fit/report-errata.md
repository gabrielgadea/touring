---
type: Report
title: "W44 — temperature scaling sob o critério corrigido (errata de 02/10/2026)"
description: "Re-execução do S-1 com a condição 2 corrigida para precisão@suportado(τ=0,6) ≥ 0,90. VEREDITO: NENHUM T satisfaz as duas condições. Condição 1 passa em 37 de 41 T; a condição 2 não é atingida por NENHUM T (máx 0,8696 em T=0,94). Config intocada."
plan_id: 2026-10-01-jev-harness-r2
date: 2026-10-02
okf_version: "0.1"
---

# W44 sob o critério corrigido — nenhum T satisfaz as duas condições

**Re-execução da corrida do S-1** sob a errata pré-registada em
`S-1-CITERIO-ACEITACAO.md` (02/10/2026 11:52 BRT, antes desta corrida). Split **val
congelado**, n = **120** amostras `choice`, τ = 0,6, grade 0,40→1,20 passo 0,02,
`current_temperature = 1,0929`. Comando: `python3 scripts/w44_support_fit.py` (1×, 11 s,
CUDA). Artefato bruto: [results-errata.json](/experiments/w44-support-fit/results-errata.json)
— o relatório original de 01/10 foi mantido.

## Veredito das 2 condições

| condição | limiar | resultado |
|---|---|---|
| **1 · suporte a τ=0,6 sobe** | ≥ 0,25 | ✅ **PASSA** em **37 de 41** T. Melhor suporte **0,7750 @ T=0,40**. |
| **2 · precisão@suportado(τ=0,6)** | ≥ 0,90 | ❌ **NENHUM T**. Máximo observado **0,8696 @ T=0,94** — falta 3 pontos. |

→ **"Nenhum T satisfaz; config intocada; W1 não adotado pelos dados."** `fitted: null`,
`rl_agent_config.json` **não foi escrito** (sha256 idêntico antes e depois:
`bfd99413…`), e o script saiu **rc=1** pelo caminho de recusa declarado.

## A curva

| T | suporte | prec@sup | acc global | C1 | C2 |
|---|---|---|---|---|---|
| 0.40 | 0.7750 | 0.7419 | 0.6250 | sim | **não** |
| 0.42 | 0.7750 | 0.7419 | 0.6250 | sim | **não** |
| 0.44 | 0.7417 | 0.7640 | 0.6250 | sim | **não** |
| 0.46 | 0.7250 | 0.7701 | 0.6250 | sim | **não** |
| 0.48 | 0.7250 | 0.7701 | 0.6250 | sim | **não** |
| 0.50 | 0.6917 | 0.7952 | 0.6250 | sim | **não** |
| 0.52 | 0.6750 | 0.7901 | 0.6250 | sim | **não** |
| 0.54 | 0.6667 | 0.7875 | 0.6250 | sim | **não** |
| 0.56 | 0.6583 | 0.7975 | 0.6250 | sim | **não** |
| 0.58 | 0.6417 | 0.8052 | 0.6250 | sim | **não** |
| 0.60 | 0.6083 | 0.8219 | 0.6250 | sim | **não** |
| 0.62 | 0.5917 | 0.8310 | 0.6250 | sim | **não** |
| 0.64 | 0.5750 | 0.8406 | 0.6250 | sim | **não** |
| 0.66 | 0.5583 | 0.8358 | 0.6250 | sim | **não** |
| 0.68 | 0.5500 | 0.8485 | 0.6250 | sim | **não** |
| 0.70 | 0.5333 | 0.8438 | 0.6250 | sim | **não** |
| 0.72 | 0.5250 | 0.8413 | 0.6250 | sim | **não** |
| 0.74 | 0.5167 | 0.8387 | 0.6250 | sim | **não** |
| 0.76 | 0.5083 | 0.8361 | 0.6250 | sim | **não** |
| 0.78 | 0.5000 | 0.8333 | 0.6250 | sim | **não** |
| 0.80 | 0.4583 | 0.8545 | 0.6250 | sim | **não** |
| 0.82 | 0.4500 | 0.8519 | 0.6250 | sim | **não** |
| 0.84 | 0.4417 | 0.8491 | 0.6250 | sim | **não** |
| 0.86 | 0.4333 | 0.8462 | 0.6250 | sim | **não** |
| 0.88 | 0.4000 | 0.8542 | 0.6250 | sim | **não** |
| 0.90 | 0.4000 | 0.8542 | 0.6250 | sim | **não** |
| 0.92 | 0.3917 | 0.8511 | 0.6250 | sim | **não** |
| 0.94 | 0.3833 | 0.8696 | 0.6250 | sim | **não** |
| 0.96 | 0.3833 | 0.8696 | 0.6250 | sim | **não** |
| 0.98 | 0.3667 | 0.8636 | 0.6250 | sim | **não** |
| 1.00 | 0.3583 | 0.8605 | 0.6250 | sim | **não** |
| 1.02 | 0.3417 | 0.8537 | 0.6250 | sim | **não** |
| 1.04 | 0.3417 | 0.8537 | 0.6250 | sim | **não** |
| 1.06 | 0.3000 | 0.8611 | 0.6250 | sim | **não** |
| 1.08 | 0.2833 | 0.8529 | 0.6250 | sim | **não** |
| 1.10 | 0.2667 | 0.8438 | 0.6250 | sim | **não** |
| 1.12 | 0.2583 | 0.8387 | 0.6250 | sim | **não** |
| 1.14 | 0.2333 | 0.8214 | 0.6250 | — | **não** |
| 1.16 | 0.2250 | 0.8148 | 0.6250 | — | **não** |
| 1.18 | 0.2083 | 0.8400 | 0.6250 | — | **não** |
| 1.20 | 0.1917 | 0.8261 | 0.6250 | — | **não** |

## Leitura

1. **A acurácia global é literalmente invariante**: 0,6250 em **todos os 41** T, como o
   critério de 01/10 previa mas media errado. Isso confirma a errata por construção — a
   condição original não podia ser satisfeita nem por T ideal.
2. **As duas grandezas puxam em sentidos opostos**: afiar (T↓) sobe suporte e **derruba**
   precisão@suportado (0,7750/0,7419 em T=0,40); amolecer (T↑) faz o contrário. O suporte
   ganho é comprando com precisão — exatamente a troca que o critério de 01/10 dizia
   proibir, agora com a medida certa.
3. **O teto de 0,90 é o gargalo, não a forma da curva**: o melhor ponto da curva inteira
   (0,8696) fica 0,03 abaixo do piso. Não há T que feche.
4. **O W1 não está perdido, está mal formulado**: o que os dados mostram é que o aluno
   **não tem** uma janela de decisão onde servir mais sem errar mais. As saídas reais são
   (a) retreinar para reduzir a sub-confiança na origem (o logit drift pós-fine-tune),
   (b) baixar o τ de produção com o número de suporte/precisão à vista, ou (c) deixar o
   aluno no estado atual (suporte 0,2667, precisão 0,8438 em T=1,09). **Todas as três são
   decisão do Gabriel** — esta corrida não escreve config.
